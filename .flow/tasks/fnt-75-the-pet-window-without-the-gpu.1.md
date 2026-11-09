---
satisfies: [R1]
---
# fnt-75-the-pet-window-without-the-gpu.1 Gate: the CPU stack against iced at the real window's worst case

## Description
Grow FNT-73's minimal prototype into the gate R1 defines and run it paired with the release iced pet. Throwaway code: it stays on the spike branch; only the proof lands. Stops the spec if any case fails.

**Size:** M
**Files:** `spikes/ui-framework/common/src/lib.rs`, `spikes/ui-framework/minimal/src/main.rs`, `spikes/ui-framework/calm.sh`, `rust/proofs/ui-framework.md`
**Touches:** [spikes/ui-framework/**, rust/proofs/ui-framework.md]

### Approach
- Work on the spike branch (`worktree-agent-af0c406e01f20bf2e`, commit df8c5f6; rename it `spike/ui-framework` as the proof asks).
- Scene at 380x600 (`aipet-ui/src/geometry.rs` SURFACE) with three cards of long text drawn like `view.rs` cards (shadow, title, detail), the pet idle and working (laptop + screen glow), Sprout (pixel) and Dewdrop (`avatars/samples/skill-example`, vector via `aipet-sprite` `vector::render`).
- Damage tracking: the scene returns changed rects per frame; keep two `SlotPool` buffers and repaint the union of the last two frames' rects; `damage_buffer` only those; no frame callback and no commit when nothing changed.
- Scale 2 via `buffer_scale` and 1.5 via wp_fractional_scale_v1 + wp_viewporter (buffer `round(logical x scale)`), pixel art at whole device px per art px.
- Text: cosmic-text glyph masks blitted into the pixmap (not `Buffer::draw`, which is per pixel).
- Same scale on both sides: run each case with the output's scale set for both processes (on Hyprland, `hyprctl keyword monitor` to scale 2, then 1.5, restored after), so the iced pet and the gate draw the same physical buffer.
- Load: the method's limit is 1.0. A run at a raised limit is provisional: it is recorded, but the gate opens only on a run at 1.0, or when the owner accepts the raised run in writing (recorded in the proof).
- Measure: a paired script (extend `calm.sh` or adapt `scripts/perf/pet-bench.sh`'s pairing) running the gate binary and the release iced pet configured for the same case (`AIPET_DEMO`-style cards or a fixed board), fn-1's settle and samples, load limit 1.0 (record any raise).

### Investigation targets
**Required:**
- `rust/proofs/ui-framework.md` - the spike, its numbers and the recommendation
- spike branch `spikes/ui-framework/minimal/src/main.rs:195-256` - tick, draw, full-surface damage today
- `scripts/perf/pet-bench.sh` - the paired method
- `rust/crates/aipet-ui/src/view.rs:101-265` - what a card draws

### Key context
- sctk 0.21 replaced the `delegate_*!` macros with `delegate_dispatch2!`; write against its examples.
- A busy wl_buffer (not yet released) must never be drawn into: `SlotPool::canvas` returns None for it.
- The iced pet's figure must come from the same run (calm CPU only compares within one run).

## Acceptance
- [ ] The gate binary draws the R1 cases with damage tracking at scale 2 and 1.5
- [ ] One paired run per case against the iced pet, medians and samples recorded in `rust/proofs/ui-framework.md` with the load limit used
- [ ] The verdict (go or stop) is written in the proof; on stop, the spec's remaining tasks are not started

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
