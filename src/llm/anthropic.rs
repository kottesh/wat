//! Anthropic provider implementation with native tool use

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::pin::Pin;

use super::provider::{LlmProvider, ProviderCapabilities, StreamOptions};
use super::types::{FinishReason, Message, MessageContent, MessageRole, StreamChunk};
use crate::tools::ToolDefinition;

/// Anthropic provider
pub struct AnthropicProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model_id: String,
    capabilities: ProviderCapabilities,
}

/// Anthropic messages request
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    temperature: f32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: Value,
}

/// Streaming event types
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicEvent {
    #[serde(rename = "message_start")]
    #[allow(dead_code)] // Part of API response schema
    MessageStart { message: MessageInfo },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: usize,
        content_block: ContentBlock,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: usize, delta: ContentDelta },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },
    #[serde(rename = "message_delta")]
    MessageDelta { delta: MessageDeltaInfo },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Deserialization only
struct MessageInfo {
    id: String,
    role: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    #[allow(dead_code)] // Part of API response schema
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Deserialize)]
struct MessageDeltaInfo {
    stop_reason: Option<String>,
}

impl AnthropicProvider {
    pub fn new(base_url: String, api_key: String, model_id: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .context("Failed to create HTTP client")?;

        let capabilities = ProviderCapabilities {
            native_tools: true,
            parallel_tool_calls: true,
            tool_streaming: false, // Anthropic sends complete tool blocks, not incremental args
            vision: true,
            max_tools_per_call: 64,
        };

        Ok(Self {
            client,
            base_url,
            api_key,
            model_id,
            capabilities,
        })
    }

    /// Convert messages and extract system message
    fn convert_messages(
        &self,
        messages: Vec<Message>,
    ) -> Result<(Option<String>, Vec<AnthropicMessage>)> {
        let mut system_message: Option<String> = None;
        let mut anthropic_messages = Vec::new();

        for msg in messages {
            // Extract system message separately
            if matches!(msg.role, MessageRole::System) {
                if let MessageContent::Text(text) = msg.content {
                    system_message = Some(text);
                }
                continue;
            }

            let role = match msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "user", // Tool results come back as user messages
                MessageRole::System => continue, // Already handled
            };

            let content = match msg.content {
                MessageContent::Text(text) => vec![AnthropicContent::Text { text }],
                MessageContent::Mixed { text, tool_calls } => {
                    let mut contents = Vec::new();
                    if let Some(t) = text {
                        contents.push(AnthropicContent::Text { text: t });
                    }
                    for tc in tool_calls {
                        contents.push(AnthropicContent::ToolUse {
                            id: tc.id,
                            name: tc.name,
                            input: tc.arguments,
                        });
                    }
                    contents
                }
                MessageContent::ToolResult(result) => {
                    vec![AnthropicContent::ToolResult {
                        tool_use_id: result.tool_call_id,
                        content: result.content,
                    }]
                }
                MessageContent::ToolCall(_) => {
                    anyhow::bail!("Unexpected ToolCall content type");
                }
            };

            anthropic_messages.push(AnthropicMessage {
                role: role.to_string(),
                content,
            });
        }

        Ok((system_message, anthropic_messages))
    }

    /// Convert tool definitions to Anthropic format
    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<AnthropicTool> {
        tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters.to_anthropic(),
            })
            .collect()
    }
}

/// Streaming state tracker for tool calls
#[derive(Default)]
struct StreamState {
    current_tool_calls: std::collections::HashMap<usize, (Option<String>, Option<String>, String)>,
}

impl StreamState {
    fn process_event(&mut self, event: AnthropicEvent) -> Vec<StreamChunk> {
        let mut chunks = Vec::new();

        match event {
            AnthropicEvent::MessageStart { .. } => {
                // Message started, nothing to emit
            }
            AnthropicEvent::ContentBlockStart {
                index,
                content_block,
            } => match content_block {
                ContentBlock::Text { .. } => {
                    // Text block started, will get deltas next
                }
                ContentBlock::ToolUse { id, name } => {
                    self.current_tool_calls
                        .insert(index, (Some(id.clone()), Some(name.clone()), String::new()));
                    chunks.push(StreamChunk::ToolCallStart { id, name, index });
                }
            },
            AnthropicEvent::ContentBlockDelta { index, delta } => match delta {
                ContentDelta::TextDelta { text } => {
                    chunks.push(StreamChunk::TextDelta(text));
                }
                ContentDelta::InputJsonDelta { partial_json } => {
                    if let Some((id, _name, args)) = self.current_tool_calls.get_mut(&index) {
                        args.push_str(&partial_json);
                        if let Some(tool_id) = id {
                            chunks.push(StreamChunk::ToolCallArgsDelta {
                                id: tool_id.clone(),
                                index,
                                args_json_delta: partial_json,
                            });
                        }
                    }
                }
            },
            AnthropicEvent::ContentBlockStop { index } => {
                if let Some((id, _name, _args)) = self.current_tool_calls.remove(&index) {
                    if let Some(tool_id) = id {
                        chunks.push(StreamChunk::ToolCallComplete { id: tool_id, index });
                    }
                }
            }
            AnthropicEvent::MessageDelta { delta } => {
                if let Some(reason) = delta.stop_reason {
                    let finish = match reason.as_str() {
                        "end_turn" => FinishReason::Stop,
                        "tool_use" => FinishReason::ToolCalls,
                        "max_tokens" => FinishReason::Length,
                        other => FinishReason::Error(format!("Unknown stop reason: {}", other)),
                    };
                    chunks.push(StreamChunk::Done {
                        finish_reason: finish,
                    });
                }
            }
            AnthropicEvent::MessageStop => {
                // Message complete - if we haven't seen a finish reason, default to Stop
                if chunks.is_empty() {
                    chunks.push(StreamChunk::Done {
                        finish_reason: FinishReason::Stop,
                    });
                }
            }
            AnthropicEvent::Ping => {
                // Keep-alive, ignore
            }
        }

        chunks
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
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
        let url = format!("{}/messages", self.base_url);

        let (system, anthropic_messages) = self.convert_messages(messages)?;
        let anthropic_tools = tools.map(|t| self.convert_tools(t));

        let request = AnthropicRequest {
            model: self.model_id.clone(),
            messages: anthropic_messages,
            system,
            max_tokens: options.max_tokens,
            tools: anthropic_tools,
            temperature: options.temperature,
            stream: true,
        };

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Anthropic")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error: {}", error_text);
        }

        // Parse SSE stream
        let stream = response
            .bytes_stream()
            .scan(StreamState::default(), |state, item| {
                let chunks = match item {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        let mut result = Vec::new();

                        for line in text.lines() {
                            let line = line.trim();
                            if line.is_empty() || !line.starts_with("data: ") {
                                continue;
                            }

                            let data = line.trim_start_matches("data: ").trim();
                            if let Ok(event) = serde_json::from_str::<AnthropicEvent>(data) {
                                let event_chunks = state.process_event(event);
                                result.extend(event_chunks);
                            }
                        }

                        result
                            .into_iter()
                            .map(Ok)
                            .collect::<Vec<Result<StreamChunk>>>()
                    }
                    Err(e) => vec![Err(anyhow::anyhow!("Stream error: {}", e))],
                };

                futures_util::future::ready(Some(futures_util::stream::iter(chunks)))
            })
            .flatten();

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_provider_creation() {
        let provider = AnthropicProvider::new(
            "https://api.anthropic.com/v1".to_string(),
            "test-key".to_string(),
            "claude-3-opus-20240229".to_string(),
        )
        .unwrap();

        assert_eq!(provider.name(), "anthropic");
        assert!(provider.capabilities().parallel_tool_calls);
        assert!(!provider.capabilities().tool_streaming); // Anthropic doesn't stream args
        assert!(provider.capabilities().vision);
    }

    #[test]
    fn test_convert_messages_with_system() {
        let provider = AnthropicProvider::new(
            "https://api.anthropic.com/v1".to_string(),
            "test-key".to_string(),
            "claude-3-opus-20240229".to_string(),
        )
        .unwrap();

        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there"),
        ];

        let (system, converted) = provider.convert_messages(messages).unwrap();
        assert_eq!(system, Some("You are helpful".to_string()));
        assert_eq!(converted.len(), 2); // System message extracted
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "assistant");
    }

    #[test]
    fn test_stream_state_text_delta() {
        let mut state = StreamState::default();

        let event = AnthropicEvent::ContentBlockDelta {
            index: 0,
            delta: ContentDelta::TextDelta {
                text: "Hello".to_string(),
            },
        };

        let chunks = state.process_event(event);
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::TextDelta(text) => assert_eq!(text, "Hello"),
            _ => panic!("Expected TextDelta"),
        }
    }

    #[test]
    fn test_stream_state_tool_use() {
        let mut state = StreamState::default();

        // Start tool block
        let start_event = AnthropicEvent::ContentBlockStart {
            index: 0,
            content_block: ContentBlock::ToolUse {
                id: "toolu_123".to_string(),
                name: "bash".to_string(),
            },
        };

        let chunks = state.process_event(start_event);
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::ToolCallStart { id, name, index } => {
                assert_eq!(id, "toolu_123");
                assert_eq!(name, "bash");
                assert_eq!(*index, 0);
            }
            _ => panic!("Expected ToolCallStart"),
        }

        // Add args delta
        let delta_event = AnthropicEvent::ContentBlockDelta {
            index: 0,
            delta: ContentDelta::InputJsonDelta {
                partial_json: "{\"command\":".to_string(),
            },
        };

        let chunks = state.process_event(delta_event);
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::ToolCallArgsDelta {
                id,
                args_json_delta,
                ..
            } => {
                assert_eq!(id, "toolu_123");
                assert_eq!(args_json_delta, "{\"command\":");
            }
            _ => panic!("Expected ToolCallArgsDelta"),
        }

        // Stop tool block
        let stop_event = AnthropicEvent::ContentBlockStop { index: 0 };
        let chunks = state.process_event(stop_event);
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::ToolCallComplete { id, index } => {
                assert_eq!(id, "toolu_123");
                assert_eq!(*index, 0);
            }
            _ => panic!("Expected ToolCallComplete"),
        }
    }

    #[test]
    fn test_stream_state_finish_reason() {
        let mut state = StreamState::default();

        let event = AnthropicEvent::MessageDelta {
            delta: MessageDeltaInfo {
                stop_reason: Some("end_turn".to_string()),
            },
        };

        let chunks = state.process_event(event);
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::Done { finish_reason } => {
                assert_eq!(*finish_reason, FinishReason::Stop);
            }
            _ => panic!("Expected Done"),
        }
    }

    #[test]
    fn test_stream_state_ping_ignored() {
        let mut state = StreamState::default();
        let event = AnthropicEvent::Ping;
        let chunks = state.process_event(event);
        assert_eq!(chunks.len(), 0); // Ping should produce no chunks
    }
}
