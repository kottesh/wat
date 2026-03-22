//! Main agent – conversation loop with inline rendering.

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
    terminal::{ReadResult, TerminalState},
    tools::{self, Tool, execute_tool, execute_tool_streaming, is_dangerous, StreamEvent},
};

pub struct Agent {
    terminal: TerminalState,
    renderer: DifferentialRenderer,
    llm_client: LlmClient,
    history: Vec<Message>,
}

impl Agent {
    pub fn new(config: Config) -> Result<Self> {
        let terminal = TerminalState::new()?;
        let renderer = DifferentialRenderer::new(config.ui.use_colors);
        let llm_client = LlmClient::new(config)?;
        Ok(Self { terminal, renderer, llm_client, history: Vec::new() })
    }

    pub async fn run_interactive(&mut self) -> Result<()> {
        self.terminal.enter_raw_mode()?;
        let result = self.main_loop().await;
        let _ = self.terminal.exit_raw_mode();
        result
    }

    async fn main_loop(&mut self) -> Result<()> {
        // Draw the initial input box — every subsequent render keeps it at the bottom.
        self.renderer.draw_input_box();

        loop {
            self.renderer.update_size();

            match self.terminal.read_line()? {
                ReadResult::Escape => {
                    // ESC while idle: clear any half-typed text, stay in loop
                    self.renderer.clear_input_line();
                    continue;
                }
                ReadResult::Input(raw) => {
                    let input = raw.trim().to_string();

                    if input.is_empty() {
                        continue;
                    }
                    if input == "exit" || input == "quit" || input == "q" {
                        break;
                    }

                    // Commit the typed text as a styled UserInput component.
                    // render_component moves above the input box, prints the
                    // component, then redraws the input box below.
                    self.renderer.add_user_input(input.clone());

                    if input == "clear" {
                        self.history.clear();
                        continue;
                    }

                    if let Err(e) = self.agent_loop(&input).await {
                        self.renderer.add_error(e.to_string());
                    }
                }
            }
        }

        Ok(())
    }

    async fn agent_loop(&mut self, query: &str) -> Result<()> {
        self.history.push(Message::user(query));

        for _ in 0..10 {
            let system = self.system_prompt();
            let mut messages = vec![Message::system(&system)];
            messages.extend(self.history.clone());

            // ── Spinner on the input line while waiting for LLM ──────────
            let spinner_running = Arc::new(AtomicBool::new(true));
            let sr = spinner_running.clone();
            let use_colors = self.renderer.use_colors();

            let spinner_handle = thread::spawn(move || {
                let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let mut idx = 0usize;
                while sr.load(Ordering::Relaxed) {
                    if use_colors {
                        // Write to input line: up 2 → print → clear EOL → down 2
                        print!(
                            "\x1b[2F\r\x1b[96m{}\x1b[0m \x1b[2mThinking...\x1b[0m\x1b[K\x1b[2B\r",
                            frames[idx]
                        );
                    } else {
                        print!("\x1b[2F\r{} Thinking...\x1b[K\x1b[2B\r", frames[idx]);
                    }
                    let _ = io::stdout().flush();
                    idx = (idx + 1) % frames.len();
                    thread::sleep(Duration::from_millis(80));
                }
                // Clear input line
                print!("\x1b[2F\r\x1b[K\x1b[2B\r");
                let _ = io::stdout().flush();
            });

            let response = self.llm_client.query(messages).await;

            spinner_running.store(false, Ordering::Relaxed);
            let _ = spinner_handle.join();

            let response = response?;
            let tools = tools::parse_tools(&response.content);

            if tools.is_empty() {
                self.history.push(Message::assistant(&response.content));
                self.renderer.add_response(response.content);
                break;
            }

            let display_response = tools::strip_tool_blocks(&response.content);
            if !display_response.is_empty() {
                self.renderer.add_response(display_response);
            }
            self.history.push(Message::assistant(&response.content));

            let mut all_results = String::new();

            for tool in &tools {
                match tool {
                    Tool::Bash { command } => {
                        if is_dangerous(command) {
                            self.renderer.add_error(format!("Refusing dangerous command: {}", command));
                            all_results.push_str(&format!("Command refused (dangerous): {}\n", command));
                            continue;
                        }

                        // Show "esc to cancel" hint on the input line
                        if self.renderer.use_colors() {
                            self.renderer.update_input_line(
                                "\x1b[2m  esc to cancel\x1b[0m"
                            );
                        } else {
                            self.renderer.update_input_line("  esc to cancel");
                        }

                        // Print bash header above the input box
                        self.renderer.print_bash_header(command);

                        let (rx, _handle) = execute_tool_streaming(command);
                        let start = std::time::Instant::now();
                        let mut output_lines: Vec<String> = Vec::new();
                        let mut exit_code: Option<i32> = None;
                        let mut cancelled = false;

                        // Live elapsed-time updater on the input line
                        let timer_alive = Arc::new(AtomicBool::new(true));
                        let ta = timer_alive.clone();
                        let timer_start = start;
                        let use_col = self.renderer.use_colors();
                        let timer_thread = thread::spawn(move || {
                            while ta.load(Ordering::Relaxed) {
                                let secs = timer_start.elapsed().as_secs_f64();
                                if use_col {
                                    print!(
                                        "\x1b[2F\r\x1b[2m  esc to cancel  {:.1}s\x1b[0m\x1b[K\x1b[2B\r",
                                        secs
                                    );
                                } else {
                                    print!("\x1b[2F\r  esc to cancel  {:.1}s\x1b[K\x1b[2B\r", secs);
                                }
                                let _ = io::stdout().flush();
                                thread::sleep(Duration::from_millis(100));
                            }
                        });

                        // Streaming loop — also checks stdin for ESC
                        loop {
                            // Non-blocking ESC check
                            if stdin_has_esc() {
                                cancelled = true;
                                break;
                            }

                            match rx.recv_timeout(Duration::from_millis(50)) {
                                Ok(StreamEvent::Stdout(line)) => {
                                    self.renderer.print_output_line(&line);
                                    output_lines.push(line);
                                }
                                Ok(StreamEvent::Stderr(line)) => {
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

                        timer_alive.store(false, Ordering::Relaxed);
                        let _ = timer_thread.join();

                        let duration = start.elapsed().as_secs_f64();
                        let success = !cancelled && exit_code == Some(0);

                        // Clear the hint/timer from the input line
                        self.renderer.clear_input_line();

                        // Repaint the full bash block with the final colour
                        self.renderer.finalize_bash_block(
                            command,
                            &output_lines,
                            duration,
                            success,
                            cancelled,
                        );

                        if cancelled {
                            all_results.push_str(&format!(
                                "$ {}\n{}\n(cancelled)\n",
                                command,
                                output_lines.join("\n")
                            ));
                            break; // stop executing further tools in this turn
                        } else {
                            all_results.push_str(&format!(
                                "$ {}\n{}\n",
                                command,
                                output_lines.join("\n")
                            ));
                        }
                    }
                    Tool::ReadFile { path } => {
                        self.renderer.add_tool_call("read_file".to_string(), path.clone());
                        let result = execute_tool(tool)?;
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

/// Non-blocking check: returns true if stdin has a byte available AND it is ESC (0x1b).
/// If a non-ESC byte is present it is silently consumed (it's from an arrow key, etc.).
fn stdin_has_esc() -> bool {
    use std::os::unix::io::AsRawFd;
    let fd = io::stdin().as_raw_fd();
    let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    let ret = unsafe { libc::poll(&mut pfd as *mut _, 1, 0) };
    if ret > 0 && (pfd.revents & libc::POLLIN) != 0 {
        let mut buf = [0u8; 1];
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, 1) };
        if n == 1 && buf[0] == 0x1b {
            return true;
        }
    }
    false
}
