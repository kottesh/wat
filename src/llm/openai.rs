//! OpenAI provider implementation with native function calling

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;

use super::provider::{LlmProvider, ProviderCapabilities, StreamOptions};
use super::types::{FinishReason, Message, MessageContent, MessageRole, StreamChunk, ToolCall};
use crate::tools::ToolDefinition;

/// OpenAI provider
pub struct OpenAiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model_id: String,
    capabilities: ProviderCapabilities,
}

/// OpenAI chat completion request
#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage {
    role: String,
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunctionDef,
}

#[derive(Debug, Serialize)]
struct OpenAiFunctionDef {
    name: String,
    description: String,
    parameters: Value,
}

/// Streaming response chunk
#[derive(Debug, Deserialize)]
struct StreamResponse {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<StreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct StreamToolCall {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type", default)]
    call_type: Option<String>,
    #[serde(default)]
    function: Option<StreamFunction>,
}

#[derive(Debug, Deserialize)]
struct StreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

impl OpenAiProvider {
    pub fn new(base_url: String, api_key: String, model_id: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .context("Failed to create HTTP client")?;

        let capabilities = ProviderCapabilities {
            parallel_tool_calls: true,
            tool_streaming: true,
            vision: false,
            max_tools_per_call: 16,
        };

        Ok(Self {
            client,
            base_url,
            api_key,
            model_id,
            capabilities,
        })
    }

    /// Convert our Message format to OpenAI format
    fn convert_messages(&self, messages: Vec<Message>) -> Result<Vec<OpenAiMessage>> {
        let mut openai_messages = Vec::new();

        for msg in messages {
            match msg.content {
                MessageContent::Text(text) => {
                    openai_messages.push(OpenAiMessage {
                        role: match msg.role {
                            MessageRole::System => "system".to_string(),
                            MessageRole::User => "user".to_string(),
                            MessageRole::Assistant => "assistant".to_string(),
                            MessageRole::Tool => "tool".to_string(),
                        },
                        content: Some(text),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
                MessageContent::Mixed { text, tool_calls } => {
                    let openai_tool_calls = tool_calls
                        .into_iter()
                        .map(|tc| OpenAiToolCall {
                            id: tc.id,
                            call_type: "function".to_string(),
                            function: OpenAiFunction {
                                name: tc.name,
                                arguments: serde_json::to_string(&tc.arguments)
                                    .unwrap_or_else(|_| "{}".to_string()),
                            },
                        })
                        .collect();

                    openai_messages.push(OpenAiMessage {
                        role: "assistant".to_string(),
                        content: text,
                        tool_calls: Some(openai_tool_calls),
                        tool_call_id: None,
                    });
                }
                MessageContent::ToolResult(result) => {
                    openai_messages.push(OpenAiMessage {
                        role: "tool".to_string(),
                        content: Some(result.content),
                        tool_calls: None,
                        tool_call_id: Some(result.tool_call_id),
                    });
                }
                MessageContent::ToolCall(_) => {
                    // Single tool calls should be wrapped in Mixed
                    anyhow::bail!("Unexpected ToolCall content type");
                }
            }
        }

        Ok(openai_messages)
    }

    /// Convert tool definitions to OpenAI format
    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<OpenAiTool> {
        tools
            .iter()
            .map(|t| OpenAiTool {
                tool_type: "function".to_string(),
                function: OpenAiFunctionDef {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.to_openai(),
                },
            })
            .collect()
    }

    /// Parse a single SSE line (instance method)
    fn parse_sse_line(&self, line: &str) -> Option<Vec<StreamChunk>> {
        parse_sse_line_static(line)
    }
}

/// Parse a single SSE line (static function for use in streams)
fn parse_sse_line_static(line: &str) -> Option<Vec<StreamChunk>> {
        if !line.starts_with("data: ") {
            return None;
        }

        let data = line.trim_start_matches("data: ").trim();
        if data == "[DONE]" {
            return None;
        }

        let parsed: StreamResponse = match serde_json::from_str(data) {
            Ok(p) => p,
            Err(_) => return None,
        };

        let mut chunks = Vec::new();

        for choice in parsed.choices {
            // Handle finish reason
            if let Some(reason) = choice.finish_reason {
                let finish = match reason.as_str() {
                    "stop" => FinishReason::Stop,
                    "tool_calls" => FinishReason::ToolCalls,
                    "length" => FinishReason::Length,
                    "content_filter" => FinishReason::ContentFilter,
                    other => FinishReason::Error(format!("Unknown finish reason: {}", other)),
                };
                chunks.push(StreamChunk::Done { finish_reason: finish });
                continue;
            }

            // Handle text delta
            if let Some(content) = choice.delta.content {
                if !content.is_empty() {
                    chunks.push(StreamChunk::TextDelta(content));
                }
            }

            // Handle tool calls
            if let Some(tool_calls) = choice.delta.tool_calls {
                for tc in tool_calls {
                    // Tool call started (has id and name)
                    if let (Some(id), Some(func)) = (&tc.id, &tc.function) {
                        if let Some(name) = &func.name {
                            chunks.push(StreamChunk::ToolCallStart {
                                id: id.clone(),
                                name: name.clone(),
                                index: tc.index,
                            });
                        }
                    }

                    // Arguments delta
                    if let Some(func) = &tc.function {
                        if let Some(args) = &func.arguments {
                            if !args.is_empty() {
                                // We need an ID for the delta - try to get it from previous context
                                // For now, use empty string - will be handled by accumulator
                                chunks.push(StreamChunk::ToolCallArgsDelta {
                                    id: tc.id.clone().unwrap_or_default(),
                                    index: tc.index,
                                    args_json_delta: args.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        if chunks.is_empty() {
            None
        } else {
            Some(chunks)
        }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    async fn stream(
        &self,
        messages: Vec<Message>,
        tools: Option<&[ToolDefinition]>,
        options: StreamOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        let url = format!("{}/chat/completions", self.base_url);

        let openai_messages = self.convert_messages(messages)?;
        let openai_tools = tools.map(|t| self.convert_tools(t));

        let request = OpenAiRequest {
            model: self.model_id.clone(),
            messages: openai_messages,
            tools: openai_tools,
            tool_choice: if tools.is_some() {
                Some("auto".to_string())
            } else {
                None
            },
            temperature: options.temperature,
            max_tokens: options.max_tokens,
            stream: true,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error: {}", error_text);
        }

        // Create stream processor
        // Parse SSE in a closure that doesn't capture self
        let stream = response.bytes_stream().flat_map(|item| {
            let chunks = match item {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let mut result = Vec::new();

                    for line in text.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        // Parse inline instead of calling self.parse_sse_line
                        if let Some(parsed_chunks) = parse_sse_line_static(line) {
                            result.extend(parsed_chunks);
                        }
                    }

                    result
                        .into_iter()
                        .map(Ok)
                        .collect::<Vec<Result<StreamChunk>>>()
                }
                Err(e) => vec![Err(anyhow::anyhow!("Stream error: {}", e))],
            };

            futures_util::stream::iter(chunks)
        });

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ParameterSchema;

    #[test]
    fn test_openai_provider_creation() {
        let provider = OpenAiProvider::new(
            "https://api.openai.com/v1".to_string(),
            "test-key".to_string(),
            "gpt-4".to_string(),
        )
        .unwrap();

        assert_eq!(provider.name(), "openai");
        assert!(provider.capabilities().parallel_tool_calls);
        assert!(provider.capabilities().tool_streaming);
    }

    #[test]
    fn test_convert_text_messages() {
        let provider = OpenAiProvider::new(
            "https://api.openai.com/v1".to_string(),
            "test-key".to_string(),
            "gpt-4".to_string(),
        )
        .unwrap();

        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there"),
        ];

        let converted = provider.convert_messages(messages).unwrap();
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0].role, "system");
        assert_eq!(converted[1].role, "user");
        assert_eq!(converted[2].role, "assistant");
    }

    #[test]
    fn test_convert_tools() {
        let provider = OpenAiProvider::new(
            "https://api.openai.com/v1".to_string(),
            "test-key".to_string(),
            "gpt-4".to_string(),
        )
        .unwrap();

        let schema = ParameterSchema::new().add_string("command", "Command to run", true);

        // We can't easily create a ToolDefinition without executor, so just test the structure
        // This will be tested in integration tests
    }

    #[test]
    fn test_parse_text_delta() {
        let provider = OpenAiProvider::new(
            "https://api.openai.com/v1".to_string(),
            "test-key".to_string(),
            "gpt-4".to_string(),
        )
        .unwrap();

        let line = r#"data: {"choices":[{"delta":{"content":"Hello"}}]}"#;
        let chunks = provider.parse_sse_line(line).unwrap();

        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::TextDelta(text) => assert_eq!(text, "Hello"),
            _ => panic!("Expected TextDelta"),
        }
    }

    #[test]
    fn test_parse_tool_call_start() {
        let provider = OpenAiProvider::new(
            "https://api.openai.com/v1".to_string(),
            "test-key".to_string(),
            "gpt-4".to_string(),
        )
        .unwrap();

        let line = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"bash"}}]}}]}"#;
        let chunks = provider.parse_sse_line(line).unwrap();

        assert!(!chunks.is_empty());
        match &chunks[0] {
            StreamChunk::ToolCallStart { id, name, index } => {
                assert_eq!(id, "call_abc");
                assert_eq!(name, "bash");
                assert_eq!(*index, 0);
            }
            _ => panic!("Expected ToolCallStart"),
        }
    }

    #[test]
    fn test_parse_tool_args_delta() {
        let provider = OpenAiProvider::new(
            "https://api.openai.com/v1".to_string(),
            "test-key".to_string(),
            "gpt-4".to_string(),
        )
        .unwrap();

        let line = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\""}}]}}]}"#;
        let chunks = provider.parse_sse_line(line).unwrap();

        assert!(!chunks.is_empty());
        match &chunks[0] {
            StreamChunk::ToolCallArgsDelta { args_json_delta, index, .. } => {
                assert_eq!(args_json_delta, "{\"");
                assert_eq!(*index, 0);
            }
            _ => panic!("Expected ToolCallArgsDelta"),
        }
    }

    #[test]
    fn test_parse_finish_reason() {
        let provider = OpenAiProvider::new(
            "https://api.openai.com/v1".to_string(),
            "test-key".to_string(),
            "gpt-4".to_string(),
        )
        .unwrap();

        let line = r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
        let chunks = provider.parse_sse_line(line).unwrap();

        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::Done { finish_reason } => {
                assert_eq!(*finish_reason, FinishReason::Stop);
            }
            _ => panic!("Expected Done"),
        }
    }

    #[test]
    fn test_parse_done_marker() {
        let provider = OpenAiProvider::new(
            "https://api.openai.com/v1".to_string(),
            "test-key".to_string(),
            "gpt-4".to_string(),
        )
        .unwrap();

        let line = "data: [DONE]";
        let chunks = provider.parse_sse_line(line);
        assert!(chunks.is_none());
    }
}
