//! LLM client and provider abstractions

pub mod types;
pub mod provider;
pub mod openai;
pub mod anthropic;
pub mod client;

pub use types::{
    Message, MessageRole, MessageContent,
    ToolCall, ToolResult,
    StreamChunk, FinishReason,
};
pub use provider::{LlmProvider, ProviderCapabilities, StreamOptions, TokenUsage};
pub use openai::OpenAiProvider;
pub use anthropic::AnthropicProvider;
pub use client::{LlmClient, ProviderType};
