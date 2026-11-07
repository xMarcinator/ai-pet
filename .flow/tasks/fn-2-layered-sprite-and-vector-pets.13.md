---
satisfies: [R21]
---
# fn-2-layered-sprite-and-vector-pets.13 Rive spike: rive-rs with a tiny-skia renderer, before any Rive implementation

## Description
A time-boxed, throwaway spike: the Rive early proof point in the spec. It answers whether Rive packs are buildable as planned before task 14 implements anything. Its code lives in a standalone Cargo project under `rust/spikes/rive/` (its own `[workspace]`, so the main workspace and lockfile are untouched) on a `spike/` branch. Only its findings are kept: `rust/proofs/rive.md`, plus adjustments to R21, task 14 and the docs and skill tasks.

**Size:** M
**Files:** `rust/spikes/rive/**` (throwaway, on the spike branch), `rust/proofs/rive.md` (new, kept)
**Touches:** [rust/spikes/rive/**, rust/proofs/rive.md, .flow/specs/fn-2-layered-sprite-and-vector-pets.md, .flow/tasks/fn-2-layered-sprite-and-vector-pets.14.md]

## Approach
The user approved these downloads for this spike:
- `git clone --recursive` of github.com/rive-app/rive-rs (MIT; its rive-cpp, harfbuzz and SheenBidi submodules);
- the crates its build needs;
- a few MIT-licensed example `.riv` files from Rive's repositories (record their source and licence).

Ask before any other download. Never create a Rive account; if a custom file is needed, the user makes it.

Questions to answer, with evidence (commands, outputs, timings):
1. **Build:** does `rive-rs` build on Windows/MSVC without its `vello` feature? Record the build time, errors and workarounds (the known `libyoga.a` issue: try without `layout`), and which features remain. Then a throwaway CI run on Linux and macOS from the spike branch.
2. **Renderer:** a minimal `rive_rs::renderer::Renderer` (plus `Path`, `Paint`, `Gradient`, `Image`) on tiny-skia 0.11. It covers transforms, solid and gradient fills, strokes with joins and caps, clip paths, blend modes and image draws; image meshes may be stubbed, but say so. Render a sample's state machine over fixed steps into PNGs at about 260×240, and note visual gaps.
3. **Inputs:** list the sample's state-machine inputs, and show that setting a number or boolean, and firing a trigger, changes the output.
4. **Performance (release build):** time per advance-plus-draw (mean and max over about 300 frames), and the cost when idle (advance reports no change and the draw is skipped). Record peak memory and estimated CPU % at 30 and 60 fps against fn-1's calm budget.
5. **Click-through:** row runs from the RGBA alpha, and what they cost.
6. **Safety:** `File::new` on truncated or corrupted files, run in a child process so a crash can't take down the run: error or crash? Can referenced (out-of-band) assets be detected?
7. **Build impact:** added build time, binary size, extra DLLs or system dependencies.
8. **Splash:** if a free-tier export is available from the user, how the splash shows.

## Investigation targets
**Required:**
- https://github.com/rive-app/rive-rs — `src/renderer.rs`, `src/scene.rs`, `src/state_machine/mod.rs`, `src/file.rs`, `build.rs`, the README's known issues
- `.flow/specs/fn-2-layered-sprite-and-vector-pets.md` — API Contracts → Rive packs, R21, the Rive proof point

## Key context
`rive-rs` is ~16 months stale, isn't on crates.io, and compiles C++. That's why this is a spike. A no-go is a valid result: record why, mark R21 deferred in the spec with the measurements, and block task 14.

## Acceptance
- [ ] `rust/proofs/rive.md` answers questions 1–8 with evidence, and ends with a go/no-go for `rive-rs` plus tiny-skia
- [ ] On go: R21 and task 14 are adjusted to the findings (renderer gaps, features, limits, build steps for CI)
- [ ] On no-go: R21 is marked deferred with the reason and numbers, and task 14 is blocked
- [ ] The spike code stays on its `spike/` branch; nothing from `rust/spikes/` is merged into the work branch

## Done summary
No-go for `rive-rs` plus tiny-skia (provisional; the Windows and macOS builds and fn-1's C# calm baseline are still to come, and could only flip it if the Rust pet is several points cheaper at calm). rive-rs `3d26aa1` builds on Linux only with no features: `text` needs a newer harfbuzz fork, and `layout` needs yoga, which isn't a submodule. The 500-line tiny-skia renderer draws the samples correctly by eye, and number, boolean and trigger inputs all change the output. A pet-like file that keeps animating costs about 0.8 ms per frame at 260×240, 98 % of it drawing: 3.4–11 % of a core at 30 Hz against a 0.5-point margin. Files that settle cost about 1 µs per idle tick. Of 525 damaged files, 15 % crashed and 3 % hung, mostly inside `File::new`. It adds +1.8 MB to the binary and a `libstdc++` dependency on Linux. R21 is deferred in the spec and task 14 is blocked. Proof: `rust/proofs/rive.md`.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: e0b5d5d, 65e27a6
- Tests: spike runs recorded in its proof
- PRs: