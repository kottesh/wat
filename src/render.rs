use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use colored::Colorize;

/// Braille spinner frames
const BRAILLE_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Renders agent output inline in the terminal
pub struct InlineRenderer {
    use_colors: bool,
}

impl InlineRenderer {
    /// Create new renderer
    pub fn new(use_colors: bool) -> Self {
        Self { use_colors }
    }
    
    /// Get terminal width
    fn width(&self) -> usize {
        crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(80)
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
    
    /// Render user input (plain text)
    pub fn render_user_input(&mut self, input: &str) -> io::Result<()> {
        println!("{}", input);
        println!();
        io::stdout().flush()
    }
    
    /// Render thinking block with background
    pub fn render_thinking(&mut self, text: &str) -> io::Result<()> {
        let width = self.width();
        // Dark gray background (238)
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
    
    /// Render tool call with background
    pub fn render_tool_call(&mut self, tool: &str, args: &str) -> io::Result<()> {
        let width = self.width();
        let content = format!("{}: {}", tool, args);
        let bg = "\x1b[48;5;238m";
        let reset = "\x1b[0m";
        
        let padded = format!("{:<width$}", content, width = width);
        if self.use_colors {
            println!("{}{}{}", bg, padded.dimmed(), reset);
        } else {
            println!("{}", content);
        }
        io::stdout().flush()
    }
    
    /// Render tool result
    pub fn render_tool_result(&mut self, result: &str) -> io::Result<()> {
        for line in result.lines().take(10) {
            println!("  {}", line.dimmed());
        }
        println!();
        io::stdout().flush()
    }
    
    /// Render agent response (plain white text)
    pub fn render_response(&mut self, text: &str) -> io::Result<()> {
        for line in text.lines() {
            println!("{}", line);
        }
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
