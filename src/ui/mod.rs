//! UI module - component-driven rendering system

pub mod fuzzy;
pub mod editor;
pub mod diff;
pub mod layout;
pub mod manager;

pub use fuzzy::{FuzzyMatcher, FuzzySearch};
pub use editor::Editor;
pub use diff::{DiffRenderer, CursorPos};
pub use layout::{Layout, Spacing};
pub use manager::{UIManager, SharedRenderer, next_component_id};
