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
  3. Draw with a generator modelled on `avatars/reference/hood_gen.py` and `hamster_gen.py`: named parts at their origins, canvas-ready shapes only, poses by moving, rotating or swapping parts.
  4. Produce the review page (pixel original beside the vector; 130×120 boxes on white, mid-grey and near-black; every state) and the state sheet.
  5. Iterate with the user.
  6. Run a Codex review: `codex exec` read-only, with screenshots attached and the owner's decisions listed as fixed.
  7. Emit the pack manifest and run `AiPet --check-pack`.
  8. Add the pack to `avatars/` or the built-ins.
- References:
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
- [ ] `.claude/skills/aipet-avatar-pack/` exists with `SKILL.md` and references covering the R19 list; `.agents/skills/aipet-avatar-pack/` is identical (checked by a test or script)
- [ ] The worked example in `avatars/samples/` passes `AiPet --check-pack`
- [ ] A fresh agent, given only the skill and a reference image, produces a pack that passes `--check-pack` without opening Rust files (one dry run recorded in the task's evidence)

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
