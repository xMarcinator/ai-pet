# Aseprite

A raster pack can be a native `.aseprite`/`.ase` file or an Aseprite sprite-sheet export, with an optional
`pack.json` beside it for the rest. The readers are `rust/crates/aipet-sprite/src/aseprite_file.rs` (native) and
`load.rs` (`aseprite`, `aseprite_native`). People's guide: `docs/MAKING-A-PET.md`, "Aseprite".

What the folder holds decides how it loads, in this order: an export (a `.json` other than `pack.json` with `frames`
and `meta`) wins; else the first `.aseprite`/`.ase` by name; else `pack.json` alone.

## Drawing in Aseprite

- **Tags** `<layer>:<value>`: `body:idle`, `eyes:blink`, `arms:wave-a`. Each becomes that layer's animation for
  that value. The layer is one declared in `pack.json`, or named after a channel (`eyes`, `arms`, `feet`, `prop`,
  `headphones`, `fx`, `facing`; `body` is the `state` channel). At least one tag.
- **Direction**: forward and reverse loop; ping-pong and ping-pong reverse become `pingpong`.
- **Repeat**: n plays the frames n times (a ping-pong counts each way) and holds the last; none loops.
- **Durations**: each frame keeps its own.
- **Slices**: `anchor` sets the anchor by its pivot. A slice named after a part (listed in that layer's `parts`
  in `pack.json`) gives the part's origin (its first key's pivot) and, from its key on each tag's first frame, the
  part's position for that value. Draw the part in the same place on every tag and move the slice; the pet moves
  the frame by the slice's move.
- **Raster parts**: one part per layer (`pixel-and-painted.md`), so an Aseprite layer naming several part slices
  skips the pack. One Aseprite layer per moving part.
- **Native files**: each Aseprite layer named after a pack layer is that layer's art, each frame cropped to what is
  drawn on it. RGBA, greyscale and indexed are read; colour profiles and unknown chunks ignored. Tilemap layers,
  tilesets, external files and non-square pixels are refused. At most 8 MB, 512 frames, 64 layers, 2048 px a side,
  64 MB decoded; the cropped frames must fit one 2048 px sheet.

## Exporting instead

File → Export Sprite Sheet:
- Layout: any sheet type; **trim** is fine (trimmed frames keep their place through `spriteSourceSize`).
- Output: the PNG, and **JSON Data** as **Array** (`json-array`), not Hash. Turn on **Tags** and **Slices** (meta).
  Keep the PNG beside the JSON; `meta.image` names it.
- The export is one flattened sheet: each tag's frames must hold only its layer's art. Draw each layer's values on
  frames of their own (hide the other layers for those frames), or use the native file instead.

From the command line:

```sh
aseprite -b pet.aseprite --sheet pet.png --data pet.json --format json-array --list-tags --list-slices
```

## `pack.json` beside the art

It supplies what Aseprite can't: `name` (default: the file's name), `kind` (default `pixel`), `pixel_size`
(default 1), `anchor` (when there is no `anchor` slice; default `[0, 0]`), `mirror`, `accent`, `glow` (plain
`#RRGGBB` or `#AARRGGBB` here), and per layer `name`, `channel`, `default`, `hit`, `glow`, `fallback` and `parts`
(a list of part ids, each with its slice). Without it, `state`, `arms` and `feet` layers take clicks and each layer's
default is the channel's resting value.

An Aseprite pack has no `palette`, `theme` or `themes` (they are not read), so it has no themes and theme files
can't attach to it. For a themed pixel pet, export the sheets and write a full `pack.json` with frames instead
(`pixel-and-painted.md`).

`avatars/samples/native/` is the small example: `sample.aseprite` and a `pack.json` that names the layers, their
channels, defaults and fallbacks.

## Checking

`AiPet --check-pack <folder>` reads the export or the native file exactly as the pet does. Reasons specific to
Aseprite are in `limits-and-reasons.md`, "Aseprite". Then build the review page from the loaded pet: screenshot the
app wearing it at each state, or render the frames from the sheet, since the generator pages in
`avatars/reference/` don't read Aseprite files.
