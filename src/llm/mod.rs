//! LLM client and provider abstractions

pub mod anthropic;
pub mod client;
pub mod openai;
pub mod provider;
pub mod types;

// Public API - only what users need
pub use client::LlmClient;
pub use types::{FinishReason, Message, MessageContent, StreamChunk, ToolCall, ToolResult};

// Internal types - not exported
// - MessageRole: internal to message construction
// - LlmProvider, ProviderCapabilities, StreamOptions, TokenUsage: internal traits
// - OpenAiProvider, AnthropicProvider: internal implementations
// - ProviderType: internal to client
