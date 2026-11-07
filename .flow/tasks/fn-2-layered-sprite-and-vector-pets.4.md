---
satisfies: [R7, R9, R10]
---
# fn-2-layered-sprite-and-vector-pets.4 Render packs in the UI: per-kind filtering and snapping, hit mask, frame pacing, Settings → Avatars

## Description
The UI renders a pack's layers directly: one image per layer frame, built once at load. It applies nearest filtering with device-pixel snapping for pixel packs, and linear filtering with `@2x` selection for painted packs. Hit runs feed `sprite_rects` at any art resolution with the merge and cap rules, and frame pacing follows animation timing. The Settings → Avatars page shows every kind with stills and lists skipped packs. This retires the three fixed 26×24 layer buffers and the CPU halo blur.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/sprite.rs`, `rust/crates/aipet-ui/src/view.rs`, `rust/crates/aipet-ui/src/geometry.rs`, `rust/crates/aipet-ui/src/lib.rs`, `rust/crates/aipet-ui/src/settings/avatars.rs`
**Touches:** [rust/crates/aipet-ui/src/**, rust/crates/aipet-ui/tests/**, rust/crates/aipet-sprite/src/pet.rs]

### Approach
- Keep iced image handles for every frame in a per-pack cache built at load and cloned per tick (a new `Handle::from_rgba` re-hashes and re-uploads).
- Snapping: compute the device scale from the shell and round the art origin and pixel size to device pixels. Follow the scale plumbing in `rust/crates/aipet-desktop/src/shell.rs:494-500`.
- `sprite_rects` (`geometry.rs:273-291`): replace the hard-coded 5 px cell with the pack's pixel size, merge rows (`push_merged`, `geometry.rs:233`), cap at 64 rectangles, and fall back to `HIT_ELLIPSE`. Keep `drawn_rects` as the halo box plus the shadow.
- `frame_interval` (`lib.rs:1033`): smooth while an on-screen animation has frames shorter than the calm interval; never faster than 60 fps; keep `AIPET_FPS`.
- Avatars page: stills from each pack's idle first frame; a "Skipped" list with reasons; Reload keeps the worn pack's frames when its folder is gone (`avatars.rs:185-203`).

### Investigation targets
**Required:**
- `rust/crates/aipet-ui/src/sprite.rs:1-200` — today's layer upload, halo, runs, still
- `rust/crates/aipet-ui/src/view.rs:41-62,291` — the layer stack
- `rust/crates/aipet-ui/src/lib.rs:414-534,951-1042` — tick, hit_rects, drawn_rects, frame_interval

**Optional:**
- `rust/crates/aipet-ui/src/lib.rs:1641-1960` — tests to adapt

### Key context
Win32 `SetWindowRgn` clips drawing to the drawn region, so the drawn region must stay the halo box. Only the input mask follows the art.

## Acceptance
- [ ] A tick that changes no frame creates no new image handle (test counts handle creations)
- [ ] Pixel packs are snapped at 100/125/150/200 % (geometry tests) and after a scale change
- [ ] Hit rects follow the displayed frame: capped at 64, ellipse fallback for an empty mask; region tests in aipet-desktop and aipet-wayland still pass
- [ ] Frame pacing follows animation timing within the smooth/calm/60 fps rules (tests in lib.rs)
- [ ] The Avatars page lists all pets with stills plus skipped packs with reasons; Pick, Reload and Open folder work; config is written only on a real change (settings tests)

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
