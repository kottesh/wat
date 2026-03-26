//! UI module - component-driven rendering system

pub mod fuzzy;
pub mod editor;
pub mod diff;

pub use fuzzy::{FuzzyMatcher, FuzzySearch};
pub use editor::Editor;
pub use diff::{DiffRenderer, CursorPos};
