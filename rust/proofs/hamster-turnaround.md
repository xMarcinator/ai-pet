# Hamster turnaround spike: a 24-frame turn and roll in today's pack format

Scope: whether the Hamster can do a 3D-style turnaround (front, side, back, curl) of 24 frames as pixel frames and
as vector parts. Also which state plays it, how its cycle finishes before a mood change (except for dangle, run and
poke), what each pack kind costs, and the follow-up tasks. This is task fnt-72-new-pet-states-and-the-hamsters.5
(R10). It records findings only and ships no art.

The idea comes from JLandbo's hamster-pet (24 frames at 100 ms, which finish their cycle except for dangle, run and
giggle). Nothing here traces or copies its frames. The scratch views below are drawn from scratch in our Hamster's
own palette. The pixel front view is our pack's own reference grid (`hamster_pixel_gen.py`), and the vector front
view is today's `hamster.svg`.

Code read: `aipet-sprite/src/pack.rs` (format model and limits), `compose.rs` (`step`, `pick`, `Parts::local`),
`vector.rs` (`item`, `render`), `pose.rs` (`Tracker::pose`), `pet.rs` (`compute_pose`, the idle acts), `painted.rs`,
and the two Hamster packs and generators.

## Verdict

- **Yes for both kinds, with today's format.** The only engine changes are in the behaviour layer: a new idle act,
  a hide value on three channels, and a hold-to-end. Pack-format keys are unchanged.
- **Pixel** is a plain 24-frame `idle-spin` animation on the body layer, at 100 ms a frame. It needs 6 hand-drawn
  cells. Mirrors, rotations and the composited front views come from the generator.
- **Vector** needs a workaround, because a vector value draws one fixed drawing (its frames only time it) and its
  parts share one key list. The fix is 24 parts on the body layer, each with a fixed `shape`, and one two-key opacity
  animation whose per-part `phase` lights exactly one part per 100 ms. It works, but every hidden view is still
  rasterised each frame unless the renderer skips zero-opacity items.
- **Painted** costs nothing extra. The painted bake turns that phase table into 24 frames on its own.
- **State:** an occasional idle act, `idle-spin`, behind fnt-72's production-only switch.
- **Cycle hold:** in `Pet::compute_pose`, not in the `Tracker`. While the spin plays, a mood change waits for its end.
  A drag (run or dangle) or a poke cuts it at once. The spin's length comes from the pack. No pack flag is needed.

## Scratch views

Pixel, at 3x: our front (the three-quarter reference), side, back, side mirrored (generated), curl, and curl
rotated 90° (generated).

![pixel views](hamster-turnaround/pixel-views.png)

Vector: today's front, then side (11 shapes), back (9) and curl (6).

![vector views](hamster-turnaround/vector-views.png)

What drawing them showed:
- The Hamster is bilaterally symmetric. Only its three-quarter rest pose is lopsided, so every opposite-turn view is
  a mirror of a drawn one. The side facing left mirrors the side facing right, the back three-quarter right mirrors
  the left one, and the front three-quarter right mirrors today's rest pose. Mirroring flips the light (upper left,
  darker orange on the right), but at 100 ms a frame that doesn't show. Pixel mirrors are lossless. Vector mirrors
  are a path transform (`x -> 260 - x`) in the generator.
- The curl is a ball, so its rotations are the same drawing. Pixel art rotates by 90° losslessly. The generator puts
  the highlight back at the upper left after rotating.
- The scratch side, back and curl are rounder than the real Hamster, which is a boxy loaf with square-ish corners.
  The real views must keep that silhouette, its outline weights, and the new right-side shading from task .3. That
  is the main drawing effort, more than the frame count.

## The 24 frames

The turn goes around through all views, then a forward roll. At 100 ms each, the cycle is 2.4 s. "F¾L" is today's
rest pose (three-quarter, face to the left). Bold names are new drawings. The rest are composites of existing layers,
or are generated.

| # | view | made from | lift (art px) |
|---|------|-----------|---------------|
| 0 | F¾L | composite of today's idle (body, ears, eyes open, arms rest, feet) | 0 |
| 1 | side, facing left | mirror of **side** | 0 |
| 2 | back ¾ left | **back ¾** | 0 |
| 3-4 | back | **back** (held two frames) | 0 |
| 5 | back ¾ right | mirror of back ¾ | 0 |
| 6 | side, facing right | **side** | 0 |
| 7 | F¾R | mirror of the frame-0 composite | 0 |
| 8 | front | **front** (straight on) | 0 |
| 9 | F¾L | as frame 0 | 0 |
| 10 | crouch | **crouch** (pixel: the generator can squash rows instead) | 0 |
| 11 | tuck | **tuck** | 4 |
| 12-18 | ball 0°, 90°, 180°, 270°, 0°, 90°, 180° | **ball**, rotated by the generator | 10, 16, 18, 16, 10, 4, 0 |
| 19 | tuck | as 11 | 0 |
| 20 | crouch | as 10 | 0 |
| 21-22 | stretch | composite: arms `raised`, eyes `happy` | 0 |
| 23 | F¾L | as frame 0 | 0 |

That is 7 new drawings: side, back ¾, back, front, crouch, tuck and ball. Everything else is reused or generated.
The lift is baked into the art: pixel cells are drawn higher in their 43 × 40 box, which has 9 cells of headroom, and
vector parts get a higher `origin`. The behaviour layer sets no motion of its own. During the spin it only stops
idle's bob.

## Pixel: expressible as is

- Put a 24-frame `idle-spin` animation on the `body` layer (state channel, `hit: true`). Use a grid sheet with
  `"ms": 100`, and let the frame list repeat cell indices (`manifest.rs` `rects`), so the 24 frames need about 15
  cells (6 drawn, the crouch squashed, 3 mirrored, 3 rotated, 2 composited). That is a body sheet of about 645 × 40 px, against the
  2048 px limit. The pack goes from 42 frames to 66, against `MAX_FRAMES` 512.
- Each frame is the whole figure, so the layered eyes, arms and feet must not draw over it. The behaviour sends a
  `turn` value on the eyes, arms and feet channels for the whole act. The pack's `eyes`, `arms` and `feet` layers
  declare `"turn": null`. Prop, fx and headphones are already `none`/`off` in idle, so their existing null fallbacks
  hide them. A pack without `turn` falls back to its default, and never sees the act anyway (below).
- `hamster-pixel` has `mirror: none`, so the spin plays the same way facing either side. Mirrored views are baked
  cells, never runtime flips.

## Vector: expressible with a phase table

The format's limits that matter:
- A vector value draws a single drawing. A vector `Animation`'s frames are `Art::Vector` and "only time them", so a
  value can't flip through shapes.
- `part_animations` holds one key list per (layer, value), shared by all the parts it names. Parts differ only by
  `phase`. Keys hold rather than tween (`compose::step`), which helps here.
- Keys scale uniformly, so a key can't make a side or back view out of the front, or mirror a part. The asymmetric
  head needs real three-quarter drawings, as the spec's research said.
- `hamster-vector` already has 16 layers, which is `MAX_LAYERS`. A dedicated turn layer doesn't fit.

The construction, on the existing `body` layer:
1. Body gets parts: `torso` (no `shape`, so it draws the value's drawing, today's `idle`) and `view-0` … `view-23`,
   each with a fixed `shape` naming its view drawing (`turn-side`, `turn-back`, …). The `origin` carries the lift.
   Repeated views share one shape, so the path data is stored once. The pack goes from 23 to 48 parts, against the
   limit of 256.
2. `idle-spin`'s drawing is empty (`{"d": "M0 0"}`), so `torso` vanishes during the spin.
3. `idle-spin` animates all 24 view parts with keys `[{opacity 0, ms 2300}, {opacity 1, ms 100}]`, loop, and phase
   `((2300 - 100·i) / 2400) mod 1` for `view-i`. Then part i shows during [100·i, 100·i + 100) ms. Key 0 is the
   invisible one, so under reduced motion (`pose.still` takes key 0) every view stays hidden.
4. Every other value the body layer resolves to needs the view parts hidden: a one-key `{opacity 0}` animation for
   `idle`, which values without their own body art fall back to. **Gotcha:** once task .2/.3 gives the body its own
   drawings for `sad`, `tired` or `news`, each of those values needs the same hiding animation, or the 24 views show
   at full opacity. The generator should add the animation to every body value, and a test should check it.
5. `ears` (state channel, drawn behind) gets `"idle-spin": null`. Eyes, arms and feet hide via `turn` as in pixel.
   Frames 0, 9, 21-23 are whole-figure composites made by the generator from the existing body, ears, eyes, arms and
   feet shapes.
6. `mirror: flip` (a perspective pet mirrors whole). Facing left, the whole spin mirrors and turns the other way,
   which is correct.

Cost to the renderer: `vector::render` rasterises every placed item, including those at opacity 0. During any body
value, about 23 hidden views of 10-30 shapes each get built into skia paths and filled at alpha 0, every frame.
Skipping items whose opacity is 0 in `vector::scene` is a one-line fix that also helps the existing fx layers.

A cleaner format exists, but it's a format change. Let a vector animation's frames name a shape (`"frames": [{"shape":
"turn-side", "ms": 100}, …]`), so one part or the whole layer flips through drawings. That removes the 24-part
phase table, the hiding gotcha and the hidden-render cost. It's worth it only if a second pet wants a vector
flipbook. Otherwise the phase table, written by the generator, is enough.

## Painted

No painted Hamster exists. A painted pack is baked from its vector one (`painted.rs`), "a frame per stretch of time
in which no part's key changes". The phase table becomes exactly 24 body frames of full-figure crops at
`pixel_size` 0.5 (about 260 × 240 each). That adds about three rows to a 2048-wide sheet. It needs no extra drawing.

## Which state plays it

Make it an occasional idle act, `idle-spin` (state key, in the `idle-<act>` family; `IdleAct::Spin`).
- Busy moods are out. Working and thinking carry the laptop, the glasses (task .3), the screen and the dots. A turn
  that shows the back would have to hide or redraw them all for each view.
- Done keeps its mood, timings and goldens (R3), and opens with the aura.
- Sad, tired and news are cues that must read at once. A 2.4 s spin delays them.
- Idle is the pet's spare time, and its acts already break up idling. A rare spin reads as a treat.

Gating:
- **The production-only switch** (task .1) is off in the C# golden replays. Off, the act list and the RNG draws are
  unchanged (`["look", "hop", "stretch", "wiggle", "tap"][rng.next_int(5)]`), so `tests/golden.rs` keeps its pixels.
- **When the switch is on:** after an act is picked, one extra roll (about 1 in 6) swaps it for `spin`. That happens
  only if the pack draws `idle-spin` (some state layer `draws` it, checked once in `set_pack`), and at most once a
  minute. The rate is the owner's taste.
- **Reduced motion** plays no idle acts, so no spin and no first-frame cue is needed.
- **Hover:** today a hovered pet keeps its act but doesn't show it. A spin should play through hover instead,
  because hiding it mid-turn and resuming later would jump. The hover wave shows after the spin ends.

## Finishing the cycle before a mood change

Today `Tracker::pose` (`pose.rs`) restarts a channel's clock on every value change. In `compute_pose` (`pet.rs`), a
new mood clears `idle_act` at once (`if st != self.anim_state { self.idle_act = None; … }`). So a chat that starts
working cuts the spin wherever it is, often on the back view.

Smallest mechanism, all in `Pet::compute_pose`, after the mood is final (after task .1's pet-mood step):
1. When the spin starts, set `idle_until = idle_start + spin_len`. `spin_len` is the pack's cycle length for
   `idle-spin`: the frames' `ms` sum (raster), or the part animation's key sum (vector). It is read once in
   `set_pack`, so a pack can retime it.
2. On a mood change while `idle_act == Some("spin") && t < idle_until`: if `drg` (run or dangle) or
   `t < inp.poke_until` (poke), drop the act as today. Otherwise keep `st = "idle"` until `idle_until`. The pose's
   values don't change, so the `Tracker` sees no change and `since` doesn't restart. Nothing in the Tracker changes.
3. When the held mood finally applies, start its clock at the hold's end, `since = t - max(inp.state_since,
   hold_end)`. Otherwise done would lose up to 2.4 s of its aura and hop.

Why not a pack-declared "hold to end" flag: the Tracker times each channel on its own. Holding only the state
channel would let the eyes, arms and props switch to the new mood's values in the middle of the turn, drawn over
the back view. The whole pose has to be held, and only the behaviour layer can do that. A flag would also add a
format key, validation and docs for one act. If a second finishing act ever appears, promote the code constant to a
pack flag then.

The open question for the owner is attention. It waits at most 2.4 s, the same as every other mood. If the owner
wants a question to cut the spin, add `attention` to the interrupts, one more condition.

## Cost per pack kind

| | new drawing | generated | format/engine | pack growth |
|---|---|---|---|---|
| Pixel | 6 cells of 28 × 28 (side, back ¾, back, front, tuck, ball), about 450-600 cells each, plus tidying | 3 mirrors, 3 rotations, 2 composites, the crouch | none beyond the behaviour work | body.png 43 → about 645 px wide; pack.json +1 KB |
| Vector | 7 views, about 10-20 shapes each (about 90 shapes), matched to the boxy silhouette and task .3's shading | path mirror and rotate helpers; 3 mirrored views, 3 ball rotations, 2 composites (about 110 more shapes); 25 parts; the phase table | none (or the zero-opacity skip) | shapes 163 → about 360; pack.json 41 KB → about 100 KB |
| Painted | none | the bake makes 24 frames | none | about 3 more sheet rows |

Shared behaviour work, the same for every kind: `IdleAct::Spin`, the `turn` value on eyes, arms and feet, the hold,
the gating, and tests.

## Proposed follow-up tasks

Proposed only. The spec keeps the turnaround itself out of fnt-72.

1. **Behaviour: `idle-spin` act, `turn` values, hold to end** (M). Covers `IdleAct::Spin` and its key, the `turn`
   keys on eyes, arms and feet, the hold with drag and poke interrupting, the since-at-hold-end fix, gating by the
   production switch and pack support, and the act playing through hover. Tests: goldens unchanged with the switch
   off; R5's matrix gets rows where the spin finishes for every mood and is cut by run, dangle and poke.
2. **Vector renderer skips zero-opacity items** (S). A filter in `vector::scene`, plus a test that a hidden part draws
   nothing and costs no path build. Do it before task 4.
3. **Pixel Hamster turnaround** (M). Add the 6 drawings, the squashed crouch and the mirror/rotate/composite helpers to
   `hamster_pixel_gen.py`, the 24-frame `idle-spin`, and the `turn: null` fallbacks. Add a preview strip in the pack
   sheet for the owner. Depends on 1, and on task .3's shading.
4. **Vector Hamster turnaround** (L). Add the 7 views, path mirror/rotate helpers, the 24 view parts and the phase
   table to `hamster_gen.py`, with a hiding animation on every body value and a test for it, plus the empty
   `idle-spin` torso and the ears null. Then re-bake. Depends on 1, 2 and task .3.
5. **Docs** (S). Add `idle-spin` and the `turn` value to `docs/MAKING-A-PET.md`, both copies of the
   `aipet-avatar-pack` skill and `ARCHITECTURE.md` §5.3. Explain how to flip through vector views with phases, and
   the hiding gotcha.
6. *Optional:* **vector frames that name a shape** (M). A format extension that replaces the phase table if another
   pet wants a vector flipbook. Not needed for the Hamster.

## Note for fnt-72's other tasks

`hamster-vector` is at `MAX_LAYERS` (16). Task .2 adds `tears`, `aura` and `heart` through `effect_layers`, one layer
per effect. On this pack that goes over the limit and the pack is refused, unless effect layers are merged or the
limit is raised. The turnaround above avoids new layers for this reason.
