use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
use crate::config::{Config, LlmProvider};

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
}

/// Message in conversation
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// LLM response
#[derive(Debug, Deserialize)]
struct LlmResponse {
    choices: Vec<Choice>,
    #[allow(dead_code)]
    usage: Option<Usage>,
}

/// Choice in response
#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
    #[allow(dead_code)]
    finish_reason: String,
}

/// Token usage
#[derive(Debug, Deserialize)]
struct Usage {
    #[allow(dead_code)]
    prompt_tokens: u32,
    #[allow(dead_code)]
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

impl LlmClient {
    /// Create new LLM client
    pub fn new(config: Config) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;
        
        Ok(Self { config, client })
    }
    
    /// Send a query to the LLM
    pub async fn query(&self, messages: Vec<Message>) -> Result<Message> {
        match self.config.llm.provider {
            LlmProvider::OpenAI => self.query_openai(messages).await,
            LlmProvider::Anthropic => self.query_anthropic(messages).await,
            LlmProvider::Local => self.query_local(messages).await,
            LlmProvider::Custom => self.query_custom(messages).await,
        }
    }
    
    /// Query OpenAI API
    async fn query_openai(&self, messages: Vec<Message>) -> Result<Message> {
        let url = self.config.llm.base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string());
        
        let request = LlmRequest {
            model: self.config.llm.model.clone(),
            messages,
            temperature: self.config.llm.temperature,
            max_tokens: self.config.llm.max_tokens,
        };
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.llm.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenAI")?;
        
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error: {}", error_text);
        }
        
        let llm_response: LlmResponse = response.json()
            .await
            .context("Failed to parse OpenAI response")?;
        
        if llm_response.choices.is_empty() {
            anyhow::bail!("No choices in OpenAI response");
        }
        
        Ok(llm_response.choices[0].message.clone())
    }
    
    /// Query Anthropic API
    async fn query_anthropic(&self, messages: Vec<Message>) -> Result<Message> {
        let url = self.config.llm.base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com/v1/messages".to_string());
        
        // Convert messages to Anthropic format
        let anthropic_messages: Vec<AnthropicMessage> = messages
            .into_iter()
            .map(|m| AnthropicMessage {
                role: m.role,
                content: vec![AnthropicContent::Text { text: m.content }],
            })
            .collect();
        
        let request = AnthropicRequest {
            model: self.config.llm.model.clone(),
            messages: anthropic_messages,
            max_tokens: self.config.llm.max_tokens,
            temperature: self.config.llm.temperature,
        };
        
        let response = self.client
            .post(&url)
            .header("x-api-key", &self.config.llm.api_key)
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
        
        let anthropic_response: AnthropicResponse = response.json()
            .await
            .context("Failed to parse Anthropic response")?;
        
        Ok(Message {
            role: "assistant".to_string(),
            content: anthropic_response.content
                .into_iter()
                .filter_map(|c| match c {
                    AnthropicContentResponse::Text { text } => Some(text),
                })
                .collect::<Vec<String>>()
                .join("\n"),
        })
    }
    
    /// Query local LLM (simplified)
    async fn query_local(&self, _messages: Vec<Message>) -> Result<Message> {
        // For local models, you'd connect to Ollama, LM Studio, etc.
        // This is a placeholder implementation
        
        anyhow::bail!("Local LLM not yet implemented");
    }
    
    /// Query custom LLM endpoint
    async fn query_custom(&self, messages: Vec<Message>) -> Result<Message> {
        let url = self.config.llm.base_url
            .clone()
            .context("Custom LLM requires base_url")?;
        
        // Assume OpenAI-compatible API
        let request = LlmRequest {
            model: self.config.llm.model.clone(),
            messages,
            temperature: self.config.llm.temperature,
            max_tokens: self.config.llm.max_tokens,
        };
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.llm.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to custom LLM")?;
        
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Custom LLM API error: {}", error_text);
        }
        
        let llm_response: LlmResponse = response.json()
            .await
            .context("Failed to parse custom LLM response")?;
        
        if llm_response.choices.is_empty() {
            anyhow::bail!("No choices in custom LLM response");
        }
        
        Ok(llm_response.choices[0].message.clone())
    }
}

/// Anthropic-specific types
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    temperature: f32,
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
struct AnthropicResponse {
    content: Vec<AnthropicContentResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentResponse {
    #[serde(rename = "text")]
    Text { text: String },
}

/// Helper functions for message creation
impl Message {
    /// Create system message
    pub fn system(content: &str) -> Self {
        Self {
            role: "system".to_string(),
            content: content.to_string(),
        }
    }
    
    /// Create user message
    pub fn user(content: &str) -> Self {
        Self {
            role: "user".to_string(),
            content: content.to_string(),
        }
    }
    
    /// Create assistant message
    pub fn assistant(content: &str) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.to_string(),
        }
    }
}