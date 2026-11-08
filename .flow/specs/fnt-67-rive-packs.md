# fnt-67-rive-packs Rive packs

## Goal & Context

Rive pets (the owner's request in fn-2, R21 there): a pack can be a `.riv` file whose state machine animates the pet, driven by the pet's Pose. Moved out of fn-2 on 2026-11-08 at the owner's word, because fn-2's spike (task fn-2-layered-sprite-and-vector-pets.13, `rust/proofs/rive.md`) was a no-go and its other tasks shouldn't wait on it. This spec is parked until something below changes.

**Why it is parked:** The spike's CPU renderer misses fn-1's calm budget: a pet-like file that keeps animating costs about 0.8 ms per frame at 260×240, 3.4–11 % of a core at 30 Hz, against a 0.5-point margin; only files that settle are free (about 1 µs per idle tick). Also: rive-rs (3d26aa1) builds only without `text` and `layout` (its harfbuzz 8.3.0 is too old, and yoga is missing), has three binding bugs, and 15 % of damaged files crash and 3 % hang in `File::new`. The result is provisional until the Windows and macOS builds run and fn-1 task 28 measures the C# pet's calm baseline; it flips only if the Rust pet turns out several points cheaper than the C# one. Revisit with a GPU path or a runtime that builds with text and layout.

**What would unblock it:** a GPU render path for Rive that can share iced's wgpu device, or a maintained Rust runtime that builds with text and layout and survives damaged files; or the calm-CPU measurements (fn-1 task 28) showing enough headroom.

## Architecture & Data Models

The pack kind sits beside pixel, painted and vector packs (fn-2): the same Pose, the same loader, limits and `--check-pack`, the same click-through, whole-body motion, mirroring and reduced motion. Only what draws the pet differs: the `.riv` file's state machine, on the CPU into an RGBA image at the pet's device size.

## API Contracts

**Rive packs** (R21, deferred: see R21): `kind: "rive"`. The pack folder holds one `.riv` file and a manifest that names the artboard and state machine, and maps Pose channels onto that state machine's inputs:

```json
{ "format_version": 1, "name": "Ghosty", "kind": "rive", "anchor": [65, 118.5], "mirror": "flip",
  "rive": { "file": "ghosty.riv", "artboard": "Pet", "state_machine": "Main",
            "inputs": {
              "mood":     { "number": "state",  "values": { "idle": 0, "sleep": 1, "thinking": 2, "working": 3,
                                                          "attention": 4, "done": 5, "listen": 6 } },
              "lookX":    { "number": "look.x" }, "lookY": { "number": "look.y" },
              "blink":    { "trigger": "eyes", "on": "closed" },
              "happy":    { "bool": "eyes", "when": ["happy"] },
              "typing":   { "bool": "arms", "when": ["type"] },
              "laptop":   { "bool": "prop", "when": ["laptop"] },
              "music":    { "bool": "headphones", "when": ["on"] },
              "poke":     { "trigger": "poke" } } } }
```

- **Input kinds**: a `number` input takes a channel value through its `values` map, or a continuous source: `look.x` and `look.y` run from −1 to 1. A `bool` input is true while the channel is one of `when`. A `trigger` fires when the channel enters `on`, or on the event named (`poke`, `land`, `wave`, `alert`).
- **Missing inputs**: inputs the file doesn't have are reported by `--check-pack` and ignored, and channels the manifest doesn't map have no effect.
- **The Rive state machine** owns the animation, blending and timing inside the pet. The app owns behaviour, whole-body motion, mirroring (unless `mirror` is `none`), the ground shadow and click-through.
- **Rendering**: the runtime advances by the elapsed time each tick and draws into a CPU RGBA image at the pet's device size. When it reports nothing animating and no input changed, nothing is drawn or uploaded. The image's alpha gives the hit mask, as for raster frames.
- **Load safety**:
  - only embedded assets (no referenced images or fonts), no Rive scripting, no data binding or view models;
  - at most 4 MB per file;
  - a `.riv` is first parsed by `AiPet --check-pack` in a child process with a time limit, and loaded in-process only if that succeeded, so a malformed file can't crash the pet.


## Edge Cases & Constraints

- Damaged files crash or hang the runtime (the spike: 15 % crash, 3 % hang in `File::new`), so a `.riv` is checked in a child process with a time limit before it loads in-process, on the exact bytes that will load.
- Nothing animating and no input changed means nothing drawn or uploaded.
- Free-tier Rive exports reportedly carry a splash; check how such a file looks before recommending the free tier.

## Acceptance Criteria

- **R1:** Rive packs (`kind: "rive"`) play a `.riv` file's state machine, driven by the manifest's mapping from Pose channels to inputs (numbers, booleans, triggers; `look.x`/`look.y` continuous), as in the API contracts above. They are rendered on the CPU into an RGBA image at the pet's device size, with no texture upload when nothing animates. They keep click-through from alpha, mirroring, whole-body motion and reduced motion, which holds the state machine's current frame and stops advancing. They load only after the child-process check succeeds. Errors: each of the following skips the pack with a reason, and never crashes the pet:
  - a file over 4 MB;
  - a missing or malformed `.riv`, or an unsupported Rive version;
  - an unknown artboard or state machine;
  - referenced (non-embedded) assets;
  - a check-process failure or timeout.

  An input the file lacks is reported by `--check-pack` and ignored.

## Boundaries

No data binding or view models, no referenced (out-of-band) assets, no Rive scripting, no built-in Rive pets (one sample only).

## Decision Context

The spike's findings and the rejected render paths are in fn-2's Decision Context and `rust/proofs/rive.md`. Carry over from the spike: a renderer that never returns `None` from `Image::decode`; an inspection shim (`FileAssetLoader` plus a no-op factory) for names, referenced assets and view models; a child check that also plays frames; static libstdc++ on Linux; CI that clones rive-rs at a pinned revision with submodules.
