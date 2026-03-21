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
            let empty_line = " ".repeat(width);
            println!("{}{}{}", bg, empty_line, reset);
            
            // Add user input with padding
            let visible = format!("  {}  ", input);
            let padding = width.saturating_sub(visible.len());
            println!("{}{}{}{}", bg, visible, " ".repeat(padding), reset);
            
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
            let padding = width.saturating_sub(line.len());
            if self.use_colors {
                println!("{}{}{}{}", bg, line, " ".repeat(padding), reset);
            } else {
                println!("{}", line);
            }
        }
        println!();
        io::stdout().flush()
    }
    
    /// Render tool call
    pub fn render_tool_call(&mut self, tool: &str, args: &str) -> io::Result<()> {
        let width = self.width();
        
        if tool == "bash" {
            // Don't render anything for bash - will be rendered with result
            // so we can apply success/failure color to the whole block
        } else if tool == "read_file" {
            // read_file tool - just the header, background continues into result
            if self.use_colors {
                // Very subtle gray background (235 = ~5% opacity gray)
                let bg = "\x1b[48;5;235m";
                let reset = "\x1b[0m";
                let bold = "\x1b[1m";
                let no_bold = "\x1b[22m"; // Reset bold only, keep other attributes
                
                // Full width empty line for top padding
                let empty_line = " ".repeat(width);
                println!("{}{}{}", bg, empty_line, reset);
                
                // "Read" in bold, then filename - use manual codes to avoid reset from colored crate
                // Visual text is "  Read filename" (2 + 4 + 1 + filename_len)
                let visible_text = format!("  Read {}", args);
                let padding = width.saturating_sub(visible_text.len());
                
                // Print with bold on "Read"
                print!("{}", bg);
                print!("  {}Read{}", bold, no_bold);
                print!(" {}", args);
                print!("{}", " ".repeat(padding));
                println!("{}", reset);
            } else {
                println!();
                println!("  Read {}", args);
            }
        } else {
            // Other tools - subtle gray background
            if self.use_colors {
                let bg = "\x1b[48;5;235m";
                let reset = "\x1b[0m";
                let content = format!(" {} {}", tool, args);
                print!("{}{}{}", bg, content, reset);
            } else {
                print!(" {} {}", tool, args);
            }
            println!();
            println!(); // Extra spacing
        }
        io::stdout().flush()
    }
    
    /// Render tool result with truncation and timing
    #[allow(dead_code)]
    pub fn render_tool_result(&mut self, result: &str) -> io::Result<()> {
        self.render_tool_result_with_timing(result, None, None, true, None)
    }
    
    /// Render tool result with optional timing, tool type, and success status for styling
    pub fn render_tool_result_with_timing(&mut self, result: &str, duration: Option<f64>, tool: Option<&str>, success: bool, command: Option<&str>) -> io::Result<()> {
        let width = self.width();
        let lines: Vec<&str> = result.lines().collect();
        let max_lines = 50;
        
        // Check if this is a read_file result - needs continuous background
        let is_read_file = tool == Some("read_file");
        let is_bash = tool == Some("bash");
        
        if is_read_file && self.use_colors {
            let bg = "\x1b[48;5;235m";
            let reset = "\x1b[0m";
            
            // Empty line separator between header and content
            let empty_line = " ".repeat(width);
            println!("{}{}{}", bg, empty_line, reset);
            
            // Show content with background
            if lines.len() > max_lines {
                for line in &lines[lines.len()-max_lines..] {
                    let visible = format!("  {}", line);
                    let padding = width.saturating_sub(visible.len());
                    println!("{}{}{}{}", bg, visible, " ".repeat(padding), reset);
                }
            } else {
                for line in &lines {
                    let visible = format!("  {}", line);
                    let padding = width.saturating_sub(visible.len());
                    println!("{}{}{}{}", bg, visible, " ".repeat(padding), reset);
                }
            }
            
            // Empty line at bottom
            println!("{}{}{}", bg, empty_line, reset);
            
            // Show timing if provided
            if let Some(duration) = duration {
                if self.use_colors {
                    println!(" {} {:.1}s", "Took".dimmed(), duration);
                } else {
                    println!(" Took {:.1}s", duration);
                }
            }
        } else if is_bash && self.use_colors {
            // Bash result with success/failure background hint
            // Base colors: #283228 (green), #CC6666 (red)
            // Blended with slightly higher visibility
            let bg = if success { "\x1b[48;2;30;38;30m" } else { "\x1b[48;2;60;30;30m" };
            let reset = "\x1b[0m";
            let bold = "\x1b[1m";
            let no_bold = "\x1b[22m";
            
            // Empty line at top
            let empty_line = " ".repeat(width);
            println!("{}{}{}", bg, empty_line, reset);
            
            // Show command header with $ prefix (bold command)
            if let Some(cmd) = command {
                let header = format!("  $ {}", cmd);
                let padding = width.saturating_sub(header.len());
                println!("{}  $ {}{}{}{}", bg, bold, cmd, no_bold, " ".repeat(padding));
                print!("{}", reset);
                // Empty line after command
                println!("{}{}{}", bg, empty_line, reset);
            }
            
            // Show content with background (with left/right padding)
            if lines.len() > max_lines {
                for line in &lines[lines.len()-max_lines..] {
                    let visible = format!("  {}  ", line);
                    let padding = width.saturating_sub(visible.len());
                    println!("{}{}{}{}", bg, visible, " ".repeat(padding), reset);
                }
            } else {
                for line in &lines {
                    let visible = format!("  {}  ", line);
                    let padding = width.saturating_sub(visible.len());
                    println!("{}{}{}{}", bg, visible, " ".repeat(padding), reset);
                }
            }
            
            // Empty line before timing
            if duration.is_some() {
                println!("{}{}{}", bg, empty_line, reset);
            }
            
            // Show timing if provided (with left/right padding)
            if let Some(duration) = duration {
                let timing_text = format!("  {:.1}s  ", duration);
                let padding = width.saturating_sub(timing_text.len());
                println!("{}{}{}{}", bg, timing_text, " ".repeat(padding), reset);
            }
            
            // Empty line at bottom
            println!("{}{}{}", bg, empty_line, reset);
        } else {
            // Regular result rendering (no colors or other tools)
            if lines.len() > max_lines {
                for line in &lines[lines.len()-max_lines..] {
                    if self.use_colors {
                        println!("{}", line.dimmed());
                    } else {
                        println!("{}", line);
                    }
                }
            } else {
                for line in &lines {
                    if self.use_colors {
                        println!("{}", line.dimmed());
                    } else {
                        println!("{}", line);
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
