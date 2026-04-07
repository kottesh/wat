//! Core types for LLM interactions

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Unified message format across all providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: MessageContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "tool")]
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
    Mixed {
        text: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
}

/// Tool call from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider-specific ID (e.g., "call_abc123" for OpenAI, "toolu_xyz" for Anthropic)
    pub id: String,
    /// Tool name (e.g., "bash", "read_file")
    pub name: String,
    /// JSON arguments
    pub arguments: Value,
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Matches ToolCall.id
    pub tool_call_id: String,
    /// Result content
    pub content: String,
    /// Whether execution succeeded
    pub success: bool,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Streaming response chunk
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Text delta (incremental text content)
    TextDelta(String),

    /// Tool call started
    ToolCallStart {
        id: String,
        name: String,
        index: usize, // Position in the tool calls array
    },

    /// Incremental tool call arguments (JSON delta)
    ToolCallArgsDelta {
        #[allow(dead_code)] // Kept for completeness, indexed by index field
        id: String,
        index: usize,
        args_json_delta: String,
    },

    /// Tool call completed
    ToolCallComplete {
        #[allow(dead_code)] // Kept for API consistency
        id: String,
        #[allow(dead_code)] // Kept for API consistency
        index: usize,
    },

    /// Response finished
    Done {
        #[allow(dead_code)] // Kept for future logging/debugging
        finish_reason: FinishReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    /// Natural completion
    Stop,
    /// Stopped to execute tools
    ToolCalls,
    /// Max tokens reached
    Length,
    /// Content filter triggered
    ContentFilter,
    /// Error occurred
    Error(String),
}

impl Message {
    pub fn system(content: &str) -> Self {
        Self {
            role: MessageRole::System,
            content: MessageContent::Text(content.to_string()),
        }
    }

    pub fn user(content: &str) -> Self {
        Self {
            role: MessageRole::User,
            content: MessageContent::Text(content.to_string()),
        }
    }

    pub fn assistant(content: &str) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: MessageContent::Text(content.to_string()),
        }
    }

    pub fn assistant_with_tools(text: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: if let Some(t) = text {
                MessageContent::Mixed {
                    text: Some(t),
                    tool_calls,
                }
            } else {
                MessageContent::Mixed {
                    text: None,
                    tool_calls,
                }
            },
        }
    }

    pub fn tool_result(result: ToolResult) -> Self {
        Self {
            role: MessageRole::Tool,
            content: MessageContent::ToolResult(result),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_constructors() {
        let sys = Message::system("system prompt");
        assert_eq!(sys.role, MessageRole::System);
        assert!(matches!(sys.content, MessageContent::Text(_)));

        let user = Message::user("hello");
        assert_eq!(user.role, MessageRole::User);

        let asst = Message::assistant("world");
        assert_eq!(asst.role, MessageRole::Assistant);
    }

    #[test]
    fn test_message_with_tools() {
        let tool_call = ToolCall {
            id: "call_123".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "ls"}),
        };

        let msg = Message::assistant_with_tools(Some("Running ls".to_string()), vec![tool_call]);
        assert_eq!(msg.role, MessageRole::Assistant);

        match msg.content {
            MessageContent::Mixed { text, tool_calls } => {
                assert_eq!(text, Some("Running ls".to_string()));
                assert_eq!(tool_calls.len(), 1);
            }
            _ => panic!("Expected Mixed content"),
        }
    }

    #[test]
    fn test_tool_result() {
        let result = ToolResult {
            tool_call_id: "call_123".to_string(),
            content: "output".to_string(),
            success: true,
            error: None,
        };

        let msg = Message::tool_result(result);
        assert_eq!(msg.role, MessageRole::Tool);

        match msg.content {
            MessageContent::ToolResult(r) => {
                assert_eq!(r.tool_call_id, "call_123");
                assert!(r.success);
            }
            _ => panic!("Expected ToolResult content"),
        }
    }
}
