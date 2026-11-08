# fn-2-layered-sprite-and-vector-pets.19 Raster part motion and the vector parts limit (gaps found by the docs pass)

## Description
Close two gaps the docs pass (task 9) found against the spec.

**Size:** S–M
**Files:** `rust/crates/aipet-ui/src/sprite.rs`, `rust/crates/aipet-sprite/src/{pet.rs,compose.rs,pack.rs}`, `docs/MAKING-A-PET.md`, `docs/ARCHITECTURE.md`

### Approach
- **Raster part motion (R18):** raster packs read and check `parts`, `poses`, `transition_ms` and part animations (an Aseprite export's part slices give them), but the UI and `Pet::composite` place raster frames only by pivot and the eyes' look table. Draw each raster part's frame at its `compose::Parts::place` position (pose target eased, animation key, look offset), as vector packs already do, so the same tables move raster and vector parts. Rotation, scale and opacity apply to vector parts only; say so in the guide.
- **Vector parts limit:** the spec caps a pack at 256 vector parts; enforce it in `Pack::validate` with a reason.
- Update the guide and ARCHITECTURE where they say raster parts don't move. Also fix pose.rs's stale module comment ("next to the pixels it still draws itself").

## Acceptance
- [ ] A raster pack's parts move by their poses, transitions and part animations (test with a small pack, and the native Aseprite sample's part slices if it has them)
- [ ] A pack over 256 vector parts is skipped with a reason (test)
- [ ] The guide and ARCHITECTURE describe it; fmt, clippy and every test pass


## Done summary
9c83645: a raster layer moves as one piece by its part's eased pose, animation key and look offset (whole art pixels for pixel packs), in Pet::composite, flatten and the UI sprite, with frame pacing while it moves; at most one part per raster layer and at most 256 vector parts, each with a reason; guide, ARCHITECTURE and spec updated; tests for both kinds and Aseprite slices. Follow-up fix: --check-pack reads a legacy avatar file as one.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits:
- Tests:
- PRs: