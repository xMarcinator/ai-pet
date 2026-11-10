# The manifest (`pack.json`)

The loader is `rust/crates/aipet-sprite/src/manifest.rs` (reading) and `pack.rs` (checking). This file is what they
accept; `docs/MAKING-A-PET.md` ("The manifest", "Part motion") explains the same keys for people.

## Top level

| Key | Required | Value |
|---|---|---|
| `format_version` | yes | `1`. Newer than the app reads: the pack is skipped. |
| `name` | yes | This drawing's name, unique among all packs: the pet's name for its main style, else like `Hood (vector)`. A built-in wins a name; of two user packs with one name, the first folder in sorted order loads and the other is skipped ("a pet named … is already loaded"). |
| `pet` | no | The pet this pack is a style of, where not its `name`. A pack inside a pet folder (`avatars/<Pet>/<style>/`) is that pet's unless it says. One pet shows once on the Avatars page, its packs as styles; a pack repeating a pet's style, or named as another pet, is skipped. |
| `style` | no | The style's label, where not the kind's (Pixel, Vector, Painted); needed for two styles of one kind. |
| `kind` | yes | `pixel`, `painted` or `vector`. (`rive` is not read: see `rive.md`.) |
| `pixel_size` | yes | Logical px per art pixel, above 0 and at most 130. 1 for vector; 5 for a 26 × 24 grid; below 1 for painted art drawn at more than 1 px per logical px. |
| `anchor` | yes | `[x, y]`: the art point that stands on the ground shadow, in art pixels (vector: logical px, `[65, 110]`). |
| `mirror` | no | `flip` (default) or `none`. |
| `faces` | no | `right` (default) or `left`: the way the art faces. A pet drawn turned to the left mirrors when it faces right; effects are drawn as for a pet facing right. |
| `accent` | yes | The pet's accent colour for Settings and the menu: `$name`, `#RRGGBB` or `#AARRGGBB` (alpha first). |
| `glow` | yes | The glow colour around `glow` layers: `$name`, `#RRGGBB` or `#AARRGGBB`. |
| `palette` | no | `{name: colour}`, the default theme's colours (`themes.md`). |
| `theme` | no | The default theme's name; "Default" if absent. |
| `themes` | no | `{theme: {name: colour}}` or `{theme: {"sheets": {sheet: file}}}`. |
| `layers` | yes | 1 to 24 layers, drawn bottom first. |

Unknown keys are ignored everywhere except in a vector shape, so a pack may carry notes; but a misspelt key is
silently ignored too.

## A layer

| Key | Value |
|---|---|
| `name` | The layer's name (string). Also the Aseprite layer or tag prefix. |
| `channel` | `state`, `eyes`, `arms`, `feet`, `prop`, `headphones`, `fx` or `facing` (`channels.md`). |
| `default` | The value shown when the pose's value has nothing; the layer must have something for it. |
| `fallback` | `{value: next value or null}`. `null` draws nothing. At most 8 steps, no cycles. |
| `hit` | `true` if its opaque pixels take clicks and drags. Default `false`. |
| `glow` | `true` to draw a halo in the pack's `glow` colour around it. Not on an `fx` layer. |
| `mirror` | `flip` or `none`, overriding the pack's. |
| `themed` | Pixel and painted packs: whether themes recolour or re-sheet it. Default: every layer but `fx`. |
| `animations` | `{value: raster animation}` or `{value: part animation}` (a part animation has `keys`). |
| `shapes` | Vector: `{name: shape or [shapes]}` (`vector.md`). |
| `parts` | `[{"id", "origin": [x, y], "shape"?}]`. Ids unique in the pack. |
| `look` | Eyes channel only: `{direction: [dx, dy]}`. |
| `poses` | `{value: {part id: [x, y]}}`. |
| `transition_ms` | 0 to 10 000; how long parts take to reach a new pose (eased). Default 0 (jump). |

A layer "has" a value when its `animations`, `shapes`, `poses` or part animations name it.

## Raster animation (pixel and painted)

```json
"idle": { "sheet": "body.png", "frames": [[0, 0, 26, 24, 400], [26, 0, 26, 24, 400]], "loop": "loop" }
"idle": { "sheet": "body.png", "grid": { "frame": [26, 24], "origin": [0, 0], "gap": [0, 0], "columns": 3 },
          "frames": [0, 1, 2, 1], "ms": 160, "loop": "loop" }
```

`frames` are `[x, y, w, h, ms]` (whole numbers), or with `grid`, cell indices and `ms` (one number or one per
frame). `loop`: `loop` (default), `once`, `pingpong`. `clock`: `change` (default) or `pet`. `pivot`: `[x, y]` for all
frames or one per frame: the frame pixel placed on the art box's top-left (or on the part's origin). Details in
`pixel-and-painted.md`.

## Part animation (any kind)

```json
"rest": { "parts": ["hand-left", "hand-right"], "phase": [0, 0.5], "loop": "loop",
          "keys": [[0, 0, 500], { "dy": -1.5, "rotate": 8, "ms": 500 }] }
```

- `part` (one id) and/or `parts` (a list). `phase`: one number for all, or one per part, 0 to 1.
- `keys`: 1 to 64. A key is `[dx, dy, ms]` or `{dx, dy, rotate, scale, opacity, ms}` (all optional but `ms`;
  defaults 0, 0, 0, 1, 1). `ms` 1 to 10 000. Offsets no larger than the art box.
- Keys hold; there is no tweening between keys. Use more keys for smoother motion.
- `rotate` (degrees about the part's origin), `scale` and `opacity` work on vector parts only; a raster part takes
  `dx` and `dy`.
- `loop` as for frames. Part animations have no `clock`.

## Part motion, in order

A part's place each frame = its pose target for the value (eased over `transition_ms` on a change; the origin if
the pose doesn't name it) + its key offset + the look offset (eyes only). Then the app's whole-body motion
(breathing, squash, hop, drag) and last the mirroring. So parts ride with the body; never animate the whole body in
the pack.

`look` directions: `center`, `up`, `up-right`, `right`, `down-right`, `down`, `down-left`, `left`, `up-left`,
`far-left`, `far-right`, `up-far-left`, `up-far-right`, `down-far-left`, `down-far-right`. A missing far direction
takes its near one; other missing ones the nearest given, then `center`. No table: the eyes stare ahead. A built-in
pet gives all 15.

## One example per kind

**Vector**: the Blob in `docs/MAKING-A-PET.md` ("The manifest"), and `avatars/samples/skill-example/pack.json`
(`worked-example.md`). Built-ins: `rust/crates/aipet-sprite/packs/{hood-vector,sprout-vector,pip}/`.

**Pixel**: `avatars/samples/basic/pack.json` (explicit frames), `avatars/samples/grid/pack.json` (grid frames).
Built-ins: `packs/sprout/`, `packs/hood/`. The shape of one:

```json
{
  "format_version": 1, "name": "Sample", "kind": "pixel", "pixel_size": 5, "anchor": [13, 22],
  "mirror": "flip", "accent": "#B88CF0", "glow": "#FFF2A8",
  "layers": [
    { "name": "body", "channel": "state", "hit": true, "default": "idle", "fallback": { "listen": "sleep" },
      "animations": {
        "idle": { "sheet": "body.png", "frames": [[0, 0, 26, 24, 400], [26, 0, 26, 24, 400]], "loop": "loop" },
        "sleep": { "sheet": "body.png", "frames": [[52, 0, 26, 24, 1000]], "loop": "once" } } },
    { "name": "eyes", "channel": "eyes", "glow": true, "default": "open",
      "fallback": { "blink": "closed", "happy": "closed" },
      "look": { "left": [-5, 0], "right": [5, 0], "far-left": [-10, 0], "far-right": [10, 0] },
      "animations": {
        "open": { "sheet": "eyes.png", "frames": [[0, 0, 26, 24, 1000]], "loop": "once" },
        "closed": { "sheet": "eyes.png", "frames": [[26, 0, 26, 24, 1000]], "loop": "once" } } }
  ]
}
```

(A raster eyes `look` is in logical px and moves by whole art pixels: at `pixel_size` 5, 5 is one pixel.)

**Painted**: `rust/crates/aipet-sprite/packs/pip-painted/pack.json`: `"kind": "painted"`, `"pixel_size": 0.5`,
`"anchor": [130, 220]`, frames as for pixel, a sheet theme (`"Lilac": {"sheets": {"body.png": "body-lilac.png", …}}`)
and no palette.

**Aseprite**: `avatars/samples/native/` (a `.aseprite` file and a `pack.json` for the rest; `aseprite.md`).
