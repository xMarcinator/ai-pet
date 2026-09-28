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
TBD

## Evidence
- Commits:
- Tests:
- PRs:
