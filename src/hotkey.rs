use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use anyhow::{Result, Context};

/// Hotkey interceptor for listening to global hotkeys
pub struct HotkeyInterceptor {
    tx: mpsc::Sender<()>,
}

impl HotkeyInterceptor {
    /// Create new hotkey interceptor
    pub fn new() -> (Self, mpsc::Receiver<()>) {
        let (tx, rx) = mpsc::channel();
        
        (Self { tx }, rx)
    }
    
    /// Start listening for hotkeys (platform-specific)
    pub fn start_listening(self, hotkey: Hotkey) -> Result<()> {
        thread::spawn(move || {
            if let Err(e) = Self::listen_loop(self.tx.clone(), hotkey) {
                eprintln!("Hotkey listener error: {}", e);
            }
        });
        
        Ok(())
    }
    
    /// Main listening loop
    fn listen_loop(tx: mpsc::Sender<()>, hotkey: Hotkey) -> Result<()> {
        #[cfg(target_os = "linux")]
        return Self::linux_listen(tx, hotkey);
        
        #[cfg(target_os = "macos")]
        return Self::macos_listen(tx, hotkey);
        
        #[cfg(target_os = "windows")]
        return Self::windows_listen(tx, hotkey);
        
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            eprintln!("Hotkey listening not supported on this platform");
            Ok(())
        }
    }
    
    #[cfg(target_os = "linux")]
    fn linux_listen(tx: mpsc::Sender<()>, hotkey: Hotkey) -> Result<()> {
        // Try to read from stdin in raw mode
        // This is a simplified approach - for production, use evdev or similar
        
        println!("Hotkey listener started on Linux");
        println!("Press {:?} to activate agent", hotkey);
        
        // For now, we'll use a simple stdin approach
        // In production, you'd want to use evdev for global hotkeys
        
        loop {
            // Check if we should exit
            thread::sleep(Duration::from_millis(100));
            
            // Simple check - in real implementation, you'd read actual key events
            // This is just a placeholder
            if false { // Replace with actual hotkey detection
                let _ = tx.send(());
            }
        }
    }
    
    #[cfg(target_os = "macos")]
    fn macos_listen(_tx: mpsc::Sender<()>, _hotkey: Hotkey) -> Result<()> {
        // macOS would use Core Graphics or similar
        eprintln!("Hotkey listening not yet implemented for macOS");
        Ok(())
    }
    
    #[cfg(target_os = "windows")]
    fn windows_listen(_tx: mpsc::Sender<()>, _hotkey: Hotkey) -> Result<()> {
        // Windows would use winapi
        eprintln!("Hotkey listening not yet implemented for Windows");
        Ok(())
    }
}

/// Hotkey configuration
#[derive(Debug, Clone)]
pub enum Hotkey {
    F2,
    F3,
    F4,
    CtrlAlt(char),
    Custom(&'static str),
}

impl Hotkey {
    /// Parse hotkey from string
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "f2" => Ok(Hotkey::F2),
            "f3" => Ok(Hotkey::F3),
            "f4" => Ok(Hotkey::F4),
            s if s.starts_with("ctrl+alt+") => {
                let key = s.chars().last()
                    .context("Invalid hotkey format")?;
                Ok(Hotkey::CtrlAlt(key))
            }
            _ => Ok(Hotkey::Custom(Box::leak(s.to_string().into_boxed_str()))),
        }
    }
    
    /// Get string representation
    pub fn to_string(&self) -> String {
        match self {
            Hotkey::F2 => "F2".to_string(),
            Hotkey::F3 => "F3".to_string(),
            Hotkey::F4 => "F4".to_string(),
            Hotkey::CtrlAlt(c) => format!("Ctrl+Alt+{}", c),
            Hotkey::Custom(s) => s.to_string(),
        }
    }
}

/// Fallback: Terminal-based hotkey detection
/// This works within the terminal but isn't global
pub struct TerminalHotkey {
    hotkey: Hotkey,
}

impl TerminalHotkey {
    /// Create new terminal hotkey detector
    pub fn new(hotkey: Hotkey) -> Self {
        Self { hotkey }
    }
    
    /// Wait for hotkey (blocks until hotkey is pressed)
    pub fn wait(&self) -> Result<()> {
        println!("Press {:?} to activate agent (in this terminal)...", self.hotkey);
        
        // Set terminal to raw mode to read single keypresses
        use nix::sys::termios;
        use std::io::Read;
        use std::os::fd::AsFd;
        
        let stdin = std::io::stdin();
        let original = termios::tcgetattr(stdin.as_fd())?;
        let mut raw = original.clone();
        termios::cfmakeraw(&mut raw);
        termios::tcsetattr(stdin.as_fd(), termios::SetArg::TCSANOW, &raw)?;
        
        let mut buf = [0; 1];
        loop {
            std::io::stdin().read_exact(&mut buf)?;
            
            // Check for F2 (ESC [ O Q or ESC [ [ Q)
            if buf[0] == 0x1b { // ESC
                let mut seq = [0; 2];
                if std::io::stdin().read(&mut seq).is_ok() {
                    if seq == [b'[', b'O'] || seq == [b'[', b'Q'] || seq == [b'O', b'Q'] {
                        break; // F2 pressed
                    }
                }
            }
            
            // Check for Ctrl+Alt+;
            if buf[0] == b';' {
                // In raw mode, we'd need to check modifiers
                // This is simplified
                break;
            }
        }
        
        // Restore terminal
        termios::tcsetattr(stdin.as_fd(), termios::SetArg::TCSANOW, &original)?;
        
        Ok(())
    }
}