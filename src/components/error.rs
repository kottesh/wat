//! Error component - displays error messages

use crate::component::{Buffer, Color, Component, ComponentId, Modifiers};

/// State for the error component
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorState {
    pub message: String,
    pub use_colors: bool,
}

/// Component that displays an error message
#[derive(Debug)]
pub struct ErrorComponent {
    id: ComponentId,
    state: ErrorState,
}

impl ErrorComponent {
    /// Create a new error component
    pub fn new(id: ComponentId, message: String, use_colors: bool) -> Self {
        Self {
            id,
            state: ErrorState { message, use_colors },
        }
    }
}

impl Component for ErrorComponent {
    fn id(&self) -> ComponentId {
        self.id
    }

    fn render(&self, width: u16) -> Buffer {
        if width == 0 {
            return Buffer::empty();
        }

        let lines: Vec<&str> = self.state.message.lines().collect();
        let height = lines.len().max(1) as u16;

        let mut buffer = Buffer::new(width, height);

        let fg = if self.state.use_colors {
            Color::BrightRed
        } else {
            Color::Default
        };

        for (idx, line) in lines.iter().enumerate() {
            if idx == 0 {
                // First line: "error: <message>"
                buffer.write_str(idx as u16, 0, "error: ", fg, Color::Default, Modifiers::bold());
                buffer.write_str(idx as u16, 7, line, fg, Color::Default, Modifiers::default());
            } else {
                // Subsequent lines
                buffer.write_str(idx as u16, 0, line, fg, Color::Default, Modifiers::default());
            }
        }

        buffer
    }

    fn preferred_height(&self, _width: u16) -> u16 {
        self.state.message.lines().count().max(1) as u16
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
