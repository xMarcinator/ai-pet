---
satisfies: [R12]
---
# fn-1-migrate-aipet-from-net-to-rust.19 Settings: General and Avatars pages, Import defaults

## Description
Bring the Settings window to parity for the General page (without its updates section) and the Avatars page. The
spike's page has switches and an avatar picker only.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/settings/{general,avatars}.rs` (stubs from task 15), `rust/crates/aipet-ui/tests/settings.rs`
**Touches:** [rust/crates/aipet-ui/src/settings/general.rs, rust/crates/aipet-ui/src/settings/avatars.rs, rust/crates/aipet-ui/tests/settings.rs]

### Approach
- Task 15's groundwork provides `settings/mod.rs` (the window, the shared rows and fields, and the actions),
  `rfd`, and the `Platform` trait. Reset position emits the action task 16 handles; Open data folder and Open avatars
  folder go through the platform. Keep the General page's slot for the updates section (`settings/updates.rs`, task
  21's). This task touches no `mod.rs` or Cargo file.
- General (`src/AiPet.UI/SettingsWindow.axaml.cs:96-125`):
  - the switches: chat bubbles, always on top, and "Listen along with <player>" when a player exists;
  - Reset position, and Open data folder.
  - Import defaults… (`:154-204`): a file dialog (`rfd`), `Preset` parsing, applying it to the Jira and GitHub settings,
    and forgetting a token when the preset moves that service to another site.
- Avatars (`:214-261`): every avatar as a still, Reload, and Open avatars folder (created if missing). Picking one
  applies it and its accent colour.
- The spike's QA mood picker is compiled only with the `demo` feature.

### Investigation targets
**Required:**
- `src/AiPet.UI/SettingsWindow.axaml.cs:1-272`, `src/AiPet.UI/SettingsWindow.axaml`
- `rust/crates/aipet-ui/src/settings.rs`
**Optional:**
- `src/AiPet.Core/Presets.cs` — `MovesJira`/`MovesGitHub`
**From task 15 (2026-10-01):** your files are `aipet-ui/src/settings/{general,avatars}.rs`. `general::import(pet)` gets
`Action::ImportDefaults`, and `tick(pet)` runs every frame, to pick up a dialog's file. `Services` has the watchers,
`data_dir` and the platform. The mood picker is already demo-only. `rfd` isn't a dependency yet: downloading it waits
for the user's go-ahead, and the conductor adds it to aipet-ui's Cargo.toml before this task starts. Then
`general::import` opens the dialog on a thread, and `general::tick` picks the file up.
## Acceptance
- [ ] Tests: importing a preset that moves the Jira or GitHub site forgets that token; one that doesn't keeps it; a bad
      file shows the C#'s message.
- [ ] Screenshots of both pages. The switches persist through `config.json`.
## Done summary
The Settings window's General and Avatars pages now match the C#. General has Team defaults with Import defaults…, and the Avatars page has Reload, Open folder, an accent dot on each tile and a "Wear <name>" tooltip. The switches, Reset position and Open data folder were already wired by tasks 15 and 16.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

What changed (commit 7d53991, on top of 2d6f460):
- `aipet-ui/src/settings/general.rs`
  - Import defaults… opens `rfd::FileDialog` ("Import defaults", filter "AiPet preset" *.json) on an `aipet-import` thread, under `catch_unwind`. The button is greyed while the picker is open.
  - `tick` picks the answer up, and `import_file` follows the C#'s `ImportAsync`: read the file, `Preset::parse`, apply to each watcher's config, forget the saved token when `moves_jira` / `moves_github` says the address changes, `save(.., "")`, then the C#'s status line under Team defaults in its tone (Ok, Bad or Muted).
  - `General.import_status` is public, for tests and the view.
- `aipet-ui/src/settings/avatars.rs`
  - `Message::{Reload, OpenFolder}`. The folder is `Avatar::custom_dir(services.data_dir)`.
  - Open folder makes the folder, then opens it through `Platform::open_folder`.
  - Reload re-reads the folder and remakes the stills. The worn avatar stays on.
  - Both buttons are greyed in the demo.
- `aipet-ui/tests/settings.rs` (new, 5 tests):
  - `a_preset_that_moves_jira_or_github_forgets_that_token`: Jira, GitHub and both moving, each with the C#'s line.
  - `a_preset_that_keeps_the_sites_keeps_the_tokens`: the same site in other letters, a search, organisations and switches, with comments and a trailing comma. A move with no token saved has nothing to forget.
  - `a_bad_file_shows_the_csharps_message_and_changes_nothing`: invalid JSON, not a preset, no version, version 2, a field of the wrong type, no Jira or GitHub settings, and an unreadable file. Settings and tokens stay unchanged.
  - `the_switches_and_the_avatar_persist_through_config_json`: through a host that writes config.json as the app's `config::Store::change` does, then a restart from `Prefs::from(&Config)`.
  - `reload_shows_new_avatars_and_open_folder_makes_the_folder_first`.
- Unit test `settings::general::tests::the_picker_runs_on_a_thread_and_tick_imports_what_it_chose` covers cancel, a chosen file and a panicking picker, and checks that one picker opens at a time. No test opens a real dialog.

Choices to review:
- `general::import_file` is `pub #[doc(hidden)]`. It is the seam the integration tests use.
- The C#'s "This desktop has no file picker…" line isn't reproduced. rfd returns None when no portal exists, the same as a cancel.
- With no watcher (only in the demo), an import says "Couldn't save the settings: nothing watches Jira/GitHub here."
- If the worn avatar's file is gone after Reload, it keeps a tile of its own, because the index must stay valid. The C# shows no tile for it.
- `PetUi` doesn't keep `Setup.avatars`, so the demo can't reload. Keeping it in lib.rs is a follow-up, outside these Touches.
- After an import, the Jira and GitHub pages refill on their own through task 20's `tick`, which compares against the watcher's config. The details are in NOTES_DIR/task-19-general-avatars.md.

Manual checks (Git Bash at the repository root, never PowerShell; quit any other pet first):
1. Screenshots of both pages (AC2): `(cd rust && cargo build -p aipet) && rust/target/debug/AiPet.exe`, then the menu → Settings… General, and the menu → Avatars…. Take a screenshot of each.
2. Switches persist: toggle Show chat bubbles, Always on top, and Listen along (shown when a player exists). Then `cat "$LOCALAPPDATA/AiPet/config.json"` (on Linux, `~/.local/share/AiPet/config.json`) shows `Pills`, `OnTop` and `Music` changed. Quit and start again, and the switches are as left.
3. Import defaults…: write `{"version":1,"jira":{"site":"other.atlassian.net"}}` to a file outside the data folder, press Import defaults… and pick it. The line reads "Imported Jira (site). It points Jira at other.atlassian.net, so the token you saved…" when a Jira token was saved, and the Jira page shows the new site. A file containing `not json` gives "Nothing was imported. The file isn't valid JSON (line 1)." Cancelling the picker leaves the line unchanged. The pet keeps animating while the picker is open. On Linux this needs the XDG portal.
4. Avatars: Open folder creates `<data>/avatars` and opens it. Copy `avatars/hood-green.json` into it and press Reload: "Hood (green)" appears with its dot and tooltip, and clicking it puts it on and recolours Settings.
5. Demo: `cd rust && cargo run -p aipet --features demo`. Avatars' Reload and Open folder are greyed.

baseline: green (Rust quick command, 371 passed at 2d6f460, --offline). Verify at 7d53991: suite_rc=0, 377 passed, clippy and fmt clean. Green receipt: 7d539917-unittest. Also `-D warnings` clippy is clean for aipet-ui on Windows, with the demo feature, and cross-target for Linux and macOS. `dotnet test` was not run: no C# or hook path was touched.

Review fix (b4467a4): Reload re-applies a changed worn avatar, a deleted worn avatar keeps being worn without a tile, Open folder reports a folder it can't create, presets read with UTF-8/16/32 byte order marks.

stage: impl-review - ran (codex: NEEDS_WORK then SHIP)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 7d539917f85a143996eefe73eac0d48dcfdb7563, b4467a4
- Tests: baseline: green (cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check, run with --offline at 2d6f460e4fae84e5b8562822d1ae60984aac0b4d: 371 passed), cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check (--offline, at 7d53991: suite_rc=0, 377 passed, green receipt 7d539917-unittest), cargo clippy -p aipet-ui --all-targets -- -D warnings (Windows): clean, cargo clippy -p aipet-ui -p aipet --features aipet-ui/demo,aipet/demo --all-targets -- -D warnings: clean, cross clippy -p aipet-ui --all-targets -Zbuild-std --target x86_64-unknown-linux-gnu -- -D warnings: clean, cross clippy -p aipet-ui --all-targets -Zbuild-std --target aarch64-apple-darwin -- -D warnings: clean (future-incompat note for block v0.1.6 only), mutation check: without forget_token / create_dir_all, a_preset_that_moves_jira_or_github_forgets_that_token and reload_shows_new_avatars_and_open_folder_makes_the_folder_first fail, dotnet test AiPet.slnx: not run (no C# or hook path touched; the task changes aipet-ui only), integrated verify (Windows, work branch 4ff23c3 with tasks 19, 20 and 21 and their review fixes merged): cargo test --workspace --no-fail-fast -- --skip credential_manager_tokens_are_the_apps green, clippy -D warnings and fmt clean; at 436d693: dotnet build and test 161 passed, 53 skipped, C# hook tests on the Rust hook 23 passed, Rust server with the .NET hook green; the live doctor replay, which failed in the full gate under load (the antivirus flake), passed alone at 4ff23c3
- PRs: