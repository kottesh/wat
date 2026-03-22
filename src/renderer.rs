//! Inline rendering system for terminal output
//!
//! Input box is always pinned at the bottom. Every render call moves the
//! cursor above the input box, prints its content, then redraws the input
//! box below. This keeps the input box permanently visible.
//!
//! Layout (from top of content downward):
//!   ... scrollback content ...
//!   [blank gap]        ← INPUT_BOX_LINES = 4
//!   [top border]
//!   [input line]       ← user types here
//!   [bottom border]
//!   (cursor here after every draw)

use std::collections::HashMap;
use std::io::{self, Write};

use crate::component::{format_cell_style, Buffer, Component, ComponentId, Size};
use crate::components::{
    ErrorComponent, ResponseComponent, ToolCallComponent, ToolResultComponent, UserInputComponent,
};
use crate::layout::LayoutManager;

/// Number of lines the input box occupies.
/// blank-gap + top-border + input-line + bottom-border = 4
const INPUT_BOX_LINES: u16 = 4;

static COMPONENT_ID_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

pub fn next_component_id() -> ComponentId {
    ComponentId(COMPONENT_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
}

struct ComponentEntry {
    component: Box<dyn Component>,
}

pub struct DifferentialRenderer {
    components: HashMap<ComponentId, ComponentEntry>,
    layout: LayoutManager,
    terminal_size: Size,
    use_colors: bool,
}

impl std::fmt::Debug for DifferentialRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DifferentialRenderer")
            .field("component_count", &self.components.len())
            .field("terminal_size", &self.terminal_size)
            .field("use_colors", &self.use_colors)
            .finish()
    }
}

impl DifferentialRenderer {
    pub fn new(use_colors: bool) -> Self {
        let terminal_size = crossterm::terminal::size()
            .map(|(w, h)| Size::new(w, h))
            .unwrap_or_else(|_| Size::new(80, 24));

        Self {
            components: HashMap::new(),
            layout: LayoutManager::new(terminal_size.width, terminal_size.height),
            terminal_size,
            use_colors,
        }
    }

    pub fn update_size(&mut self) {
        if let Ok((w, h)) = crossterm::terminal::size() {
            self.terminal_size = Size::new(w, h);
            self.layout.set_size(w, h);
        }
    }

    // ── Input box ───────────────────────────────────────────────────────────

    /// Draw the 4-line input box at the current cursor position.
    /// After this call the cursor is at the line *after* the bottom border.
    pub fn draw_input_box(&self) {
        let width = self.terminal_size.width as usize;
        let border_str = "─".repeat(width.saturating_sub(1));
        let border = if self.use_colors {
            format!("\x1b[38;5;152m{}\x1b[0m", border_str)
        } else {
            border_str
        };
        // blank gap
        print!("\r\n");
        // top border
        print!("{}\r\n", border);
        // input line (empty initially)
        print!("\r\n");
        // bottom border
        print!("{}\r\n", border);
        let _ = io::stdout().flush();
    }

    /// Move cursor to the start of the blank-gap line (4 lines above cursor).
    fn move_above_input_box(&self) {
        print!("\x1b[{}F", INPUT_BOX_LINES);
    }

    /// Write `text` onto the input line of the input box.
    /// Cursor must be after the bottom border when called.
    /// After the call cursor is restored to after the bottom border.
    pub fn update_input_line(&self, text: &str) {
        // up 2 → input line (row: blank=0, top=1, input=2, bottom=3, cursor after=4;
        // from cursor@4, up 2 → input line@2)
        print!("\x1b[2F\r{}\x1b[K\x1b[2B\r", text);
        let _ = io::stdout().flush();
    }

    /// Clear the input line.
    pub fn clear_input_line(&self) {
        print!("\x1b[2F\r\x1b[K\x1b[2B\r");
        let _ = io::stdout().flush();
    }

    // ── Component add helpers ───────────────────────────────────────────────

    pub fn add_user_input(&mut self, content: String) -> ComponentId {
        let id = next_component_id();
        let component = UserInputComponent::new(id, content, self.use_colors);
        let id = self.add_component(Box::new(component));
        self.render_component(id);
        id
    }

    pub fn add_response(&mut self, content: String) -> ComponentId {
        let id = next_component_id();
        let component = ResponseComponent::new(id, content, self.use_colors);
        let id = self.add_component(Box::new(component));
        self.render_component(id);
        id
    }

    pub fn add_tool_call(&mut self, tool_name: String, args: String) -> ComponentId {
        let id = next_component_id();
        let component = ToolCallComponent::new(id, tool_name, args, self.use_colors);
        let id = self.add_component(Box::new(component));
        self.render_component(id);
        id
    }

    pub fn add_tool_result(
        &mut self,
        tool_name: String,
        output: String,
        duration_secs: Option<f64>,
        success: bool,
        command: Option<String>,
    ) -> ComponentId {
        let id = next_component_id();
        let component = ToolResultComponent::new(
            id,
            tool_name,
            output,
            duration_secs,
            success,
            command,
            self.use_colors,
        );
        let id = self.add_component(Box::new(component));
        self.render_component(id);
        id
    }

    pub fn add_error(&mut self, message: String) -> ComponentId {
        let id = next_component_id();
        let component = ErrorComponent::new(id, message, self.use_colors);
        let id = self.add_component(Box::new(component));
        self.render_component(id);
        id
    }

    // ── Bash streaming ──────────────────────────────────────────────────────

    fn bash_running_bg() -> &'static str {
        "\x1b[48;2;39;39;39m" // #272727 — grey while running
    }

    /// Print the bash block header (top-pad + bold command line + gap = 3 lines)
    /// above the input box, then redraw the input box.
    pub fn print_bash_header(&self, command: &str) {
        self.move_above_input_box();
        if self.use_colors {
            let width = self.terminal_size.width as usize;
            let bg = Self::bash_running_bg();
            let reset = "\x1b[0m";
            let bold = "\x1b[1m";
            let empty = " ".repeat(width);
            print!("{}{}{}\r\n", bg, empty, reset);
            let content = format!("  $ {}", command);
            let padding = " ".repeat(width.saturating_sub(content.len()));
            print!("{}{}{}{}{}{}\r\n", bg, bold, content, padding, bold, reset);
            print!("{}{}{}\r\n", bg, empty, reset);
        } else {
            print!("\r\n  $ {}\r\n\r\n", command);
        }
        self.draw_input_box();
        let _ = io::stdout().flush();
    }

    /// Print a single streamed output line above the input box, then redraw it.
    pub fn print_output_line(&self, line: &str) {
        self.move_above_input_box();
        if self.use_colors {
            let width = self.terminal_size.width as usize;
            let bg = Self::bash_running_bg();
            let reset = "\x1b[0m";
            let content = format!("  {}", line);
            let padding = " ".repeat(width.saturating_sub(content.len()));
            print!("{}{}{}{}\r\n", bg, content, padding, reset);
        } else {
            print!("  {}\r\n", line);
        }
        self.draw_input_box();
        let _ = io::stdout().flush();
    }

    /// Repaint the entire bash block in the final success/failure colour.
    ///
    /// Cursor layout when called (after N output lines):
    ///   [bash top-pad]          ← 3 + N + 4 lines above cursor
    ///   [$ command]
    ///   [gap]
    ///   [output 0 … N-1]
    ///   [input blank]           ← 4 lines above cursor
    ///   [input top border]
    ///   [input line]
    ///   [input bottom border]
    ///   cursor ← here
    pub fn finalize_bash_block(
        &self,
        command: &str,
        output_lines: &[String],
        duration_secs: f64,
        success: bool,
        cancelled: bool,
    ) {
        // Move to the very start of the bash block
        let lines_up = output_lines.len() + 3 + INPUT_BOX_LINES as usize;
        print!("\x1b[{}F", lines_up);

        if self.use_colors {
            let width = self.terminal_size.width as usize;
            let reset = "\x1b[0m";
            let bold = "\x1b[1m";
            let empty = " ".repeat(width);
            let bg: &str = if cancelled {
                "\x1b[48;2;80;70;30m"  // amber — cancelled
            } else if success {
                "\x1b[48;2;34;46;36m"  // muted green
            } else {
                "\x1b[48;2;55;34;34m"  // muted red
            };

            // header
            print!("{}{}{}\r\n", bg, empty, reset);
            let cmd_content = format!("  $ {}", command);
            let cmd_pad = " ".repeat(width.saturating_sub(cmd_content.len()));
            print!("{}{}{}{}{}{}\r\n", bg, bold, cmd_content, cmd_pad, bold, reset);
            print!("{}{}{}\r\n", bg, empty, reset);

            // output lines
            for line in output_lines {
                let content = format!("  {}", line);
                let padding = " ".repeat(width.saturating_sub(content.len()));
                print!("{}{}{}{}\r\n", bg, content, padding, reset);
            }

            // footer
            if !output_lines.is_empty() {
                print!("{}{}{}\r\n", bg, empty, reset);
            }
            let status = if cancelled { "  Cancelled" } else { &format!("  Took {:.1}s", duration_secs) };
            let status_pad = " ".repeat(width.saturating_sub(status.len()));
            print!("{}{}{}{}\r\n", bg, status, status_pad, reset);
            print!("{}{}{}\r\n", bg, empty, reset);
        } else {
            // no-color: skip bash header + output (already printed), just add footer
            for _ in 0..(output_lines.len() + 3) {
                print!("\x1b[1B"); // skip past already-printed lines
            }
            if !output_lines.is_empty() {
                print!("\r\n");
            }
            if cancelled {
                print!("  Cancelled\r\n");
            } else {
                print!("  Took {:.1}s\r\n", duration_secs);
            }
            print!("\r\n");
        }

        self.draw_input_box();
        let _ = io::stdout().flush();
    }

    // ── Private helpers ─────────────────────────────────────────────────────

    fn add_component(&mut self, component: Box<dyn Component>) -> ComponentId {
        let id = component.id();
        self.layout.append_component(id);
        self.components.insert(id, ComponentEntry { component });
        id
    }

    /// Render one component above the input box, then redraw the input box.
    fn render_component(&self, id: ComponentId) {
        if let Some(entry) = self.components.get(&id) {
            let buffer = entry.component.render(self.terminal_size.width);
            if buffer.height == 0 {
                return;
            }
            // Go above the input box
            self.move_above_input_box();
            let output = self.buffer_to_string(&buffer);
            print!("{}", output);
            // blank line after component for breathing room
            print!("\r\n");
            // Redraw input box below
            self.draw_input_box();
            let _ = io::stdout().flush();
        }
    }

    fn buffer_to_string(&self, buffer: &Buffer) -> String {
        let mut output = String::new();
        for row in &buffer.cells {
            let mut current_style: Option<String> = None;
            let mut current_chars = String::new();
            for cell in row {
                let style = format_cell_style(&cell.fg, &cell.bg, &cell.modifiers);
                if current_style.as_ref() != Some(&style) {
                    if !current_chars.is_empty() {
                        if let Some(ref s) = current_style {
                            output.push_str(s);
                        }
                        output.push_str(&current_chars);
                        current_chars.clear();
                    }
                    current_style = if style.is_empty() { None } else { Some(style) };
                }
                current_chars.push(cell.char);
            }
            if !current_chars.is_empty() {
                if let Some(ref s) = current_style {
                    output.push_str(s);
                }
                output.push_str(&current_chars);
            }
            output.push_str("\x1b[0m\r\n");
        }
        output
    }

    pub fn width(&self) -> u16 {
        self.terminal_size.width
    }

    pub fn use_colors(&self) -> bool {
        self.use_colors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Color;
    use crate::components::UserInputComponent;

    #[test]
    fn test_component_id_uniqueness() {
        let id1 = next_component_id();
        let id2 = next_component_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_renderer_creation() {
        let renderer = DifferentialRenderer::new(true);
        assert!(renderer.components.is_empty());
    }

    #[test]
    fn test_user_input_rendering() {
        let id = next_component_id();
        let component = UserInputComponent::new(id, "hello".to_string(), true);
        let buffer = component.render(20);
        assert_eq!(buffer.height, 3);
        assert_eq!(buffer.width, 20);
        for row in &buffer.cells {
            for cell in row {
                assert_eq!(cell.bg, Color::Ansi(235));
            }
        }
        for cell in &buffer.cells[0] {
            assert_eq!(cell.char, ' ');
        }
        for cell in &buffer.cells[2] {
            assert_eq!(cell.char, ' ');
        }
        let middle_row = &buffer.cells[1];
        assert_eq!(middle_row[2].char, 'h');
        assert_eq!(middle_row[3].char, 'e');
        assert_eq!(middle_row[4].char, 'l');
        assert_eq!(middle_row[5].char, 'l');
        assert_eq!(middle_row[6].char, 'o');
    }

    #[test]
    fn test_buffer_to_string() {
        let renderer = DifferentialRenderer::new(true);
        let id = next_component_id();
        let component = UserInputComponent::new(id, "test".to_string(), true);
        let buffer = component.render(10);
        let output = renderer.buffer_to_string(&buffer);
        assert!(output.contains("\x1b[48;5;235m"));
        assert!(output.contains("test"));
    }
}
