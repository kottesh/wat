//! LLM client and provider abstractions

pub mod types;
pub mod provider;
pub mod openai;
pub mod anthropic;
pub mod client;

// Public API - only what users need
pub use types::{
    Message, MessageContent,
    ToolCall, ToolResult,
    StreamChunk, FinishReason,
};
pub use client::LlmClient;

// Internal types - not exported
// - MessageRole: internal to message construction
// - LlmProvider, ProviderCapabilities, StreamOptions, TokenUsage: internal traits
// - OpenAiProvider, AnthropicProvider: internal implementations
// - ProviderType: internal to client
