---
satisfies: [R10, R11]
---
# fn-1-migrate-aipet-from-net-to-rust.16 Placement and config persistence across the layer-shell and winit shells

## Description
Load and save `config.json` through the core, and place the pet where the user left it. The saved position is the full
window's top-left corner in desktop pixels, and both shells must read it the same way.

**Size:** M
**Files:** `rust/crates/aipet/src/{placement,config}.rs` (stubs from task 15), `rust/crates/aipet-wayland/src/{shell,drag}.rs`, `rust/crates/aipet-desktop/src/shell.rs`
**Touches:** [rust/crates/aipet/src/placement.rs, rust/crates/aipet/src/config.rs, rust/crates/aipet-wayland/src/shell.rs, rust/crates/aipet-wayland/src/drag.rs, rust/crates/aipet-desktop/src/shell.rs]

### Approach
- Task 15's groundwork already calls the placement hooks from `main.rs` (load at start, save on a drop and on
  quit, reset), gives `config.rs` its API (first writing at once) and has `PetUi` expose the window's size and a
  finished-drag event. This task implements placement and completes the config's write rules without touching
  `main.rs` or `lib.rs`; Settings (task 19) emits the reset-position action this task handles.
- Desktop shell: port `PlaceWindow`, `ResetPosition` and `SaveConfig` (`src/AiPet.UI/MainWindow.axaml.cs:160-201`). That
  covers the physical `Left`/`Top`, the `WindowHeight` and legacy `Toolbar` adjustments, the check that the pet is
  visible on some screen, and the reset.
- Layer-shell: convert the saved desktop rectangle into margins on the output that contains it, using the output's
  logical position, size and scale from `shell_broadcast`. Convert back when saving. An unknown output gives the
  default placement.
- The spec's config-write rule (R10):
  - Keep the config in memory with a dirty flag. A setting change, a finished drag, or Reset position sets it.
  - Write when the flag is set: at once for settings, and on a drop or on quit for the position.
  - Quitting without a change writes nothing. That holds even when task 10's loader reported `Missing` or `Corrupt`,
    so a corrupt file survives until the user changes something.
  - `Pills`, `OnTop`, `Avatar` and `Music` persist the same way.

### Investigation targets
**Required:**
- `src/AiPet.UI/MainWindow.axaml.cs:42-56, 150-201`
- `rust/crates/aipet-wayland/src/drag.rs` — margins and clamping
- `rust/notes/exwlshell.md` §7 (output info)
**Optional:**
- `rust/SPIKE.md` — the 1.5 vs 1 scale observed between the shells
**From task 15 (2026-10-01):** your files are the stubs `aipet/src/placement.rs` and `aipet/src/config.rs`, plus both
`shell.rs` files. `App` (main.rs) calls `placement::{load, save, reset, quit}` from `Host::{saved_place, save_place,
reset_place, quit}`. `config::Store { load, get, change, flush }` keeps a dirty flag and writes at once; `Place { left,
top, window_height }` is config.json's position. `PetUi::window_size()` gives the size, and `Effect::Dropped` is a
finished drag. The desktop shell already calls `save_place` after a drop and after Reset position but doesn't read
`saved_place` at boot; it starts 24 px from the right at the monitor's bottom (move it to the work area). The layer
shell only logs a drop, and its Reset position only calls `reset_place`.
## Acceptance
- [ ] Round-trip unit tests from the desktop rectangle to margins and back, at scales 1, 1.25 and 1.5.
- [ ] Manual: after switching `AIPET_BACKEND` between `wayland` and `desktop`, the pet opens in the same place.
- [ ] An old config with `Toolbar: true` places the pet as the C# does. An off-screen position resets.
- [ ] Start and quit without a change: a corrupt, missing or valid `config.json` is left untouched (byte-identical, or
      still absent). One setting change writes it.
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
