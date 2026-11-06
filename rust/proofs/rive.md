# Rive proof: rive-rs with a tiny-skia renderer

Scope: task fn-2-layered-sprite-and-vector-pets.13, the Rive proof point in the fn-2 spec. Can a Rive pack (R21) play
a `.riv` state machine through the official `rive-rs` runtime and our own CPU renderer on tiny-skia, within fn-1's
calm-CPU budget (fn-1 R18), and load untrusted files safely?

The spike is throwaway code on branch `spike/rive`, in `rust/spikes/rive/` (its own Cargo workspace; the main
workspace and its lockfile are untouched):
- `src/render.rs`: `rive_rs::renderer::Renderer` with its `Path`, `Paint`, `Gradient`, `Image` and `Buffer` on
  tiny-skia 0.11.4, plus the hit runs from alpha (500 lines).
- `src/inspect.cpp`, `src/inspect.rs`, `build.rs`: a C++ shim for what `rive-rs` doesn't expose (names, assets, view
  models), linked against the rive-cpp that `rive-rs` builds.
- `src/main.rs`: the `info`, `render`, `render-scene`, `inputs`, `bench`, `load` and `fuzz` subcommands.
- `run.sh`, `bench.sh`, `repeat.sh`, `fuzz.sh`: the runs quoted below.
- `ci/rive-spike.yml`: the Windows, Linux and macOS CI run. It has not run (nothing was pushed).

Machine: Linux (Arch, Hyprland), i7-12850HX (8 P-cores with 2 threads each, 8 E-cores; `powersave` governor),
64 GB, cargo and rustc 1.99.1, GCC 16.2.1, clang 23.1.4. The machine was not idle during the CPU runs: load average
3–4, with another session's benchmark at about 20 % of a core and a browser.

## Verdict

**No-go for now** for `rive-rs` plus tiny-skia. R21 should be deferred and task 14 blocked. The CPU result alone
decides it, so the pending Windows and macOS CI run can't turn it into a go; it could only add reasons.

- **CPU.** A pet-like file that keeps animating (`death_knight.riv`, 26 fills, 13 strokes, 15 gradients) costs
  0.81 ms per advance-plus-draw at 260×240. Paced at the pet's calm 30 Hz, that is 3.4–9.8 % of a core over six
  runs (median of five 5.6 %); the lowest possible figure, from the unpaced mean, is 2.4 %. fn-1's calm budget allows the whole
  Rust pet 0.5 percentage points over the C# pet. Rive alone is 5 to 20 times that margin, before iced uploads each
  frame. A file that settles costs nothing (1 µs per idle tick), but pets don't settle: they breathe and blink.
- **Build.** Linux builds only without both default features. `text` fails (its harfbuzz submodule is too old for
  rive-cpp) and `layout` fails (the yoga sources aren't in the repository). Without them, text isn't drawn and files
  that use layouts collapse to nothing. Fixing either needs a download that wasn't approved.
- **Safety.** 15 % of corrupted or truncated copies crash the process and 3 % hang inside `File::new`. The child-process
  check is mandatory, and must also play a few frames.
- **Binding.** Three bugs in `rive-rs` (one segfault in its API, one null dereference on a bad image, one type not
  exported) and missing APIs (artboard and state machine names, assets, view models), so task 14 would carry a patched
  fork plus a C++ shim.

Windows (MSVC) and macOS are pending the CI run in `ci/rive-spike.yml`.

## 1. Build

Clone: `git clone --recursive https://github.com/rive-app/rive-rs` (16 s, 496 MB). Revisions:

| Repository | Revision |
|---|---|
| rive-rs | `3d26aa1c` (main, 2025-07-04, "feat(webgpu): Finish input attachments...") |
| rive-cpp (submodule) | `24c6b5db` (`runtime-v0.1.0~533`); reads `.riv` major version 7 |
| harfbuzz (submodule, rive-app fork) | `582a5b3a`, version 8.3.0 |
| SheenBidi (submodule) | `adfccc46` (v2.6) |

The spike depends on the clone by path, `default-features = false`. Without the `vello` feature, Cargo never fetches
vello's git dependency: the lockfile has 32 packages, none of them vello.

| Features | Result (Linux, GCC 16.2.1, 24 threads) |
|---|---|
| default (`text`, `layout`) | fails after 1 min 12 s: `font_hb.cpp:36:21: error: 'hb_face_create_or_fail' was not declared in this scope` |
| `layout` | fails in 1.2 s: `ar: '.../out/libyoga.a': No such file` |
| `text` | fails as the default does |
| none | builds: 42 s wall, 10 min 51 s CPU (`debug = 1`); 68 s wall, 4 min 12 s CPU at `-j4`, `debug = 0`. Five rustc lint warnings in rive-rs (elided lifetimes), no errors |
| none, `CC=clang CXX=clang++` (clang 23.1.4) | builds in 34 s |

Causes:
- `text`: rive-cpp needs harfbuzz 10.1 (`dependencies/premake5_harfbuzz.lua` pins `rive-app/harfbuzz` tag
  `rive_10.1.0`), but rive-rs's submodule is 8.3.0. rive-rs's own CI also builds `--all-features` (with Rust 1.72);
  whether it passes at this revision wasn't checked.
- `layout`: `build.rs` compiles `../submodules/yoga/yoga/*.cpp`, but yoga isn't a submodule. No files match, and `ar`
  is asked to create an empty archive. rive-cpp expects `rive-app/yoga` branch `rive_changes_v2_0_1_1`.
- Both fixes mean fetching those two repositories at those revisions (and possibly rive-cpp's symbol renames for
  harfbuzz). Not tried: those downloads weren't approved.

One Cargo trap: a path dependency inside the spike's directory becomes a member of the spike's workspace, and rive-rs
then fails with "error inheriting `vello` from workspace root manifest's `workspace.dependencies.vello`". The fix is
`exclude = ["vendor"]` in the spike's `[workspace]`.

Features that remain: none. State machines, linear animations, inputs, events, pointer input, nested artboards and
embedded PNG images work. Text runs aren't drawn, and layout components have no size.

Windows (MSVC, without `vello`): pending the CI run. Expected trouble: `build.rs` passes `-std=c++14` to cl.exe (a
D9002 warning, not an error). macOS: pending the CI run.

## 2. Renderer

`src/render.rs` implements the whole trait on tiny-skia 0.11.4:
- transforms: a state stack of `Transform`s, `state_push`/`state_pop`, `transform` pre-concatenates;
- paths: a `PathBuilder` plus a cached built `Path` (rebuilt only after a change); `extend` with a transform; winding
  and even-odd;
- solid colours, linear and radial gradients (with a solid fallback where tiny-skia refuses a degenerate gradient);
- strokes with tiny-skia's joins (miter, round, bevel) and caps (butt, round, square), unlike rive-rs's vello
  renderer, which draws every stroke round;
- clip paths as a `Mask` per state. Clips intersect with the enclosing state's clip and with earlier clips in the same
  state, as rive-cpp expects (a drawable with several clipping shapes calls `clipPath` once per shape). The vello
  renderer replaces the earlier clip instead;
- all 16 blend modes, which map one to one onto tiny-skia's;
- images: `draw_pixmap` centred on the origin, with opacity, blend mode, bilinear filtering and the clip;
- image meshes: each triangle filled with the image as a `Pattern`, mapped from its UVs by an affine transform.
  Implemented but **untested**: none of the 133 sample files under 400 KB, nor `zombie_skins.riv` or `marty.riv`, drew
  a mesh in its first 4 s.

Frames: `run.sh render` writes 260×240 PNGs at 250 ms steps; the artboard is fitted into the frame by rive-rs's
viewport transform (contain, centred).

| File | Draws per frame | Result |
|---|---|---|
| `death_knight.riv` (500×500) | 26 fills, 13 strokes, 15 gradients, 1 clip, 1 blend | correct by eye, frames change every step ([frame](rive/death-knight.png)) |
| `bad_skin.riv` (999×720) | 42 fills, 10 strokes, 13 gradients, 3 clips, 13 images | correct by eye, embedded PNGs drawn ([frame](rive/bad-skin-images.png)) |
| `rating-animation.riv` (rive-rs's own sample) | 6 fills, 5 strokes, 1 gradient, 1 clip | correct by eye; settles after 1 s |
| `hunter_x_demo.riv` (3387×1905) | 319 fills, 169 strokes, 181 gradients, 131 clips, 12 blends | only the grey background shows ([frame](rive/hunter-x-no-layout.png)): every other draw's bounds collapse to a point. Most likely its layouts, which have no size without `layout` |

There is no reference renderer here, so "correct" means by eye. Visual gaps:
- text isn't drawn (no `text` feature);
- layout components have no size (no `layout` feature), so layout-based files are empty;
- feather (soft edges) is ignored: rive-rs's `RustPaint` doesn't override `feather`, so edges stay hard;
- only PNG images decode (tiny-skia's decoder); JPEG and WebP would need another decoder;
- image meshes untested; adjacent triangles are anti-aliased separately, so seams are possible.

Binding problems found while writing it:
- `Scene::name()` on a `StateMachine` segfaults: `rive_rs_component_name` casts the state machine instance to a
  `Component`, which it isn't (`gdb`: `rive::StateMachineInstance::name ... this=0xa`).
- `Image::decode` returning `None` becomes a null `R::Image`. rive-cpp still draws it, and `renderer_draw_image`
  dereferences it (`gdb`: `render::draw_image () at src/render.rs:411`, from `rive::Image::draw`). The spike's decode
  never returns `None` (a broken image becomes 1×1 transparent).
- The `Input` enum that `StateMachine::inputs()` yields isn't exported (`state_machine::inputs` is private). The spike
  reads the kind and name from its `Debug` output.
- No API for artboard names, state machine names, assets or view models: `src/inspect.cpp` gets them.

## 3. Inputs

`run.sh info` lists the default state machine's inputs, and `run.sh inputs` compares two instances: both advance 1 s
at 60 Hz, then one input changes on one of them, and both advance 90 more frames. "Pixels" is the largest number of
the 62,400 pixels that differ from the control.

| File | Input | Change | Pixels | At frame |
|---|---|---|---|---|
| `death_knight.riv` | number `Direction` (1) | set to 2 | 15,476 | 5 |
| | trigger `Attack` | fired | 18,270 | 22 |
| | trigger `Death` | fired | 21,940 | 62 |
| | bool `isMoving` (false) | set to true | 11,669 | 8 |
| `rating-animation.riv` | number `rating` (0) | set to 1 | 589 | 16 |
| `light_switch.riv` | bool `On` (true) | set to false | 5,900 | 2 |
| `Timer.riv` | trigger `Start` | fired | 4,936 | 11 |
| | triggers `Stop`, `Reset` | fired | 0 (they only act after `Start`) | |
| `bad_skin.riv` | bools `woman switch`, `a_boolean`, `Letter_boolean`, `PNG_boolean` | set to true | 7,439, 1,215, 795, 1,122 | |
| | triggers `a_Click`, `Letter_Click`, `PNG_Click` | fired | 0 (driven by listeners) | |

Numbers, booleans and triggers all change the output. Before and after `Direction` changes:
[1](rive/death-knight-direction-1.png), [2](rive/death-knight-direction-2.png).

## 4. Performance

Release build, `bench.sh` and `repeat.sh`. Advance-plus-draw is one `advance_and_maybe_draw` (rive-rs skips the draw
when advancing reports no change); the frame is cleared only when it draws. "Paced" runs 10 s at 30 or 60 Hz, sleeping
between frames, and reads this process's CPU time from `/proc/self/schedstat` (checked against utime + stime). Hit runs
are included in the paced runs.

At 260×240, `death_knight.riv`, five runs unpinned:

| | Run 1 | 2 | 3 | 4 | 5 | Median |
|---|---|---|---|---|---|---|
| advance+draw mean (300 frames) | 0.824 ms | 0.945 | 0.808 | 0.796 | 0.755 | 0.808 ms |
| advance+draw max | 1.609 ms | 1.523 | 1.430 | 2.678 | 0.824 | 1.523 ms |
| paced 30 Hz, % of a core | 9.78 | 5.11 | 4.57 | 6.33 | 5.57 | 5.57 |
| paced 60 Hz, % of a core | 15.85 | 13.77 | 21.84 | 10.04 | 6.75 | 13.77 |

Advance alone takes 0.012–0.016 ms: the draw is 98 % of the cost. The paced cost per frame is higher than the
unpaced one (1.1–4.2 ms against 0.8 ms) because the core slows down and caches go cold between frames. Pinned to a
P-core (cpu 8): 8.2 % at 30 Hz, 10.6 % at 60 Hz. Pinned to an E-core (cpu 16): 4.7 % and 10.6 %.

All samples, one unpinned run each (`bench.sh`):

| File, size | advance+draw mean / p95 / max | Paced 30 Hz | Paced 60 Hz | Peak memory (VmHWM) |
|---|---|---|---|---|
| `death_knight.riv`, 260×240 | 0.943 / 1.051 / 1.593 ms | 3.4 % | 5.8 % | 5.6 → 12.0 MB |
| `death_knight.riv`, 520×480 | 1.580 / 1.631 / 1.885 ms | 18.3 % | 14.4 % | 5.6 → 12.9 MB |
| `bad_skin.riv`, 260×240 | 2.468 / 3.302 / 3.768 ms | 10.1 % | 13.4 % | 5.9 → 24.6 MB |
| `bad_skin.riv`, 520×480 | 3.950 / 4.294 / 5.723 ms | 33.1 % | 38.9 % | 5.8 → 25.9 MB |
| `rating-animation.riv`, 260×240 | 0.138 / 0.623 / 1.763 ms, 60 of 300 frames drawn | 0.6 % | 0.6 % | 5.4 → 7.1 MB |

- Peak memory is before loading and after the bench, which holds two instances of the file (one for the
  advance-only timing). Each instance includes a 250 KB frame at 260×240 and its decoded images.
- Load plus instantiate: 0.6 ms (`rating-animation.riv`, 16 KB), 4.4 ms (`death_knight.riv`, 66 KB), 18 ms
  (`bad_skin.riv`, 291 KB, 14 PNGs).
- Idle: `rating-animation.riv` settles after its first second. Then 300 ticks drew nothing and took 0.7–1.2 µs each
  (max 3.4 µs). `death_knight.riv` and `bad_skin.riv` never settle within 30 s.

Against fn-1's calm budget (R18: the Rust pet's median calm CPU ≤ the C# pet's median + 0.5 percentage points; calm
is 30 Hz): the C# baseline isn't measured yet (fn-1 task 28), and the Rust sprite pet costs 7.6–8.0 % calm on this
laptop (SPIKE.md). A Rive pack that keeps animating adds 2.4 % at the very least (the unpaced mean × 30) and 3.4–9.8 %
as measured, plus iced's upload of each new frame, which the spike doesn't measure. That passes only if the Rust pet
were 2 to 10 points under the C# pet. Rive passes the budget only for files that settle.

## 5. Click-through

`render::runs` takes rows of non-transparent pixels from the frame's alpha as `(x, y, length)`, as aipet-ui's
`sprite::runs` does. It runs only after a frame was drawn.

| File, size | Runs | Mean | Max |
|---|---|---|---|
| `death_knight.riv`, 260×240 | 315 | 0.024–0.035 ms | 0.070 ms |
| `death_knight.riv`, 520×480 | 636 | 0.080 ms | 0.269 ms |
| `bad_skin.riv`, 260×240 | 188 | 0.035 ms | 0.074 ms |

About 3 % of a frame's draw. Click-through isn't the problem.

## 6. Safety

`fuzz.sh` makes 175 copies of each file: truncated at fixed lengths and at 40 even steps, the major version plus one,
1, 4, 16, 64 and 256 random bit flips after the 16-byte header (20 copies each), and 25 % random bytes (20 copies). Each
copy runs in a child process (`rive-spike load`: `File::new`, the default artboard and state machine, 30 frames
drawn) with a 5 s limit, then again with `SPIKE_PARSE_ONLY=1` (`File::new` only).

| File | Loaded | Malformed | Unsupported version | No artboard or state machine | Crashed | Timed out |
|---|---|---|---|---|---|---|
| `death_knight.riv` | 34 | 110 | 1 | 3 | 17 (16 in `File::new`) | 10 (10) |
| `rating-animation.riv` | 45 | 91 | 1 | 9 | 24 (17) | 5 (5) |
| `bad_skin.riv` | 54 | 80 | 1 | 0 | 37 (32) | 3 (3) |
| All 525 | 133 | 281 | 3 | 12 | 78 (15 %) | 18 (3 %) |

- `File::new` returns `Error::Malformed` for most damage, and `Error::UnsupportedVersion` for the bumped major
  version. But it also crashes (SIGSEGV and SIGABRT) and hangs. One truncated `bad_skin.riv` crashes reading past the
  end of the buffer: `rive::FileAssetContents::decodeBytes` at `file_asset_contents.cpp:24`, from
  `FileAssetContentsBase::deserialize`.
- 13 of the crashes come after a successful `File::new`, while playing, so the child check must also play frames.
- Crashes vary between runs of the same copies (out-of-bounds reads), so the check must run on the exact bytes that
  will be loaded.
- Before the spike's decode fix, `bad_skin.riv` crashed in 121 of 175 copies, most of them in rive-rs's null-image
  dereference.

Referenced assets can be detected, but not through rive-rs. `src/inspect.cpp` imports the file a second time with
rive-cpp's no-op factory and a `FileAssetLoader` that records each asset and its in-band byte count:

```
hosted_image_file.riv: asset one-45008.png: REFERENCED (not embedded), has a CDN uuid
in_band_asset.riv:     asset 1x1-45022.png: embedded, 308 bytes
bad_skin.riv:          14 PNG assets, all embedded (1,525–88,144 bytes)
hunter_x_demo.riv:     2 fonts embedded; view models: 3
```

The same shim counts view models, so files that need data binding can be rejected too.

## 7. Build impact

- Build time: rive-cpp compiles in 42 s wall at 24 threads and 68 s at 4 threads (about 4–11 CPU-minutes). It is
  rebuilt only when it changes; the app crate relinks in 0.6 s.
- Binary size (release, stripped): 2,545,760 bytes with rive-rs, renderer and shim, against 755,312 bytes for the
  same tiny-skia drawing without them (`examples/baseline.rs`): **+1.79 MB**.
- System dependencies, Linux: a C++ compiler at build time, and `libstdc++.so.6` at run time. Today's `AiPet` links
  only `libgcc_s`, `libm` and `libc` (`ldd`). Linking libstdc++ statically would avoid the new runtime dependency.
- Windows and macOS: pending the CI run. The workflow records the DLLs (`dumpbin /DEPENDENTS`) and dylibs (`otool -L`).
  rive-cpp's use of the C++ standard library will likely add `msvcp140.dll`, which a Rust-only build doesn't need.
- Crates: rive-rs brings only `cc`, `walkdir` and `bitflags` 2 (plus tiny-skia, which iced already locks). The clone is
  496 MB and isn't on crates.io, so CI must clone it at a pinned revision with its submodules (16 s here).

## 8. Splash

No free-tier export from the user exists yet, so this is unanswered. What to check when the user makes one (the user
makes it; no Rive account is created for this):
- `rive-spike info`: an extra artboard, nested artboard, image asset or animation that the user didn't make;
- `rive-spike render` over the first seconds: whether a splash shows before the state machine's first state, how long,
  and whether it returns;
- whether it is drawn into the frame (and so into the hit mask) or only by Rive's own players. The runtimes are open
  source, so a splash rive-cpp draws would have to be in the file.

## Re-running it

On the `spike/rive` branch, from `rust/spikes/rive/`:

```
git clone --recursive https://github.com/rive-app/rive-rs vendor/rive-rs   # then check out 3d26aa1c
cargo build --release
./run.sh info; ./run.sh render; ./run.sh inputs   # also links the samples into samples/
./bench.sh; ./repeat.sh
./fuzz.sh
```

Sample files, all MIT-licensed, from the clone (no other downloads):

| File | Source | Licence |
|---|---|---|
| `rating-animation.riv` | https://github.com/rive-app/rive-rs/blob/3d26aa1c3865260e5ef9751ad0172dcc197ff8eb/assets/rating-animation.riv | MIT (rive-rs `LICENSE`, © 2023 Rive) |
| `death_knight.riv`, `bad_skin.riv`, `hunter_x_demo.riv`, `light_switch.riv`, `state_machine_triggers.riv`, `hosted_image_file.riv`, `in_band_asset.riv`, `feather_render_test.riv`, `zombie_skins.riv` | https://github.com/rive-app/rive-cpp/tree/24c6b5db10b458f179a8abc3b91d2824ecd96042/tests/unit_tests/assets | MIT (rive-cpp `LICENSE`, © 2020 Rive) |
| `Timer.riv`, `avatar_demo.riv` | https://github.com/rive-app/rive-cpp/tree/24c6b5db10b458f179a8abc3b91d2824ecd96042/renderer/webgpu_player/rivs | MIT (rive-cpp `LICENSE`) |
| `marty.riv` | https://github.com/rive-app/rive-cpp/tree/24c6b5db10b458f179a8abc3b91d2824ecd96042/tess/test/assets | MIT (rive-cpp `LICENSE`) |

The licence is the repository's; the files carry no licence of their own.

## Still unverified

- Windows (MSVC) and macOS builds, runs, DLLs and dylibs: `ci/rive-spike.yml` hasn't run.
- iced's cost to upload a new 260×240 frame each tick, and the whole pet's CPU with a Rive pack (fn-1's script).
- The C# pet's calm CPU (fn-1 task 28), which sets the budget's baseline.
- Image meshes (no sample drew one), text and layouts (not buildable), and a pixel comparison with Rive's own renderer.
- The free-tier splash (question 8).
