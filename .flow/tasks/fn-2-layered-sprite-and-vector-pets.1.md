---
satisfies: [R1, R2, R18]
---
# fn-2-layered-sprite-and-vector-pets.1 Pose, pack model and compositor in aipet-sprite (headless)

## Description
Split today's `compute_pose` into a renderer-neutral **Pose** (spec: Architecture) and add the in-memory pack model and the compositor, with no file loading and no UI. Today's drawing stays in place and the goldens stay green; the next task switches to the compositor. This is the seam every later task builds on.

**Size:** M
**Files:** `rust/crates/aipet-sprite/src/pose.rs` (new), `rust/crates/aipet-sprite/src/pack.rs` (new), `rust/crates/aipet-sprite/src/compose.rs` (new), `rust/crates/aipet-sprite/src/pet.rs`, `rust/crates/aipet-sprite/src/lib.rs`
**Touches:** [rust/crates/aipet-sprite/src/**]

### Approach
- Derive the Pose from the existing `Pose` struct and `compute_pose` (`pet.rs:365-531`). Keep `PetInput`, `PetRng` and `update`'s motion maths (`pet.rs:534-573`) unchanged, and use enums, not strings, for channel values.
- Look direction: quantise the pointer (grid DIPs, `PetInput.mouse`) into 9 directions with hysteresis. Keep the existing eye-follow maths as the source.
- Pack model types: Pack, Layer (channel binding, hit flag, default, fallback), Animation (frames with durations, loop mode), Frame (raster rect with pivot, or vector parts placeholder), Part (id, origin), Pose table, Look table, keyframe Animation (R18). Validate fallback cycles and depth here.
- Compositor: `(pack, pose, t) -> per-layer (animation, frame index, mirrored)`. A pure function, unit-tested with scripted Poses.
- Hit runs: per-frame alpha → row runs at art resolution (adapt `runs()` from `rust/crates/aipet-ui/src/sprite.rs:136`).

### Investigation targets
**Required:**
- `rust/crates/aipet-sprite/src/pet.rs:27-86` — PetInput, PetFrame
- `rust/crates/aipet-sprite/src/pet.rs:365-573` — compute_pose and update
- `rust/crates/aipet-sprite/tests/golden.rs:462-552` — what the goldens pin about states and RNG

**Optional:**
- `docs/ARCHITECTURE.md` §3.3–3.4 — state and override tables

### Key context
The goldens compare doubles bit-exactly (`EXACT_LIBM`, golden.rs:205). The Pose extraction must not change the order of RNG draws or any float maths.

## Acceptance
- [ ] Pose type with the spec's channels, computed from PetInput, time and RNG; the existing golden tests still pass unchanged
- [ ] Pack model with validation: missing default, unknown loop mode, fallback cycle or depth > 8 → error
- [ ] Compositor picks animation and frame by channel and elapsed time for loop, once and pingpong, with mirroring; unit tests cover each mode and the fallbacks
- [ ] Look direction has hysteresis (a test moves the pointer across a threshold and back without flicker)
- [ ] Per-frame hit runs from alpha, unit-tested
- [ ] Part motion (spec: API Contracts → Part motion, R18) in the model and compositor: parts with origins, shapes or frames per value, the `look` table with nearest-direction fallback, `poses` with eased transitions, and keyframe `animations` with `phase`, applied in the stated order before whole-body motion and mirroring. Unit tests drive a pose change mid-transition, a looping keyframe animation with two phases, and a look offset, and read the parts' final positions

## Done summary
`Pet::update` computes a renderer-neutral `Pose` (`Pet::pose`, `pose.rs`) next to the pixels it still draws: the state with its idle act (`idle-hop`, …), the eyes' expression and one of 9 look directions (the pointer's with ±4 DIP hysteresis), named arm and foot poses (with `hold` and `lens` while working), the prop, the headphones, one effect, the facing, each channel's time of change, and the motion. It is derived from what `compute_pose` already chose, so the goldens pass unchanged. `pack.rs` holds the in-memory pack with the spec's validation (missing default, fallback cycles and depth > 8, durations, part ids, ranges); a fallback to `null` draws nothing. `compose.rs` picks each layer's value and frame (loop, once, pingpong), places parts (eased pose transitions that restart from where a part is, keyframes held per key with per-part phase, the look offset with nearest-within-45° fallback, then body motion about the anchor, then mirroring), and turns alpha into row runs.

For task 2: the renderer's eyes have 5 horizontal positions (`lx` −2..2: the idle look act and running use ±2), and the Pose has 3. A bake per look direction can't reproduce the ±2 golden frames, so task 2 may need the eyes' `look` offsets to carry the ±2 steps, or a wider look channel. Also, a poke's sparkles and thinking's dots were drawn together; the Pose's single fx channel says sparkles then.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: c616d5b
- Tests: cd rust && cargo test -p aipet-sprite (44 unit, 5 golden: passed), cargo test -p aipet-ui (passed), cargo clippy --workspace --all-targets -D warnings (clean), cargo fmt --check (clean)
- PRs: