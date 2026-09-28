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
**Files:** `rust/crates/aipet/src/{main,updates}.rs`, `rust/crates/aipet-ui/src/settings/general.rs` (updates section), `rust/crates/aipet/Cargo.toml`
**Touches:** [rust/crates/aipet/src/main.rs, rust/crates/aipet/src/updates.rs, rust/crates/aipet/Cargo.toml, rust/crates/aipet-ui/src/settings/general.rs, rust/Cargo.lock]

### Approach
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
## Acceptance
- [ ] On the Windows CI, a local-feed test downloads an update and applies it on quit. The status texts match the C#.
- [ ] Uninstalling a test install runs the hook cleanup (the proof workflow extended).
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
