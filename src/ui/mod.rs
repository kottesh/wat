//! UI module - component-driven rendering system

pub mod diff;
pub mod editor;
pub mod fuzzy;
pub mod layout;
pub mod manager;

pub use diff::{CursorPos, DiffRenderer};
pub use editor::Editor;
pub use fuzzy::{FuzzyMatcher, FuzzySearch};
pub use layout::{Layout, Spacing};
pub use manager::{SharedRenderer, UIManager, next_component_id};
