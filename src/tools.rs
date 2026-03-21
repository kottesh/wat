use std::process::Command;
use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use anyhow::Result;

/// Tools available to the agent
#[derive(Debug, Clone)]
pub enum Tool {
    Bash { command: String },
    ReadFile { path: String },
}

/// Result of executing a tool
#[derive(Debug, Clone)]
pub struct ToolResult {
    #[allow(dead_code)]
    pub tool: Tool,
    pub output: String,
    #[allow(dead_code)]
    pub success: bool,
    pub duration_secs: f64,
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

/// Execute a tool and return the result
pub fn execute_tool(tool: &Tool) -> Result<ToolResult> {
    let start = std::time::Instant::now();
    
    let result = match tool {
        Tool::Bash { command } => {
            let output = Command::new("sh")
                .arg("-c")
                .arg(command)
                .output()?;
            
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let success = output.status.success();
            
            let combined = if stderr.is_empty() {
                stdout
            } else if stdout.is_empty() {
                stderr
            } else {
                format!("{}\n{}", stdout, stderr)
            };
            
            ToolResult {
                tool: tool.clone(),
                output: truncate_output(&combined, 100),
                success,
                duration_secs: start.elapsed().as_secs_f64(),
            }
        }
        Tool::ReadFile { path } => {
            let path = Path::new(path);
            
            if !path.exists() {
                ToolResult {
                    tool: tool.clone(),
                    output: format!("File not found: {}", path.display()),
                    success: false,
                    duration_secs: start.elapsed().as_secs_f64(),
                }
            } else {
                match fs::read_to_string(path) {
                    Ok(content) => {
                        // Show line numbers
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
                        
                        ToolResult {
                            tool: tool.clone(),
                            output: display,
                            success: true,
                            duration_secs: start.elapsed().as_secs_f64(),
                        }
                    }
                    Err(e) => ToolResult {
                        tool: tool.clone(),
                        output: format!("Failed to read file: {}", e),
                        success: false,
                        duration_secs: start.elapsed().as_secs_f64(),
                    },
                }
            }
        }
    };
    
    Ok(result)
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

/// Parse tools from LLM response
pub fn parse_tools(response: &str) -> Vec<Tool> {
    let mut tools = Vec::new();
    
    // Parse ```bash blocks
    let bash_markers = ["```bash\n", "```sh\n", "```shell\n"];
    for marker in &bash_markers {
        let mut search_start = 0;
        while let Some(start) = response[search_start..].find(*marker) {
            let content_start = search_start + start + marker.len();
            if let Some(end) = response[content_start..].find("```") {
                let command = response[content_start..content_start + end].trim();
                if !command.is_empty() {
                    tools.push(Tool::Bash { command: command.to_string() });
                }
                search_start = content_start + end + 3;
            } else {
                break;
            }
        }
    }
    
    // Parse ```read_file blocks
    let file_markers = ["```read_file\n", "```file\n"];
    for marker in &file_markers {
        let mut search_start = 0;
        while let Some(start) = response[search_start..].find(*marker) {
            let content_start = search_start + start + marker.len();
            if let Some(end) = response[content_start..].find("```") {
                let path = response[content_start..content_start + end].trim();
                if !path.is_empty() {
                    tools.push(Tool::ReadFile { path: path.to_string() });
                }
                search_start = content_start + end + 3;
            } else {
                break;
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
