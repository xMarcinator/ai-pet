---
satisfies: [R5]
---
# fnt-81-tracker-task-bubbles-in-the-pet-fnt-64.5 Bubble actions: answer a prompt, pause and stop

## Description
When hello.companion.actions_allowed: Allow once / Deny on a permission need (POST /api/runs/{run}/permissions/{request}), Pause and Stop on a live run (/pause, /cancel). Conflict and 403 shown on the bubble. Hidden otherwise.

**Size:** S

## Acceptance
- [ ] buttons only when allowed
- [ ] each posts the right route and body (fake tracker)
- [ ] conflict and 403 show on the bubble

## Done summary
Actions on branch fnt-81-tracker (4672a34). Core: tracker::Action {Allow, Deny, Pause, Stop}; TrackerWatcher::act posts POST /api/runs/{run}/permissions/{request} {"decision":"allow"|"deny"}, /api/runs/{run}/pause and /cancel with Authorization: Bearer and JSON; refused locally ("Answers and pause/stop are off for this pet") unless status is Connected{actions_allowed: true}; a non-2xx gives the tracker's {error} text (409 "already answered by …"). TrackerTask::run_id() (from links.run fnt://run/<id>) and prompt(). TrackerSources.actions_allowed → Board::tracker_actions(). UI: Button::{Allow, Deny, PauseRun, StopRun} (Buttons is now u16), icons Check/Close/Pause and a new filled-square Stop, tips "Allow once"/"Deny"/"Pause the run"/"Stop the run"; buttons(): Allow+Deny on a permission need, else PauseRun+StopRun when agents.runs > 0 and a run link exists, only when actions are allowed; press runs act() on a thread, the bubble's detail says "Sending to the tracker…" then the outcome (done text or refusal) for 6 s. Fix: watcher thread state writes (status, view, runs) go through Shared::state(stop), which checks the stop flag under the lock, so a stopped thread can't overwrite the new one (found by the actions test: after restart() the old thread's reset wrote Unreachable over Connected). Tests: tests/tracker.rs actions_post_the_trackers_routes_only_when_allowed (off → refused locally and nothing sent; on → allow, conflict, pause, stop, deny bodies and routes; tracker refusal text); wire a_task_names_its_run_and_its_prompt; lib a_task_bubble_offers_actions_only_where_the_tracker_allows_them. Workspace clippy clean; aipet-core and aipet-ui tests pass (tracker suite 3x).

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 4672a349a39aaab254d729ac57f88265775a64ba
- Tests: cargo test -p aipet-core; cargo test -p aipet-ui
- PRs: