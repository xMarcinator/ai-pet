---
satisfies: [R26]
---
# fn-2-layered-sprite-and-vector-pets.23 Pets drawn facing the viewer don't mirror their body

## Description
Facing left, the front-facing vector packs flipped their body, so Pip's sprout and the Hood's laces jumped sides. Set those packs' mirror to none and flip only hands or arms, feet and dust; document the rule (R26).

**Size:** S
**Files:** `avatars/reference/{hood,sprout,pip}_gen.py`, `rust/crates/aipet-sprite/packs/{hood-vector,sprout-vector,pip,pip-painted}/pack.json`, `docs/MAKING-A-PET.md`, both copies of the skill's `references/design-conventions.md`
**Touches:** [avatars/reference/hood_gen.py, avatars/reference/sprout_gen.py, avatars/reference/pip_gen.py, rust/crates/aipet-sprite/packs/**, docs/MAKING-A-PET.md, .claude/skills/aipet-avatar-pack/**, .agents/skills/aipet-avatar-pack/**]

## Acceptance
- [ ] Those packs have mirror none; only arms or hands, feet and dust layers flip
- [ ] `cargo test -p aipet-sprite` passes (generators and the painted bake match)
- [ ] MAKING-A-PET and both skill copies state the rule

## Done summary
The vector Hood (laces no longer flip), Sprout (vector), Pip and painted Pip now have mirror none and flip only hands or arms, feet and dust; the vector Hamster, drawn in three-quarter view, still mirrors whole. Checked: pack_sheet render of Pip facing left (the sprout stays, hands and dust follow); cargo test -p aipet-sprite passes after the painted rebake. Not checked: dragging in the running pet.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 09667a1
- Tests: cargo test -p aipet-sprite
- PRs: