//! Main agent — conversation loop with decoupled background rendering.

use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use futures_util::StreamExt;

use anyhow::Result;

use crate::{
    config::Config,
    llm::{LlmClient, Message},
    renderer::SharedRenderer,
    terminal::{InputEvent, TerminalState},
    tools::{self, ToolRegistry, ToolUpdate, ToolCall, StreamEvent, is_dangerous},
};

pub struct Agent {
    terminal: TerminalState,
    renderer: SharedRenderer,
    llm_client: LlmClient,
    history: Vec<Message>,
    registry: ToolRegistry,
}

impl Agent {
    pub fn new(config: Config) -> Result<Self> {
        let terminal = TerminalState::new()?;
        let renderer = Arc::new(Mutex::new(
            crate::renderer::DifferentialRenderer::new(config.ui.use_colors)
        ));
        let llm_client = LlmClient::new(config)?;
        Ok(Self { 
            terminal, 
            renderer, 
            llm_client, 
            history: Vec::new(),
            registry: ToolRegistry::new(),
        })
    }

    pub async fn run_interactive(&mut self) -> Result<()> {
        self.terminal.enter_raw_mode()?;
        let mut input_rx = self.terminal.spawn_input_handler(self.renderer.clone());
        let result = self.main_loop(&mut input_rx).await;
        let _ = self.terminal.exit_raw_mode();
        result
    }

    async fn main_loop(&mut self, input_rx: &mut tokio::sync::mpsc::Receiver<InputEvent>) -> Result<()> {
        self.renderer.lock().unwrap().render();

        loop {
            tokio::select! {
                Some(event) = input_rx.recv() => {
                    match event {
                        InputEvent::Shutdown => return Ok(()),
                        InputEvent::Cancel => { self.renderer.lock().unwrap().render(); }
                        InputEvent::Submit(raw) => {
                            let input = raw.trim().to_string();
                            if input.is_empty() { continue; }
                            if input == "exit" || input == "quit" || input == "q" { return Ok(()); }

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

            // ── Start background spinner ─────────────────────────────────────
            let spinner_active = Arc::new(AtomicBool::new(true));
            let spinner_label = Arc::new(Mutex::new("Thinking...".to_string()));
            let spinner_handle = self.spawn_spinner_task(spinner_active.clone(), spinner_label.clone());

            // ── LLM query with streaming ─────────────────────────────────────
            let mut stream = self.llm_client.query_stream(messages).await?;
            let mut full_content = String::new();
            
            {
                let mut r = self.renderer.lock().unwrap();
                r.start_streaming_response();
            }

            let mut first_chunk = true;
            let aborted = loop {
                tokio::select! {
                    chunk = stream.next() => {
                        match chunk {
                            Some(Ok(text)) => {
                                if first_chunk {
                                    *spinner_label.lock().unwrap() = "Responding...".to_string();
                                    first_chunk = false;
                                }
                                full_content.push_str(&text);
                                let mut r = self.renderer.lock().unwrap();
                                r.push_response_chunk(&text);
                                r.render();
                            }
                            None => break false, 
                            Some(Err(e)) => {
                                spinner_active.store(false, Ordering::Relaxed);
                                let _ = spinner_handle.await;
                                return Err(e);
                            }
                        }
                    }
                    Some(event) = input_rx.recv() => {
                        match event {
                            InputEvent::Shutdown => {
                                spinner_active.store(false, Ordering::Relaxed);
                                return Err(anyhow::anyhow!("Interrupted"));
                            }
                            InputEvent::Cancel => break true,
                            _ => {}
                        }
                    }
                }
            };

            spinner_active.store(false, Ordering::Relaxed);
            let _ = spinner_handle.await;

            {
                let mut r = self.renderer.lock().unwrap();
                r.clear_spinner();
                r.finalize_response();
                r.render();
            }

            if aborted { return Ok(()); }

            let tools = tools::parse_tools(&full_content, &self.registry);
            if tools.is_empty() {
                self.history.push(Message::assistant(&full_content));
                break;
            }

            self.history.push(Message::assistant(&full_content));
            let mut all_results = String::new();

            for tool_call in &tools {
                if let Some(tool) = self.registry.get(&tool_call.name) {
                    // Start generic tool timer/spinner
                    let active = Arc::new(AtomicBool::new(true));
                    let start = std::time::Instant::now();
                    let spinner = self.spawn_bash_timer_task(active.clone(), start);

                    {
                        let mut r = self.renderer.lock().unwrap();
                        if tool_call.name == "bash" {
                            r.start_bash(tool_call.args["command"].as_str().unwrap_or(""));
                        } else {
                            r.add_tool_call(tool_call.name.clone(), tool_call.args.to_string());
                        }
                        r.render();
                    }

                    let renderer = self.renderer.clone();
                    let tool_name = tool_call.name.clone();
                    
                    let on_update = Box::new(move |update| {
                        let mut r = renderer.lock().unwrap();
                        match update {
                            ToolUpdate::Stdout(line) | ToolUpdate::Stderr(line) => {
                                if tool_name == "bash" {
                                    r.push_bash_output(line);
                                }
                            }
                            ToolUpdate::Status(_status) => {}
                        }
                        r.render();
                    });

                    let result = tool.execute(tool_call.args.clone(), on_update).await?;

                    active.store(false, Ordering::Relaxed);
                    let _ = spinner.await;

                    {
                        let mut r = self.renderer.lock().unwrap();
                        r.clear_input_hint();
                        if tool_call.name == "bash" {
                            r.finalize_bash(result.duration_secs, result.success, false);
                        } else {
                            r.add_tool_result(
                                tool_call.name.clone(),
                                result.content.clone(),
                                Some(result.duration_secs),
                                result.success,
                                None,
                            );
                        }
                        r.render();
                    }

                    all_results.push_str(&format!("Tool: {}\n{}\n", tool_call.name, result.content));
                }
            }

            if !all_results.is_empty() {
                self.history.push(Message::user(&format!("Tool output:\n{}", all_results)));
            }
        }

        Ok(())
    }

    fn spawn_spinner_task(&self, active: Arc<AtomicBool>, label: Arc<Mutex<String>>) -> tokio::task::JoinHandle<()> {
        let renderer = self.renderer.clone();
        tokio::spawn(async move {
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut idx = 0usize;
            while active.load(Ordering::Relaxed) {
                {
                    let mut r = renderer.lock().unwrap();
                    if !active.load(Ordering::Relaxed) { break; }
                    let frame = frames[idx];
                    let current_label = label.lock().unwrap().clone();
                    let text = if r.use_colors() {
                        format!("  \x1b[96m{}\x1b[0m \x1b[2m{}\x1b[0m", frame, current_label)
                    } else {
                        format!("  {} {}", frame, current_label)
                    };
                    r.set_spinner(text);
                    r.render();
                }
                idx = (idx + 1) % frames.len();
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
    }

    fn spawn_bash_timer_task(&self, active: Arc<AtomicBool>, start: std::time::Instant) -> tokio::task::JoinHandle<()> {
        let renderer = self.renderer.clone();
        tokio::spawn(async move {
            while active.load(Ordering::Relaxed) {
                {
                    let mut r = renderer.lock().unwrap();
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
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
    }

    fn system_prompt(&self) -> String {
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let os = whoami::distro();

        let mut tools_desc = String::new();
        for tool in self.registry.list() {
            tools_desc.push_str(&format!("- ```{}\\nargs```: {}\n", tool.name(), tool.description()));
        }

        format!(
            r#"You are WAT, a terminal assistant. 

OS: {} | CWD: {}

Tools:
{}"#,
            os, cwd, tools_desc
        )
    }
}
