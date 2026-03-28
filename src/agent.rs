//! Main agent — conversation loop with native tool calling

use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use futures_util::StreamExt;
use serde_json::Value;

use anyhow::Result;

use crate::{
    config::Config,
    llm::{LlmClient, Message, StreamChunk, ToolCall, ToolResult},
    ui::SharedRenderer,
    terminal::{InputEvent, TerminalState},
    tools::{ToolRegistry, ToolUpdate},
};

pub struct Agent {
    terminal: TerminalState,
    renderer: SharedRenderer,
    llm_client: LlmClient,
    history: Vec<Message>,
    registry: ToolRegistry,
}

/// In-progress tool call accumulator
struct InProgressToolCall {
    id: String,
    name: String,
    args_json: String,
}

impl Agent {
    pub fn new(config: Config) -> Result<Self> {
        let terminal = TerminalState::new()?;
        let renderer = Arc::new(Mutex::new(
            crate::ui::UIManager::new(true)
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
        // Clear screen on startup to avoid cargo output mixing with UI
        {
            let mut r = self.renderer.lock().unwrap();
            r.force_redraw();
            r.render();
        }

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

        const MAX_ITERATIONS: usize = 10;
        const MAX_TOOLS_PER_ITERATION: usize = 4;

        for _ in 0..MAX_ITERATIONS {
            let system = self.system_prompt();
            let mut messages = vec![Message::system(&system)];
            messages.extend(self.history.clone());

            // Get tool definitions
            let tools: Vec<_> = self.registry.get_all_definitions()
                .into_iter()
                .map(|t| t.clone())
                .collect();

            // Start background spinner
            let spinner_active = Arc::new(AtomicBool::new(true));
            let spinner_label = Arc::new(Mutex::new("Thinking...".to_string()));
            let spinner_handle = self.spawn_spinner_task(spinner_active.clone(), spinner_label.clone());

            // Stream LLM response
            let mut stream = self.llm_client.stream_default(messages, Some(&tools)).await?;

            {
                let mut r = self.renderer.lock().unwrap();
                r.start_streaming_response();
            }

            // Accumulate response
            let mut text_content = String::new();
            let mut tool_calls_map: std::collections::HashMap<usize, InProgressToolCall> = std::collections::HashMap::new();
            let mut first_chunk = true;

            let aborted = loop {
                tokio::select! {
                    chunk = stream.next() => {
                        match chunk {
                            Some(Ok(StreamChunk::TextDelta(text))) => {
                                if first_chunk {
                                    *spinner_label.lock().unwrap() = "Responding...".to_string();
                                    first_chunk = false;
                                }
                                text_content.push_str(&text);
                                let mut r = self.renderer.lock().unwrap();
                                r.push_response_chunk(&text);
                                r.render();
                            }
                            Some(Ok(StreamChunk::ToolCallStart { id, name, index })) => {
                                tool_calls_map.insert(index, InProgressToolCall {
                                    id,
                                    name,
                                    args_json: String::new(),
                                });
                            }
                            Some(Ok(StreamChunk::ToolCallArgsDelta { index, args_json_delta, .. })) => {
                                if let Some(tc) = tool_calls_map.get_mut(&index) {
                                    tc.args_json.push_str(&args_json_delta);
                                }
                            }
                            Some(Ok(StreamChunk::ToolCallComplete { .. })) => {
                                // Tool call complete - will process after stream ends
                            }
                            Some(Ok(StreamChunk::Done { .. })) => break false,
                            Some(Err(e)) => {
                                spinner_active.store(false, Ordering::Relaxed);
                                let _ = spinner_handle.await;
                                return Err(e);
                            }
                            None => break false,
                        }
                    }
                    Some(event) = input_rx.recv() => {
                        match event {
                            InputEvent::Cancel => break true,
                            InputEvent::Shutdown => {
                                spinner_active.store(false, Ordering::Relaxed);
                                return Err(anyhow::anyhow!("Interrupted"));
                            }
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

            if aborted {
                return Ok(());
            }

            // Convert accumulated tool calls
            let mut tool_calls: Vec<ToolCall> = tool_calls_map
                .into_iter()
                .filter_map(|(_, tc)| {
                    serde_json::from_str::<Value>(&tc.args_json)
                        .ok()
                        .map(|args| ToolCall {
                            id: tc.id,
                            name: tc.name,
                            arguments: args,
                        })
                })
                .collect();

            // Limit to 4 tools per iteration
            tool_calls.truncate(MAX_TOOLS_PER_ITERATION);

            // No tools to call - we're done
            if tool_calls.is_empty() {
                self.history.push(Message::assistant(&text_content));
                break;
            }

            // Add assistant message with tool calls
            self.history.push(Message::assistant_with_tools(
                if text_content.is_empty() { None } else { Some(text_content) },
                tool_calls.clone(),
            ));

            // Execute tools
            for call in tool_calls {
                let result = self.execute_tool(&call, input_rx).await?;
                self.history.push(Message::tool_result(result));
            }
        }

        Ok(())
    }

    async fn execute_tool(
        &mut self,
        call: &ToolCall,
        input_rx: &mut tokio::sync::mpsc::Receiver<InputEvent>,
    ) -> Result<ToolResult> {
        let tool_def = self.registry
            .get(&call.name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", call.name))?;

        // Setup UI
        {
            let mut r = self.renderer.lock().unwrap();
            if call.name == "bash" {
                r.start_bash(call.arguments["command"].as_str().unwrap_or(""));
            } else {
                r.add_tool_call(call.name.clone(), format!("{}", call.arguments));
            }
            r.render();
        }

        // Setup progress callback
        let renderer = self.renderer.clone();
        let tool_name = call.name.clone();
        let on_update = Box::new(move |update: ToolUpdate| {
            let mut r = renderer.lock().unwrap();
            match update {
                ToolUpdate::Stdout(line) if tool_name == "bash" => {
                    r.push_bash_output(line);
                    r.render();
                }
                ToolUpdate::Stderr(line) if tool_name == "bash" => {
                    r.push_bash_output(line);
                    r.render();
                }
                _ => {}
            }
        });

        // Execute with timeout
        let timeout_duration = tool_def.executor.timeout().unwrap_or(Duration::from_secs(30));
        
        let active = Arc::new(AtomicBool::new(true));
        let start = std::time::Instant::now();
        let timer_handle = self.spawn_bash_timer_task(active.clone(), start);

        let mut exec_future = Box::pin(tool_def.executor.execute(call.arguments.clone(), on_update));
        let mut cancelled = false;

        let exec_result = loop {
            tokio::select! {
                result = &mut exec_future => {
                    break Some(result);
                }
                _ = tokio::time::sleep(timeout_duration) => {
                    // Timeout
                    break None;
                }
                Some(event) = input_rx.recv() => {
                    match event {
                        InputEvent::Cancel => {
                            cancelled = true;
                            break None;
                        }
                        InputEvent::Shutdown => {
                            active.store(false, Ordering::Relaxed);
                            return Err(anyhow::anyhow!("Interrupted"));
                        }
                        _ => {}
                    }
                }
            }
        };

        active.store(false, Ordering::Relaxed);
        let _ = timer_handle.await;

        let execution_result = match exec_result {
            Some(Ok(result)) => result,
            Some(Err(e)) => {
                crate::tools::ExecutionResult {
                    content: format!("Tool execution failed: {}", e),
                    success: false,
                    error: Some(e.to_string()),
                    duration: start.elapsed(),
                }
            }
            None => {
                let reason = if cancelled { "Cancelled by user" } else { "Timeout" };
                crate::tools::ExecutionResult {
                    content: reason.to_string(),
                    success: false,
                    error: Some(reason.to_string()),
                    duration: start.elapsed(),
                }
            }
        };

        // Update UI
        {
            let mut r = self.renderer.lock().unwrap();
            r.clear_input_hint();
            if call.name == "bash" {
                r.finalize_bash(
                    execution_result.duration.as_secs_f64(),
                    execution_result.success,
                    cancelled,
                );
            } else {
                r.add_tool_result(
                    call.name.clone(),
                    execution_result.content.clone(),
                    Some(execution_result.duration.as_secs_f64()),
                    execution_result.success,
                    None,
                );
            }
            r.render();
        }

        Ok(ToolResult {
            tool_call_id: call.id.clone(),
            content: execution_result.content,
            success: execution_result.success,
            error: execution_result.error,
        })
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
        for tool in self.registry.get_all_definitions() {
            tools_desc.push_str(&format!("- {}: {}\n", tool.name, tool.description));
        }

        format!(
            r#"You are WAT, a terminal assistant.
OS: {} | CWD: {}

Available Tools:
{}
You can use tools by calling them. The system will execute them and provide results.
Be concise and helpful."#,
            os, cwd, tools_desc
        )
    }
}
