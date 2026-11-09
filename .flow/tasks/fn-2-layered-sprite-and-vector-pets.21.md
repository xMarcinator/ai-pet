---
satisfies: [R24]
---
# fn-2-layered-sprite-and-vector-pets.21 Pet effects stay clear of the chat bubbles at every pet size

## Description
The pet's effects (thinking dots, Zs, notes, "!", sparkles, and their outlines and glows) are drawn over the chat bubbles. The owner's screenshot (2026-11-09, Windows, Hood (vector) shrunk) showed the thinking dots and dark fragments of their outlines on a "Thinking…" bubble's right edge (R24). The bubble stacks start just above the pet's art box (`geometry::stacks_bottom`, which moves with the pet size), while the effects sit in that box's top-right corner, so they meet. Shrinking the pet moves the bubbles down with it and makes it worse.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/geometry.rs`, `rust/crates/aipet-ui/src/lib.rs`, `rust/crates/aipet-ui/src/view.rs`, possibly the built-in packs' fx layers (`rust/crates/aipet-sprite/packs/*/pack.json`) and `avatars/reference/pet_states.py`
**Touches:** [rust/crates/aipet-ui/src/**, rust/crates/aipet-sprite/packs/**, avatars/reference/**]

### Approach
- Measure first: the fx layer's bounds per built-in pack (vector and raster), scaled by every allowed pet size, against `stacks_bottom(size)` and the stacks' right edge at every bubble size.
- Then pick the smallest fix:
  - lift `stacks_bottom` above the highest effect of the worn pack (computed from the pack's fx bounds at load, so custom packs are covered too), or
  - move the shared effects down and inward in the packs and in `pet_states.py`, regenerating the reference drawings.
  Prefer the first, as it covers user packs.
- Find the "dark fragments": check whether they are the dots' outlines peeking out under the bubble's rounded edge, or stale pixels from the region or halo not being redrawn after a size change (`Sprite::update` with the new scale, `drawn_rects`). Fix whichever it is.

### Investigation targets
**Required:**
- `rust/crates/aipet-ui/src/geometry.rs:9-61` — `SURFACE`, `SPRITE`, `HALO`, `STACKS_BOTTOM`, `stacks_bottom`
- `rust/crates/aipet-ui/src/lib.rs:1018-1035` — `sizes`, `pet_factor`, `sized`
- `rust/crates/aipet-ui/src/view.rs` — where the sprite, its effects and the stacks are layered

### Key context
Ignore any text behind the window in screenshots; it's the desktop.

## Acceptance
- [ ] A test checks, for every built-in pack and every allowed pet and bubble size, that the effects' scaled bounds stay out of the stacks' area
- [ ] A custom pack whose effects reach higher still doesn't draw over the bubbles (the stacks move, or the rule applies to it too)
- [ ] No fragments around a bubble after shrinking or growing the pet (hands-on on Windows, screenshots at the smallest, default and largest pet sizes, while thinking); recorded in task 8's proof

## Done summary
Wearing a pet measures how high its effects get (fx_top per fx value, lifted by fx_lift, the most its mood bounces; a test holds compute_pose to fx_lift), and the bubble stacks start that far higher at every pet size (geometry::stacks_bottom(size, reach)). It was every pet, not just the vector Hood: the '!' and done's sparkles bounce 15 px; the vector Hood's effects also sit 8-11 px above its box. Measured reach: pixel Sprout/Hood -15, vector Hood -26, Hamster -12, Sprout (vector) -16, pixel Hamster -9, Pip 0 px; about 17 ms per vector pack on wear. Trade-off: the stacks sit up to 26 px higher (52 at 200%) on the vector Hood. Not checked: the hands-on screenshots for fragments (Windows, with fn-2.8).

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: b6c000a, ed0ab02
- Tests: cargo test -p aipet-ui (every_built_in_pets_effects_stay_below_the_bubbles_at_every_size), cargo test -p aipet-sprite (the_body_never_lifts_an_effect_further_than_fx_lift_says)
- PRs: