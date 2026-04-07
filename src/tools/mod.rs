//! Tool system for executing commands and reading files

pub mod bash;
pub mod definition;
pub mod executor;
pub mod read_file;
pub mod registry;

// Public API - only what users need
pub use definition::{ParameterSchema, ToolDefinition};
pub use executor::{ExecutionResult, ToolUpdate};
pub use registry::ToolRegistry;

// Internal types - not exported
// - PropertySchema: internal to schema building
// - ToolExecutor: internal trait
// - BashTool, ReadFileTool: internal implementations, accessed via registry
