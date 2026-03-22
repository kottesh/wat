//! Terminal raw-mode management and line input.

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use nix::sys::termios;
use anyhow::{Result, Context};

use crate::renderer::DifferentialRenderer;

pub enum ReadResult {
    Input(String),
    Escape,
}

pub struct TerminalState {
    original_termios: termios::Termios,
}

impl TerminalState {
    pub fn new() -> Result<Self> {
        let stdin = io::stdin();
        let original_termios = termios::tcgetattr(stdin.as_fd())
            .context("Failed to get terminal attributes")?;
        Ok(Self { original_termios })
    }

    pub fn enter_raw_mode(&mut self) -> Result<()> {
        let stdin = io::stdin();
        let mut raw = self.original_termios.clone();
        termios::cfmakeraw(&mut raw);
        termios::tcsetattr(stdin.as_fd(), termios::SetArg::TCSANOW, &raw)
            .context("Failed to enter raw mode")?;
        Ok(())
    }

    pub fn exit_raw_mode(&self) -> Result<()> {
        let stdin = io::stdin();
        termios::tcsetattr(stdin.as_fd(), termios::SetArg::TCSANOW, &self.original_termios)
            .context("Failed to restore terminal")?;
        io::stdout().flush()?;
        Ok(())
    }

    /// Read one line of input, updating the renderer on every keypress so the
    /// input box reflects what the user types in real time.
    pub fn read_line(&self, renderer: &mut DifferentialRenderer) -> Result<ReadResult> {
        let mut stdin = io::stdin();
        let mut buf = [0u8; 1];

        loop {
            stdin.read_exact(&mut buf)?;

            match buf[0] {
                // Enter
                b'\r' | b'\n' => {
                    let input = renderer.take_input();
                    return Ok(ReadResult::Input(input));
                }
                // Backspace / DEL
                0x7f | 0x08 => {
                    renderer.pop_input_char();
                    renderer.render();
                }
                // Ctrl-C
                0x03 => return Err(anyhow::anyhow!("Interrupted")),
                // Ctrl-D
                0x04 => return Err(anyhow::anyhow!("EOF")),
                // ESC — drain any CSI sequence that may follow
                0x1b => {
                    let _ = self.drain_escape_sequence();
                    renderer.clear_input();
                    renderer.render();
                    return Ok(ReadResult::Escape);
                }
                // Printable ASCII
                b if b >= 0x20 && b < 0x7f => {
                    renderer.push_input_char(b as char);
                    renderer.render();
                }
                _ => {}
            }
        }
    }

    /// Non-blocking drain of escape-sequence bytes that follow 0x1b (e.g. arrow keys).
    fn drain_escape_sequence(&self) -> io::Result<()> {
        use std::os::unix::io::AsRawFd;
        let fd = io::stdin().as_raw_fd();
        for _ in 0..8 {
            let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            let ret = unsafe { libc::poll(&mut pfd as *mut _, 1, 0) };
            if ret <= 0 || pfd.revents & libc::POLLIN == 0 {
                break;
            }
            let mut b = [0u8; 1];
            let n = unsafe { libc::read(fd, b.as_mut_ptr() as *mut libc::c_void, 1) };
            if n <= 0 {
                break;
            }
            // CSI sequences end with a letter or ~
            if b[0].is_ascii_alphabetic() || b[0] == b'~' {
                break;
            }
        }
        Ok(())
    }
}

impl Drop for TerminalState {
    fn drop(&mut self) {
        let _ = self.exit_raw_mode();
    }
}
