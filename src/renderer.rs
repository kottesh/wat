//! Differential renderer — pi-style.
//!
//! All UI state lives here.  `render()` converts everything to a flat
//! `Vec<String>` of lines, diffs against `previous_lines`, then writes
//! only the changed lines to stdout wrapped in synchronized-output guards.
//! No cursor-juggling escape sequences outside of `render()`.
//!
//! Layout (lines produced by render_all):
//!   [component 0 lines]
//!   [blank separator]
//!   [component 1 lines]
//!   [blank separator]
//!   ...
//!   [current bash block lines]  ← while a command is running
//!   [blank separator]
//!   [blank gap]                 ─┐
//!   [top border]                 │  input box (always last)
//!   [input / spinner / hint]     │
//!   [bottom border]             ─┘

use std::io::{self, Write};

use crate::component::{format_cell_style, Buffer, Component, ComponentId, Size};
use crate::components::{
    ErrorComponent, ResponseComponent, ToolCallComponent, ToolResultComponent, UserInputComponent,
};

use std::sync::{Arc, Mutex};

/// A thread-safe wrapper around the differential renderer
pub type SharedRenderer = Arc<Mutex<DifferentialRenderer>>;

static COMPONENT_ID_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

pub fn next_component_id() -> ComponentId {
    ComponentId(COMPONENT_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
}

// ── Bash block ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum BashStatus {
    Running,
    Done { success: bool },
    Cancelled,
}

#[derive(Debug)]
struct BashBlock {
    command: String,
    output_lines: Vec<String>,
    elapsed_secs: f64,
    status: BashStatus,
}

impl BashBlock {
    fn new(command: &str) -> Self {
        Self {
            command: command.to_string(),
            output_lines: Vec::new(),
            elapsed_secs: 0.0,
            status: BashStatus::Running,
        }
    }

    fn render_lines(&self, width: usize, use_colors: bool) -> Vec<String> {
        if !use_colors {
            let mut lines = vec![format!("  $ {}", self.command)];
            for l in &self.output_lines {
                lines.push(format!("  {}", l));
            }
            let status = match &self.status {
                BashStatus::Running => format!("  Running {:.1}s", self.elapsed_secs),
                BashStatus::Done { .. } => format!("  Took {:.1}s", self.elapsed_secs),
                BashStatus::Cancelled => "  Cancelled".to_string(),
            };
            lines.push(status);
            return lines;
        }

        let bg = match &self.status {
            BashStatus::Running => "\x1b[48;2;39;39;39m",
            BashStatus::Done { success: true } => "\x1b[48;2;34;46;36m",
            BashStatus::Done { success: false } => "\x1b[48;2;55;34;34m",
            BashStatus::Cancelled => "\x1b[48;2;80;70;30m",
        };
        let reset = "\x1b[0m";
        let bold = "\x1b[1m";
        let pad = |s: &str| " ".repeat(width.saturating_sub(s.len()));

        let mut lines = Vec::new();
        let empty = " ".repeat(width);

        // top padding
        lines.push(format!("{}{}{}", bg, empty, reset));

        // command
        let cmd = format!("  $ {}", self.command);
        lines.push(format!("{}{}{}{}{}{}", bg, bold, cmd, pad(&cmd), bold, reset));

        // gap
        lines.push(format!("{}{}{}", bg, empty, reset));

        // output lines
        for l in &self.output_lines {
            let content = format!("  {}", l);
            lines.push(format!("{}{}{}{}", bg, content, pad(&content), reset));
        }

        // footer gap
        if !self.output_lines.is_empty() {
            lines.push(format!("{}{}{}", bg, empty, reset));
        }

        // status line
        let status = match &self.status {
            BashStatus::Running => format!("  Running {:.1}s", self.elapsed_secs),
            BashStatus::Done { .. } => format!("  Took {:.1}s", self.elapsed_secs),
            BashStatus::Cancelled => "  Cancelled".to_string(),
        };
        lines.push(format!("{}{}{}{}", bg, status, pad(&status), reset));

        // bottom padding
        lines.push(format!("{}{}{}", bg, empty, reset));

        lines
    }
}

// ── Render items (history) ──────────────────────────────────────────────────

enum RenderItem {
    /// A completed component rendered via the Buffer system
    Buffer(Vec<String>),
    /// A completed bash block
    Bash(BashBlock),
}

// ── Renderer ────────────────────────────────────────────────────────────────

pub struct DifferentialRenderer {
    /// Ordered list of completed render items
    items: Vec<RenderItem>,
    /// Currently executing bash block (None when idle)
    current_bash: Option<BashBlock>,
    /// Currently streaming response (None when idle)
    current_response: Option<ResponseComponent>,
    /// Current input lines for rendering
    current_input: Vec<String>,
    /// Cursor row within the multiline input (0-indexed)
    input_cursor_row: usize,
    /// Cursor column within the current input line (0-indexed)
    input_cursor_col: usize,
    
    /// History for undo/redo
    undo_stack: Vec<(Vec<String>, usize, usize)>,
    redo_stack: Vec<(Vec<String>, usize, usize)>,

    /// Spinner text shown on the input line while LLM is thinking
    spinner_text: Option<String>,
    /// Hint text shown on the input line during bash (e.g. "esc to cancel")
    input_hint: Option<String>,

    /// Last rendered lines (for diffing)
    previous_lines: Vec<String>,
    /// Where the hardware cursor currently sits (index into previous_lines)
    cursor_row: usize,
    /// Hardware cursor column position
    cursor_col: usize,

    terminal_size: Size,
    use_colors: bool,
}

impl DifferentialRenderer {
    pub fn new(use_colors: bool) -> Self {
        let terminal_size = crossterm::terminal::size()
            .map(|(w, h)| Size::new(w, h))
            .unwrap_or_else(|_| Size::new(80, 24));

        Self {
            items: Vec::new(),
            current_bash: None,
            current_response: None,
            current_input: vec![String::new()],
            input_cursor_row: 0,
            input_cursor_col: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            spinner_text: None,
            input_hint: None,
            previous_lines: Vec::new(),
            cursor_row: 0,
            cursor_col: 0,
            terminal_size,
            use_colors,
        }
    }

    pub fn update_size(&mut self) {
        if let Ok((w, h)) = crossterm::terminal::size() {
            self.terminal_size = Size::new(w, h);
        }
    }

    // ── Input box state (multiline editor) ──────────────────────────────────

    fn save_undo(&mut self) {
        self.undo_stack.push((self.current_input.clone(), self.input_cursor_row, self.input_cursor_col));
        self.redo_stack.clear();
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push((self.current_input.clone(), self.input_cursor_row, self.input_cursor_col));
            self.current_input = prev.0;
            self.input_cursor_row = prev.1;
            self.input_cursor_col = prev.2;
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push((self.current_input.clone(), self.input_cursor_row, self.input_cursor_col));
            self.current_input = next.0;
            self.input_cursor_row = next.1;
            self.input_cursor_col = next.2;
        }
    }

    pub fn push_input_char(&mut self, c: char) {
        self.save_undo();
        let row = self.input_cursor_row;
        let col = self.input_cursor_col;
        self.current_input[row].insert(col, c);
        self.input_cursor_col += 1;
    }

    pub fn pop_input_char(&mut self) {
        self.save_undo();
        let row = self.input_cursor_row;
        let col = self.input_cursor_col;

        if col > 0 {
            // Delete character on current line
            self.current_input[row].remove(col - 1);
            self.input_cursor_col -= 1;
        } else if row > 0 {
            // Merge current line with previous line
            let current_line = self.current_input.remove(row);
            let prev_row = row - 1;
            let prev_len = self.current_input[prev_row].len();
            self.current_input[prev_row].push_str(&current_line);
            self.input_cursor_row = prev_row;
            self.input_cursor_col = prev_len;
        }
    }

    pub fn insert_newline(&mut self) {
        self.save_undo();
        let row = self.input_cursor_row;
        let col = self.input_cursor_col;
        let line = &mut self.current_input[row];
        let remaining = line.split_off(col);
        self.current_input.insert(row + 1, remaining);
        self.input_cursor_row += 1;
        self.input_cursor_col = 0;
    }

    pub fn move_cursor_up(&mut self) {
        if self.input_cursor_row > 0 {
            self.input_cursor_row -= 1;
            self.input_cursor_col = self.input_cursor_col.min(self.current_input[self.input_cursor_row].len());
        }
    }

    pub fn move_cursor_down(&mut self) {
        if self.input_cursor_row + 1 < self.current_input.len() {
            self.input_cursor_row += 1;
            self.input_cursor_col = self.input_cursor_col.min(self.current_input[self.input_cursor_row].len());
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.input_cursor_col > 0 {
            self.input_cursor_col -= 1;
        } else if self.input_cursor_row > 0 {
            self.input_cursor_row -= 1;
            self.input_cursor_col = self.current_input[self.input_cursor_row].len();
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.input_cursor_col < self.current_input[self.input_cursor_row].len() {
            self.input_cursor_col += 1;
        } else if self.input_cursor_row + 1 < self.current_input.len() {
            self.input_cursor_row += 1;
            self.input_cursor_col = 0;
        }
    }

    pub fn take_input(&mut self) -> String {
        let result = self.current_input.join("\n");
        self.current_input = vec![String::new()];
        self.input_cursor_row = 0;
        self.input_cursor_col = 0;
        self.undo_stack.clear();
        self.redo_stack.clear();
        result
    }

    // ── Spinner ─────────────────────────────────────────────────────────────

    pub fn set_spinner(&mut self, text: String) {
        self.spinner_text = Some(text);
    }

    pub fn clear_spinner(&mut self) {
        self.spinner_text = None;
    }

    // ── Input hint (shown while bash runs) ──────────────────────────────────

    pub fn set_input_hint(&mut self, hint: String) {
        self.input_hint = Some(hint);
    }

    pub fn clear_input_hint(&mut self) {
        self.input_hint = None;
    }

    // ── Completed components ─────────────────────────────────────────────────

    pub fn add_user_input(&mut self, content: String) {
        self.update_size();
        let id = next_component_id();
        let comp = UserInputComponent::new(id, content, self.use_colors);
        let lines = self.component_to_lines(&comp);
        if !lines.is_empty() {
            self.items.push(RenderItem::Buffer(lines));
        }
    }

    pub fn add_response(&mut self, content: String) {
        self.update_size();
        let id = next_component_id();
        let comp = ResponseComponent::new(id, content, self.use_colors);
        let lines = self.component_to_lines(&comp);
        if !lines.is_empty() {
            self.items.push(RenderItem::Buffer(lines));
        }
    }

    pub fn add_tool_call(&mut self, tool_name: String, args: String) {
        self.update_size();
        let id = next_component_id();
        let comp = ToolCallComponent::new(id, tool_name, args, self.use_colors);
        let lines = self.component_to_lines(&comp);
        if !lines.is_empty() {
            self.items.push(RenderItem::Buffer(lines));
        }
    }

    pub fn add_tool_result(
        &mut self,
        tool_name: String,
        output: String,
        duration_secs: Option<f64>,
        success: bool,
        command: Option<String>,
    ) {
        self.update_size();
        let id = next_component_id();
        let comp = ToolResultComponent::new(
            id, tool_name, output, duration_secs, success, command, self.use_colors,
        );
        let lines = self.component_to_lines(&comp);
        if !lines.is_empty() {
            self.items.push(RenderItem::Buffer(lines));
        }
    }

    pub fn add_error(&mut self, message: String) {
        self.update_size();
        let id = next_component_id();
        let comp = ErrorComponent::new(id, message, self.use_colors);
        let lines = self.component_to_lines(&comp);
        if !lines.is_empty() {
            self.items.push(RenderItem::Buffer(lines));
        }
    }

    // ── Bash block lifecycle ─────────────────────────────────────────────────

    pub fn start_bash(&mut self, command: &str) {
        self.current_bash = Some(BashBlock::new(command));
    }

    pub fn push_bash_output(&mut self, line: String) {
        if let Some(ref mut b) = self.current_bash {
            b.output_lines.push(line);
        }
    }

    pub fn set_bash_elapsed(&mut self, secs: f64) {
        if let Some(ref mut b) = self.current_bash {
            b.elapsed_secs = secs;
        }
    }

    // ── Response streaming ──────────────────────────────────────────────────

    pub fn start_streaming_response(&mut self) {
        let id = next_component_id();
        self.current_response = Some(ResponseComponent::new(id, String::new(), self.use_colors));
    }

    pub fn push_response_chunk(&mut self, chunk: &str) {
        if let Some(ref mut resp) = self.current_response {
            if let Some(state) = resp.as_any_mut().downcast_mut::<crate::components::ResponseComponent>() {
                state.append_content(chunk);
            }
        }
    }

    /// Finalise the streaming response — moves it to history.
    pub fn finalize_response(&mut self) {
        if let Some(resp) = self.current_response.take() {
            let lines = self.component_to_lines(&resp);
            if !lines.is_empty() {
                self.items.push(RenderItem::Buffer(lines));
            }
        }
    }

    /// Finalise the current bash block — moves it to the completed item list.
    pub fn finalize_bash(&mut self, duration: f64, success: bool, cancelled: bool) {
        if let Some(mut b) = self.current_bash.take() {
            b.elapsed_secs = duration;
            b.status = if cancelled {
                BashStatus::Cancelled
            } else {
                BashStatus::Done { success }
            };
            self.items.push(RenderItem::Bash(b));
        }
    }

    // ── Core render ──────────────────────────────────────────────────────────

    /// Produce the complete list of terminal lines and the logical cursor position.
    fn render_all(&self) -> (Vec<String>, usize, usize) {
        let width = self.terminal_size.width as usize;
        let mut lines: Vec<String> = Vec::new();

        // Completed items
        for item in &self.items {
            match item {
                RenderItem::Buffer(item_lines) => {
                    if !item_lines.is_empty() {
                        lines.extend_from_slice(item_lines);
                        lines.push(String::new()); // blank separator
                    }
                }
                RenderItem::Bash(bash) => {
                    let bash_lines = bash.render_lines(width, self.use_colors);
                    lines.extend(bash_lines);
                    lines.push(String::new());
                }
            }
        }

        // Running bash block
        if let Some(ref bash) = self.current_bash {
            let bash_lines = bash.render_lines(width, self.use_colors);
            lines.extend(bash_lines);
            lines.push(String::new());
        }

        // Currently streaming response
        if let Some(ref comp) = self.current_response {
            let resp_lines = self.component_to_lines(comp);
            if !resp_lines.is_empty() {
                lines.extend(resp_lines);
                lines.push(String::new());
            }
        }

        // Dedicated status row (Spinner OR Hint) above the input box.
        // This row is always allocated to prevent the input box from jumping.
        let status_line = if let Some(ref s) = self.spinner_text {
            s.clone()
        } else if let Some(ref h) = self.input_hint {
            h.clone()
        } else {
            String::new()
        };
        lines.push(status_line);

        // ── Input box (always last) ─────────────────────────────────────────
        let border_str = "─".repeat(width.saturating_sub(1));
        let border = if self.use_colors {
            format!("\x1b[38;5;152m{}\x1b[0m", border_str)
        } else {
            border_str
        };

        lines.push(border.clone()); // top border

        // Multi-line input area (always visible)
        let mut cursor_row = 0;
        let mut cursor_col = 0;

        for (i, line) in self.current_input.iter().enumerate() {
            lines.push(format!("  {}", line));
            if i == self.input_cursor_row {
                cursor_row = lines.len() - 1;
                // Calculate cursor column based on actual chars before input_cursor_col
                let prefix: String = line.chars().take(self.input_cursor_col).collect();
                cursor_col = crate::renderer::visible_width(&prefix) + 2;
            }
        }

        lines.push(border); // bottom border

        (lines, cursor_row, cursor_col)
    }

    /// Differential render: compute new lines, diff, write only changes.
    pub fn render(&mut self) {
        self.update_size();
        let (new_lines, target_cursor_row, target_cursor_col) = self.render_all();
        let new_len = new_lines.len();
        let old_len = self.previous_lines.len();

        // Find first changed line
        let mut first_changed: Option<usize> = None;
        let max_len = new_len.max(old_len);
        for i in 0..max_len {
            if self.previous_lines.get(i) != new_lines.get(i) {
                first_changed = Some(i);
                break;
            }
        }

        // If nothing changed, just update hardware cursor position and return
        if first_changed.is_none() {
            self.move_hardware_cursor(target_cursor_row, target_cursor_col);
            return;
        }

        let first = first_changed.unwrap();
        let mut buf = String::new();
        buf.push_str("\x1b[?2026h"); // begin synchronized output
        buf.push_str("\x1b[?25l");    // hide cursor

        // 1. Move to first changed line
        let diff = first as i64 - self.cursor_row as i64;
        if diff > 0 {
            buf.push_str(&format!("\x1b[{}B", diff));
        } else if diff < 0 {
            buf.push_str(&format!("\x1b[{}A", -diff));
        }
        buf.push('\r');
        self.cursor_row = first;

        // 2. Write from first_changed up to the end of new_lines
        if new_len > first {
            for i in first..new_len {
                if i > first {
                    // \r\n at the bottom of the viewport will scroll the terminal.
                    buf.push_str("\r\n");
                    self.cursor_row += 1;
                }
                buf.push_str("\x1b[2K"); // clear current line
                buf.push_str(&new_lines[i]);
            }
        }

        // 3. Clear everything after our content to the end of the screen.
        // This handles cases where content shrunk and we have leftover lines.
        buf.push_str("\x1b[J");

        buf.push_str("\x1b[?25h");    // show cursor
        buf.push_str("\x1b[?2026l"); // end synchronized output

        print!("{}", buf);
        let _ = io::stdout().flush();

        self.previous_lines = new_lines;
        self.cursor_row = if new_len > 0 { new_len - 1 } else { 0 };

        // 4. Finally, move to the desired logical cursor position (input line)
        self.move_hardware_cursor(target_cursor_row, target_cursor_col);
    }

    fn move_hardware_cursor(&mut self, row: usize, col: usize) {
        if self.previous_lines.is_empty() {
            return;
        }
        let diff = row as i64 - self.cursor_row as i64;
        if diff > 0 {
            print!("\x1b[{}B", diff);
        } else if diff < 0 {
            print!("\x1b[{}A", -diff);
        }
        // Move to absolute column (1-indexed)
        print!("\r\x1b[{}G", col + 1);
        let _ = io::stdout().flush();
        self.cursor_row = row;
        self.cursor_col = col;
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn component_to_lines<C: crate::component::Component>(&self, comp: &C) -> Vec<String> {
        let buf = comp.render(self.terminal_size.width);
        self.buffer_to_lines(&buf)
    }

    fn buffer_to_lines(&self, buffer: &Buffer) -> Vec<String> {
        let mut result = Vec::new();
        for row in &buffer.cells {
            let mut line = String::new();
            let mut cur_style: Option<String> = None;
            let mut cur_chars = String::new();

            for cell in row {
                let style = format_cell_style(&cell.fg, &cell.bg, &cell.modifiers);
                if cur_style.as_deref() != Some(&style) {
                    if !cur_chars.is_empty() {
                        if let Some(ref s) = cur_style {
                            line.push_str(s);
                        }
                        line.push_str(&cur_chars);
                        cur_chars.clear();
                    }
                    cur_style = if style.is_empty() { None } else { Some(style) };
                }
                cur_chars.push(cell.char);
            }
            if !cur_chars.is_empty() {
                if let Some(ref s) = cur_style {
                    line.push_str(s);
                }
                line.push_str(&cur_chars);
            }
            line.push_str("\x1b[0m");
            result.push(line);
        }
        result
    }

    /// Return the stdout/stderr output lines of the most recently finalised bash block.
    pub fn last_bash_output(&self) -> String {
        for item in self.items.iter().rev() {
            if let RenderItem::Bash(b) = item {
                return b.output_lines.join("\n");
            }
        }
        String::new()
    }

    pub fn use_colors(&self) -> bool {
        self.use_colors
    }
}

/// Calculate the visible width of a string (stripping ANSI escape codes).
pub fn visible_width(s: &str) -> usize {
    let mut width = 0;
    let mut in_esc = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_esc = true;
            continue;
        }
        if in_esc {
            if c.is_ascii_alphabetic() || c == 'm' {
                in_esc = false;
            }
            continue;
        }
        width += 1;
    }
    width
}
