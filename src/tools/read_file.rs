//! Read file tool implementation

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::Duration;

use super::executor::{ExecutionResult, ToolExecutor, ToolUpdate};

/// Read file tool
pub struct ReadFileTool;

impl ReadFileTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ToolExecutor for ReadFileTool {
    async fn execute(
        &self,
        args: Value,
        _on_update: Box<dyn Fn(ToolUpdate) + Send + Sync>,
    ) -> Result<ExecutionResult> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;

        let start = std::time::Instant::now();

        // Expand tilde
        let expanded_path = shellexpand::tilde(path_str).to_string();
        let path = Path::new(&expanded_path);

        if !path.exists() {
            return Ok(ExecutionResult {
                content: format!("File not found: {}", path_str),
                success: false,
                error: Some("File not found".to_string()),
                duration: start.elapsed(),
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

        Ok(ExecutionResult {
            content: display,
            success: true,
            error: None,
            duration: start.elapsed(),
        })
    }

    fn timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(30)) // 30 second timeout for file operations
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_read_file_tool_success() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "Hello\nWorld").unwrap();

        let tool = ReadFileTool::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap()
        });

        let on_update = Box::new(|_: ToolUpdate| {});
        let result = tool.execute(args, on_update).await.unwrap();

        assert!(result.success);
        assert!(result.content.contains("Hello"));
        assert!(result.content.contains("World"));
        assert!(result.content.contains("     1  Hello"));
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_read_file_tool_not_found() {
        let tool = ReadFileTool::new();
        let args = serde_json::json!({
            "path": "/nonexistent/file.txt"
        });

        let on_update = Box::new(|_: ToolUpdate| {});
        let result = tool.execute(args, on_update).await.unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.content.contains("File not found"));
    }

    #[tokio::test]
    async fn test_read_file_tool_timeout() {
        let tool = ReadFileTool::new();
        assert_eq!(tool.timeout(), Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_add_line_numbers() {
        let content = "Line 1\nLine 2\nLine 3";
        let numbered = add_line_numbers(content);

        assert!(numbered.contains("     1  Line 1"));
        assert!(numbered.contains("     2  Line 2"));
        assert!(numbered.contains("     3  Line 3"));
    }
}
