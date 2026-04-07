//! Terminal raw-mode management and line input.

use anyhow::{Context, Result};
use nix::sys::termios;
use std::io::{self, Write};
use std::os::fd::AsFd;

pub enum InputEvent {
    /// User pressed Enter with the current input string.
    Submit(String),
    /// User pressed Escape.
    Cancel,
    /// User pressed Ctrl-C or similar shutdown signal.
    Shutdown,
    /// Terminal was resized.
    Resize,
}

pub struct TerminalState {
    original_termios: termios::Termios,
}

impl TerminalState {
    pub fn new() -> Result<Self> {
        let stdin = io::stdin();
        let original_termios =
            termios::tcgetattr(stdin.as_fd()).context("Failed to get terminal attributes")?;
        Ok(Self { original_termios })
    }

    pub fn enter_raw_mode(&mut self) -> Result<()> {
        let stdin = io::stdin();
        let mut raw = self.original_termios.clone();
        termios::cfmakeraw(&mut raw);
        termios::tcsetattr(stdin.as_fd(), termios::SetArg::TCSANOW, &raw)
            .context("Failed to enter raw mode")?;

        // Do NOT enable mouse tracking - we want native terminal scrollback to work

        Ok(())
    }

    pub fn exit_raw_mode(&self) -> Result<()> {
        let stdin = io::stdin();
        termios::tcsetattr(
            stdin.as_fd(),
            termios::SetArg::TCSANOW,
            &self.original_termios,
        )
        .context("Failed to restore terminal")?;
        io::stdout().flush()?;
        Ok(())
    }

    pub fn spawn_input_handler(
        &self,
        renderer: crate::ui::SharedRenderer,
    ) -> tokio::sync::mpsc::Receiver<InputEvent> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        let renderer_for_winch = renderer.clone();
        tokio::spawn(async move {
            use tokio::signal::unix::{SignalKind, signal};
            if let Ok(mut sigwinch) = signal(SignalKind::window_change()) {
                while sigwinch.recv().await.is_some() {
                    let mut renderer_lock = renderer_for_winch.lock().unwrap();
                    renderer_lock.force_redraw();
                    renderer_lock.render();
                }
            }
        });

        std::thread::spawn(move || {
            use std::os::unix::io::AsRawFd;
            let fd = io::stdin().as_raw_fd();
            let mut b = [0u8; 1];

            loop {
                let n = unsafe { libc::read(fd, b.as_mut_ptr() as *mut libc::c_void, 1) };
                if n <= 0 {
                    let _ = tx.blocking_send(InputEvent::Shutdown);
                    break;
                }

                let byte = b[0];

                match byte {
                    // Ctrl + F - Toggle fuzzy search
                    0x06 => {
                        let mut renderer_lock = renderer.lock().unwrap();
                        renderer_lock.toggle_fuzzy_mode();
                        renderer_lock.render();
                    }
                    // Enter (\r)
                    b'\r' => {
                        let mut renderer_lock = renderer.lock().unwrap();
                        if renderer_lock.fuzzy_mode() {
                            renderer_lock.fuzzy_submit();
                            renderer_lock.render();
                        } else {
                            let input = renderer_lock.take_input();
                            renderer_lock.render();
                            drop(renderer_lock);
                            if tx.blocking_send(InputEvent::Submit(input)).is_err() {
                                break;
                            }
                        }
                    }
                    // Ctrl + J (\n) - Move down
                    b'\n' => {
                        let mut renderer_lock = renderer.lock().unwrap();
                        if renderer_lock.fuzzy_mode() {
                            renderer_lock.fuzzy_move_down();
                        } else {
                            renderer_lock.move_cursor_down();
                        }
                        renderer_lock.render();
                    }
                    // Ctrl + K - Move up
                    0x0b => {
                        let mut renderer_lock = renderer.lock().unwrap();
                        if renderer_lock.fuzzy_mode() {
                            renderer_lock.fuzzy_move_up();
                        } else {
                            renderer_lock.move_cursor_up();
                        }
                        renderer_lock.render();
                    }
                    // Ctrl + U - Undo
                    0x15 => {
                        let mut renderer_lock = renderer.lock().unwrap();
                        renderer_lock.undo();
                        renderer_lock.render();
                    }
                    // Ctrl + R - Redo
                    0x12 => {
                        let mut renderer_lock = renderer.lock().unwrap();
                        renderer_lock.redo();
                        renderer_lock.render();
                    }
                    // Ctrl + O - Toggle full view
                    0x0f => {
                        let mut renderer_lock = renderer.lock().unwrap();
                        renderer_lock.toggle_last_tool_result();
                        renderer_lock.render();
                    }
                    // Ctrl + L - Clear screen / Full redraw
                    0x0c => {
                        let mut renderer_lock = renderer.lock().unwrap();
                        renderer_lock.force_redraw();
                        renderer_lock.render();
                    }
                    // Backspace / DEL
                    0x7f | 0x08 => {
                        let mut renderer_lock = renderer.lock().unwrap();
                        renderer_lock.pop_input_char();
                        renderer_lock.render();
                    }
                    // Ctrl-C
                    0x03 => {
                        let _ = tx.blocking_send(InputEvent::Shutdown);
                        break;
                    }
                    // ESC or sequence
                    0x1b => {
                        let escape = get_escape_type(fd);
                        let mut renderer_lock = renderer.lock().unwrap();
                        match escape {
                            EscapeType::Plain => {
                                if renderer_lock.fuzzy_mode() {
                                    renderer_lock.cancel_fuzzy();
                                    renderer_lock.render();
                                } else {
                                    drop(renderer_lock);
                                    if tx.blocking_send(InputEvent::Cancel).is_err() {
                                        break;
                                    }
                                }
                            }
                            EscapeType::AltEnter => {
                                renderer_lock.insert_newline();
                                renderer_lock.render();
                            }
                            EscapeType::ArrowUp => {
                                if renderer_lock.fuzzy_mode() {
                                    renderer_lock.fuzzy_move_up();
                                } else {
                                    renderer_lock.move_cursor_up();
                                }
                                renderer_lock.render();
                            }
                            EscapeType::ArrowDown => {
                                if renderer_lock.fuzzy_mode() {
                                    renderer_lock.fuzzy_move_down();
                                } else {
                                    renderer_lock.move_cursor_down();
                                }
                                renderer_lock.render();
                            }
                            EscapeType::ArrowLeft => {
                                renderer_lock.move_cursor_left();
                                renderer_lock.render();
                            }
                            EscapeType::ArrowRight => {
                                renderer_lock.move_cursor_right();
                                renderer_lock.render();
                            }
                            _ => {}
                        }
                    }
                    // Printable
                    byte if byte >= 0x20 && byte < 0x7f => {
                        let mut renderer_lock = renderer.lock().unwrap();
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

enum EscapeType {
    Plain,
    AltEnter,
    ArrowUp,
    ArrowDown,
    ArrowRight,
    ArrowLeft,
    Unknown,
}

fn get_escape_type(fd: i32) -> EscapeType {
    // Check if more data is available
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ret = unsafe { libc::poll(&mut pfd as *mut _, 1, 20) };
    if ret <= 0 {
        return EscapeType::Plain;
    }

    let mut b1 = [0u8; 1];
    if unsafe { libc::read(fd, b1.as_mut_ptr() as *mut libc::c_void, 1) } <= 0 {
        return EscapeType::Plain;
    }

    match b1[0] {
        b'\r' | b'\n' => EscapeType::AltEnter,
        b'[' => {
            let mut b2 = [0u8; 1];
            if unsafe { libc::read(fd, b2.as_mut_ptr() as *mut libc::c_void, 1) } <= 0 {
                return EscapeType::Unknown;
            }
            match b2[0] {
                b'A' => EscapeType::ArrowUp,
                b'B' => EscapeType::ArrowDown,
                b'C' => EscapeType::ArrowRight,
                b'D' => EscapeType::ArrowLeft,
                _ => EscapeType::Unknown,
            }
        }
        b'O' => {
            let mut b2 = [0u8; 1];
            if unsafe { libc::read(fd, b2.as_mut_ptr() as *mut libc::c_void, 1) } <= 0 {
                return EscapeType::Unknown;
            }
            match b2[0] {
                b'A' => EscapeType::ArrowUp,
                b'B' => EscapeType::ArrowDown,
                b'C' => EscapeType::ArrowRight,
                b'D' => EscapeType::ArrowLeft,
                _ => EscapeType::Unknown,
            }
        }
        _ => EscapeType::Unknown,
    }
}

impl Drop for TerminalState {
    fn drop(&mut self) {
        let _ = self.exit_raw_mode();
    }
}
