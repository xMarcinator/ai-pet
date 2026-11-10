# The tracker's task bubbles against a real tracker (fnt-81 task 7)

The pet's tracker watcher (`aipet-core/src/tracker/`) checked against a real flow-next tracker, not a fake: its FNT-64
build, `fnt-31-production-ready-a-clean-hostable` at 355294f5 (2026-11-10).

**How:** the tracker ran as `serve --port 3299` in a scratch folder, with its own database and `--secrets memory`.
So neither the owner's tracker (3210) nor their keyring was touched, and it was stopped afterwards. The pet's side was
`tests/tracker_real.rs`, an ignored test that runs the real `TrackerWatcher` against the address and code it's given
and prints what each bubble would say:

```text
AIPET_TRACKER_URL=http://127.0.0.1:3299 AIPET_TRACKER_CODE=<code> \
    cargo test -p aipet-core --test tracker_real -- --ignored --nocapture
```

The task came from the tracker's own MCP:
- `sync_task`: a spec with one subtask in progress and one done;
- `report_session`: a running session under it;
- `report_progress`: phase `reviewing`, round 2.

The codes came from `flow-next-tracker enroll-code --role companion --id pet-check`.

## On this computer (loopback), 2026-11-10, Linux

| Check | Result |
|---|---|
| Enroll | The one-time code redeemed at `POST /enroll` for a token. The tracker's companion routes refuse a call without it (401). |
| Snapshot | Connected; actions off. The task read as `FNT-90 · Pet check over the companion feed`, `Reviewing · round 2`, subtasks 1/2, 1 agent. |
| A change on the feed | `report_progress` with phase `fixing` reached the pet over the feed: the bubble said `Fixing` within the second. |
| Revocation | `flow-next-tracker revoke pet-check`: the tracker closed the feed with "Device credential no longer valid". The watcher went to Revoked, dropped the task and stopped. The old token now gets 401. |
| The actions switch | Turned on with `PATCH /api/devices/pet-check {"actions_allowed": true}` while the pet was connected. The pet saw it within one lookup interval. |
| An action | A Stop for a run that doesn't exist passed the tracker's companion scope (no 403). It came back as the tracker's refusal, "run not found", which is what the bubble shows. |

**Found and fixed here:** the watcher read `hello` only when it connected. So a pet already connected didn't see the
user switch its actions on until its next reconnect, which could be hours later. It now reads `hello` again every
10 s while connected (`watcher.rs`, `LOOKUP_EVERY`), and `tests/tracker.rs` checks it without a restart.

## Not checked here

- **A hosted tracker:** this needs the tracker's `host` with its own CA, which wasn't run. Its mutual TLS is checked
  only against the fake in `tests/tracker_tls.rs`: the CA pinned by the code, the CSR, the certificate on every call
  and on the feed, and a revoked certificate refused.
- **A real permission prompt answered from the bubble:** this needs a managed Claude run.
- **A managed run's chat collapsing into its task:** this needs `runs/by-session` to find a real run. The lookup
  itself ran against the real tracker with no chats to ask about.
- **How the bubble and the Settings › Tracker page look:** this needs the pet's window, which isn't started without
  the owner. The bubble's layout is checked in `aipet-ui`'s tests, not on screen.
