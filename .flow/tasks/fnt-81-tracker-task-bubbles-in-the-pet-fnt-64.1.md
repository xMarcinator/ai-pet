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
TBD

## Evidence
- Commits:
- Tests:
- PRs:
