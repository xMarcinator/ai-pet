---
satisfies: [R9]
---
# fn-1-migrate-aipet-from-net-to-rust.12 Codex log watcher and the Board

## Description
Port `CodexWatcher` and `Board`, which merges hooks, Codex logs, Jira, GitHub and media into bubbles. Both are checked
against a new golden mode for the Board.

**Size:** M
**Files:** `rust/crates/aipet-core/src/{codex_watcher,board}.rs`, `rust/golden/Board.cs`, `rust/crates/aipet-core/tests/board.rs`, `rust/crates/aipet-core/tests/golden/board/**`
**Touches:** [rust/crates/aipet-core/src/codex_watcher.rs, rust/crates/aipet-core/src/board.rs, rust/crates/aipet-core/tests/board.rs, rust/crates/aipet-core/tests/golden/board/**, rust/golden/Board.cs]

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
## Acceptance
- [ ] Every Board golden case gives the same bubbles, order, sections and links.
- [ ] `BoardTests` is ported. The watcher reads the same files as the C# on a fixture Codex home.
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
