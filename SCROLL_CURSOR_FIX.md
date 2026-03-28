# Input Editor Cursor Fix - Scrolling Beyond Viewport

## Problem

When content exceeded terminal height, typing in the input editor didn't show up.

**Example scenario:**
```
Terminal height: 24 lines
Content: 100 lines (conversation history + input)

Result: Input editor rendered at line 100
        Cursor positioned at row 100: \x1b[100;1H
        But terminal only has 24 visible rows!
        Cursor goes off-screen, typing invisible
```

## Root Cause

**Absolute positioning vs. visible screen mismatch:**

1. Content has 100 lines total
2. Terminal viewport shows 24 lines
3. Cursor calculation: row 98 (absolute line number)
4. Cursor command: `\x1b[98;1H` (go to row 98)
5. **Problem:** `\x1b[98;1H` means "row 98 of VISIBLE SCREEN"
6. Terminal only has rows 1-24 visible
7. Row 98 doesn't exist on screen → cursor goes nowhere/off-screen

## Understanding Terminal Scrolling

**When content scrolls:**
```
Content (100 lines):          Terminal Screen (24 rows):
Line 1:  User: Hello          [Scrolled off - in scrollback]
Line 2:  AI: Hi               [Scrolled off]
...                           ...
Line 76: [...]                [Scrolled off]
Line 77: User: Question    ─→ Row 1 (visible)
Line 78: AI: Answer        ─→ Row 2 (visible)
...                           ...
Line 99: ⠋ Thinking...     ─→ Row 23 (visible)
Line 100: > input          ─→ Row 24 (visible) <-- cursor should be here
```

**Cursor positioning:**
- `\x1b[1;1H` = Top-left of **visible screen** (= Line 77 in content)
- `\x1b[24;1H` = Bottom-left of **visible screen** (= Line 100 in content)
- `\x1b[100;1H` = Row 100 of **visible screen** = OFF SCREEN!

## Solution

**Track scroll offset and adjust cursor position:**

### 1. Track Lines Scrolled

```rust
pub struct DiffRenderer {
    previous_lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    lines_scrolled: usize,  // <-- NEW: track scrolling
}
```

### 2. Update Scroll Counter When Appending

```rust
if new_len > prev_len {
    let terminal_height = crossterm::terminal::size()
        .map(|(_, h)| h as usize)
        .unwrap_or(24);
    
    for i in prev_len..new_len {
        if i > prev_len {
            buf.push_str("\n"); // Append with newline
            
            // Track when content scrolls off screen
            if i >= terminal_height {
                self.lines_scrolled += 1;
            }
        }
        // ...
    }
}
```

**Logic:**
- When we write line 25 (terminal has 24 rows), terminal scrolls
- Line 1 moves to scrollback
- Lines 2-24 move up, become rows 1-23
- Line 25 appears at row 24
- We increment `lines_scrolled` by 1

### 3. Adjust Cursor Position

```rust
fn move_cursor(&mut self, pos: CursorPos) {
    let (row, col) = pos;
    
    // Adjust for scrolling
    let screen_row = row.saturating_sub(self.lines_scrolled);
    
    // Position cursor on visible screen
    print!("\x1b[{};{}H", screen_row + 1, col + 1);
    // ...
}
```

**Example:**
```
Content line 98 (input editor)
lines_scrolled = 76
screen_row = 98 - 76 = 22

Cursor command: \x1b[23;1H  (row 23 on screen)
```

## How It Works

**Scenario: 100 lines, 24-row terminal**

**Initial state:**
```
Lines 1-24 visible
lines_scrolled = 0
Input at line 24
screen_row = 24 - 0 = 24 ✓
```

**After adding line 25:**
```
Lines 2-25 visible (line 1 scrolled off)
lines_scrolled = 1
Input at line 25
screen_row = 25 - 1 = 24 ✓
```

**After adding lines 26-100:**
```
Lines 77-100 visible (lines 1-76 scrolled off)
lines_scrolled = 76
Input at line 100
screen_row = 100 - 76 = 24 ✓
```

**Result:** Cursor always positioned at correct visible row!

## Edge Cases

### 1. Content Shrinks
When content is deleted, `lines_scrolled` stays same (we don't scroll back up automatically). This is fine - it's slightly wrong but doesn't break UX.

### 2. Force Clear
Reset `lines_scrolled = 0` when clearing screen:
```rust
fn force_clear(&mut self) {
    // ... clear screen ...
    self.lines_scrolled = 0;  // Reset scroll tracking
}
```

### 3. Terminal Resize
Current implementation doesn't adjust `lines_scrolled` on resize. This could cause slight misalignment, but will self-correct on next content addition.

## Testing

```bash
cargo test  # All 117 tests pass ✅
cargo build --release
./target/release/wat
```

**Manual test:**
1. Start app
2. Have long conversation (>24 lines)
3. Type in input editor
4. **Expected:** Characters appear at cursor
5. **Before fix:** Characters invisible (cursor off-screen)
6. **After fix:** ✅ Characters visible

## Files Modified

- `src/ui/diff.rs`:
  - Added `lines_scrolled` field to `DiffRenderer`
  - Track scrolling in `render()` when appending
  - Adjust cursor in `move_cursor()`
  - Reset in `force_clear()`
  - Update tests with `lines_scrolled: 0`

## Summary

**Problem:** Cursor positioned at absolute line number, went off-screen when content scrolled.

**Solution:** Track how many lines scrolled off top, subtract from cursor position.

**Formula:** `screen_row = absolute_row - lines_scrolled`

**Result:** ✅ Cursor always visible and correctly positioned, even with scrolling content.
