use std::path::Path;
use std::fs;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use futures_util::stream::StreamExt;

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
        
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::channel(100);

        let tx_out = tx.clone();
        tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if tx_out.send(ToolUpdate::Stdout(line)).await.is_err() { break; }
            }
        });

        let tx_err = tx.clone();
        tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if tx_err.send(ToolUpdate::Stderr(line)).await.is_err() { break; }
            }
        });

        let mut output = String::new();
        
        loop {
            tokio::select! {
                Some(update) = rx.recv() => {
                    match &update {
                        ToolUpdate::Stdout(l) | ToolUpdate::Stderr(l) => {
                            output.push_str(l);
                            output.push('\n');
                        }
                        _ => {}
                    }
                    on_update(update);
                }
                status = child.wait() => {
                    // Drain remaining channel items
                    while let Ok(update) = rx.try_recv() {
                        match &update {
                            ToolUpdate::Stdout(l) | ToolUpdate::Stderr(l) => {
                                output.push_str(l);
                                output.push('\n');
                            }
                            _ => {}
                        }
                        on_update(update);
                    }
                    
                    let success = status.map(|s| s.success()).unwrap_or(false);
                    return Ok(ToolResult {
                        content: truncate_output(&output, 100),
                        details: serde_json::json!({ "output": output, "success": success }),
                        success,
                        duration_secs: start.elapsed().as_secs_f64(),
                    });
                }
            }
        }
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
