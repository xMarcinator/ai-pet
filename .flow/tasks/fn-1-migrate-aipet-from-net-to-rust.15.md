---
satisfies: [R7, R12]
---
# fn-1-migrate-aipet-from-net-to-rust.15 Pet app wiring: real data from the core, the shared single-instance lock, demo as a dev feature

## Description
Make the spike a real pet. Start the core (hook server, sessions, watchers, Board), feed its snapshots to `PetUi`
instead of the demo script, and add the single-instance rules shared with the .NET pet. The binary is renamed `AiPet`.
It depends on task 13 so the desktop shell's Windows route is settled first.

**Size:** M
**Files:** `rust/crates/aipet/` (renamed from `aipet-spike`: `Cargo.toml`, `src/main.rs`, `src/core_thread.rs`, `src/single.rs`), `rust/crates/aipet-ui/{Cargo.toml,src/lib.rs,src/demo.rs}`, `rust/crates/aipet-wayland/src/shell.rs`, `rust/crates/aipet-desktop/src/shell.rs`, `rust/Cargo.toml`; groundwork for tasks 16–21: `rust/crates/aipet-ui/src/{platform.rs,settings/}` (split from `settings.rs`), `rust/crates/aipet-desktop/src/{lib.rs,platform/}`, `rust/crates/aipet-desktop/Cargo.toml`
**Touches:** [rust/crates/aipet/**, rust/crates/aipet-spike/**, rust/crates/aipet-ui/Cargo.toml, rust/crates/aipet-ui/src/lib.rs, rust/crates/aipet-ui/src/demo.rs, rust/crates/aipet-ui/src/platform.rs, rust/crates/aipet-ui/src/settings.rs, rust/crates/aipet-ui/src/settings/**, rust/crates/aipet-wayland/src/shell.rs, rust/crates/aipet-desktop/src/shell.rs, rust/crates/aipet-desktop/src/lib.rs, rust/crates/aipet-desktop/src/platform/**, rust/crates/aipet-desktop/Cargo.toml, rust/Cargo.toml, rust/Cargo.lock]

### Approach
- A core thread does what `MainWindow`'s constructor does (`src/AiPet.UI/MainWindow.axaml.cs:62-150`):
  - start the server, `AgentSessions`, `CodexWatcher` and the Jira/GitHub watchers;
  - refresh the Board every 250 ms and on `Changed`;
  - send snapshots and alerts to the shells.

  Each shell turns them into a `ui::Message`.
- `PetUi::tick` uses the Board's state and prop for the mood, and its cards for the bubbles
  (`rust/crates/aipet-ui/src/lib.rs:202-213` today calls `demo::scene`).
- The demo script becomes a cargo feature `demo`, off by default and never in release builds. It is for development,
  screenshots and tests, not a runtime option.
- Single instance, as the spec's single-instance contract (R7):
  - Windows: the `Local\AiPetApp` mutex with `CreateMutexW`, the same kernel object the .NET pet holds.
  - Linux: first the `flock` lock that task 6 added to the .NET pet: the same file, derived from the socket path by `aipet-ipc`'s
    endpoint rule, with the same flags, then the
    server's live check and the other-session socket probe (`src/AiPet.UI/Program.cs:18-37`).
  - Quit quietly when another pet holds either, .NET or Rust.
- Startup order: single-instance, then the core, then the shell. Task 21 puts Velopack first.
- **Groundwork for tasks 16–21** (the user's decision, 2026-09-30), so each of them works in files of its own and they
  can run side by side. Each piece is an interface with a minimal implementation, not the feature:
  - `aipet-ui/src/platform.rs`: the `Platform` trait (`open_url`, `open_folder`, `focus_agent`, and media: poll,
    previous, play-pause, next, focus), a no-op implementation, and a recording fake for tests. `PetUi` and Settings
    take it. Task 17 calls it; task 18 implements it. There is no `send_escape` (no Stop button).
  - `aipet-desktop/src/platform/{mod,linux,windows,mac}.rs`: no-op implementations, declared in
    `aipet-desktop/src/lib.rs` and handed to the UI in `main.rs`, so task 18 fills them without touching either.
  - `aipet-ui/src/settings/`: split `settings.rs` into `mod.rs` (the window, page navigation, the shared rows,
    sections and fields, and the actions Settings emits: reset position, open data folder, import defaults, check
    for updates, restart to update) and page stubs `general.rs`, `avatars.rs`, `jira.rs`, `github.rs` and
    `updates.rs`. The General page shows the updates section from `updates.rs`. Tasks 19, 20 and 21 fill their pages
    without touching `mod.rs`.
  - `aipet/src/`: `main.rs` already calls, in startup order, `updates::startup()` (first, for task 21), the placement
    hooks (load at start, save on a drop and on quit, reset) and the Settings action dispatch. `config.rs` holds the
    config in memory with a dirty flag behind a small API; its first version writes at once, and task 16 completes the
    write rules. `placement.rs` and `updates.rs` start as stubs. `PetUi` exposes what placement needs (the window's
    size and a finished-drag event), so task 16 doesn't touch `lib.rs`.
  - Crates: add `rfd` (task 19) to aipet-ui, `velopack` (task 21) to aipet, and the windows-sys features task 18 needs
    (`EnumWindows`, `SetForegroundWindow`, `AttachThreadInput`, `ShellExecuteW`, `WM_APPCOMMAND`) to aipet-desktop, so
    tasks 16–21 change no `Cargo.toml` and no `Cargo.lock`.

### Investigation targets
**Required:**
- `src/AiPet.UI/MainWindow.axaml.cs:62-150, 297-315` — wiring and Refresh
- `src/AiPet.UI/Program.cs` — single instance
- `rust/crates/aipet-ui/src/lib.rs:104-230`, `rust/crates/aipet-spike/src/main.rs`
**Optional:**
- `rust/crates/aipet-ui/src/demo.rs` — the scene shape to replace
**From task 13's proof (2026-09-29):** make GL the default GPU backend on Windows too, in the pet and the spike, the
way the spike already does on Linux (`WGPU_BACKEND=gl` unless the user set it). On Windows GL is transparent, shows the
pet in 0.9 s against 4.5–7.3 s for Vulkan, doesn't load the NVIDIA driver and never falls back to DX12, which is opaque
through iced 0.14 ([rust/proofs/windows.md](../../rust/proofs/windows.md)).
**From task 11 (2026-09-30):** build the watchers with `JiraWatcher::new(data_dir, secrets, Http::new(), sender)` and
the same for `GitHubWatcher`. `E: From<Event>` lets one channel of the app's own enum carry both; `data_dir` is
`aipet_ipc::paths::data_dir()`, and `secrets::platform()`'s Box goes in with `.into()`. Wire them as
MainWindow.axaml.cs:122-130 does: `github.set_jira_keys(...)` over a shared `Arc<JiraWatcher>`; on
`jira::Event::Changed`, call `github.jira_changed()`, then refresh; on `NewReviews(_)`, alert; then `restart()` both.
Dropping a watcher stops its loop. Also: aipet-ui's `tests::a_poke_and_a_hop_are_smooth` (`lib.rs`, in this task's
Touches) failed once under load in task 11's full-suite baseline (a 33.3 ms step where 16.7 ms was expected, at
`lib.rs:789`) and passed alone three times; make it independent of how busy the machine is.
**From task 9 (2026-09-30):** the hook server is `aipet_core::server::HookServer::new(Arc<AgentSessions>, changed)`,
then `start() -> io::Result<()>` and `stop()`; dropping it stops it too. `start` logs "hooks: listening on …" or
"hooks: can't listen on …: …" to aipet.log as the C# does, and also returns the error; the pet runs on either way.
`changed` runs on the hook's own handler thread before the hook gets its answer, so it must not block: post to the
core thread, never wait for the UI. `Options { endpoint, events_log, log }` keeps tests off the user's endpoint and data.
**From task 12 (2026-10-01):** the Board is `aipet_core::board::Board::new()`, then `refresh(&Sources, now)` every
250 ms and on Changed, with `now = aipet_ipc::protocol::unix_time(SystemTime::now())` (Board.Unix to the bit) and
`Sources::read(&hooks, &jira, &github, Some(&codex), media, music_on)`. Read it with `cards()`, `all()`,
`extra("chats"|"reviews"|"music")`, `state()`, `prop()` and `find(id)`; `dismiss(id, now)` hides a bubble until it does
something new. `Session.eff` is a `&'static str` that maps straight onto aipet-ui's `Bubble.state`. The Codex log
watcher is `aipet_core::codex_watcher::CodexWatcher::new()` (reads `$CODEX_HOME` on each poll), `start()` polls now
and then every 2 s, `stop()` or dropping it ends the loop, and `sessions()` never waits on a poll.
**From the conductor (2026-10-01), the Linux lock:** task 6 adds the C# side of the shared Linux lock and runs in
parallel with this task, so it is not in your base. Write the Rust side to the contract (R7 and task 6's description:
the socket path with `.lock` for `.sock`, a non-blocking exclusive `flock`, the file opened 0600 with `O_NOFOLLOW`,
held for the whole run), and test the contention against a lock your test takes exactly that way (on Windows: the
`Local\AiPetApp` mutex, made by the test as the C# makes it). The test against the C#'s own lock moves to task 23's
cross-runtime CI. Starting either pet, and the manual checks with real chats and with both apps, wait for the user:
list them in the handover, don't run them.
## Acceptance
- [ ] Unit tests map Board snapshots to bubbles and mood (states, props, alerts).
- [ ] With real Claude and Codex hooks on the developer's machine, the Rust pet shows real chats (manual check, noted in
      the task).
- [ ] A .NET pet and a Rust pet never run together: in either start order, and when started at the same moment (a test
      where the Rust lock contends on one file with a lock taken as task 6's C# takes it, including with different or
      unset `XDG_RUNTIME_DIR` values; on Windows, with the `Local\AiPetApp` mutex made as the C# makes it). The test
      against task 6's real C# lock is task 23's, and the manual check with both apps is listed for the user.
- [ ] `cargo run -p aipet --features demo` still runs the demo. A release build contains no demo code.
- [ ] Groundwork: the stubs build, and the app runs with them (a no-op platform, placeholder Settings pages, the
      default placement, no update check). The handover lists each extension point and the task that fills it. None of
      tasks 16–21 needs to edit `main.rs`, `aipet-desktop/src/lib.rs`, `settings/mod.rs`, `platform/mod.rs`, a
      `Cargo.toml` or `Cargo.lock`.
## Done summary
The Rust pet is now a real pet. Started with `cargo run -p aipet` (the binary is `AiPet`), it shows the bubbles and mood of the user's real Claude and Codex chats, Jira and GitHub, through the core's Board. It quits quietly when a .NET or Rust pet already runs. The demo's scripted day runs only with `--features demo`, and a release build has none of it. Tasks 16 to 21 each get files of their own to fill.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

What the pet does now:
- `aipet/src/main.rs` starts in this order: `updates::startup()` (Velopack's, a stub), the GPU and shell environment (`WGPU_BACKEND=gl` on Windows too, from task 13's proof), the single instance, `updates::start()`, the core, then the shell until the pet quits. The core stops before the instance is let go. Release builds on Windows are GUI programs (`windows_subsystem`), so no console window opens.
- `single.rs` follows the spec's R7 contract.
  - Windows: `CreateMutexW(NULL, TRUE, "Local\AiPetApp")`. `ERROR_ALREADY_EXISTS` or `ERROR_ACCESS_DENIED` (an elevated pet's mutex) quits quietly.
  - Unix: an exclusive non-blocking `flock` on the socket's path with `.lock` for `.sock`, or `<AIPET_PIPE>.lock` under the override. The file is opened 0600 with `O_NOFOLLOW`. Then the other-session probe runs: anything but `NoPet::Closed` quits.
  - Errors other than contention (a link where the file goes, an unwritable folder) stop the pet with a line on stderr and in aipet.log.
  - The names come through `Names`, so tests use unique ones. `the_mutex_is_the_csharps` reads `new Mutex(true, @"Local\AiPetApp", out created)` from Program.cs.
- `core_thread.rs` runs what MainWindow's constructor and Refresh do.
  - It starts `HookServer::with_options` over `AgentSessions`, `CodexWatcher`, and the Jira and GitHub watchers. GitHub reads Jira's keys through a `Weak`, and Jira's Changed calls `github.jira_changed()`.
  - It refreshes the Board every 250 ms and on Changed, and sends `News::Board(Arc<Board>)` only when the cards, the rest of the bubbles, the state or the prop changed. NewReviews from either watcher goes out as `News::Alert`.
  - A thread of its own polls the player every 1 s while Music is on.
  - `Options` takes the data folder, server options, Codex home, secrets, HTTP client and platform, so tests keep off the user's pet and data.
- Both shells drain the news on every frame and hand it to `PetUi` as `Message::Board` and `Message::Alert`.
- `aipet-ui/src/lib.rs` turns the Board into bubbles as SyncCards does.
  - Chats get their app's colour, and the app's name before the detail unless every chat is in the Claude app or Claude Code.
  - The last bubble of a stack gets "  ·  +N more". The reviews header counts jira and github cards plus `extra("reviews")`.
  - The mood and prop are `Board::state()` and `prop()`. The pet bops while the Board's music bubble is "music".
  - A dismissal goes to the core (`Host::dismiss`), and the bubble leaves with the next Board. The UI's own dismissal list is gone.
- The demo (`demo.rs`, under `cfg(any(test, feature = "demo"))`) plays its script through a Board of its own, as hook entries and a Jira issue, so it shows real labels and moods. `PetUi::demo(setup)` runs it. In the binary, `--features demo` skips the single instance and the core, and reads and writes nothing in the data folder.
- The renames make the Rust pet the real one.
  - Desktop shell: title and X11 class `AiPet`, Settings `AiPet · Settings`, default corner 24 px from the right (was 420).
  - Layer shell: namespace `aipet` (was `aipet-spike`), Settings `AiPet · Settings` at 800 × 640.

Extension points for tasks 16 to 21. None of them needs `main.rs`, `aipet-desktop/src/lib.rs`, `settings/mod.rs`, `platform/mod.rs` or a manifest, except rfd (below):

| Task | Where | What is there |
|---|---|---|
| 16 placement, config | `aipet/src/placement.rs`, `aipet/src/config.rs`, both `shell.rs` | `App` (main.rs) calls `placement::{load, save, reset, quit}` from `Host::{saved_place, save_place, reset_place, quit}`. `config::Store { load, get, change (dirty flag, writes at once), flush }`. `Place { left, top, window_height }` is config.json's position. `PetUi::window_size()`; `Effect::Dropped` is the finished drag. The desktop shell already calls `save_place` after a drop and after Reset position, and doesn't read `saved_place` at boot. The layer shell only logs a drop. |
| 17 bubble actions | `aipet-ui/src/{cards,view,lib}.rs` | `PetUi::platform()`, the Board in `PetUi.board` (`find`), and `Effect::OpenSettings` with `settings.page` set first. |
| 18 platforms | `aipet-desktop/src/platform/{windows,linux,mac}.rs` | `pub struct Windows/Linux/Mac` with `new()` and no-op `impl Platform`. `platform::new()` picks one. Windows features added: `Win32_System_Threading` (AttachThreadInput, OpenProcess, QueryFullProcessImageNameW), `Win32_System_SystemServices` (APPCOMMAND_*); EnumWindows, SetForegroundWindow, ShellExecuteW and WM_APPCOMMAND were already there. aipet-core is a dependency, for aipet.log. |
| 19 General, Avatars | `aipet-ui/src/settings/{general,avatars}.rs` | `general::import(pet)` gets `Action::ImportDefaults`. `tick(pet)` runs every frame, to pick up a dialog's file. `Services` has the watchers, `data_dir` and the platform. The mood picker is already demo-only. |
| 20 Jira, GitHub | `aipet-ui/src/settings/{jira,github}.rs` | Empty `Message` enums and page states, `Services { jira, github, secrets, http }`, `field(.., secret)`, `status` + `Tone`, and `PetUi::settings()` for tests. |
| 21 updates | `aipet/src/updates.rs`, `aipet-ui/src/settings/updates.rs` | main.rs calls `startup()` first, then `start()` after the single instance (false = exit). Settings calls `check()`; `restart()` returning true quits the pet; `install_on_quit()` runs at quit, skipped after a restart. `Services { version, update_status: fn() -> UpdateStatus }`. `velopack = "=1.2.158"` is a dependency (resolved offline). The section's view, with its button for anything but Off, is there. |

**rfd isn't added.** It isn't in cargo's cache, so downloading it waits for the user. It goes in `aipet-ui/Cargo.toml`. Then `general::import` opens the dialog on a thread, and `general::tick` picks the file up.

Acceptance:
- Board to bubbles and mood (aipet-ui `tests::`):
  - `board_chats_become_bubbles_with_their_apps`, `reviews_errors_music_and_what_the_caps_leave_out_become_bubbles`, `the_mood_and_the_prop_are_the_boards`, `an_alert_calls_for_attention_for_six_seconds`, `the_pet_bops_along_only_while_the_player_plays_and_music_is_on`, `hidden_bubbles_show_nothing_and_a_dismissal_goes_to_the_core`;
  - the core end to end: `core_thread::tests::a_hooks_chat_reaches_the_pet_and_a_dismissal_takes_its_bubble_away` (a real wire event on a test pipe), `new_reviews_become_alerts_...` and `the_player_is_asked_only_while_the_user_listens_along`.
- Never two pets:
  - Windows tests, run here: `a_mutex_the_csharp_made_keeps_this_pet_out_and_the_other_way_round` (the test makes the mutex with `CreateMutexEx(CREATE_MUTEX_INITIAL_OWNER, MAXIMUM_ALLOWED|SYNCHRONIZE|MUTEX_MODIFY_STATE)`, as .NET's Mutex does), `pets_started_at_the_same_moment_give_one` (20 rounds of 8 threads, half of them C#-style), `a_mutex_this_pet_may_not_open_is_another_pets` (empty DACL) and `a_pet_in_another_process_is_kept_out`.
  - The Unix tests are the same contention cases against a lock opened 0600 with `O_NOFOLLOW|O_CLOEXEC` and `flock`ed as task 6's C# will, plus a link refused, child processes with unset or different `XDG_RUNTIME_DIR` values kept out, and the lock path checked to be the same whatever `XDG_RUNTIME_DIR` says. That last test only reads where the lock would be, so it never touches the user's. They passed clippy with `-D warnings` for Linux and macOS through a scratch crate. They have not run: there is no Linux toolchain here.
- Demo: `cargo build -p aipet --features demo` and clippy `-D warnings` are clean. `cargo build --release -p aipet` contains none of the demo's strings ("Make the spike a real pet", "Allow cargo build?", "AIPET-42", "claude:demo" each 0 times), while the demo debug build has each.
- Groundwork: everything builds, and `cargo check --workspace --all-targets --locked` passes. Starting the app is a manual check (below): no GUI app was started here.
- Task 11's flaky test `a_poke_and_a_hop_are_smooth` now seeds the pet's whims (`XorShift64::new(15)`) and looks 2 calm frames after the hop starts. On its first frame a hop hasn't moved yet, which is what failed under load. It passed 10 runs out of 10, and every full-suite run.

Manual checks for the user (Git Bash at the repository root; never with PowerShell):
1. Real chats (AC2). Quit the .NET pet first, or the Rust pet quits at once. Run `(cd rust && cargo build -p aipet) && AIPET_DEBUG=1 rust/target/debug/AiPet.exe`, then work in a Claude Code chat and a Codex chat. Their bubbles, the mood and the laptop or lens should follow. A dismissed bubble stays away until the chat does something new, and Quit in the menu ends the pet.
2. Never two pets (AC3), Windows. Start the .NET pet, then `rust/target/debug/AiPet.exe; echo $?` gives 0, and no second pet shows. Then the reverse order: no .NET pet shows. Then at the same moment: `"$LOCALAPPDATA/AiPetApp/current/AiPet.exe" & rust/target/debug/AiPet.exe &` gives one pet.
3. Never two pets, Linux. The same three orders, once task 6's .NET build holds the lock. Until then the .NET pet's probe covers the two start orders, but not the same moment.
4. Demo (AC4): `cd rust && cargo run -p aipet --features demo`. The 40 s script plays, and Settings → General has Pet mood and Listen along. It may run next to a real pet.
5. Groundwork run (AC5): `cd rust && cargo run -p aipet` with no other pet.
   - The pet sits in the bottom-right corner, at the monitor's bottom (task 16 moves it to the work area).
   - The menu works. Settings shows General (switches, Position: Reset, Data folder: Open, About: "This copy doesn't update itself"), Avatars, and empty Jira and GitHub pages.
   - Open does nothing yet (no-op platform).
   - Toggling a switch rewrites config.json, and a start and quit without one leaves it untouched (check its time).
6. Linux shells: `AIPET_BACKEND=wayland cargo run -p aipet` and `AIPET_BACKEND=desktop ...`. The layer namespace is now `aipet`, so the spike's Hyprland rule for `aipet-spike` no longer matches.
7. Release console: start `rust/target/release/AiPet.exe` from Explorer. No console window should open.

Choices to review:
- **`Launch::hand_over`, a process-wide hand-off.** aipet-wayland's `lib.rs` (outside Touches) has `run()` with no arguments, so main.rs leaves the pet in the hand-off and the Wayland shell takes it. The desktop shell takes `run(launch)`.
- **Interned ids.** Bubble ids are `&'static str` (cards.rs, task 17's file), so lib.rs interns each Board id once for the pet's life, a few dozen bytes per chat, review or player.
- **A busy socket counts as another pet.** The C#'s probe counts only a pet that answers.
- **Lock path under `AIPET_PIPE`.** The lock is `<AIPET_PIPE>.lock` (appended), as the spec's contract says. Task 6's text says ".lock in place of .sock", which differs for an override ending in `.sock`, so task 23 should pin one rule. Only tests set it.
- **No music stack.** The music bubble goes in the chats stack, behind the chats: cards.rs has none.
- **Old order for thinking dots.** cards.rs adds thinking dots after "+N more".
- **GitHub dot colour.** A GitHub review's dot is the review blue, where the C# uses purple (view.rs).
- **Reset position on Wayland.** The layer shell's Reset position only calls `reset_place`.

baseline: green. Before any edit, at f3678f4: the Rust quick command passed 311 tests with clippy and fmt clean, and `dotnet build AiPet.slnx -p:UseAppHost=false` then `dotnet test AiPet.slnx --no-build` gave 160 passed, 42 skipped.

Verify at 81b8262:
- The Rust quick command passed 336 tests, with clippy and fmt clean, and `cargo clippy --workspace --all-targets -- -D warnings` is clean. Both gates have green receipts.
- The C# suite gave 160 passed, 42 skipped.
- Cross-target clippy with `-D warnings` is clean for aipet-wayland, aipet-desktop and aipet-ui on x86_64-unknown-linux-gnu, for aipet-desktop and aipet-ui on aarch64-apple-darwin, and for single.rs (scratch crate) on both. The recipe is in the run notes. `-p aipet` itself can't be checked offline for Linux: velopack's unix crates aren't cached.
- Mutation checks:
  - Ignoring `ERROR_ALREADY_EXISTS` fails 3 single tests, and treating `ERROR_ACCESS_DENIED` as an error fails the elevated one.
  - Dropping the core's dismiss or alert fails both core tests.
  - Dropping the app labels, the "+N more" or the Board's mood fails 5 aipet-ui tests.

Follow-ups (outside Touches): rust/SPIKE.md and rust/proofs/windows.md still say `aipet-spike` (run commands, class and namespace). Notes: NOTES_DIR/task-15-pet-wiring.md.

Review fixes:
- dc78a08: the single instance is taken right after Velopack's startup, before the GPU environment and the shell choice; the lock goes to the pet, which lets it go after the core stops.
- 2f33e33: bubble and card ids, and the CardPressed/Dismiss messages, are `Arc<str>`; the interner is gone (cards.rs and view.rs touched for the id type only, as the conductor allowed).
- a6b6b03: the Wayland shell's Linux-only redraw-scope test dismisses with an owned id.

rfd: added by the conductor after this task (the user agreed to the download), for task 19.

stage: impl-review - ran (codex: NEEDS_WORK x3 then SHIP; fixes dc78a08, 2f33e33, a6b6b03)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 40b7c245154b31f792310bcd597cca8dbcf1d9ba, 0c1887a685e012d9629d24452078bc7d6c9cb03b, 81b8262e01470a603f6e793b2d38c9a3cf9b7161, dc78a08, 2f33e33, a6b6b03
- Tests: baseline: green at f3678f4 (cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check: 311 passed, clippy and fmt clean; dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped), cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check: green at 81b8262 (336 passed, 0 failed; green receipt 81b8262e-unittest), cd rust && cargo clippy --workspace --all-targets -- -D warnings: clean at 81b8262, dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped at 81b8262 (green receipt 81b8262e-dotnet), cd rust && cargo check --workspace --all-targets --locked --offline: clean (Cargo.lock current), cargo clippy -p aipet --features demo --all-targets -- -D warnings, cargo clippy -p aipet-ui -- -D warnings (no demo, no test): clean, cross-target clippy -D warnings, -Zbuild-std with stand-in cc/ar/pkg-config: aipet-wayland, aipet-desktop, aipet-ui --all-targets on x86_64-unknown-linux-gnu; aipet-desktop, aipet-ui on aarch64-apple-darwin; single.rs through a scratch crate on both: clean. Not run: no Linux toolchain here; -p aipet can't be checked offline for Linux (velopack's unix crates aren't cached), cargo build --release -p aipet: the demo's strings (Make the spike a real pet, Allow cargo build?, AIPET-42, claude:demo) are absent from target/release/AiPet.exe; present in the --features demo debug build, aipet-ui a_poke_and_a_hop_are_smooth (reseeded, looks two calm frames after the hop starts): 10 of 10 runs passed, mutation checks: single.rs ignoring ERROR_ALREADY_EXISTS fails 3 tests, ERROR_ACCESS_DENIED as an error fails 1; core_thread without the dismiss or the alert fails 2; lib.rs without app labels, +N more and the Board's mood fails 5, not run here (GUI apps and the Unix-only tests): manual checks 1-7 in the summary; single.rs's unix tests run in CI's Linux job, integrated verify (Windows, work branch 5d9765a with task 15 and its review fixes merged; rfd added uncommitted in the same tree): cargo test --workspace --no-fail-fast -- --skip credential_manager_tokens_are_the_apps green, clippy --workspace --all-targets -D warnings and fmt --check clean; cargo check --workspace --all-targets --locked --offline green; dotnet build AiPet.slnx -p:UseAppHost=false, dotnet test AiPet.slnx --no-build: 160 passed, 51 skipped; AIPET_TEST_HOOK=<rust hook> dotnet test --filter PluginHooksTests|RegistrationTests|ResourceTests: 23 passed, 10 Unix-only skipped; AIPET_TEST_HOOK=<.NET hook dll> cargo test -p aipet-core --test server: 15 passed; with AIPET_GOLDEN the workspace green except aipet-hook's codex live replay, whose C# generator failed under load (the antivirus flake); rerun alone at 50b5ee2 it passed in 55 s (it also passed alone twice at c324cd4), only CI confirms: the Linux and macOS builds of aipet (velopack's Unix crates aren't in cargo's cache here, and a6b6b03 was checked by reading), and the Unix single-instance tests; manual checks 1-7 in the summary wait for the user (carried into tasks 22 and 27)
- PRs: