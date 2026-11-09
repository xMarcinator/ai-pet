---
satisfies: [R2, R3, R10]
---
# fnt-75-the-pet-window-without-the-gpu.2 The pet scene without iced: own geometry types, sprite drawing and damage

## Description
Turn `PetUi` into a framework-free scene: replace iced's geometry types with the scene's own, and draw the pet itself (not yet the bubbles) into a tiny-skia pixmap with changed-rect tracking. The iced shells keep working on top of it (they draw the pixmap as one image) so nothing breaks between tasks.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/{lib,geometry,cards,sprite,vector_view}.rs`, `rust/crates/aipet-wayland/src/drag.rs`, a new scene draw module
**Touches:** [rust/crates/aipet-ui/**, rust/crates/aipet-wayland/src/drag.rs, rust/crates/aipet-desktop/src/shell.rs, rust/crates/aipet-wayland/src/shell.rs]

### Approach
- Own `Point/Size/Rect/Vector` in the scene; convert at the iced shells' edge only.
- Draw sprites from `sprite.rs` `Art` frames (Nearest for pixel, smooth for painted) and vector packs via `aipet-sprite` `vector::render` (replacing the iced canvas in `vector_view.rs`), plus `sprite::halo` and `shadow_image`.
- Damage: track per-layer bounds before and after each tick; return the union of what moved or changed.
- Equivalence harness: render each built-in pet's states through the old iced path and the new pixmap at scale 1 and 2 and compare (screenshots of the iced canvas are not available headless, so compare against `vector::still` and the sprite frames, which both paths use).

### Investigation targets
**Required:**
- `rust/crates/aipet-ui/src/lib.rs:1-18,95-295,605,844,1174-1267` - the shell contract
- `rust/crates/aipet-ui/src/sprite.rs:84-150,198-300,363-430` - frames, halo, shadow
- `rust/crates/aipet-ui/src/vector_view.rs:21,97-160` - the canvas to replace
- `rust/crates/aipet-sprite/src/vector.rs:438-500` - tiny-skia render

### Key context
- Keep frame pacing (`SMOOTH_FRAME`/`CALM_FRAME`, `AIPET_FPS`) and reduced motion unchanged.

## Acceptance
- [ ] No iced type in the scene's public API, geometry, cards or drag
- [ ] Every built-in pet, style and theme draws into the pixmap at scale 1 and 2, matching the frames the iced path shows
- [ ] A calm pet with nothing changing reports no changed rects; existing aipet-ui, drag and golden tests pass

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
