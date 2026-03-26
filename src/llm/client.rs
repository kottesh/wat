//! Unified LLM client that wraps provider implementations

use anyhow::{Context, Result};
use futures_util::Stream;
use std::pin::Pin;

use super::anthropic::AnthropicProvider;
use super::openai::OpenAiProvider;
use super::provider::{LlmProvider, ProviderCapabilities, StreamOptions};
use super::types::{Message, StreamChunk};
use crate::config::{ApiType, Config};
use crate::tools::ToolDefinition;

/// Unified LLM client
pub struct LlmClient {
    provider: Box<dyn LlmProvider>,
    provider_type: ProviderType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderType {
    OpenAi,
    Anthropic,
}

impl LlmClient {
    /// Create a new client from config
    pub fn new(config: Config) -> Result<Self> {
        let (provider, provider_type): (Box<dyn LlmProvider>, ProviderType) = match config.api_type {
            ApiType::OpenAiCompletions => {
                let openai = OpenAiProvider::new(
                    config.base_url,
                    config.api_key,
                    config.model_id,
                )?;
                (Box::new(openai), ProviderType::OpenAi)
            }
            ApiType::AnthropicMessages => {
                let anthropic = AnthropicProvider::new(
                    config.base_url,
                    config.api_key,
                    config.model_id,
                )?;
                (Box::new(anthropic), ProviderType::Anthropic)
            }
        };

        Ok(Self {
            provider,
            provider_type,
        })
    }

    /// Get provider type
    pub fn provider_type(&self) -> ProviderType {
        self.provider_type
    }

    /// Get provider capabilities
    pub fn capabilities(&self) -> &ProviderCapabilities {
        self.provider.capabilities()
    }

    /// Stream a response with optional tools
    pub async fn stream(
        &self,
        messages: Vec<Message>,
        tools: Option<&[ToolDefinition]>,
        options: StreamOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        self.provider.stream(messages, tools, options).await
    }

    /// Stream with default options
    pub async fn stream_default(
        &self,
        messages: Vec<Message>,
        tools: Option<&[ToolDefinition]>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        self.stream(messages, tools, StreamOptions::default()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelsConfig;

    #[test]
    fn test_provider_type_equality() {
        assert_eq!(ProviderType::OpenAi, ProviderType::OpenAi);
        assert_ne!(ProviderType::OpenAi, ProviderType::Anthropic);
    }

    #[test]
    fn test_client_creation_openai() {
        let config = Config {
            provider_name: "openai".to_string(),
            model_id: "gpt-4".to_string(),
            model_name: "GPT-4".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_type: ApiType::OpenAiCompletions,
            api_key: "test-key".to_string(),
        };

        let client = LlmClient::new(config).unwrap();
        assert_eq!(client.provider_type(), ProviderType::OpenAi);
    }

    #[test]
    fn test_client_creation_anthropic() {
        let config = Config {
            provider_name: "anthropic".to_string(),
            model_id: "claude-3-opus-20240229".to_string(),
            model_name: "Claude 3 Opus".to_string(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            api_type: ApiType::AnthropicMessages,
            api_key: "test-key".to_string(),
        };

        let client = LlmClient::new(config).unwrap();
        assert_eq!(client.provider_type(), ProviderType::Anthropic);
    }
}
