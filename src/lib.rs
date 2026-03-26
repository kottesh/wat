//! WAT - Well Assisted Terminal
//!
//! An inline terminal assistant that appears at your command line.
//! Uses differential rendering for efficient updates.

pub mod component;
pub mod components;
// Old renderer removed - replaced by ui module
pub mod terminal;
pub mod agent;
pub mod config;
pub mod llm;
pub mod tools;
pub mod ui;

/// Re-exports for convenience
pub use agent::Agent;
pub use config::{Config, ModelsConfig};
pub use llm::Message;
// Backward compatibility export
pub use ui::UIManager as DifferentialRenderer;
