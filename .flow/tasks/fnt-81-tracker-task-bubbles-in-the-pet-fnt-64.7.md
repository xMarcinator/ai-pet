---
satisfies: [R7]
---
# fnt-81-tracker-task-bubbles-in-the-pet-fnt-64.7 Docs, notices and a hands-on pass with a real tracker

## Description
README (Tracker section, privacy), ARCHITECTURE (the watcher, protocol v2, the Board's task bubbles), CHANGELOG; THIRD-PARTY-NOTICES regenerated; hands-on against flow-next-tracker built from FNT-64's branch: enrol, bubbles through a progress change and a permission prompt, answer, revoke.

**Size:** S

## Acceptance
- [ ] docs updated
- [ ] notices check passes
- [ ] hands-on recorded in rust/proofs/tracker.md

## Done summary
Docs, notices and the real-tracker check on branch fnt-81-tracker (8ee1d8f). README: tracker bubbles, their Allow once/Deny/Pause/Stop, Settings › Tracker, tracker.json and the credential in Privacy, the tracker in Network (no link: the tracker repo is private). ARCHITECTURE: §4.5 actions and body click, §5.2 step 4 tracker tasks (steps renumbered), error kinds, §8.4 AiPet:Tracker, new §8.7, Appendix A row. CHANGELOG milestone 3 Added. Notices: 382 crates, checked. Real tracker: FNT-64 build (fnt-31 branch at 355294f5) as serve --port 3299 in scratch with --secrets memory, stopped after; tests/tracker_real.rs (ignored) ran the real TrackerWatcher: enrol (401 without token), snapshot read as "FNT-90 · Pet check over the companion feed / Reviewing · round 2 / 1/2 / 1 agent", feed change to Fixing, revoke → feed closed with the policy reason → Revoked, old token 401; actions switch flipped while connected → seen; Stop of a missing run → "run not found" (scope passed). Fix from the run: hello is read again every LOOKUP_EVERY (10 s) while connected (tests/tracker.rs no longer restarts to see it). Not checked, recorded in rust/proofs/tracker.md: a hosted tracker's real host (fake only), a real permission prompt answered, a managed run's chat collapsing, and the visual look of the bubble and the Settings page (needs the owner's GUI run).

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 8ee1d8fa760a03b52acb66b7974303b0b6074018
- Tests: cargo test -p aipet-core --test tracker_real -- --ignored (against serve --port 3299)
- PRs: