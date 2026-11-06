---
satisfies: [R21]
---
# fn-2-layered-sprite-and-vector-pets.13 Rive proof: rive-rs builds everywhere and renders on the CPU within budget

## Description
The proof point for Rive (spec: the Rive early proof point; Decision Context). Before building the pack kind, prove three things:
1. The official `rive-rs`, pinned by git revision, builds on Windows, Linux and macOS in CI.
2. Our own tiny-skia implementation of its `Renderer` trait draws a real `.riv` correctly.
3. Idle and lively CPU at the pet's size stays within fn-1's calm-CPU budget.

If any of these fails, record the numbers and mark R21 deferred. The rest of fn-2 is unaffected.

**Size:** M
**Files:** `rust/crates/aipet-rive/**` (new crate: the binding wrapper and the tiny-skia renderer), `rust/Cargo.toml`, `.github/workflows/ci.yml`, `rust/proofs/rive.md` (new)
**Touches:** [rust/crates/aipet-rive/**, rust/Cargo.toml, rust/Cargo.lock, .github/workflows/ci.yml, rust/proofs/rive.md]

### Approach
- Depend on `rive-rs` by git rev, without its `vello` feature, so it is `no_std`, with no vello and no wgpu. Its build compiles C++ (rive-cpp, harfbuzz, SheenBidi submodules), so CI needs submodules and a C++ toolchain. Note the open `libyoga.a` build issue; turn off the `layout` feature if it bites.
- Implement `rive_rs::renderer::Renderer` and its `Path`, `Paint`, `Gradient` and `Image` types on tiny-skia 0.11 (the version iced already locks): transforms, clip paths, linear and radial gradients, stroke joins and caps, blend modes, and image meshes (a simple triangle rasteriser is fine).
- Render into an RGBA buffer at the pet's device size. Advance with `advance_and_apply(dt)`, and skip drawing when it returns false and no input changed.
- Measure with fn-1's performance script: idle (nothing animating), and lively (continuous animation at the smooth rate). Record the results in `rust/proofs/rive.md`.
- Test files: Rive's own example `.riv` files from the MIT-licensed rive-app repositories (record their source and licence). Never create a Rive account on the user's behalf; if a custom file is needed, the user makes it.

### Investigation targets
**Required:**
- https://github.com/rive-app/rive-rs — `src/renderer.rs`, `src/scene.rs`, `src/state_machine/mod.rs`, `src/file.rs`, `build.rs`
- `rust/crates/aipet-ui/src/sprite.rs` (after task 4) — how raster frames reach iced, to match
- fn-1's `scripts/perf/**` and `rust/proofs/performance.md` — the budget and method

### Key context
`rive-rs` is ~16 months stale (submodule pinned June 2025) and isn't on crates.io; that is accepted for now, and the reason this is a proof task.

## Acceptance
- [ ] CI builds `aipet-rive` with the pinned `rive-rs` on Windows, Linux and macOS
- [ ] A sample `.riv` renders through the tiny-skia renderer and matches a reference render to a stated tolerance (gradients, clips, strokes, an image mesh)
- [ ] Idle and lively CPU and RSS at the pet's size are recorded in `rust/proofs/rive.md` against fn-1's budget, with a go/no-go for task 14
- [ ] If no-go, R21 is marked deferred in the spec with the reason, and task 14 is blocked

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
