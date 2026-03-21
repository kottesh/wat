//! WAT - Well Assisted Terminal
//! 
//! An inline terminal assistant that appears at your command line.

pub mod terminal;
pub mod render;
pub mod hotkey;
pub mod agent;
pub mod config;
pub mod llm;
pub mod tools;

/// Re-exports for convenience
pub use agent::{Agent, SimpleAgent};
pub use config::Config;
pub use llm::Message;