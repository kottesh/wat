//! Prompt component - displays the input prompt for user interaction

use crate::component::{Buffer, Color, Component, ComponentId, Modifiers};

/// State for the prompt component
#[derive(Debug, Clone, PartialEq)]
pub struct PromptState {
    pub current_input: String,
    pub cursor_position: usize,
    pub use_colors: bool,
}

/// Component that displays the input prompt
#[derive(Debug)]
pub struct PromptComponent {
    id: ComponentId,
    state: PromptState,
}

impl PromptComponent {
    /// Create a new prompt component
    pub fn new(id: ComponentId, use_colors: bool) -> Self {
        Self {
            id,
            state: PromptState {
                current_input: String::new(),
                cursor_position: 0,
                use_colors,
            },
        }
    }

    /// Set the current input
    pub fn set_input(&mut self, input: String) {
        self.state.cursor_position = input.len();
        self.state.current_input = input;
    }

    /// Get the current input
    pub fn input(&self) -> &str {
        &self.state.current_input
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, c: char) {
        self.state.current_input.insert(self.state.cursor_position, c);
        self.state.cursor_position += 1;
    }

    /// Delete the character before the cursor
    pub fn backspace(&mut self) {
        if self.state.cursor_position > 0 {
            self.state.current_input.remove(self.state.cursor_position - 1);
            self.state.cursor_position -= 1;
        }
    }

    /// Clear the input
    pub fn clear(&mut self) {
        self.state.current_input.clear();
        self.state.cursor_position = 0;
    }
}

impl Component for PromptComponent {
    fn id(&self) -> ComponentId {
        self.id
    }

    fn render(&self, width: u16) -> Buffer {
        if width == 0 {
            return Buffer::empty();
        }

        // Height is 3: top border, input line, bottom border
        let height = 3u16;
        let mut buffer = Buffer::new(width, height);

        let border_color = if self.state.use_colors {
            Color::Ansi(152) // Teal-ish color
        } else {
            Color::Default
        };

        // Top border - teal line
        let top_border = "─".repeat(width as usize);
        buffer.write_str(0, 0, &top_border, border_color, Color::Default, Modifiers::default());

        // Input line (cursor position shown)
        buffer.write_str(1, 0, &self.state.current_input, Color::Default, Color::Default, Modifiers::default());

        // Bottom border - teal line
        buffer.write_str(2, 0, &top_border, border_color, Color::Default, Modifiers::default());

        buffer
    }

    fn preferred_height(&self, _width: u16) -> u16 {
        3
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
