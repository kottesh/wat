//! Core component system for differential rendering

use std::any::Any;
use std::fmt;

/// Unique identifier for a component
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComponentId(pub u64);

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ComponentId({})", self.0)
    }
}

/// Size of a component
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

impl Size {
    pub fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }
}

/// A single cell in the terminal grid
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub char: char,
    pub fg: Color,
    pub bg: Color,
    pub modifiers: Modifiers,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            char: ' ',
            fg: Color::Default,
            bg: Color::Default,
            modifiers: Modifiers::default(),
        }
    }
}

/// Terminal colors
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Ansi(u8),
    Rgb { r: u8, g: u8, b: u8 },
}

impl Default for Color {
    fn default() -> Self {
        Color::Default
    }
}

/// Text modifiers (bold, italic, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
    pub hidden: bool,
    pub strikethrough: bool,
}

impl Modifiers {
    pub fn bold() -> Self {
        Self { bold: true, ..Default::default() }
    }

    pub fn dim() -> Self {
        Self { dim: true, ..Default::default() }
    }
}

/// A buffer of cells representing a renderable surface
#[derive(Debug, Clone)]
pub struct Buffer {
    pub cells: Vec<Vec<Cell>>,
    pub width: u16,
    pub height: u16,
}

impl Buffer {
    pub fn new(width: u16, height: u16) -> Self {
        let cells = vec![vec![Cell::default(); width as usize]; height as usize];
        Self { cells, width, height }
    }

    pub fn empty() -> Self {
        Self {
            cells: vec![],
            width: 0,
            height: 0,
        }
    }

    /// Write a string at a position, wrapping at the buffer width
    pub fn write_str(&mut self, row: u16, col: u16, s: &str, fg: Color, bg: Color, modifiers: Modifiers) -> u16 {
        let mut current_row = row;
        let mut current_col = col;

        for ch in s.chars() {
            if ch == '\n' {
                current_row += 1;
                current_col = col;
                continue;
            }

            if current_col >= self.width {
                current_row += 1;
                current_col = col;
            }

            if current_row >= self.height {
                break;
            }

            if let Some(cell) = self.cells.get_mut(current_row as usize)
                .and_then(|r| r.get_mut(current_col as usize))
            {
                *cell = Cell {
                    char: ch,
                    fg,
                    bg,
                    modifiers,
                };
            }
            current_col += 1;
        }

        current_row + 1
    }

    /// Fill a row with a background color (sets all cells to spaces with this background)
    pub fn fill_row(&mut self, row: u16, bg: Color) {
        if let Some(row_cells) = self.cells.get_mut(row as usize) {
            for cell in row_cells {
                cell.char = ' ';
                cell.bg = bg;
            }
        }
    }
}

/// Format a single cell's style as ANSI escape codes
pub fn format_cell_style(fg: &Color, bg: &Color, mods: &Modifiers) -> String {
    let mut codes: Vec<String> = Vec::new();

    if mods.bold { codes.push("1".to_string()); }
    if mods.dim { codes.push("2".to_string()); }
    if mods.italic { codes.push("3".to_string()); }
    if mods.underline { codes.push("4".to_string()); }
    if mods.blink { codes.push("5".to_string()); }
    if mods.reverse { codes.push("7".to_string()); }
    if mods.hidden { codes.push("8".to_string()); }
    if mods.strikethrough { codes.push("9".to_string()); }

    match fg {
        Color::Default => {}
        Color::Black => codes.push("30".to_string()),
        Color::Red => codes.push("31".to_string()),
        Color::Green => codes.push("32".to_string()),
        Color::Yellow => codes.push("33".to_string()),
        Color::Blue => codes.push("34".to_string()),
        Color::Magenta => codes.push("35".to_string()),
        Color::Cyan => codes.push("36".to_string()),
        Color::White => codes.push("37".to_string()),
        Color::BrightBlack => codes.push("90".to_string()),
        Color::BrightRed => codes.push("91".to_string()),
        Color::BrightGreen => codes.push("92".to_string()),
        Color::BrightYellow => codes.push("93".to_string()),
        Color::BrightBlue => codes.push("94".to_string()),
        Color::BrightMagenta => codes.push("95".to_string()),
        Color::BrightCyan => codes.push("96".to_string()),
        Color::BrightWhite => codes.push("97".to_string()),
        Color::Ansi(n) => codes.push(format!("38;5;{}", n)),
        Color::Rgb { r, g, b } => codes.push(format!("38;2;{};{};{}", r, g, b)),
    }

    match bg {
        Color::Default => {}
        Color::Black => codes.push("40".to_string()),
        Color::Red => codes.push("41".to_string()),
        Color::Green => codes.push("42".to_string()),
        Color::Yellow => codes.push("43".to_string()),
        Color::Blue => codes.push("44".to_string()),
        Color::Magenta => codes.push("45".to_string()),
        Color::Cyan => codes.push("46".to_string()),
        Color::White => codes.push("47".to_string()),
        Color::BrightBlack => codes.push("100".to_string()),
        Color::BrightRed => codes.push("101".to_string()),
        Color::BrightGreen => codes.push("102".to_string()),
        Color::BrightYellow => codes.push("103".to_string()),
        Color::BrightBlue => codes.push("104".to_string()),
        Color::BrightMagenta => codes.push("105".to_string()),
        Color::BrightCyan => codes.push("106".to_string()),
        Color::BrightWhite => codes.push("107".to_string()),
        Color::Ansi(n) => codes.push(format!("48;5;{}", n)),
        Color::Rgb { r, g, b } => codes.push(format!("48;2;{};{};{}", r, g, b)),
    }

    if codes.is_empty() {
        String::new()
    } else {
        format!("\x1b[{}m", codes.join(";"))
    }
}

/// Trait for all UI components
pub trait Component: std::fmt::Debug + Send {
    /// Get the component's unique ID
    fn id(&self) -> ComponentId;

    /// Render the component to a buffer
    fn render(&self, width: u16) -> Buffer;

    /// Get the component's preferred height for a given width
    #[allow(dead_code)]
    fn preferred_height(&self, width: u16) -> u16;

    /// Get a mutable reference to self as Any for downcasting
    #[allow(dead_code)]
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
