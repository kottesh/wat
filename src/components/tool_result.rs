//! Tool result component - displays tool execution results

use crate::component::{Buffer, Color, Component, ComponentId, Modifiers};

/// State for the tool result component
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResultState {
    pub tool_name: String,
    pub output: String,
    pub duration_secs: Option<f64>,
    pub success: bool,
    pub command: Option<String>,
    pub use_colors: bool,
    pub max_lines: usize,
}

/// Component that displays a tool result
#[derive(Debug)]
pub struct ToolResultComponent {
    id: ComponentId,
    state: ToolResultState,
}

impl ToolResultComponent {
    /// Create a new tool result component
    pub fn new(
        id: ComponentId,
        tool_name: String,
        output: String,
        duration_secs: Option<f64>,
        success: bool,
        command: Option<String>,
        use_colors: bool,
    ) -> Self {
        Self {
            id,
            state: ToolResultState {
                tool_name,
                output,
                duration_secs,
                success,
                command,
                use_colors,
                max_lines: 50,
            },
        }
    }

    /// Check if this is a bash result
    pub fn is_bash(&self) -> bool {
        self.state.tool_name == "bash"
    }

    /// Check if this is a read_file result
    pub fn is_read_file(&self) -> bool {
        self.state.tool_name == "read_file"
    }

    /// Get the lines to display (with truncation)
    fn get_display_lines(&self) -> Vec<&str> {
        let all_lines: Vec<&str> = self.state.output.lines().collect();

        if all_lines.len() > self.state.max_lines {
            all_lines[all_lines.len() - self.state.max_lines..].to_vec()
        } else {
            all_lines
        }
    }
}

impl Component for ToolResultComponent {
    fn id(&self) -> ComponentId {
        self.id
    }

    fn render(&self, width: u16) -> Buffer {
        if width == 0 {
            return Buffer::empty();
        }

        let display_lines = self.get_display_lines();

        // Determine background color based on tool type
        let bg_color = if self.state.use_colors {
            if self.is_bash() {
                if self.state.success {
                    Color::Rgb {
                        r: 30,
                        g: 38,
                        b: 30,
                    } // Dark green tint
                } else {
                    Color::Rgb {
                        r: 60,
                        g: 30,
                        b: 30,
                    } // Dark red tint
                }
            } else if self.is_read_file() {
                Color::Ansi(235)
            } else {
                Color::Default
            }
        } else {
            Color::Default
        };

        // Calculate height: top pad + content rows + bottom pad
        let mut content_rows = 0u16;

        if self.is_bash() {
            if self.state.command.is_some() {
                content_rows += 1; // command
            }
            content_rows += display_lines.len() as u16; // output
            content_rows += 1; // took Xs
        } else if self.is_read_file() {
            content_rows += 1; // empty line after header
            content_rows += display_lines.len() as u16;
            if self.state.duration_secs.is_some() {
                content_rows += 1;
            }
        } else {
            content_rows += display_lines.len() as u16;
            if self.state.duration_secs.is_some() {
                content_rows += 1;
            }
        }

        let height = std::cmp::max(content_rows + 2, 1); // +2 for top + bottom padding, min 1

        let mut buffer = Buffer::new(width, height as u16);
        let mut current_row = 0u16;

        // Top padding
        buffer.fill_row(current_row, bg_color);
        current_row += 1;

        if self.is_bash() && self.state.use_colors {
            // === Bash: top pad | $ command | output... | took Xs | bottom pad ===

            // Command line
            if let Some(ref cmd) = self.state.command {
                buffer.fill_row(current_row, bg_color);
                buffer.write_str(
                    current_row,
                    0,
                    "  $ ",
                    Color::Default,
                    bg_color,
                    Modifiers::default(),
                );
                buffer.write_str(
                    current_row,
                    4,
                    cmd,
                    Color::Default,
                    bg_color,
                    Modifiers::bold(),
                );
                current_row += 1;
            }

            // Output lines
            for line in &display_lines {
                buffer.fill_row(current_row, bg_color);
                let text = format!("  {}", line);
                buffer.write_str(
                    current_row,
                    0,
                    &text,
                    Color::Default,
                    bg_color,
                    Modifiers::default(),
                );
                current_row += 1;
            }

            // Timing
            if let Some(duration) = self.state.duration_secs {
                buffer.fill_row(current_row, bg_color);
                let timing_text = format!("Took {:.1}s", duration);
                buffer.write_str(
                    current_row,
                    0,
                    &timing_text,
                    Color::Default,
                    bg_color,
                    Modifiers::default(),
                );
                current_row += 1;
            }

            // Bottom padding
            if current_row < height as u16 {
                buffer.fill_row(current_row, bg_color);
            }
        } else if self.is_read_file() && self.state.use_colors {
            // === Read file result rendering ===

            // Empty line after header (from ToolCallComponent)
            buffer.fill_row(current_row, bg_color);
            current_row += 1;

            // Content lines
            for line in &display_lines {
                buffer.fill_row(current_row, bg_color);
                let text = format!("  {}", line);
                buffer.write_str(
                    current_row,
                    0,
                    &text,
                    Color::Default,
                    bg_color,
                    Modifiers::default(),
                );
                current_row += 1;
            }

            // Bottom padding
            buffer.fill_row(current_row, bg_color);
            current_row += 1;

            // Timing (outside background, dimmed)
            if let Some(duration) = self.state.duration_secs {
                let timing_text = format!(" Took {:.1}s", duration);
                if current_row < height as u16 {
                    buffer.write_str(
                        current_row,
                        0,
                        &timing_text,
                        Color::Ansi(8),
                        Color::Default,
                        Modifiers::dim(),
                    );
                }
            }
        } else {
            // === Regular output (no colors or other tools) ===
            let fg = if self.state.use_colors {
                Color::Ansi(8) // Dim gray
            } else {
                Color::Default
            };

            for line in &display_lines {
                buffer.write_str(current_row, 0, line, fg, bg_color, Modifiers::default());
                current_row += 1;
            }

            // Timing
            if let Some(duration) = self.state.duration_secs {
                current_row += 1; // Empty line
                let timing_text = format!(" Took {:.1}s", duration);
                if current_row < height as u16 {
                    buffer.write_str(
                        current_row,
                        0,
                        &timing_text,
                        fg,
                        Color::Default,
                        Modifiers::default(),
                    );
                    current_row += 1;
                }
            }

            // Bottom padding
            buffer.fill_row(current_row, bg_color);
        }

        buffer
    }

    fn preferred_height(&self, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }

        let display_lines = self.get_display_lines();
        let mut content_rows = 0u16;

        if self.is_bash() {
            if self.state.command.is_some() {
                content_rows += 1;
            }
            content_rows += display_lines.len() as u16;
            content_rows += 1; // took Xs
        } else if self.is_read_file() {
            content_rows += 1;
            content_rows += display_lines.len() as u16;
            if self.state.duration_secs.is_some() {
                content_rows += 1;
            }
        } else {
            content_rows += display_lines.len() as u16;
            if self.state.duration_secs.is_some() {
                content_rows += 1;
            }
        }

        content_rows + 2 // top + bottom padding
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
