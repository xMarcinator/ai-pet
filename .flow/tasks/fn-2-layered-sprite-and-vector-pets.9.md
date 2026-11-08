---
satisfies: [R15]
---
# fn-2-layered-sprite-and-vector-pets.9 Docs: authoring guide, architecture, README, changelog, notices

## Description
Document the new pet model: a new `docs/MAKING-A-PET.md` authoring guide, the pet section of `docs/ARCHITECTURE.md`, the README's Avatars text and data folder listing, the Settings help text, `CHANGELOG.md` under Unreleased, third-party notices for the new crates, and a sample pack plus a short README in `avatars/`.

**Size:** M
**Files:** `docs/MAKING-A-PET.md` (new), `docs/ARCHITECTURE.md`, `README.md`, `CHANGELOG.md`, `THIRD-PARTY-NOTICES.md`, `rust/crates/aipet-ui/src/settings/avatars.rs`, `avatars/README.md` (new)
**Touches:** [docs/**, README.md, CHANGELOG.md, THIRD-PARTY-NOTICES.md, rust/crates/aipet-ui/src/settings/avatars.rs, avatars/**]

### Approach
- The authoring guide covers the manifest (with the spec's example), layers and channels with every key, the three kinds, pixel size and the art box, Part motion (parts, origins, `look`, `poses`, keyframe animations; R18), Aseprite (native `.aseprite` files and `json-array` exports, tags, the `anchor` and part slices) grid sprite sheets (R20), that Rive packs aren't supported yet (R21 moved to fnt-67-rive-packs), the limits, `--check-pack`, legacy avatars and Reload.
- ARCHITECTURE: replace the procedural-renderer text (pipeline, layers, anchors, avatars) with the Pose → compositor → renderers model, keeping the numbered-section style.
- Notices: follow how fn-1 task 26 generates them (cargo-about).

### Investigation targets
**Required:**
- `docs/ARCHITECTURE.md` (the pet section as rewritten by fn-1 task 26)
- `README.md` — Settings → Avatars and the data folder listing
- `rust/crates/aipet-ui/src/settings/avatars.rs:130-135` — help text

### Key context
Keep CHANGELOG's keep-a-changelog style and 120-column wrapping.

## Acceptance
- [ ] `docs/MAKING-A-PET.md` exists and its manifest example passes `--check-pack` when saved as a pack
- [ ] ARCHITECTURE, README, help text and CHANGELOG describe layered packs; no text describes the removed renderer as current
- [ ] Third-party notices list the new crates

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
