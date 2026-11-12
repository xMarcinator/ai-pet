---
name: aipet-avatar-pack
description: Make, edit, recolour, theme, check or review an AiPet avatar pack (a pet, sprite or theme for the AiPet desktop pet) in the ai-pet repository. Use when the user wants a new pet drawn (pixel, painted or vector), an existing pet or its pack.json changed, a colour theme or theme file added, a pack checked with AiPet --check-pack, a --check-pack reason explained, or a pet's drawing reviewed.
---

# Making an AiPet avatar pack

A pet is a **pack**: a folder with a manifest (`pack.json`) and its art. The app works out a **pose** every frame
(what each channel shows: the mood, the eyes, the arms, …) and the pack only says how to draw each value. You can
make a pack that loads without reading any Rust: this skill and its references hold everything the loader checks.

`docs/MAKING-A-PET.md` is the guide for people. Read it once for the big picture; the references here are the
agent's working notes and checklists, and point back to the guide's sections rather than repeat them.

## The workflow

Work through these steps in order. Keep the user in the loop at steps 2, 6 and 7: the owner decides how the pet
looks.

1. **Pick the kind** with the user: `pixel` (pixel-art PNG sheets), `painted` (smooth PNG sheets) or `vector`
   (shapes in the manifest). Rive packs are not supported: see `references/rive.md`. Load the reference for the kind:
   `references/pixel-and-painted.md` or `references/vector.md`, and `references/aseprite.md` if the art comes from
   Aseprite.
2. **Gather references** from the user: their pixel art, sketches or images, the colours, and what the pet should
   feel like. Ask before inventing a character. Read `references/design-conventions.md` before drawing anything.
3. **List the states to cover.** `avatars/reference/pet_states.py` has every state (`STATES`) and second frame
   (`PHASES`) the pet shows. `references/channels.md` maps them to channel values and fallbacks. Decide with the user
   which values the pet draws itself and which fall back. Cover the newer states too (`sad`, `tired`, `news`: tears,
   sleepy eyes, the envelope); a pet that skips them shows attention or idle instead. The generator draws only the pet's own parts per pose; the
   shared `pet_states.py` adds the effects (the tears, hearts and golden glow too), the magnifier, the envelope, the headphones, the whole-body motion, the shadow, mirroring, the
   review page and the state sheet.
4. **Draw with a generator**, a Python script beside the pack, modelled on `avatars/reference/hood_gen.py` and
   `sprout_gen.py` (vector; a pixel pet's generator writes its sheets instead, see
   `references/pixel-and-painted.md`). Named parts at their origins, canvas-ready shapes only, poses by moving, rotating or swapping parts. Never hand-edit a generated `pack.json`:
   change the generator and run it again.
5. **Produce the review page and the state sheet**: every state in 130 × 120 boxes on white, mid-grey and
   near-black, with the user's original beside the new drawing (`pet_states.write_pages`). See
   `references/review.md`.
6. **Iterate with the user.** Show the review page (screenshots of it), take their notes, redraw. Record each
   decision the owner makes; the Codex review must treat them as fixed.
7. **Run a Codex review** (`codex exec`, read-only, the review page's screenshots attached, the owner's decisions
   listed as fixed): `references/review.md`. Bring its findings to the user; don't apply them unasked.
8. **Emit the pack and check it**: the generator writes `pack.json`; then
   `rust/target/release/AiPet --check-pack <pack folder>` must print `OK …`. Any other line is the one reason the
   pack doesn't load: look it up in `references/limits-and-reasons.md`, fix the generator, run both again.
9. **Add the pack** where it belongs: a user's pet goes in their avatars folder; a sample in `avatars/samples/`; a
   built-in in `rust/crates/aipet-sprite/packs/` (see "Where packs go" below).

`references/worked-example.md` walks through all of this on a small pet, `avatars/samples/skill-example/`, that
passes `--check-pack`. Start there if this is your first pack.

## References (load only what the task needs)

| File | Load it when |
|---|---|
| `references/manifest.md` | Writing or reading any `pack.json`: every key, Part motion, an example per kind |
| `references/channels.md` | Choosing what to draw: channels, every value, fallbacks, how `pet_states.py` maps to them |
| `references/themes.md` | A palette, `$name` colours, `themes`, sheet themes, or a user's theme file |
| `references/vector.md` | Vector packs: shapes, parts, generators and `vector_pack.py` |
| `references/pixel-and-painted.md` | Pixel or painted packs: sheets, explicit and grid frames, raster part motion |
| `references/aseprite.md` | The art is a `.aseprite` file or an Aseprite export |
| `references/rive.md` | Someone asks for a Rive pack (they aren't supported) |
| `references/limits-and-reasons.md` | A limit, or any line `--check-pack` prints |
| `references/design-conventions.md` | Before drawing, and when reviewing a drawing |
| `references/review.md` | The review page, the state sheet and the Codex review |
| `references/worked-example.md` | A first pack, or a template to copy |

## Checking

```sh
rust/target/release/AiPet --check-pack <pack folder>    # OK <name> (<kind>, <n> layers, <n> frames[, themes …])
rust/target/release/AiPet --check-pack <theme file>     # OK <pet> in <theme> (a theme)
rust/target/release/AiPet --check-pack <legacy .json>   # OK <name> (a legacy avatar)
```

It loads exactly as the pet does, prints one line, exits 0 or 1, never starts the pet and never writes anything.
Pass the folder, not its `pack.json`: a `.json` file is read as a theme file or a legacy avatar. If the release
binary is missing, say so and ask; don't start a `cargo build` unasked, since another session may be building.

## Where packs go

- **A user's pet**: the data folder's `avatars/<folder>/` (`~/.local/share/AiPet/avatars` on Linux,
  `%LOCALAPPDATA%\AiPet\avatars` on Windows). The user clicks **Reload** on Settings → Avatars.
- **A theme for an existing pet**: a theme file in `avatars/themes/` there (`references/themes.md`), or **New
  theme** on the Avatars page.
- **A sample**: `avatars/samples/<folder>/`, with a line in `avatars/README.md`.
- **A built-in pet** (only when the user asks): the pack in `rust/crates/aipet-sprite/packs/<folder>/`, its files
  listed in `rust/crates/aipet-sprite/src/builtin.rs` and the pet added to `all()`. A generated one is also listed in
  `rust/crates/aipet-sprite/tests/vector_packs.rs`, which checks the generator still writes it, and must draw every
  value of `pet_states.py` explicitly (a fallback that names the value counts) with all 15 `look` directions. The same
  test runs over every built-in pack, baked pixel ones included: some layer must draw each value with art (a `null`
  fallback alone fails), and each of `sad`, `tired` and `news` must be drawn as a state or by its sign.
  Built-ins are a code change: run the crate's tests, or say they weren't run.

## Things that are easy to get wrong

- The pet's ground is the anchor, `[65, 110]` in a vector pack. Part positions and raster frames outside the
  130 × 120 box and its 10 px margin are refused; shapes that stick out of it are cut off by the window.
- A raster layer moves as one piece: at most one part per raster layer; `rotate`, `scale` and `opacity` keys are
  vector-only.
- A vector shape's keys are checked (`stroke_width`, not `stroke-width`), and a wrong one fails as "data did not
  match any variant of untagged enum Drawing". Everything else ignores unknown keys, so a misspelt layer key
  silently does nothing. Read the review page, not just the `OK`.
- `accent` and `glow` written as plain colours are `#RRGGBB` or `#AARRGGBB` (alpha first); shape and palette
  colours are `#RGB`, `#RRGGBB` or `#RRGGBBAA` (alpha last). Use `$name` to avoid the difference.
- Users choose Pet size (75 to 200 %) and Bubble size in Settings → General; the pack is always drawn into the same
  art box and scaled, so draw for the box, and check readability at the smallest size.
