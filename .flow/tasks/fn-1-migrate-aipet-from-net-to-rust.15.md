---
satisfies: [R7, R12]
---
# fn-1-migrate-aipet-from-net-to-rust.15 Pet app wiring: real data from the core, the shared single-instance lock, demo as a dev feature

## Description
Make the spike a real pet. Start the core (hook server, sessions, watchers, Board), feed its snapshots to `PetUi`
instead of the demo script, and add the single-instance rules shared with the .NET pet. The binary is renamed `AiPet`.
It depends on task 13 so the desktop shell's Windows route is settled first.

**Size:** M
**Files:** `rust/crates/aipet/` (renamed from `aipet-spike`: `Cargo.toml`, `src/main.rs`, `src/core_thread.rs`, `src/single.rs`), `rust/crates/aipet-ui/{Cargo.toml,src/lib.rs,src/demo.rs}`, `rust/crates/aipet-wayland/src/shell.rs`, `rust/crates/aipet-desktop/src/shell.rs`, `rust/Cargo.toml`
**Touches:** [rust/crates/aipet/**, rust/crates/aipet-spike/**, rust/crates/aipet-ui/Cargo.toml, rust/crates/aipet-ui/src/lib.rs, rust/crates/aipet-ui/src/demo.rs, rust/crates/aipet-wayland/src/shell.rs, rust/crates/aipet-desktop/src/shell.rs, rust/Cargo.toml, rust/Cargo.lock]

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
## Acceptance
- [ ] Unit tests map Board snapshots to bubbles and mood (states, props, alerts).
- [ ] With real Claude and Codex hooks on the developer's machine, the Rust pet shows real chats (manual check, noted in
      the task).
- [ ] A .NET pet and a Rust pet never run together: in either start order, and when started at the same moment (a test
      where the Rust lock and task 6's C# lock contend on one file, including with different or unset
      `XDG_RUNTIME_DIR` values, plus a manual check with both apps).
- [ ] `cargo run -p aipet --features demo` still runs the demo. A release build contains no demo code.
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
