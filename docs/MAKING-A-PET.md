# Making a pet

This guide is for people who want to draw their own pet for AiPet. A pet is a **pack**: a folder with a manifest
(`pack.json`) and its art. You can draw the art as pixel-art sheets, as smooth painted PNGs, or as vector shapes
written straight into the manifest, and you can bring it straight from Aseprite. You don't need to write any code.

Packs are read by the Rust build of AiPet (in `rust/`). The .NET app that the 0.1.0 packages ship reads only the
older avatar `.json` files (see [Legacy avatars](#legacy-avatars)).

- [Quick start](#quick-start)
- [How a pet works](#how-a-pet-works)
- [The manifest](#the-manifest)
- [Kinds, the art box and the anchor](#kinds-the-art-box-and-the-anchor)
- [Raster frames and grid sheets](#raster-frames-and-grid-sheets)
- [Vector shapes](#vector-shapes)
- [Part motion](#part-motion)
- [Themes](#themes)
- [Aseprite](#aseprite)
- [Rive](#rive)
- [Limits](#limits)
- [Checking a pack](#checking-a-pack)
- [Legacy avatars](#legacy-avatars)
- [The built-in pets and their generators](#the-built-in-pets-and-their-generators)

## Quick start

1. Open Settings → Avatars and click **Open folder**. This opens the avatars folder in the data folder
   (`%LOCALAPPDATA%\AiPet\avatars` on Windows, `~/.local/share/AiPet/avatars` on Linux), and makes it first if it is
   missing.
2. Make a folder in it for your pet, say `blob/`, and save the [example below](#the-manifest) in it as `pack.json`.
   Or copy one of the samples in the repository's [avatars/samples/](../avatars/samples/).
3. Check it: `AiPet --check-pack <folder>` prints `OK Blob (vector, 4 layers, 0 frames, themes Mint, Plum)`, or what
   is wrong with it.
4. Click **Reload** on the Avatars page. The pet gets a tile of its own: click it to wear it.

A pack that doesn't load is listed under "Your own avatars" with the reason, one line each, and the other pets load
as usual. A pack loads completely or not at all.

## How a pet works

The pet's behaviour is the same for every pet. Each frame it works out a **pose**: what each part of the pet should
show (the mood, the eyes, the arms, and so on), plus the whole-body motion (bobbing, hopping, squash and stretch,
breathing and the ground shadow). The pack only says how to draw each value. That is why every pet follows the
pointer with its eyes, blinks, waves, holds the laptop, wears headphones while music plays, and shows its effects.

A pack is a list of **layers**, drawn in the order the manifest gives them, bottom first. Each layer is bound to one
**channel** of the pose, and has something to draw for some of that channel's values.

| Channel | Values |
|---|---|
| `state` | `idle`, the idle acts `idle-look`, `idle-hop`, `idle-stretch`, `idle-wiggle` and `idle-tap`, `sleep`, `thinking`, `working`, `attention`, `done`, and `listen` (music plays while nothing else is going on) |
| `eyes` | `open`, `closed` (asleep), `blink`, `happy`, `squint` (poked), `wide` (held), `up` (thinking, or the pointer is above) and `down` (reading the laptop). The eyes also have a look direction: see [`look`](#part-motion). |
| `arms` | `rest`, `raised` (held, or stretching), `wave-a` and `wave-b` (waving), `run-a` and `run-b` (dragged), `hold` (at the laptop) and `lens` (holding the magnifier) |
| `feet` | `stand`, `tap`, `dangle-a` and `dangle-b` (held), `run-a` and `run-b` (dragged) |
| `prop` | `none`, `laptop` (working) and `lens` (searching or reading) |
| `headphones` | `on` and `off` |
| `fx` | `none`, `zs` (asleep), `dots` (thinking), `notes` (listening), `alert` (attention), `sparkles` (done, or poked) and `dust` (running). Sparkles can come on top of another effect: `zs+sparkles`, `dots+sparkles`, `notes+sparkles`, `alert+sparkles` and `dust+sparkles`. |
| `facing` | `left` and `right` |

Several layers may share a channel: a pet can have a `body` and a `mouth` layer both bound to `state`, or one effects
layer per effect.

**Defaults and fallbacks.** Each layer has a `default`, which it must have something for. A value the layer has
nothing for walks the layer's `fallback` map to the next value, for at most 8 steps; a value the map doesn't name
goes to the default. A combined effect such as `zs+sparkles` that the layer doesn't have, and that its fallbacks
don't name, falls back to its last part (`sparkles`). A fallback of `null` means the layer draws nothing for that
value. That is how a headphones layer draws nothing while they are `off`, and how an effects layer for the Zs draws
nothing for every other effect:

```json
"fallback": { "off": null }
"fallback": { "zs+sparkles": "zs", "none": null, "dots": null, "notes": null, "alert": null,
              "sparkles": null, "dust": null }
```

You only need to draw what your pet should show differently. A pet with just a body and eyes is fine: the arms,
feet, props, headphones and effects simply aren't drawn.

**Timing.** An animation starts when its channel's value changes, so a wave starts at its first frame. An animation
with `"clock": "pet"` runs on the pet's own clock instead, which keeps effects in step however often the value
changes. When the system asks for reduced motion (or `AIPET_REDUCE_MOTION=1` is set), every animation shows its
first frame, and the pet doesn't bob, squash or play idle acts.

**Facing.** The art is drawn facing right. When the pet faces left (it is dragged to the left), a layer is mirrored
if the pack's `mirror` is `flip`, unless the layer has a `mirror` of its own. A layer may also give a value's
left-facing frames as `<value>@left` (for example `run-a@left`), which are drawn as they are, unmirrored.

## The manifest

This is a complete pack: a vector pet with a body that changes when it sleeps, eyes that follow the pointer, hands
that wave and type, Zs while it sleeps, and two colour themes. Save it as `pack.json` in a folder of its own and it
loads as it is.

```json
{
  "format_version": 1,
  "name": "Blob",
  "kind": "vector",
  "pixel_size": 1,
  "anchor": [65, 110],
  "mirror": "flip",
  "accent": "$body",
  "glow": "#7FEFFF",
  "palette": { "body": "#7FD88F", "outline": "#1E4A2A", "eyes": "#10241A" },
  "theme": "Mint",
  "themes": { "Plum": { "body": "#C08FD8", "outline": "#3E1E4A" } },
  "layers": [
    {
      "name": "body",
      "channel": "state",
      "hit": true,
      "default": "idle",
      "shapes": {
        "idle": { "d": "M 25 74 A 40 34 0 1 1 105 74 A 40 34 0 1 1 25 74 Z",
                  "fill": "$body", "stroke": "$outline", "stroke_width": 3 },
        "sleep": { "d": "M 23 80 A 42 28 0 1 1 107 80 A 42 28 0 1 1 23 80 Z",
                   "fill": "$body", "stroke": "$outline", "stroke_width": 3 }
      }
    },
    {
      "name": "eyes",
      "channel": "eyes",
      "hit": true,
      "default": "open",
      "fallback": { "blink": "closed", "squint": "happy" },
      "parts": [ { "id": "eye-left", "origin": [52, 68] }, { "id": "eye-right", "origin": [78, 68] } ],
      "shapes": {
        "open": { "d": "M -4 0 A 4 5 0 1 0 4 0 A 4 5 0 1 0 -4 0 Z", "fill": "$eyes" },
        "closed": { "d": "M -5 0 L 5 0", "stroke": "$eyes", "stroke_width": 2, "linecap": "round" },
        "happy": { "d": "M -5 2 Q 0 -4 5 2", "stroke": "$eyes", "stroke_width": 2, "linecap": "round" }
      },
      "look": { "center": [0, 0], "left": [-3, 0], "right": [3, 0], "up": [0, -2], "down": [0, 2] }
    },
    {
      "name": "arms",
      "channel": "arms",
      "hit": true,
      "default": "rest",
      "parts": [ { "id": "hand-left", "origin": [28, 88], "shape": "hand" },
                 { "id": "hand-right", "origin": [102, 88], "shape": "hand" } ],
      "shapes": { "hand": { "d": "M -6 0 A 6 5 0 1 0 6 0 A 6 5 0 1 0 -6 0 Z",
                            "fill": "$body", "stroke": "$outline", "stroke_width": 2 } },
      "poses": {
        "rest": {},
        "raised": { "hand-left": [22, 56], "hand-right": [108, 56] },
        "wave-a": { "hand-right": [110, 54] },
        "wave-b": { "hand-right": [114, 62] }
      },
      "fallback": { "run-a": "raised", "run-b": "raised", "lens": "rest" },
      "transition_ms": 140,
      "animations": {
        "hold": { "parts": ["hand-left", "hand-right"], "phase": [0, 0.5], "loop": "loop",
                  "keys": [ [0, 0, 110], [0, -1.5, 110] ] }
      }
    },
    {
      "name": "zs",
      "channel": "fx",
      "default": "zs",
      "fallback": { "zs+sparkles": "zs", "none": null, "dots": null, "notes": null, "alert": null,
                    "sparkles": null, "dust": null },
      "parts": [ { "id": "z-1", "origin": [100, 46], "shape": "z" },
                 { "id": "z-2", "origin": [108, 36], "shape": "z" } ],
      "shapes": { "z": { "d": "M -3 -3 H 3 L -3 3 H 3", "stroke": "#C9D0FF", "stroke_width": 1.6,
                         "linecap": "round", "linejoin": "round" } },
      "animations": {
        "zs": { "parts": ["z-1", "z-2"], "phase": [0, 0.5], "loop": "loop",
                "keys": [ { "dy": 0, "ms": 300 }, { "dx": 1, "dy": -3, "opacity": 0.7, "ms": 300 },
                          { "dx": 2, "dy": -6, "opacity": 0.4, "ms": 300 } ] }
      }
    }
  ]
}
```

The top-level keys:

| Key | What it is |
|---|---|
| `format_version` | `1`. A pack with a newer version than the app reads is skipped. |
| `name` | The pet's name, as the Avatars page shows it and `config.json` saves it. A built-in pet wins a name; of two of your own with one name, the first in sorted folder order loads and the other is skipped. |
| `kind` | `pixel`, `painted` or `vector` (see below). |
| `pixel_size` | How big one pixel of the art is, in logical px: 5 for the 26 × 24 grid of the original pets, 1 for vector art. |
| `anchor` | `[x, y]`: the point of the art that stands on the ground shadow, in the art's own pixels. |
| `mirror` | `flip` (the default) or `none`. |
| `accent` | The pet's accent colour, which Settings and the menu take on while the pet is worn. A colour, or `$name` for a palette colour. |
| `glow` | The colour of the glow around glowing layers. A colour, or `$name`. |
| `palette`, `theme`, `themes` | Named colours and the pet's themes (see [Themes](#themes)). Optional. |
| `layers` | The layers, bottom first. 1 to 16 of them. |

Each layer has:

| Key | What it is |
|---|---|
| `name` | The layer's name. For a raster pack made from Aseprite, it is also the Aseprite layer or tag prefix. |
| `channel` | One of the channels above. |
| `default` | The value it shows when it has nothing for the pose's value, and which it must have. |
| `fallback` | `{value: next value or null}`. Optional. |
| `hit` | Whether its pixels take clicks and drags. Optional, `false` by default. The pet always takes clicks over an ellipse in the middle of the sprite as well, so a pet with gaps can still be grabbed. |
| `glow` | Whether it glows: the pet draws a soft halo in the `glow` colour around it, as it does around the eyes and the laptop screen of the original pets. An `fx` layer can't glow. |
| `mirror` | `flip` or `none`, where the layer differs from the pack. |
| `themed` | For a pixel pack: whether a colour theme recolours this layer. Every layer is themed except the `fx` ones, unless it says. |
| `animations` | Raster frames per value, or [part animations](#part-motion). |
| `parts`, `shapes`, `look`, `poses`, `transition_ms` | Vector shapes and part motion (below). |

Keys the app doesn't know are ignored, so a pack can carry notes of its own. The exception is a vector shape, whose
keys are checked, so that a misspelt `stroke-width` is reported instead of silently drawing nothing.

## Kinds, the art box and the anchor

Every pet fits the same **art box**: 130 × 120 logical px, plus a 10 px margin on every side for glows and things
that stick out. The window around the pet never changes size, so art that is larger than the box and its margin is
refused. The pet's ground is near the bottom middle of the box, at (65, 110).

- **pixel**: pixel-art PNG sheets. They are drawn with nearest-neighbour scaling and snapped to whole device pixels,
  so they stay crisp at every display scale. `pixel_size` is the size of one art pixel: the built-in Sprout and Hood
  are 26 × 24 pixels at 5 px each, and the Hamster (pixel) is 3 px a pixel. The `anchor` is in art pixels:
  `[13, 22]` for Sprout and Hood.
- **painted**: smooth, high-resolution PNG sheets, drawn with smooth scaling. Use a `pixel_size` below 1 for art drawn
  at more than one pixel per logical px: Pip (painted) is drawn at twice the size, with a `pixel_size` of 0.5 and an
  `anchor` of `[130, 220]`. A painted pack has no named colours; its themes are [sheet themes](#themes).
- **vector**: shapes as path data in the manifest, drawn smoothly at any size. `pixel_size` is 1, and coordinates
  and the anchor are in the art box's logical px (`[65, 110]`, as every built-in vector pet has).

## Raster frames and grid sheets

A pixel or painted layer's `animations` map each value to frames on a PNG sheet in the pack's folder:

```json
"idle": { "sheet": "body.png", "frames": [[0, 0, 26, 24, 400], [26, 0, 26, 24, 400]], "loop": "loop" }
```

- A frame is `[x, y, w, h, duration_ms]`: a rectangle on the sheet, and how long it shows (1 ms to 60 s).
- `loop` is `loop` (the default), `once` (play once and hold the last frame) or `pingpong` (forwards, then back).
- `clock` is `change` (the default: from when the value changed) or `pet`.
- A frame's top-left corner sits on the art box's top-left corner. `pivot`, one `[x, y]` for every frame or a list
  with one per frame, is the frame pixel that goes there instead: a pivot of `[-4, -6]` draws the frame 4 pixels in
  and 6 down. Use it to keep small frames small.
- Fully transparent pixels draw nothing. A pixel in a transparent colour other than `#00000000` (the bake writes
  `#00000001`) still covers what is under it, so a layer can cut a hole in the one below.
- Several layers and animations may share one sheet.

Instead of a rectangle per frame, an animation can name a **grid** on its sheet and list cell numbers:

```json
"idle": { "sheet": "body.png", "grid": { "frame": [26, 24], "origin": [0, 0], "gap": [0, 0], "columns": 3 },
          "frames": [0, 1, 2, 1], "ms": 160, "loop": "loop" }
```

Cell `i` is in column `i % columns` and row `i / columns`, counted from `origin` with `gap` pixels between cells
(both are optional and 0 by default). `ms` is one duration for every frame, or a list with one per frame. A sheet can
mix grid animations and explicit frames. [avatars/samples/basic/](../avatars/samples/basic/) and
[avatars/samples/grid/](../avatars/samples/grid/) are the same small pet drawn both ways.

The eyes' [`look`](#part-motion) table moves a raster eyes layer by whole art pixels.

## Vector shapes

A vector layer's `shapes` map a name to a drawing: one shape, or a list of shapes drawn bottom to top.

```json
"shapes": {
  "open": { "d": "M -4 0 A 4 5 0 1 0 4 0 A 4 5 0 1 0 -4 0 Z", "fill": "$eyes" },
  "closed": { "d": "M -5 0 L 5 0", "stroke": "$eyes", "stroke_width": 2, "linecap": "round" }
}
```

| Key | What it is |
|---|---|
| `d` | Path data in SVG's `d` syntax: `M L H V C S Q T A Z`, absolute and relative. |
| `fill`, `stroke` | `#RGB`, `#RRGGBB`, `#RRGGBBAA`, `none` (the default), or `$name` for a palette colour. |
| `stroke_width` | 0 to 100, 1 by default. |
| `linecap` | `butt` (the default), `round` or `square`. |
| `linejoin` | `miter` (the default), `round` or `bevel`. |
| `opacity` | 0 to 1, 1 by default. |

A shape named after a value of the layer's channel is what the layer draws for that value. A layer without `parts`
draws its shapes in the art box's coordinates. A layer with parts draws each part's shape around the part's
origin, in coordinates centred on (0, 0): the part's own `shape` if it names one, else the shape for the value.
Each expression or pose is a drawing of its own, never a scaled copy.

Vector packs load nothing from outside the manifest: there are no SVG files, images, fonts or `url(…)` paints.

## Part motion

Parts say where a layer's movable pieces are and how they move: the eyes, the hands, the feet, a lens, a caret, each
Z or sparkle. This is how the Hamster's arms swing from the shoulder and Pip's sprout sways.

- **`parts`**: `{"id", "origin", "shape"}` each. The `id` is unique in the pack, and the `origin` is a point in the
  art box. `shape` is optional (see above).
- **`look`**: for an `eyes` layer only. An offset `[dx, dy]` for each direction the eyes look: `center`, `up`,
  `up-right`, `right`, `down-right`, `down`, `down-left`, `left` and `up-left`, plus the far ones `far-left`,
  `far-right`, `up-far-left`, `up-far-right`, `down-far-left` and `down-far-right`, which the pet uses when the
  pointer is well to one side. A far direction the table lacks takes the near one, other missing directions take the
  nearest one given, and then `center`. Without a table, the eyes stare ahead.
- **`poses`**: for each value, a target position for some or all parts. Parts it doesn't list stay at their origin
  (`"rest": {}` keeps every part there). When the value changes, each part moves from where it is to its target over
  `transition_ms` (0 to 10 000), eased; 0 makes it jump.
- **`animations`** with `keys`: keyframes on top of the pose's position, for one `part` or a list of `parts`.
  - A key is `[dx, dy, ms]`, or an object with any of `dx`, `dy`, `rotate` (degrees, about the part's origin),
    `scale` and `opacity`, and `ms` (1 to 10 000). `rotate`, `scale` and `opacity` apply to vector parts only; a
    raster part takes a key's `dx` and `dy`.
  - Each key holds for its `ms`; the pet doesn't tween between keys, so use more keys for smoother motion. At most 64
    keys.
  - `loop` is `loop`, `once` or `pingpong`, as for frames.
  - `phase` (0 to 1), one for all parts or one each, starts each part that far into the loop, so two hands can bob
    out of step.

A part's place each frame is its pose target (on its way there, during a transition), plus its keyframe, plus the
look offset for the eyes. Then the whole body's motion moves it (breathing, squash, the hop), and last it is
mirrored when the pet faces left. Parts therefore ride along with the body.

Every number is checked when the pack loads: finite numbers only, positions inside the art box and its margin,
offsets no larger than the box, durations in range, and part ids that exist. A key, pose or animation that names an
unknown part, and a part id used twice, skip the pack with the reason.

**Raster parts.** In a pixel or painted pack, a layer moves as one piece. It has at most one part, and its frame is
drawn where its pivot puts it, moved by the part's place less its origin: the pose's position (eased during a
transition), the key's `dx` and `dy`, and for the eyes the look offset. A pixel pack moves by whole art pixels, a
painted pack by logical px. A raster layer with two or more parts skips the pack, since its one frame can't move
two ways; give each moving part a layer of its own (`hand-left` and `hand-right`, say, both on the `arms` channel).
An eyes layer without parts still moves by its `look` table.

A value may only move parts: give it a pose or a part animation and no frames, and the layer draws its fallback's
frames (else its default's) where the pose puts the part. A vector layer does the same with shapes. In an
Aseprite file the part's slice gives the poses: draw the part in the same place on every tag, and move the slice
on each tag's first frame to where the part should go, since the pet moves the frame by the slice's move.

## Themes

A theme is the same pet in other colours (or, for a raster pack, drawn from other sheets). The Avatars page shows
the worn pet's themes as pills under the tiles, and `config.json` keeps the one you pick as `AvatarTheme`.

**Colour themes.** The manifest's `palette` names the pet's colours, which are its default theme, and `theme` names
that theme (it is "Default" otherwise). `themes` maps each other theme's name to the colours it changes:

```json
"palette": { "body": "#7FD88F", "outline": "#1E4A2A", "eyes": "#10241A" },
"theme": "Mint",
"themes": { "Plum": { "body": "#C08FD8", "outline": "#3E1E4A" } }
```

- In a **vector** pack, shapes, `accent` and `glow` use the palette as `$body`, `$outline`, and so on.
- In a **pixel** pack, the palette names exact colours in the sheets, and a theme swaps those pixels (keeping their
  alpha) in every themed layer. Every layer is themed except the `fx` ones; a layer can say `"themed": true` or
  `false`. No two names may share a colour. `accent` and `glow` may be `$name` here too.
- A **painted** pack can't have colour themes, since its colours are blended.

**Sheet themes.** A raster theme can draw some of the pack's sheets from other files instead. Each replacement is
the same size as the sheet it replaces, so the frames still line up:

```json
"themes": { "Night": { "sheets": { "body.png": "body-night.png" } } }
```

A theme changes colours or sheets, never both. Pip (painted)'s Lilac theme is a sheet theme.

**Your own themes.** You can add a theme to any pet with named colours, the built-in ones included, without
touching its pack. On the Avatars page, **New theme** opens an editor on the worn pet in the theme it has on: give
it a name, change any colour, and **Save**. **Edit** and **Delete** work on your own themes only; a pet's own themes
come with it. Each is a file in the avatars folder's `themes/` folder:

```json
{
  "pet": "Hood (vector)",
  "theme": "Red",
  "palette": { "body": "#441010", "hi": "#FF4A3A", "eye": "#FFB15C", "glow": "#FF5A2B" }
}
```

`pet` is the pet's name, `theme` a name it doesn't have yet, and `palette` some of its named colours. You can write
these files by hand too. [avatars/samples/themes/hood-red.json](../avatars/samples/themes/hood-red.json) is one;
copy it into `themes/` and click **Reload**. A theme file that doesn't fit its pet (no pet by that name, a colour it
doesn't have, a name it already has) is listed with the reason, like a pack. A pet has at most 64 themes, its own and
yours together.

## Aseprite

A raster pack can come straight from [Aseprite](https://www.aseprite.org/), either as the `.aseprite` file itself
or as an export.

**Tags** name the animations: a tag `<layer>:<value>`, such as `body:idle`, `eyes:blink` or `arms:wave-a`, becomes
that layer's animation for that value.
- The tag's direction (forward, reverse, ping-pong, ping-pong reverse) becomes the loop: forwards or backwards on a
  loop, or `pingpong`.
- A repeat count plays the frames that many times (a ping-pong counts each way), then holds the last frame.
  Without one, the animation loops.
- Each frame keeps its duration.
- Every tag must name a layer: one in `pack.json`, or one named after a channel (`body` is the `state` channel).

**Slices** place things:
- a slice named `anchor` sets the anchor, through its pivot;
- a slice named after a part gives that part's origin (its first key's pivot) and, on each tag's first frame, the
  part's position for that value.

**The `.aseprite` file.** Put the file (`.aseprite` or `.ase`) in the pack folder. Each Aseprite layer named after a
pack layer (`body`, `eyes`, `arms`, …) is that layer's art, and each frame is cropped to what is drawn on it. RGBA,
greyscale and indexed files are read; colour profiles and unknown chunks are ignored. Tilemap layers, tilesets,
external files and non-square pixels are refused. The file is at most 8 MB, 512 frames and 64 layers, and its size
is checked before anything is decoded. [avatars/samples/native/](../avatars/samples/native/) is a small example.

**An export.** Export the sprite sheet with its data as **Array** (`json-array`, not Hash), with tags and slices on,
and keep the PNG beside the JSON. The export is one flattened sheet, so each tag's frames are the art of its layer
alone: draw each layer's values on frames of their own. Trimmed frames keep their place. If a folder has both an
export and an `.aseprite` file, the export is used.

**`pack.json` beside the art** gives the rest: `name` (the file's name otherwise), `kind` (`pixel` by default),
`pixel_size` (1 by default), `anchor` (when there is no `anchor` slice), `mirror`, `accent`, `glow`, and for each
layer `name`, `channel`, `default`, `hit`, `glow`, `fallback` and `parts` (a list of part ids, each with its slice).
Without it, the `state`, `arms` and `feet` layers take clicks, and each layer's default is the channel's resting
value (`idle`, `open`, `rest`, `stand`, …) when it has one.

## Rive

Rive packs aren't supported yet.

## Limits

A pack is skipped, with the reason, when it breaks any of these. They keep a broken or hostile pack from slowing
down or crashing the pet.

| What | Limit |
|---|---|
| `pack.json`, an Aseprite export's JSON, a theme file | 256 KB, nested at most 64 deep |
| A PNG sheet | 8 MB, 2048 px a side, checked from its header before it is decoded |
| A pack's sheets, decoded | 64 MB in all |
| An `.aseprite` file | 8 MB, 512 frames, 64 layers, 2048 px a side, 64 MB decoded |
| Layers | 1 to 16 |
| Frames | 512 in the pack |
| Parts | 256 in a vector pack; one per layer in a raster pack |
| Frame durations | 1 ms to 60 s |
| Part keys | 64 per animation, 1 ms to 10 s each; `transition_ms` 0 to 10 s; `phase` 0 to 1 |
| Numbers | finite, within ±10 000 |
| Part positions | inside the art box and its 10 px margin |
| Raster frames | no larger than the art box and its margin at the pack's `pixel_size` |
| Fallbacks | at most 8 steps, and no loops |
| Strokes | width 0 to 100, opacity 0 to 1 |
| Themes | 64 per pet |

Files are named relative to the pack's folder and must stay inside it: absolute paths, `..`, links, junctions and
device names (`CON`, `NUL`, …) are refused.

## Checking a pack

```sh
AiPet --check-pack <pack folder>
AiPet --check-pack <theme file>
AiPet --check-pack <legacy avatar file>
```

It loads the pack exactly as the pet does, with the same limits, and prints one line:
- `OK <name> (<kind>, <layers> layers, <frames> frames)`, with `, themes <names>` when the pet has themes, and exits
  0;
- or the reason the pack doesn't load, and exits 1.

A theme file is checked against its pet: a built-in one, or one in the avatars folder the `themes/` folder is in, with
the other theme files there. It prints `OK <pet> in <theme> (a theme)`. A `.json` file outside a `themes/` folder that
doesn't name a `pet` is a legacy avatar, and prints `OK <name> (a legacy avatar)`. The check never starts the pet and never
writes anything.

On Windows, run it from a terminal: `AiPet.exe --check-pack <folder>`.

## Legacy avatars

The pets of 0.1.0 were avatar `.json` files: a name, a few ellipses in the 26 × 24 grid, a shading style and a
palette. A `.json` file in the avatars folder is still read as one, and turned into a pixel pack when it loads, so
it looks and behaves as it did before packs, with every state, prop and effect.
[avatars/hood-green.json](../avatars/hood-green.json) is one: the green Hood, which is now also the built-in Hood's
Green theme. A `config.json` that names "Hood (green)" wears the Hood in Green, unless that file is in your avatars
folder.

| Key | What it is |
|---|---|
| `name` | The avatar's name; the file's name otherwise. |
| `shapes` | Ellipses `[cx, cy, rx, ry]` in grid cells. The body is every cell whose centre is in one. |
| `shading` | `soft` (the default) or `rim`. |
| `blush` | `true` (the default) or `false`. |
| `faceTop`, `faceBottom` | The face panel's rows, 9 and 14 by default. |
| `palette` | `outline`, `deep`, `shade`, `body`, `hi`, `spec`, `blush`, `screen`, `bezel`, `glint`, `eye`, `eyeHi`, `eyeLo`, `glow` and `accent`, as `#RRGGBB` or `#AARRGGBB`. A missing colour is Sprout's. |

## The built-in pets and their generators

AiPet ships eight pets: Sprout and Hood (in Blue, with a Green theme), baked once from the original pixel pets;
Pip (in Peach, with a Lilac theme) as a vector pack and Pip (painted), painted from it; Hood (vector) (in Green, with
a Blue theme), Hamster and Sprout (vector), drawn as vector packs; and Hamster (pixel). Their packs are in
[rust/crates/aipet-sprite/packs/](../rust/crates/aipet-sprite/packs/), embedded in the app.

The vector and pixel pets beyond Sprout and Hood are written by Python scripts in
[avatars/reference/](../avatars/reference/), which are a good starting point for a pet of your own:
- `hood_gen.py`, `hamster_gen.py` and `sprout_gen.py` draw every state as SVG (the reference drawings, review pages
  and state sheets beside them), and with `--pack` write the vector pack from that drawing through `vector_pack.py`;
- `pip_gen.py` writes the vector Pip;
- `hamster_pixel_gen.py` writes the Hamster (pixel);
- `pet_states.py` is the list of states and alternating frames every built-in pet draws.
