---
satisfies: [R4]
---
# fnt-81-tracker-task-bubbles-in-the-pet-fnt-64.4 UI: task bubble and Settings › Tracker

## Description
aipet-ui: task bubble drawing (short id + title, phase word, subtasks strip, acceptance n/m, agent badge, amber/red), click opens links.task with the system opener; Settings › Tracker page (address, code, Enroll, status, sessions without a task, Forget).

**Size:** M

## Acceptance
- [ ] the bubble draws every field and both tones (screenshot in the proof)
- [ ] click opens fnt:// through the opener
- [ ] enrol, status and forget work from Settings; a failure shows on the page

## Done summary
UI on branch fnt-81-tracker (68978af). cards.rs: Bubble/Card.progress (Progress {done, total, agents}). lib.rs fill(): a task bubble gets app ("Tracker", board::TRACKER_TEAL) for its border (the app label never prefixes a task's detail), " · n/m criteria" after the detail when the task has acceptance criteria, and its Progress; open_item: tracker-error opens Page::Tracker; a task's click opens its link (fnt://) through the platform opener as chats' deep links do. view.rs progress_marks(): a 2.5 px strip 5 px above the bottom from the text's start to the rounded end (track 0x33FFFFFF, fill the dot's colour), and an agent badge (13 px, count up to 99) at the dot's top right when agents > 1. settings/tracker.rs: Page::Tracker (5th page) with address (default http://127.0.0.1:3210), write-only one-time code (emptied on Enroll), Enroll on a thread (status Enrolling… then the watcher's status or the refusal), Forget, "Hide chats that aren't tracker tasks while the tracker is connected" saved at once, the status line from the watcher (Off/Connecting/Connected with the task count and whether actions are on/Unreachable/Revoked), and the privacy line. Services.tracker; core_thread Core::tracker(); main.rs passes it. Tests: lib a_tracker_task_is_a_bubble_with_its_counts (collapse, border, title, detail, progress, error click → Tracker page); tests/settings.rs the_tracker_page_enrolls_says_why_not_and_saves_the_hidden_chats_choice. Workspace clippy clean; aipet-ui and aipet tests pass. Not done here: a screenshot of the bubble (no GUI started without the owner; the demo's script is too tightly pinned by its tests to carry a task) — the visual check is in task 7's hands-on pass.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 68978af7c98a82135cfe777aa70cc34e625f1f0e
- Tests: cargo test -p aipet-ui; cargo test -p aipet --bin AiPet
- PRs: