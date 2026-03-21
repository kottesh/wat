//! Response component - displays agent responses

use crate::component::{Buffer, Color, Component, ComponentId, Modifiers};

/// State for the response component
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseState {
    pub content: String,
    pub use_colors: bool,
}

/// Component that displays agent responses
#[derive(Debug)]
pub struct ResponseComponent {
    id: ComponentId,
    state: ResponseState,
}

impl ResponseComponent {
    /// Create a new response component
    pub fn new(id: ComponentId, content: String, use_colors: bool) -> Self {
        Self {
            id,
            state: ResponseState { content, use_colors },
        }
    }

    /// Simple markdown parsing - identifies headers, code blocks, etc.
    fn parse_markdown(&self, width: u16) -> Vec<StyledLine> {
        let mut lines = Vec::new();
        let max_width = (width as usize).saturating_sub(4); // Account for padding

        let mut in_code_block = false;
        let mut code_content = String::new();

        for line in self.state.content.lines() {
            // Handle code blocks
            if line.trim().starts_with("```") {
                if in_code_block {
                    // End of code block - output accumulated code
                    for code_line in code_content.lines() {
                        lines.push(StyledLine {
                            text: format!("  {}", code_line),
                            fg: Color::Ansi(2), // Green for code
                            bg: Color::Default,
                            modifiers: Modifiers::default(),
                        });
                    }
                    code_content.clear();
                    in_code_block = false;
                } else {
                    in_code_block = true;
                }
                continue;
            }

            if in_code_block {
                code_content.push_str(line);
                code_content.push('\n');
                continue;
            }

            // Word wrap
            let wrapped = self.wrap_line(line, max_width);

            for wrapped_line in wrapped {
                let styled = if line.starts_with("# ") {
                    StyledLine {
                        text: format!("  {}", wrapped_line.trim_start_matches("# ")),
                        fg: Color::Yellow,
                        bg: Color::Default,
                        modifiers: Modifiers::bold(),
                    }
                } else if line.starts_with("## ") {
                    StyledLine {
                        text: format!("  {}", wrapped_line.trim_start_matches("## ")),
                        fg: Color::Yellow,
                        bg: Color::Default,
                        modifiers: Modifiers::default(),
                    }
                } else if wrapped_line.starts_with("- ") || wrapped_line.starts_with("* ") {
                    StyledLine {
                        text: format!("  {}", wrapped_line),
                        fg: Color::Default,
                        bg: Color::Default,
                        modifiers: Modifiers::default(),
                    }
                } else if wrapped_line.contains('`') && wrapped_line.matches('`').count() >= 2 {
                    // Inline code
                    StyledLine {
                        text: format!("  {}", wrapped_line),
                        fg: Color::Ansi(2),
                        bg: Color::Default,
                        modifiers: Modifiers::default(),
                    }
                } else {
                    StyledLine {
                        text: format!("  {}", wrapped_line),
                        fg: Color::Default,
                        bg: Color::Default,
                        modifiers: Modifiers::default(),
                    }
                };
                lines.push(styled);
            }
        }

        lines
    }

    /// Wrap a line to fit within the given width
    fn wrap_line(&self, line: &str, max_width: usize) -> Vec<String> {
        if line.len() <= max_width {
            return if line.is_empty() { vec![String::new()] } else { vec![line.to_string()] };
        }

        let mut wrapped = Vec::new();
        let mut current = String::new();

        for word in line.split_whitespace() {
            if current.len() + word.len() + 1 > max_width {
                if !current.is_empty() {
                    wrapped.push(current.trim().to_string());
                    current = String::new();
                }
                if word.len() > max_width {
                    wrapped.push(word.to_string());
                } else {
                    current = word.to_string();
                    current.push(' ');
                }
            } else {
                current.push_str(word);
                current.push(' ');
            }
        }

        if !current.trim().is_empty() {
            wrapped.push(current.trim().to_string());
        }

        if wrapped.is_empty() {
            vec![String::new()]
        } else {
            wrapped
        }
    }
}

/// A line with styling information
struct StyledLine {
    text: String,
    fg: Color,
    bg: Color,
    modifiers: Modifiers,
}

impl Component for ResponseComponent {
    fn id(&self) -> ComponentId {
        self.id
    }

    fn render(&self, width: u16) -> Buffer {
        if width == 0 || self.state.content.is_empty() {
            return Buffer::empty();
        }

        let styled_lines = self.parse_markdown(width);
        let height = styled_lines.len() as u16;

        let mut buffer = Buffer::new(width, height);

        for (idx, line) in styled_lines.iter().enumerate() {
            buffer.write_str(idx as u16, 0, &line.text, line.fg, line.bg, line.modifiers);
        }

        buffer
    }

    fn preferred_height(&self, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }
        let styled_lines = self.parse_markdown(width);
        styled_lines.len() as u16
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
