//! Layout management for component positioning

use crate::component::{ComponentId, Size};

/// Layout manager that handles component positioning
#[derive(Debug)]
pub struct LayoutManager {
    /// Children appended in order (vertical layout)
    children: Vec<ComponentId>,
    /// Total available size
    available_size: Size,
}

impl LayoutManager {
    /// Create a new layout manager
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            children: Vec::new(),
            available_size: Size::new(width, height),
        }
    }

    /// Update the available size
    pub fn set_size(&mut self, width: u16, height: u16) {
        self.available_size = Size::new(width, height);
    }

    /// Add a component at the end (for vertical scrolling layout)
    pub fn append_component(&mut self, component_id: ComponentId) {
        self.children.push(component_id);
    }
}

impl Default for LayoutManager {
    fn default() -> Self {
        Self::new(80, 24)
    }
}
