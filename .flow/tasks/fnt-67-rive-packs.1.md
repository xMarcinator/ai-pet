---
satisfies: [R1]
---
# fnt-67-rive-packs.1 Rive pack kind: input mapping, rendering, click-through, safe loading

## Description
**Blocked:** task fn-2-layered-sprite-and-vector-pets.13's spike is a no-go (`rust/proofs/rive.md`), and R21 is deferred. The CPU renderer misses fn-1's calm budget (about 0.8 ms per frame, 3.4–11 % of a core at 30 Hz for a pet that keeps animating). rive-rs builds only without text and layout, and damaged files crash or hang in `File::new`. Don't start this task until the spec's unblock condition holds. If R21 comes back, carry over from the spike: a renderer that never returns `None` from `Image::decode`; an inspection shim (`FileAssetLoader` plus a no-op factory) for names, referenced assets and view models; a child check that also plays frames, with a time limit, on the exact bytes that will load; static libstdc++ on Linux; CI that clones rive-rs at a pinned revision with submodules.

Build the `rive` pack kind on the renderer from fn-2's Rive spike (task fn-2-layered-sprite-and-vector-pets.13) (spec: API Contracts → Rive packs; R1). Map Pose channels onto state-machine inputs; render each changed frame into the sprite path; take click-through from alpha; validate `.riv` files in a child process before loading them; report them in `--check-pack`. Ship one sample Rive pack.

**Size:** M
**Files:** `rust/crates/aipet-rive/**`, `rust/crates/aipet-sprite/src/load.rs`, `rust/crates/aipet-ui/src/sprite.rs`, `rust/crates/aipet/src/main.rs`, `avatars/samples/rive/**` (new)
**Touches:** [rust/crates/aipet-rive/**, rust/crates/aipet-sprite/src/**, rust/crates/aipet-ui/src/**, rust/crates/aipet/src/main.rs, avatars/samples/**]

### Approach
- Manifest: parse the `rive` block (file, artboard, state machine, inputs) with fn-2's loader conventions and limits (4 MB per file, containment).
- Inputs: on each Pose change, set `number` inputs through their value maps, set `look.x`/`look.y` as −1..1, set `bool` inputs from `when`, and fire `trigger` inputs on entry or on the named events (poke, land, wave, alert). Enumerate the file's inputs once and report missing ones.
- Rendering: advance by elapsed time and render into the RGBA buffer, only when advancing reports change or an input changed. Hand the frame to iced like a raster frame (one handle per new frame). Reduced motion holds the current frame and stops advancing.
- Click-through: the hit runs come from the buffer's alpha (fn-2's run helper), recomputed only on a new frame.
- Safe loading: before loading a `.riv` in-process, run `AiPet --check-pack <folder>` as a child process with a time limit (a few seconds); load only after it succeeds. Reject referenced (non-embedded) assets. Cache the check result by file hash, so a startup with unchanged packs stays fast.
- Sample pack: an MIT-licensed example `.riv` from Rive's repositories with a manifest mapping its inputs (or a file the user provides). Record its source and licence.

### Investigation targets
**Required:**
- `rust/crates/aipet-rive/**` (task 13) — the binding and renderer
- `rust/crates/aipet-sprite/src/load.rs` (task 3) — loader conventions and limits
- `rust/crates/aipet/src/main.rs` — the `--check-pack` flag (task 3)

### Key context
The free Rive tier reportedly adds a splash to exported files. Check how such a file looks in the pet, and note the result in the authoring guide (`docs/MAKING-A-PET.md`) and the pack skill.

## Acceptance
- [ ] A Rive pack loads and plays; Pose changes drive its inputs (tests with a sample file: a number input changes with the mood, look moves, a trigger fires on poke)
- [ ] No new frame and no upload while the state machine is idle and inputs are unchanged (test counts frames)
- [ ] Click-through follows the rendered alpha; mirroring, whole-body motion and reduced motion work
- [ ] Every R21 error case gives its reason and never crashes the pet: an oversize file, a malformed file (the child check fails and the pet keeps running), an unknown artboard or state machine, a referenced asset, a check timeout
- [ ] `AiPet --check-pack` reports the sample Rive pack OK and lists missing inputs for a mismatched manifest


## Done summary
Blocked:
Parked: fn-2's Rive spike was a no-go (rust/proofs/rive.md). Unblock when the spec's condition holds: a GPU render path that shares iced's device, a runtime that builds with text and layout and survives damaged files, or calm-CPU headroom from fn-1 task 28.
## Evidence
- Commits:
- Tests:
- PRs:
