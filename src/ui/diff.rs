//! Inline diff renderer - updates same lines in place

use std::io::{self, Write};

pub type CursorPos = (usize, usize);

pub struct DiffRenderer {
    previous_lines: Vec<String>,
    cursor_is_at_row: usize, // Track where cursor actually is (0-indexed)
}

impl DiffRenderer {
    pub fn new() -> Self {
        Self {
            previous_lines: Vec::new(),
            cursor_is_at_row: 0,
        }
    }

    pub fn render(&mut self, new_lines: Vec<String>, cursor_pos: CursorPos) {
        let prev_len = self.previous_lines.len();
        let new_len = new_lines.len();
        
        // Check if anything changed
        let changed = prev_len != new_len || 
                     self.previous_lines.iter().zip(&new_lines).any(|(a, b)| a != b);
        
        if !changed {
            return;
        }

        let mut output = String::new();
        output.push_str("\x1b[?25l"); // Hide cursor
        
        if prev_len == 0 {
            // First render - just print the lines
            for (i, line) in new_lines.iter().enumerate() {
                output.push_str(line);
                if i < new_len - 1 {
                    output.push_str("\r\n");
                }
            }
            // After first render, cursor is at last line
            self.cursor_is_at_row = new_len.saturating_sub(1);
        } else {
            // Move to first line from wherever cursor currently is
            if self.cursor_is_at_row > 0 {
                output.push_str(&format!("\x1b[{}A", self.cursor_is_at_row));
            }
            output.push_str("\r");
            
            // Update each line by clearing and rewriting
            for (i, line) in new_lines.iter().enumerate() {
                output.push_str("\x1b[K"); // Clear from cursor to end of line
                output.push_str(line);
                if i < new_len - 1 {
                    output.push_str("\r\n"); // Move to next line
                }
            }
            
            // If shrinking, clear the extra lines
            if new_len < prev_len {
                for _ in new_len..prev_len {
                    output.push_str("\r\n\x1b[K");
                }
                // Move back to last content line
                if prev_len > new_len {
                    output.push_str(&format!("\x1b[{}A", prev_len - new_len));
                }
            }
            
            // After redrawing, cursor is at last line
            self.cursor_is_at_row = new_len.saturating_sub(1);
        }
        
        // Position cursor at desired location
        let (target_row, target_col) = cursor_pos;
        
        if target_row < self.cursor_is_at_row {
            output.push_str(&format!("\x1b[{}A", self.cursor_is_at_row - target_row));
        } else if target_row > self.cursor_is_at_row && target_row < new_len {
            output.push_str(&format!("\x1b[{}B", target_row - self.cursor_is_at_row));
        }
        output.push_str("\r");
        if target_col > 0 {
            output.push_str(&format!("\x1b[{}C", target_col));
        }
        
        // Update where cursor ended up
        self.cursor_is_at_row = target_row;
        
        output.push_str("\x1b[?25h"); // Show cursor
        
        print!("{}", output);
        let _ = io::stdout().flush();
        
        self.previous_lines = new_lines;
    }

    pub fn force_clear(&mut self) {
        if std::env::var("TMUX").is_ok() {
            let _ = std::process::Command::new("tmux").arg("clear-history").status();
        }
        print!("\x1b[3J\x1b[2J\x1b[H");
        let _ = io::stdout().flush();
        self.previous_lines.clear();
        self.cursor_is_at_row = 0;
    }

    #[cfg(test)]
    pub fn previous_lines(&self) -> &[String] {
        &self.previous_lines
    }
    
    #[cfg(test)]
    pub fn cursor_pos(&self) -> CursorPos {
        (self.cursor_is_at_row, 0)
    }
}

impl Default for DiffRenderer {
    fn default() -> Self {
        Self::new()
    }
}
