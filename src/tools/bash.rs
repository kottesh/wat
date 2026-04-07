//! Bash tool implementation with timeout support

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::executor::{ExecutionResult, ToolExecutor, ToolUpdate};

/// Bash tool for executing shell commands
pub struct BashTool;

impl BashTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ToolExecutor for BashTool {
    async fn execute(
        &self,
        args: Value,
        on_update: Box<dyn Fn(ToolUpdate) + Send + Sync>,
    ) -> Result<ExecutionResult> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'command' parameter"))?;

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

        // Spawn stdout reader
        let tx_out = tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = tx_out.send(ToolUpdate::Stdout(line)).await;
            }
        });

        // Spawn stderr reader
        let tx_err = tx;
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = tx_err.send(ToolUpdate::Stderr(line)).await;
            }
        });

        let mut output = String::new();

        // Collect output
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
                    // Drain remaining updates
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
                    let duration = start.elapsed();

                    return Ok(ExecutionResult {
                        content: truncate_output(&output, 100),
                        success,
                        error: if success { None } else { Some("Command failed".to_string()) },
                        duration,
                    });
                }
            }
        }
    }

    fn timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(120)) // 120 second timeout as specified
    }
}

/// Truncate output to max lines
fn truncate_output(output: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();

    if lines.len() > max_lines {
        let truncated: Vec<&str> = lines[..max_lines].to_vec();
        format!(
            "{}\n... ({} more lines)",
            truncated.join("\n"),
            lines.len() - max_lines
        )
    } else {
        output.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn test_bash_tool_success() {
        let tool = BashTool::new();
        let args = serde_json::json!({
            "command": "echo 'Hello, World!'"
        });

        let updates = Arc::new(Mutex::new(Vec::new()));
        let updates_clone = updates.clone();

        let on_update = Box::new(move |update: ToolUpdate| {
            updates_clone.lock().unwrap().push(update);
        });

        let result = tool.execute(args, on_update).await.unwrap();

        assert!(result.success);
        assert!(result.content.contains("Hello, World!"));
        assert!(result.error.is_none());

        let updates = updates.lock().unwrap();
        assert!(!updates.is_empty());
    }

    #[tokio::test]
    async fn test_bash_tool_failure() {
        let tool = BashTool::new();
        let args = serde_json::json!({
            "command": "exit 1"
        });

        let on_update = Box::new(|_: ToolUpdate| {});
        let result = tool.execute(args, on_update).await.unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_bash_tool_timeout() {
        let tool = BashTool::new();
        assert_eq!(tool.timeout(), Some(Duration::from_secs(120)));
    }

    #[test]
    fn test_truncate_output() {
        let long_output = (0..150)
            .map(|i| format!("Line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let truncated = truncate_output(&long_output, 100);

        assert!(truncated.contains("... (50 more lines)"));
        assert!(truncated.contains("Line 0"));
        assert!(truncated.contains("Line 99"));
        assert!(!truncated.contains("Line 100"));
    }
}
