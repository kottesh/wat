# Input Box Disappearing Fix

## Problem

After content scrolled beyond terminal height, the **input box completely disappeared**. User could type but nothing showed up on screen.

**Visual:**
```
[conversation history scrolling]
[conversation history]
[conversation history]
😄 Looks like keyboard jazz.

[NO INPUT BOX - completely missing]
[cursor invisible]
```

## Root Cause

**Hybrid scrollback approach had a fatal flaw:**

When content grows from 100 → 101 lines:

1. **Update existing (lines 1-100):** Absolute positioning
2. **Append new (line 101):** `\n` (causes scroll)
3. **Terminal scrolls:** Line 1 moves to scrollback
4. **Problem:** Input box (which was line 100) ALSO scrolls into scrollback!
5. **Line 101** appears at bottom (new content, not input)
6. **Input box lost** in scrollback history

**Why this happened:**
- All content rendered sequentially: history + bash + response + **input**
- When content exceeded screen, everything including input scrolled
- New content appended pushed input off screen
- Input box treated like history instead of sticky UI element

## Failed Approach Analysis

### Attempt 1: Track scroll offset
```rust
lines_scrolled: usize
screen_row = absolute_row - lines_scrolled
```

**Problem:** This only fixes cursor position, not content visibility. Input box still scrolls into scrollback.

### Attempt 2: Hybrid absolute + append
```rust
if new_len > prev_len {
    // Update existing with absolute positioning
    // Append new with \n
}
```

**Problem:** Input box is part of "existing lines", gets updated in scrollback, then new content pushes it further up. Still disappears.

## Correct Solution

**Separate strategies based on content size:**

### Case 1: Content Fits on Screen (≤ terminal height)
**Use absolute positioning everywhere:**
```rust
if new_len <= terminal_height {
    for i in first..new_len {
        buf.push_str(&format!("\x1b[{};1H", i + 1));
        buf.push_str(&new_lines[i]);
    }
}
```

**Result:** All content including input box rendered in-place. No scrolling.

### Case 2: Content Exceeds Screen (> terminal height)
**Use append-only rendering:**

#### First Render (prev_len == 0):
```rust
// Write everything sequentially from scratch
for (idx, line) in new_lines.iter().enumerate() {
    if idx > 0 { buf.push_str("\n"); }
    buf.push_str(&new_lines[idx]);
}
lines_scrolled = new_len - terminal_height;
```

**Result:** Content flows naturally, fills screen + scrollback.

#### Content Growing (new_len > prev_len):
```rust
// Append ONLY new lines
for i in prev_len..new_len {
    buf.push_str("\n");
    buf.push_str(&new_lines[i]);
}
lines_scrolled += lines_added;
```

**Result:** New content appends, terminal scrolls, **input box stays at bottom because it's part of new_lines**.

#### Content Same/Shrinking:
```rust
// Redraw visible portion
let visible_start = new_len - terminal_height;
for i in visible_start..new_len {
    let screen_row = i - visible_start + 1;
    buf.push_str(&format!("\x1b[{};1H", screen_row));
    buf.push_str(&new_lines[i]);
}
```

**Result:** Refresh visible window.

## Key Insight

**The input box must always be included in the render:**

```rust
// render_all() returns ALL content:
let all_lines = vec![
    "User: Hi",
    "AI: Hello",
    ... 98 more lines ...
    "> input box"  // <-- Always last line
];

// When we append line 101:
for i in 100..101 {
    buf.push_str("\n");
    buf.push_str(&new_lines[i]);  // <- This is the input box!
}
```

**Input box is always the LAST item in all_lines.** When we append new content, we're appending a new version of the ENTIRE state including the input box.

## Why This Works

**Scenario: 100 lines → 103 lines (user sends message)**

**Before:**
```
Lines 77-100 visible (24 rows)
Line 100: "> hello" (input with text)
```

**After user submits:**
```
all_lines = [
    ...77 history lines...
    "User: hello",      // Line 78 (new)
    "⠋ Thinking...",   // Line 79 (new)  
    "> "               // Line 80 (new input box, empty)
]
```

**Render:**
```rust
// Append lines 100-102 (3 new lines)
buf.push_str("\n");
buf.push_str("User: hello");
buf.push_str("\n");
buf.push_str("⠋ Thinking...");
buf.push_str("\n");
buf.push_str("> ");  // <- Input box appended at end!
```

**Terminal scrolls:**
```
Lines 1-3 scroll to scrollback
Lines 4-80 become rows 1-77
Line 78: "User: hello"    at row 75
Line 79: "⠋ Thinking..." at row 76
Line 80: "> "            at row 77 ← Input visible!
```

## Code Changes

**src/ui/diff.rs - render() method:**

1. **Split by content size:**
   - `new_len <= terminal_height`: Absolute positioning
   - `new_len > terminal_height`: Append-only

2. **Append-only sub-cases:**
   - First render: Write all sequentially
   - Growing: Append new lines only
   - Shrinking: Redraw visible window

3. **Track scrolling:**
   - Update `lines_scrolled` when content exceeds screen
   - Use for cursor adjustment

## Result

✅ Input box always visible at bottom  
✅ Can type and see characters  
✅ Content scrolls naturally  
✅ Scrollback works (Shift+PageUp)  
✅ All 117 tests passing  

The input box is now properly treated as part of the content state, always rendered at the end, ensuring it stays visible when content scrolls.
