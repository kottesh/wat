//! User input component - displays user messages with background styling

use crate::component::{Buffer, Color, Component, ComponentId, Modifiers};

/// State for the user input component
#[derive(Debug, Clone, PartialEq)]
pub struct UserInputState {
    pub content: String,
    pub use_colors: bool,
}

/// Component that displays user input with a subtle background
#[derive(Debug)]
pub struct UserInputComponent {
    id: ComponentId,
    state: UserInputState,
}

impl UserInputComponent {
    /// Create a new user input component
    pub fn new(id: ComponentId, content: String, use_colors: bool) -> Self {
        Self {
            id,
            state: UserInputState { content, use_colors },
        }
    }
}

impl Component for UserInputComponent {
    fn id(&self) -> ComponentId {
        self.id
    }

    fn render(&self, width: u16) -> Buffer {
        if width == 0 {
            return Buffer::empty();
        }

        let lines: Vec<&str> = self.state.content.lines().collect();
        // Height: top padding + content lines + bottom padding
        let height = (lines.len() + 2) as u16;

        let mut buffer = Buffer::new(width, height);
        let bg_color = if self.state.use_colors {
            Color::Ansi(235) // Subtle dark grey background
        } else {
            Color::Default
        };

        // Top padding line - full width
        buffer.fill_row(0, bg_color);

        // Content lines with padding
        for (idx, line) in lines.iter().enumerate() {
            let row = (idx + 1) as u16;
            buffer.fill_row(row, bg_color);

            // Format: "  {content}  " with padding to fill width
            let content_text = format!("  {}  ", line);
            buffer.write_str(row, 0, &content_text, Color::Default, bg_color, Modifiers::default());
        }

        // Bottom padding line
        buffer.fill_row(height - 1, bg_color);

        buffer
    }

    fn preferred_height(&self, _width: u16) -> u16 {
        let lines = self.state.content.lines().count();
        (lines + 2) as u16
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
