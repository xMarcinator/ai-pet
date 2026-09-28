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
TBD

## Evidence
- Commits:
- Tests:
- PRs:
