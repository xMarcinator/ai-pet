# Tracker task bubbles in the pet (FNT-64 companion)

## Conversation Evidence

> user (2026-11-10): "also please work on the tracker integration in the pet since that task is now done in the tracker"

The tracker side is flow-next-tracker's FNT-64 ("AiPet integration: task bubbles, attention feed and deep links"), in self-review on its `fnt-31-production-ready-a-clean-hostable` branch. Its spec ends with an appendix of pet-side work "built in `../ai-pet`, not FNT-64 tasks", and the contract the pet builds against is that branch's `docs/integrations/aipet.md`. Quoted from FNT-64's evidence (user, 2026-11-05): "Task bubbles, not session bubbles", and "the wire protocol is NOT frozen".

## Goal & Context

Today a tracker-managed run shows on the pet as a headless terminal chat. It is titled with the prompt's first line, a click finds no window, Stop can't reach it, and the tracker's own knowledge never reaches the pet. That knowledge is the task, its phase and review round, the acceptance count, and the prompts waiting in the tracker. With this spec the pet enrolls as the tracker's **companion device** and shows **one bubble per active tracker task**:
- its short id and title;
- the phase ("Reviewing · round 2");
- subtasks and acceptance done of total;
- how many agents work on it;
- amber when it needs the user, red on a failed run or a tripped guard.

Chats that belong to a task collapse into its bubble. A click opens the task in the tracker's desktop client through `fnt://`. When the user has switched them on for the device, the bubble can answer a permission prompt and pause or stop the run.

## Architecture & Data Models

- **Protocol v2 (`aipet-ipc`).** `VERSION` stays 1 for what the hook sends; the listener also accepts `v: 2`. A v2 `event` carries v1's payload unchanged. A v2 `task` message (`op: upsert | remove`, `source: "tracker"`) has the shape in the tracker's doc: `id`, `short_id`, `title`, `status`, `phase`, `review_round`, `subtasks{done,total}`, `acceptance{done,total}`, `placement{runner,lane}`, `agents{total,runs,sessions,subagents}`, `needs[{kind,run_id?,request_id?,tool?}]`, `tone`, `sessions[]`, `links{task,run?}` and `at`. Unknown fields are ignored and unknown `kind` and `tone` values read as none or normal. Task messages come only from the in-process `TrackerWatcher`: the pipe and socket refuse a `task` line ("task messages come from the tracker watcher only"), so no local process can paint bubbles.
- **`TrackerWatcher` (`aipet-core/src/tracker.rs`)**, built like `JiraWatcher` and `GitHubWatcher`, on its own thread, with `Event::Changed` to the owner:
  - **Settings:** `tracker.json` in the data folder (`base_url`, `sessions_without_task: show | hide`, default show).
  - **Credentials:** `AiPet:Tracker` in the existing `SecretStore`, a JSON value `{base_url, device_id, token, cert?, key?}`.
  - **Enroll:** `POST {base}/enroll {code, role:"companion"}` on loopback gives a bearer token. A hosted tracker's mutual-TLS listener gives a certificate for a CSR (task .6).
  - **Hello:** `GET /api/attach/hello`, whose `companion.actions_allowed` decides whether the bubble shows actions.
  - **Snapshot and feed:** `GET /api/v1/tasks/active`, then `WS /api/v1/companion/feed?feed=<id>&since=<seq>`. It applies the `snapshot` and `change` messages to a `TaskView` (map by task id, plus `feed` and `seq`). A disconnect reconnects with backoff (1 s doubling to 60 s) and resumes from `(feed, seq)`.
  - **Failures:** a policy close or a 401/403 means the device was revoked. The pet drops the tasks, keeps the error for the error bubble and Settings, and stops retrying until the user enrolls again.
  - **Calls:** the HTTP calls go through the pet's `Http` (`ureq`). The WebSocket uses `tungstenite` (sync, rustls), which brings the one new dependency, its notices and a hakari regeneration.
- **Board.** `Sources` gains `tasks: Vec<TrackerTask>` and `tracker_error`. Each task becomes a `Session` with `kind: "task"` and id `task:<id>`, in the chats stack:
  - **state** (`eff`): `error` when the tone is red, `attention` when amber, `working` while a run or agent is live, else `idle`;
  - **detail:** the phase word and review round;
  - **link:** `links.task`.

  A chat whose session id is in a task's `sessions` is left out and counted in the task's badge. With `sessions_without_task: hide`, chats not in any task are hidden while the tracker is connected. Golden replays never have tasks, so the C# parity is unchanged.
- **UI (`aipet-ui`).**
  - **The bubble:** a task bubble draws its short id and title, the phase word, a thin strip for subtasks done of total, "acceptance n/m", and an agent-count badge. Amber and red use the existing attention and error colours. A click opens `links.task` with the system opener.
  - **Settings › Tracker:** fields for the address and the one-time code, Enroll, the connection's status, the "Sessions without a task" choice, and Forget. Forgetting deletes the stored credential and says so; revocation is done in the tracker's client.
- **Actions (opt-in).** Only when hello says `actions_allowed`:
  - a permission need shows **Allow once / Deny**, which posts to `POST /api/runs/{run}/permissions/{request}`;
  - a live run shows **Pause** and **Stop**, which post to `/pause` and `/cancel`.

  A conflict ("already answered") shows as the bubble's detail. The bubble never shows these buttons when hello doesn't allow them.

## API Contracts

The pet calls the routes in the tracker's `docs/integrations/aipet.md` and no others. Its credential sits only in the secret store and never in a URL or a log. `fnt://` links open with `xdg-open`, `open` or `start`, as other bubble links already do.

## Edge Cases & Constraints

- **No tracker configured:** no thread and no bubbles. Nothing changes for users without a tracker.
- **The tracker unreachable:** an error bubble "Tracker: can't reach <base>", as Jira's. The tasks stay shown as of the last snapshot for `STALE_AFTER` (2 min), then drop.
- **A feed with another `feed` id, or a snapshot:** it replaces the view whole.
- **Titles:** they arrive already cleaned by the tracker. The pet shows them as given and logs no titles.
- **Privacy:** nothing from the pet's hook is sent to the tracker, and the pet sends no session ids except in `runs/by-session` lookups. That lookup isn't needed for the bubbles and stays unused until a task needs it.
- **The pet and the tracker never start each other.**

## Acceptance Criteria

- **R1:** Protocol v2 types for task messages and v2 events in `aipet-ipc`, with golden JSON tests. v1 events are still accepted and task lines are refused on the pipe. Errors: a `task` line on the pipe gets "task messages come from the tracker watcher only".
- **R2:** `TrackerWatcher` enrolls a companion on loopback with a one-time code, stores `AiPet:Tracker` in the secret store, reads hello, the snapshot and the feed, resumes by `(feed, seq)` and reconnects with backoff. Tests run against a fake tracker. Errors: a bad or used code shows the tracker's refusal; revocation drops the tasks and shows "Tracker: this pet was revoked; enroll it again".
- **R3:** The Board shows one bubble per active task with tone, phase, counts and badge. Chats in a task's `sessions` collapse into it, and the "Sessions without a task" setting works. Golden replays are unchanged.
- **R4:** The UI draws the task bubble (progress strip, acceptance, phase word, badge), opens `links.task` on click, and has Settings › Tracker (enroll, status, sessions setting, forget). Errors: an enroll failure shows on the page.
- **R5:** The actions (Allow once / Deny, Pause, Stop) show only when hello allows them and post to the tracker's routes. Errors: a 403 or a conflict shows on the bubble.
- **R6:** A hosted tracker: enrollment on the mutual-TLS listener with a CSR, and calls with the certificate. Errors: a certificate refused or revoked is shown as revocation.
- **R7:** Docs (README and ARCHITECTURE, the privacy note), the third-party notices, and a hands-on pass against a real tracker built from FNT-64's branch.

## Boundaries

- No ingestion of pet hook events by the tracker, and no webhooks.
- No starting runs from the pet.
- No process control of the tracker.
- No change to how the hook talks to the pet.

## Decision Context

- **Task lines only from the in-process watcher:** the pipe is open to any local process, so accepting tasks there would let anything paint "needs you" bubbles.
- **Sync `tungstenite` over an async client:** the watchers are threads with `ureq`, and the pet's only tokio runtime belongs to the MCP listener (FNT-71).
- **The secret store, not a keyring crate:** it already keeps `AiPet:Jira` and `AiPet:GitHub` under the platforms' own interfaces.
- **The pet owns its state:** the tracker settings are the pet's. An MCP tool for them can follow in FNT-71's settings tools, as Settings and MCP are equal front-ends.

## Requirement coverage

| Req | Task |
|---|---|
| R1 | .1 |
| R2 | .2 |
| R3 | .3 |
| R4 | .4 |
| R5 | .5 |
| R6 | .6 |
| R7 | .7 |
