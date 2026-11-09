---
satisfies: [R6, R10]
---
# fnt-75-the-pet-window-without-the-gpu.7 The Windows shell: a layered window, with the owner present

## Description
A topmost, non-activating, toolwindow layered window updated with `UpdateLayeredWindow` from a premultiplied BGRA DIB section, sized to its content. Built and tried with the owner on Windows (Git Bash only).

**Size:** M
**Files:** `rust/crates/aipet-desktop/src/native/win32.rs`, a Windows shell module, `rust/crates/aipet-desktop/src/native.rs`
**Touches:** [rust/crates/aipet-desktop/src/native/win32.rs, rust/crates/aipet-desktop/src/win_shell.rs, rust/crates/aipet-desktop/src/native.rs, rust/crates/aipet-desktop/src/lib.rs]

### Approach
- One DIB section in a memory DC kept between frames; `ULW_ALPHA` with `AC_SRC_ALPHA`; skip the call when nothing changed.
- Size the window to the union of the pet and its bubbles; move its origin so the pet stays put when it grows or shrinks.
- Per-pixel hit testing comes from alpha: drop the `SetWindowRgn` and `WS_EX_TRANSPARENT` toggling (`win32.rs:1-13,135-206`).
- Per-monitor DPI v2; hold a DPI change during a drag (`win32.rs:88-133`).

### Investigation targets
**Required:**
- `rust/crates/aipet-desktop/src/native/win32.rs`
- `rust/proofs/windows.md`, `rust/proofs/windows-hands-on.md`

### Key context
- `UpdateLayeredWindow` always updates the whole window: keep it small.
- windows-sys 0.61: check every BOOL return; `BOOL` moved to `windows_sys::core`.

## Acceptance
- [ ] On Windows the pet draws, clicks through on transparent pixels, drags, stays on top without taking focus, and handles DPI as R6 says (tried with the owner)
- [ ] win32 tests pass or are ported

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
