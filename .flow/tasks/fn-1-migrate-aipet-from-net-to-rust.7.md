---
satisfies: [R8]
---
# fn-1-migrate-aipet-from-net-to-rust.7 Core golden harness and AgentSessions (Claude path)

## Description
Fill in the chat store and its Claude ordering in `aipet-core`, checked against the golden generator's sessions mode.
The mode is filled in here and extended by task 8.

**Size:** M
**Files:** `rust/crates/aipet-core/src/sessions/{mod,claude,describe,titles}.rs`, `rust/golden/Sessions.cs`, `rust/crates/aipet-core/tests/sessions.rs`, `rust/crates/aipet-core/tests/golden/sessions/**`
**Touches:** [rust/crates/aipet-core/src/sessions/mod.rs, rust/crates/aipet-core/src/sessions/claude.rs, rust/crates/aipet-core/src/sessions/describe.rs, rust/crates/aipet-core/src/sessions/titles.rs, rust/crates/aipet-core/tests/sessions.rs, rust/crates/aipet-core/tests/golden/sessions/**, rust/golden/Sessions.cs]

### Approach
- Golden mode `sessions`: a case is a list of steps. Each step is an envelope and a `now`, plus transcript files as
  fixtures. Replay through C# `AgentSessions.Apply` (`src/AiPet.Core/AgentSessions.cs:62`) and record the outcome, the
  log line and `Snapshot()` at the chosen steps.
  - Cases: each Claude case in `tests/AiPet.Tests/OrderingTests.cs` as data, plus seeded random sequences (shuffled
    pairs, delays and duplicates) generated in C# and written out with their inputs.
- Port:
  - `Entry`, `Apply`'s Claude path, `Rank`, `Stale`, `Stamp` (`:504-524`) and `ClaudePair` with the 1 s window and 16
    halves kept (`:540-560`).
  - `CallKey` (`:735`), tombstones, and `Describe` (`:758-786`).
  - `ChatTitle`: the last 512 KB, and the last `custom-title` line (`:267-289`).
  - The `Peek`-before-lock pattern and the `Snapshot` clone. Guard state with a `Mutex`; file reads happen outside it.

### Investigation targets
**Required:**
- `src/AiPet.Core/AgentSessions.cs:1-560` — the store and the Claude path
- `tests/AiPet.Tests/OrderingTests.cs` — the Claude cases
- `docs/ARCHITECTURE.md` §6.3–6.4 — the event→state table and ordering
**Optional:**
- `rust/golden/Program.cs` — generator structure
- `rust/crates/aipet-sprite/tests/golden.rs` — the replay pattern

### Key context
- .NET's `HashCode.Combine` is seeded per process, so `CallKey` values differ between runs. Only pairing decisions
  must match, not the keys. Record decisions, never hashes.
## Acceptance
- [ ] Every Claude golden case gives the same outcomes, log lines and snapshots.
- [ ] The Claude cases in `OrderingTests` are ported as Rust tests.
- [ ] Missing or oversized transcripts and malformed payloads give the C#'s outcomes.
## Done summary
Ported AgentSessions' Claude path to `aipet-core::sessions`: the Mutex-guarded chat store (`AgentSessions::apply(envelope, now)`, `snapshot`, `Entry`, `Applied`), peek-before-lock title reads, Claude's event table, Rank/Stale/Stamp, the 1 s PreToolUse/PermissionRequest pairing with 16 halves, CallKey, tombstones, pruning, where and host ids (Guid "D" parsing, legacy forms included), Describe/Short/log lines in UTF-16 units, and ChatTitle over the last 512 KB. `rust/golden`'s sessions mode now writes `tests/golden/sessions/claude.json` (63 cases, 1796 envelopes: the 13 ClaudeOrderingTests, event/tool/text tables, transcripts of every shape, malformed envelopes, 32 seeded random multi-chat sequences), which `tests/sessions.rs` replays; the Claude ordering tests are also ported as Rust tests.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

Acceptance: every Claude golden case matches (outcomes, log lines, snapshots, times bit for bit): `every_golden_case_matches_the_csharp`. The Claude OrderingTests are ported (`tests/sessions.rs` mod `ordering`). Missing, oversized and odd transcripts and malformed payloads come from the C#: golden cases `claude/titles`, `claude/title-reads`, `claude/malformed`.

What the golden data showed about the C# (and the port now does):
- ChatTitle skips a line whose top-level object has a duplicate key (escaped spellings too), a line deeper than 64, and a customTitle with a lone surrogate. Other members are not decoded, so a lone surrogate or 1e400 elsewhere is fine.
- OrdinalIgnoreCase keeps ı and ſ, while ToUpperInvariant maps ſ to S. Short follows ICU's simple mapping; non-BMP first letters stay as they are.

Where parity bends (for the reviewer to accept or reject):
- CallKey's whole-input fallback hashes serde_json's text, not System.Text.Json's. Inputs that differ only in key order or number spelling, or a command equal to another input's STJ-escaped JSON, pair differently. Claude writes both halves of a call the same way, so real pairs are unaffected. serde_json has no preserve_order here, and Cargo.toml was outside Touches.
- A string the C# cuts mid-pair holds a lone surrogate there; the port holds U+FFFD, which is what any UTF-8 writer of the C# string produces. The generator writes U+FFFD too.
- Codex envelopes fall through to "ignored" until task 8 fills codex.rs/turns.rs. Entry already has has_transcript/turn/turn_ended.

Follow-ups (outside Touches):
- `aipet-ipc` `connect::tests::windows::a_silent_pet_times_out` is a pre-existing Windows flake: `cargo test -p aipet-ipc --lib` fails about 1 run in 2 at HEAD (took 1.9977 s < 2 s). It turns the workspace test gate red on this machine.
- clippy 1.98 with `-D warnings` (CI's flags) fails `aipet-sprite` (chunks_exact_to_as_chunks, manual_is_multiple_of). The pinned 1.89 toolchain may not have these lints.
- `rust/golden/Program.cs` still says only sprite and ipc modes are written.
- On this machine, regenerate with `dotnet build rust/golden -c Release -p:UseAppHost=false` and `dotnet rust/golden/bin/Release/net10.0/aipet-golden.dll sessions`: no fresh exe. The generator and the Rust replay both retry file writes that the virus scanner holds.
- `claude/describe-paths` is os_specific (Path.GetFileName rules). The committed file was written on Windows, so Linux skips that case. Linux's rules are covered by the unit test `file_names_follow_the_os_rules`.

stage: impl-review - ran (codex: NEEDS_WORK then SHIP; fix fbc6632)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 018b9d92af7f4cba4f06c97d9f9c55009f5287bd, fbc663220d6b6c4f0a816a110c14196042c07445
- Tests: baseline: green (cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check at 242d7cc; dotnet test tests/AiPet.Tests -p:UseAppHost=false --filter FullyQualifiedName~OrderingTests: 46 passed), cd rust && cargo test -p aipet-core: 18 passed (every_golden_case_matches_the_csharp over 63 cases / 1796 envelopes incl. the os_specific Windows path case, the_golden_data_covers_the_claude_path, 13 ported ClaudeOrderingTests, 3 unit tests), cd rust && cargo test --workspace --no-fail-fast: every test binary green except aipet-ipc connect::tests::windows::a_silent_pet_times_out (INHERITED FLAKE, not in this diff: aipet-ipc is untouched; cargo test -p aipet-ipc --lib fails 2 of 4 runs at HEAD with took 1.9977s < 2s), cd rust && cargo clippy --workspace --all-targets: rc 0 (warnings only in aipet-sprite and aipet-ui, as at baseline); cargo clippy -p aipet-core --all-targets --locked -- -D warnings: clean, cd rust && cargo fmt --all -- --check: clean, dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx -p:UseAppHost=false --no-build: 160 passed, 42 skipped, 0 failed (no apphost: this machine blocks freshly built exes; plain dotnet test AiPet.slnx does not build AiPet.UI, which fails 6 ResourceTests/VelopackTests for a missing AiPet.dll), golden: dotnet build rust/golden -c Release -p:UseAppHost=false && dotnet rust/golden/bin/Release/net10.0/aipet-golden.dll sessions: 63 cases, 1796 envelopes, mutation checks on the replay (pair window, dotless i, duplicate keys, depth 64, log width, 512 KB window, ts clock rule, pruning tombstones): each fails the golden test; peek-always-read and stamp max are unobservable on the Claude path, integrated verify (Windows, work branch 0ccb70c): cargo test -p aipet-core green (15 + 16 + 7), cargo test --workspace green, clippy --workspace --all-targets -D warnings and fmt --check clean; the flaky aipet-ipc test the worker hit is fixed on the target in 7bac145
- PRs: