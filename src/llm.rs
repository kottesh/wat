use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
use crate::config::{Config, LlmProvider};
use futures_util::StreamExt;
use futures_util::stream::BoxStream;

/// LLM client for different providers
pub struct LlmClient {
    config: Config,
    client: reqwest::Client,
}

/// LLM request
#[derive(Debug, Serialize)]
struct LlmRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
}

/// Message in conversation
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Streaming delta for OpenAI/Custom
#[derive(Debug, Deserialize)]
struct StreamResponse {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

impl LlmClient {
    pub fn new(config: Config) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .context("Failed to create HTTP client")?;
        
        Ok(Self { config, client })
    }
    
    pub async fn query_stream(&self, messages: Vec<Message>) -> Result<BoxStream<'static, Result<String>>> {
        let provider = self.config.llm.provider;
        let client = self.client.clone();
        let config = self.config.clone();

        match provider {
            LlmProvider::OpenAI | LlmProvider::Custom => {
                let s = Self::stream_openai(client, config, messages).await?;
                Ok(s.boxed())
            }
            LlmProvider::Anthropic => {
                let s = Self::stream_anthropic(client, config, messages).await?;
                Ok(s.boxed())
            }
            LlmProvider::Local => anyhow::bail!("Local LLM streaming not implemented"),
        }
    }

    async fn stream_openai(client: reqwest::Client, config: Config, messages: Vec<Message>) -> Result<impl futures_util::Stream<Item = Result<String>>> {
        let url = config.llm.base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string());
        
        let request = LlmRequest {
            model: config.llm.model.clone(),
            messages,
            temperature: config.llm.temperature,
            max_tokens: config.llm.max_tokens,
            stream: true,
        };
        
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.llm.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request")?;
        
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error: {}", error_text);
        }

        let stream = response.bytes_stream().map(|item| {
            let chunk = item.context("Failed to read stream chunk")?;
            let text = String::from_utf8_lossy(&chunk).to_string();
            let mut result = String::new();

            for line in text.lines() {
                let line = line.trim();
                if line.starts_with("data: ") {
                    let data = line.trim_start_matches("data: ").trim();
                    if data == "[DONE]" { continue; }
                    if let Ok(parsed) = serde_json::from_str::<StreamResponse>(data) {
                        if let Some(content) = &parsed.choices[0].delta.content {
                            result.push_str(content);
                        }
                    }
                }
            }
            Ok(result)
        });

        Ok(stream)
    }

    async fn stream_anthropic(client: reqwest::Client, config: Config, messages: Vec<Message>) -> Result<impl futures_util::Stream<Item = Result<String>>> {
        let url = config.llm.base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com/v1/messages".to_string());
        
        let anthropic_messages: Vec<AnthropicMessage> = messages
            .into_iter()
            .map(|m| AnthropicMessage {
                role: m.role,
                content: vec![AnthropicContent::Text { text: m.content }],
            })
            .collect();
        
        let request = AnthropicStreamRequest {
            model: config.llm.model.clone(),
            messages: anthropic_messages,
            max_tokens: config.llm.max_tokens,
            temperature: config.llm.temperature,
            stream: true,
        };
        
        let response = client
            .post(&url)
            .header("x-api-key", &config.llm.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Anthropic")?;
        
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic error: {}", error_text);
        }

        let stream = response.bytes_stream().map(|item| {
            let chunk = item.context("Failed to read Anthropic chunk")?;
            let text = String::from_utf8_lossy(&chunk).to_string();
            let mut result = String::new();

            for line in text.lines() {
                let line = line.trim();
                if line.starts_with("data: ") {
                    let data = line.trim_start_matches("data: ").trim();
                    if let Ok(parsed) = serde_json::from_str::<AnthropicStreamEvent>(data) {
                        match parsed {
                            AnthropicStreamEvent::ContentBlockDelta { delta } => {
                                result.push_str(&delta.text);
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(result)
        });

        Ok(stream)
    }

    pub async fn query(&self, messages: Vec<Message>) -> Result<Message> {
        let mut stream = self.query_stream(messages).await?;
        let mut full_content = String::new();
        while let Some(chunk) = stream.next().await {
            full_content.push_str(&chunk?);
        }
        Ok(Message::assistant(&full_content))
    }
}

/// Anthropic types
#[derive(Debug, Serialize)]
struct AnthropicStreamRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    temperature: f32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum AnthropicContent {
    #[serde(rename = "text")]
    Text { text: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart {},
    #[serde(rename = "content_block_start")]
    ContentBlockStart {},
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { delta: AnthropicDelta },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop {},
    #[serde(rename = "message_delta")]
    MessageDelta {},
    #[serde(rename = "message_stop")]
    MessageStop {},
    #[serde(rename = "ping")]
    Ping {},
}

#[derive(Debug, Deserialize)]
struct AnthropicDelta {
    text: String,
}

impl Message {
    pub fn system(content: &str) -> Self {
        Self { role: "system".to_string(), content: content.to_string() }
    }
    pub fn user(content: &str) -> Self {
        Self { role: "user".to_string(), content: content.to_string() }
    }
    pub fn assistant(content: &str) -> Self {
        Self { role: "assistant".to_string(), content: content.to_string() }
    }
}
