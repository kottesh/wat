# Cursor Positioning Fix

## Problem

On app startup, UI was broken:
1. Cursor appeared at top of screen (row 1)
2. Input prompt was not where cursor was
3. Two horizontal lines with huge gap between them

Example:
```
[compiler warnings from cargo run]
[compiler warnings]
[cursor here at row 1] <-- WRONG

────────────────────────────────────  <- Line drawn at row 50
                                     
 ────────────────────────────────────  <- Input prompt at row 52
```

## Root Causes

### Issue 1: `move_cursor()` Skipped on First Render

**Code:**
```rust
fn move_cursor(&mut self, pos: CursorPos) {
    if self.previous_lines.is_empty() {
        return;  // <-- BUG: Skips cursor positioning!
    }
    // ...
}
```

**Why:** On first render, `previous_lines` is empty, so cursor never gets positioned.

**Result:** Cursor stays at row 1 (default terminal position).

### Issue 2: No Screen Clear on Startup

**Problem:** 
- Cargo outputs warnings/build info
- Terminal cursor ends up at row 50+ after all that output
- App starts rendering with absolute positioning at row 1
- But content gets drawn where cursor currently is (row 50+)
- Massive disconnect between row numbers and actual position

**Example:**
```
Terminal state before app starts:
Row 1:  warning: unused import
Row 2:  --> src/ui/mod.rs:9:17
...
Row 45: Finished `dev` profile
Row 46: Running `target/debug/wat`
Row 47: [cursor here]

App renders:
\x1b[1;1H  -> "Go to row 1, col 1"
           -> But terminal interprets this relative to current screen position
           -> Results in weird offset
```

## Fixes

### Fix 1: Remove Empty Check in `move_cursor()`

```rust
// OLD
fn move_cursor(&mut self, pos: CursorPos) {
    if self.previous_lines.is_empty() {
        return;  // REMOVED
    }
    let (row, col) = pos;
    print!("\x1b[{};{}H", row + 1, col + 1);
    // ...
}

// NEW  
fn move_cursor(&mut self, pos: CursorPos) {
    let (row, col) = pos;
    print!("\x1b[{};{}H", row + 1, col + 1);
    // ...
}
```

**Result:** Cursor ALWAYS gets positioned, even on first render.

### Fix 2: Clear Screen on Startup

```rust
// agent.rs - main_loop()
async fn main_loop(&mut self, input_rx: ...) -> Result<()> {
    // Clear screen on startup
    {
        let mut r = self.renderer.lock().unwrap();
        r.force_redraw();  // <-- Clears screen + scrollback
        r.render();
    }
    
    loop {
        // ...
    }
}
```

**What `force_redraw()` does:**
```rust
fn force_clear(&mut self) {
    print!("\x1b[3J\x1b[2J\x1b[H");
    //      ^^^^^^  ^^^^^^  ^^^^^^
    //      Clear   Clear   Move to
    //      scrollback visible home (1,1)
}
```

**Result:** 
- Clears all cargo output
- Resets cursor to (1,1)
- Fresh screen for UI

## How It Works Now

**App startup sequence:**
```
1. Cargo compiles and runs
   - Warnings/output appear
   - Cursor at row 50+

2. Agent::run_interactive()
   - Enters raw mode
   - Calls force_redraw()
     -> Clears screen
     -> Moves cursor to (1,1)
   - Calls render()
     -> Draws UI at rows 1-N
     -> Positions cursor at input line

3. User sees clean UI:
   ────────────────────────────────────
   
    ────────────────────────────────────
   > [cursor here]
```

## Why This Is Correct

### Absolute Positioning Assumptions

When we do `\x1b[5;10H` (go to row 5, col 10), this means:
- Row 5 from the **top of the visible screen**
- NOT from top of scrollback
- NOT from current cursor position

### Without Clear

If terminal shows:
```
Row 1 (on screen): [cargo warning]
Row 2 (on screen): [cargo warning]  
...
Row 24 (on screen): Running target/debug/wat
```

And we do `\x1b[1;1H`, terminal goes to **row 1 of CURRENT screen**.

But content might scroll, and absolute rows refer to visible screen, creating confusion.

### With Clear

After `\x1b[2J\x1b[H`:
```
Row 1 (on screen): [empty]
Row 2 (on screen): [empty]
...
Row 24 (on screen): [empty]
Cursor at: (1,1)
```

Now `\x1b[1;1H` unambiguously means top-left corner.

## Testing

```bash
cargo build --release
./target/release/wat
```

**Expected:**
1. Screen clears
2. UI appears cleanly with two horizontal lines
3. Cursor is between them at input prompt
4. No cargo warnings visible

**Result:** ✅ Works correctly

## Summary

**Two bugs fixed:**
1. ✅ Removed `is_empty()` check preventing cursor positioning on first render
2. ✅ Added `force_redraw()` on startup to clear cargo output

**Files modified:**
- `src/ui/diff.rs` - Removed empty check in `move_cursor()`
- `src/agent.rs` - Added `force_redraw()` before first render

**Result:** Clean UI on startup with cursor in correct position.
