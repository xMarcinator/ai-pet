---
satisfies: [R3]
---
# fnt-81-tracker-task-bubbles-in-the-pet-fnt-64.3 Board: task bubbles, collapsed chats and the sessions setting

## Description
board.rs: Sources.tasks and tracker_error; `kind: task` bubbles (id task:<id>, eff from tone and liveness, detail = phase word and round, link = links.task), chats in a task's sessions hidden and counted, sessions_without_task hide; a tracker error bubble. Golden replays have no tasks.

**Size:** M

## Acceptance
- [ ] table tests for eff from tone/agents and the phase word
- [ ] a chat in a task's sessions shows only as the task's count
- [ ] hide setting hides chats not in a task while connected
- [ ] golden replays unchanged

## Done summary
Board task bubbles on branch fnt-81-tracker (c99e822). board.rs: Sources.tracker (TrackerSources {tasks, error, connected, sessions_without_task}, read from TrackerWatcher); Session.task carries the TrackerTask for the UI. fn tasks(): kind "task", id task:<id>, agent "tracker", section chats; eff = red→error, amber→attention, else the busiest collapsed run's state, else working when agents.total > 0, else idle (hidden like an idle chat); name task_name ("FNT-12 · title", short id alone when the title is empty or the short id); detail task_detail (needs first: permission with tool, decision, input, review; red: "The run failed or was paused"; else phase word with review round; else status); prop from the busiest run; link = links.task; ts = max(task.at, runs' ts). Chats in a task's sessions are removed; with hide + connected all other chats are removed. The pet's state counts tasks like chats. tracker:_error bubble (tracker-error kind, treated like jira/github errors in stacks and dismissals). core_thread runs TrackerWatcher (shared secret store, chats from the hooks' snapshot) and refreshes on its events. Tests: five unit tests (collapse and prop/time, tones and needs, hide only while connected, the error bubble, names/details); golden replays and core-thread tests unchanged; workspace clippy clean. Click on a task bubble opens links.task via the platform opener already; the error bubble's Settings page is task 4.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: c99e822175b11bbb82940b3e8a05acb5b2368d31
- Tests: cargo test -p aipet-core; cargo test -p aipet --bin AiPet core_thread
- PRs: