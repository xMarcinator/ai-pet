---
satisfies: [R2]
---
# fnt-81-tracker-task-bubbles-in-the-pet-fnt-64.2 TrackerWatcher: enrol, hello, snapshot and feed with resume

## Description
aipet-core/src/tracker.rs like jira.rs: tracker.json settings, `AiPet:Tracker` secret JSON, loopback enrol (POST /enroll {code, role:companion}), hello (companion.actions_allowed), GET /api/v1/tasks/active, WS feed via tungstenite with (feed, seq) resume and 1 s→60 s backoff, revocation handling, STALE_AFTER. A fake tracker (axum or tiny_http already in the tree? reuse what the tests use) serves enrol, hello, active, the feed with snapshot/changes/policy close. New dependency: notices and hakari regenerated.

**Size:** M

## Acceptance
- [ ] enrol stores the token in the secret store, never in a log or URL
- [ ] snapshot then changes keep the view; a reconnect resumes by (feed, seq); another feed id replaces the view
- [ ] revocation drops tasks and stops retrying with its message
- [ ] check-notices and hakari verify pass

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
