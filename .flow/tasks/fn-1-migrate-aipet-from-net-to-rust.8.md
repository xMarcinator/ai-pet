---
satisfies: [R8]
---
# fn-1-migrate-aipet-from-net-to-rust.8 AgentSessions Codex ordering

## Description
Port the Codex half of AgentSessions: per-thread turn tracking with UUIDv7 times, call matching and titles, checked
against the golden cases.

**Size:** M
**Files:** `rust/crates/aipet-core/src/sessions/{codex,turns}.rs`, `rust/golden/Sessions.cs` (Codex cases), `rust/crates/aipet-core/tests/golden/sessions/**`, `rust/crates/aipet-core/tests/sessions.rs`
**Touches:** [rust/crates/aipet-core/src/sessions/**, rust/crates/aipet-core/tests/sessions.rs, rust/crates/aipet-core/tests/golden/sessions/**, rust/golden/Sessions.cs]

### Approach
- Port:
  - `CodexOrder` (`src/AiPet.Core/AgentSessions.cs:598-658`) and `Turns` (`:659-731`), with threads keyed `""`,
    `agent:<id>` and `transcript:<path>`.
  - `V7Before` with the clock-back guard (`:748-749`), the 5 s `HookSpread`, and the call matching: `tool_use_id`, else
    `CallKey` plus timing, including reruns after a sandbox denial.
  - A manual `/compact` ending the turn; a late `UserPromptSubmit`.
  - `CodexDescribe` (`:383-396`), `Originator` (the first 4 MB, `:411-427`) and `ThreadName` (the last 256 KB of
    `session_index.jsonl`, `:431-453`).
- Golden: each Codex case in `OrderingTests`, plus seeded random Codex sequences with sub-agents and interleaved turns.

### Investigation targets
**Required:**
- `src/AiPet.Core/AgentSessions.cs:290-458, 587-809`
- `tests/AiPet.Tests/OrderingTests.cs` — the Codex cases
**Optional:**
- `docs/ARCHITECTURE.md` §6.4, §6.8 (one turn, end to end)
## Acceptance
- [ ] Every Codex golden case, including the random corpus, gives the same outcomes and snapshots.
- [ ] The Codex cases in `OrderingTests` are ported as Rust tests.
- [ ] Clock-set-back and future turn ids behave as in the C#.
## Done summary
Ported the Codex half of AgentSessions to `aipet-core::sessions`:
- `codex.rs`: the Codex event table; `CodexStale` / `CodexOrder` (rules 1-5), `ManualCompact`, `ThreadOf` (`""`,
  `agent:<id>`, `transcript:<path>`, the path matched ignoring case the way .NET's OrdinalIgnoreCase does);
  `CodexDescribe` (apply_patch's files, spawn_agent, view_image); `CodexWhere`; and `Originator`, which reads the
  first line within the first 4 MB and runs the C#'s regex over the raw text.
- `turns.rs`: `Turns` per thread, with the last 16 turns and 64 calls. Calls match by tool_use_id, else by CallKey
  within the 5 s HookSpread, including the request a rerun waits on and its removal by the asking call's
  PostToolUse. Also `V7Before` with its clock-back guard.
- `titles.rs`: `ThreadName` over the last 256 KB of `session_index.jsonl`.
- `mod.rs`: the codex branch of `apply`, with the log's `lag=` written as .NET's `"0"` format writes it, and
  `AgentSessions::with_codex_home` so tests never read `$CODEX_HOME`.

`rust/golden`'s sessions mode now also writes `tests/golden/sessions/codex.json`: 79 cases, 1889 envelopes. It has
each CodexOrderingTests case (a theory's each inline data too), the event, tool, originator and name tables, and
edges for cwd and threads, lag, malformed envelopes, turn ids, calls, the clock and SessionEnd/pruning. It also has
32 seeded random chats: Windows (hooks 1-4 s late, no PostToolUse) and elsewhere, with sub-agents, other threads,
reruns after a sandbox denial, late prompts, /compact, a clock set back with its turn ids, duplicates, resumes, and
names and originators arriving late. `tests/sessions.rs` replays both golden files, and CodexOrderingTests is
ported as mod `codex_ordering`.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

Acceptance:
- Every Codex golden case, the random corpus included, gives the same outcomes, log lines and snapshots (times bit
  for bit): `every_codex_case_matches_the_csharp`, with coverage checked by `the_golden_data_covers_the_codex_path`.
  Claude still matches: `every_claude_case_matches_the_csharp`, also against a freshly regenerated claude.json.
- The Codex OrderingTests are ported: `tests/sessions.rs` mod `codex_ordering` (27 facts, and 2 theories that loop
  over their 3 cases).
- Clock set back and future turn ids: golden cases `codex/clock`, `codex/turn-ids` and
  `ordering/codex/ClockSetBack_…`, plus seeded clock-back chats in `random-codex/*`.
- Mutation checks: 27 mutations of the port. Every observable one fails the tests. The other two can't be seen in
  the C# either: an originator read while `where` is already set.

What the golden data showed about the C#:
- A Codex turn id that .NET's Guid parser takes, with a `+` or `0x` inside its second group, makes
  `Convert.ToInt64` throw in V7Before. HookServer then answers `error:FormatException`. The port gives
  `Applied { outcome: "error:FormatException", log: "event -> error:FormatException" }`, as that server reports it.
  The Rust server (task 9) must refuse it and not fire Changed (see the run notes).
- `lag=` rounds to 15 significant digits first, then half away from zero, and keeps a negative zero's sign (`-0ms`).
- CodexWhere's `ToLowerInvariant` is the simple lower-case mapping, surrogate pairs too, and keeps İ. There is no
  final-sigma rule.
- A thread_name of `""` stops the search: the chat gets no name, and earlier lines aren't read. A blank name sets
  ChatTitle to `""`.

Where parity bends: none beyond task 7's (CallKey's hash is the port's own, and only which calls it pairs must
match).

Follow-ups (outside Touches):
- `rust/golden/Program.cs` still says "sessions (the Claude path so far)".
- `contains_ignoring_case` / `lower_invariant` are pub(super) in `sessions/describe.rs`. Move them to a shared module
  if the Codex watcher (task 12) needs them.
- claude.json wasn't recommitted: a regeneration only changes its times.

stage: impl-review - ran (codex: SHIP first round, 0 findings from 3 reviewers)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 60d5a5f678ca8e03bb05b45e075097065fef872d
- Tests: baseline: green (cd rust && cargo test --workspace -- --skip credential_manager_tokens_are_the_apps, cargo clippy --workspace --all-targets -- -D warnings, cargo fmt --all -- --check at 3bc7a88; dotnet test tests/AiPet.Tests -p:UseAppHost=false --filter FullyQualifiedName~OrderingTests: 46 passed), cd rust && cargo test --workspace -- --skip credential_manager_tokens_are_the_apps: 182 passed, 0 failed (tests/sessions.rs: 46 passed, including every_claude_case_matches_the_csharp, every_codex_case_matches_the_csharp, the_golden_data_covers_the_codex_path and 29 ported CodexOrderingTests), cd rust && cargo clippy --workspace --all-targets -- -D warnings: clean, cd rust && cargo fmt --all -- --check: clean, dotnet build AiPet.slnx -p:UseAppHost=false: 0 warnings, 0 errors; dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped, 0 failed, golden: dotnet build rust/golden -c Release -p:UseAppHost=false && dotnet rust/golden/bin/Release/net10.0/aipet-golden.dll sessions: codex.json 79 cases, 1889 envelopes (every outcome including error:FormatException); claude.json regenerated 64 cases / 1831 envelopes, replays green, not recommitted (only its times change), mutation checks on the Codex port (27 mutations: spread, past 16, calls 64, lag rounding and digits, ordinal casing, ToLowerInvariant, v7 clock guard, regex whitespace, 256 KB and 4 MB windows, empty names, patch dedupe, manual compact thread, late prompt, stamp when late, marker removal, cwd prefix, rule 4, rule 2, when names are read, the tie on equal times, FormatException, duplicate keys): every observable one fails the tests; 2 are unobservable in the C# too (originator read while where is set), gate receipts not written: parallel wave, the conductor owns shared Flow state, integrated verify (Windows, work branch 7819186 with tasks 3 and 8 merged): cargo test --workspace -- --skip credential_manager_tokens_are_the_apps green (sessions: 46 passed), clippy --workspace --all-targets -D warnings and fmt --check clean; dotnet build AiPet.slnx -p:UseAppHost=false, dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped, review note: two of the three Codex reviewers ran aipet-core's whole suite, including credential_manager_tokens_are_the_apps, which writes and deletes a throwaway AiPet:GoldenTest:<pid>-<nanos> entry in the real Credential Manager; it passed. The conductor then made that test opt-in (AIPET_TEST_CREDENTIAL_MANAGER, set by CI)
- PRs: