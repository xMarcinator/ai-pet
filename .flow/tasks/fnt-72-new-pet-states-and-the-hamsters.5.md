---
satisfies: [R10]
---
# fnt-72-new-pet-states-and-the-hamsters.5 Spike: the Hamster's turnaround

## Description
Findings only, no shipped art: can a 24-frame front-side-back-curl turnaround be expressed for the pixel and the vector Hamster, which state uses it, how a cycle finishes before a mood change, and what it costs. Ends with proposed follow-up tasks.

**Size:** S
**Files:** `rust/proofs/hamster-turnaround.md` (new), scratch drawings outside the packs
**Touches:** [rust/proofs/hamster-turnaround.md]

### Approach
- Pixel: raster frames take per-frame `ms` (`pack.rs:96`, `manifest.rs:707-767`); 24 frames fit `MAX_FRAMES` 512 and a 2048 px sheet. Draw 3-4 scratch frames (front, side, back) to cost them.
- Vector: part keys scale uniformly only (`pack.rs:167`); whole-body `scale_x`/`scale_y` exists (`compose.rs:203`). So back, side and both 3/4 views must be drawn shapes (the head is asymmetric). Say how the layered eyes, arms and props hide during the turn (null fallbacks).
- Cycle hold: `Tracker::pose` (`pose.rs:523`) restarts a channel on every value change and nothing holds a mood to a loop's end. Propose the smallest mechanism (a pack-declared hold-to-end flag, with dangle, run and poke interrupting).
- Recommend the state (likely an occasional idle act, since busy carries the laptop or glasses).

### Investigation targets
**Required:**
- `rust/crates/aipet-sprite/src/pose.rs:500-560`
- `rust/crates/aipet-sprite/src/pack.rs:90-210`

### Key context
- Never trace JLandbo's spin frames.

## Acceptance
- [ ] `rust/proofs/hamster-turnaround.md` answers expressibility per kind, the state, the cycle-hold mechanism and the cost
- [ ] It lists proposed follow-up tasks with sizes

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
