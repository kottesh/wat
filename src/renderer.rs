//! Inline rendering system for terminal output
//!
//! This module implements a component-based inline renderer that outputs
//! directly to the terminal, similar to the original inline style.

use std::io::{self, Write};
use std::collections::HashMap;
use std::any::Any;

use crate::component::{Buffer, Component, ComponentId, Size, format_cell_style};
use crate::components::{
    ErrorComponent, PromptComponent, ResponseComponent, SeparatorComponent,
    SpinnerComponent, ToolCallComponent, ToolResultComponent,
    UserInputComponent,
};
use crate::layout::LayoutManager;

/// Global counter for generating unique component IDs
static COMPONENT_ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Generate a new unique component ID
pub fn next_component_id() -> ComponentId {
    ComponentId(COMPONENT_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
}

/// Component wrapper
struct ComponentEntry {
    component: Box<dyn Component>,
}

/// The inline renderer - outputs directly to terminal
pub struct DifferentialRenderer {
    /// All registered components
    components: HashMap<ComponentId, ComponentEntry>,
    /// Layout manager
    layout: LayoutManager,
    /// Current terminal size
    terminal_size: Size,
    /// Whether colors are enabled
    use_colors: bool,
    /// Current prompt component (if any)
    prompt_id: Option<ComponentId>,
}

impl std::fmt::Debug for DifferentialRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DifferentialRenderer")
            .field("component_count", &self.components.len())
            .field("terminal_size", &self.terminal_size)
            .field("use_colors", &self.use_colors)
            .finish()
    }
}

impl DifferentialRenderer {
    /// Create a new inline renderer
    pub fn new(use_colors: bool) -> Self {
        let terminal_size = crossterm::terminal::size()
            .map(|(w, h)| Size::new(w, h))
            .unwrap_or_else(|_| Size::new(80, 24));

        Self {
            components: HashMap::new(),
            layout: LayoutManager::new(terminal_size.width, terminal_size.height),
            terminal_size,
            use_colors,
            prompt_id: None,
        }
    }

    /// Update terminal size
    pub fn update_size(&mut self) {
        if let Ok((w, h)) = crossterm::terminal::size() {
            self.terminal_size = Size::new(w, h);
            self.layout.set_size(w, h);
        }
    }

    /// Add a user input component
    pub fn add_user_input(&mut self, content: String) -> ComponentId {
        let id = next_component_id();
        let component = UserInputComponent::new(id, content, self.use_colors);
        let id = self.add_component(Box::new(component));
        self.render_component(id);
        id
    }

    /// Add a response component
    pub fn add_response(&mut self, content: String) -> ComponentId {
        let id = next_component_id();
        let component = ResponseComponent::new(id, content, self.use_colors);
        let id = self.add_component(Box::new(component));
        self.render_component(id);
        id
    }

    /// Add a tool call component
    pub fn add_tool_call(&mut self, tool_name: String, args: String) -> ComponentId {
        let id = next_component_id();
        let component = ToolCallComponent::new(id, tool_name, args, self.use_colors);
        let id = self.add_component(Box::new(component));
        self.render_component(id);
        id
    }

    /// Add a tool result component
    pub fn add_tool_result(
        &mut self,
        tool_name: String,
        output: String,
        duration_secs: Option<f64>,
        success: bool,
        command: Option<String>,
    ) -> ComponentId {
        let id = next_component_id();
        let component = ToolResultComponent::new(
            id,
            tool_name,
            output,
            duration_secs,
            success,
            command,
            self.use_colors,
        );
        let id = self.add_component(Box::new(component));
        self.render_component(id);
        id
    }

    /// Add a spinner component
    pub fn add_spinner(&mut self, message: String) -> ComponentId {
        let id = next_component_id();
        let component = SpinnerComponent::new(id, message, self.use_colors);
        let id = self.add_component(Box::new(component));
        self.render_component(id);
        id
    }

    /// Add an error component
    pub fn add_error(&mut self, message: String) -> ComponentId {
        let id = next_component_id();
        let component = ErrorComponent::new(id, message, self.use_colors);
        let id = self.add_component(Box::new(component));
        self.render_component(id);
        id
    }

    /// Add a separator component
    pub fn add_separator(&mut self) -> ComponentId {
        let id = next_component_id();
        let component = SeparatorComponent::new(id, self.use_colors);
        let id = self.add_component(Box::new(component));
        self.render_component(id);
        id
    }

    /// Create a prompt component
    pub fn create_prompt(&mut self) -> ComponentId {
        // Remove existing prompt if any
        if let Some(old_id) = self.prompt_id.take() {
            self.remove_component(old_id);
        }

        let id = next_component_id();
        let component = PromptComponent::new(id, self.use_colors);
        let id = self.add_component(Box::new(component));
        self.prompt_id = Some(id);
        id
    }

    /// Get the prompt component for input handling
    pub fn get_prompt_mut(&mut self) -> Option<&mut PromptComponent> {
        let id = self.prompt_id?;
        self.components.get_mut(&id).and_then(|entry| {
            entry.component.as_any_mut().downcast_mut::<PromptComponent>()
        })
    }

    /// Clear the prompt
    pub fn clear_prompt(&mut self) {
        if let Some(id) = self.prompt_id.take() {
            self.remove_component(id);
        }
    }

    /// Add a generic component
    fn add_component(&mut self, component: Box<dyn Component>) -> ComponentId {
        let id = component.id();
        self.layout.append_component(id);
        self.components.insert(id, ComponentEntry { component });
        id
    }

    /// Remove a component
    pub fn remove_component(&mut self, id: ComponentId) {
        self.components.remove(&id);
        self.layout.remove_component(id);
        if self.prompt_id == Some(id) {
            self.prompt_id = None;
        }
    }

    /// Render a single component to stdout
    fn render_component(&self, id: ComponentId) {
        if let Some(entry) = self.components.get(&id) {
            let buffer = entry.component.render(self.terminal_size.width);
            let output = self.buffer_to_string(&buffer);
            print!("{}", output);
            // Add blank line after each component for spacing
            println!();
            let _ = io::stdout().flush();
        }
    }

    /// Convert buffer to string for output
    fn buffer_to_string(&self, buffer: &Buffer) -> String {
        let mut output = String::new();

        for row in &buffer.cells {
            // Group consecutive cells with the same style
            let mut current_style: Option<String> = None;
            let mut current_chars = String::new();

            for cell in row {
                let style = format_cell_style(&cell.fg, &cell.bg, &cell.modifiers);
                
                if current_style.as_ref() != Some(&style) {
                    // Style changed - flush current chars and start new group
                    if !current_chars.is_empty() {
                        if let Some(ref s) = current_style {
                            output.push_str(s);
                        }
                        output.push_str(&current_chars);
                        current_chars.clear();
                    }
                    current_style = if style.is_empty() { None } else { Some(style) };
                }
                current_chars.push(cell.char);
            }

            // Flush remaining chars
            if !current_chars.is_empty() {
                if let Some(ref s) = current_style {
                    output.push_str(s);
                }
                output.push_str(&current_chars);
            }
            
            output.push_str("\x1b[0m\r\n"); // Reset and newline WITH carriage return
        }

        output
    }

    /// Perform a render (for compatibility)
    pub fn render(&mut self) -> io::Result<()> {
        // In inline mode, components are rendered as they're added
        // This is a no-op for compatibility with the API
        io::stdout().flush()
    }

    /// Force a full redraw (no-op in inline mode)
    pub fn force_redraw(&mut self) {
        // No-op in inline mode
    }

    /// Clear all components
    pub fn clear(&mut self) {
        self.components.clear();
        self.layout.clear();
        self.prompt_id = None;
    }

    /// Get terminal width
    pub fn width(&self) -> u16 {
        self.terminal_size.width
    }

    /// Get terminal height
    pub fn height(&self) -> u16 {
        self.terminal_size.height
    }

    /// Scroll to bottom (no-op in inline mode)
    pub fn scroll_to_bottom(&mut self) {
        // No-op in inline mode
    }

    /// Scroll (no-op in inline mode)
    pub fn scroll(&mut self, _lines: i16) {
        // No-op in inline mode
    }

    /// Tick spinner animation
    pub fn tick_spinner(&mut self, id: ComponentId) {
        if let Some(entry) = self.components.get_mut(&id)
            && let Some(spinner) = entry.component.as_any_mut().downcast_mut::<SpinnerComponent>() {
                spinner.tick();
            }
    }
}

/// Helper trait to access component internals for downcasting
pub trait AsAny {
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Color;

    #[test]
    fn test_component_id_uniqueness() {
        let id1 = next_component_id();
        let id2 = next_component_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_renderer_creation() {
        let renderer = DifferentialRenderer::new(true);
        assert!(renderer.components.is_empty());
    }

    #[test]
    fn test_user_input_rendering() {
        let id = next_component_id();
        let component = UserInputComponent::new(id, "hello".to_string(), true);
        let buffer = component.render(20);
        
        // Check dimensions - should be 3 rows (top padding, content, bottom padding)
        assert_eq!(buffer.height, 3);
        assert_eq!(buffer.width, 20);
        
        // Check that background is set on all cells
        for row in &buffer.cells {
            for cell in row {
                assert_eq!(cell.bg, Color::Ansi(235), "Background should be ANSI 235");
            }
        }
        
        // Top and bottom rows should be all spaces
        for cell in &buffer.cells[0] {
            assert_eq!(cell.char, ' ', "Top row should be all spaces");
        }
        for cell in &buffer.cells[2] {
            assert_eq!(cell.char, ' ', "Bottom row should be all spaces");
        }
        
        // Middle row should contain "  hello  "
        let middle_row = &buffer.cells[1];
        assert_eq!(middle_row[0].char, ' ');
        assert_eq!(middle_row[1].char, ' ');
        assert_eq!(middle_row[2].char, 'h');
        assert_eq!(middle_row[3].char, 'e');
        assert_eq!(middle_row[4].char, 'l');
        assert_eq!(middle_row[5].char, 'l');
        assert_eq!(middle_row[6].char, 'o');
        assert_eq!(middle_row[7].char, ' ');
        assert_eq!(middle_row[8].char, ' ');
    }

    #[test]
    fn test_buffer_to_string() {
        let renderer = DifferentialRenderer::new(true);
        
        let id = next_component_id();
        let component = UserInputComponent::new(id, "test".to_string(), true);
        let buffer = component.render(10);
        let output = renderer.buffer_to_string(&buffer);
        
        // Output should contain ANSI escape codes for background color
        assert!(output.contains("\x1b[48;5;235m"), "Should contain background ANSI code");
        assert!(output.contains("test"), "Should contain the text");
    }

    #[test]
    fn test_user_input_full_output() {
        let renderer = DifferentialRenderer::new(true);
        
        let id = next_component_id();
        let component = UserInputComponent::new(id, "hello world".to_string(), true);
        let buffer = component.render(20);
        let output = renderer.buffer_to_string(&buffer);
        
        // Should have 3 lines
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3, "Should have 3 lines");
        
        // Each line should have the background color
        for line in &lines {
            assert!(line.contains("\x1b[48;5;235m"), "Each line should have background color");
        }
        
        // Middle line should contain "hello world"
        assert!(lines[1].contains("hello world"), "Middle line should contain the text");
    }
}
