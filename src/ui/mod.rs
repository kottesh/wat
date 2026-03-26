//! UI module - component-driven rendering system

pub mod fuzzy;
pub mod editor;

pub use fuzzy::{FuzzyMatcher, FuzzySearch};
pub use editor::Editor;
