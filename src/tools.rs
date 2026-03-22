use std::process::Command;
use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

/// Result of executing a tool
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: String,
    pub details: Value,
    pub success: bool,
    pub duration_secs: f64,
}

/// A progress update from a tool
#[derive(Debug, Clone)]
pub enum ToolUpdate {
    Stdout(String),
    Stderr(String),
    Status(String),
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn aliases(&self) -> Vec<String> { Vec::new() }
    fn description(&self) -> &str;
    fn schema(&self) -> Value;
    fn primary_arg_name(&self) -> &str;
    
    /// Returns a human-readable string for the tool call (for the UI)
    fn display_call(&self, args: &Value) -> String {
        format!("{} {}", self.name(), args[self.primary_arg_name()].as_str().unwrap_or(""))
    }
    
    async fn execute(
        &self, 
        args: Value, 
        on_update: Box<dyn Fn(ToolUpdate) + Send + Sync>
    ) -> Result<ToolResult>;
}

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str { "bash" }
    fn aliases(&self) -> Vec<String> { vec!["sh".to_string(), "shell".to_string()] }
    fn description(&self) -> &str { "Execute shell commands" }
    fn primary_arg_name(&self) -> &str { "command" }
    fn display_call(&self, args: &Value) -> String {
        format!("$ {}", args["command"].as_str().unwrap_or(""))
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The command to execute" }
            },
            "required": ["command"]
        })
    }
    
    async fn execute(
        &self, 
        args: Value, 
        on_update: Box<dyn Fn(ToolUpdate) + Send + Sync>
    ) -> Result<ToolResult> {
        let command = args["command"].as_str().ok_or_else(|| anyhow::anyhow!("Missing command"))?;
        let start = std::time::Instant::now();
        
        let (rx, _handle) = execute_tool_streaming(command);
        let mut output = String::new();
        let mut success = false;
        
        while let Ok(event) = rx.recv() {
            match event {
                StreamEvent::Stdout(line) => {
                    output.push_str(&line);
                    output.push('\n');
                    on_update(ToolUpdate::Stdout(line));
                }
                StreamEvent::Stderr(line) => {
                    output.push_str(&line);
                    output.push('\n');
                    on_update(ToolUpdate::Stderr(line));
                }
                StreamEvent::Done { exit_code } => {
                    success = exit_code == Some(0);
                    break;
                }
            }
        }
        
        Ok(ToolResult {
            content: truncate_output(&output, 100),
            details: serde_json::json!({ "output": output, "success": success }),
            success,
            duration_secs: start.elapsed().as_secs_f64(),
        })
    }
}

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    fn aliases(&self) -> Vec<String> { vec!["file".to_string()] }
    fn description(&self) -> &str { "Read file contents with line numbers" }
    fn primary_arg_name(&self) -> &str { "path" }
    fn display_call(&self, args: &Value) -> String {
        args["path"].as_str().unwrap_or("").to_string()
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The path to the file" }
            },
            "required": ["path"]
        })
    }
    
    async fn execute(
        &self, 
        args: Value, 
        _on_update: Box<dyn Fn(ToolUpdate) + Send + Sync>
    ) -> Result<ToolResult> {
        let path_str = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("Missing path"))?;
        let expanded_path = shellexpand::tilde(path_str).to_string();
        let path = Path::new(&expanded_path);
        let start = std::time::Instant::now();
        
        if !path.exists() {
            return Ok(ToolResult {
                content: format!("File not found: {}", path_str),
                details: serde_json::json!({ "error": "File not found", "path": path_str }),
                success: false,
                duration_secs: start.elapsed().as_secs_f64(),
            });
        }
        
        let content = fs::read_to_string(path)?;
        let total_lines = content.lines().count();
        let max_lines = 200;
        
        let display = if total_lines > max_lines {
            let truncated: Vec<&str> = content.lines().take(max_lines).collect();
            format!(
                "{}\n... ({} more lines)",
                add_line_numbers(&truncated.join("\n")),
                total_lines - max_lines
            )
        } else {
            add_line_numbers(&content)
        };
        
        Ok(ToolResult {
            content: display.clone(),
            details: serde_json::json!({ "content": content, "lines": total_lines }),
            success: true,
            duration_secs: start.elapsed().as_secs_f64(),
        })
    }
}

pub struct ToolRegistry {
    tools: std::collections::HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut tools = std::collections::HashMap::new();
        tools.insert("bash".to_string(), Box::new(BashTool) as Box<dyn Tool>);
        tools.insert("read_file".to_string(), Box::new(ReadFileTool) as Box<dyn Tool>);
        Self { tools }
    }
    
    pub fn get(&self, name: &str) -> Option<&Box<dyn Tool>> {
        self.tools.get(name)
    }
    
    pub fn list(&self) -> Vec<&Box<dyn Tool>> {
        self.tools.values().collect()
    }
}

/// A line emitted from a streaming command
#[derive(Debug)]
pub enum StreamEvent {
    /// A line of stdout
    Stdout(String),
    /// A line of stderr
    Stderr(String),
    /// Command finished with exit code
    Done { exit_code: Option<i32> },
}

/// Add line numbers to file content
fn add_line_numbers(content: &str) -> String {
    content
        .lines()
        .enumerate()
        .map(|(i, line)| format!("{:>6}  {}", i + 1, line))
        .collect::<Vec<String>>()
        .join("\n")
}

/// Truncate output to max lines
fn truncate_output(output: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    
    if lines.len() > max_lines {
        let truncated: Vec<&str> = lines[..max_lines].to_vec();
        format!("{}\n... ({} more lines)", truncated.join("\n"), lines.len() - max_lines)
    } else {
        output.to_string()
    }
}

/// Execute a bash command and stream its output line-by-line.
///
/// Returns a receiver that yields `StreamEvent` items as they arrive,
/// and a `thread::JoinHandle` for the spawned process thread.
/// The caller must drop/join the handle when done.
pub fn execute_tool_streaming(
    command: &str,
) -> (mpsc::Receiver<StreamEvent>, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel::<StreamEvent>();
    let command = command.to_string();

    let handle = thread::spawn(move || {
        let child = match Command::new("sh")
            .arg("-c")
            .arg(&command)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(StreamEvent::Stderr(format!("Failed to spawn: {}", e)));
                let _ = tx.send(StreamEvent::Done { exit_code: None });
                return;
            }
        };

        // Take stdout/stderr pipes before moving child into the wait thread
        let mut child = child;
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        // Spawn a thread to read stdout line-by-line
        let stdout_tx = tx.clone();
        let stdout_handle = thread::spawn(move || {
            if let Some(reader) = stdout_pipe {
                use std::io::{BufRead, BufReader};
                let mut buf_reader = BufReader::new(reader);
                let mut line = String::new();
                loop {
                    line.clear();
                    match buf_reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            // Strip trailing newline for clean output
                            let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
                            if stdout_tx.send(StreamEvent::Stdout(trimmed.to_string())).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        });

        // Spawn a thread to read stderr line-by-line
        let stderr_tx = tx.clone();
        let stderr_handle = thread::spawn(move || {
            if let Some(reader) = stderr_pipe {
                use std::io::{BufRead, BufReader};
                let mut buf_reader = BufReader::new(reader);
                let mut line = String::new();
                loop {
                    line.clear();
                    match buf_reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
                            if stderr_tx.send(StreamEvent::Stderr(trimmed.to_string())).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        });

        // Wait for child to finish
        let status = child.wait().ok();
        let exit_code = status.and_then(|s| s.code());

        // Wait for reader threads to finish draining
        let _ = stdout_handle.join();
        let _ = stderr_handle.join();

        let _ = tx.send(StreamEvent::Done { exit_code });
    });

    (rx, handle)
}

/// A call to a tool from the LLM
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub args: Value,
}

/// Parse tools from LLM response using the provided registry
pub fn parse_tools(response: &str, registry: &ToolRegistry) -> Vec<ToolCall> {
    let mut tools = Vec::new();
    
    for tool in registry.list() {
        let name = tool.name();
        let mut markers = vec![format!("```{}\n", name)];
        for alias in tool.aliases() {
            markers.push(format!("```{}\n", alias));
        }

        for marker in markers {
            let mut search_start = 0;
            while let Some(start) = response[search_start..].find(&marker) {
                let content_start = search_start + start + marker.len();
                if let Some(end) = response[content_start..].find("```") {
                    let content = response[content_start..content_start + end].trim();
                    if !content.is_empty() {
                        tools.push(ToolCall { 
                            name: name.to_string(), 
                            args: serde_json::json!({ tool.primary_arg_name(): content.to_string() }) 
                        });
                    }
                    search_start = content_start + end + 3;
                } else {
                    break;
                }
            }
        }
    }
    
    tools
}

/// Strip tool code blocks from response text (for display)
pub fn strip_tool_blocks(response: &str) -> String {
    let mut result = response.to_string();
    
    // Strip ```bash, ```sh, ```shell, ```read_file, ```file blocks
    let markers = ["```bash\n", "```sh\n", "```shell\n", "```read_file\n", "```file\n"];
    
    for marker in &markers {
        loop {
            if let Some(start) = result.find(*marker) {
                let content_start = start + marker.len();
                if let Some(end) = result[content_start..].find("```") {
                    // Remove from the marker start to the closing ```
                    let end_abs = content_start + end + 3;
                    result.replace_range(start..end_abs, "");
                    // Clean up extra blank lines left behind
                    while result.contains("\n\n\n") {
                        result = result.replace("\n\n\n", "\n\n");
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
    
    result.trim().to_string()
}

/// Check if a command looks dangerous
pub fn is_dangerous(command: &str) -> bool {
    let dangerous = [
        "rm -rf /",
        "rm -rf ~",
        "rm -rf *",
        "mkfs",
        "dd if=",
        "> /dev/sd",
        "chmod -R 777 /",
        ":(){ :|:& };:",
    ];
    
    dangerous.iter().any(|d| command.contains(d))
}
