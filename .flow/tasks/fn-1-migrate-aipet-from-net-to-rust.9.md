---
satisfies: [R6, R7]
---
# fn-1-migrate-aipet-from-net-to-rust.9 Rust hook server for Unix and Windows, with cross-runtime tests

## Description
Port `HookServer`: the pet's listener with the C#'s trust boundary. It runs on threads, with no async runtime. Tests
run the Rust hook, and the C# hook, against it.

**Size:** M
**Files:** `rust/crates/aipet-core/src/server/{mod,unix,windows,answer,record}.rs`, `rust/crates/aipet-core/tests/server.rs`, `.github/workflows/cross-runtime.yml` (the reverse direction)
**Touches:** [rust/crates/aipet-core/src/server/**, rust/crates/aipet-core/tests/server.rs, .github/workflows/cross-runtime.yml]

### Approach
- Unix (`src/AiPet.Core/HookServer.cs:54-90, 354-400`):
  - `RefuseLiveServer` (connect first), then remove a stale socket file, `bind`, `chmod 0600`, and only then `listen`.
    Use `socket2` or `libc` to separate bind from listen; `std`'s `UnixListener::bind` also listens.
  - Accept on 8 threads.
  - Peer credentials: `SO_PEERCRED` through `libc` on Linux, `getpeereid` elsewhere. `std`'s `peer_cred` wasn't stable
    on Rust 1.89 (the workspace is on 1.98 now: check it before choosing).
- Windows (`:330-351`):
  - `CreateNamedPipeW` with a security descriptor (owner = the current user's SID, network denied),
    `FILE_FLAG_FIRST_PIPE_INSTANCE` on the first instance, and 8 blocking `ConnectNamedPipe` listeners.
  - Cut silent clients off with `CancelIoEx`.
- Shared:
  - A thread per connection, capped at 64. `Answer` (`:266-299`): version check, then ping or event, else refuse.
  - `Record`: timestamp format, the 30-line ring, `hook-events.log` halved at 64 KB (`:305-325`).
  - `Stop` wakes the listeners by connecting to them (`:92-116`).
- Tests: the Rust hook ↔ the Rust server end to end; floods; oversized and too-deep requests; wrong versions; silent
  clients; the socket mode; a stale socket; refusing a live server.
- Cross-runtime: add a job to `cross-runtime.yml` that runs the C# hook (`dotnet aipet-hook.dll`) against the Rust
  server. Task 2's hook is used for the Rust ↔ Rust tests.

### Investigation targets
**Required:**
- `src/AiPet.Core/HookServer.cs:1-406`
- `tests/AiPet.Tests/HookServerTests.cs`, `HookServerUnixTests.cs`, `StarvedPoolTests.cs` — the cases to port
- `docs/ARCHITECTURE.md` §6.5, §10 (threads)
**Optional:**
- `rust/notes/native.md` — the Windows security APIs from the spike

### Key context
- Don't use `PipeOptions.CurrentUserOnly`-style checks that compare the token owner. They break between elevated and
  normal processes of the same user (dotnet/runtime#123903).
**From task 7's review (2026-09-29):** `AgentSessions::apply` takes an `Envelope`: the parsed fields plus the raw
request line, which pairing needs to key a whole tool input as System.Text.Json writes it. The server calls
`Envelope::parse(line)` and passes the result to `apply`, not a parsed `Map`.
**From task 3 (2026-09-29):** `PluginHooksTests` and `RegistrationTests` now run the hook through
`TestEnv.HookStartInfo`, so `AIPET_TEST_HOOK` reaches them. Add `PluginHooksTests` to `cross-runtime.yml`'s class list:
it passes against the Rust hook on Windows, and its Unix-only case (`check-plugin.sh --hook`) has never run against
it. Don't add `RegistrationTests` yet: its Unix-only doctor cases run `--doctor codex` through the hook, which the Rust
hook doesn't port until task 5, so they would fail there. Task 5 adds it.
**From task 8 (2026-09-29):** `apply` can return `Applied { outcome: "error:FormatException", log: "event ->
error:FormatException" }`, as the C# does when a Codex turn id passes .NET's Guid parser but not
`Convert.ToInt64(…, 16)`. Do what `HookServer.Answer` does with any `error:` outcome: log the line, reply
`{"ok":false,"error":<outcome>}`, and don't fire Changed. Server tests should build their store with
`AgentSessions::with_codex_home(dir)`, so they never read the user's `~/.codex`.

## Acceptance
- [ ] Rust tests cover every case of `HookServerTests`, `HookServerUnixTests` and `StarvedPoolTests`, on Linux, and on
      Windows where they apply.
- [ ] The C# hook works against the Rust server in CI.
- [ ] The socket is 0600 before `listen`, and a peer with another uid is refused (Linux test using a second uid where
      CI allows, else a unit test on the credential check).
## Done summary
The Rust pet now has its own hook server: `aipet_core::server::HookServer` listens on the endpoint `aipet-ipc` names,
with the C#'s trust boundary, and answers hooks on threads of its own. The Rust hook and the .NET hook both pass
its tests on Windows.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

What it does (`rust/crates/aipet-core/src/server/`):
- `mod.rs` holds `HookServer::new(sessions, changed)`, `start() -> io::Result<()>`, `stop()` and Drop, which stops.
  It has the 64-connection cap with one log line per 10 s, and a thread per connection. `changed` runs on the
  handler thread before the answer, and a panic in it is logged. `Options { endpoint, events_log, log }` lets tests
  keep off the user's pet, data and aipet.log.
- `answer.rs` reads the request as `JsonNode.Parse` does. The JSON must be valid and at most 128 deep, and the
  request's own members must all differ. Then comes the version check, then ping, event or "unknown request".
  `error:` outcomes are logged and refused without Changed. The replies carry System.Text.Json's default escapes.
- `record.rs` writes the `yyyy-MM-dd HH:mm:ss.fff` local time, keeps the 30-line ring, and appends hook-events.log
  with the system's newline. Past 64 KB it keeps the last half, cut by UTF-16 units. When the C#'s UTF-8 writer
  would throw on a half that starts mid-pair, the file is left as it was.
- `unix.rs` refuses to start when a live pet answers at the path, and replaces what a crash left. The socket is 0600
  before `listen` (socket2 for bind, then chmod, then listen). 8 threads accept, and `SO_PEERCRED` (getpeereid
  elsewhere) must give this euid. std's `peer_cred` is still unstable on 1.98.1. Each connection has 2 s for its
  request and reply, through the socket's timeouts. Stop makes 8 wake connections, shuts the socket down and removes
  its file.
- `windows.rs` gives the pipe the user's SID as owner and the C#'s ACL: `O:<me>D:(D;;0x1f019f;;;NU)(A;;0x1f019f;;;<me>)`,
  the SDDL .NET's PipeSecurity produced for the C#. The first instance is made with FILE_FLAG_FIRST_PIPE_INSTANCE,
  and 8 threads block in `ConnectNamedPipe`. Each listener makes the next instance before it hands off. A cut-off
  thread, which runs only while connections do, calls `CancelIoEx` 2 s after a connection came and every 2 s after.
  Stop opens up to 16 wake connections.

Tests (`rust/crates/aipet-core/tests/server.rs`) run real hooks: the Rust one next to the test binary (a staleness
guard fails when its sources are newer), or `AIPET_TEST_HOOK` (a .dll runs through `dotnet`). Each C# case maps to
one Rust test:
- EndToEndTests: `codex_scrambled_hooks_end_at_the_latest_turn`, `claude_late_envelopes_over_the_socket_are_stale`
  (also counts Changed), `claude_hook_forwards_the_event_and_its_environment_silently`,
  `a_hook_with_garbage_on_stdin_exits_0_silently`.
- HookServerTests: `silent_clients_are_cut_off_while_the_pet_keeps_serving`,
  `stop_returns_promptly_with_clients_connected`, `garbage_is_dropped_without_a_reply` (its 5 lines),
  `too_deep_is_dropped_without_a_reply`, `json_it_cant_serve_is_refused`,
  `an_oversized_request_is_dropped_without_a_reply`.
- HookServerUnixTests: `unix::the_socket_is_the_users_only_and_goes_with_stop`, `unix::what_a_crash_left_is_replaced`
  (its 3 leftovers), `a_live_pet_is_not_replaced` (on Windows too, where the first instance is taken), and
  `unix::a_flood_of_silent_connections_is_capped_and_the_pet_still_answers` (Linux, threads counted in /proc).
- StarvedPoolTests: the Rust pet has no thread pool to starve, so `listeners_answer_hooks_while_every_cpu_is_busy`
  runs the 20 hooks while 2 threads per CPU spin, and
  `silent_clients_are_cut_off_and_stop_returns_while_every_cpu_is_busy` runs on both systems.
- NoPetTests has no server in it. aipet-hook's `with_no_pet_it_leaves_no_trace` and aipet-ipc's
  `a_missing_or_stale_socket_is_closed_at_once` cover it, and the C# class runs against the Rust hook in
  cross-runtime.yml.
- Beyond the C#'s cases:
  - `the_answers_are_the_csharps` checks 37 request lines against the C# HookServer's replies, byte for byte. A
    scratch .NET 10 probe produced 36 of them, covering versions, types, a member twice, BOMs, depths 126 to 129
    and the ping's escaped lines. The 37th, a request with spaces around its members, came in 165221a and wasn't
    probed.
  - `an_event_the_csharp_throws_on_is_refused` covers `error:FormatException` with no Changed.
  - `unix::a_peer_running_as_another_user_is_refused` runs this test binary as root through `sudo -n`. Root gets
    past the 0600 mode, so the peer check alone refuses it. The test skips without passwordless sudo.
- Unit tests:
  - `the_socket_is_the_users_before_it_listens` checks 0600 and a refused connect while bound, before `listen`.
  - `only_a_peer_known_to_be_this_user_is_let_in` checks a socketpair (let in) and /dev/null (refused).
  - `the_pipe_is_the_users_alone` reads the pipe's SDDL back, and a second first instance fails with error 5.
  - STJ escapes, hook-events.log halving on the C#'s own outputs (lengths and edge bytes), and the calendar.

cross-runtime.yml:
- The new `csharp-hook` job (Windows and Linux) builds `src/AiPet.Hook` and runs the server tests with
  `AIPET_TEST_HOOK` set to its dll. Each hook-running test must print `ok`. Its Linux leg also runs the root-peer
  test and fails if that test skipped.
- `PluginHooksTests` joins the C# classes run against the Rust hook, as task 3 asked. `RegistrationTests` waits for
  task 5. Locally on Windows it gives 5 passed and 1 skipped (Unix only).

Acceptance:
- Every case of the three C# files is ported. On Windows all 15 integration tests pass, 4 repeats under load too.
  The Linux-only tests (the flood, the root peer) and the Unix unit tests pass clippy with `-D warnings` for Linux
  and macOS, through a build-std scratch crate. They have not run: this machine has no Linux toolchain (WSL Ubuntu
  has no rustc and no crt1.o). CI runs them.
- The C# hook works against the Rust server locally: the 15 tests pass with `AIPET_TEST_HOOK=` the Debug
  aipet-hook.dll. In CI this waits on a push, and cross-runtime.yml has never run.
- 0600 before listen is `the_socket_is_the_users_before_it_listens`. Another uid refused is the root-peer test (CI,
  sudo) plus the unit test on the credential check. None of these ran here (Windows).

Where parity bends:
- Windows adds PIPE_REJECT_REMOTE_CLIENTS. .NET couldn't set it, which is why the C# denies NU in the ACL. The ACL is
  kept byte for byte.
- Windows cut-offs run on the server's own thread, so they happen while the pet is busy. The C#'s Timer waits for
  the pool there.
- Unix gives each connection one 2 s deadline for reading and replying. A client dripping bytes is cut at 2 s. The
  C#'s per-read socket timeout lets it live on while the pool is starved.
- Windows Stop wakes only a server that listened. The C# also connects to another pet's pipe from a server whose
  Start failed.
- `start()` returns its error as well as logging the C#'s lines. A failed event logs "hooks: an event failed:
  FormatException", while the C# logs the whole exception text.
- Halving hook-events.log strips a UTF-8 BOM, as `File.ReadAllText` does, but doesn't detect UTF-16 or UTF-32 BOMs.
- Inherited from `Envelope::parse` (sessions, outside Touches). The Rust server gives no answer to events the C#
  applies: a number past a double's range, a lone-surrogate escape, nesting exactly 128. A payload with a member
  twice gets `error:ArgumentException` from the C#, while the port applies it. No hook sends any of these. The run
  notes have the details.

Follow-ups:
- sessions: a RawValue-backed Envelope, and `error:ArgumentException` for a member twice in objects Apply reads.
- A panic inside `apply` (a bug) drops that connection unanswered and unlogged. The C# logs any exception and answers
  `error:<Type>`.
- Push, so ci.yml runs the Linux tests and cross-runtime.yml runs both jobs.

Review fixes: bb4c7d3 makes a Windows start serve with the listeners it got, as on Unix (fewer than 8 is still a start, logged; none returns Err and leaves nothing listening); 3d93a05 times the busy-CPU hooks by their own bounds instead of a 15 s batch limit.

stage: impl-review - ran (codex: NEEDS_WORK then SHIP; fixes bb4c7d3, 3d93a05)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 5a68af1e59898a0791758a5a8e94b1d0e717b7d9, 165221a4a80e43d9c522237832475f81cb516151, bb4c7d3, 3d93a05
- Tests: baseline: green at c66df24 (Windows, rust 1.98.1): cd rust && cargo test --workspace rc 0; cargo clippy --workspace --all-targets -- -D warnings rc 0; cargo fmt --all -- --check rc 0; dotnet build AiPet.slnx -p:UseAppHost=false then dotnet test AiPet.slnx -p:UseAppHost=false --no-build: 160 passed, 42 skipped, 0 failed, cd rust && cargo test --workspace (Windows, at 5a68af1): rc 0; aipet-core lib 27 passed (7 new server unit tests), tests/server.rs 15 passed, every other suite as at baseline, cd rust && cargo clippy --workspace --all-targets -- -D warnings (Windows, at 5a68af1): clean, cd rust && cargo fmt --all -- --check (at 5a68af1 and 165221a): clean, dotnet test AiPet.slnx -p:UseAppHost=false --no-build --nologo (at 5a68af1): 160 passed, 42 skipped, 0 failed, cd rust && cargo test -p aipet-core --test server (Windows, at 165221a): 15 passed, and 4 more runs under load, 15 passed each, AIPET_TEST_HOOK=src/AiPet.Hook/bin/Debug/net10.0/aipet-hook.dll cargo test -p aipet-core --test server (Windows, the C# hook against the Rust server, at 5a68af1): 15 passed; a missing AIPET_TEST_HOOK path fails the hook tests (checked), AIPET_TEST_HOOK=rust/target/debug/aipet-hook.exe dotnet test tests/AiPet.Tests --no-build --filter PluginHooksTests (Windows): 5 passed, 1 skipped (Unix only), Linux and macOS: RUSTC_BOOTSTRAP=1 cargo clippy --offline --all-targets -Zbuild-std --target x86_64-unknown-linux-gnu and --target aarch64-apple-darwin -- -D warnings, on a scratch crate over aipet-core's server/, sessions/, log.rs and tests/server.rs (at 165221a): clean for both; not run: no Linux toolchain here (WSL Ubuntu has no rustc and no crt1.o), so the Unix tests run first in CI, C# probe (scratch, .NET 10, AiPet.Core's HookServer on a test pipe): the replies to 36 request lines, the STJ escapes, the OnlyMe SDDL and 9 hook-events.log halving cases, encoded in the_answers_are_the_csharps, strings_are_written_as_system_text_json_writes_them, the_pipe_is_the_users_alone and a_file_keeps_its_last_half_as_the_csharp_cuts_it, flowctl gate classify --base c66df24: FULL (cross-runtime.yml), gate receipts written in the workspace only (.flow/tmp/green-receipts, 5a68af1 unittest and dotnet), cross-runtime.yml has not run in CI (nothing pushed), first integrated verify (3486e04, three Codex reviews loading the machine): listeners_answer_hooks_while_every_cpu_is_busy failed with the .NET hook (the batch over 15 s; on an isolated rerun, one hook's pipe cut off by the 2 s silent-client cut-off), and aipet-hook's a_mode_without_an_agent_is_a_usage_error missed its 2 s bound; both passed alone; 3d93a05 then dropped the batch bound, and the 2 s bound is recorded for task 5, integrated verify (Windows, work branch 0a5517b with tasks 9 and 11 and their review fixes merged): cargo test --workspace -- --skip credential_manager_tokens_are_the_apps green, clippy --workspace --all-targets -D warnings and fmt --check clean; dotnet build AiPet.slnx -p:UseAppHost=false, dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped; AIPET_TEST_HOOK=<Debug aipet-hook.dll> cargo test -p aipet-core --test server: 15 passed (the .NET hook against the Rust server); AIPET_TEST_HOOK=<rust hook> dotnet test --filter PluginHooksTests|RegistrationTests: 14 passed, 10 Unix-only skipped
- PRs: