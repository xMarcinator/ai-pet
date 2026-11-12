---
satisfies: [R1]
---
# fnt-75-the-pet-window-without-the-gpu.10 Gate retry: the breathing cached in squash steps

## Description
The first gate (task 1, 2026-11-12) failed: the CPU stack's calm CPU was over the iced pet's in all eight cases (7.26-14.50 % against 6.40-7.13 %), because the pet's breathing changes the sprite's squash every frame and each frame re-samples the sprite and its halo bilinearly (about 85k device px at scale 2). Retry the gate once with that cost cached, on the spike branch `spike/ui-framework`, before deciding the spec. Throwaway code: only the proof lands.

**Size:** M
**Files:** `spikes/ui-framework/gate/src/scene.rs`, `spikes/ui-framework/gate.sh`, `rust/proofs/ui-framework.md`

### Approach
- Quantise the squash and stretch (scale_x, scale_y) to a step of half a device px of the sprite's box, and the hop offset to whole device px; cache each distinct sprite (and halo) already scaled, keyed by picks or vector key plus the quantised squash, rebuilt per output scale.
- Draw a cached sprite at an integer translate with no resampling (tiny-skia's identity path, or a hand-written premultiplied source-over blit).
- Check by eye, with the owner, that the stepped breathing looks the same as today's at scale 1, 1.5 and 2; if it doesn't, record that and stop.
- Re-run `gate.sh 2` and `gate.sh 1.5`, all eight cases, at the method's load limit 1.0 (or a raised one the owner accepts, recorded).

### Key context
- `hyprctl keyword` answers "unknown request" on Hyprland 0.57's Lua config; gate.sh's header has the `hyprctl eval 'hl.monitor(...)'` form.
- The CPU runs at low clocks between the pet's short bursts (`powersave`), so a live tick costs several times a headless one; measure live.

## Acceptance
- [ ] The stepped breathing looks like today's to the owner at scale 1, 1.5 and 2
- [ ] One paired run per case at scale 2 and 1.5, medians and samples recorded in `rust/proofs/ui-framework.md` with the load limit used
- [ ] The verdict is written in the proof: GO unblocks task 2; STOP leaves the spec for the owner's choice (keep iced, accept the CPU cost, or damage-tracked Slint)

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
