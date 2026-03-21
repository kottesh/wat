//! WAT - Well Assisted Terminal
//!
//! An inline terminal assistant that appears at your command line.
//! Uses differential rendering for efficient updates.

pub mod component;
pub mod components;
pub mod layout;
pub mod renderer;
pub mod terminal;
pub mod agent;
pub mod config;
pub mod llm;
pub mod tools;

/// Re-exports for convenience
pub use agent::{Agent, SimpleAgent};
pub use config::Config;
pub use llm::Message;
pub use renderer::DifferentialRenderer;
