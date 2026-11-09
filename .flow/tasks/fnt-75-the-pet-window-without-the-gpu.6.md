---
satisfies: [R3, R5, R10]
---
# fnt-75-the-pet-window-without-the-gpu.6 The X11 shell: ARGB window, MIT-SHM and SHAPE, also for Wayland without layer-shell

## Description
A native X11 pet window with x11rb, replacing the winit desktop shell on Linux, and serving GNOME and other compositors without layer-shell through XWayland.

**Size:** M
**Files:** `rust/crates/aipet-desktop/src/{shell,native,native/x11}.rs`, `rust/crates/aipet-desktop/src/lib.rs`
**Touches:** [rust/crates/aipet-desktop/src/native/x11.rs, rust/crates/aipet-desktop/src/native.rs, rust/crates/aipet-desktop/src/x11_shell.rs, rust/crates/aipet-desktop/src/lib.rs, rust/crates/aipet-desktop/Cargo.toml, rust/Cargo.toml, rust/Cargo.lock]

### Approach
- Depth-32 TrueColor visual, its colormap, `border_pixel` at CreateWindow (else BadMatch); EWMH states and SHAPE input from `native/x11.rs:96-182`.
- MIT-SHM put of changed rects, waiting for ShmCompletion before reusing the segment; plain PutImage when the extension is missing.
- No compositing manager: log once, keep running.
- Placement and screens from `aipet-desktop/src/shell.rs:656-850` (`Located`, `Screen`, `Area`); drag via native pointer as today (:491).
- x11rb: today's pin has no `shm` feature; enable `shm` (and move to 0.14 if the workspace does) in `aipet-desktop/Cargo.toml`.
- `WAYLAND_DISPLAY` emptied before threads start (`aipet-desktop/src/lib.rs:5-8`).

### Investigation targets
**Required:**
- `rust/crates/aipet-desktop/src/native/x11.rs`
- `rust/crates/aipet-desktop/src/shell.rs:388-850`
- `rust/crates/aipet-desktop/src/native.rs:42-146`

### Key context
- Keep today's WM_CLASS so window rules keep matching.

## Acceptance
- [ ] On X11 and under XWayland (GNOME or Hyprland with `AIPET_BACKEND`) the pet draws, clicks through outside its art and bubbles, drags and stays above
- [ ] Missing MIT-SHM and a missing compositing manager behave as R5 says
- [ ] The desktop shell tests for placement and screens pass on the new shell

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
