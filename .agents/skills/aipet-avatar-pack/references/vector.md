# Vector packs

`"kind": "vector"`, `"pixel_size": 1`, `"anchor": [65, 110]`. Coordinates are logical px in the 130 × 120 art box,
plus a 10 px margin on every side. The pack loads nothing but its manifest: no SVG files, images, fonts or
`url(…)` paints. The reader is `rust/crates/aipet-sprite/src/vector.rs`; people's guide: `docs/MAKING-A-PET.md`,
"Vector shapes" and "Part motion".

## Shapes

```json
"shapes": {
  "open": [ { "d": "M -3.5 0 A 3.5 5 0 1 0 3.5 0 A 3.5 5 0 1 0 -3.5 0 Z", "fill": "$eyes" },
            { "d": "M -2.5 -2 A 1.2 1.2 0 1 0 0 -2 A 1.2 1.2 0 1 0 -2.5 -2 Z", "fill": "#FFFFFF" } ],
  "closed": { "d": "M -4 1 Q 0 3 4 1", "stroke": "$eyes", "stroke_width": 2.2, "linecap": "round" }
}
```

A drawing is one shape or a list, bottom to top. A shape's keys, and only these: any other key, such as
`stroke-width`, skips the pack with `the manifest isn't valid: data did not match any variant of untagged enum
Drawing at line … column …`, which doesn't name the key.

| Key | Value |
|---|---|
| `d` | Required. SVG path data: `M L H V C S Q T A Z`, upper or lower case. Must start with `M`. Numbers within ±10 000. |
| `fill`, `stroke` | `#RGB`, `#RRGGBB`, `#RRGGBBAA`, `none` (default) or `$name`. |
| `stroke_width` | 0 to 100; default 1. |
| `linecap` | `butt` (default), `round`, `square`. |
| `linejoin` | `miter` (default), `round`, `bevel`. |
| `opacity` | 0 to 1; default 1. |

There is no transform, gradient, filter, blur, mask, clip path or text. Draw an outline by drawing the shape twice:
the fill first, the inner details (shade bands, rim light, highlights) over it, then the same path with only a
thick stroke on top; the stroke hides where the details meet the edge, so no clipping is needed.

## Which shape a layer draws

- **A layer without parts** draws the shape named after the value it resolves to, in art-box coordinates.
- **A layer with parts** draws, for each part, the part's own `shape` if it names one, else the shape named after
  the value, in coordinates centred on (0, 0), placed at the part's position (origin, pose, keys, look).
- Each expression or pose is a drawing of its own: there is no way to scale one value's drawing into another's.

So: a body is usually a layer without parts (`idle`, `sleep` drawings); eyes are two parts with a shape per
expression (both eyes draw the same shape, centred on each eye); hands and feet are parts with one fixed `shape`,
moved by `poses`; each Z, dot, note, sparkle or dust puff is a part of its effect's layer.

## Parts and motion

- At most **256 parts** in a vector pack, ids unique across the pack.
- Origins and pose targets lie in the art box and its margin (−10 to 140 across, −10 to 130 down).
- `poses` move parts per value; `transition_ms` eases between poses.
- Part animations add keys: `dx`, `dy`, `rotate` (degrees about the origin), `scale` (about the origin),
  `opacity`. Put a part's origin where it should turn: a shoulder for an arm, a stem's base for a sprout (Pip).
- `look` (eyes layer only) offsets the eye parts per direction. Small: 2 to 6 px.
- Parts keep their origins whatever the body draws: when a state's body drawing moves the face (a squatter `sleep`,
  say), the eyes stay where they were. Keep the face at the same place in every body drawing, or move the eyes with
  a pose on that state's eye value. Mind the fallbacks: a pose on `closed` also applies to `blink`, which falls back
  to it, so give the moved state an eye value of its own.
- Never animate the whole body: the app breathes, squashes, hops and mirrors it.

## Generators

Every vector built-in is written by a Python script, never by hand. Two ways, by size:

**Small pets: write the manifest's shapes directly.** `avatars/samples/skill_example_gen.py` builds the shapes as
dicts, writes `pack.json`, and draws its review page from the pack it wrote. See `worked-example.md`.

**Full pets: draw SVG, convert with `vector_pack.py`.** `avatars/reference/hood_gen.py` and `sprout_gen.py`:

1. Constants: the palette `C` (and other themes, such as `hood_gen.BLUE`), the geometry in a 260 × 250 drawing
   whose ground is `GROUND` (a pet traced from a pixel reference can place its shapes in the reference's cells
   through small helpers, such as `X(c)` and `Y(r)`).
2. `figure(pose)` returns the pet's SVG for one `pet_states` pose: groups with stable ids (`body`, `face`,
   `eye-left`, `eye-right`, `arm-left`, `foot-left`, `prop`, `caret`, `headphones`, …), each part group
   `translate`d (and for arms `rotate`d) to its place, its contents drawn around (0, 0). Canvas-ready only: `path`,
   `rect`, `circle`, `ellipse` with flat fill, stroke and opacity; no filters, masks, clip paths, gradients or `<use>`.
3. `frame(pose)` = `pet_states.compose(figure(pose), shadow(), GROUND, pose, accent, outline)`: adds effects,
   whole-body offset and squash, the shadow and mirroring, for the review pages only.
4. `main()` writes the reference SVG, the review page and the state sheet (`pet_states.write_pages`); with
   `--pack [folder]` it writes the pack instead (`vp.write(folder, pack())`; the default folder is the built-in's in
   `rust/crates/aipet-sprite/packs/`).
5. `pack()` reads the groups back from `figure()` for each value through `vp.Drawing(figure, GROUND)`:
   - `d.art(pose, *ids, skip=())`: the groups' shapes in the art box; `d.at(pose, id)`: a group's place and turn;
     `d.inner(pose, id)`: its contents about its own (0, 0).
   - `vp.relative(items, origin)`, `vp.drawing(items)`: shapes about an origin, as the manifest writes them.
   - `vp.layer(name, channel, default, **keys)`, `vp.eyes(d, pose_of)` (expressions, origins, the `look` table),
     `vp.feet(d, ids)`, `vp.effect_layers(accent, outline, d.norm)` (one layer per effect, as `pet_states` draws
     them), `vp.caret(d, pose)`.
   - Motion from the preview's CSS: `vp.held(STYLE, cls, name)`, `vp.eased(…)`, `vp.phase(STYLE, cls)`,
     `vp.keys(…)`, `vp.drawn(…)` (drawing units to art px), `vp.anim(parts, keys)`, `vp.scan_keys()` (the
     magnifier's scan).
   - `vp.named(pack, C, theme=…, themes=…)`: hex colours to `$name`s, the palette and the themes.
   The drawing is halved into the art box with its ground on the anchor: x' = x / 2, y' = (y − ground) / 2 + 110.

Run a generator with `python3 avatars/reference/<name>_gen.py` (pages) and `--pack <folder>` (the pack), then
`--check-pack <folder>`. Copy the closest generator and change its drawing; keep `pet_states` and `vector_pack`
shared, and don't change them for one pet (every built-in depends on them).

Pip is the odd one: `pip_gen.py` converts a drawing from an HTML page with its own helpers. A pixel pet's generator
writes PNG sheets instead (`pixel-and-painted.md`).
