# Pixel and painted packs

Raster packs draw frames from PNG sheets in the pack folder. People's guide: `docs/MAKING-A-PET.md`, "Kinds, the art
box and the anchor", "Raster frames and grid sheets" and "Part motion" (raster parts).

## The two kinds

| | `pixel` | `painted` |
|---|---|---|
| Scaling | Nearest neighbour, snapped to whole device pixels | Smooth |
| `pixel_size` | Logical px per art pixel: 5 for a 26 × 24 grid (Sprout, Hood), 3 for the Hamster (pixel)'s 43 × 40 | Below 1 for art drawn larger: 0.5 for art at twice the size (Pip (painted)) |
| `anchor` | In art pixels: `[13, 22]` (Sprout, Hood), `[21, 37]` (Hamster (pixel)) | In art pixels: `[130, 220]` at 0.5 |
| Colour themes | Palette of exact sheet colours, swapped per theme | None; sheet themes only |

The art box is 130 × 120 logical px with a 10 px margin; a frame wider than 150 or taller than 140 logical px (its
pixel size × `pixel_size`) is refused. Pick `pixel_size` so the whole pet fits: 130 / `pixel_size` art pixels
across.

## Sheets

- RGBA, RGB, grey or grey-and-alpha PNG, 8 bits a channel; indexed and fewer-bit images are expanded on loading,
  16-bit ones are refused. At most 8 MB, 2048 px a side; the pack's sheets together at most 64 MB decoded
  (width × height × 4).
- A file name relative to the pack folder, inside it: no absolute paths, `..`, links or device names.
- Fully transparent pixels draw nothing. A pixel in a transparent colour other than `#00000000` (the bake writes
  `#00000001`) still covers what is under it: a layer can cut a hole in the one below.
- Several layers and animations may share a sheet; one animation uses one sheet.
- For a pixel pack with themes, every colour that should change must be exactly a palette colour in the sheets, and
  no two palette names may share a colour.

## Frames

Explicit: `"frames": [[x, y, w, h, ms], …]`, whole numbers, each rectangle inside the sheet, `ms` 1 to 60 000.

Grid: `"grid": {"frame": [w, h], "origin": [x, y], "gap": [x, y], "columns": n}` with `"frames": [cell, …]` and
`"ms"`: one number, or one per frame. Cell `i` is column `i % columns`, row `i / columns`, from `origin` with `gap`
between cells (both optional, default 0). The cell must lie inside the sheet. A sheet may mix both forms.
`avatars/samples/basic/` and `avatars/samples/grid/` are the same pet drawn both ways.

`loop`: `loop`, `once` (hold the last frame), `pingpong`. `clock`: `change` (from the value change) or `pet` (the
pet's clock, for effects that should stay in step). At most 512 frames in the pack.

**Placement.** A frame's top-left sits on the art box's top-left, moved by its `pivot`: one `[x, y]` or one per
frame, the frame pixel placed there (negative values draw it further in). With a part, the pivot pixel is placed on
the part's origin instead. The simplest layout, which `hamster_pixel_gen.py` uses: every frame is the whole box
(43 × 40 cells), frames side by side on the sheet, no pivots.

`<value>@left` frames, if a layer has them, are drawn unmirrored while the pet faces left (Sprout's and Hood's running
arms and feet).

## Raster part motion

A raster layer moves **as one piece**:

- **At most one part per raster layer.** Two or more parts skip the pack ("a raster layer moves as one piece; give
  each moving part a layer of its own"): make `hand-left` and `hand-right` two layers on the `arms` channel.
- Its frame is drawn moved by its part's place less its origin: the pose target (eased over `transition_ms`), plus
  the key's `dx` and `dy`, plus the look offset for an eyes layer.
- Keys take `dx` and `dy` only; `rotate`, `scale` and `opacity` are vector-only (draw rotated frames instead).
- A pixel pack moves by whole art pixels, a painted pack by logical px. Part positions are logical px.
- An eyes layer without parts still moves by its `look` table (logical px, whole art pixels).
- A value may only move parts: with a pose or part animation and no frames, the layer draws its fallback's frames
  (else the default's) at the pose. Give it frames of its own only when it should look different.

## The pixel generator

`avatars/reference/hamster_pixel_gen.py` is the model: the reference image read into a grid of palette letters
(`HAMSTER`), split into layers (body, arms, feet, eyes), plus what the reference lacks (raised, waving, typing and
running arms, the laptop, the magnifier, the headphones, the effects) drawn in its palette and outline; PNGs written
with `zlib` and `struct` (no Pillow needed); the manifest with a palette for themes. Run it with
`python3 avatars/reference/hamster_pixel_gen.py --pack <folder>`.

For a painted pack, the art usually comes from a painting program or a renderer; Pip (painted) is baked from the
vector Pip by the Rust code (`rust/crates/aipet-sprite/src/painted.rs`), which a new pet should not copy. Write the
manifest with a short script that lists the sheets and frames, rather than by hand.
