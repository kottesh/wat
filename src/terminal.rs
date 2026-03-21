use std::io::{self, Write, Read};
use std::os::fd::AsFd;
use nix::sys::termios;
use anyhow::{Result, Context};
use colored::Colorize;

/// Manages terminal state for inline agent mode
pub struct TerminalState {
    original_termios: termios::Termios,
}

impl TerminalState {
    /// Create new terminal state manager
    pub fn new() -> Result<Self> {
        let stdin = std::io::stdin();
        let original_termios = termios::tcgetattr(stdin.as_fd())
            .context("Failed to get terminal attributes")?;
        
        Ok(Self {
            original_termios,
        })
    }
    
    /// Enter agent mode - takes over terminal input
    pub fn enter_agent_mode(&mut self) -> Result<()> {
        // Set raw mode
        let stdin = std::io::stdin();
        let mut raw_termios = self.original_termios.clone();
        termios::cfmakeraw(&mut raw_termios);
        termios::tcsetattr(stdin.as_fd(), termios::SetArg::TCSANOW, &raw_termios)
            .context("Failed to set terminal to raw mode")?;
        
        Ok(())
    }
    
    /// Exit agent mode - restore terminal to normal
    pub fn exit_agent_mode(&self) -> Result<()> {
        // Restore original terminal settings
        let stdin = std::io::stdin();
        termios::tcsetattr(stdin.as_fd(), termios::SetArg::TCSANOW, &self.original_termios)
            .context("Failed to restore terminal attributes")?;
        
        io::stdout().flush()?;
        
        Ok(())
    }
    
    /// Read a line of input from user with pi-style UI
    pub fn read_line(&self, _prompt: &str) -> Result<String> {
        // Get terminal width
        let width = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
        
        // Draw blue lines
        let line = "─".repeat(width).bright_blue();
        print!("\r\n{}\r\n", line);
        
        // Draw empty input line and bottom line
        print!("\r\n{}", line);
        
        // Move cursor back up to input line, at start
        print!("\x1b[1A\r");
        io::stdout().flush()?;
        
        let mut input = String::new();
        let mut buf = [0; 1];
        
        loop {
            io::stdin().read_exact(&mut buf)?;
            let c = buf[0] as char;
            
            match c {
                '\r' | '\n' => break,
                '\x7f' | '\x08' => {
                    if !input.is_empty() {
                        input.pop();
                        print!("\x08 \x08");
                        io::stdout().flush()?;
                    }
                }
                '\x03' => return Err(anyhow::anyhow!("Interrupted")),
                '\x04' => return Err(anyhow::anyhow!("EOF")),
                '\x1b' => {
                    // Escape sequence - skip it
                    let _ = io::stdin().read_exact(&mut buf);
                    let _ = io::stdin().read_exact(&mut buf);
                }
                _ if c.is_ascii() && !c.is_control() => {
                    input.push(c);
                    print!("{}", c);
                    io::stdout().flush()?;
                }
                _ => {}
            }
        }
        
        // Move to line after bottom border
        print!("\r\n\r\n");
        io::stdout().flush()?;
        
        Ok(input)
    }
}

/// Terminal utilities
pub mod utils {
    use std::io::{self, Write};
    use crossterm::terminal;
    
    /// Get terminal size
    pub fn terminal_size() -> Result<(u16, u16), io::Error> {
        terminal::size()
    }
    
    /// Clear from cursor to end of line
    pub fn clear_to_end_of_line() -> io::Result<()> {
        print!("\x1b[0K");
        io::stdout().flush()
    }
    
    /// Move cursor up N lines
    pub fn cursor_up(n: u16) -> io::Result<()> {
        if n > 0 {
            print!("\x1b[{}A", n);
            io::stdout().flush()
        } else {
            Ok(())
        }
    }
    
    /// Move cursor down N lines
    pub fn cursor_down(n: u16) -> io::Result<()> {
        if n > 0 {
            print!("\x1b[{}B", n);
            io::stdout().flush()
        } else {
            Ok(())
        }
    }
    
    /// Move cursor to specific position (1-indexed)
    pub fn cursor_to(row: u16, col: u16) -> io::Result<()> {
        print!("\x1b[{};{}H", row, col);
        io::stdout().flush()
    }
}