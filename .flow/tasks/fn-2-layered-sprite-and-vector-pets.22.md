---
satisfies: [R25]
---
# fn-2-layered-sprite-and-vector-pets.22 Vector Hood: two drawstrings, and only the laces and hands mirror

## Description
The vector Hood's single cloth fold on the robe reads as one drawstring; a hood has two. Replace it with two drawstrings hanging from the hood opening, one each side of the face, on a layer of their own. Today the pack's `"mirror": "flip"` flips the whole pet when it faces left; set the pack to `none` and mirror only the laces, the hands and the dust, as the pixel Hood mirrors only its dust (R25).

**Size:** S
**Files:** `avatars/reference/hood_gen.py`, `rust/crates/aipet-sprite/packs/hood-vector/pack.json`, `avatars/reference/hood-green*.{svg,html}`
**Touches:** [avatars/reference/hood_gen.py, avatars/reference/hood-green*, rust/crates/aipet-sprite/packs/hood-vector/**]

### Approach
- Remove the fold path in `figure()` (`avatars/reference/hood_gen.py:163`); draw the laces as their own `laces` group so `vp.Drawing.art` reads them.
- In `pack()`, a `laces` layer on the `state` channel with `"mirror": "flip"`, the hand layers flipped, the dust effect layer flipped; pack `mirror: none` (per-layer mirroring as in `rust/crates/aipet-sprite/packs/hood/pack.json`).
- The eyes need no mirroring: running sets the look from the facing (`rust/crates/aipet-sprite/src/pet.rs:494-496`).
- Regenerate with `python avatars/reference/hood_gen.py` and `--pack`; `cargo test -p aipet-sprite --test vector_packs`.

### Key context
- Only dragging makes the pet face left, so the laptop and magnifier states never mirror; still check them on the review page.

## Acceptance
- [ ] The review page and sheet show two drawstrings in every state, the laptop and the magnifier included
- [ ] `hood-vector/pack.json` has pack mirror `none`, and only the laces, hands and dust layers flip
- [ ] `cargo test -p aipet-sprite` passes (vector_packs: the committed pack is the generator's)
- [ ] Dragged left in the running pet: the body stays unmirrored, the laces and hands mirror, the eyes look left

## Done summary
The vector Hood wears two drawstrings, one from each side of the hood opening, on their own laces layer; the robe's single fold is gone. The pack's mirror is none; the laces, both hands and the dust flip, so facing left no longer mirrors the body. Checked: the review sheet (every state, laptop and magnifier included) and pack_sheet through the compositor (running left: dust behind, hands mirrored, body unchanged). Not checked: the eyes' look while dragged left in the running pet (hands-on, with fn-2.8).

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 30bfc47
- Tests: cargo test -p aipet-sprite (87+5+5+2 passed)
- PRs: