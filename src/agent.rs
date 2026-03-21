//! Main agent that handles the conversation with inline rendering

use std::io::{self, Write};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::Result;

use crate::{
    config::Config,
    llm::{LlmClient, Message},
    renderer::DifferentialRenderer,
    terminal::TerminalState,
    tools::{self, Tool, execute_tool, execute_tool_streaming, is_dangerous, StreamEvent},
};

/// Main agent that handles the conversation
pub struct Agent {
    terminal: TerminalState,
    renderer: DifferentialRenderer,
    llm_client: LlmClient,
    history: Vec<Message>,
}

impl Agent {
    /// Create new agent
    pub fn new(config: Config) -> Result<Self> {
        let terminal = TerminalState::new()?;
        let renderer = DifferentialRenderer::new(config.ui.use_colors);
        let llm_client = LlmClient::new(config)?;

        Ok(Self {
            terminal,
            renderer,
            llm_client,
            history: Vec::new(),
        })
    }

    /// Run the agent loop
    pub async fn run_interactive(&mut self) -> Result<()> {
        // Enter raw mode for proper input handling
        self.terminal.enter_raw_mode()?;
        
        // Ensure we restore terminal on exit
        let result = self.main_loop().await;
        
        // Restore terminal
        let _ = self.terminal.exit_raw_mode();
        
        result
    }

    /// Main event loop
    async fn main_loop(&mut self) -> Result<()> {
        loop {
            // Update terminal size
            self.renderer.update_size();

            // Read input using the original inline style
            let input = self.terminal.read_line("");

            let input = match input {
                Ok(i) => i,
                Err(_) => break,
            };

            let input = input.trim();

            // Exit conditions
            if input.is_empty() || input == "exit" || input == "quit" || input == "q" {
                break;
            }

            // Show user input
            self.renderer.add_user_input(input.to_string());

            // Special commands
            if input == "clear" {
                self.history.clear();
                println!("History cleared.");
                continue;
            }

            // Process with agent
            if let Err(e) = self.agent_loop(input).await {
                self.renderer.add_error(e.to_string());
            }
        }

        Ok(())
    }

    /// Main agent loop - handles tool use
    async fn agent_loop(&mut self, query: &str) -> Result<()> {
        // Add user message
        self.history.push(Message::user(query));

        // Max iterations to prevent infinite loops
        let max_iterations = 10;

        for _ in 0..max_iterations {
            // Build messages
            let system = self.system_prompt();
            let mut messages = vec![Message::system(&system)];
            messages.extend(self.history.clone());

            // Start animated spinner
            let spinner_running = Arc::new(AtomicBool::new(true));
            let spinner_running_clone = spinner_running.clone();
            
            // Spawn spinner animation thread
            let spinner_handle = thread::spawn(move || {
                let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let mut idx = 0usize;
                while spinner_running_clone.load(Ordering::Relaxed) {
                    // Print spinner frame: cyan frame, dimmed message
                    print!("\r\x1b[96m{}\x1b[0m \x1b[2mThinking...\x1b[0m  ", frames[idx]);
                    let _ = std::io::stdout().flush();
                    idx = (idx + 1) % frames.len();
                    thread::sleep(Duration::from_millis(80));
                }
                // Clear the spinner line when done
                print!("\r\x1b[2K");
                let _ = std::io::stdout().flush();
            });

            // Get LLM response
            let response = self.llm_client.query(messages).await;

            // Stop spinner
            spinner_running.store(false, Ordering::Relaxed);
            let _ = spinner_handle.join();

            let response = response?;

            // Parse for tools
            let tools = tools::parse_tools(&response.content);

            if tools.is_empty() {
                // No tools - show response and done
                self.history.push(Message::assistant(&response.content));
                self.renderer.add_response(response.content);
                break;
            }

            // Show the response without the tool code blocks
            let display_response = tools::strip_tool_blocks(&response.content);
            if !display_response.is_empty() {
                self.renderer.add_response(display_response);
            }
            self.history.push(Message::assistant(&response.content));

            // Execute tools
            let mut all_results = String::new();

            for tool in &tools {
                match tool {
                    Tool::Bash { command } => {
                        // Check for dangerous commands
                        if is_dangerous(command) {
                            self.renderer.add_error(format!("Refusing dangerous command: {}", command));
                            all_results.push_str(&format!("Command refused (dangerous): {}\n", command));
                            continue;
                        }

                        // Show command header immediately
                        self.renderer.print_bash_header(command);

                        // Start streaming
                        let (rx, _handle) = execute_tool_streaming(command);
                        let start = std::time::Instant::now();
                        let mut output_lines: Vec<String> = Vec::new();
                        let mut exit_code: Option<i32> = None;

                        // Spawn a live timer thread — simple inline timer with bg
                        let timer_alive = Arc::new(AtomicBool::new(true));
                        let timer_alive_clone = timer_alive.clone();
                        let timer_start = start.clone();
                        let use_colors = self.renderer.use_colors();
                        let term_width = self.renderer.width();
                        let timer_thread = thread::spawn(move || {
                            while timer_alive_clone.load(Ordering::Relaxed) {
                                let elapsed = timer_start.elapsed().as_secs_f64();
                                if use_colors {
                                    let width = term_width as usize;
                                    let bg = "\x1b[48;2;30;38;30m";
                                    let reset = "\x1b[0m";
                                    let timing = format!("  Took {:.1}s", elapsed);
                                    let padding = " ".repeat(width.saturating_sub(timing.len()));
                                    // Timer with bg color across full row
                                    print!("\r{}{}{}{}", bg, timing, padding, reset);
                                } else {
                                    print!("\r  Took {:.1}s", elapsed);
                                }
                                let _ = io::stdout().flush();
                                thread::sleep(Duration::from_millis(100));
                            }
                        });

                        loop {
                            match rx.recv_timeout(Duration::from_millis(50)) {
                                Ok(StreamEvent::Stdout(line)) => {
                                    self.renderer.clear_timer_line();
                                    self.renderer.print_output_line(&line);
                                    output_lines.push(line);
                                }
                                Ok(StreamEvent::Stderr(line)) => {
                                    self.renderer.clear_timer_line();
                                    self.renderer.print_output_line(&line);
                                    output_lines.push(line);
                                }
                                Ok(StreamEvent::Done { exit_code: ec }) => {
                                    exit_code = ec;
                                    break;
                                }
                                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            }
                        }

                        // Stop timer thread and wait for it
                        timer_alive.store(false, Ordering::Relaxed);
                        let _ = timer_thread.join();

                        let duration = start.elapsed().as_secs_f64();
                        let success = exit_code == Some(0);

                        // Clear last timer tick, then print gap + timing + bottom pad
                        self.renderer.clear_timer_line();
                        self.renderer.print_bash_footer(duration, success);

                        all_results.push_str(&format!(
                            "$ {}\n{}\n",
                            command,
                            output_lines.join("\n")
                        ));
                    }
                    Tool::ReadFile { path } => {
                        // Show tool call header
                        self.renderer.add_tool_call("read_file".to_string(), path.clone());

                        // Execute
                        let result = execute_tool(tool)?;

                        // Show result
                        self.renderer.add_tool_result(
                            "read_file".to_string(),
                            result.output.clone(),
                            Some(result.duration_secs),
                            result.success,
                            None,
                        );

                        all_results.push_str(&format!("File: {}\n{}\n", path, result.output));
                    }
                }
            }

            // Add results to history for next iteration
            if !all_results.is_empty() {
                self.history.push(Message::user(&format!("Tool output:\n{}", all_results)));
            }
        }

        Ok(())
    }

    /// Build system prompt
    fn system_prompt(&self) -> String {
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        format!(
            r#"You are WAT (Well Assisted Terminal), a command-line assistant.

Tools:
- bash: Execute shell commands. Put commands in ```bash code blocks.
- read_file: Read file contents. Put the file path in a ```read_file code block. Shows line numbers.

Current directory: {}

When asked to do something, use the appropriate tool. Show the tool call you're making."#,
            cwd
        )
    }
}

/// Simple agent for non-interactive queries
pub struct SimpleAgent {
    llm_client: LlmClient,
}

impl SimpleAgent {
    pub fn new(config: Config) -> Result<Self> {
        let llm_client = LlmClient::new(config)?;
        Ok(Self { llm_client })
    }

    pub async fn process_query(&self, query: &str) -> Result<String> {
        let messages = vec![
            Message::system("You are WAT, a terminal assistant. Be concise."),
            Message::user(query),
        ];

        let response = self.llm_client.query(messages).await?;
        Ok(response.content)
    }
}
