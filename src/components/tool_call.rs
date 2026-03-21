//! Tool call component - displays tool invocations

use crate::component::{Buffer, Color, Component, ComponentId, Modifiers};

/// State for the tool call component
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallState {
    pub tool_name: String,
    pub args: String,
    pub use_colors: bool,
}

/// Component that displays a tool call
#[derive(Debug)]
pub struct ToolCallComponent {
    id: ComponentId,
    state: ToolCallState,
}

impl ToolCallComponent {
    /// Create a new tool call component
    pub fn new(id: ComponentId, tool_name: String, args: String, use_colors: bool) -> Self {
        Self {
            id,
            state: ToolCallState { tool_name, args, use_colors },
        }
    }

    /// Check if this is a read_file tool
    pub fn is_read_file(&self) -> bool {
        self.state.tool_name == "read_file"
    }
}

impl Component for ToolCallComponent {
    fn id(&self) -> ComponentId {
        self.id
    }

    fn render(&self, width: u16) -> Buffer {
        if width == 0 {
            return Buffer::empty();
        }

        // Only render for read_file - bash is rendered with result
        if !self.is_read_file() {
            return Buffer::empty();
        }

        // 2 lines: empty top padding + "Read filename" line
        let height = 2u16;
        let mut buffer = Buffer::new(width, height);

        let bg_color = if self.state.use_colors {
            Color::Ansi(235)
        } else {
            Color::Default
        };

        // Top padding line
        buffer.fill_row(0, bg_color);

        // "Read" in bold, then filename
        buffer.fill_row(1, bg_color);
        buffer.write_str(1, 0, "  ", Color::Default, bg_color, Modifiers::default());
        buffer.write_str(1, 2, "Read", Color::Default, bg_color, Modifiers::bold());
        buffer.write_str(1, 6, &format!(" {}", self.state.args), Color::Default, bg_color, Modifiers::default());

        buffer
    }

    fn preferred_height(&self, width: u16) -> u16 {
        if width == 0 || !self.is_read_file() {
            return 0;
        }
        2
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
