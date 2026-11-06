---
satisfies: [R21]
---
# fn-2-layered-sprite-and-vector-pets.14 Rive pack kind: input mapping, rendering, click-through, safe loading

## Description
Build the `rive` pack kind on task 13's renderer (spec: API Contracts → Rive packs; R21). Map Pose channels onto state-machine inputs; render each changed frame into the sprite path; take click-through from alpha; validate `.riv` files in a child process before loading them; report them in `--check-pack`. Ship one sample Rive pack.

**Size:** M
**Files:** `rust/crates/aipet-rive/**`, `rust/crates/aipet-sprite/src/load.rs`, `rust/crates/aipet-ui/src/sprite.rs`, `rust/crates/aipet/src/main.rs`, `avatars/samples/rive/**` (new)
**Touches:** [rust/crates/aipet-rive/**, rust/crates/aipet-sprite/src/**, rust/crates/aipet-ui/src/**, rust/crates/aipet/src/main.rs, avatars/samples/**]

### Approach
- Manifest: parse the `rive` block (file, artboard, state machine, inputs) with task 3's loader conventions and limits (4 MB per file, containment).
- Inputs: on each Pose change, set `number` inputs through their value maps, set `look.x`/`look.y` as −1..1, set `bool` inputs from `when`, and fire `trigger` inputs on entry or on the named events (poke, land, wave, alert). Enumerate the file's inputs once and report missing ones.
- Rendering: advance by elapsed time and render into the RGBA buffer, only when advancing reports change or an input changed. Hand the frame to iced like a raster frame (one handle per new frame). Reduced motion holds the current frame and stops advancing.
- Click-through: the hit runs come from the buffer's alpha (task 1's helper), recomputed only on a new frame.
- Safe loading: before loading a `.riv` in-process, run `AiPet --check-pack <folder>` as a child process with a time limit (a few seconds); load only after it succeeds. Reject referenced (non-embedded) assets. Cache the check result by file hash, so a startup with unchanged packs stays fast.
- Sample pack: an MIT-licensed example `.riv` from Rive's repositories with a manifest mapping its inputs (or a file the user provides). Record its source and licence.

### Investigation targets
**Required:**
- `rust/crates/aipet-rive/**` (task 13) — the binding and renderer
- `rust/crates/aipet-sprite/src/load.rs` (task 3) — loader conventions and limits
- `rust/crates/aipet/src/main.rs` — the `--check-pack` flag (task 3)

### Key context
The free Rive tier reportedly adds a splash to exported files. Check how such a file looks in the pet, and note the result in the authoring guide (task 9) and the skill (task 12).

## Acceptance
- [ ] A Rive pack loads and plays; Pose changes drive its inputs (tests with a sample file: a number input changes with the mood, look moves, a trigger fires on poke)
- [ ] No new frame and no upload while the state machine is idle and inputs are unchanged (test counts frames)
- [ ] Click-through follows the rendered alpha; mirroring, whole-body motion and reduced motion work
- [ ] Every R21 error case gives its reason and never crashes the pet: an oversize file, a malformed file (the child check fails and the pet keeps running), an unknown artboard or state machine, a referenced asset, a check timeout
- [ ] `AiPet --check-pack` reports the sample Rive pack OK and lists missing inputs for a mismatched manifest

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
