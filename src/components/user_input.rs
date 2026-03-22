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
    state: UserInputState,
}

impl UserInputComponent {
    /// Create a new user input component
    pub fn new(_id: ComponentId, content: String, use_colors: bool) -> Self {
        Self {
            state: UserInputState { content, use_colors },
        }
    }
}

impl Component for UserInputComponent {
    fn render(&self, width: u16) -> Buffer {
        if width == 0 {
            return Buffer::empty();
        }

        let input_width = width.saturating_sub(4) as usize; // "  " prefix + "  " suffix
        let mut visual_lines = Vec::new();

        for logical_line in self.state.content.lines() {
            if logical_line.is_empty() {
                visual_lines.push(String::new());
                continue;
            }

            let mut current_line = String::new();
            let mut current_width = 0;

            for c in logical_line.chars() {
                let cw = 1; // Simplified width calculation
                if current_width + cw > input_width {
                    visual_lines.push(current_line.clone());
                    current_line.clear();
                    current_width = 0;
                }
                current_line.push(c);
                current_width += cw;
            }
            if !current_line.is_empty() {
                visual_lines.push(current_line);
            }
        }

        // Height: top padding + content lines + bottom padding
        let height = (visual_lines.len() + 2) as u16;
        let mut buffer = Buffer::new(width, height);
        
        let bg_color = if self.state.use_colors {
            Color::Ansi(235) // Subtle dark grey background
        } else {
            Color::Default
        };

        // Top padding line - full width
        buffer.fill_row(0, bg_color);

        // Content lines with padding
        for (idx, line) in visual_lines.iter().enumerate() {
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

    fn preferred_height(&self, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }

        let mut lines = 0;
        for logical_line in self.state.content.lines() {
            if logical_line.is_empty() {
                lines += 1;
                continue;
            }
            
            let mut current_width = 0;
            let mut wrapped_lines = 1;
            
            for _ in logical_line.chars() {
                let cw = 1;
                if current_width + cw > (width.saturating_sub(4)) as usize {
                    wrapped_lines += 1;
                    current_width = 0;
                }
                current_width += cw;
            }
            lines += wrapped_lines;
        }

        (lines + 2) as u16
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
