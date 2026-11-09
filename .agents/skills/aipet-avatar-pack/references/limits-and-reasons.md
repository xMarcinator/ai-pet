# Limits, and every reason `--check-pack` prints

`AiPet --check-pack` prints `OK …` or **one** reason, the first the loader hits, and exits 1. Fix it and run it
again: the next problem may be behind it. The Avatars page shows the same reason under "Your own avatars".

The reasons below are the loader's own wording (`rust/crates/aipet-sprite/src/{load,manifest,pack,vector,
aseprite_file}.rs`). `…` and `<…>` stand for names and numbers the line fills in; strings are quoted as JSON is.

## Limits

| What | Limit |
|---|---|
| `pack.json`, an Aseprite export's JSON, a theme file, a legacy avatar | 256 KB (262 144 bytes), nested at most 64 deep |
| A PNG sheet | 8 MB, 2048 px a side, 8 bits a channel |
| All of a pack's sheets, decoded | 64 MB (width × height × 4 summed) |
| An `.aseprite` file | 8 MB, 512 frames, 64 layers, 2048 px a side, 64 MB decoded, square pixels |
| Layers | 1 to 24 |
| Frames | at most 512 in the pack |
| Parts | at most 256 in a vector pack; at most 1 per layer in a pixel or painted pack |
| Frame durations | 1 to 60 000 ms |
| Part animation keys | 1 to 64 per animation, each 1 to 10 000 ms |
| `transition_ms` | 0 to 10 000 |
| `phase` | 0 to 1 |
| Fallbacks | at most 8 steps, no cycles |
| Numbers (coordinates, keys, path data) | finite, within ±10 000 |
| Part origins and pose targets | inside the art box and its margin: x −10 to 140, y −10 to 130 |
| Offsets (`look`, key `dx`/`dy`) | at most 150 across and 140 down |
| Raster frames | at most 150 × 140 logical px (frame size × `pixel_size`) |
| `pixel_size` | above 0, at most 130 |
| Stroke width, opacity | 0 to 100; 0 to 1 |
| Themes | 64 per pet, its own and the user's |
| Files | relative to the pack folder and inside it: no absolute paths, `..`, links, junctions or device names |

## Finding and reading files

| Reason | Meaning and fix |
|---|---|
| `<path>: not a folder or a theme file` | The path doesn't exist, or isn't a folder or a file. Check the path. |
| `no pack.json, Aseprite export or .aseprite file` | The folder has none of them. Pass the pack folder itself. |
| `the folder is a link` | The pack folder is a symlink or junction. Copy the folder instead. |
| `<file> is <n> bytes (at most 262144)` | A JSON file is over 256 KB. Shorten numbers (2 decimals), share shapes through parts, split nothing: simplify. |
| `<file> is nested more than 64 deep` | The JSON nests too deeply. Flatten it. |
| `<file>: <error>` | The JSON file can't be read, or (an export candidate) isn't JSON. |
| `"<name>" isn't a path inside the pack` | An empty name, or an absolute path. Use a plain file name. |
| `"<name>" names a device` | `CON`, `NUL`, `COM1`, … Rename the file. |
| `"<name>" leaves the pack's folder` | `..`, or a path that resolves outside. Keep files in the folder. |
| `"<name>" is missing` | A sheet named in the manifest isn't there. Check the name and its case. |
| `"<name>" is a link` | A sheet is a symlink. Copy the file in. |
| `"<name>" is <n> bytes (at most 8388608)` | A PNG is over 8 MB. Crop it, or split the sheet. |
| `"<name>" is <w> × <h> px (at most 2048 a side)` | A PNG is too large. Split it into several sheets. |
| `the pack's sheets come to more than 64 MB decoded` | Too much sheet area in all. Crop empty space, lower painted art's resolution, share sheets. |
| `"<name>": not a PNG: …` / `the PNG doesn't decode: …` | Not a valid PNG. Re-export it. |
| `"<name>": the PNG has Sixteen-bit channels (8 expected)` | 16 bits a channel. Export at 8 bits. |
| `"<name>": the PNG is <type> (RGBA, RGB or grey expected)` | An unexpected colour type. Export as RGBA. |
| `the PNG is too large` | The decoder can't size the image. Re-export it smaller. |
| `layer "<l>": a frame of "<v>" is <w> × <h> px, larger than the art box` | A raster frame times `pixel_size` exceeds 150 × 140. Crop the frame or lower `pixel_size`. |
| `a pet named "<name>" is already loaded` | (Avatars page only) another pack, or a built-in, has the name. Rename the pet. |

## The manifest's top level

| Reason | Meaning and fix |
|---|---|
| `the manifest isn't valid: <serde error> at line <l> column <c>` | Not JSON, a required key missing (`` missing field `glow` ``: `format_version`, `name`, `kind`, `pixel_size`, `anchor`, `accent`, `glow`, `layers`; per layer `name`, `channel`, `default`), or a value of the wrong type. The column points at the place. |
| `the manifest isn't valid: data did not match any variant of untagged enum Drawing at line <l> column <c>` | A vector shape with an unknown key (`stroke-width` for `stroke_width`, `transform`, `class`, …), without `d`, or with a wrong type. Look at the shape just before the column. |
| `format_version <n> is newer than this AiPet reads (1)` | Use `1`. |
| `unknown kind "<k>"` | `pixel`, `painted` or `vector`. (`rive` is not supported.) |
| `unknown mirror "<m>" (flip or none)` | Pack or layer `mirror`. |
| `palette colour "<n>": colour "<c>" is not #RGB, #RRGGBB, #RRGGBBAA or none` | A palette colour isn't hex. |
| `palette colour "<n>" is none` | A palette colour can't be `none`. |
| `palette colours "<a>" and "<b>" are the same colour: name each colour once` | Pixel packs: two names share one RGB. Merge the names or nudge one colour (in the sheets too). |
| `a painted pack has no named colours or colour themes (its colours are blended; its themes are sheets)` | Remove the palette and colour themes; use sheet themes. |
| `the default theme's name "<t>" is empty, or starts or ends with a space` | Fix `theme`. |
| `the accent: no palette colour named "<n>"` / `the glow: …` | `$name` the palette lacks. |
| `colour "<c>" doesn't start with #` / `isn't hex` / `isn't #RRGGBB or #AARRGGBB` | A plain `accent` or `glow` (or an Aseprite `pack.json`'s): `#RRGGBB` or `#AARRGGBB`, alpha first. |
| `<n> layers (1 to 24)` | Too many or no layers. Merge layers (one effects layer can hold several particles as parts). |
| `pixel_size is <n>` | Above 0, at most 130. |
| `layer "-": the anchor is (<x>, <y>)` | The anchor isn't a finite number within ±10 000. |
| `<n> frames (at most 512)` | Too many frames. Reuse frames, use `pingpong`, drop near-duplicates. |
| `<n> vector parts (at most 256)` | Too many parts. Make fixed decorations part of a layer's shape; share particles. |
| `part id "<id>" is used twice` | Ids are unique across the whole pack. |

## Themes in the manifest

| Reason | Meaning and fix |
|---|---|
| `theme "<t>": colour "<n>" isn't a colour` | A theme colour isn't a string. |
| `theme "<t>": colour "<n>": colour "<c>" is not #RGB, …` / `theme "<t>": colour "<n>" is none` | A theme colour isn't hex. |
| `theme "<t>": a theme changes colours or sheets, not both` | Split it into two themes. |
| `theme "<t>": it names no sheets` | An empty `sheets` map. |
| `theme "<t>": the sheet for "<s>" isn't a file name` | A sheet theme's value isn't a string. |
| `theme "<t>": "<s>" isn't a sheet the pack's frames are on` | The key must be a sheet a frame uses. |
| `theme "<t>": <file>: <reason>` | The replacement sheet can't be read (see the file reasons). |
| `theme "<t>": <file> is <w> × <h> px, but <sheet> is <w> × <h> px (a theme's sheet is the same size)` | Make the replacement the same size. |
| `theme "<t>": its name is empty, or starts or ends with a space` | Fix the theme's name. |
| `theme "<t>": there are no named colours to change` | A colour theme on a pack without a palette. Add the palette. |
| `theme "<t>": there is already a theme by that name` | It repeats the default theme's or another theme's name. |
| `theme "<t>": there are too many themes (at most 64)` | Fewer themes. |
| `theme "<t>": there is no colour named "<n>" (the colours are …)` | A theme names a colour the palette lacks. Add it to the palette, or fix the name. |

## Layers

| Reason | Meaning and fix |
|---|---|
| `layer "<l>": unknown channel "<c>"` | One of `state`, `eyes`, `arms`, `feet`, `prop`, `headphones`, `fx`, `facing`. |
| `layer "<l>": unknown look "<d>"` | A `look` key that isn't one of the 15 directions. |
| `layer "<l>": its default "<v>" has nothing to show` | The layer has no animation, shape, pose or part animation named after its `default`. |
| `layer "<l>": the fallbacks from "<v>" go round in a cycle` | `a → b → a`. Break the loop. |
| `layer "<l>": the fallbacks from "<v>" take more than 8 steps` | Shorten the chain. |
| `layer "<l>": an effects layer can't glow` | Remove `glow` from the `fx` layer. |
| `layer "<l>": only the eyes channel looks around` | `look` on a layer that isn't `eyes`. |
| `layer "<l>": a raster layer moves as one piece; give each moving part a layer of its own` | A pixel or painted layer with two or more parts. One layer per moving part. |

## Raster animations

| Reason | Meaning and fix |
|---|---|
| `layer "<l>": animation "<v>" isn't valid: <serde error>` | A required key missing (`sheet`, `frames`) or a wrong type. An animation without `keys` is read as raster frames, so a part animation that lost its `keys` fails here with `` missing field `sheet` ``. |
| `layer "<l>": "<v>": frames isn't a list` | `frames` must be a list. |
| `layer "<l>": "<v>": a frame isn't [x, y, w, h, ms]` | Five whole, non-negative numbers. |
| `layer "<l>": "<v>": a frame is outside <sheet>` | The rectangle goes past the sheet's edge. |
| `layer "<l>": "<v>": the grid's frame is <w> × <h>` | A grid frame size must be positive. |
| `layer "<l>": "<v>": the grid has <n> columns` | At least 1. |
| `layer "<l>": "<v>": the grid's origin or gap is negative` | 0 or more. |
| `layer "<l>": "<v>": a grid animation needs its ms` | Add `ms`. |
| `layer "<l>": "<v>": <n> durations for <m> frames` | One `ms` per frame, or one number. |
| `layer "<l>": "<v>": frame index <i> isn't a cell` | Cells are whole numbers from 0. |
| `layer "<l>": "<v>": cell <i> is outside <sheet>` | The cell lies past the sheet's edge: check `columns`, `origin`, `gap`, `frame`. |
| `the duration isn't a whole number of ms` / `a duration isn't …` | A grid `ms` that isn't a whole number. |
| `layer "<l>": "<v>" has <n> pivots for <m> frames` | One pivot, or one per frame. |
| `a pivot isn't [x, y]` / `a pivot isn't a number` | A pivot must be two numbers. |
| `layer "<l>": a frame's pivot is (<x>, <y>)` | A pivot outside ±10 000. |
| `layer "<l>": animation "<v>" has no frames` | An empty `frames` list. |
| `layer "<l>": a frame of "<v>" lasts <n> ms (1 to 60 000)` | Fix the duration. |
| `layer "<l>": unknown loop mode "<m>" (loop, once or pingpong)` | Fix `loop`. |
| `layer "<l>": unknown clock "<c>"` | `change` or `pet`. |
| `layer "<l>": <sheet>: <reason>` | The sheet can't be read (see "Finding and reading files"). |

## Parts and part animations

| Reason | Meaning and fix |
|---|---|
| `the manifest isn't valid: …` (in `parts`, `poses`, `look`) | A part without `id` or `origin`, or a position that isn't `[x, y]`. |
| `layer "<l>": part "<id>"'s position is (<x>, <y>)` | Not finite, or beyond ±10 000. |
| `layer "<l>": part "<id>" at (<x>, <y>) is outside the art box and its halo` | Origins and pose targets lie in x −10 to 140, y −10 to 130. |
| `layer "<l>": part "<id>" draws unknown shape "<s>"` | A part's `shape` names no shape of the layer. |
| `layer "<l>": pose "<v>" moves unknown part "<id>"` | A pose names a part the layer doesn't have (parts belong to one layer). |
| `layer "<l>": transition_ms is <n> (0 to 10 000)` | Fix it. |
| `layer "<l>": animation "<v>" isn't valid: …` | A part animation (one with `keys`) with a wrong type. |
| `layer "<l>": animation "<v>" has <n> keys (1 to 64)` | Fewer keys, or longer ones. |
| `layer "<l>": animation "<v>" moves unknown part "<id>"` | `part`/`parts` name a part the layer doesn't have. |
| `layer "<l>": part "<id>"'s phase is <p> (0 to 1)` | Fix `phase`. A `phase` list of the wrong length is read as one phase and fails as `a phase isn't a number`. |
| `layer "<l>": a key isn't [dx, dy, ms]` / `… or an object` | A key must be three numbers or an object. |
| `layer "<l>": a key has no ms` | An object key needs `ms`. |
| `layer "<l>": a key's <k> isn't a number` / `a key's ms isn't a whole number of ms` | Fix the value. |
| `layer "<l>": a key of "<v>" lasts <n> ms (1 to 10 000)` | Fix the duration. |
| `layer "<l>": a key of "<v>" has <n>` | A key's number isn't finite or is beyond ±10 000. |
| `layer "<l>": <v>'s offset is (<x>, <y>)` / `…'s offset (<x>, <y>) is larger than the art box` | A key or look offset beyond 150 across or 140 down. Offsets are relative; keep them small. |

## Vector shapes

| Reason | Meaning and fix |
|---|---|
| `layer "<l>": shape "<v>": path data must start with M` | Begin `d` with `M`. |
| `… path data must start with a command` | `d` begins with a number. |
| `… path data: unknown command "<c>"` | Only `M L H V C S Q T A Z` (either case). |
| `… path data: expected a number at byte <i>` | Missing or malformed number. |
| `… path data: <n> is out of range (±10000)` | A coordinate too large. |
| `… path data: expected an arc flag at byte <i>` | An `A` command's flags must be `0` or `1`. |
| `… colour "<c>" is not #RGB, #RRGGBB, #RRGGBBAA or none` | Fix `fill` or `stroke`. |
| `… no palette colour named "<n>"` | A `$name` the palette lacks. |
| `… unknown linecap "<c>" (butt, round or square)` / `unknown linejoin "<j>" (miter, round or bevel)` | Fix it. |
| `layer "<l>": shape "<v>"'s stroke width is <n> (0 to 100)` | Fix `stroke_width`. |
| `layer "<l>": shape "<v>"'s opacity is <n> (0 to 1)` | Fix `opacity`. |

## Theme files and legacy avatars

| Reason | Meaning and fix |
|---|---|
| `the theme file isn't valid: <serde error>` | Not JSON, a key missing, or a key other than `pet`, `theme`, `palette`. |
| `there is no pet named "<p>" for the theme` | `pet` must be a loaded pet's exact name. In a `themes/` folder, the pets are those of the avatars folder above it and the built-ins. |
| `<p> is a painted pet, which has no themes` | Painted pets take sheet themes only, in their pack. |
| `the colour "<n>": …` / `the colour "<n>" is none` | A palette colour isn't hex. |
| `<p>'s theme "<t>": <reason>` | As for themes in the manifest: the name, an existing name, too many, or a colour the pet lacks (`there is no colour named "<n>" (the colours are …)`). |
| `<path>: not an avatar file` / `<path>: the avatar has nothing to draw` / `<path>: the file is <n> bytes (at most 262144)` | A legacy avatar `.json` (outside `themes/`, without `pet`) that isn't one. See `docs/MAKING-A-PET.md`, "Legacy avatars". |

## Aseprite

| Reason | Meaning and fix |
|---|---|
| `the export isn't valid: …` | The export JSON doesn't parse. |
| `the export is json-hash; export it as an array (json-array)` | Re-export with JSON Data as Array. |
| `the export has no frames` / `the export names no image` | Not a sprite-sheet export, or `meta.image` missing. |
| `frame <i> has no rectangle` / `frame <i> is outside <image>` | The export and its PNG don't match. Export both again together. |
| `tag "<t>" covers frames <a> to <b>, of <n>` | A tag past the last frame. |
| `pack.json isn't valid: …` | The `pack.json` beside the art isn't JSON. |
| `the export has no tags (name them <layer>:<value>)` | Add tags (also for native files). |
| `tag "<t>" isn't <layer>:<value>` | Name the tag `layer:value`. |
| `tag "<t>" names layer "<l>", which isn't a channel or in pack.json` | Declare the layer in `pack.json`, or name it after a channel (`body` for `state`). |
| `layer "<l>": part "<id>" has no slice` | Every part listed in `pack.json` needs a slice of that name. |
| `<file>: layer "<l>" has no frame <i>` | A tag's frame has no cel on that layer. |
| `<file>: its frames don't fit a 2048 px sheet` | The cropped frames are too large together. Fewer or smaller frames. |
| `<file> is <n> bytes (at most 8388608)` | The `.aseprite` file is over 8 MB. |
| `<file>: not an Aseprite file` / `bad frame header` / `bad palette` / `bad old palette` / `bad cel data: …` / `cel data is <n> bytes, expected <m>` / `unknown cel type <t>` / `a cel names an unknown layer` / `tag "<t>" is malformed` / `slice "<s>" has more keys than frames` / `frame <f> links to an empty or later cel` / `colour index <i> is not in the palette` | The file is damaged or from an unusual writer. Save it again from Aseprite. |
| `<file>: file is <n> bytes, over the <m> limit` / `canvas <w>x<h> is outside the limits` / `<n> frames, over the 512 limit` / `over 64 layers` / `decoded art over the <n> byte limit` | Over a native-file limit. |
| `<file>: colour depth <d> is not supported` / `non-square pixels are not supported` | Use RGBA, greyscale or indexed, with square pixels. |
| `<file>: layer "<l>" is a tilemap; tilemaps are not supported` / `tilemap cels are not supported` / `the file has a tileset; tilesets are not supported` / `the file links external files; those are not supported` | Convert tilemap layers to normal layers; embed external files. |

An Aseprite pack is checked against the same pack limits afterwards, so the manifest, layer and part reasons above
apply to it too.
