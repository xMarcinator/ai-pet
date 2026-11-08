# Channels, values and fallbacks

The pose's channels and values come from `rust/crates/aipet-sprite/src/pose.rs`; the table in
`docs/MAKING-A-PET.md` ("How a pet works") says when each value happens. The layer's channel decides which value it
is asked for.

| Channel | Values | Resting value |
|---|---|---|
| `state` | `idle`, `idle-look`, `idle-hop`, `idle-stretch`, `idle-wiggle`, `idle-tap`, `sleep`, `thinking`, `working`, `attention`, `done`, `listen` | `idle` |
| `eyes` | `open`, `closed`, `blink`, `happy`, `squint`, `wide`, `up`, `down` (and a look direction) | `open` |
| `arms` | `rest`, `raised`, `wave-a`, `wave-b`, `run-a`, `run-b`, `hold`, `lens` | `rest` |
| `feet` | `stand`, `tap`, `dangle-a`, `dangle-b`, `run-a`, `run-b` | `stand` |
| `prop` | `none`, `laptop`, `lens` | `none` |
| `headphones` | `on`, `off` | `off` |
| `fx` | `none`, `zs`, `dots`, `notes`, `alert`, `sparkles`, `dust`, and `zs+sparkles`, `dots+sparkles`, `notes+sparkles`, `alert+sparkles`, `dust+sparkles` | `none` |
| `facing` | `left`, `right` | `right` |

Any of these may also be asked as `<value>@left` while the pet faces left: a layer that has it draws it unmirrored.

## How a value resolves

For the value the pose asks for, a layer shows:

1. the value itself, if the layer has it (an animation, shape, pose or part animation by that name);
2. else, if `fallback` names it, the next value, and so on (at most 8 steps); `null` draws nothing;
3. else, for a combined effect (`zs+sparkles`), the same for its last part (`sparkles`);
4. else the layer's `default`.

So a layer the pet doesn't need for a value must say `null`, or it shows its default. That is the usual mistake with
headphones (`"fallback": {"off": null}`), props (`{"none": null}`) and effects layers (every other effect `null`).

An effects layer per effect is the clearest pattern (the built-ins do it):

```json
{ "name": "zs", "channel": "fx", "default": "zs",
  "fallback": { "zs+sparkles": "zs", "none": null, "dots": null, "notes": null, "alert": null,
                "sparkles": null, "dust": null } }
{ "name": "sparkles", "channel": "fx", "default": "sparkles",
  "fallback": { "none": null, "zs": null, "dots": null, "notes": null, "alert": null, "dust": null } }
```

The second one draws its sparkles for every `…+sparkles` by step 3. The first needs `"zs+sparkles": "zs"`, or step 3
would send it to `sparkles`, which is `null`.

A pet may leave a whole channel out: the parts simply aren't drawn. A pet with a body and eyes is valid.

## From `pet_states.py` to channel values

`avatars/reference/pet_states.py` describes each state as a generator pose (`expr`, `arms`, `feet`, `prop`,
`headphones`, `fx`, plus `look`, `dx`/`dy`, `sx`/`sy`, `shadow`, `mirror`, `phase`, `scan`). `vector_pack.py` maps
them to the pack's values:

| Generator pose | Pack value |
|---|---|
| `expr`: `open`, `sleep`, `blink`, `happy`, `squint`, `wide`, `up`, `down` | eyes `open`, `closed`, `blink`, `happy`, `squint`, `wide`, `up`, `down` |
| `arms`: `rest`, `raised`, `wave` (`phase` 0/1), `run` (0/1), `hold` (+ laptop), `lens` (+ lens) | arms `rest`, `raised`, `wave-a`/`wave-b`, `run-a`/`run-b`, `hold`, `lens` |
| `feet`: `stand`, `tap`, `dangle` (0/1), `run` (0/1) | feet `stand`, `tap`, `dangle-a`/`dangle-b`, `run-a`/`run-b` |
| `prop`: none, `laptop`, `lens` | prop `none`, `laptop`, `lens` |
| `headphones`: true | headphones `on` |
| `fx`: none, `zs`, `dots`, `notes`, `alert`, `sparkles`, `dust` | fx values of the same names |
| `look`, `dx`, `dy`, `sx`, `sy`, `shadow`, `mirror` | the app's (look offsets, whole-body motion, shadow, facing): not drawn in the pack |
| `scan` | the lens part's keys (`vector_pack.scan_keys`) |

The `state` values are the moods and idle acts; most pets draw one body (`idle`) and perhaps `sleep`, and let
everything else fall back to the default. The app's whole-body motion already makes the idle acts, sleep, the hop and
the landing read differently.

## What "covers every state" means

- **A user's pet**: any value without a drawing falls back to the default, which is valid. Make the fallbacks
  deliberate (the 15 `look` direction names are in `manifest.md`): write the ones that matter (`blink` → `closed`, `squint` → `happy`, `run-a` → `raised`, …) and
  `null` the ones that must draw nothing.
- **A built-in pet** (`tests/vector_packs.rs`): every layer of the eyes, arms, prop, headphones and fx channels (and
  feet, if it has any) has each value of the table above itself or names it in `fallback`; a combined effect may take
  its last part. The eyes give all 15 `look` directions. Only feet may be missing as a channel.
