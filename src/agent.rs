//! Main agent — conversation loop with differential inline rendering.

use std::io;
use std::sync::mpsc;
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
        // First render draws the empty input box
        self.renderer.render();

        loop {
            match self.terminal.read_line(&mut self.renderer)? {
                ReadResult::Escape => {
                    // ESC while idle — clear any partial input, stay in loop
                    self.renderer.render();
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

                    // Commit the typed line as a styled UserInput component
                    self.renderer.add_user_input(input.clone());
                    self.renderer.render();

                    if input == "clear" {
                        self.history.clear();
                        continue;
                    }

                    if let Err(e) = self.agent_loop(&input).await {
                        self.renderer.add_error(e.to_string());
                        self.renderer.render();
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

            // ── LLM query with animated spinner on the input line ────────────
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut idx = 0usize;

            let query_fut = self.llm_client.query(messages);
            tokio::pin!(query_fut);

            let response = loop {
                tokio::select! {
                    r = &mut query_fut => break r,
                    _ = tokio::time::sleep(Duration::from_millis(80)) => {
                        let frame = frames[idx];
                        let text = if self.renderer.use_colors() {
                            format!("  \x1b[96m{}\x1b[0m \x1b[2mThinking...\x1b[0m", frame)
                        } else {
                            format!("  {} Thinking...", frame)
                        };
                        self.renderer.set_spinner(text);
                        self.renderer.render();
                        idx = (idx + 1) % frames.len();
                    }
                }
            };

            self.renderer.clear_spinner();
            let response = response?;
            let tools = tools::parse_tools(&response.content);

            if tools.is_empty() {
                self.history.push(Message::assistant(&response.content));
                self.renderer.add_response(response.content);
                self.renderer.render();
                break;
            }

            let display = tools::strip_tool_blocks(&response.content);
            if !display.is_empty() {
                self.renderer.add_response(display);
                self.renderer.render();
            }
            self.history.push(Message::assistant(&response.content));

            let mut all_results = String::new();

            for tool in &tools {
                match tool {
                    Tool::Bash { command } => {
                        if is_dangerous(command) {
                            self.renderer.add_error(format!(
                                "Refusing dangerous command: {}",
                                command
                            ));
                            self.renderer.render();
                            all_results
                                .push_str(&format!("Command refused: {}\n", command));
                            continue;
                        }

                        // Show hint on the input line
                        let hint = if self.renderer.use_colors() {
                            "  \x1b[2mesc to cancel\x1b[0m".to_string()
                        } else {
                            "  esc to cancel".to_string()
                        };
                        self.renderer.set_input_hint(hint);

                        // Start bash block and do initial render
                        self.renderer.start_bash(command);
                        self.renderer.render();

                        let (rx, _handle) = execute_tool_streaming(command);
                        let start = std::time::Instant::now();
                        let mut exit_code: Option<i32> = None;
                        let mut cancelled = false;

                        loop {
                            // Non-blocking ESC check
                            if stdin_has_esc() {
                                cancelled = true;
                                break;
                            }

                            match rx.recv_timeout(Duration::from_millis(50)) {
                                Ok(StreamEvent::Stdout(line)) => {
                                    self.renderer.push_bash_output(line);
                                    self.renderer.render();
                                }
                                Ok(StreamEvent::Stderr(line)) => {
                                    self.renderer.push_bash_output(line);
                                    self.renderer.render();
                                }
                                Ok(StreamEvent::Done { exit_code: ec }) => {
                                    exit_code = ec;
                                    break;
                                }
                                Err(mpsc::RecvTimeoutError::Timeout) => {
                                    // Update elapsed timer on input line
                                    let secs = start.elapsed().as_secs_f64();
                                    let hint = if self.renderer.use_colors() {
                                        format!(
                                            "  \x1b[2mesc to cancel  {:.1}s\x1b[0m",
                                            secs
                                        )
                                    } else {
                                        format!("  esc to cancel  {:.1}s", secs)
                                    };
                                    self.renderer.set_input_hint(hint);
                                    self.renderer.set_bash_elapsed(secs);
                                    self.renderer.render();
                                }
                                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            }
                        }

                        let duration = start.elapsed().as_secs_f64();
                        let success = !cancelled && exit_code == Some(0);

                        self.renderer.clear_input_hint();
                        self.renderer.finalize_bash(duration, success, cancelled);
                        self.renderer.render();

                        let output_text = self.renderer.last_bash_output();

                        if cancelled {
                            all_results.push_str(&format!(
                                "$ {}\n{}\n(cancelled)\n",
                                command, output_text
                            ));
                            break;
                        } else {
                            all_results
                                .push_str(&format!("$ {}\n{}\n", command, output_text));
                        }
                    }
                    Tool::ReadFile { path } => {
                        self.renderer.add_tool_call("read_file".to_string(), path.clone());
                        self.renderer.render();
                        let result = execute_tool(tool)?;
                        self.renderer.add_tool_result(
                            "read_file".to_string(),
                            result.output.clone(),
                            Some(result.duration_secs),
                            result.success,
                            None,
                        );
                        self.renderer.render();
                        all_results
                            .push_str(&format!("File: {}\n{}\n", path, result.output));
                    }
                }
            }

            if !all_results.is_empty() {
                self.history
                    .push(Message::user(&format!("Tool output:\n{}", all_results)));
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

/// Non-blocking check: is there an ESC byte (0x1b) waiting on stdin?
fn stdin_has_esc() -> bool {
    use std::os::unix::io::AsRawFd;
    let fd = io::stdin().as_raw_fd();
    let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    let ret = unsafe { libc::poll(&mut pfd as *mut _, 1, 0) };
    if ret > 0 && pfd.revents & libc::POLLIN != 0 {
        let mut b = [0u8; 1];
        let n = unsafe { libc::read(fd, b.as_mut_ptr() as *mut libc::c_void, 1) };
        return n == 1 && b[0] == 0x1b;
    }
    false
}
