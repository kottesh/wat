//! Terminal raw-mode management and line input.

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use nix::sys::termios;
use anyhow::{Result, Context};

use crate::renderer::DifferentialRenderer;

pub enum InputEvent {
    /// User pressed Enter with the current input string.
    Submit(String),
    /// User pressed Escape.
    Cancel,
    /// User pressed Ctrl-C or similar shutdown signal.
    Shutdown,
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

    pub fn spawn_input_handler(
        &self,
        renderer: crate::renderer::SharedRenderer,
    ) -> tokio::sync::mpsc::Receiver<InputEvent> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        std::thread::spawn(move || {
            let mut stdin = io::stdin();
            let mut buf = [0u8; 1];

            loop {
                if stdin.read_exact(&mut buf).is_err() {
                    let _ = tx.blocking_send(InputEvent::Shutdown);
                    break;
                }

                let b = buf[0];
                let mut renderer_lock = renderer.lock().unwrap();

                match b {
                    // Enter
                    b'\r' | b'\n' => {
                        let input = renderer_lock.take_input();
                        renderer_lock.render();
                        if tx.blocking_send(InputEvent::Submit(input)).is_err() {
                            break;
                        }
                    }
                    // Backspace / DEL
                    0x7f | 0x08 => {
                        renderer_lock.pop_input_char();
                        renderer_lock.render();
                    }
                    // Ctrl-C
                    0x03 => {
                        let _ = tx.blocking_send(InputEvent::Shutdown);
                        break;
                    }
                    // ESC
                    0x1b => {
                        if let Ok(true) = is_plain_esc() {
                            if tx.blocking_send(InputEvent::Cancel).is_err() {
                                break;
                            }
                        }
                    }
                    // Printable
                    byte if byte >= 0x20 && byte < 0x7f => {
                        renderer_lock.push_input_char(byte as char);
                        renderer_lock.render();
                    }
                    _ => {}
                }
            }
        });

        rx
    }
}

/// Helper for the input thread to distinguish ESC from CSI sequences.
fn is_plain_esc() -> io::Result<bool> {
    use std::os::unix::io::AsRawFd;
    let fd = io::stdin().as_raw_fd();
    let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    // Wait a tiny bit to see if more bytes follow
    let ret = unsafe { libc::poll(&mut pfd as *mut _, 1, 10) };
    Ok(ret <= 0)
}

impl Drop for TerminalState {
    fn drop(&mut self) {
        let _ = self.exit_raw_mode();
    }
}
