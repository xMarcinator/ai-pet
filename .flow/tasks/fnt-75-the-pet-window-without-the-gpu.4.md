---
satisfies: [R7]
---
# fnt-75-the-pet-window-without-the-gpu.4 Settings in the pet process on iced's CPU renderer

## Description
Set up the process as task 9's spike proved: an iced daemon on the main thread drawing with tiny-skia, opening Settings on demand, and the pet's shell on a `pet` thread, with the two channels the spec's API Contracts describe. Settings' pages stay as they are; what changes is that they reach the pet through the channels instead of mutating `PetUi` directly.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/settings/**`, `rust/crates/aipet-ui/src/lib.rs`, `rust/crates/aipet/src/main.rs`, `rust/Cargo.toml`
**Touches:** [rust/crates/aipet-ui/src/settings/**, rust/crates/aipet-ui/src/lib.rs, rust/crates/aipet/src/**, rust/Cargo.toml, rust/Cargo.lock]

## Approach
- Follow the spike's prototype (spike branch, `spikes/ui-framework/settings-thread/`, commit 67dd686): the iced daemon with no window, a subscription on the pet-to-Settings channel that opens or raises the window and refreshes its state, and a sender for Settings' changes.
- iced's `tiny-skia` feature; the default run selects it (wgpu stays compiled in for `AIPET_BACKEND=iced` only, R9).
- Split `settings::update` (`lib.rs:968`, `settings/mod.rs:10-33,177-210`) into the window's own form state and the messages to the pet; the pet applies them as `PetUi` does today. `Settings::services` (watchers, `SecretStore`, `Http`) stay on the pet side; connection tests run where they run today.
- The pet sends the full state after every change, whatever made it.
- Quit ends the process directly, not through `iced::exit()` (it panicked in 2 of 4 spike runs when a window was opening).
- Every page: a round-trip test per page (General, Avatars, Jira, GitHub, About) that the state fills every field, each action reaches the pet as its message (including reset position, check for updates, restart to update), and a change made outside Settings refreshes the open window.
- Check GPU libraries: `grep -E 'libGL|libEGL|libvulkan|nvidia|libgallium|/dri/' /proc/<pet>/maps` empty with Settings open and closed.
- Until the new shells land (tasks 5-7), the iced pet shells keep working; this task changes how Settings is reached, not the pet window.

## Investigation targets
**Required:**
- `rust/proofs/ui-framework.md` - task 9's results
- `rust/crates/aipet-ui/src/settings/mod.rs` - pages, services
- `rust/crates/aipet-ui/src/lib.rs:415,968,1150` - Settings inside PetUi
- `rust/crates/aipet-ui/tests/settings.rs`, `tests/settings_services.rs` - behaviour to keep

## Key context
- macOS keeps its iced desktop shell on the main thread; the pet-thread layout applies to Linux and Windows (Windows untried: checked with the owner in task 7).

## Acceptance
- [ ] Settings opens from the menu, every page works on the CPU renderer, and changes show live in both directions
- [ ] The pet process maps no GPU or GL library with Settings open or closed (default run)
- [ ] A second open raises the window; quit with Settings open ends cleanly; settings tests ported and passing

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
