//! UI Manager - orchestrates all UI components and rendering

use crate::component::{Component, ComponentId};
use crate::components::{
    BashComponent, ErrorComponent, ResponseComponent, ToolCallComponent, ToolResultComponent,
    UserInputComponent,
};
use crate::ui::{CursorPos, DiffRenderer, Editor, FuzzySearch, Layout, Spacing};
use std::sync::{Arc, Mutex};

/// Terminal size
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Size {
    width: u16,
    height: u16,
}

impl Size {
    fn current() -> Self {
        let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
        Self {
            width: w,
            height: h,
        }
    }
}

/// Render item in the history
enum HistoryItem {
    Component(Box<dyn Component>),
    Bash(BashComponent),
}

/// UI Manager orchestrating all rendering
pub struct UIManager {
    // History
    history: Vec<HistoryItem>,
    current_bash: Option<BashComponent>,
    current_response: Option<ResponseComponent>,

    // Input editor
    editor: Editor,

    // Fuzzy search
    fuzzy: Option<FuzzySearch>,
    fuzzy_triggered_by_at: bool,

    // Rendering
    diff_renderer: DiffRenderer,
    terminal_size: Size,
    last_terminal_size: Size,

    // UI state
    spinner_text: Option<String>,
    input_hint: Option<String>,
    use_colors: bool,
}

impl UIManager {
    /// Create a new UI manager
    pub fn new(use_colors: bool) -> Self {
        let terminal_size = Size::current();

        Self {
            history: Vec::new(),
            current_bash: None,
            current_response: None,
            editor: Editor::new(),
            fuzzy: None,
            fuzzy_triggered_by_at: false,
            diff_renderer: DiffRenderer::new(),
            terminal_size,
            last_terminal_size: terminal_size,
            spinner_text: None,
            input_hint: None,
            use_colors,
        }
    }

    /// Update terminal size
    pub fn update_size(&mut self) {
        self.terminal_size = Size::current();
    }

    // ── Fuzzy search methods ──

    pub fn toggle_fuzzy_mode(&mut self) {
        if self.fuzzy.is_some() {
            self.fuzzy = None;
            self.fuzzy_triggered_by_at = false;
        } else {
            self.fuzzy = Some(FuzzySearch::new());
            self.fuzzy_triggered_by_at = false;
        }
    }

    pub fn trigger_fuzzy_at(&mut self) {
        if self.fuzzy.is_none() {
            self.fuzzy = Some(FuzzySearch::new());
            self.fuzzy_triggered_by_at = true;
        }
    }

    pub fn cancel_fuzzy(&mut self) {
        self.fuzzy = None;
        self.fuzzy_triggered_by_at = false;
    }

    pub fn update_fuzzy_results(&mut self) {
        if let Some(ref mut fuzzy) = self.fuzzy {
            let query = self.editor.content();
            fuzzy.update_query(query);
        }
    }

    pub fn fuzzy_submit(&mut self) {
        if let Some(ref fuzzy) = self.fuzzy {
            if let Some(selected) = fuzzy.selected_file() {
                self.editor.clear();
                self.editor.insert_str(&selected);
                self.cancel_fuzzy();
            }
        }
    }

    pub fn fuzzy_move_up(&mut self) {
        if let Some(ref mut fuzzy) = self.fuzzy {
            fuzzy.move_up();
        }
    }

    pub fn fuzzy_move_down(&mut self) {
        if let Some(ref mut fuzzy) = self.fuzzy {
            fuzzy.move_down();
        }
    }

    pub fn fuzzy_mode(&self) -> bool {
        self.fuzzy.is_some()
    }

    // ── Input editor methods ──

    pub fn undo(&mut self) {
        self.editor.undo();
    }

    pub fn redo(&mut self) {
        self.editor.redo();
    }

    pub fn push_input_char(&mut self, c: char) {
        self.editor.insert_char(c);
    }

    pub fn pop_input_char(&mut self) {
        self.editor.delete_char();
    }

    pub fn insert_newline(&mut self) {
        self.editor.insert_newline();
    }

    pub fn move_cursor_up(&mut self) {
        self.editor.move_up();
    }

    pub fn move_cursor_down(&mut self) {
        self.editor.move_down();
    }

    pub fn move_cursor_left(&mut self) {
        self.editor.move_left();
    }

    pub fn move_cursor_right(&mut self) {
        self.editor.move_right();
    }

    pub fn take_input(&mut self) -> String {
        self.editor.take_content()
    }

    // ── UI state methods ──

    pub fn set_spinner(&mut self, text: String) {
        self.spinner_text = Some(text);
    }

    pub fn clear_spinner(&mut self) {
        self.spinner_text = None;
    }

    pub fn set_input_hint(&mut self, hint: String) {
        self.input_hint = Some(hint);
    }

    pub fn clear_input_hint(&mut self) {
        self.input_hint = None;
    }

    // ── Component addition methods ──

    pub fn add_user_input(&mut self, content: String) {
        let comp = UserInputComponent::new(next_component_id(), content, self.use_colors);
        self.history.push(HistoryItem::Component(Box::new(comp)));
    }

    pub fn add_response(&mut self, content: String) {
        let comp = ResponseComponent::new(next_component_id(), content, self.use_colors);
        self.history.push(HistoryItem::Component(Box::new(comp)));
    }

    pub fn add_tool_call(&mut self, tool_name: String, args: String) {
        let comp = ToolCallComponent::new(next_component_id(), tool_name, args, self.use_colors);
        self.history.push(HistoryItem::Component(Box::new(comp)));
    }

    pub fn add_tool_result(
        &mut self,
        tool_name: String,
        content: String,
        duration_secs: Option<f64>,
        success: bool,
        command: Option<String>,
    ) {
        let comp = ToolResultComponent::new(
            next_component_id(),
            tool_name,
            content,
            duration_secs,
            success,
            command,
            self.use_colors,
        );
        self.history.push(HistoryItem::Component(Box::new(comp)));
    }

    pub fn add_error(&mut self, message: String) {
        let comp = ErrorComponent::new(next_component_id(), message, self.use_colors);
        self.history.push(HistoryItem::Component(Box::new(comp)));
    }

    // ── Bash execution methods ──

    pub fn start_bash(&mut self, command: &str) {
        self.current_bash = Some(BashComponent::new(
            next_component_id(),
            command.to_string(),
            self.use_colors,
        ));
    }

    pub fn push_bash_output(&mut self, line: String) {
        if let Some(ref mut bash) = self.current_bash {
            bash.push_output(line);
        }
    }

    pub fn set_bash_elapsed(&mut self, secs: f64) {
        if let Some(ref mut bash) = self.current_bash {
            bash.set_elapsed(secs);
        }
    }

    pub fn finalize_bash(&mut self, duration: f64, success: bool, cancelled: bool) {
        if let Some(mut bash) = self.current_bash.take() {
            bash.set_elapsed(duration);
            if cancelled {
                bash.set_cancelled();
            } else {
                bash.set_done(success);
            }
            self.history.push(HistoryItem::Bash(bash));
        }
    }

    pub fn last_bash_output(&self) -> String {
        if let Some(_bash) = &self.current_bash {
            // Get the last output line from the bash component
            // This is a simplified version - you may need to access bash.output_lines
            String::new() // Placeholder
        } else {
            String::new()
        }
    }

    // ── Streaming response methods ──

    pub fn start_streaming_response(&mut self) {
        self.current_response = Some(ResponseComponent::new(
            next_component_id(),
            String::new(),
            self.use_colors,
        ));
    }

    pub fn push_response_chunk(&mut self, chunk: &str) {
        if let Some(ref mut resp) = self.current_response {
            resp.append_content(chunk);
        }
    }

    pub fn finalize_response(&mut self) {
        if let Some(resp) = self.current_response.take() {
            self.history.push(HistoryItem::Component(Box::new(resp)));
        }
    }

    // ── Toggle methods ──

    pub fn toggle_last_tool_result(&mut self) {
        // Find last tool result and toggle show_all
        for item in self.history.iter_mut().rev() {
            if let HistoryItem::Component(comp) = item {
                if comp.toggle_show_all() {
                    break;
                }
            }
        }
    }

    // ── Rendering ──

    pub fn force_redraw(&mut self) {
        self.diff_renderer.force_clear();
    }

    pub fn render(&mut self) {
        let old_size = self.terminal_size;
        self.update_size();

        // Detect terminal resize and force redraw
        // Note: Inside tmux, resizing the pane won't trigger this - only resizing
        // the actual terminal window will. Use Ctrl-L to manually clear if needed.
        if self.terminal_size != old_size {
            self.force_redraw();
        }

        self.last_terminal_size = self.terminal_size;

        // Trust the DiffRenderer to handle incremental updates
        // (Removed force_clear during streaming - it caused flickering)

        let (all_lines, cursor_pos) = self.render_all();
        self.diff_renderer
            .render(all_lines, cursor_pos, self.terminal_size.height as usize);
    }

    fn render_all(&self) -> (Vec<String>, CursorPos) {
        let width = self.terminal_size.width;
        let mut components: Vec<(Vec<String>, Spacing)> = Vec::new();

        // Render history
        for item in &self.history {
            let lines = match item {
                HistoryItem::Component(comp) => component_to_lines(comp.as_ref(), width),
                HistoryItem::Bash(bash) => bash.render_lines(width as usize),
            };
            components.push((lines, Spacing::default()));
        }

        // Render current bash if running
        if let Some(ref bash) = self.current_bash {
            let lines = bash.render_lines(width as usize);
            components.push((lines, Spacing::default()));
        }

        // Render current streaming response
        if let Some(ref resp) = self.current_response {
            let lines = component_to_lines(resp, width);
            components.push((lines, Spacing::default()));
        }

        // Render spinner if present (above input editor)
        if let Some(ref spinner_text) = self.spinner_text {
            let lines = vec![spinner_text.clone()];
            components.push((lines, Spacing::none()));
        }

        // Track where input block starts (before we add it)
        let input_block_start = {
            let stacked_before_input = Layout::stack_with_spacing(components.clone());
            stacked_before_input.len()
        };

        // Render fuzzy search or input editor
        let (input_lines, input_cursor_pos) = if let Some(ref fuzzy) = self.fuzzy {
            let lines = fuzzy.render(width);
            let cursor = (lines.len().saturating_sub(1), 0);
            (lines, cursor)
        } else {
            let (lines, cursor_row, cursor_col) = self.editor.render(width, self.use_colors);
            (lines, (cursor_row, cursor_col))
        };

        components.push((input_lines, Spacing::none()));

        // Stack all components
        let all_lines = Layout::stack_with_spacing(components);
        let final_lines = Layout::trim_trailing_blank(all_lines);

        // Calculate absolute cursor position:
        // input_block_start is where input starts in the final buffer.
        // Add the cursor offset within the input block.
        let abs_cursor_row = input_block_start + input_cursor_pos.0;
        let abs_cursor_col = input_cursor_pos.1;

        (final_lines, (abs_cursor_row, abs_cursor_col))
    }

    pub fn use_colors(&self) -> bool {
        self.use_colors
    }
}

/// Convert component to lines (helper)
fn component_to_lines(comp: &dyn Component, width: u16) -> Vec<String> {
    let buffer = comp.render(width);
    buffer_to_lines(&buffer)
}

/// Convert buffer to lines (helper)
fn buffer_to_lines(buffer: &crate::component::Buffer) -> Vec<String> {
    use crate::component::format_cell_style;

    let mut result = Vec::new();
    for row in &buffer.cells {
        let mut line = String::new();
        let mut cur_style: Option<String> = None;
        let mut cur_chars = String::new();

        for cell in row {
            let style = format_cell_style(&cell.fg, &cell.bg, &cell.modifiers);
            if cur_style.as_deref() != Some(&style) {
                // Flush previous style
                if let Some(s) = cur_style.take() {
                    line.push_str(&s); // Style already includes \x1b[...m
                    line.push_str(&cur_chars);
                    line.push_str("\x1b[0m"); // Reset
                    cur_chars.clear();
                }
                cur_style = Some(style);
            }
            cur_chars.push(cell.char);
        }

        // Flush remaining
        if !cur_chars.is_empty() {
            if let Some(s) = cur_style {
                line.push_str(&s); // Style already includes \x1b[...m
                line.push_str(&cur_chars);
                if !s.is_empty() {
                    line.push_str("\x1b[0m"); // Reset
                }
            } else {
                line.push_str(&cur_chars);
            }
        }

        result.push(line);
    }
    result
}

/// Shared renderer type (thread-safe)
pub type SharedRenderer = Arc<Mutex<UIManager>>;

/// Global component ID counter
static COMPONENT_ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Generate next component ID
pub fn next_component_id() -> ComponentId {
    ComponentId(COMPONENT_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_ui_manager() {
        let ui = UIManager::new(true);
        assert!(ui.use_colors());
        assert!(!ui.fuzzy_mode());
        assert!(ui.history.is_empty());
    }

    #[test]
    fn test_toggle_fuzzy_mode() {
        let mut ui = UIManager::new(false);
        assert!(!ui.fuzzy_mode());

        ui.toggle_fuzzy_mode();
        assert!(ui.fuzzy_mode());

        ui.toggle_fuzzy_mode();
        assert!(!ui.fuzzy_mode());
    }

    #[test]
    fn test_input_editing() {
        let mut ui = UIManager::new(false);

        ui.push_input_char('h');
        ui.push_input_char('i');

        let content = ui.take_input();
        assert_eq!(content, "hi");
    }

    #[test]
    fn test_spinner() {
        let mut ui = UIManager::new(false);

        ui.set_spinner("Loading...".to_string());
        assert!(ui.spinner_text.is_some());

        ui.clear_spinner();
        assert!(ui.spinner_text.is_none());
    }

    #[test]
    fn test_add_components() {
        let mut ui = UIManager::new(false);

        ui.add_user_input("test input".to_string());
        assert_eq!(ui.history.len(), 1);

        ui.add_response("test response".to_string());
        assert_eq!(ui.history.len(), 2);
    }

    #[test]
    fn test_bash_lifecycle() {
        let mut ui = UIManager::new(false);

        ui.start_bash("echo test");
        assert!(ui.current_bash.is_some());

        ui.push_bash_output("test".to_string());
        ui.set_bash_elapsed(0.5);

        ui.finalize_bash(0.5, true, false);
        assert!(ui.current_bash.is_none());
        assert_eq!(ui.history.len(), 1);
    }

    #[test]
    fn test_streaming_response() {
        let mut ui = UIManager::new(false);

        ui.start_streaming_response();
        assert!(ui.current_response.is_some());

        ui.push_response_chunk("Hello ");
        ui.push_response_chunk("World");

        ui.finalize_response();
        assert!(ui.current_response.is_none());
        assert_eq!(ui.history.len(), 1);
    }

    #[test]
    fn test_next_component_id() {
        let id1 = next_component_id();
        let id2 = next_component_id();
        assert!(id2.0 > id1.0);
    }

    #[test]
    fn test_cursor_position_with_spinner() {
        let mut ui = UIManager::new(false);

        ui.add_response("Line 1".to_string());
        ui.set_spinner("Loading...".to_string());
        ui.push_input_char('h');
        ui.push_input_char('i');

        let (lines, cursor) = ui.render_all();

        // Cursor should be positioned at the input area
        // Not at the spinner or earlier content
        assert!(cursor.0 > 0, "Cursor row should be > 0");
        assert!(
            cursor.0 < lines.len(),
            "Cursor should be within rendered lines"
        );
    }

    #[test]
    fn test_cursor_position_multiline_input() {
        let mut ui = UIManager::new(false);

        ui.push_input_char('a');
        ui.insert_newline();
        ui.push_input_char('b');
        ui.insert_newline();
        ui.push_input_char('c');

        let (lines, cursor) = ui.render_all();

        // Editor has borders + 3 lines of input
        // Should have at least 5 lines total (top border, 3 input lines, bottom border)
        assert!(
            lines.len() >= 5,
            "Should have at least 5 lines (borders + 3-line input)"
        );
        // Cursor column should be after 'c' which is at position 1 ("> c|")
        assert_eq!(
            cursor.1, 3,
            "Cursor column should be at position 3 (after '> c')"
        );
    }

    #[test]
    fn test_cursor_position_empty_history() {
        let mut ui = UIManager::new(false);

        ui.push_input_char('x');

        let (_lines, cursor) = ui.render_all();

        // With only input, cursor should be after border (row 1)
        // Editor renders: top border, "> x|", bottom border
        assert!(cursor.0 >= 1, "Cursor should be after top border");
        assert_eq!(
            cursor.1, 3,
            "Cursor column should be at position 3 (after '> x')"
        );
    }

    // ── Scroll tests ──
}
