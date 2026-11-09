---
satisfies: [R27]
---
# fn-2-layered-sprite-and-vector-pets.24 The Hamster turns the way it is dragged (pack 'faces')

## Description
The Hamster is drawn turned to the left, so it faced away from the way it was dragged (owner, 2026-11-09). Add a pack `faces` setting (R27) and set it on both Hamsters.

**Size:** S
**Files:** `rust/crates/aipet-sprite/src/{pack,manifest,compose,load}.rs`, `rust/crates/aipet-ui/src/lib.rs`, `avatars/reference/hamster_gen.py`, `avatars/reference/hamster_pixel_gen.py`, both Hamster packs, `docs/MAKING-A-PET.md`, both skill copies
**Touches:** [rust/crates/aipet-sprite/**, rust/crates/aipet-ui/src/lib.rs, avatars/reference/hamster_gen.py, avatars/reference/hamster_pixel_gen.py, docs/MAKING-A-PET.md, .claude/skills/aipet-avatar-pack/**, .agents/skills/aipet-avatar-pack/**]

## Acceptance
- [ ] A pack with faces left mirrors its non-effect layers when facing right, and its effect layers when facing left
- [ ] faces is read, written only when left, and a bad value is refused
- [ ] Both Hamsters face the way they are dragged in a compositor render

## Done summary
Pack 'faces' (right by default, written only when left): non-effect layers mirror when the pet faces away from the art's way; effect layers keep the right-facing convention. Both Hamsters say faces left; the pixel one now mirrors whole (drawn in perspective). The pet starts facing as drawn. Checked: pack_sheet render of the vector Hamster running both ways; workspace tests pass. Not checked: dragging in the running pet.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 322439f
- Tests: cargo test --workspace
- PRs: