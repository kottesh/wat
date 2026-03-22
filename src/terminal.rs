//! Terminal state management

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use nix::sys::termios;
use anyhow::{Result, Context};

/// Result of a read_line call.
pub enum ReadResult {
    /// User pressed Enter with this input.
    Input(String),
    /// User pressed Escape (abort signal).
    Escape,
}

/// Manages terminal raw mode.
pub struct TerminalState {
    original_termios: termios::Termios,
}

impl TerminalState {
    pub fn new() -> Result<Self> {
        let stdin = std::io::stdin();
        let original_termios = termios::tcgetattr(stdin.as_fd())
            .context("Failed to get terminal attributes")?;
        Ok(Self { original_termios })
    }

    pub fn enter_raw_mode(&mut self) -> Result<()> {
        let stdin = std::io::stdin();
        let mut raw = self.original_termios.clone();
        termios::cfmakeraw(&mut raw);
        termios::tcsetattr(stdin.as_fd(), termios::SetArg::TCSANOW, &raw)
            .context("Failed to set terminal to raw mode")?;
        Ok(())
    }

    pub fn exit_raw_mode(&self) -> Result<()> {
        let stdin = std::io::stdin();
        termios::tcsetattr(stdin.as_fd(), termios::SetArg::TCSANOW, &self.original_termios)
            .context("Failed to restore terminal attributes")?;
        io::stdout().flush()?;
        Ok(())
    }

    /// Read a line of input from the already-drawn input box.
    ///
    /// The renderer has already drawn the 4-line input box and the cursor is
    /// sitting *after* the bottom border.  This function:
    ///   1. Moves cursor up 2 lines to the input line.
    ///   2. Reads characters, echoing each one.
    ///   3. On Enter  → moves cursor back down 2 (to after-bottom-border) and
    ///                   returns `ReadResult::Input`.
    ///   4. On ESC    → returns `ReadResult::Escape` (cursor restored).
    ///   5. On Ctrl-C / Ctrl-D → returns Err (triggers shutdown).
    pub fn read_line(&self) -> Result<ReadResult> {
        // Position cursor on the input line
        // From after-bottom-border: up 2 = input line
        print!("\x1b[2F\r");
        io::stdout().flush()?;

        let mut input = String::new();
        let mut stdin = io::stdin();
        let mut buf = [0u8; 1];

        loop {
            stdin.read_exact(&mut buf)?;
            let b = buf[0];

            match b {
                // Enter
                b'\r' | b'\n' => {
                    // Restore cursor to after-bottom-border (down 2)
                    print!("\x1b[2B\r");
                    io::stdout().flush()?;
                    return Ok(ReadResult::Input(input));
                }
                // Backspace / DEL
                0x7f | 0x08 => {
                    if !input.is_empty() {
                        input.pop();
                        print!("\x08 \x08");
                        io::stdout().flush()?;
                    }
                }
                // Ctrl-C
                0x03 => return Err(anyhow::anyhow!("Interrupted")),
                // Ctrl-D
                0x04 => return Err(anyhow::anyhow!("EOF")),
                // ESC
                0x1b => {
                    // Drain any following escape sequence bytes (e.g. arrow keys)
                    // without blocking — use a non-blocking peek.
                    let _ = self.drain_escape_sequence();
                    // Restore cursor position then signal escape
                    print!("\x1b[2B\r");
                    io::stdout().flush()?;
                    return Ok(ReadResult::Escape);
                }
                // Printable ASCII
                b if b >= 0x20 && b < 0x7f => {
                    let c = b as char;
                    input.push(c);
                    print!("{}", c);
                    io::stdout().flush()?;
                }
                _ => {}
            }
        }
    }

    /// Clear the current input line content (leaves cursor on that line).
    /// Called when ESC is pressed to wipe any partially-typed text.
    pub fn clear_typed_input(&self) {
        print!("\r\x1b[K");
        let _ = io::stdout().flush();
    }

    /// Non-blocking drain of a CSI / SS3 escape sequence that may follow 0x1b.
    fn drain_escape_sequence(&self) -> std::io::Result<()> {
        use std::os::unix::io::AsRawFd;
        let fd = io::stdin().as_raw_fd();
        // Check for up to 8 more bytes with 0 ms timeout
        for _ in 0..8 {
            let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            let ret = unsafe { libc::poll(&mut pfd as *mut _, 1, 0) };
            if ret <= 0 { break; }
            if pfd.revents & libc::POLLIN == 0 { break; }
            let mut b = [0u8; 1];
            let n = unsafe { libc::read(fd, b.as_mut_ptr() as *mut libc::c_void, 1) };
            if n <= 0 { break; }
            // Stop after the final byte of a CSI sequence (A-Z, a-z, ~)
            let ch = b[0];
            if ch.is_ascii_alphabetic() || ch == b'~' { break; }
        }
        Ok(())
    }
}

impl Drop for TerminalState {
    fn drop(&mut self) {
        let _ = self.exit_raw_mode();
    }
}
