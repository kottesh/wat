//! LLM Provider trait and capabilities

use anyhow::Result;
use async_trait::async_trait;
use futures_util::Stream;
use std::pin::Pin;

use super::types::{Message, StreamChunk};
use crate::tools::ToolDefinition;

/// Provider capabilities
#[derive(Debug, Clone)]
pub struct ProviderCapabilities {
    /// Whether native tool calling is supported (always true for our providers)
    #[allow(dead_code)] // Part of capabilities API
    pub native_tools: bool,
    /// Can make multiple tool calls in parallel
    #[allow(dead_code)] // Future use
    pub parallel_tool_calls: bool,
    /// Streams tool call arguments incrementally
    #[allow(dead_code)] // Future use
    pub tool_streaming: bool,
    /// Supports vision/image inputs
    #[allow(dead_code)] // Future use for vision support
    pub vision: bool,
    /// Maximum tools that can be called in one response
    #[allow(dead_code)] // Future use
    pub max_tools_per_call: usize,
}

/// Streaming options
#[derive(Debug, Clone)]
pub struct StreamOptions {
    pub temperature: f32,
    pub max_tokens: u32,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            temperature: 0.3,
            max_tokens: 2000,
        }
    }
}

/// Token usage information
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // Future use for usage tracking
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// LLM Provider trait - implemented by OpenAI, Anthropic, etc.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider name for identification
    #[allow(dead_code)] // Public API for provider identification
    fn name(&self) -> &str;

    /// Provider capabilities
    #[allow(dead_code)] // Public API for capability detection
    fn capabilities(&self) -> &ProviderCapabilities;

    /// Stream a response with optional tool definitions
    async fn stream(
        &self,
        messages: Vec<Message>,
        tools: Option<&[ToolDefinition]>,
        options: StreamOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_options_default() {
        let opts = StreamOptions::default();
        assert_eq!(opts.temperature, 0.3);
        assert_eq!(opts.max_tokens, 2000);
    }

    #[test]
    fn test_provider_capabilities() {
        let caps = ProviderCapabilities {
            native_tools: true,
            parallel_tool_calls: true,
            tool_streaming: true,
            vision: false,
            max_tools_per_call: 16,
        };

        assert!(caps.native_tools);
        assert!(caps.parallel_tool_calls);
        assert!(caps.tool_streaming);
        assert!(!caps.vision);
        assert_eq!(caps.max_tools_per_call, 16);
    }

    #[test]
    fn test_token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
    }
}
