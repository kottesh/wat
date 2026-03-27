//! Differential renderer - pure line diffing and ANSI output

use std::io::{self, Write};

/// Cursor position (row, col)
pub type CursorPos = (usize, usize);

/// Pure differential renderer - no business logic
pub struct DiffRenderer {
    previous_lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

impl DiffRenderer {
    /// Create a new differential renderer
    pub fn new() -> Self {
        Self {
            previous_lines: Vec::new(),
            cursor_row: 0,
            cursor_col: 0,
        }
    }

    /// Render new lines with differential update
    /// 
    /// Only writes changed lines to stdout, using ANSI escape sequences
    /// for efficient terminal updates.
    pub fn render(&mut self, new_lines: Vec<String>, cursor_pos: CursorPos) {
        let new_len = new_lines.len();

        // Find first changed line
        let first_changed = self.find_first_change(&new_lines);

        // If nothing changed, just update cursor position and return
        if first_changed.is_none() {
            self.move_cursor(cursor_pos);
            return;
        }

        let first = first_changed.unwrap();
        let mut buf = String::new();

        // Begin synchronized output (prevents flicker)
        buf.push_str("\x1b[?2026h"); // begin synchronized output
        buf.push_str("\x1b[?25l");    // hide cursor

        // 1. Move to first changed line
        self.write_move_to_line(&mut buf, first);

        // 2. Write from first_changed up to the end of new_lines
        if new_len > first {
            for i in first..new_len {
                // Use absolute positioning - move to start of line i
                buf.push_str(&format!("\x1b[{};1H", i + 1)); // row is 1-indexed in ANSI
                buf.push_str("\x1b[2K"); // clear current line
                buf.push_str(&new_lines[i]); // write content
                self.cursor_row = i; // update our tracking
            }
        }

        // 3. Clear everything after our content to the end of the screen
        // This handles cases where content shrunk and we have leftover lines
        buf.push_str("\x1b[J");

        // End synchronized output
        buf.push_str("\x1b[?25h");    // show cursor
        buf.push_str("\x1b[?2026l"); // end synchronized output

        // Write all changes at once
        print!("{}", buf);
        let _ = io::stdout().flush();

        // Update state
        self.previous_lines = new_lines;
        self.cursor_row = if new_len > 0 { new_len - 1 } else { 0 };

        // 4. Finally, move to the desired logical cursor position
        self.move_cursor(cursor_pos);
    }

    /// Force a complete clear and redraw of the screen
    pub fn force_clear(&mut self) {
        // If running inside tmux, explicitly clear the tmux scrollback buffer
        if std::env::var("TMUX").is_ok() {
            let _ = std::process::Command::new("tmux")
                .arg("clear-history")
                .status();
        }

        // \x1b[3J = Clear scrollback buffer
        // \x1b[2J = Clear visible screen
        // \x1b[H  = Move cursor to top
        print!("\x1b[3J\x1b[2J\x1b[H");
        let _ = io::stdout().flush();

        self.previous_lines.clear();
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    /// Find the first line that differs between old and new
    fn find_first_change(&self, new_lines: &[String]) -> Option<usize> {
        let max_len = new_lines.len().max(self.previous_lines.len());
        for i in 0..max_len {
            if self.previous_lines.get(i) != new_lines.get(i) {
                return Some(i);
            }
        }
        None
    }

    /// Write ANSI codes to move to a specific line
    fn write_move_to_line(&mut self, buf: &mut String, target_row: usize) {
        // Use absolute positioning for consistency
        buf.push_str(&format!("\x1b[{};1H", target_row + 1)); // row is 1-indexed
        self.cursor_row = target_row;
        self.cursor_col = 0;
    }

    /// Move hardware cursor to a specific position
    fn move_cursor(&mut self, pos: CursorPos) {
        if self.previous_lines.is_empty() {
            return;
        }

        let (row, col) = pos;
        
        // Use absolute positioning
        print!("\x1b[{};{}H", row + 1, col + 1); // both are 1-indexed
        let _ = io::stdout().flush();

        self.cursor_row = row;
        self.cursor_col = col;
    }

    /// Get current cursor position
    pub fn cursor_pos(&self) -> CursorPos {
        (self.cursor_row, self.cursor_col)
    }

    /// Get previous lines (for testing)
    #[cfg(test)]
    pub fn previous_lines(&self) -> &[String] {
        &self.previous_lines
    }
}

impl Default for DiffRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_renderer() {
        let renderer = DiffRenderer::new();
        assert_eq!(renderer.previous_lines().len(), 0);
        assert_eq!(renderer.cursor_pos(), (0, 0));
    }

    #[test]
    fn test_find_first_change_no_change() {
        let renderer = DiffRenderer {
            previous_lines: vec!["line1".to_string(), "line2".to_string()],
            cursor_row: 0,
            cursor_col: 0,
        };

        let new_lines = vec!["line1".to_string(), "line2".to_string()];
        assert_eq!(renderer.find_first_change(&new_lines), None);
    }

    #[test]
    fn test_find_first_change_at_start() {
        let renderer = DiffRenderer {
            previous_lines: vec!["line1".to_string(), "line2".to_string()],
            cursor_row: 0,
            cursor_col: 0,
        };

        let new_lines = vec!["changed".to_string(), "line2".to_string()];
        assert_eq!(renderer.find_first_change(&new_lines), Some(0));
    }

    #[test]
    fn test_find_first_change_in_middle() {
        let renderer = DiffRenderer {
            previous_lines: vec!["line1".to_string(), "line2".to_string(), "line3".to_string()],
            cursor_row: 0,
            cursor_col: 0,
        };

        let new_lines = vec!["line1".to_string(), "changed".to_string(), "line3".to_string()];
        assert_eq!(renderer.find_first_change(&new_lines), Some(1));
    }

    #[test]
    fn test_find_first_change_length_diff() {
        let renderer = DiffRenderer {
            previous_lines: vec!["line1".to_string(), "line2".to_string()],
            cursor_row: 0,
            cursor_col: 0,
        };

        // New has more lines
        let new_lines = vec!["line1".to_string(), "line2".to_string(), "line3".to_string()];
        assert_eq!(renderer.find_first_change(&new_lines), Some(2));

        // New has fewer lines
        let new_lines = vec!["line1".to_string()];
        assert_eq!(renderer.find_first_change(&new_lines), Some(1));
    }

    #[test]
    fn test_find_first_change_empty_to_content() {
        let renderer = DiffRenderer::new();
        let new_lines = vec!["line1".to_string()];
        assert_eq!(renderer.find_first_change(&new_lines), Some(0));
    }

    #[test]
    fn test_find_first_change_content_to_empty() {
        let renderer = DiffRenderer {
            previous_lines: vec!["line1".to_string()],
            cursor_row: 0,
            cursor_col: 0,
        };
        let new_lines: Vec<String> = vec![];
        assert_eq!(renderer.find_first_change(&new_lines), Some(0));
    }

    #[test]
    fn test_write_move_to_line_forward() {
        let mut renderer = DiffRenderer {
            previous_lines: vec![],
            cursor_row: 2,
            cursor_col: 0,
        };
        let mut buf = String::new();
        renderer.write_move_to_line(&mut buf, 5);

        assert_eq!(buf, "\n\n\n\r");
        assert_eq!(renderer.cursor_row, 5);
    }

    #[test]
    fn test_write_move_to_line_backward() {
        let mut renderer = DiffRenderer {
            previous_lines: vec![],
            cursor_row: 5,
            cursor_col: 0,
        };
        let mut buf = String::new();
        renderer.write_move_to_line(&mut buf, 2);

        assert_eq!(buf, "\x1b[3A\r");
        assert_eq!(renderer.cursor_row, 2);
    }

    #[test]
    fn test_write_move_to_line_same() {
        let mut renderer = DiffRenderer {
            previous_lines: vec![],
            cursor_row: 3,
            cursor_col: 0,
        };
        let mut buf = String::new();
        renderer.write_move_to_line(&mut buf, 3);

        assert_eq!(buf, "\r");
        assert_eq!(renderer.cursor_row, 3);
    }

    #[test]
    fn test_default() {
        let renderer = DiffRenderer::default();
        assert_eq!(renderer.previous_lines().len(), 0);
        assert_eq!(renderer.cursor_pos(), (0, 0));
    }
}
