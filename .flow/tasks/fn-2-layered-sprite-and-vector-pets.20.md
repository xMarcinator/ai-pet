---
satisfies: [R23]
---
# fn-2-layered-sprite-and-vector-pets.20 Keep the pet on a visible screen when the displays change

## Description
When the displays change while the pet runs, re-check its position and bring it back onto a visible screen if it no longer shows (R23). Today placement only runs at start, so closing a laptop lid with an external screen left the pet running but invisible (owner, 2026-11-09, Windows 11).

**Size:** M
**Files:** `rust/crates/aipet-desktop/src/shell.rs`, `rust/crates/aipet-desktop/src/native/win32.rs`, `rust/crates/aipet-desktop/src/native/x11.rs`, `rust/crates/aipet-wayland/src/shell.rs`, `rust/crates/aipet-ui/src/lib.rs`
**Touches:** [rust/crates/aipet-desktop/src/**, rust/crates/aipet-wayland/src/**, rust/crates/aipet-ui/src/lib.rs]

### Approach
- Reuse the start rule in `shell.rs` (`start()`, `shows`, around lines 738-760) as a pure function on (screens, window position, scale), and the corner fallback (`corner()`), so start and display changes share one rule.
- Trigger:
  - **Windows:** `WM_DISPLAYCHANGE` and `WM_DPICHANGED` (the latter is already intercepted in `native/win32.rs`; see the DPI hold during drags), or winit's monitor and scale events if they cover a removed monitor.
  - **X11:** RandR `ScreenChangeNotify` / `CrtcChange` on the connection the shell already has (x11rb has `randr`).
  - **Wayland layer-shell:** `wl_output` removal, re-anchoring the layer surface to an output that exists.
- After a change, re-read the screens (`locate`), and move only if the pet doesn't show; then persist through the existing placement path, which writes config.json only on a real move.
- Mid-drag changes wait for the drop (follow the existing DPI hold).

### Investigation targets
**Required:**
- `rust/crates/aipet-desktop/src/shell.rs:715-790` — `work_area`, `start`, `corner`, `locate`
- `rust/crates/aipet-desktop/src/native/win32.rs` — window message handling and the DPI hold
- `rust/crates/aipet-wayland/src/shell.rs` — layer surface and outputs

### Key context
The `shows` rule needs at least 80 px across and part of the bottom 120 px on some screen.

## Acceptance
- [ ] A unit test removes the pet's screen from the list and gets the corner on the remaining screen; a position that still shows is unchanged
- [ ] Windows: closing the lid (or disabling the pet's monitor) with another screen attached brings the pet onto that screen within a second; recorded in task 8's hands-on proof
- [ ] X11 and Wayland handle a removed output the same way (unit-tested; checked hands-on where a machine is available)
- [ ] config.json is written only when the pet actually moves

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
