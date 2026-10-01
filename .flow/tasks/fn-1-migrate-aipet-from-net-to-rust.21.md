---
satisfies: [R14]
---
# fn-1-migrate-aipet-from-net-to-rust.21 Updates through the Velopack Rust SDK, and the uninstall hook

## Description
Windows updates in the Rust pet, following task 14's proof:
- Velopack's startup hook runs first, including the uninstall cleanup of registered hooks;
- the update loop;
- install on quit;
- the updates section in Settings.

**Size:** M
**Files:** `rust/crates/aipet/src/updates.rs`, `rust/crates/aipet-ui/src/settings/updates.rs` (the updates section; both stubs from task 15)
**Touches:** [rust/crates/aipet/src/updates.rs, rust/crates/aipet-ui/src/settings/updates.rs]

### Approach
- Task 15's groundwork calls `updates::startup()` first thing in `main.rs`, adds `velopack`, and has the General
  page show `settings/updates.rs`, so this task touches neither `main.rs`, `general.rs` nor a Cargo file.
- The first line of `main` is `VelopackApp::build()`, with the uninstall callback running the cleanup from task 10, as
  `Updates.App()` does (`src/AiPet.UI/Updates.cs:43-52`).
- On Windows only, `Updates.Start` (`:73-113`): install a pending update first, wait the first-check delay, then check
  every interval with `GithubSource(repo)` on the `win` channel.
- `CheckAsync`, `InstallOnQuit` and `Restart` (`:116-170`), reporting download progress and status.
- The Settings updates section (`src/AiPet.UI/SettingsWindow.axaml.cs:128-150`): the version, and "Check for updates" or
  "Restart to update".
- If task 14 chose a one-time reinstall, add the migration notice it specifies.

### Investigation targets
**Required:**
- `src/AiPet.UI/Updates.cs:1-171`, `src/AiPet.Core/UpdateStatus.cs`
- `rust/proofs/velopack.md` — task 14's results
**Optional:**
- https://docs.rs/velopack
**From task 14's proof (2026-09-29):** `vpk pack` checks that `VelopackApp.Run` comes first only in .NET exes, so the
Rust pet needs its own test that Velopack's `run()` runs before anything else in `main` ([rust/proofs/velopack.md](../../rust/proofs/velopack.md)).

**From task 15 (2026-10-01):** your files are `aipet/src/updates.rs` and `aipet-ui/src/settings/updates.rs`. main.rs calls
`startup()` first, then `start()` after the single instance (false means exit). Settings calls `check()`; `restart()`
returning true quits the pet; `install_on_quit()` runs at quit, but not after a restart. `Services { version,
update_status: fn() -> UpdateStatus }`. `velopack = "=1.2.158"` is a dependency of aipet (resolved offline from cargo's
cache; its Unix-only crates, wait-timeout and waitpid-any, aren't on this machine, so CI fetches them). The section's
view, with its button for anything but Off, is there.
## Acceptance
- [ ] On the Windows CI, a local-feed test downloads an update and applies it on quit. The status texts match the C#.
- [ ] Uninstalling a test install runs the hook cleanup (the proof workflow extended).
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
