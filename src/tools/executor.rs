//! Tool executor trait and types

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;

/// Result of tool execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub content: String,
    pub success: bool,
    pub error: Option<String>,
    pub duration: Duration,
}

/// Progress update from a running tool
#[derive(Debug, Clone)]
pub enum ToolUpdate {
    Stdout(String),
    Stderr(String),
    #[allow(dead_code)] // Future use for progress bars
    Progress { current: u64, total: u64 },
}

/// Trait for executing tools
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute the tool with given arguments
    async fn execute(
        &self,
        args: Value,
        on_update: Box<dyn Fn(ToolUpdate) + Send + Sync>,
    ) -> Result<ExecutionResult>;

    /// Get timeout for this tool (None = no timeout)
    fn timeout(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_result() {
        let result = ExecutionResult {
            content: "test output".to_string(),
            success: true,
            error: None,
            duration: Duration::from_secs(1),
        };

        assert!(result.success);
        assert_eq!(result.content, "test output");
        assert!(result.error.is_none());
    }

    #[test]
    fn test_execution_result_with_error() {
        let result = ExecutionResult {
            content: "".to_string(),
            success: false,
            error: Some("Command failed".to_string()),
            duration: Duration::from_millis(500),
        };

        assert!(!result.success);
        assert_eq!(result.error, Some("Command failed".to_string()));
    }
}
