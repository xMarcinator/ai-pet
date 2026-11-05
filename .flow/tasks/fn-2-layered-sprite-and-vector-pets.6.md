---
satisfies: [R11]
---
# fn-2-layered-sprite-and-vector-pets.6 Hamster pixel pack from the user's reference image

## Description
Draw the Hamster as a layered pixel pack from the user's reference image: a chubby orange-and-cream pixel-art hamster with round ears, black eyes, pink cheeks, a cream belly and small feet, about 32×32 cells. Add every state, eye expression and look direction, arm and foot poses, laptop and lens props, headphones, effects (zs, dots, notes, alert, sparkles, dust) and glow. Embed it as a built-in.

**Size:** M
**Files:** `rust/crates/aipet-sprite/packs/hamster/**` (new), `rust/crates/aipet-sprite/src/pack.rs` (built-in list), `avatars/reference/hamster.png` (the user's reference, added by the user)
**Touches:** [rust/crates/aipet-sprite/packs/hamster/**, rust/crates/aipet-sprite/src/pack.rs, avatars/reference/**]

### Approach
- Author the sheets with a small script (a dev-only bake input), so they are reproducible and editable, or as hand-made PNGs. Keep cell size and palette consistent with the reference.
- Follow Sprout's baked pack (task 2) for the layer list and channel keys.
- Pick `pixel_size` so the art fits the 130×120 box.

### Investigation targets
**Required:**
- `rust/crates/aipet-sprite/packs/sprout/manifest.json` (from task 2) — layer and key conventions
- `avatars/reference/hamster.png` — the reference

### Key context
The reference image was shared in chat; the user saves it to `avatars/reference/hamster.png` before this task starts. Confirm the look with the user in the hands-on task.

## Acceptance
- [ ] The Hamster pack has every state, expression, look direction, prop, headphones and effect; `--check-pack` reports OK
- [ ] It is built in and selectable in Settings
- [ ] It matches the reference's proportions and palette (user confirms in the hands-on task)

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
