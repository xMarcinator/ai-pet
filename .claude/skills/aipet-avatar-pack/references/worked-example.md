# Worked example: Dewdrop

`avatars/samples/skill-example/` is a small, complete vector pet, Dewdrop (example): a water drop with a body that
squashes when it sleeps, eyes that follow the pointer, two hands that rest, bob, wave and lift, Zs while asleep and
sparkles when it's done or poked, in two themes. It is written by `avatars/samples/skill_example_gen.py`, which also
writes its review page, `avatars/samples/skill-example-preview.html`.

```sh
python3 avatars/samples/skill_example_gen.py
rust/target/release/AiPet --check-pack avatars/samples/skill-example
# OK Dewdrop (example) (vector, 5 layers, 0 frames, themes Rain, Dusk)
```

To make a pet of your own from it, copy the generator, change `PACK`, `PREVIEW`, the name and the drawing, and
follow the steps below. To try it in the app, copy the folder into the avatars folder and click **Reload**.

## 1. The kind

Vector: the pet is a few smooth shapes, it should stay sharp at every Pet size, and its colours should change with
a theme. So `"kind": "vector"`, `"pixel_size": 1`, and `"anchor": [65, 110]`, where every built-in vector pet
stands. All coordinates are in the 130 × 120 art box.

## 2. References and colours

For a real pet, this is where the user's references come in. Dewdrop has none, so the palette is chosen by role,
not hue, so that a theme can change it:

```python
PALETTE = {"outline": "#123C4A", "body": "#5CC8E0", "shade": "#3A9FBF", "rim": "#C8F4FF", "eyes": "#0E2530",
           "accent": "#3A9FBF", "glow": "#9FEAFF"}
DUSK = {"outline": "#2E1F4A", "body": "#B08CE0", "shade": "#8A66C2", "rim": "#EBDDFF", "accent": "#8A66C2",
        "glow": "#E0C8FF"}
```

The manifest gets `"palette": PALETTE`, `"theme": "Rain"` (the default theme's name), `"themes": {"Dusk": DUSK}`,
and `"accent": "$accent"`, `"glow": "$glow"`, so the accent follows the theme too. The eyes' colour isn't in `DUSK`,
so Dusk keeps Rain's.

## 3. The states it covers

Dewdrop draws these values itself, and lets the rest fall back (`channels.md`):

| Channel | Drawn | Falls back |
|---|---|---|
| `state` (layer `body`) | `idle`, `sleep` | everything else → `idle` (the default) |
| `eyes` | `open`, `wide`, `closed`, `happy` | `blink` → `closed`, `squint` → `happy`, `up`, `down` → `open` (the look moves them) |
| `arms` | `rest`, `raised`, `wave-a`, `wave-b` | `hold`, `lens` → `rest`; `run-a`, `run-b` → `raised` |
| `fx` (layer `zs`) | `zs` | `zs+sparkles` → `zs`; every other effect → `null` |
| `fx` (layer `sparkles`) | `sparkles` | every `…+sparkles` → `sparkles` by itself; every other effect → `null` |
| `feet`, `prop`, `headphones` | no layer | not drawn |

So it shows no laptop, magnifier, headphones, dots, notes, alert or dust: the pose still changes (eyes, hands), and
the app adds the whole-body motion. A built-in pet would need all of them (`SKILL.md`, "Where packs go").

## 4. The drawing

**The body** is a layer without parts, so its shapes are in art-box coordinates, one drawing per value:

```python
body = {"name": "body", "channel": "state", "hit": True, "default": "idle",
        "shapes": {"idle": drop(30, 30, 30), "sleep": drop(44, 34, 25)}}
```

`drop(top, rx, ry)` returns five shapes, bottom to top, following `design-conventions.md`: the fill
(`$body`), a hard-edged shade band along the bottom (`$shade`), a rim light stroke down the right side (`$rim`),
a small highlight, and last the same outline path with only a 4 px `$outline` stroke and round joins, which hides
where the inner shapes meet the edge. The round bottom always ends on y = 110, the ground. `sleep` is a separate,
squatter drawing, not a scaled `idle`.

**The eyes** are two parts. Without a `shape` of their own, both draw the shape named after the value, centred on
(0, 0), at their origins plus the look offset:

```python
"parts": [{"id": "eye-left", "origin": [56, 80]}, {"id": "eye-right", "origin": [74, 80]}],
"shapes": {"open": eye(3.5, 5), "wide": eye(4.5, 6),
           "closed": line("M -4 1 Q 0 3 4 1"), "happy": line("M -4 2 Q 0 -3 4 2")},
"fallback": {"blink": "closed", "squint": "happy", "up": "open", "down": "open"},
"look": LOOK
```

`LOOK` gives all 15 directions: 3 px for a near look, 5 for a far one, 2 up or down.

**The hands** are two parts that both draw the fixed shape `hand`, moved by `poses`:

```python
"parts": [{"id": "hand-left", "origin": [33, 92], "shape": "hand"},
          {"id": "hand-right", "origin": [97, 92], "shape": "hand"}],
"poses": {"rest": {}, "raised": {"hand-left": [24, 64], "hand-right": [106, 64]},
          "wave-a": {"hand-right": [106, 60]}, "wave-b": {"hand-right": [111, 72]}},
"transition_ms": 140,
"animations": {"rest": {"parts": ["hand-left", "hand-right"], "phase": [0, 0.5], "loop": "loop",
                        "keys": [[0, 0, 500], [0, -1, 250], [0, -1.5, 500], [0, -1, 250]]}}
```

`"rest": {}` keeps both at their origins; `wave-a` moves only the right hand. The app alternates `wave-a` and
`wave-b`, and the 140 ms transition eases between them. While resting, the keys bob each hand up to 1.5 px, half a
loop apart. Keys hold, so the bob uses four steps rather than two.

**The effects** are a layer each, every particle a part. The Zs layer's `fallback` names `zs+sparkles` so that a
poke while asleep keeps the Zs, and sets every other effect to `null`. The sparkles layer only needs the plain
effects set to `null`: each combined effect falls back to its last part, `sparkles`. The sparkles twinkle with
`scale` and `rotate` keys, which vector parts can use.

## 5. The review page

`preview(p)` draws each state from the manifest just written, following the fallbacks as the app does, in a
130 × 120 box on white, mid-grey and near-black, once per theme, with a mirrored row. Screenshot it as `review.md`
says, and look for the problems listed there. On Dewdrop, for instance, the dark outline fades on near-black, and
the rim light and the bright body carry the shape there instead.

## 6 and 7. Iterating and the Codex review

For a real pet: show the screenshots to the user, change the generator, run it again; then run Codex read-only on
the screenshots with the owner's decisions listed as fixed (`review.md`). Dewdrop had none of this; it is a sample.

## 8. The pack and the check

`main()` writes `pack.json` with `json.dump(p, f, indent=1)`, then the page. `--check-pack` on the folder prints
the `OK` line above. When it doesn't, look the line up in `limits-and-reasons.md`, fix the generator (never the
JSON), and run both again.

## 9. Where it went

As a sample: `avatars/samples/skill-example/`, with its generator and page beside it and a line in
`avatars/README.md`. A user's own pet goes in their avatars folder instead.
