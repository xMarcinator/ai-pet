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
- [ ] The update decisions (check, download, restart to update, install on quit, a pending update at start, offline)
      are tested against a fake update manager, with no network, and the status texts match the C#.
- [ ] Velopack's real startup with `--veloapp-uninstall` runs the hook cleanup, tested in a child process against
      temporary Claude, Codex and data folders.
- [ ] The CI-level checks moved to task 23 (the conductor, 2026-10-02: they need workflow files this task doesn't
      own): a local-feed update on the Windows CI, and the extended proof workflow's uninstall.
## Done summary
The Rust pet now updates itself on Windows through Velopack's Rust SDK, as `Updates.cs` does. Velopack's startup comes first in `main`, and its uninstall callback runs the hook cleanup. The installed copy installs a pending update at start, checks GitHub's `win` channel 3 minutes after start and then every 6 hours, shows download progress, and offers Restart to update. A ready update installs when the pet quits. Settings shows the same status texts, button and greying as the C#.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

What changed (commit d4e2f60):
- `rust/crates/aipet/src/updates.rs`:
  - `startup()` reads `VELOPACK_RESTART` before `run()` clears it. It then runs `VelopackApp` with `set_auto_apply_on_startup(false)`, and on Windows `on_before_uninstall_fast_callback` runs `aipet_core::cleanup::run(<exe dir>\aipet-hook.exe)`.
  - A copy Velopack didn't install gets a locator that finds no install, as the C#'s `NotInstalled` does.
  - `start`, `check`, `restart`, `install_on_quit` and `status` port `Start`, `CheckAsync`, `Restart`, `InstallOnQuit` and `Status`. Velopack's `UpdateManager` sits behind a private `Manager` trait.
- `rust/crates/aipet-ui/src/settings/updates.rs`: the section's tone, button and greying are `Shown::of(&UpdateStatus)`, a pure function with a table test.

Tests (all with fakes, no network, no real install):
- aipet `updates::tests`:
  - `an_update_downloaded_earlier_installs_first_and_the_pet_exits`
  - `the_start_after_an_update_leaves_a_pending_one_ready`
  - `a_pending_update_that_cant_start_leaves_the_pet_running`
  - `the_checks_come_a_while_after_the_start_and_then_every_few_hours`
  - `a_check_downloads_a_newer_release_and_says_how_far` (the C#'s exact texts)
  - `a_failed_check_or_download_is_the_status_text` (R14: offline or rate limited gives the status text)
  - `one_check_at_a_time`
  - `quitting_and_restarting_install_the_ready_update`
  - `a_failed_apply_keeps_the_old_version_running` (R14: after a failed apply the old version keeps running)
  - `velopack_runs_before_anything_else_in_main` (task 14's ask)
  - `a_copy_velopack_didnt_install_starts_as_usual`
  - `the_hook_is_next_to_the_app`
  - `the_uninstaller_runs_the_hook_cleanup`: runs the real Velopack startup with `--veloapp-uninstall` in a child test process, with Claude, Codex and the data folder all in a temp folder, and `whoami.exe` standing in for the hook.
- aipet-ui: `the_section_shows_each_status_as_the_csharp_does`.
- Mutation checks:
  - Dropping the uninstall callback fails the uninstall test.
  - A statement before `updates::startup()` in main.rs fails the order test.

Gates:
- baseline: green at 2d6f460 (371 Rust tests; C# 161 passed, 53 skipped).
- Verify at d4e2f60:
  - The Rust quick command (`--offline`): 385 passed, 3 ignored, with clippy and fmt clean. Green receipt d4e2f600-unittest.
  - C#: 161 passed, 53 skipped.
  - Cross-target clippy with `-D warnings` (`-p aipet -p aipet-ui`, real velopack crates) is clean for Linux and macOS, and so is the demo build.

Not done, outside Touches (the acceptance names them):
- AC1's local-feed update test on the Windows CI and AC2's extended proof workflow need `.github/workflows/velopack-proof.yml` and `rust/notes/prototypes/velopack-proof/`. Neither file is in this task's Touches.
- Suggested follow-up for task 23 or a task that owns the workflow:
  - Pack the real Rust pet (with `current\aipet-hook.exe`) as 0.1.99 next to a .NET 0.1.98 in a local feed.
  - Have the job drive the pet's own update path rather than the .NET driver. `Manager` is private, so a test feed needs a small hook here, for example an `AIPET_TEST_UPDATE_FEED` that swaps `GithubSource` for `FileSource`. That was not added (YAGNI, and it would be new runtime surface).
  - Seed a temp `settings.json` that names the installed hook before the uninstall step, and check that it comes out clean.

Manual checks (Git Bash, never PowerShell; in Windows Sandbox or a throwaway user, never the user's real `%LOCALAPPDATA%\AiPetApp`):
- Task 27 (Windows hands-on):
  1. Run `cd rust && cargo build -p aipet && AIPET_OPEN_SETTINGS=1 target/debug/AiPet.exe`. General → updates should read "This copy doesn't update itself. To update, install AiPet again." with no button, and no `%LOCALAPPDATA%\velopack` log should appear.
  2. Run `target/debug/AiPet.exe --veloapp-uninstall 0.3.0; echo $?`. It prints 0 and the pet doesn't start. The hook cleanup finds no `aipet-hook.exe` next to a dev build, so nothing changes.
- Task 23 (the release), on a sandbox with milestone 1 installed from its Setup.exe:
  1. Publish the Rust release. Start the installed pet. After about 3 minutes, Settings shows "Checking for updates…", then "Downloading AiPet <v>… N%", then "AiPet <v> is ready. It's installed when you quit the pet, or restart now."
  2. Quit from the menu. Then `cat "$LOCALAPPDATA/AiPetApp/current/sq.version"` shows the new version, and `ls "$LOCALAPPDATA/AiPetApp/current/aipet-hook.exe"` exists. The data folder, tokens and hook registrations are unchanged.
  3. Repeat with "Restart to update". The updater's window shows and the pet starts again.
  4. Download an update, then end the pet with `taskkill //IM AiPet.exe //F`. Start it again: it exits, installs, and restarts once.
  5. Offline (network adapter off), use Check for updates. The status shows "Couldn't check for updates: …".
  6. Uninstall. Register first with `"$LOCALAPPDATA/AiPetApp/current/aipet-hook.exe" --install claude` (sandbox only), then run Settings > Apps > AiPet > Uninstall. Afterwards `grep -c aipet-hook ~/.claude/settings.json` shows 0, and `aipet.log` has an "uninstall: aipet-hook --uninstall claude (exit 0)" line.

Notes: NOTES_DIR/task-21-updates.md.

Review fix (446003f): each Velopack request runs on its own thread; a check gives up after 5 minutes without an answer and a download after 10 without progress, ending in Failed.

stage: impl-review - ran (codex: NEEDS_WORK then SHIP)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: d4e2f600dd8f792c5f6b7f3c75ce2e1f480c0b3f, 446003f
- Tests: cd rust && cargo test --workspace --offline && cargo clippy --workspace --all-targets --offline && cargo fmt --all -- --check (385 passed, 0 failed, 3 ignored; green receipt d4e2f600-unittest), dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build (161 passed, 53 skipped), cargo clippy --offline --locked -p aipet -p aipet-ui --all-targets -Zbuild-std --target x86_64-unknown-linux-gnu -- -D warnings (clean), same for aarch64-apple-darwin (clean), cargo clippy --offline -p aipet --features demo --all-targets -- -D warnings (clean), mutation: uninstall callback dropped -> the_uninstaller_runs_the_hook_cleanup fails; a statement before updates::startup() in main -> velopack_runs_before_anything_else_in_main fails, baseline: green (371 Rust tests, C# 161 passed / 53 skipped at 2d6f460), integrated verify (Windows, work branch 4ff23c3 with tasks 19, 20 and 21 and their review fixes merged): cargo test --workspace --no-fail-fast -- --skip credential_manager_tokens_are_the_apps green, clippy -D warnings and fmt clean; at 436d693: dotnet build and test 161 passed, 53 skipped, C# hook tests on the Rust hook 23 passed, Rust server with the .NET hook green; the live doctor replay, which failed in the full gate under load (the antivirus flake), passed alone at 4ff23c3
- PRs: