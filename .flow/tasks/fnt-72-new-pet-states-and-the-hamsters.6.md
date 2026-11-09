---
satisfies: [R11]
---
# fnt-72-new-pet-states-and-the-hamsters.6 Docs, the Hamster's credit, and the sample pack

## Description
Credit JLandbo's hamster-pet as the Hamster's origin, and describe the new states, values, fallbacks and the error chat everywhere packs and behaviour are documented.

**Size:** M
**Files:** `README.md`, `avatars/README.md`, `THIRD-PARTY-NOTICES.md`, `docs/MAKING-A-PET.md`, `docs/ARCHITECTURE.md`, `docs/HANDOFF.md`, `CHANGELOG.md`, `.claude/skills/aipet-avatar-pack/**`, `.agents/skills/aipet-avatar-pack/**`, `avatars/samples/skill_example_gen.py` and its preview
**Touches:** [README.md, avatars/README.md, THIRD-PARTY-NOTICES.md, docs/**, CHANGELOG.md, .claude/skills/aipet-avatar-pack/**, .agents/skills/aipet-avatar-pack/**, avatars/samples/**]

### Approach
- Credit: a line under THIRD-PARTY-NOTICES "The Rust pet's packs" and in `avatars/README.md`: the Hamster's design and the new states' ideas come from https://github.com/JLandbo/hamster-pet, used with its author's permission; README's pet section links it.
- MAKING-A-PET channel table (lines 49-58) and fallback examples (72-73, 160-161): the new values; say `lens` art is the pack's (glasses or magnifier). ARCHITECTURE §3.2, §3.3 Mood table, the line-82 parity claim (qualify it), §5.3 (the error chat).
- Skill (both copies, identical; `diff -r` clean): `references/channels.md`, `design-conventions.md`, `SKILL.md`, `review.md`.
- Dewdrop sample: add the new values to `EFFECTS` (`skill_example_gen.py:93`) and regenerate, so "fallbacks for every state" stays true.
- HANDOFF line 42: the Hamster's new shading decision. CHANGELOG `[Unreleased]` Added: the four states and the Hamster's look.
- fnt-71's task 8 edits the same docs: whichever lands second rebases.

### Investigation targets
**Required:**
- `docs/MAKING-A-PET.md:45-80,150-170,480-495`
- `docs/ARCHITECTURE.md` §3.2-3.3, §5.3
- `.claude/skills/aipet-avatar-pack/references/channels.md`

## Acceptance
- [ ] The credit appears in README, avatars/README.md and THIRD-PARTY-NOTICES
- [ ] Every doc in R11 updated; the two skill copies are identical
- [ ] The Dewdrop sample regenerates with the new values and its test passes

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
