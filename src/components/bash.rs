//! Bash execution output component

use crate::component::{Buffer, Cell, Color, Component, ComponentId, Modifiers};
use std::any::Any;

/// Status of bash command execution
#[derive(Debug, Clone, PartialEq)]
pub enum BashStatus {
    Running,
    Done { success: bool },
    Cancelled,
}

/// Component for displaying bash command execution
#[derive(Debug)]
pub struct BashComponent {
    #[allow(dead_code)] // Stored for future component tracking
    id: ComponentId,
    command: String,
    output_lines: Vec<String>,
    elapsed_secs: f64,
    status: BashStatus,
    show_all: bool,
    use_colors: bool,
}

impl BashComponent {
    /// Create a new bash component
    pub fn new(
        id: ComponentId,
        command: String,
        use_colors: bool,
    ) -> Self {
        Self {
            id,
            command,
            output_lines: Vec::new(),
            elapsed_secs: 0.0,
            status: BashStatus::Running,
            show_all: false,
            use_colors,
        }
    }

    /// Add an output line
    pub fn push_output(&mut self, line: String) {
        self.output_lines.push(line);
    }

    /// Update elapsed time
    pub fn set_elapsed(&mut self, secs: f64) {
        self.elapsed_secs = secs;
    }

    /// Mark as completed
    pub fn set_done(&mut self, success: bool) {
        self.status = BashStatus::Done { success };
    }

    /// Mark as cancelled
    pub fn set_cancelled(&mut self) {
        self.status = BashStatus::Cancelled;
    }

    /// Check if streaming (not complete)
    pub fn is_streaming(&self) -> bool {
        matches!(self.status, BashStatus::Running)
    }

    /// Render lines (non-Component version for backward compatibility)
    pub fn render_lines(&self, width: usize) -> Vec<String> {
        let max_lines = 50;
        let display_lines = if self.output_lines.len() > max_lines && !self.show_all {
            // Show the END
            self.output_lines[self.output_lines.len() - max_lines..].to_vec()
        } else {
            self.output_lines.clone()
        };

        if !self.use_colors {
            let mut lines = vec![format!("  $ {}", self.command)];
            for l in &display_lines {
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
        let pad = |s: &str| " ".repeat(width.saturating_sub(visible_width(s)));

        let mut lines = Vec::new();
        let empty = " ".repeat(width);

        let wrap_text = |text: &str| -> Vec<String> {
            let mut result = Vec::new();
            let mut current_line = String::new();
            let mut current_width = 0;

            for c in text.chars() {
                let cw = 1; // Simplification
                if current_width + cw > width {
                    result.push(current_line);
                    current_line = String::new();
                    current_width = 0;
                }
                current_line.push(c);
                current_width += cw;
            }
            if !current_line.is_empty() || result.is_empty() {
                result.push(current_line);
            }
            result
        };

        // top padding
        lines.push(format!("{}{}{}", bg, empty, reset));

        // command
        let cmd = format!("  $ {}", self.command);
        let wrapped_cmds = wrap_text(&cmd);
        for wc in wrapped_cmds {
            let cmd_padded = format!("{}{}", wc, pad(&wc));
            lines.push(format!("{}{}{}{}", bg, bold, cmd_padded, reset));
        }

        // gap
        lines.push(format!("{}{}{}", bg, empty, reset));

        // output lines
        for l in &display_lines {
            let content = format!("  {}", l);
            let wrapped_content = wrap_text(&content);
            for wc in wrapped_content {
                lines.push(format!("{}{}{}{}", bg, wc, pad(&wc), reset));
            }
        }

        // footer gap
        if !display_lines.is_empty() {
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

impl Component for BashComponent {
    fn render(&self, width: u16) -> Buffer {
        // For now, use the existing render_lines and convert to Buffer
        // This maintains compatibility with the current rendering system
        let lines = self.render_lines(width as usize);
        let height = lines.len() as u16;
        
        let mut buffer = Buffer::new(width, height);
        
        for (row, line) in lines.iter().enumerate() {
            let mut col = 0;
            for ch in line.chars() {
                if col >= width as usize {
                    break;
                }
                if row >= height as usize {
                    break;
                }
                
                buffer.cells[row][col] = Cell {
                    char: ch,
                    fg: Color::Default,
                    bg: Color::Default,
                    modifiers: Modifiers::default(),
                };
                col += 1;
            }
        }
        
        buffer
    }

    fn preferred_height(&self, width: u16) -> u16 {
        self.render_lines(width as usize).len() as u16
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn toggle_show_all(&mut self) -> bool {
        self.show_all = !self.show_all;
        true
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
    fn test_new_bash_component() {
        let comp = BashComponent::new(
            ComponentId(1),
            "echo test".to_string(),
            true,
        );
        
        assert_eq!(comp.command, "echo test");
        assert_eq!(comp.output_lines.len(), 0);
        assert_eq!(comp.elapsed_secs, 0.0);
        assert_eq!(comp.status, BashStatus::Running);
        assert!(comp.is_streaming());
    }

    #[test]
    fn test_push_output() {
        let mut comp = BashComponent::new(
            ComponentId(1),
            "ls".to_string(),
            false,
        );
        
        comp.push_output("file1.txt".to_string());
        comp.push_output("file2.txt".to_string());
        
        assert_eq!(comp.output_lines.len(), 2);
        assert_eq!(comp.output_lines[0], "file1.txt");
        assert_eq!(comp.output_lines[1], "file2.txt");
    }

    #[test]
    fn test_set_elapsed() {
        let mut comp = BashComponent::new(
            ComponentId(1),
            "sleep 1".to_string(),
            false,
        );
        
        comp.set_elapsed(1.5);
        assert_eq!(comp.elapsed_secs, 1.5);
    }

    #[test]
    fn test_set_done() {
        let mut comp = BashComponent::new(
            ComponentId(1),
            "true".to_string(),
            false,
        );
        
        comp.set_done(true);
        assert_eq!(comp.status, BashStatus::Done { success: true });
        assert!(!comp.is_streaming());
        
        comp.set_done(false);
        assert_eq!(comp.status, BashStatus::Done { success: false });
    }

    #[test]
    fn test_set_cancelled() {
        let mut comp = BashComponent::new(
            ComponentId(1),
            "sleep 100".to_string(),
            false,
        );
        
        comp.set_cancelled();
        assert_eq!(comp.status, BashStatus::Cancelled);
        assert!(!comp.is_streaming());
    }

    #[test]
    fn test_render_lines_no_color() {
        let mut comp = BashComponent::new(
            ComponentId(1),
            "echo hello".to_string(),
            false,
        );
        
        comp.push_output("hello".to_string());
        comp.set_elapsed(0.1);
        comp.set_done(true);
        
        let lines = comp.render_lines(80);
        
        assert_eq!(lines[0], "  $ echo hello");
        assert_eq!(lines[1], "  hello");
        assert_eq!(lines[2], "  Took 0.1s");
    }

    #[test]
    fn test_toggle_show_all() {
        let mut comp = BashComponent::new(
            ComponentId(1),
            "test".to_string(),
            false,
        );
        
        assert!(!comp.show_all);
        comp.toggle_show_all();
        assert!(comp.show_all);
        comp.toggle_show_all();
        assert!(!comp.show_all);
    }

    #[test]
    fn test_visible_width() {
        assert_eq!(visible_width("hello"), 5);
        assert_eq!(visible_width("\x1b[31mred\x1b[0m"), 3);
        assert_eq!(visible_width(""), 0);
    }

    #[test]
    fn test_render_component() {
        let mut comp = BashComponent::new(
            ComponentId(1),
            "test".to_string(),
            false,
        );
        
        comp.push_output("output".to_string());
        comp.set_done(true);
        
        let buffer = comp.render(80);
        assert_eq!(buffer.width, 80);
        assert!(buffer.height > 0);
    }

    #[test]
    fn test_preferred_height() {
        let mut comp = BashComponent::new(
            ComponentId(1),
            "test".to_string(),
            false,
        );
        
        comp.push_output("line1".to_string());
        
        let height = comp.preferred_height(80);
        assert!(height > 0);
    }
}
