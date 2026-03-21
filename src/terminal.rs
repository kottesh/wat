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

    /// Read a single keypress
    pub fn read_key(&self) -> Result<KeyEvent> {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 16];

        let n = stdin.read(&mut buf)?;
        if n == 0 {
            return Ok(KeyEvent::Eof);
        }

        parse_key_event(&buf[..n])
    }

    /// Get terminal size
    pub fn size(&self) -> (u16, u16) {
        crossterm::terminal::size().unwrap_or((80, 24))
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
        // Cursor is on input line (line 2 of 3), after Enter
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

/// Key event from terminal
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyEvent {
    Char(char),
    Backspace,
    Delete,
    Enter,
    Escape,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Ctrl(char),
    Alt(char),
    CtrlC,
    CtrlD,
    CtrlL,
    Eof,
    Unknown(Vec<u8>),
}

/// Parse a key event from raw bytes
fn parse_key_event(bytes: &[u8]) -> Result<KeyEvent> {
    if bytes.is_empty() {
        return Ok(KeyEvent::Unknown(vec![]));
    }

    // Check for escape sequences
    if bytes[0] == 0x1b {
        if bytes.len() == 1 {
            return Ok(KeyEvent::Escape);
        }

        // Parse escape sequence
        match bytes.len() {
            2 => {
                // Alt + key
                if bytes[1].is_ascii() && !bytes[1].is_ascii_control() {
                    return Ok(KeyEvent::Alt(bytes[1] as char));
                }
            }
            3 => {
                match &bytes[1..] {
                    b"[A" => return Ok(KeyEvent::Up),
                    b"[B" => return Ok(KeyEvent::Down),
                    b"[C" => return Ok(KeyEvent::Right),
                    b"[D" => return Ok(KeyEvent::Left),
                    b"[H" => return Ok(KeyEvent::Home),
                    b"[F" => return Ok(KeyEvent::End),
                    b"[Z" => return Ok(KeyEvent::Tab), // Shift-Tab
                    _ => {}
                }
            }
            4 => {
                match &bytes[1..] {
                    b"[1~" => return Ok(KeyEvent::Home),
                    b"[4~" => return Ok(KeyEvent::End),
                    b"[5~" => return Ok(KeyEvent::PageUp),
                    b"[6~" => return Ok(KeyEvent::PageDown),
                    b"[3~" => return Ok(KeyEvent::Delete),
                    _ => {}
                }
            }
            _ => {}
        }

        return Ok(KeyEvent::Unknown(bytes.to_vec()));
    }

    // Control characters
    match bytes[0] {
        0x03 => return Ok(KeyEvent::CtrlC),
        0x04 => return Ok(KeyEvent::CtrlD),
        0x0c => return Ok(KeyEvent::CtrlL),
        0x08 | 0x7f => return Ok(KeyEvent::Backspace),
        0x0d | 0x0a => return Ok(KeyEvent::Enter),
        0x09 => return Ok(KeyEvent::Tab),
        0x01..=0x1a => {
            // Ctrl + letter
            let c = (bytes[0] - 0x01 + b'a') as char;
            return Ok(KeyEvent::Ctrl(c));
        }
        _ => {}
    }

    // Single character
    if bytes.len() == 1 && bytes[0].is_ascii() {
        return Ok(KeyEvent::Char(bytes[0] as char));
    }

    // UTF-8 character
    if let Ok(s) = std::str::from_utf8(bytes) {
        if let Some(c) = s.chars().next() {
            return Ok(KeyEvent::Char(c));
        }
    }

    Ok(KeyEvent::Unknown(bytes.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_enter() {
        assert_eq!(parse_key_event(&[0x0d]).unwrap(), KeyEvent::Enter);
    }

    #[test]
    fn test_parse_key_char() {
        assert_eq!(parse_key_event(&[b'a']).unwrap(), KeyEvent::Char('a'));
    }

    #[test]
    fn test_parse_key_ctrl_c() {
        assert_eq!(parse_key_event(&[0x03]).unwrap(), KeyEvent::CtrlC);
    }

    #[test]
    fn test_parse_key_escape() {
        assert_eq!(parse_key_event(&[0x1b]).unwrap(), KeyEvent::Escape);
    }

    #[test]
    fn test_parse_key_up() {
        assert_eq!(parse_key_event(&[0x1b, b'[', b'A']).unwrap(), KeyEvent::Up);
    }
}
