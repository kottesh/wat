//! Layout management for component positioning

use std::collections::HashMap;
use crate::component::{ComponentId, Rect, Size};

/// A node in the layout tree
#[derive(Debug, Clone)]
pub struct LayoutNode {
    /// Component ID (if this is a leaf node)
    pub component_id: Option<ComponentId>,
    /// Children (if this is a container)
    pub children: Vec<LayoutNode>,
    /// Allocated rectangle
    pub rect: Rect,
    /// Whether this node is visible
    pub visible: bool,
}

impl LayoutNode {
    /// Create a new leaf node for a component
    pub fn leaf(component_id: ComponentId) -> Self {
        Self {
            component_id: Some(component_id),
            children: Vec::new(),
            rect: Rect::default(),
            visible: true,
        }
    }

    /// Create a new container node
    pub fn container() -> Self {
        Self {
            component_id: None,
            children: Vec::new(),
            rect: Rect::default(),
            visible: true,
        }
    }

    /// Add a child node
    pub fn add_child(&mut self, child: LayoutNode) {
        self.children.push(child);
    }

    /// Remove a child by component ID
    pub fn remove_child(&mut self, component_id: ComponentId) -> Option<LayoutNode> {
        if let Some(pos) = self.children.iter().position(|c| c.component_id == Some(component_id)) {
            Some(self.children.remove(pos))
        } else {
            // Recursively search in children
            for child in &mut self.children {
                if let Some(removed) = child.remove_child(component_id) {
                    return Some(removed);
                }
            }
            None
        }
    }
}

/// Layout manager that handles component positioning
#[derive(Debug)]
pub struct LayoutManager {
    /// Root node of the layout tree
    root: LayoutNode,
    /// Component ID to position mapping for quick lookup
    positions: HashMap<ComponentId, Rect>,
    /// Total available size
    available_size: Size,
    /// Next Y position for appending components (for vertical layout)
    next_y: u16,
}

impl LayoutManager {
    /// Create a new layout manager
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            root: LayoutNode::container(),
            positions: HashMap::new(),
            available_size: Size::new(width, height),
            next_y: 0,
        }
    }

    /// Update the available size
    pub fn set_size(&mut self, width: u16, height: u16) {
        self.available_size = Size::new(width, height);
        // Recalculate layout
        self.recalculate_layout();
    }

    /// Add a component at the end (for vertical scrolling layout)
    pub fn append_component(&mut self, component_id: ComponentId) {
        let node = LayoutNode::leaf(component_id);
        self.root.add_child(node);
        // Position will be calculated in recalculate_layout
    }

    /// Remove a component
    pub fn remove_component(&mut self, component_id: ComponentId) {
        self.root.remove_child(component_id);
        self.positions.remove(&component_id);
    }

    /// Get the position of a component
    pub fn get_position(&self, component_id: ComponentId) -> Option<Rect> {
        self.positions.get(&component_id).copied()
    }

    /// Get all component positions
    pub fn get_all_positions(&self) -> &HashMap<ComponentId, Rect> {
        &self.positions
    }

    /// Recalculate the entire layout
    pub fn recalculate_layout(&mut self) {
        self.positions.clear();
        // Note: We need component heights from outside
        // For now, just reset positions
    }

    /// Recalculate layout with known component heights
    pub fn recalculate_with_heights(&mut self, component_heights: &HashMap<ComponentId, u16>) {
        self.positions.clear();
        let mut current_y = 0u16;

        // Layout each child directly without recursion to avoid borrow issues
        for child in &mut self.root.children {
            if !child.visible {
                continue;
            }

            if let Some(component_id) = child.component_id {
                // Leaf node - get height from the provided map
                let height = component_heights.get(&component_id).copied().unwrap_or(1);
                child.rect = Rect::new(current_y, 0, self.available_size.width, height);
                self.positions.insert(component_id, child.rect);
                current_y += height;
            }
        }

        self.next_y = current_y;
    }

    /// Get the total height of the layout
    pub fn total_height(&self) -> u16 {
        self.next_y
    }

    /// Clear all components
    pub fn clear(&mut self) {
        self.root = LayoutNode::container();
        self.positions.clear();
        self.next_y = 0;
    }
}

impl Default for LayoutManager {
    fn default() -> Self {
        Self::new(80, 24)
    }
}
