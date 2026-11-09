---
satisfies: [R8, R9]
---
# fnt-72-new-pet-states-and-the-hamsters.3 The Hamster's accessories and shading, vector and pixel

## Description
The Hamster's own look for the new states and its accessories, in both generators, plus the new shading (R9), drawn in our own style from the ideas only.

**Size:** M
**Files:** `avatars/reference/hamster_gen.py`, `avatars/reference/hamster_pixel_gen.py`, `rust/crates/aipet-sprite/packs/{hamster-vector,hamster-pixel}/**`, the regenerated Hamster pages
**Touches:** [avatars/reference/hamster_gen.py, avatars/reference/hamster_pixel_gen.py, avatars/reference/hamster*, rust/crates/aipet-sprite/packs/hamster-vector/**, rust/crates/aipet-sprite/packs/hamster-pixel/**]

### Approach
- Glasses: the prop layer's `lens` value draws round glasses on the face (`hamster_gen.py:299-304`, `hamster_pixel_gen.py:401 lens()`); drop the lens `scan_keys` from the prop, draw `lens` arms as reading paws (the behaviour forces `Arms::Lens`, `pet.rs:693`), keep the eye scan.
- Envelope in both paws at the belly; tears and yawn on the off-centre head (props follow the head: the memory note on the three-quarter view); aura; heart above the head for poke; speed lines inside the `dust` value; blush on done, aura and heart only.
- Shading: darker orange on the right side and bottom right; paws as small nubs on the belly. Replaces the "back in the head's yellow" decision (update `docs/HANDOFF.md` in task 6).
- Hold frames for tired and sad may use the long-hold rhythm; no blink frames in loops (the random blink stays).
- Never trace JLandbo's pixels; work from the descriptions in the spec's Resolved via Research.

### Investigation targets
**Required:**
- `avatars/reference/hamster_gen.py` - the vector Hamster, its head offset and props
- `avatars/reference/hamster_pixel_gen.py:380-420` - pixel lens and props
**Optional:**
- `avatars/reference/hamster.png` - the reference the Hamster was drawn from

### Key context
- `hamster_pixel_gen.py`'s docstring says `packs/hamster/`; the folder is `hamster-pixel`.

## Acceptance
- [ ] Both Hamster pages show glasses for reading, the envelope, tears, yawn, aura, heart, speed lines, blush only on done/aura/heart, and the new shading
- [ ] `cargo test -p aipet-sprite` passes
- [ ] The owner reviews the Hamster pages (screenshot or the page) and approves

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
