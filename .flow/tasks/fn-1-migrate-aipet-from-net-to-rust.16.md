---
satisfies: [R10, R11]
---
# fn-1-migrate-aipet-from-net-to-rust.16 Placement and config persistence across the layer-shell and winit shells

## Description
Load and save `config.json` through the core, and place the pet where the user left it. The saved position is the full
window's top-left corner in desktop pixels, and both shells must read it the same way.

**Size:** M
**Files:** `rust/crates/aipet/src/placement.rs`, `rust/crates/aipet-wayland/src/{shell,drag}.rs`, `rust/crates/aipet-desktop/src/shell.rs`, `rust/crates/aipet-ui/src/lib.rs`
**Touches:** [rust/crates/aipet/src/placement.rs, rust/crates/aipet/src/main.rs, rust/crates/aipet-wayland/src/shell.rs, rust/crates/aipet-wayland/src/drag.rs, rust/crates/aipet-desktop/src/shell.rs, rust/crates/aipet-ui/src/lib.rs]

### Approach
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
