use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use colored::Colorize;
use termimad::MadSkin;

/// Braille spinner frames
const BRAILLE_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Renders agent output inline in the terminal
pub struct InlineRenderer {
    use_colors: bool,
    skin: MadSkin,
}

impl InlineRenderer {
    /// Create new renderer
    pub fn new(use_colors: bool) -> Self {
        let mut skin = MadSkin::default();
        
        if use_colors {
            // Customize markdown rendering colors
            skin.bold.set_fg(termimad::crossterm::style::Color::White);
            skin.italic.set_fg(termimad::crossterm::style::Color::Cyan);
            skin.code_block.set_fg(termimad::crossterm::style::Color::Green);
            skin.inline_code.set_fg(termimad::crossterm::style::Color::Green);
            skin.headers[0].set_fg(termimad::crossterm::style::Color::Yellow);
            skin.headers[1].set_fg(termimad::crossterm::style::Color::Yellow);
        }
        
        Self { use_colors, skin }
    }
    
    /// Update skin for current terminal size
    #[allow(dead_code)]
    fn update_skin_for_terminal(&mut self) {
        // The skin will automatically use the current terminal size
        // when we call area_text with a fresh Area
    }
    
    /// Get current terminal width (recalculated each time)
    fn width(&self) -> usize {
        // Try termimad first
        let (w, _) = termimad::terminal_size();
        w as usize
    }
    
    /// Start a braille spinner with message
    pub fn start_spinner(&self, message: &str) -> SpinnerHandle {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let msg = message.to_string();
        let use_colors = self.use_colors;
        
        let handle = thread::spawn(move || {
            let mut frame_idx = 0;
            while running_clone.load(Ordering::Relaxed) {
                let frame = BRAILLE_FRAMES[frame_idx % BRAILLE_FRAMES.len()];
                let text = if use_colors {
                    format!("\r{} {}", frame.bright_cyan(), msg.dimmed())
                } else {
                    format!("\r{} {}", frame, msg)
                };
                print!("{}", text);
                let _ = io::stdout().flush();
                frame_idx += 1;
                thread::sleep(Duration::from_millis(80));
            }
            print!("\r\x1b[K");
            let _ = io::stdout().flush();
        });
        
        SpinnerHandle { running, handle: Some(handle) }
    }
    
    /// Render user input with background and padding
    pub fn render_user_input(&mut self, input: &str) -> io::Result<()> {
        let width = self.width();
        
        if self.use_colors {
            // Very subtle dark grey background (235 = very dark grey, subtle like 5% opacity)
            let bg = "\x1b[48;5;235m";
            let reset = "\x1b[0m";
            
            // Add padding line
            let empty_line = format!("{:<width$}", "", width = width);
            println!("{}{}{}", bg, empty_line, reset);
            
            // Add user input with padding
            let padded_input = format!("  {}  ", input);
            let full_line = format!("{:<width$}", padded_input, width = width);
            println!("{}{}{}", bg, full_line.white(), reset);
            
            // Add padding line
            println!("{}{}{}", bg, empty_line, reset);
        } else {
            // Plain text fallback with just padding
            println!();
            println!("  {}  ", input);
            println!();
        }
        
        println!();
        io::stdout().flush()
    }
    
    /// Render thinking block with background
    #[allow(dead_code)]
    pub fn render_thinking(&mut self, text: &str) -> io::Result<()> {
        let width = self.width();
        let bg = "\x1b[48;5;238m";
        let reset = "\x1b[0m";
        
        for line in text.lines() {
            let padded = format!("{:<width$}", line, width = width);
            if self.use_colors {
                println!("{}{}{}", bg, padded.dimmed(), reset);
            } else {
                println!("{}", line);
            }
        }
        println!();
        io::stdout().flush()
    }
    
    /// Render tool call as command with $ prefix and background
    pub fn render_tool_call(&mut self, tool: &str, args: &str) -> io::Result<()> {
        let width = self.width();
        
        if tool == "bash" {
            if self.use_colors {
                // Light teal background for tool calls (152 = closest to #8ABEB7)
                let bg = "\x1b[48;5;152m";
                let reset = "\x1b[0m";
                
                // Show as terminal command with background
                let command = format!(" {} {}", "$".green().bold(), args.black());
                let full_line = format!("{:<width$}", command, width = width);
                println!("{}{}{}", bg, full_line, reset);
            } else {
                println!(" $ {}", args);
            }
            println!();
        } else {
            // Other tools (if any)
            if self.use_colors {
                let bg = "\x1b[48;5;152m";
                let reset = "\x1b[0m";
                let content = format!(" {} {}", tool, args);
                let full_line = format!("{:<width$}", content, width = width);
                println!("{}{}{}", bg, full_line, reset);
            } else {
                println!(" {} {}", tool, args);
            }
            println!();
        }
        io::stdout().flush()
    }
    
    /// Render tool result with truncation and timing
    #[allow(dead_code)]
    pub fn render_tool_result(&mut self, result: &str) -> io::Result<()> {
        self.render_tool_result_with_timing(result, None)
    }
    
    /// Render tool result with optional timing
    pub fn render_tool_result_with_timing(&mut self, result: &str, duration: Option<f64>) -> io::Result<()> {
        let lines: Vec<&str> = result.lines().collect();
        let max_lines = 50;
        
        if lines.len() > max_lines {
            // Show truncation message
            let hidden_count = lines.len() - max_lines;
            if self.use_colors {
                println!(" {} ({} earlier lines, ctrl+o to expand)", 
                    "...".dimmed(), 
                    hidden_count.to_string().dimmed()
                );
            } else {
                println!(" ... ({} earlier lines, ctrl+o to expand)", hidden_count);
            }
            
            // Show last max_lines
            for line in &lines[lines.len()-max_lines..] {
                if self.use_colors {
                    println!(" {}", line.dimmed());
                } else {
                    println!(" {}", line);
                }
            }
        } else {
            // Show all lines
            for line in &lines {
                if self.use_colors {
                    println!(" {}", line.dimmed());
                } else {
                    println!(" {}", line);
                }
            }
        }
        
        // Show timing if provided
        if let Some(duration) = duration {
            println!();
            if self.use_colors {
                println!(" {} {:.1}s", "Took".dimmed(), duration);
            } else {
                println!(" Took {:.1}s", duration);
            }
        }
        
        println!();
        io::stdout().flush()
    }
    
    /// Render agent response with markdown support and padding (no background)
    pub fn render_response(&mut self, text: &str) -> io::Result<()> {
        if self.use_colors {
            // Get fresh width for current screen size
            let width = self.width();
            // Render markdown with current terminal width
            let rendered = self.skin.area_text(text, &termimad::Area::new(0, 0, width as u16, 100));
            let rendered_str = format!("{}", rendered);
            
            // Add padding to each line
            for line in rendered_str.lines() {
                println!("  {}  ", line);
            }
        } else {
            // Plain text with padding
            for line in text.lines() {
                println!("  {}  ", line);
            }
        }
        
        // Just one blank line after response
        println!();
        io::stdout().flush()
    }
    
    /// Render error
    pub fn render_error(&mut self, error: &str) -> io::Result<()> {
        if self.use_colors {
            println!("{} {}", "error:".bright_red().bold(), error.bright_red());
        } else {
            println!("error: {}", error);
        }
        io::stdout().flush()
    }
    
    /// Render shell prompt
    #[allow(dead_code)]
    pub fn render_shell_prompt(&mut self) -> io::Result<()> {
        println!();
        io::stdout().flush()
    }
}

/// Handle to control a running spinner
pub struct SpinnerHandle {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl SpinnerHandle {
    pub fn stop(mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for SpinnerHandle {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
