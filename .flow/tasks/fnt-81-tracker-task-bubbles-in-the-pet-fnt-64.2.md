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
TrackerWatcher on branch fnt-81-tracker (9b01785): aipet-core/src/tracker/{api,view,watcher}.rs. Enrol: POST {base}/enroll {code, role:companion} (JSON) -> {id, token}, stored as AiPet:Tracker {base_url, device_id, token} via SecretStore (secrets::TRACKER), never in tracker.json or a URL; tracker.json holds base_url and sessions_without_task (show|hide). Thread aipet-tracker: GET /api/attach/hello (companion.actions_allowed), then WS /api/v1/companion/feed with Authorization: Bearer (tungstenite 0.30, handshake only), resume ?feed=&since= from TaskView's point, 1 s->60 s backoff (reset after a connection got through), Resync on another feed id. Revocation: policy close with "Device credential no longer valid", or 401/403 on hello, the handshake or a lookup -> tasks dropped, status Revoked, no retry. STALE_AFTER 2 min after the connection is lost. Chat collapse: the tracker's TaskProgress has no sessions list (the doc's v2 shape assumed one), so while connected the watcher looks up the pet's chats with GET /api/v1/runs/by-session/{sid} (new ones, misses again after 60 s, 20 at once) and tasks() fills TrackerTask.sessions. Deviations: the feed's snapshot stands in for GET /api/v1/tasks/active (same content, one connection); the tracker also has an input need (a run waiting for a reply), now in wire::Need; only loopback trackers until task 6. tungstenite + notices (374 crates) + hakari regenerated. Tests: api/view/watcher unit tests and tests/tracker.rs against a fake tracker (enrol refusal, snapshot, run lookup, ordering, skipped old seq, resume at since=3, removal, revocation by close with no reconnect, refused token, forget, unreachable). Workspace clippy -D warnings clean; aipet-core and aipet-ipc tests pass. Not yet wired into the app (task 3/4).

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 9b017854c670d1fcf77a131d73bc9369a11860b6
- Tests: cargo test -p aipet-core -p aipet-ipc
- PRs: