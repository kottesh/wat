//! Spinner component - displays an animated loading indicator

use crate::component::{Buffer, Color, Component, ComponentId, Modifiers};

/// Braille spinner frames
const BRAILLE_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// State for the spinner component
#[derive(Debug, Clone, PartialEq)]
pub struct SpinnerState {
    pub message: String,
    pub frame_index: usize,
    pub use_colors: bool,
}

/// Component that displays an animated spinner
#[derive(Debug)]
pub struct SpinnerComponent {
    id: ComponentId,
    state: SpinnerState,
}

impl SpinnerComponent {
    /// Create a new spinner component
    pub fn new(id: ComponentId, message: String, use_colors: bool) -> Self {
        Self {
            id,
            state: SpinnerState {
                message,
                frame_index: 0,
                use_colors,
            },
        }
    }

    /// Advance the spinner to the next frame
    pub fn tick(&mut self) {
        self.state.frame_index = (self.state.frame_index + 1) % BRAILLE_FRAMES.len();
    }

    /// Get the current frame character
    fn current_frame(&self) -> &'static str {
        BRAILLE_FRAMES[self.state.frame_index % BRAILLE_FRAMES.len()]
    }
}

impl Component for SpinnerComponent {
    fn id(&self) -> ComponentId {
        self.id
    }

    fn render(&self, width: u16) -> Buffer {
        if width == 0 {
            return Buffer::empty();
        }

        let height = 1u16;
        let mut buffer = Buffer::new(width, height);

        let frame = self.current_frame();

        let fg = if self.state.use_colors {
            Color::BrightCyan
        } else {
            Color::Default
        };

        let msg_fg = if self.state.use_colors {
            Color::Ansi(8) // Dim
        } else {
            Color::Default
        };

        // Render spinner frame with cyan color
        buffer.write_str(0, 0, frame, fg, Color::Default, Modifiers::default());
        // Render message dimmed
        buffer.write_str(0, 2, &self.state.message, msg_fg, Color::Default, Modifiers::dim());

        buffer
    }

    fn preferred_height(&self, _width: u16) -> u16 {
        1
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
