# Aseprite spike: asefile, a direct reader, a fixture writer and grid sheets

Scope: whether the `asefile` crate fits fn-2's native `.aseprite` packs (R20, task fn-2-layered-sprite-and-vector-pets.11),
or a direct reader is needed, and whether CI can test without a paid Aseprite. This is task
fn-2-layered-sprite-and-vector-pets.15.

The spike lives on branch `spike/aseprite` in `rust/spikes/aseprite/`, a standalone Cargo project with its own
`[workspace]`. The main workspace and its lockfile are untouched. Files:
- `src/writer.rs`: the test-only `.aseprite` writer (question 4).
- `src/reader.rs`: a direct reader for the subset, written to size the alternative and checked against `asefile`.
- `src/main.rs`: the experiments (`info`, `render`, `fixture`, `map`, `grid`, `typical`, `bench`, `rbench`, `compare`,
  `hostile`, `direct`) and a counting allocator for heap numbers.
- `chunks.py`: lists each frame's chunk types, to compare the writer's layout with real files.

Machine: Arch Linux, i7-12850HX, cargo 1.99. Release builds for every number. Downloads: the `asefile` crate and its
dependencies, and a clone of github.com/alpine-alpaca/asefile for its test files. `image` 0.25.10 with `png` (for
writing renders) came from the local cargo cache; builds ran with `--offline`. No LibreSprite file and no Aseprite.

## Verdict

Use a direct reader, not `asefile`. The reader for the needed subset is 215 lines (198 non-blank, non-comment). It
matches `asefile` pixel for pixel on every file both accept, and it enforces the pack limits before allocating.
`asefile` doesn't fit for four reasons:
- its released version (0.3.8) adds a second `image`;
- it rejects valid Aseprite files (ping-pong reverse tags, embedded ICC profiles, any unknown chunk);
- a hostile file can abort the process or make it allocate about 900 MB;
- it decodes every cel inside one call, so the 64 MB decoded limit can't be enforced.

The fixture writer is easy: 145 lines (128 non-blank, non-comment). Its files read back correctly with both `asefile` and
the direct reader on the first run. CI needs no Aseprite.

## 1. Build and dependencies

The latest release builds. It pins `image` 0.24, so it can't unify with the workspace's 0.25.10.

| | Release 0.3.8 (crates.io, 2024-03-14) | master `2274c35` (unreleased, 2024-07-21) |
|---|---|---|
| `image` requirement | `0.24`, `default-features = false` | `>= 0.24, < 0.26`, `default-features = false` |
| With `image = "=0.25.10"` beside it | two copies: `image v0.24.9` (asefile) and `image v0.25.10` | one copy, 0.25.10 |
| Builds | yes (`cargo build`, 3.75 s dev) | yes |

Evidence (`cargo tree -d`, release 0.3.8 plus `image = "=0.25.10"`):

```
image v0.24.9
└── asefile v0.3.8
image v0.25.10
└── aseprite-spike v0.0.0
```

The master version unifies only after a fresh resolve. An existing lockfile kept 0.24.9 until `Cargo.lock` was
deleted. Using master means a git dependency on a rev with no release in over two years (its last commit is
2024-07-21). A fork, `ah-asefile` 0.4.1, shows in `cargo search`, but downloading it was not approved, so it was not
examined.

Other dependencies: `bitflags` 2, `byteorder` 1.3, `flate2` 1, `log` 0.4, `nohash` 0.2. The main lockfile already has
`bitflags` 2.13.2, `flate2` 1.1.10, `miniz_oxide` 0.9.1 and `log`. Only `byteorder` and `nohash` would be new.

Licences:
- `asefile`: MIT, "Copyright (c) 2020 Alpine Alpaca Games" (`LICENSE` in the crate and the repo).
- Its test files: 44 `.aseprite` files and 49 reference PNGs in `tests/data/`, plus 3 in `examples/`, 15 MB in total.
  They have no licence of their own, so the repo's MIT `LICENSE` covers them. Reusing them needs that notice in
  THIRD-PARTY-NOTICES.

## 2. Real files

`aseprite-spike info tests/data/*.aseprite examples/basic/input.aseprite` read 44 of 45 files. A selection:

```
indexed.aseprite: 64x64 indexed(t=1), 4 frames ms=[100, 100, 100, 100], palette=73   (3 layers, 4 cels each)
layers_and_tags.aseprite: 16x16 rgba, 4 frames   (6 layers, 2 hidden, 1 group)
  tag "T1" 0..=1 Forward repeat=None
  tag "T3" 1..=3 Forward repeat=None
  tag "T2" 3..=3 Forward repeat=None
linked_cels.aseprite: 16x16 rgba, 3 frames ms=[100, 100, 100]
slice_advanced.aseprite: 32x32 rgba, 4 frames
  slice "Slice 1" keys=[f0@(12, 11)(8, 10) pivot=Some((4, 10)), f1@(18, 5)(8, 10) pivot=Some((4, 10)),
                        f2@(24, 11)(8, 10) pivot=Some((4, 10)), f3@(15, 21)(8, 10) pivot=Some((4, 10))]
  slice "Slice 2" keys=[f0@(2, 1)(8, 8) pivot=None 9=true]
user_data.aseprite: 16x16 rgba, 8 frames   (user data text on the sprite, a layer and two tags)
grayscale.aseprite: 64x64 grayscale, 1 frames
tilemap_indexed.aseprite: 32x32 indexed(t=0), 1 frames, tilesets=2
color-curve.aseprite: ERROR Unsupported Aseprite feature: Embedded ICC color profiles are currently not supported
```

Renders: `aseprite-spike render` wrote every frame and every layer cel to PNG, and compared each frame with the
repo's reference PNG. 19 frames from 13 files match exactly (0 pixels differ). They cover RGBA, indexed, greyscale,
the old palette chunk, linked cels, hidden layers, a group, a tilemap, an indexed tilemap, and a 256×256 file. One
indexed render was also checked by eye. The references are `asefile`'s own test oracles; that they came from Aseprite
was not verified.

What the test files don't cover:
- Tags: every tag is Forward with no repeat. No Reverse, ping-pong or repeat count appears.
- Aseprite 1.3 features: every file has header flags = 1. None has layer UUIDs, cel z-index or user-data properties.
- Chunk order differs from the writer's. In real files the palette comes first, then layers, then tags and slices
  (each followed by user data), then cels, e.g. `f0:2007,2019,0004,2004,2022,2020,2022,2020,2005`.

Gaps in `asefile`, from its source and the tests above:
- **Ping-pong reverse** (tag direction 3, in Aseprite since 1.3): `AnimationDirection` has no variant for it, and the
  parse fails with "Unknown animation direction: 3". The whole file is rejected.
- **Unknown chunk types**: a hard error ("Invalid or unsupported chunk type: 0x2099"), so a chunk added by a future
  Aseprite breaks every file. The format allows readers to skip chunks they don't know.
- **Embedded ICC profile**: rejects the whole file (`color-curve.aseprite`).
- **Indexed, greyscale, linked cels**: supported. Every image comes back as RGBA.
- **Tilemaps**: supported and rendered. External tilesets show up in `external_files()`.
- **Layer groups**: `parent()` works, and `is_visible()` includes the parents. Group opacity and blend (header flag 2)
  were not checked.
- **User data**: text and colour only. Properties maps are not parsed.
- **Cel z-index**: ignored.
- **Cel bounds**: `Cel::image()` always returns a canvas-sized image, and there's no accessor for the cel's size, so
  the loader must crop to the opaque box itself.

## 3. Mapping

`aseprite-spike map fixture-rgba.aseprite` prototypes the spec's mapping on the question 4 fixture. It uses what
`pack.json` would declare: the `eyes` layer has the parts `eye-left` and `eye-right`.

```
anchor (from slice): Some((16, 31))
layer "eyes" part "eye-left": origin (12, 16)
layer "eyes" part "eye-right": origin (20, 16)
animation body.idle: frames [0, 1, 2] loop "loop"
  f0: RasterFrame { x: 6, y: 14, w: 20, h: 14, pivot: (10, 17), ms: 160 }
  f1: RasterFrame { x: 6, y: 15, w: 20, h: 14, pivot: (10, 16), ms: 120 }
  f2: RasterFrame { x: 6, y: 14, w: 20, h: 14, pivot: (10, 17), ms: 90 }
animation eyes.open: frames [0, 1] loop "pingpong"
  f0 eye-left: RasterFrame { x: 10, y: 14, w: 4, h: 5, pivot: (2, 2), ms: 160 } opaque=6
  ...
animation eyes.closed: frames [2] loop "once"
  f2 eye-left: RasterFrame { x: 10, y: 15, w: 4, h: 5, pivot: (2, 2), ms: 90 } opaque=2
  f2 eye-right: RasterFrame { x: 18, y: 14, w: 4, h: 5, pivot: (2, 2), ms: 90 } opaque=2
  poses.closed.eye-left = (12, 17) (origin (12, 16))
```

The rules used:
- **Tags**: `<layer>:<key>` splits at the first `:`. A tag without `:`, or one naming an unknown layer, gives a
  reason line.
- **Frame order**: direction sets the order (reverse runs backwards).
- **Loop mode**:
  - no repeat count: `loop` (`pingpong` for ping-pong);
  - a count of n: the frames are unrolled n times, then `once` holds the last frame. A ping-pong count counts each
    direction, so ping-pong with repeat 2 over frames 0..2 becomes 0,1,2,1,0.
- **Durations**: each frame's own duration.
- **Part origins**: the part slice's key at frame 0, origin plus pivot. A slice without a pivot falls back to its
  centre.
- **Part frames**: the layer's pixels inside the part's slice rectangle at that frame, placed by the slice's pivot.
- **Whole-layer frames**: the layer's cel cropped to its opaque box, with pivot = anchor minus the crop origin. Body
  bob therefore shows up as a pivot change (17 → 16 above).
- **Pose positions**: the slice key in effect on the tag's first frame. If it differs from the origin, it becomes
  `poses.<key>.<part>`.

Two choices the spec leaves open:
- A part's raster frame is the slice-rectangle crop of the layer, so one Aseprite layer can hold several parts.
- A pose comes from the slice key at the tag's first frame. Keys later inside the same tag are ignored, because poses
  are per channel value, not per frame.

## 4. Fixture writer

`src/writer.rs` writes the documented subset:
- the 128-byte header and the frame headers;
- palette 0x2019;
- layers 0x2004 (normal and group, optional UUIDs);
- cels 0x2005 (raw, linked, zlib);
- tags 0x2018, each followed by an empty user data chunk 0x2020;
- slices 0x2022, with keys and pivots.

It handles RGBA and indexed.

It is 145 lines (128 non-blank, non-comment) and uses only `flate2`. The fixture is 32×32, 2 layers (`body`, `eyes`),
3 frames (160, 120, 90 ms) and these tags:
- `body:idle` (0..2, forward);
- `eyes:open` (0..1, ping-pong);
- `eyes:closed` (2, repeat 1).

The slices are `eye-left` (keys at frames 0 and 2, pivot (2,2)), `eye-right` and `anchor`. Frame 0's body cel is
raw, frame 1's eyes cel is linked to frame 0, and the rest are zlib.

```
$ aseprite-spike fixture $SCRATCH/fx
wrote fixture-rgba.aseprite (2048 bytes)
wrote fixture-indexed.aseprite (1164 bytes)
wrote fixture-uuid.aseprite (2080 bytes)
wrote fixture-pingpong-reverse.aseprite (2048 bytes)
wrote fixture-group.aseprite (2076 bytes)
fixture-rgba.aseprite: 32x32 rgba, 3 frames ms=[160, 120, 90], palette=5
  layer 0 "body" image ...  layer 1 "eyes" image ...
  tag "body:idle" 0..=2 Forward repeat=None
  tag "eyes:open" 0..=1 PingPong repeat=None
  tag "eyes:closed" 2..=2 Forward repeat=Some(1)
  slice "eye-left" keys=[f0@(10, 14)(4, 5) pivot=Some((2, 2)), f2@(10, 15)(4, 5) pivot=Some((2, 2))]
  slice "eye-right" keys=[f0@(18, 14)(4, 5) pivot=Some((2, 2))]
  slice "anchor" keys=[f0@(16, 31)(1, 1) pivot=Some((0, 0))]
fixture-pingpong-reverse.aseprite: ERROR Invalid Aseprite input: Unknown animation direction: 3
fixture-group.aseprite: ... layer 0 "face" group ... layer 1 "body" image parent="face" visible=false ...
read back OK: fixture-rgba.aseprite
read back OK: fixture-indexed.aseprite
read back OK: fixture-uuid.aseprite
```

`asefile` accepts the RGBA, indexed, UUID and group files. The read-back asserts:
- the size, frame and layer counts, and the durations;
- each layer cel's bounding box;
- sample pixels, including the linked frame and the closed eyes.

It rejects only the ping-pong reverse file, which is valid per the spec.

Pitfalls, all handled in the writer:
- The frame size includes the 16-byte frame header, and the chunk size includes its 6-byte header. The header's file
  size covers everything.
- Write both chunk counts: the old WORD and the new DWORD.
- The cel header is 16 bytes: layer, x, y, opacity, type, then a z-index SHORT and 5 reserved bytes. Older documents
  show 7 reserved bytes; the size is the same.
- Cel data is a zlib stream (with its header), not raw deflate.
- A slice pivot is relative to the slice's origin, and key fields depend on the slice flags (9-patch, pivot).
- A user data chunk attaches to the last object read, so the per-tag user data must follow the tags chunk directly.
- Layer index is the order of layer chunks. Group children are marked by `child_level`, not by nesting.
- A tag's 3 deprecated colour bytes and the extra byte come before the name.
- Indexed: header byte 28 is the transparent index, and the palette must hold every index used.
- Chunk order differs from Aseprite's (tags and slices before cels). Both readers accept either order.

## 5. Grid sheets

`aseprite-spike grid` draws a 134×71 sheet: 5 columns × 3 rows of 24×20 cells, origin (4,2), gap (2,3). Each cell is
filled with a colour that encodes its index, and the gap is magenta. It computes
`origin + (i % columns, i / columns) * (frame + gap)` for every index and checks:
- every pixel inside each rectangle is its cell's colour;
- the pixel just past each edge is the gap or the sheet edge.

```
sheet 134x71
all 15 rects match their cells exactly; first [4, 2, 24, 20, 160], last [108, 48, 24, 20, 160]
frames [0,1,2,1] ms [100,200,300,400]: [[4, 2, 24, 20, 100], [30, 2, 24, 20, 200], [56, 2, 24, 20, 300], [30, 2, 24, 20, 400]]
cell outside: "cell 15 (4,71) is outside the 134x71 sheet"
zero frame: "non-positive frame size"
ms mismatch: "ms list has 2 entries for 3 frames"
negative index: "frame index -1 is negative"
```

The rule needs no row count. A sheet without a trailing gap works, because the check is `x + w <= sheet width`.
Besides the spec's three errors, the loader also needs `columns > 0`, a non-negative origin and gap, and a
non-negative index. Do these with checked arithmetic, since indices come from JSON.

## 6. Performance

Heap numbers come from a counting global allocator. Times are medians of 51 runs (11 for the large file).

A typical pixel pet was generated with the writer: 32×32, 6 layers, 64 frames, every cel a zlib RGBA image of runs
of colour, 77 KB on disk.

| typical.aseprite (77 KB) | `asefile` | direct reader |
|---|---|---|
| Parse | 2.51 ms | 2.15 ms (parse and decode to RGBA) |
| Decode 384 layer cels to canvas images and crop | 1.08 ms | — |
| Heap peak | 1.55 MB (0.87 MB during parse) | 0.84 MB |
| Kept after load | 0.79 MB parsed and 0.74 MB of cropped frames | 0.77 MB |

A file at the spec's limits: 130×120, 16 layers, 512 frames, 13.7 MB.

| limits.aseprite (13.7 MB) | `asefile` | direct reader |
|---|---|---|
| Parse | 235 ms, 257 MB heap held | 139 ms, 244 MB heap |
| Decode and crop | 374 ms more, peak 500 MB | — |
| With the spec's limits | no way to stop: everything is decoded inside `read()` | rejected before decoding: "file is 13673151 bytes, over the 8388608 limit" |

Small real files take 0.02 to 0.06 ms to parse. Load cost doesn't matter for a typical pet. Memory at the limits
does: 512 frames × 16 layers at the full canvas is about 500 MB, so the 64 MB decoded limit is what actually binds.

Hostile files (`aseprite-spike hostile`, run under `ulimit -v 4194304`):

| File | `asefile` | direct reader |
|---|---|---|
| `bomb.aseprite`: 262 KB, a 16×16 cel whose zlib data inflates to 256 MB | `Ok`, 229 ms, heap peak 896 MB | `Err(cel data is 1025 bytes, expected 1024)`, 0.1 ms, 0.33 MB |
| `hugecel.aseprite`: 949 bytes, a cel claiming 65535×65535 | process aborted: `memory allocation of 17179344900 bytes failed` (exit 134) | `Err(decoded art over the 67108864 byte limit)`, 0.0 ms |
| `hugecanvas.aseprite`: header 65535×65535, small cels | parses; the first `Cel::image()` aborts the same way | `Err(canvas 65535x65535 is outside the limits)` |
| `unknownchunk.aseprite`: one chunk retyped 0x2099 | `Err(Invalid or unsupported chunk type: 0x2099)` | `Ok` (skipped) |

A header check before calling `asefile` would catch the third case only. `asefile` sizes cel buffers from each cel's
own width and height with `Vec::with_capacity`, and inflates with an unbounded `read_to_end`.

## Direct reader

`src/reader.rs` reads:
- frames and durations;
- layers (normal and group; UUIDs skipped);
- cels (raw, linked, zlib), in RGBA, greyscale (converted) and indexed (through the palette, with the transparent
  index and the background flag);
- tags (all four directions, repeat);
- slices (keys, 9-patch skipped, pivots);
- palettes 0x2019, falling back to 0x0004.

It rejects tilemap layers and cels, tilesets, external files, non-square pixels, unknown colour depths and malformed
tags. It skips every other chunk: colour profile, user data, cel extra, mask, path, and future chunks.

Limits are checked before each allocation: file size, canvas side, frame count, layer count, and a decoded byte
budget charged per cel before inflating. Inflating is bounded to the expected size plus one byte.

`aseprite-spike compare` reads each file with both readers. Where both accept it, it checks they agree on:
- durations;
- layer names;
- tags (name and range);
- slices (every key's frame, rectangle and pivot);
- every normal-blend layer cel's canvas image.

```
indexed.aseprite: same frames, durations, layers, tags, slices; 12 layer cels, 0 px differ
layers_and_tags.aseprite: same frames, durations, layers, tags, slices; 20 layer cels, 0 px differ
linked_cels.aseprite: same frames, durations, layers, tags, slices; 9 layer cels, 0 px differ
slice_advanced.aseprite: same frames, durations, layers, tags, slices; 4 layer cels, 0 px differ
typical.aseprite: same frames, durations, layers, tags, slices; 384 layer cels, 0 px differ
cel_overflow.aseprite: direct="the file has a tileset; tilesets are not supported" asefile="Ok"
color-curve.aseprite: direct="Ok" asefile="Unsupported Aseprite feature: Embedded ICC color profiles ..."
fixture-pingpong-reverse.aseprite: direct="Ok" asefile="Invalid Aseprite input: Unknown animation direction: 3"
```

Across the 42 files that both accept (37 real files, 4 fixtures and the typical file), 501 layer cels match with 0
differing pixels. The 7 tilemap files are rejected by design. The ICC file and the ping-pong reverse fixture load only
in the direct reader. Its only dependency is `flate2`, which the workspace already has.

## Recommendation

- **Direct reader.** Write `aipet-sprite`'s native reader directly from the format document, starting from
  `src/reader.rs`: about 200 lines plus tests. Add no new crates; `flate2` is already in the lockfile. Reject
  tilemaps and tilesets (a pet has no use for them), and treat the colour profile as sRGB. `asefile` stays useful as
  a test oracle in the spike, not as a dependency.
- **Fixtures.** Generate them in the tests with the writer, about 130 lines. RGBA and indexed both work. Each
  edge case is a few lines: ping-pong reverse, repeat counts, linked cels, groups, UUIDs, unknown chunks, a zlib bomb
  and an oversized cel. Real-editor coverage comes from `asefile`'s MIT test files (44 files, 15 MB; a subset is
  enough). CI never needs Aseprite.

## Not verified

- No LibreSprite file: downloading one was not approved.
- No file from a current Aseprite (1.3.x): the test files all lack 1.3 features. The writer's UUID, z-index and
  ping-pong reverse cases follow the format document only.
- That the reference PNGs came from Aseprite itself.
- Group opacity and blend (header flag 2), and non-normal blend modes, in per-layer art.
- `ah-asefile`, the fork.
- Windows and macOS: everything ran on Linux only. The code is plain Rust and `flate2`.
