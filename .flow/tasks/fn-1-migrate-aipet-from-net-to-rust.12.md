---
satisfies: [R9]
---
# fn-1-migrate-aipet-from-net-to-rust.12 Codex log watcher and the Board

## Description
Port `CodexWatcher` and `Board`, which merges hooks, Codex logs, Jira, GitHub and media into bubbles. Both are checked
against a new golden mode for the Board.

**Size:** M
**Files:** `rust/crates/aipet-core/src/{codex_watcher,board}.rs`, `rust/golden/Board.cs`, `rust/crates/aipet-core/tests/board.rs`, `rust/crates/aipet-core/tests/golden/board/**`
**Touches:** [rust/crates/aipet-core/src/codex_watcher.rs, rust/crates/aipet-core/src/sessions/describe.rs, rust/crates/aipet-core/src/board.rs, rust/crates/aipet-core/tests/board.rs, rust/crates/aipet-core/tests/golden/board/**, rust/golden/Board.cs]

### Approach
- `CodexWatcher` (`src/AiPet.Core/CodexWatcher.cs`): polls the session files every 2 s on a thread, reads the same
  files, and publishes a snapshot.
- `Board` (`src/AiPet.Core/Board.cs`):
  - `Refresh` merging by turn (`ByTurn`), else by time, with a hook winning a tie (`:133-160, 253`).
  - `Dismissed` holding (Ts, state, detail), so a bubble comes back when it changes.
  - Sections, error bubbles named by source ("Jira" and "GitHub") and put first in the reviews cap.
  - Deep links (`:96-102`), `AgentLabel`/`AppOf`, `State`/`Prop`, and `Find`/`All`/`Cards`.
- Golden mode `board`: the `BoardTests` cases as data, plus generated hook-versus-log merges.

### Investigation targets
**Required:**
- `src/AiPet.Core/Board.cs:1-271`, `src/AiPet.Core/CodexWatcher.cs`
- `tests/AiPet.Tests/BoardTests.cs`
**Optional:**
- `docs/ARCHITECTURE.md` §5 (the Board)
**From task 8 (2026-09-29):** Codex entries carry `turn` (the chat's own current turn id) and `turn_ended`, which the
Board's ByTurn ordering reads. .NET's OrdinalIgnoreCase `Contains` and `ToLowerInvariant` are ported as
`contains_ignoring_case` and `lower_invariant` in `sessions/describe.rs` (pub(super)); if the watcher needs them, make
them pub(crate) there rather than copying them.
**From task 11 (2026-09-30):** the Board reads `jira.config()`, `jira.issues()`, `jira.last_error()`, `github.config()`,
`github.review_requests()`, `github.pr_for_key()` and `github.last_error()`. C# nulls in `Issue` and `PullRequest`
fields are `""` here, so the C# Board's crash on a null Jira key (`PrForKey.GetValueOrDefault(null)`) can't happen.
`http.rs` carries its own `simple_upper` beside `describe.rs`'s casing helpers: share them if the Board needs them.
## Acceptance
- [ ] Every Board golden case gives the same bubbles, order, sections and links.
- [ ] `BoardTests` is ported. The watcher reads the same files as the C# on a fixture Codex home.
## Done summary
Ported the Board and the Codex log watcher to aipet-core, checked against a new golden mode, `board`, which writes board.json and watcher.json from the real C# Board and CodexWatcher.

- `board.rs` (the Board): `Board::refresh(&Sources, now)` merges by turn (`by_turn`), then by time, with a hook winning a tie. It also covers name ranks, deep links, and the review, music and error bubbles. It sorts with LINQ's stable order and .NET's double comparison. It keeps `Dismissed` as (ts, eff, detail) and applies the caps with errors first. It sets the mood (`state`/`prop`). The rest of the API is `dismiss(id, now)`, `find`, `all`, `cards`, `extra(section)`, `app_of`, `agent_label`, `pr_label`, `section_of`, `priority`, `status_color` and `MAX_CARDS`. `Sources::read(hooks, jira, github, codex, media, music_on)` reads the live sources. `Media` is what the Board reads of `IMediaPlayer`.
- `codex_watcher.rs` (the watcher): `CodexWatcher::new()` / `with_codex_home(dir)`, `start()` (poll now, then every 2 s, on its own thread), `stop()` and `sessions()`. It reads the index (last 512 KB) and scans the day folders (all of them first, then the newest 21 every 30 s). Each rollout file is read incrementally: session_meta within its first 4 MB, the last 512 KB, then what was appended; a rewritten file is read anew. Idle-for-hours files are looked at every 30 s, and only chats active in the last 3 h are shown. JSON parsing goes through `http::parse_json`/`Element`, so a line counts only if .NET's JsonDocument would accept it.
- `sessions/describe.rs`: an `impl AgentSessions` block exposes `v7_before`, `is_host_id`, `is_guid`, `lower_invariant` and `file_name` as pub(crate). Making describe.rs's functions pub(crate) would not have been enough: `sessions::describe` is a private module, so they stay unreachable from `crate::board` unless sessions/mod.rs (outside Touches) re-exports them.

Acceptance:
- Every Board golden case gives the same bubbles (all 18 fields, times bit for bit), order, cards, extra, mood and dismissals: `every_board_case_matches_the_csharp`. There are 33 cases and 59 steps:
  - the 10 BoardTests cases, with their chats and logs made by the real AgentSessions and CodexWatcher;
  - tables: states by age, hook chats (names, links, host ids including the `+0x` form), folders (os_specific), log chats, reviews, the caps, music, the mood, and a dismissal sequence;
  - 12 seeded random hook-versus-log merge boards of 30 chats each.

  The labels test is `app_agent_and_pull_request_labels_match_the_csharp`, and coverage is checked by `the_golden_data_covers_the_board`.
- The ported BoardTests are in `tests/board.rs`. `mod merge` has the 6 BoardMergeTests, run through the port's AgentSessions and a started CodexWatcher on a fixture Codex home. `mod reviews` has the 4 BoardReviewTests.
- The watcher reads the same files as the C#: `every_watcher_case_matches_the_csharp` covers 12 fixture Codex homes (basic, meta, events, names, index-tail, recency, discovery, discovery-case, incremental, tails, no-home and no-sessions). Unit tests in codex_watcher.rs cover the 30 s look-again and rescans, and the 21-folder limit. Unit tests in board.rs cover the 600 s music window and the case where the C# throws.
- Mutation checks: I tried 11 mutations of the port. The 10 that change behaviour the Board can reach each fail tests/board.rs. They cover the tie rule, the 2 s dismissal window, ByTurn's ended rule, `done`'s age, the name rank, TAIL 512 KB, FIRST_LINE 4 MB (both ways), the 11-character folder rule and the empty originator. On the first run the TAIL mutation survived, so the golden data now pins that window (commit 8c546af). The eleventh only changes a hook offered after a log with the same id, which can't happen: hooks are always offered first.

Choices to review:
- A turn id whose time .NET's `Convert.ToInt64` can't read makes the C# Refresh throw, which crashes the pet; the port lets time decide there. `AppOf`'s `StartsWith("sdk")`, thread_source's `StartsWith("memory")` and the day-folder sort are ordinal in the port and culture-sensitive in the C#. The index is read as UTF-8 with a leading BOM dropped. Folder listing doesn't follow symbolic links. Times without an offset read as UTC (`http::unix_seconds`). All are documented in the module docs.
- The C#'s null `Name` and `Cwd` are "" in `Session`.
- `http.rs` isn't changed. The Board uses its pub(crate) `equals_ignoring_case` (OrdinalIgnoreCase for the Jira keys) and `escape_data_string`.

baseline: green. The Rust chain passed pre-edit at fb1a6df, and `dotnet build AiPet.slnx -p:UseAppHost=false` then `dotnet test --no-build` gave 160 passed, 42 skipped.
Verify at 8c546af: the Rust chain was green (299 tests, with clippy and fmt clean), and `dotnet test AiPet.slnx --no-build` gave 160 passed, 42 skipped.

Follow-ups (outside Touches):
- `rust/golden/Program.cs` still says board is "filled in by the tasks that port their C#".
- claude.rs's private `is_host_id` could call `AgentSessions::is_host_id`.
- A later task could re-export describe.rs's helpers from sessions/mod.rs in place of the impl block.

Notes: NOTES_DIR/task-12-board.md.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

stage: impl-review - ran (codex: SHIP in one round, no findings; an earlier fan-out stopped on Codex credits)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 9d0d6ea4309d26e87d75f7e4242a764721414e64, 8c546af3fcbbf8d3c6728a0cf38374cd9bdcc228
- Tests: baseline: green (cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check at fb1a6df; dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped), cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check: green at 8c546af (299 tests; tests/board.rs 16 passed: every_board_case_matches_the_csharp, every_watcher_case_matches_the_csharp, app_agent_and_pull_request_labels_match_the_csharp, the_golden_data_covers_the_board, the_golden_data_covers_the_watcher, sources_are_read_from_the_live_watchers, merge::* 6, reviews::* 4; unit tests board::tests 2, codex_watcher::tests 2), cd rust && cargo clippy -p aipet-core --all-targets -- -D warnings: clean, dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped, golden: dotnet build rust/golden -c Release -p:UseAppHost=false && dotnet rust/golden/bin/Release/net10.0/aipet-golden.dll board: board.json 33 cases / 59 steps, watcher.json 12 cases, mutation checks: 11 mutations of board.rs and codex_watcher.rs; the 10 that change reachable behaviour each fail tests/board.rs (the TAIL one survived the first run, so 8c546af pins that window in the golden data); the eleventh only changes a hook offered after a log with the same id, which can't happen, integrated verify (Windows, work branch f289072 with task 12 merged): cargo test --workspace --no-fail-fast -- --skip credential_manager_tokens_are_the_apps green, clippy --workspace --all-targets -D warnings and fmt --check clean; dotnet build AiPet.slnx -p:UseAppHost=false, dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped
- PRs: