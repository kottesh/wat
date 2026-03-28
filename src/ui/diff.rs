//! Differential renderer - pure line diffing and ANSI output

use std::io::{self, Write};

/// Cursor position (row, col)
pub type CursorPos = (usize, usize);

/// Pure differential renderer - no business logic
pub struct DiffRenderer {
    previous_lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    lines_scrolled: usize,  // Track how many lines have scrolled off top
}

impl DiffRenderer {
    /// Create a new differential renderer
    pub fn new() -> Self {
        Self {
            previous_lines: Vec::new(),
            cursor_row: 0,
            cursor_col: 0,
            lines_scrolled: 0,
        }
    }

    /// Render new lines with differential update
    /// 
    /// Uses scrolling region to keep content flowing while maintaining
    /// input area at bottom of screen.
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
        let prev_len = self.previous_lines.len();
        let mut buf = String::new();

        // Get terminal height
        let terminal_height = crossterm::terminal::size()
            .map(|(_, h)| h as usize)
            .unwrap_or(24);

        // Begin synchronized output (prevents flicker)
        buf.push_str("\x1b[?2026h"); // begin synchronized output
        buf.push_str("\x1b[?25l");    // hide cursor

        // For all content: if it fits on screen, use absolute positioning
        // If it doesn't fit, append with newlines (scroll naturally)
        
        if new_len <= terminal_height {
            // Content fits on screen - use absolute positioning
            for i in first..new_len {
                buf.push_str(&format!("\x1b[{};1H", i + 1));
                buf.push_str("\x1b[2K");
                buf.push_str(&new_lines[i]);
            }
            
            // Clear any leftover lines
            if new_len < prev_len {
                buf.push_str("\x1b[J");
            }
            
            self.cursor_row = if new_len > 0 { new_len - 1 } else { 0 };
            self.lines_scrolled = 0;
        } else {
            // Content exceeds screen - scroll naturally
            // We need to append all new content
            
            if prev_len == 0 {
                // First render with lots of content
                // Just write everything sequentially
                for (idx, line) in new_lines.iter().enumerate() {
                    if idx > 0 {
                        buf.push_str("\n");
                    }
                    buf.push_str("\x1b[2K");
                    buf.push_str(line);
                }
                self.lines_scrolled = new_len.saturating_sub(terminal_height);
            } else if new_len > prev_len {
                // Content growing - append new lines
                let lines_to_add = new_len - prev_len;
                
                // Scroll will happen - track it
                if prev_len >= terminal_height {
                    self.lines_scrolled += lines_to_add;
                } else if new_len > terminal_height {
                    self.lines_scrolled = new_len - terminal_height;
                }
                
                // Append new lines
                for i in prev_len..new_len {
                    buf.push_str("\n");
                    buf.push_str("\x1b[2K");
                    buf.push_str(&new_lines[i]);
                }
            } else {
                // Content same size or shrinking - rare, just redraw visible portion
                let visible_start = new_len.saturating_sub(terminal_height);
                for i in visible_start..new_len {
                    let screen_row = i - visible_start + 1;
                    buf.push_str(&format!("\x1b[{};1H", screen_row));
                    buf.push_str("\x1b[2K");
                    buf.push_str(&new_lines[i]);
                }
            }
            
            self.cursor_row = new_len.saturating_sub(1);
        }

        // End synchronized output
        buf.push_str("\x1b[?25h");    // show cursor
        buf.push_str("\x1b[?2026l"); // end synchronized output

        // Write all changes at once
        print!("{}", buf);
        let _ = io::stdout().flush();

        // Update state
        self.previous_lines = new_lines;

        // Move cursor to final position
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
        self.lines_scrolled = 0;
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
        let (row, col) = pos;
        
        // Adjust row for scrolling - cursor positions are relative to visible screen
        // If lines have scrolled off top, subtract that offset
        let screen_row = row.saturating_sub(self.lines_scrolled);
        
        // Use absolute positioning for cursor
        print!("\x1b[{};{}H", screen_row + 1, col + 1);
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
            lines_scrolled: 0,
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
            lines_scrolled: 0,
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
            lines_scrolled: 0,
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
            lines_scrolled: 0,
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
            lines_scrolled: 0,
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
            lines_scrolled: 0,
        };
        let mut buf = String::new();
        renderer.write_move_to_line(&mut buf, 5);

        assert_eq!(buf, "\x1b[6;1H"); // absolute positioning: row 6 (5+1), col 1
        assert_eq!(renderer.cursor_row, 5);
    }

    #[test]
    fn test_write_move_to_line_backward() {
        let mut renderer = DiffRenderer {
            previous_lines: vec![],
            cursor_row: 5,
            cursor_col: 0,
            lines_scrolled: 0,
        };
        let mut buf = String::new();
        renderer.write_move_to_line(&mut buf, 2);

        assert_eq!(buf, "\x1b[3;1H"); // absolute positioning: row 3 (2+1), col 1
        assert_eq!(renderer.cursor_row, 2);
    }

    #[test]
    fn test_write_move_to_line_same() {
        let mut renderer = DiffRenderer {
            previous_lines: vec![],
            cursor_row: 3,
            cursor_col: 0,
            lines_scrolled: 0,
        };
        let mut buf = String::new();
        renderer.write_move_to_line(&mut buf, 3);

        assert_eq!(buf, "\x1b[4;1H"); // absolute positioning: row 4 (3+1), col 1
        assert_eq!(renderer.cursor_row, 3);
    }

    #[test]
    fn test_default() {
        let renderer = DiffRenderer::default();
        assert_eq!(renderer.previous_lines().len(), 0);
        assert_eq!(renderer.cursor_pos(), (0, 0));
    }
}
