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
    pub show_all: bool,
}

/// Component that displays a tool result
#[derive(Debug)]
pub struct ToolResultComponent {
    state: ToolResultState,
}

impl ToolResultComponent {
    /// Create a new tool result component
    pub fn new(
        _id: ComponentId,
        tool_name: String,
        output: String,
        duration_secs: Option<f64>,
        success: bool,
        command: Option<String>,
        use_colors: bool,
    ) -> Self {
        Self {
            state: ToolResultState {
                tool_name,
                output,
                duration_secs,
                success,
                command,
                use_colors,
                max_lines: 50,
                show_all: false,
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

    fn get_display_lines(&self) -> Vec<&str> {
        let all_lines: Vec<&str> = self.state.output.lines().collect();

        if all_lines.len() > self.state.max_lines && !self.state.show_all {
            if self.is_read_file() {
                all_lines[..self.state.max_lines].to_vec()
            } else {
                all_lines[all_lines.len() - self.state.max_lines..].to_vec()
            }
        } else {
            all_lines
        }
    }

    fn calculate_content_height(&self, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }

        let display_lines = self.get_display_lines();
        let mut content_rows = 0u16;

        let wrap_count = |text_len: usize, prefix: usize| -> u16 {
            let total_len = text_len + prefix;
            if total_len == 0 { return 1; }
            std::cmp::max(1, (total_len as u16 + width - 1) / width)
        };

        if self.is_bash() && self.state.use_colors {
            if let Some(ref cmd) = self.state.command {
                content_rows += wrap_count(cmd.chars().count(), 4);
            }
            for line in &display_lines {
                content_rows += wrap_count(line.chars().count(), 2);
            }
            if let Some(duration) = self.state.duration_secs {
                let timing_text = format!("Took {:.1}s", duration);
                content_rows += wrap_count(timing_text.chars().count(), 0);
            }
        } else if self.is_read_file() && self.state.use_colors {
            for line in &display_lines {
                content_rows += wrap_count(line.chars().count(), 2);
            }
            if let Some(duration) = self.state.duration_secs {
                content_rows += 1;
                let timing_text = format!("  Took {:.1}s", duration);
                content_rows += wrap_count(timing_text.chars().count(), 0);
            }
        } else {
            for line in &display_lines {
                content_rows += wrap_count(line.chars().count(), 0);
            }
            if let Some(duration) = self.state.duration_secs {
                content_rows += 1;
                let timing_text = format!(" Took {:.1}s", duration);
                content_rows += wrap_count(timing_text.chars().count(), 0);
            }
        }

        content_rows
    }
}

impl Component for ToolResultComponent {
    fn toggle_show_all(&mut self) -> bool {
        self.state.show_all = !self.state.show_all;
        true
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
                        g: 100,
                        b: 50,
                    } // green
                } else {
                    Color::Rgb {
                        r: 150,
                        g: 40,
                        b: 40,
                    } // red
                }
            } else if self.is_read_file() {
                Color::Ansi(235)
            } else {
                Color::Default
            }
        } else {
            Color::Default
        };

        let content_rows = self.calculate_content_height(width);
        let height = content_rows + 2;

        let mut buffer = Buffer::new(width, height as u16);
        let mut current_row = 0u16;

        if self.is_bash() && self.state.use_colors {
            // === Bash: top pad | $ command | output... | took Xs | bottom pad ===

            // Command line
            if let Some(ref cmd) = self.state.command {
                let cmd_text = format!("  $ {}", cmd);
                let text_len = cmd_text.chars().count() as u16;
                let rows = std::cmp::max(1, (text_len + width - 1) / width);
                for r in current_row..current_row + rows {
                    if r < height { buffer.fill_row(r, bg_color); }
                }
                buffer.write_str(
                    current_row,
                    0,
                    "  $ ",
                    Color::Default,
                    bg_color,
                    Modifiers::default(),
                );
                current_row = buffer.write_str(
                    current_row,
                    4,
                    cmd,
                    Color::Default,
                    bg_color,
                    Modifiers::bold(),
                );
            }

            // Output lines
            for line in &display_lines {
                let text = format!("  {}", line);
                let text_len = std::cmp::max(1, text.chars().count());
                let rows = std::cmp::max(1, (text_len as u16 + width - 1) / width);
                for r in current_row..current_row + rows {
                    if r < height { buffer.fill_row(r, bg_color); }
                }
                current_row = buffer.write_str(
                    current_row,
                    0,
                    &text,
                    Color::Default,
                    bg_color,
                    Modifiers::default(),
                );
            }

            // Timing
            if let Some(duration) = self.state.duration_secs {
                let timing_text = format!("Took {:.1}s", duration);
                let text_len = std::cmp::max(1, timing_text.chars().count());
                let rows = std::cmp::max(1, (text_len as u16 + width - 1) / width);
                for r in current_row..current_row + rows {
                    if r < height { buffer.fill_row(r, bg_color); }
                }
                current_row = buffer.write_str(
                    current_row,
                    0,
                    &timing_text,
                    Color::Default,
                    bg_color,
                    Modifiers::default(),
                );
            }

            // Bottom padding
            if current_row < height {
                buffer.fill_row(current_row, bg_color);
            }
        } else if self.is_read_file() && self.state.use_colors {
            // === Read file result rendering ===

            // Top padding
            buffer.fill_row(current_row, bg_color);
            current_row += 1;

            // Content lines
            for line in &display_lines {
                let text = format!("  {}", line);
                let text_len = std::cmp::max(1, text.chars().count());
                let rows = std::cmp::max(1, (text_len as u16 + width - 1) / width);
                for r in current_row..current_row + rows {
                    if r < height { buffer.fill_row(r, bg_color); }
                }
                current_row = buffer.write_str(
                    current_row,
                    0,
                    &text,
                    Color::Default,
                    bg_color,
                    Modifiers::default(),
                );
            }

            // Padding above timing
            if self.state.duration_secs.is_some() {
                if current_row < height { buffer.fill_row(current_row, bg_color); }
                current_row += 1;
            }

            // Timing (inside background)
            if let Some(duration) = self.state.duration_secs {
                let timing_text = format!("  Took {:.1}s", duration);
                let text_len = std::cmp::max(1, timing_text.chars().count());
                let rows = std::cmp::max(1, (text_len as u16 + width - 1) / width);
                for r in current_row..current_row + rows {
                    if r < height { buffer.fill_row(r, bg_color); }
                }
                current_row = buffer.write_str(
                    current_row,
                    0,
                    &timing_text,
                    Color::Default,
                    bg_color,
                    Modifiers::default(),
                );
            }

            // Bottom padding
            if current_row < height {
                buffer.fill_row(current_row, bg_color);
            }
        } else {
            // === Regular output (no colors or other tools) ===
            let fg = if self.state.use_colors {
                Color::Ansi(8) // Dim gray
            } else {
                Color::Default
            };

            for line in &display_lines {
                current_row = buffer.write_str(current_row, 0, line, fg, bg_color, Modifiers::default());
            }

            // Timing
            if let Some(duration) = self.state.duration_secs {
                current_row += 1; // Empty line
                let timing_text = format!(" Took {:.1}s", duration);
                if current_row < height {
                    current_row = buffer.write_str(
                        current_row,
                        0,
                        &timing_text,
                        fg,
                        Color::Default,
                        Modifiers::default(),
                    );
                }
            }

            // Bottom padding
            if current_row < height { buffer.fill_row(current_row, bg_color); }
        }

        buffer
    }

    fn preferred_height(&self, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }

        self.calculate_content_height(width) + 2
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
