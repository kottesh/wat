//! Thinking component - displays agent thinking/reasoning blocks

use crate::component::{Buffer, Color, Component, ComponentId, Modifiers};

/// State for the thinking component
#[derive(Debug, Clone, PartialEq)]
pub struct ThinkingState {
    pub content: String,
    pub use_colors: bool,
}

/// Component that displays thinking/reasoning blocks
#[derive(Debug)]
pub struct ThinkingComponent {
    id: ComponentId,
    state: ThinkingState,
}

impl ThinkingComponent {
    /// Create a new thinking component
    pub fn new(id: ComponentId, content: String, use_colors: bool) -> Self {
        Self {
            id,
            state: ThinkingState { content, use_colors },
        }
    }
}

impl Component for ThinkingComponent {
    fn id(&self) -> ComponentId {
        self.id
    }

    fn render(&self, width: u16) -> Buffer {
        if width == 0 || self.state.content.is_empty() {
            return Buffer::empty();
        }

        let lines: Vec<&str> = self.state.content.lines().collect();
        let height = lines.len() as u16;

        if height == 0 {
            return Buffer::empty();
        }

        let mut buffer = Buffer::new(width, height);

        let bg_color = if self.state.use_colors {
            Color::Ansi(238) // Slightly lighter grey for thinking
        } else {
            Color::Default
        };

        for (idx, line) in lines.iter().enumerate() {
            buffer.fill_row(idx as u16, bg_color);
            buffer.write_str(idx as u16, 0, line, Color::Default, bg_color, Modifiers::dim());
        }

        buffer
    }

    fn preferred_height(&self, _width: u16) -> u16 {
        self.state.content.lines().count() as u16
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
