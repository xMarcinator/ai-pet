---
satisfies: [R7, R8]
---
# fnt-72-new-pet-states-and-the-hamsters.2 Shared effects and the new states for Hood, Sprout (vector) and Pip

## Description
Draw the shared pieces once in `pet_states.py` (tears, aura, heart, the envelope prop, the new STATES rows) and give each vector generator its sleepy eyes and yawn, then regenerate the vector Hood, Sprout and Pip packs, their pages, and the painted Pip.

**Size:** M
**Files:** `avatars/reference/pet_states.py`, `avatars/reference/vector_pack.py`, `avatars/reference/{hood_gen,sprout_gen,pip_gen}.py`, `rust/crates/aipet-sprite/packs/{hood-vector,sprout-vector,pip,pip-painted}/**`, the regenerated pages in `avatars/reference/`
**Touches:** [avatars/reference/pet_states.py, avatars/reference/vector_pack.py, avatars/reference/hood_gen.py, avatars/reference/sprout_gen.py, avatars/reference/pip_gen.py, avatars/reference/hood-green*, avatars/reference/sprout*, avatars/reference/pip*, rust/crates/aipet-sprite/packs/hood-vector/**, rust/crates/aipet-sprite/packs/sprout-vector/**, rust/crates/aipet-sprite/packs/pip/**, rust/crates/aipet-sprite/packs/pip-painted/**]

### Approach
- Add rows to `STATES` (`pet_states.py:22`) for sad, tired, news, done's aura and the poke heart, with poses using the new values; extend `effects()` (`:107`) and `vector_pack.FX`/`FX_VALUES` (`:59-60`) and `effect_layers` (`:527`) with `tears`, `aura`, `heart`, and add an `envelope()` prop beside `magnifier()` (`:77`).
- Fixed colours: tear blue, aura gold, blush pink as module constants beside the pet.rs fixed colours (`pet_states.py:14-16`); the envelope's seal takes the accent.
- First frame or key carries each cue (R7): tear already on the cheek, aura fully built, yawn open.
- Each generator's `eyes()` draws a `sleepy` expression and a yawn mouth state.
- Regenerate: each `*_gen.py` and `--pack`; painted Pip via the existing paint path (`rust/crates/aipet-sprite/tests/bake.rs`, `AIPET_BAKE=1`).
- Hood drawing comes after fn-2.22 (laces, done) and fn-2.21 (effects vs bubbles); follow 21's placement rule.

### Investigation targets
**Required:**
- `avatars/reference/pet_states.py:14-180`
- `avatars/reference/vector_pack.py:41-60,527-575`
- `rust/crates/aipet-sprite/src/painted.rs` - how painted Pip follows the vector Pip
**Optional:**
- `.flow/tasks/fn-2-layered-sprite-and-vector-pets.21.md` - the effects placement rule

### Key context
- `glow` is reserved (pack colour, layer halo; fx layers may not glow): the effect is `aura`, drawn art.

## Acceptance
- [ ] The Hood, Sprout and Pip pages and sheets show sad, tired, news, the aura and the heart
- [ ] `cargo test -p aipet-sprite` passes (committed packs are the generators' output)
- [ ] Each new state's first frame or key shows its cue

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
