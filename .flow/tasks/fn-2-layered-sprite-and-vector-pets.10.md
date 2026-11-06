---
satisfies: [R16, R18, R22]
---
# fn-2-layered-sprite-and-vector-pets.10 Vector Hood (green) and vector Hamster packs from the approved reference drawings

## Description
Turn the three approved reference drawings (Hood (green), Hamster and Sprout, R16 and R22) into built-in vector packs, using the vector kind that task 5 builds. The designs are settled: they were iterated with the user and reviewed by Codex (Hood READY; Hamster NEEDS_WORK resolved). This task converts them and adds the missing parts. It doesn't redesign them.

**Size:** M
**Files:** `rust/crates/aipet-sprite/packs/{hood-green-vector,hamster-vector,sprout-vector}/**` (new), `rust/crates/aipet-sprite/src/pack.rs` (built-in list), `avatars/reference/*_gen.py` (an export path, if converting from the generators)
**Touches:** [rust/crates/aipet-sprite/packs/**, rust/crates/aipet-sprite/src/pack.rs, avatars/reference/**]

### Approach
- Convert from the generators (`avatars/reference/hood_gen.py`, `hamster_gen.py`) rather than parsing the SVGs. They already name every part, origin and pose; add an option that writes the task-5 manifest. Keep the generated manifest reproducible: a test regenerates it and diffs it.
- Apply Codex's deferred conversion items:
  - normalise coordinates to the pack's art box: the 260×250 drawing scaled by about 0.48, so x′=x/2, y′=y/2−2.5, with the anchor near (65,118.5);
  - drop the `shadow` group, because the app owns the ground shadow;
  - turn rects, circles and ellipses into path `d` data, with explicit opacity, stroke width, round caps and joins;
  - map the Hood's preview-only "glow off" to a real Pose value (for example, dim the glow when asleep) or drop it.
- Map the parts to Pose channels:
  - eyes: the look offsets and every expression;
  - Hood hands and Hamster arms: their poses, where Hamster arms rotate about their shoulder pivots and lifted arms add the (±16,−8) shift;
  - the prop slot: the laptop.
- Add the missing parts in each pet's style: the magnifier prop, headphones, and effects (zs, dots, notes, alert, sparkles, dust). Effects are pack layers (spec: packs draw their own effects).
- Mirroring: flip about the art box's centre. The Hamster's ¾ turn flips with it, which is intended.
- Owner decisions to keep: the Hood has no peak, a short robe, tall eyes, and hands that rest in front of the robe and float out only to wave or raise. The Hamster's back is the head's yellow, with orange only as an edge shadow and no brown patch.

### Investigation targets
**Required:**
- `avatars/reference/hood-green.svg`, `avatars/reference/hamster.svg` — the approved drawings (part names and draw order are in their header comments)
- `avatars/reference/hood_gen.py`, `avatars/reference/hamster_gen.py` — the generators
- `avatars/reference/hood-green-preview.html`, `avatars/reference/hamster-preview.html` — the approved states

### Key context
The preview pages show the reference pixel art beside each vector, and 130×120 boxes on three backgrounds. Use them to check the converted packs look the same in the app.

## Acceptance
- [ ] Every built-in vector pack covers every state and second frame in `pet_states.STATES` and `PHASES`, with the R22 part ids; a pack test fails on a missing state (R22)
- [ ] Hood (green), Hamster and Sprout vector packs are built in, generated reproducibly from the reference generators, and `--check-pack` reports OK for both
- [ ] Rendered in the app, every state shown on the review pages matches its drawing (screenshots side by side)
- [ ] Both packs have every state, including the magnifier, headphones and the effects
- [ ] No ground shadow in the packs; coordinates sit in the art box with the anchor on the ground
- [ ] The generators emit the Part motion tables (R18) from their existing numbers: eye origins and look offsets, hand poses (rest, wave, raised, type) and the typing keyframes, the hand bob with phases, the Hamster's shoulder rotations, and the caret's opacity loop

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
