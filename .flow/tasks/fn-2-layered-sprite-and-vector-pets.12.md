---
satisfies: [R19]
---
# fn-2-layered-sprite-and-vector-pets.12 Agent skill for making avatar packs

## Description
Write an agent skill so a new agent (Claude or Codex) can design, build, check and review an avatar pack without reading the Rust code (R19). It captures the workflow used for the green Hood and the Hamster, and the finished pack format.

**Size:** M
**Files:** `.claude/skills/aipet-avatar-pack/SKILL.md` (new), `.claude/skills/aipet-avatar-pack/references/*.md` (new), `.agents/skills/aipet-avatar-pack/**` (a copy for Codex), `avatars/samples/**` (the skill's worked example)
**Touches:** [.claude/skills/aipet-avatar-pack/**, .agents/skills/aipet-avatar-pack/**, avatars/samples/**]

### Approach
- Follow the shape of the repo's existing skill (`.claude/skills/flow-next-tracker/`): a short `SKILL.md` with a trigger description ("make / edit / check an AiPet avatar pack, pet or sprite"), plus reference files loaded only when needed.
- `SKILL.md`, the workflow:
  1. Pick the kind: pixel, painted, vector or Rive.
  2. Gather references from the user (their pixel art, sketches, images).
  3. Cover every state in `avatars/reference/pet_states.py` (`STATES` and `PHASES`); the generator only draws the pet's own parts per pose, while the shared module adds effects, the magnifier, headphones, whole-body motion, the shadow, mirroring, the review page and the state sheet.
  4. Draw with a generator modelled on `avatars/reference/hood_gen.py`, `hamster_gen.py` and `sprout_gen.py`: named parts at their origins, canvas-ready shapes only, poses by moving, rotating or swapping parts.
  5. Produce the review page (pixel original beside the vector; 130×120 boxes on white, mid-grey and near-black; every state) and the state sheet.
  6. Iterate with the user.
  7. Run a Codex review: `codex exec` read-only, with screenshots attached and the owner's decisions listed as fixed.
  8. Emit the pack manifest and run `AiPet --check-pack`.
  9. Add the pack to `avatars/` or the built-ins.
- One skill (the user's choice), with one reference file per technology, loaded only when that technology is used: `references/pixel-and-painted.md` (sheets, explicit and grid frames), `references/vector.md` (parts and generators), `references/aseprite.md` (native files and exports), `references/rive.md` (state machine, inputs, the manifest mapping). Plus shared references:
  - the full manifest with Part motion (R18) and an example per kind;
  - native Aseprite and export settings, and grid sheets (R20);
  - Rive packs: building the state machine and inputs a pet needs, and the manifest's input mapping (R21);
  - channels and every value with fallbacks;
  - the limits and every reason line `--check-pack` can print;
  - the owner's design conventions (cartoon style, thick outline, flat cel shading, rim light, no blur, filters, masks or gradients in vector packs; app-size readability on three backgrounds).
- Point to `docs/MAKING-A-PET.md` (task 9) for humans rather than duplicating it. The skill holds the agent-specific workflow and checks.
- Worked example: a tiny complete pack in `avatars/samples/` that the skill walks through, and that passes `--check-pack`.

### Investigation targets
**Required:**
- `.claude/skills/flow-next-tracker/SKILL.md` — the repo's skill format
- `avatars/reference/hood_gen.py`, `avatars/reference/hamster_gen.py` — the generator workflow to document
- `docs/MAKING-A-PET.md` (task 9) — the human guide to point at
- `.flow/specs/fn-2-layered-sprite-and-vector-pets.md` — API Contracts (manifest, Part motion, Aseprite, grid sheets)

### Key context
The copy under `.agents/skills/` keeps Codex in step; a test or script should check that the two copies are identical.

## Acceptance
- [ ] `.claude/skills/aipet-avatar-pack/` exists with `SKILL.md`, one reference per technology (pixel and painted, vector, Aseprite, Rive) and the shared references, covering the R19 list; `.agents/skills/aipet-avatar-pack/` is identical (checked by a test or script)
- [ ] The worked example in `avatars/samples/` passes `AiPet --check-pack`
- [ ] A fresh agent, given only the skill and a reference image, produces a pack that passes `--check-pack` without opening Rust files (one dry run recorded in the task's evidence)

## Done summary
Skill .claude/skills/aipet-avatar-pack (f0c3a90): SKILL.md workflow plus references per technology (pixel and painted, vector, Aseprite, Rive: not supported, FNT-67) and shared ones (manifest, channels, themes, limits and every reason line, design conventions, review, worked example); identical Codex copy in .agents/skills, checked by scripts/check-skill-copies.sh in CI; worked example avatars/samples/skill-example (Dewdrop) passes --check-pack. Dry run: a fresh agent given only the skill and avatars/reference/sprout-pixel.png made a vector 'Sprout (dry run)' that passed --check-pack on its first try (OK Sprout (dry run) (vector, 3 layers, 0 frames)), opening no Rust files; its six gaps were fixed in the skill.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits:
- Tests:
- PRs: