//! Main agent — conversation loop with asynchronous input handling.

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;

use crate::{
    config::Config,
    llm::{LlmClient, Message},
    renderer::SharedRenderer,
    terminal::{InputEvent, TerminalState},
    tools::{self, Tool, execute_tool, execute_tool_streaming, is_dangerous, StreamEvent},
};

pub struct Agent {
    terminal: TerminalState,
    renderer: SharedRenderer,
    llm_client: LlmClient,
    history: Vec<Message>,
}

impl Agent {
    pub fn new(config: Config) -> Result<Self> {
        let terminal = TerminalState::new()?;
        let renderer = Arc::new(Mutex::new(
            crate::renderer::DifferentialRenderer::new(config.ui.use_colors)
        ));
        let llm_client = LlmClient::new(config)?;
        Ok(Self { terminal, renderer, llm_client, history: Vec::new() })
    }

    pub async fn run_interactive(&mut self) -> Result<()> {
        self.terminal.enter_raw_mode()?;
        
        // Spawn the dedicated background input thread
        let mut input_rx = self.terminal.spawn_input_handler(self.renderer.clone());
        
        let result = self.main_loop(&mut input_rx).await;
        
        let _ = self.terminal.exit_raw_mode();
        result
    }

    async fn main_loop(&mut self, input_rx: &mut tokio::sync::mpsc::Receiver<InputEvent>) -> Result<()> {
        // Initial draw
        self.renderer.lock().unwrap().render();

        loop {
            tokio::select! {
                Some(event) = input_rx.recv() => {
                    match event {
                        InputEvent::Shutdown => return Ok(()),
                        InputEvent::Cancel => {
                             // ESC while idle: just refresh
                             self.renderer.lock().unwrap().render();
                        }
                        InputEvent::Submit(raw) => {
                            let input = raw.trim().to_string();
                            if input.is_empty() { continue; }
                            if input == "exit" || input == "quit" || input == "q" {
                                return Ok(());
                            }

                            // Commit typed line to history view
                            {
                                let mut r = self.renderer.lock().unwrap();
                                r.add_user_input(input.clone());
                                r.render();
                            }

                            if input == "clear" {
                                self.history.clear();
                                continue;
                            }

                            if let Err(e) = self.agent_loop(&input, input_rx).await {
                                let mut r = self.renderer.lock().unwrap();
                                r.add_error(e.to_string());
                                r.render();
                            }
                        }
                    }
                }
            }
        }
    }

    async fn agent_loop(
        &mut self,
        query: &str,
        input_rx: &mut tokio::sync::mpsc::Receiver<InputEvent>,
    ) -> Result<()> {
        self.history.push(Message::user(query));

        for _ in 0..10 {
            let system = self.system_prompt();
            let mut messages = vec![Message::system(&system)];
            messages.extend(self.history.clone());

            // ── LLM query with concurrent animated spinner ───────────────────
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut idx = 0usize;

            let query_fut = self.llm_client.query(messages);
            tokio::pin!(query_fut);

            let response = loop {
                tokio::select! {
                    r = &mut query_fut => break r,
                    // Handle shutdown/cancel while waiting for LLM
                    Some(event) = input_rx.recv() => {
                        match event {
                            InputEvent::Shutdown => return Err(anyhow::anyhow!("Interrupted")),
                            InputEvent::Cancel => {
                                // Abort the agent turn
                                return Ok(());
                            }
                            _ => {} // Typing is handled by background thread
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(80)) => {
                        let mut r = self.renderer.lock().unwrap();
                        let frame = frames[idx];
                        let text = if r.use_colors() {
                            format!("  \x1b[96m{}\x1b[0m \x1b[2mThinking...\x1b[0m", frame)
                        } else {
                            format!("  {} Thinking...", frame)
                        };
                        r.set_spinner(text);
                        r.render();
                        idx = (idx + 1) % frames.len();
                    }
                }
            };

            self.renderer.lock().unwrap().clear_spinner();
            let response = response?;
            let tools = tools::parse_tools(&response.content);

            if tools.is_empty() {
                self.history.push(Message::assistant(&response.content));
                let mut r = self.renderer.lock().unwrap();
                r.add_response(response.content);
                r.render();
                break;
            }

            let display = tools::strip_tool_blocks(&response.content);
            if !display.is_empty() {
                let mut r = self.renderer.lock().unwrap();
                r.add_response(display);
                r.render();
            }
            self.history.push(Message::assistant(&response.content));

            let mut all_results = String::new();

            for tool in &tools {
                match tool {
                    Tool::Bash { command } => {
                        if is_dangerous(command) {
                            let mut r = self.renderer.lock().unwrap();
                            r.add_error(format!("Refusing dangerous command: {}", command));
                            r.render();
                            all_results.push_str(&format!("Command refused: {}\n", command));
                            continue;
                        }

                        // Start bash block
                        {
                            let mut r = self.renderer.lock().unwrap();
                            let hint = if r.use_colors() {
                                "  \x1b[2mesc to cancel\x1b[0m".to_string()
                            } else {
                                "  esc to cancel".to_string()
                            };
                            r.set_input_hint(hint);
                            r.start_bash(command);
                            r.render();
                        }

                        let (rx, _handle) = execute_tool_streaming(command);
                        let start = std::time::Instant::now();
                        let mut output_lines: Vec<String> = Vec::new();
                        let mut exit_code: Option<i32> = None;
                        let mut cancelled = false;

                        // Unified streaming + input loop
                        loop {
                            tokio::select! {
                                Some(event) = input_rx.recv() => {
                                    match event {
                                        InputEvent::Cancel => {
                                            cancelled = true;
                                            break;
                                        }
                                        InputEvent::Shutdown => return Err(anyhow::anyhow!("Interrupted")),
                                        _ => {}
                                    }
                                }
                                stream_event = async {
                                    // Use a non-blocking try_recv in a tight loop with small sleep
                                    // or just wait on the mpsc channel if we refactor tools.rs.
                                    // For now, we bridge the std::sync::mpsc with a sleep.
                                    loop {
                                        match rx.try_recv() {
                                            Ok(ev) => return Some(ev),
                                            Err(std::sync::mpsc::TryRecvError::Empty) => {
                                                tokio::time::sleep(Duration::from_millis(10)).await;
                                                // We must return None occasionally to let select!
                                                // check the input_rx and timer.
                                                return None;
                                            }
                                            Err(std::sync::mpsc::TryRecvError::Disconnected) => return None,
                                        }
                                    }
                                } => {
                                    if let Some(event) = stream_event {
                                        match event {
                                            StreamEvent::Stdout(line) | StreamEvent::Stderr(line) => {
                                                let mut r = self.renderer.lock().unwrap();
                                                r.push_bash_output(line.clone());
                                                r.render();
                                                output_lines.push(line);
                                            }
                                            StreamEvent::Done { exit_code: ec } => {
                                                exit_code = ec;
                                                break;
                                            }
                                        }
                                    }
                                    // Periodic timer update
                                    let mut r = self.renderer.lock().unwrap();
                                    let secs = start.elapsed().as_secs_f64();
                                    let hint = if r.use_colors() {
                                        format!("  \x1b[2mesc to cancel  {:.1}s\x1b[0m", secs)
                                    } else {
                                        format!("  esc to cancel  {:.1}s", secs)
                                    };
                                    r.set_input_hint(hint);
                                    r.set_bash_elapsed(secs);
                                    r.render();
                                }
                            }
                        }

                        let duration = start.elapsed().as_secs_f64();
                        let success = !cancelled && exit_code == Some(0);

                        {
                            let mut r = self.renderer.lock().unwrap();
                            r.clear_input_hint();
                            r.finalize_bash(duration, success, cancelled);
                            r.render();
                        }

                        let output_text = self.renderer.lock().unwrap().last_bash_output();
                        if cancelled {
                            all_results.push_str(&format!("$ {}\n{}\n(cancelled)\n", command, output_text));
                            // Add results to history so far before aborting
                            if !all_results.is_empty() {
                                self.history.push(Message::user(&format!("Tool output:\n{}", all_results)));
                            }
                            return Ok(()); // Abort the whole agent turn
                        } else {
                            all_results.push_str(&format!("$ {}\n{}\n", command, output_text));
                        }
                    }
                    Tool::ReadFile { path } => {
                        {
                            let mut r = self.renderer.lock().unwrap();
                            r.add_tool_call("read_file".to_string(), path.clone());
                            r.render();
                        }
                        let result = execute_tool(tool)?;
                        {
                            let mut r = self.renderer.lock().unwrap();
                            r.add_tool_result(
                                "read_file".to_string(),
                                result.output.clone(),
                                Some(result.duration_secs),
                                result.success,
                                None,
                            );
                            r.render();
                        }
                        all_results.push_str(&format!("File: {}\n{}\n", path, result.output));
                    }
                }
            }

            if !all_results.is_empty() {
                self.history.push(Message::user(&format!("Tool output:\n{}", all_results)));
            }
        }

        Ok(())
    }

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
