//! Multi-line text editor with undo/redo support

/// Snapshot of editor state for undo/redo
#[derive(Debug, Clone)]
struct EditorSnapshot {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

/// Multi-line text editor
pub struct Editor {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    undo_stack: Vec<EditorSnapshot>,
    redo_stack: Vec<EditorSnapshot>,
    max_undo_history: usize,
}

impl Editor {
    /// Create a new editor with empty content
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_undo_history: 100,
        }
    }

    /// Get current lines
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Get cursor position (row, col)
    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    /// Get full content as single string
    pub fn content(&self) -> String {
        self.lines.join("\n")
    }

    /// Clear all content
    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Render with optional spinner and hint
    pub fn render(&self, width: u16, use_colors: bool) -> (Vec<String>, usize, usize) {
        let (lines, (cursor_row, cursor_col)) = self.render_with_border(width, use_colors);
        (lines, cursor_row, cursor_col)
    }

    /// Save current state to undo stack
    fn save_undo(&mut self) {
        self.undo_stack.push(EditorSnapshot {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        });
        self.redo_stack.clear();

        // Limit undo history size
        if self.undo_stack.len() > self.max_undo_history {
            self.undo_stack.remove(0);
        }
    }

    /// Undo last change
    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(EditorSnapshot {
                lines: self.lines.clone(),
                cursor_row: self.cursor_row,
                cursor_col: self.cursor_col,
            });
            self.lines = prev.lines;
            self.cursor_row = prev.cursor_row;
            self.cursor_col = prev.cursor_col;
        }
    }

    /// Redo last undone change
    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(EditorSnapshot {
                lines: self.lines.clone(),
                cursor_row: self.cursor_row,
                cursor_col: self.cursor_col,
            });
            self.lines = next.lines;
            self.cursor_row = next.cursor_row;
            self.cursor_col = next.cursor_col;
        }
    }

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, c: char) {
        self.save_undo();
        let row = self.cursor_row;
        let col = self.cursor_col;
        self.lines[row].insert(col, c);
        self.cursor_col += 1;
    }

    /// Delete character before cursor (backspace)
    pub fn delete_char(&mut self) {
        self.save_undo();
        let row = self.cursor_row;
        let col = self.cursor_col;

        if col > 0 {
            // Delete character on current line
            self.lines[row].remove(col - 1);
            self.cursor_col -= 1;
        } else if row > 0 {
            // Merge current line with previous line
            let current_line = self.lines.remove(row);
            let prev_row = row - 1;
            let prev_len = self.lines[prev_row].len();
            self.lines[prev_row].push_str(&current_line);
            self.cursor_row = prev_row;
            self.cursor_col = prev_len;
        }
    }

    /// Insert newline at cursor position
    pub fn insert_newline(&mut self) {
        self.save_undo();
        let row = self.cursor_row;
        let col = self.cursor_col;
        let line = &mut self.lines[row];
        let remaining = line.split_off(col);
        self.lines.insert(row + 1, remaining);
        self.cursor_row += 1;
        self.cursor_col = 0;
    }

    /// Insert string at cursor position
    pub fn insert_str(&mut self, s: &str) {
        self.save_undo();
        let row = self.cursor_row;
        let col = self.cursor_col;
        self.lines[row].insert_str(col, s);
        self.cursor_col += s.len();
    }

    /// Move cursor up
    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
        }
    }

    /// Move cursor down
    pub fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        if self.cursor_col < self.lines[self.cursor_row].len() {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    /// Take content and reset editor
    pub fn take_content(&mut self) -> String {
        let result = self.lines.join("\n");
        self.lines = vec![String::new()];
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.undo_stack.clear();
        self.redo_stack.clear();
        result
    }

    /// Render editor with borders and return (lines, cursor_position)
    pub fn render_with_border(
        &self,
        width: u16,
        use_colors: bool,
    ) -> (Vec<String>, (usize, usize)) {
        let mut lines = Vec::new();

        // Create border
        let border_str = "─".repeat(width.saturating_sub(1) as usize);
        let border = if use_colors {
            format!("\x1b[38;5;152m{}\x1b[0m", border_str)
        } else {
            border_str
        };

        lines.push(border.clone()); // top border

        let mut cursor_row = 0;
        let mut cursor_col = 0;

        let input_width = (width as usize).saturating_sub(2); // "> " prefix

        if input_width == 0 {
            // Fallback for extremely narrow terminals
            for (i, line) in self.lines.iter().enumerate() {
                lines.push(format!("> {}", line));
                if i == self.cursor_row {
                    cursor_row = lines.len() - 1;
                    let prefix: String = line.chars().take(self.cursor_col).collect();
                    cursor_col = visible_width(&prefix) + 2;
                }
            }
        } else {
            // Render with wrapping
            for (i, line) in self.lines.iter().enumerate() {
                let is_cursor_line = i == self.cursor_row;

                if line.is_empty() {
                    lines.push("> ".to_string());
                    if is_cursor_line {
                        cursor_row = lines.len() - 1;
                        cursor_col = 2;
                    }
                    continue;
                }

                // Wrap the line
                let mut current_visual_line = String::new();
                let mut current_visual_width = 0;
                let mut char_idx = 0;

                let mut visual_lines = Vec::new();
                let mut cursor_mapped = false;

                for c in line.chars() {
                    if is_cursor_line && char_idx == self.cursor_col {
                        cursor_row = lines.len() + visual_lines.len();
                        cursor_col = current_visual_width + 2;
                        cursor_mapped = true;
                    }

                    // Simple character width (assuming 1 for now)
                    let cw = 1;

                    if current_visual_width + cw > input_width {
                        visual_lines.push(current_visual_line.clone());
                        current_visual_line.clear();
                        current_visual_width = 0;
                    }

                    current_visual_line.push(c);
                    current_visual_width += cw;
                    char_idx += 1;
                }

                if is_cursor_line && !cursor_mapped {
                    // Cursor is at the very end of the logical line
                    if current_visual_width >= input_width {
                        // Cursor needs to wrap to the next visual line
                        visual_lines.push(current_visual_line.clone());
                        current_visual_line.clear();
                        cursor_row = lines.len() + visual_lines.len();
                        cursor_col = 2;
                    } else {
                        cursor_row = lines.len() + visual_lines.len();
                        cursor_col = current_visual_width + 2;
                    }
                }

                if !current_visual_line.is_empty() {
                    visual_lines.push(current_visual_line);
                }

                for vl in visual_lines {
                    lines.push(format!("> {}", vl));
                }
            }
        }

        lines.push(border); // bottom border

        (lines, (cursor_row, cursor_col))
    }

    /// Check if editor is empty
    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
    }

    /// Get total number of lines
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate visible width of a string (stripping ANSI escape codes)
fn visible_width(s: &str) -> usize {
    let mut width = 0;
    let mut in_esc = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_esc = true;
            continue;
        }
        if in_esc {
            if c.is_ascii_alphabetic() {
                in_esc = false;
            }
            continue;
        }
        width += 1;
    }
    width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_editor() {
        let editor = Editor::new();
        assert_eq!(editor.lines(), &[String::new()]);
        assert_eq!(editor.cursor(), (0, 0));
        assert!(editor.is_empty());
    }

    #[test]
    fn test_insert_char() {
        let mut editor = Editor::new();
        editor.insert_char('h');
        editor.insert_char('i');
        assert_eq!(editor.lines(), &["hi"]);
        assert_eq!(editor.cursor(), (0, 2));
    }

    #[test]
    fn test_delete_char() {
        let mut editor = Editor::new();
        editor.insert_char('h');
        editor.insert_char('i');
        editor.delete_char();
        assert_eq!(editor.lines(), &["h"]);
        assert_eq!(editor.cursor(), (0, 1));
    }

    #[test]
    fn test_insert_newline() {
        let mut editor = Editor::new();
        editor.insert_char('h');
        editor.insert_char('i');
        editor.insert_newline();
        editor.insert_char('b');
        editor.insert_char('y');
        assert_eq!(editor.lines(), &["hi", "by"]);
        assert_eq!(editor.cursor(), (1, 2));
    }

    #[test]
    fn test_delete_at_line_start() {
        let mut editor = Editor::new();
        editor.insert_char('h');
        editor.insert_char('i');
        editor.insert_newline();
        editor.insert_char('b');
        editor.delete_char(); // Delete 'b'
        editor.delete_char(); // Merge with previous line
        assert_eq!(editor.lines(), &["hi"]);
        assert_eq!(editor.cursor(), (0, 2));
    }

    #[test]
    fn test_move_cursor() {
        let mut editor = Editor::new();
        editor.insert_str("hello");
        editor.insert_newline();
        editor.insert_str("world");

        // Currently at (1, 5) - end of "world"
        assert_eq!(editor.cursor(), (1, 5));

        editor.move_left();
        assert_eq!(editor.cursor(), (1, 4));

        editor.move_up();
        assert_eq!(editor.cursor(), (0, 4));

        editor.move_right();
        assert_eq!(editor.cursor(), (0, 5));

        editor.move_down();
        assert_eq!(editor.cursor(), (1, 5));
    }

    #[test]
    fn test_move_across_lines() {
        let mut editor = Editor::new();
        editor.insert_str("hi");
        editor.insert_newline();
        editor.insert_str("bye");

        // At (1, 3)
        editor.move_left();
        editor.move_left();
        editor.move_left();
        // Should be at (1, 0)
        assert_eq!(editor.cursor(), (1, 0));

        editor.move_left();
        // Should wrap to end of previous line (0, 2)
        assert_eq!(editor.cursor(), (0, 2));
    }

    #[test]
    fn test_undo_redo() {
        let mut editor = Editor::new();
        editor.insert_char('a');
        editor.insert_char('b');
        assert_eq!(editor.lines(), &["ab"]);

        editor.undo();
        assert_eq!(editor.lines(), &["a"]);

        editor.undo();
        assert_eq!(editor.lines(), &[""]);

        editor.redo();
        assert_eq!(editor.lines(), &["a"]);

        editor.redo();
        assert_eq!(editor.lines(), &["ab"]);
    }

    #[test]
    fn test_undo_clears_redo() {
        let mut editor = Editor::new();
        editor.insert_char('a');
        editor.insert_char('b');
        editor.undo();
        // Now redo stack has one item
        editor.insert_char('c');
        // Redo stack should be cleared
        editor.redo();
        // Should not change (redo stack is empty)
        assert_eq!(editor.lines(), &["ac"]);
    }

    #[test]
    fn test_take_content() {
        let mut editor = Editor::new();
        editor.insert_str("line1");
        editor.insert_newline();
        editor.insert_str("line2");

        let content = editor.take_content();
        assert_eq!(content, "line1\nline2");
        assert_eq!(editor.lines(), &[String::new()]);
        assert_eq!(editor.cursor(), (0, 0));
        assert!(editor.is_empty());
    }

    #[test]
    fn test_insert_str() {
        let mut editor = Editor::new();
        editor.insert_str("hello world");
        assert_eq!(editor.lines(), &["hello world"]);
        assert_eq!(editor.cursor(), (0, 11));
    }

    #[test]
    fn test_render_simple() {
        let mut editor = Editor::new();
        editor.insert_str("test");
        let (lines, cursor) = editor.render_with_border(80, false);

        assert_eq!(lines.len(), 3); // top border + content + bottom border
        assert!(lines[0].contains("─"));
        assert_eq!(lines[1], "> test");
        assert!(lines[2].contains("─"));
        assert_eq!(cursor, (1, 6)); // row 1, col 6 ("> test" -> cursor after 't')
    }

    #[test]
    fn test_render_multiline() {
        let mut editor = Editor::new();
        editor.insert_str("line1");
        editor.insert_newline();
        editor.insert_str("line2");
        let (lines, cursor) = editor.render_with_border(80, false);

        assert_eq!(lines.len(), 4); // top border + 2 lines + bottom border
        assert_eq!(lines[1], "> line1");
        assert_eq!(lines[2], "> line2");
        assert_eq!(cursor, (2, 7)); // After "line2"
    }

    #[test]
    fn test_visible_width() {
        assert_eq!(visible_width("hello"), 5);
        assert_eq!(visible_width("\x1b[31mred\x1b[0m"), 3);
        assert_eq!(visible_width(""), 0);
    }

    #[test]
    fn test_line_count() {
        let mut editor = Editor::new();
        assert_eq!(editor.line_count(), 1);

        editor.insert_newline();
        assert_eq!(editor.line_count(), 2);

        editor.insert_newline();
        assert_eq!(editor.line_count(), 3);
    }
}
