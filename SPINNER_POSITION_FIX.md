# Spinner Positioning Fix

## Change

Moved spinner above the top horizontal line and removed the "  " prefix.

## Before

```
────────────────────────────────────
                                   
 ────────────────────────────────────
> user input
  ⠋ Thinking...
```

**Problems:**
- Spinner below bottom border (looks like it's outside input area)
- Two-space prefix inconsistent with clean design
- Spinner mixed with input content

## After

```
⠋ Thinking...
────────────────────────────────────
                                   
 ────────────────────────────────────
> user input
```

**Benefits:**
- Spinner above input area (clear status indicator)
- No prefix (cleaner, aligned left)
- Visual separation between status and input

## Implementation

### 1. Removed Spinner from Editor

**Before (`src/ui/editor.rs`):**
```rust
pub fn render(
    &self,
    width: u16,
    use_colors: bool,
    spinner: Option<String>,  // ← Removed
    hint: Option<String>,     // ← Removed
) -> (Vec<String>, usize, usize) {
    let (mut lines, cursor) = self.render_with_border(width, use_colors);
    
    // Add spinner below border
    if let Some(spinner_text) = spinner {
        lines.push(format!("  {}", spinner_text));  // ← Removed
    }
    
    (lines, cursor.0, cursor.1)
}
```

**After:**
```rust
pub fn render(
    &self,
    width: u16,
    use_colors: bool,
) -> (Vec<String>, usize, usize) {
    let (lines, cursor) = self.render_with_border(width, use_colors);
    (lines, cursor.0, cursor.1)
}
```

### 2. Added Spinner as Separate Component

**`src/ui/manager.rs` - render_all():**
```rust
// Render spinner if present (above input editor)
if let Some(ref spinner_text) = self.spinner_text {
    let spinner_lines = vec![spinner_text.clone()];  // No prefix!
    components.push((spinner_lines, Spacing::none()));
}

// Render input editor
let (input_lines, cursor_pos) = // ...
components.push((input_lines, Spacing::none()));
```

### 3. Adjusted Cursor Position Calculation

**Account for spinner in cursor position:**
```rust
let content_lines = final_lines.len();
let spinner_offset = if self.spinner_text.is_some() { 1 } else { 0 };
let abs_cursor_row = content_lines.saturating_sub(input_len + spinner_offset) 
                   + cursor_pos.0 
                   + spinner_offset;
```

**Logic:**
- If spinner present, add 1 to offset
- This ensures cursor lands on correct row in input editor
- Spinner doesn't affect cursor position (it's above)

## Visual Flow

**Component stacking order:**
```
1. History items (user/AI messages)
2. Current bash output
3. Current streaming response
4. Spinner (if present)        ← NEW POSITION
5. Input editor
   - Top border
   - Input content
   - Bottom border
```

## Edge Cases

### Spinner + Fuzzy Search
```rust
if let Some(ref spinner_text) = self.spinner_text {
    components.push((spinner_lines, Spacing::none()));
}

if let Some(ref fuzzy) = self.fuzzy {
    // Spinner above fuzzy search too
}
```

### No Spinner
```rust
spinner_offset = 0
// Cursor calculation works normally
```

### Spinner Changes
When spinner text updates:
- Old spinner line: replaced with new text
- Diff renderer detects change at that line
- Updates in-place (no scrolling)

## Testing

All 117 tests pass ✅

**Manual verification:**
```bash
cargo build --release
./target/release/wat
# Type message
# Watch spinner appear above top border
# Verify no "  " prefix
```

## Files Modified

- `src/ui/editor.rs`: Removed spinner/hint params from render()
- `src/ui/manager.rs`: 
  - Add spinner as separate component before input
  - Adjust cursor calculation with spinner_offset

## Result

✅ Spinner appears above input area  
✅ No indentation prefix  
✅ Clear visual separation  
✅ Cursor positioning correct  
✅ All tests passing
