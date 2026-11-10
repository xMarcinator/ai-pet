---
satisfies: [R1]
---
# fnt-81-tracker-task-bubbles-in-the-pet-fnt-64.1 Protocol v2: task messages and v2 events in aipet-ipc

## Description
aipet-ipc/src/protocol.rs and event.rs: v2 constants and `TaskMessage`/`TrackerTask` types (serde, unknown fields ignored, unknown kind/tone tolerated); the listener accepts `v:2` events like v1 and refuses `task` lines with "task messages come from the tracker watcher only". Golden JSON tests from the tracker doc's three examples.

**Size:** S

## Acceptance
- [ ] v1 and v2 events parse to the same event
- [ ] the doc's upsert and remove lines round-trip; unknown fields ignored
- [ ] a task line on the pipe is refused with the message

## Done summary
Protocol version 2 on branch fnt-81-tracker (51ef01e). aipet-ipc: VERSION_2, TASK/UPSERT/REMOVE/OP/SOURCE/ID and TASK_ONLY_FROM_WATCHER; the header documents v2. The hook server (aipet-core server/answer.rs) applies a v2 event like v1's, refuses a v2 task line with "task messages come from the tracker watcher only", and keeps refusing a v2 ping as "unsupported version" (the C# answers table is unchanged). Deviation from the plan: the serde types (TrackerTask, Count, Placement, Agents, Need, Tone, Links, TaskMessage with parse/to_line) live in aipet-core/src/tracker/wire.rs, because the hook builds aipet-ipc without serde. Lenient reading: unknown members ignored, unknown need kinds dropped, unknown tone normal. Tests: the tracker doc's upsert and remove examples read and round-trip; additions ignored; each malformed line says why; tests/server.rs version_2_takes_events_and_refuses_task_lines. cargo test -p aipet-ipc -p aipet-core (lib, server) pass; clippy -D warnings clean.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 51ef01e5327d96afe564192f79e2cb382bebf18f
- Tests: cargo test -p aipet-ipc -p aipet-core --lib --test server
- PRs: