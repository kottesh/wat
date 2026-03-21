//! Separator component - displays visual separators between sections

use crate::component::{Buffer, Color, Component, ComponentId, Modifiers};

/// State for the separator component
#[derive(Debug, Clone, PartialEq)]
pub struct SeparatorState {
    pub char: char,
    pub use_colors: bool,
}

/// Component that displays a horizontal separator line
#[derive(Debug)]
pub struct SeparatorComponent {
    id: ComponentId,
    state: SeparatorState,
}

impl SeparatorComponent {
    /// Create a new separator component
    pub fn new(id: ComponentId, use_colors: bool) -> Self {
        Self {
            id,
            state: SeparatorState {
                char: '─',
                use_colors,
            },
        }
    }
}

impl Component for SeparatorComponent {
    fn id(&self) -> ComponentId {
        self.id
    }

    fn render(&self, width: u16) -> Buffer {
        if width == 0 {
            return Buffer::empty();
        }

        let height = 1u16;
        let mut buffer = Buffer::new(width, height);

        let line: String = self.state.char.to_string().repeat(width as usize);

        let color = if self.state.use_colors {
            Color::Ansi(152) // Teal-ish
        } else {
            Color::Default
        };

        buffer.write_str(0, 0, &line, color, Color::Default, Modifiers::default());

        buffer
    }

    fn preferred_height(&self, _width: u16) -> u16 {
        1
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
