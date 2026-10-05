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
The pet now opens where the user left it in either shell, and `config.json` stays untouched unless the user changes
something. After a drop or Reset position, both shells save the whole window's top-left corner in desktop pixels.
The desktop shell (Windows, X11) and the layer shell (Wayland) read that saved corner the same way. The desktop
shell's first start and its Reset position use the screen's work area, so the pet sits above the taskbar.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

### What changed

- `aipet/src/config.rs`: `Store::keep` changes the config in memory and marks it dirty, and the next `flush` writes it.
  `Store::change` keeps and then flushes, so the settings (`Pills`, `OnTop`, `Avatar`, `Music`) are still written at
  once. A failed write leaves the store dirty for the next flush.
- `aipet/src/placement.rs`:
  - `load` reads `Left` and `Top`, and takes 42 off `WindowHeight` when `Toolbar` is true, as PlaceWindow does. It
    gives no place unless both `Left` and `Top` are set.
  - `save` keeps `Left`, `Top` and `WindowHeight`, clears `Toolbar` as SaveConfig does, and writes at once. Both
    shells call it after a drop and after Reset position.
  - `reset` forgets the place without writing. The shell then saves the corner it moved the pet to, which writes it.
    When a shell can't name that corner, the quit writes the forgotten place and the pet starts at home next time.
  - `quit` writes whatever is still unwritten.
- `aipet-desktop/src/shell.rs`:
  - The window still opens hidden on the primary monitor. On `PetOpened` the shell reads the scale, the window's
    position and the screens, moves the window to `Located::start` (a port of PlaceWindow), and then shows it.
  - `Located::start` keeps the C#'s `(int)` truncations, the `WindowHeight` and `Toolbar` adjustment, and the
    visibility test against every screen. With no place, or one that fails the test, the window goes to the bottom of
    the work area, 24 px from its right edge.
  - Reset position moves the window to `Located::corner` and saves that place. `Located::corner` is a port of
    ResetPosition and rounds half to even, as .NET's Math.Round does.
  - The work area is that of the screen the window overlaps most, else the primary screen's, else 1920 × 1080 at the
    origin (MainWindow.WorkArea).
  - The screen list comes from `EnumDisplayMonitors` and `GetMonitorInfoW` on Windows, and from RandR 1.5
    `GetMonitors` intersected with `_NET_WORKAREA` on X11. On macOS, or when that query fails, the window's monitor
    at the origin stands in, and the shell reports a failure once on stderr.
- `aipet-wayland/src/drag.rs`:
  - `Screen` holds an output's logical position, size and scale. A desktop px is a logical px times the scale.
  - `home_for(place)` turns a place into a home (the surface's margins), with the C# height adjustment and visibility
    test and the drag's clamp. `place_at(home)` turns a home back into desktop px.
  - `scale_of` derives an output's scale from its current mode and logical size: the ratio of their longer sides,
    snapped to 1/120. A zero size gives no scale.
  - `Drag::place` moves the home only at rest. `Drag::destination` is where a drop leaves the surface, including an
    overlay drag that was released but hasn't landed yet.
- `aipet-wayland/src/shell.rs`:
  - The shell keeps a `Screen` for each output from the shell broadcast (logical position, logical size, current
    mode).
  - With a saved place, the shell makes the pet's surface only once an output accepts that place, and makes it there
    (`OutputOption::GlobalName`). If no known output accepts the place by the first retry tick, 1 s after start, the
    surface opens at home on the output the compositor picks. That covers an unknown output or scale.
  - A drop saves `destination()` in desktop px through the pet's output.
  - Reset position moves the home back to the corner and saves it. It acts only at rest, with no drag on and the
    surface not being made. A reset before the pet shows also drops the saved place, so the pet comes up at home.

### Acceptance criteria

- **Round trip at scales 1, 1.25 and 1.5:** `drag::tests::a_place_goes_to_margins_and_back_at_scales_1_1_25_and_1_5`
  uses two side-by-side outputs per scale.
  - Margins to px and back give the same margins.
  - Px to margins and back give the same px at scale 1. At fractional scales they stay within 1 px, because a px can
    fall between two logical ones.
  - It pins worked numbers: (1500, 600) px at 1.5 gives margins (400, 327, 67, 1000). It also pins the second
    output's px origin, and the rounding of 1954.5 px up to 1955.
- **Switching `AIPET_BACKEND` between `wayland` and `desktop`:** not run here, since there is no Linux session. The
  checks are listed below for task 22.
- **An old `Toolbar: true` config, and an off-screen place:**
  - `placement::tests::the_place_is_read_as_the_csharp_reads_it_old_toolbar_and_all`.
  - The desktop shell's `shell::tests::the_window_goes_where_the_pet_was_left_as_placewindow_puts_it`: scales 1,
    1.25 and 1.5, WindowHeight 420, Toolbar 378 (277.5 truncated to 277), another screen, off-screen, an infinite
    height, and PlaceWindow's exact edges.
  - `drag::tests::an_old_toolbar_place_moves_up_as_placewindow_has_it_and_one_off_the_output_has_no_home`.
- **Starting and quitting leave `config.json` as it is, and one setting change writes it:**
  `placement::tests::starting_and_quitting_leave_config_json_as_it_is_until_a_setting_changes`.
  - It runs a missing, a corrupt and a valid file through the app's start and quit, twice each.
  - The valid file is indented and carries an extra field, so any write would change its bytes.
  - Each file stays byte-identical, or absent, and one setting change then writes it.

### Verification at cb7c007

- **Rust gate:** green, with 343 passed, 0 failed and 1 ignored. The green receipt is `cb7c0075-unittest`. The command:
  `cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check`
- **C# gate:** green, with 160 passed and 52 skipped. The green receipt is `cb7c0075-dotnet`. The command:
  `dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build`
- **Baseline at e3c3e82:** green.
  - The same Rust command stopped twice on aipet-core's server tests, which time out while the sibling builds load the
    machine. Those tests passed alone.
  - `cargo test --workspace --no-fail-fast` then gave 337 passed, with clippy `-D warnings` and fmt clean.
  - The C# suite gave 160 passed and 52 skipped.
- **drag.rs on Windows:** 18 tests passed. They run through a scratch crate whose library is the worktree's drag.rs.
- **Cross-target clippy `-D warnings` against the real crates:** clean. The commands used `--locked` and left
  Cargo.lock unchanged, with task 15's stand-ins for the C compiler, ar and pkg-config only.
  - Linux: aipet-wayland, aipet-desktop, aipet-ui and aipet, all targets.
  - macOS: aipet-desktop, aipet-ui and aipet, all targets.
- **Mutation checks:** the tests caught all 12 mutations.
  - Desktop (5): move-up rounding, no visibility test, screen bounds in place of the work area, the corner rounded
    half away from zero, and the window's screen ignored.
  - drag.rs (7): no visibility test, no clamp, a truncated place, an unsnapped scale, the destination missing the
    return home, a place during a drag, and an unscaled px origin.
  - Earlier, the placement and config tests caught their mutations: 5 of 6 tests failed as intended.
  - The 4 new Wayland shell tests compile under the Linux cross-check but have not run. Nothing here can run Linux test
    binaries, so CI's Linux job is their first run, and their mutations are unchecked.

### Commits

- 58813e8: wip(placement), the implementation.
- cb7c007: fix(placement), the review fixes. Reset position on Wayland now acts only at rest and drops a place still
  waiting for its output. Two drag.rs rules are now pinned.

### Manual checks for the hands-on passes

Run these from Git Bash at the repository root. Quit any other pet first.

- **Windows (task 27):** run `(cd rust && cargo build -p aipet) && rust/target/debug/AiPet.exe`.
  1. The first start puts the pet at the bottom-right of the work area, above the taskbar.
  2. Drag the pet, quit, and start again. The pet is where it was left.
  3. Reset position puts it back above the taskbar, on the monitor it is on.
  4. An old `config.json` with `"Toolbar":true` and no `WindowHeight` places the pet where the .NET pet puts it.
  5. A `Left` of 99999 resets the pet to the corner.
  6. Start and quit without a change. `config.json` keeps its modified time.
  7. With two monitors at different scales, a place on the second monitor survives a restart. Multi-monitor placement
     follows the C#'s rules and wasn't redesigned.
- **Linux (task 22):**
  1. Run `AIPET_BACKEND=wayland cargo run -p aipet` in `rust/`, drag the pet, and quit.
  2. `AIPET_BACKEND=desktop cargo run -p aipet` opens the window with the same top-left corner. The pet itself sits in
     the same place only when XWayland's scale equals the output's. SPIKE.md saw 1.5 on a scale-1 output.
  3. Switch back, then repeat at output scales 1.25 and 1.5.
  4. Leave the pet on a second output and restart: it opens there. With that output unplugged, it opens at home about
     1 s after start.
  5. Choose Reset position from Settings opened at start (`AIPET_OPEN_SETTINGS=1`) before the pet shows. The pet comes
     up at home.

### Choices to review

- **Reset position forgets the place, and the shell's save of the corner writes it.** A shell that can't name the
  corner still starts the pet at home next time. The C# writes the corner directly.
- **A half place gives the default corner.** A place with only `Left` or only `Top` set gives no place. The C# would
  use the one coordinate it has.
- **On Wayland a desktop px is a logical px times the output's scale.** That matches X11 under XWayland with
  zero-scaling, and Windows.
  - With Hyprland's `xwayland:force_zero_scaling` off on a scaled output, X11's pixels are logical ones, so the two
    shells disagree by the scale.
  - With several outputs, an output's px origin is its logical position times its scale. That wasn't redesigned.
- **The Wayland shell waits for its outputs.** A pet left on an unplugged output opens at home about 1 s late, at the
  first retry tick.
- **The desktop shell holds its own screen query** in shell.rs, because `native/` was outside the Touches. It could
  move to `native/` later.

### Follow-ups (outside the Touches)

- Run `cargo test -p aipet-wayland` on Linux. CI does this, and it covers the 4 new shell tests.
- The work branch declares x11rb's `randr` feature (4e1c4b1) for the X11 screen query. This branch compiles without
  that declaration, through winit's x11rb.

Review fix (758bb86): Wayland outputs side by side in desktop px (drag::arrange), visible saved places used as is, Reset at any time, a drop on an unknown output forgets the place.

stage: impl-review - ran (codex: NEEDS_WORK then SHIP in 2 rounds)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 58813e8d5058c5d69aeffc79d946aa8574b1b3da, cb7c0075d657ec0189b6516232e499daa093e294, 758bb86
- Tests: baseline: green at e3c3e82 (cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check stopped twice on aipet-core server tests that time out under the sibling builds; they passed alone, and cargo test --workspace --no-fail-fast with clippy -D warnings and fmt --check gave 337 passed, clean; dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build gave 160 passed, 52 skipped), cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check: green at cb7c007, 343 passed, 0 failed, 1 ignored (green receipt cb7c0075-unittest), dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build: green at cb7c007, 160 passed, 52 skipped (green receipt cb7c0075-dotnet), aipet-wayland drag.rs through a scratch crate on Windows (cargo test --offline, [lib] path = the worktree drag.rs): 18 passed, cross-target clippy -D warnings --locked --offline with the real crates (task 15 cc/ar/pkg-config stand-ins only): x86_64-unknown-linux-gnu -p aipet-wayland -p aipet-desktop -p aipet-ui -p aipet --all-targets clean; aarch64-apple-darwin -p aipet-desktop -p aipet-ui -p aipet --all-targets clean, mutation checks: 12 of 12 caught (aipet-desktop shell 5, drag.rs 7); placement and config earlier: 5 of 6 tests failed as intended, not run here: aipet-wayland shell.rs tests (Linux only; 4 new, compiled by the Linux cross clippy) and the manual AIPET_BACKEND switch check (task 22), integrated verify (Windows, work branch 1d84482 with tasks 16, 17 and 18 and their review fixes merged): cargo test --workspace --no-fail-fast -- --skip credential_manager_tokens_are_the_apps green, clippy -D warnings and fmt clean; at fd770c6: dotnet build and test 160 passed, 53 skipped, and AIPET_TEST_HOOK=<.NET hook dll> cargo test -p aipet-core --test server green; the full gate's antivirus failures under load (two C# RegistrationTests with 'Access to the path is denied', the live registration and doctor replays, claude_registration_is_the_csharps) all passed when rerun alone at 73c1310 and f0ebc65
- PRs: