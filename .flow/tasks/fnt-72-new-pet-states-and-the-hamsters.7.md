---
satisfies: [R12]
---
# fnt-72-new-pet-states-and-the-hamsters.7 Prototype the aura beside today's sparkles, for the owner's approval

## Description
Before any pet draws the aura that replaces done's opening sparkles, show the owner a prototype (R12). One standalone page with the aura drawn for the vector Hood and the vector Hamster, beside today's done (sparkles), animated with the aura's one-time play (about 1.8 s, then sparkles) and as a still first frame, on light, dark and the desktop backgrounds. Offer two or three variants (for example a soft ring, a halo with rays, a glow pulse with sparkle stars) so the owner can pick.

**Size:** S
**Files:** `avatars/reference/aura_prototype.py` (new), `avatars/reference/aura-prototype.html` (generated)
**Touches:** [avatars/reference/aura_prototype.py, avatars/reference/aura-prototype.html]

### Approach
- Reuse `pet_states.compose()` and each generator's `frame()` (`avatars/reference/hood_gen.py`, `hamster_gen.py`) to draw the pets in done, and draw the aura variants as plain shapes (canvas-ready: no filters or gradients), gold as a fixed colour.
- Write the page like `pet_states.write_pages` (`avatars/reference/pet_states.py:171`).
- Show it to the owner (send the page, or publish it as a private page if they prefer) and record the chosen variant and any changes in this task's done summary.

### Key context
- Nothing in the packs changes in this task; task 2 draws the approved aura.

## Acceptance
- [ ] The prototype page shows each variant beside today's sparkles, animated and still, on three backgrounds, for the Hood and the Hamster
- [ ] The owner's choice (or requested changes, shown again) is recorded in the done summary

## Done summary
Owner chose variant B, the soft glow (2026-11-09): three stacked gold ellipses behind the pet (opacity 0.10/0.16/0.24, the largest 22 units past the body) with three small gold-highlight stars above the head, building in and fading over about 1.8 s, then today's sparkles. Drawn by avatars/reference/aura_prototype.py glow(); task 2 draws it for every pet.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 6d73a1e
- Tests:
- PRs: