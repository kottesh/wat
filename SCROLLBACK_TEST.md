# Scrollback Feature Testing Guide

## Implementation Summary

Added virtual scrollback support to the UI manager with the following features:

### New State
- `scroll_offset: usize` - Lines scrolled up from bottom (0 = at bottom)
- `auto_scroll: bool` - Whether to stick to bottom on new content

### Keybindings Added

| Key | Action |
|-----|--------|
| `Ctrl+Y` | Scroll up 1 line |
| `Ctrl+E` | Scroll down 1 line |
| `Ctrl+B` | Scroll up one page |
| `Ctrl+D` | Scroll down one page |
| `Ctrl+T` | Scroll to top |
| `Ctrl+G` | Scroll to bottom |

### Auto-Scroll Behavior
- Any input (typing, backspace, newline) → auto-scroll to bottom
- Scrolling up → disable auto-scroll (content stays in view)
- Scrolling to bottom (Ctrl+G or naturally) → re-enable auto-scroll
- New content arrives while scrolled → position maintained
- New content arrives at bottom → stays at bottom

### Scroll Indicator
When scrolled up, a banner appears at the top showing:
```
[↑ Scrolled 50/200 lines (25%) | Ctrl+G: bottom | Ctrl+T: top]
```

## Manual Testing Steps

1. **Start the application:**
   ```bash
   cargo run
   ```

2. **Generate content:**
   - Type some commands that produce lots of output
   - Example: `bash ls -la /usr/bin` or `bash find /usr -type f | head -100`
   - Or: Have a conversation that generates many components

3. **Test scroll up:**
   - Press `Ctrl+Y` several times → should scroll up line by line
   - Press `Ctrl+B` → should scroll up one page
   - Verify indicator appears at top

4. **Test scroll down:**
   - Press `Ctrl+E` several times → should scroll down
   - Press `Ctrl+D` → should scroll down one page
   - When reaching bottom, indicator should disappear

5. **Test scroll to extremes:**
   - Press `Ctrl+T` → should jump to top of history
   - Press `Ctrl+G` → should jump to bottom (input visible)

6. **Test auto-scroll on input:**
   - Scroll up with `Ctrl+Y`
   - Start typing → should auto-scroll to bottom
   - Verify input is visible and cursor correct

7. **Test content addition while scrolled:**
   - Scroll up to middle of history
   - Submit a command
   - While AI is responding, verify scroll position holds
   - (Or scroll up after response starts streaming)

8. **Test with streaming responses:**
   - Start a conversation
   - Ask: "Count from 1 to 100 in your response"
   - While streaming, try `Ctrl+Y` to scroll up
   - Verify you can read earlier content while stream continues

## Code Changes

### Files Modified:
1. `src/ui/manager.rs` (+140 lines)
   - Added scroll state fields
   - Added scroll methods (scroll_up, scroll_down, etc.)
   - Modified apply_viewport to use scroll_offset
   - Added scroll_indicator rendering
   - Updated input methods to trigger auto-scroll
   - Updated content addition to respect auto_scroll

2. `src/terminal.rs` (+42 lines)
   - Added 6 new keybindings for scroll control

### Tests Added:
- 14 new unit tests covering:
  - Initial state
  - Scroll up/down
  - Scroll to extremes
  - Auto-scroll triggers
  - Viewport calculation with offset
  - Scroll indicator formatting
  - Scroll clamping

### Total Impact:
- ~182 lines of implementation
- 14 new tests
- All 131 tests passing
- Zero breaking changes to existing functionality
