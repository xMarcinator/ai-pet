---
satisfies: [R20]
---
# fn-2-layered-sprite-and-vector-pets.15 Aseprite spike: reading .aseprite files, a fixture writer, and grid sheets, before any Aseprite implementation

## Description
A time-boxed, throwaway spike before task 11 implements native Aseprite files and grid sheets. It answers whether the `asefile` crate fits, or a direct reader is needed, and whether CI can test without a paid Aseprite. Its code lives in a standalone Cargo project under `rust/spikes/aseprite/` (its own `[workspace]`, so the main workspace and lockfile are untouched) on a `spike/` branch. Only its findings are kept: `rust/proofs/aseprite.md`, plus adjustments to R20 and task 11.

**Size:** S
**Files:** `rust/spikes/aseprite/**` (throwaway, on the spike branch), `rust/proofs/aseprite.md` (new, kept)
**Touches:** [rust/spikes/aseprite/**, rust/proofs/aseprite.md, .flow/specs/fn-2-layered-sprite-and-vector-pets.md, .flow/tasks/fn-2-layered-sprite-and-vector-pets.11.md]

### Approach
The user approved these downloads for this spike:
- the `asefile` crate and its dependencies (via cargo);
- the asefile repository's test `.aseprite` files.

Ask before any other download.

Questions to answer, with evidence:
1. **Build and dependencies:** does the latest `asefile` build? Which `image` version does it pull: does it unify with the workspace's `image` 0.25, or add a second copy? What are its licence and its test files' licence?
2. **Real files:** read several real `.aseprite` test files and list their frames and durations, layers, tags (direction, repeat), slices (keys per frame, pivots) and colour modes. Render layers to PNG and sanity-check the pixels. Note gaps: indexed mode, tilemaps, linked cels, layer groups, user data.
3. **Mapping:** prototype the spec's mapping on a file from step 4: `<layer>:<key>` tags become per-layer animations; part slices give origins; per-frame slice keys give pose positions (R18).
4. **Fixture writer:** a minimal test-only `.aseprite` writer (header, frame, layer, cel raw or zlib, tags, slice, palette; RGBA) per the documented format. Generate a 2-layer, 3-frame file with tags `body:idle`, `eyes:open`, `eyes:closed` and slices `eye-left`/`eye-right` with pivots, then read it back. Report its size in lines and the pitfalls. This is what lets CI test without a paid Aseprite.
5. **Grid sheets:** compute frame rectangles from a grid spec and check them against a generated sheet.
6. **Performance:** parse-and-decode time and decoded memory for a typical file.

### Investigation targets
**Required:**
- https://github.com/aseprite/aseprite/blob/main/docs/ase-file-specs.md — the format
- https://docs.rs/asefile — the crate
- `.flow/specs/fn-2-layered-sprite-and-vector-pets.md` — API Contracts → Native Aseprite files, Grid sprite sheets, Part motion; R5, R18, R20
- `.flow/tasks/fn-2-layered-sprite-and-vector-pets.11.md` — the implementation this spike de-risks

### Key context
Aseprite is paid. The spike must show the tests need no Aseprite install (generated fixtures, plus the `asefile` and LibreSprite files).

## Acceptance
- [ ] `rust/proofs/aseprite.md` answers questions 1–6 with evidence, and ends with a recommendation: `asefile` or a direct reader, plus the fixture writer's feasibility
- [ ] R20 and task 11 are adjusted to the findings (dependency choice, unsupported features, the fixture approach)
- [ ] The spike code stays on its `spike/` branch; nothing from `rust/spikes/` is merged into the work branch

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
