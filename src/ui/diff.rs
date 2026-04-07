//! Inline diff renderer - updates terminal in place with viewport-safe diffs.

use std::io::{self, Write};

pub type CursorPos = (usize, usize);

pub struct DiffRenderer {
    previous_lines: Vec<String>,
    previous_height: usize,
    previous_viewport_top: usize,
    hardware_cursor_row: usize,
    hardware_cursor_col: usize,
}

impl DiffRenderer {
    pub fn new() -> Self {
        Self {
            previous_lines: Vec::new(),
            previous_height: 0,
            previous_viewport_top: 0,
            hardware_cursor_row: 0,
            hardware_cursor_col: 0,
        }
    }

    pub fn render(
        &mut self,
        new_lines: Vec<String>,
        cursor_pos: CursorPos,
        terminal_height: usize,
    ) {
        let height = terminal_height.max(1);
        let prev_len = self.previous_lines.len();
        let new_len = new_lines.len();

        let target_row = cursor_pos.0.min(new_len.saturating_sub(1));
        let target_col = cursor_pos.1;

        // First render: print everything without clearing scrollback.
        if prev_len == 0 {
            self.full_render(&new_lines, target_row, target_col, false, height);
            return;
        }

        // On terminal height changes, reset with a safe full redraw.
        if self.previous_height != 0 && self.previous_height != height {
            self.full_render(&new_lines, target_row, target_col, true, height);
            return;
        }

        // Find first and last changed lines.
        let mut first_changed: Option<usize> = None;
        let mut last_changed: usize = 0;
        let max_len = prev_len.max(new_len);
        for i in 0..max_len {
            let old = self.previous_lines.get(i).map(String::as_str).unwrap_or("");
            let new = new_lines.get(i).map(String::as_str).unwrap_or("");
            if old != new {
                if first_changed.is_none() {
                    first_changed = Some(i);
                }
                last_changed = i;
            }
        }

        let appended_lines = new_len > prev_len;
        if appended_lines {
            if first_changed.is_none() {
                first_changed = Some(prev_len);
            }
            last_changed = new_len.saturating_sub(1);
        }

        let mut prev_viewport_top = self.previous_viewport_top;
        let mut viewport_top = prev_viewport_top;
        let mut hardware_row = self.hardware_cursor_row;

        // No textual changes, only move cursor if needed.
        if first_changed.is_none() {
            self.move_cursor_only(target_row, target_col, viewport_top, new_len, height);
            self.previous_lines = new_lines;
            self.previous_height = height;
            return;
        }

        let first_changed = first_changed.unwrap();
        let append_start = appended_lines && first_changed == prev_len && first_changed > 0;

        // If first changed line is above prior viewport, relative updates are unsafe.
        if first_changed < prev_viewport_top {
            self.full_render(&new_lines, target_row, target_col, true, height);
            return;
        }

        let mut buffer = String::new();
        buffer.push_str("\x1b[?2026h\x1b[?25l"); // synchronized output + hide cursor

        let prev_viewport_bottom = prev_viewport_top + height.saturating_sub(1);
        let move_target_row = if append_start {
            first_changed.saturating_sub(1)
        } else {
            first_changed
        };

        // If target row is below visible viewport, scroll down via CRLF to commit history.
        if move_target_row > prev_viewport_bottom {
            let current_screen_row = hardware_row
                .saturating_sub(prev_viewport_top)
                .min(height - 1);
            let move_to_bottom = (height - 1).saturating_sub(current_screen_row);
            if move_to_bottom > 0 {
                buffer.push_str(&format!("\x1b[{}B", move_to_bottom));
            }

            let scroll = move_target_row - prev_viewport_bottom;
            for _ in 0..scroll {
                buffer.push_str("\r\n");
            }

            prev_viewport_top += scroll;
            viewport_top += scroll;
            hardware_row = move_target_row;
        }

        // Move cursor to first changed line in screen coordinates.
        let current_screen_row = hardware_row as isize - prev_viewport_top as isize;
        let target_screen_row = move_target_row as isize - viewport_top as isize;
        let line_diff = target_screen_row - current_screen_row;
        if line_diff > 0 {
            buffer.push_str(&format!("\x1b[{}B", line_diff));
        } else if line_diff < 0 {
            buffer.push_str(&format!("\x1b[{}A", -line_diff));
        }

        // For pure appends, start writing by moving to next line first.
        buffer.push_str(if append_start { "\r\n" } else { "\r" });

        // Rewrite only changed region.
        let render_end = last_changed.min(new_len.saturating_sub(1));
        for i in first_changed..=render_end {
            if i > first_changed {
                buffer.push_str("\r\n");
            }
            buffer.push_str("\x1b[2K");
            if let Some(line) = new_lines.get(i) {
                buffer.push_str(line);
            }
        }

        let mut final_row = render_end;

        // If content shrunk, clear extra rows from previous frame.
        if prev_len > new_len {
            if render_end < new_len.saturating_sub(1) {
                let move_down = new_len.saturating_sub(1) - render_end;
                if move_down > 0 {
                    buffer.push_str(&format!("\x1b[{}B", move_down));
                }
            }

            let extra = prev_len - new_len;
            for _ in 0..extra {
                buffer.push_str("\r\n\x1b[2K");
            }
            if extra > 0 {
                buffer.push_str(&format!("\x1b[{}A", extra));
            }
            final_row = new_len.saturating_sub(1);
        }

        // Position hardware cursor at desired input location.
        let row_delta = target_row as isize - final_row as isize;
        if row_delta > 0 {
            buffer.push_str(&format!("\x1b[{}B", row_delta));
        } else if row_delta < 0 {
            buffer.push_str(&format!("\x1b[{}A", -row_delta));
        }
        buffer.push_str("\r");
        if target_col > 0 {
            buffer.push_str(&format!("\x1b[{}C", target_col));
        }

        buffer.push_str("\x1b[?25h\x1b[?2026l"); // show cursor + end synchronized output

        print!("{}", buffer);
        let _ = io::stdout().flush();

        self.previous_lines = new_lines;
        self.previous_height = height;
        self.previous_viewport_top =
            prev_viewport_top.max(final_row.saturating_sub(height.saturating_sub(1)));
        self.hardware_cursor_row = target_row;
        self.hardware_cursor_col = target_col;
    }

    fn full_render(
        &mut self,
        new_lines: &[String],
        target_row: usize,
        target_col: usize,
        clear_screen: bool,
        height: usize,
    ) {
        let mut buffer = String::new();
        buffer.push_str("\x1b[?2026h\x1b[?25l");
        if clear_screen {
            // Do not clear scrollback; only clear viewport.
            buffer.push_str("\x1b[2J\x1b[H");
        }

        for (i, line) in new_lines.iter().enumerate() {
            if i > 0 {
                buffer.push_str("\r\n");
            }
            buffer.push_str(line);
        }

        let end_row = new_lines.len().saturating_sub(1);
        if target_row < end_row {
            buffer.push_str(&format!("\x1b[{}A", end_row - target_row));
        } else if target_row > end_row {
            buffer.push_str(&format!("\x1b[{}B", target_row - end_row));
        }
        buffer.push_str("\r");
        if target_col > 0 {
            buffer.push_str(&format!("\x1b[{}C", target_col));
        }

        buffer.push_str("\x1b[?25h\x1b[?2026l");

        print!("{}", buffer);
        let _ = io::stdout().flush();

        self.previous_lines = new_lines.to_vec();
        self.previous_height = height;
        self.previous_viewport_top = new_lines.len().saturating_sub(height);
        self.hardware_cursor_row = target_row;
        self.hardware_cursor_col = target_col;
    }

    fn move_cursor_only(
        &mut self,
        target_row: usize,
        target_col: usize,
        viewport_top: usize,
        new_len: usize,
        height: usize,
    ) {
        if self.hardware_cursor_row == target_row && self.hardware_cursor_col == target_col {
            return;
        }

        let viewport_top = if viewport_top > new_len.saturating_sub(1) {
            new_len.saturating_sub(height)
        } else {
            viewport_top
        };
        let current_screen_row = self.hardware_cursor_row.saturating_sub(viewport_top) as isize;
        let target_screen_row = target_row.saturating_sub(viewport_top) as isize;
        let row_delta = target_screen_row - current_screen_row;

        let mut buffer = String::new();
        buffer.push_str("\x1b[?25l");
        if row_delta > 0 {
            buffer.push_str(&format!("\x1b[{}B", row_delta));
        } else if row_delta < 0 {
            buffer.push_str(&format!("\x1b[{}A", -row_delta));
        }
        buffer.push_str("\r");
        if target_col > 0 {
            buffer.push_str(&format!("\x1b[{}C", target_col));
        }
        buffer.push_str("\x1b[?25h");

        print!("{}", buffer);
        let _ = io::stdout().flush();

        self.hardware_cursor_row = target_row;
        self.hardware_cursor_col = target_col;
    }

    pub fn force_clear(&mut self) {
        // Clear viewport only. Keep terminal scrollback intact.
        print!("\x1b[2J\x1b[H");
        let _ = io::stdout().flush();

        self.previous_lines.clear();
        self.previous_height = 0;
        self.previous_viewport_top = 0;
        self.hardware_cursor_row = 0;
        self.hardware_cursor_col = 0;
    }

    #[cfg(test)]
    pub fn previous_lines(&self) -> &[String] {
        &self.previous_lines
    }

    #[cfg(test)]
    pub fn cursor_pos(&self) -> CursorPos {
        (self.hardware_cursor_row, self.hardware_cursor_col)
    }
}

impl Default for DiffRenderer {
    fn default() -> Self {
        Self::new()
    }
}
