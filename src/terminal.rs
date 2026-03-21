//! Terminal state management for inline agent mode

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use nix::sys::termios;
use anyhow::{Result, Context};

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

    /// Enter raw mode for input reading
    pub fn enter_raw_mode(&mut self) -> Result<()> {
        let stdin = std::io::stdin();
        let mut raw_termios = self.original_termios.clone();
        termios::cfmakeraw(&mut raw_termios);
        termios::tcsetattr(stdin.as_fd(), termios::SetArg::TCSANOW, &raw_termios)
            .context("Failed to set terminal to raw mode")?;
        Ok(())
    }

    /// Exit raw mode and restore terminal
    pub fn exit_raw_mode(&self) -> Result<()> {
        let stdin = std::io::stdin();
        termios::tcsetattr(stdin.as_fd(), termios::SetArg::TCSANOW, &self.original_termios)
            .context("Failed to restore terminal attributes")?;
        io::stdout().flush()?;
        Ok(())
    }

    /// Read a line of input from user with pi-style UI (inline)
    pub fn read_line(&self, _prompt: &str) -> Result<String> {
        // Get terminal width
        let width = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(80);

        // Draw 3-line input area: top border, input line, bottom border
        let line = format!("\x1b[38;5;152m{}\x1b[0m", "─".repeat(width));
        print!("{}\r\n", line);      // Line 1: Top border
        print!("\r\n");               // Line 2: Empty input line
        print!("{}", line);           // Line 3: Bottom border (cursor here now)
        print!("\x1b[1A\r");          // Move up 1 line to input line, go to start
        io::stdout().flush()?;

        let mut input = String::new();
        let mut stdin = std::io::stdin();
        let mut buf = [0; 1];

        loop {
            stdin.read_exact(&mut buf)?;
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
                    let _ = stdin.read_exact(&mut buf);
                    let _ = stdin.read_exact(&mut buf);
                }
                _ if c.is_ascii() && !c.is_control() => {
                    input.push(c);
                    print!("{}", c);
                    io::stdout().flush()?;
                }
                _ => {}
            }
        }

        // Clear the 3-line prompt area before returning
        print!("\r\x1b[1A");    // Go up to top border (line 1)
        print!("\x1b[2K");      // Clear top border
        print!("\r\x1b[1B");    // Go down to input line (line 2)
        print!("\x1b[2K");      // Clear input line  
        print!("\r\x1b[1B");    // Go down to bottom border (line 3)
        print!("\x1b[2K");      // Clear bottom border
        print!("\r");           // Stay at start of this line
        io::stdout().flush()?;

        Ok(input)
    }
}

impl Drop for TerminalState {
    fn drop(&mut self) {
        let _ = self.exit_raw_mode();
    }
}


