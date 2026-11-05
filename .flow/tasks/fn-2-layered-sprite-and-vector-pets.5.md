---
satisfies: [R8, R11]
---
# fn-2-layered-sprite-and-vector-pets.5 Vector packs and the two Pip pets (vector and painted)

## Description
Add the vector kind: parts with SVG-syntax path data, fills, strokes and parameter bindings to Pose channels and motion, drawn through iced's canvas with geometry cached per Pose. The hit mask is rasterised with tiny-skia at art resolution when the Pose changes. Ship a vector Pip modelled on the repo-root pocket-sprout.html (the user's design), and a painted Pip baked from it at high resolution into PNG sheets (with `@2x`).

**Size:** M
**Files:** `rust/crates/aipet-sprite/src/vector.rs` (new), `rust/crates/aipet-ui/src/vector_view.rs` (new), `rust/crates/aipet-ui/src/view.rs`, `rust/crates/aipet-sprite/packs/{pip,pip-painted}/**` (new), `rust/Cargo.toml`
**Touches:** [rust/crates/aipet-sprite/**, rust/crates/aipet-ui/src/**, rust/Cargo.toml, rust/Cargo.lock]

### Approach
- Path data: parse the SVG `d` syntax with a small parser or a maintained crate. Prefer what lyon (already locked) offers before adding a crate. Reject non-finite and out-of-range numbers.
- Canvas: one `canvas::Program` with a `Cache` cleared only when the Pose or motion bucket changes.
- tiny-skia is already in the lock at 0.11 through iced; depend on that version, not 0.12.
- Pip: translate pocket-sprout.html's SVG (body, belly, cheeks, sprout leaves, eyes with highlights, lids, mouth) into parts. Map its procedural motions (breathe squash, look-at, blink by eye ry, leaf sway) to bindings, and add per-state parts for props, headphones and effects.
- Painted Pip: render the vector pack per channel value with tiny-skia into sheets (a bake like task 2's); linear filtering.

### Investigation targets
**Required:**
- `pocket-sprout.html` (repo root, untracked) — lines ~155-176 (character SVG), ~548-624 (render loop)
- `rust/crates/aipet-ui/src/view.rs:41-62` — where the pet is stacked

### Key context
iced's `svg` feature is off and would re-rasterise each animated frame; don't enable it.

## Acceptance
- [ ] Vector packs load (with the R8 rejections tested) and draw through canvas; geometry is rebuilt only on a Pose change (test counts rebuilds)
- [ ] The vector hit mask follows the drawn parts
- [ ] Vector Pip and painted Pip are built-in, with every state, eye, arm, prop, headphone and effect layer; `--check-pack` reports OK for both
- [ ] No external resource can be loaded by a vector pack (test with a path or URL-like value)

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
