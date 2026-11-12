# Themes

A theme is the same pet in other colours, or for a raster pack, drawn from other sheets. The Avatars page shows the
worn pet's themes as pills; `config.json` keeps the chosen one as `AvatarTheme`. A pet has at most 64 themes, its
own and the user's together. People's guide: `docs/MAKING-A-PET.md`, "Themes".

## Colour themes, in the manifest

```json
"palette": { "body": "#5CC8E0", "outline": "#123C4A", "eyes": "#0E2530", "accent": "#3A9FBF" },
"theme": "Rain",
"themes": { "Dusk": { "body": "#B08CE0", "outline": "#2E1F4A", "accent": "#8A66C2" } },
"accent": "$accent"
```

- `palette` is the default theme; `theme` names it ("Default" otherwise). Each entry of `themes` changes some of the
  palette's colours; a name the palette lacks is refused.
- A pet with one look needs no `themes`: a `palette` on its own is fine, and still lets the user make themes in
  Settings. `--check-pack` names themes only when the pack has some besides the default.
- Colours: `#RGB`, `#RRGGBB` or `#RRGGBBAA`, not `none`.
- **Vector**: shapes' `fill` and `stroke`, and the pack's `accent` and `glow`, use `$name`. A `$name` the palette
  lacks is refused. A colour written as hex never changes with the theme, which is right for fixed colours (white
  glints, the sparkles' yellow) and wrong for anything that should follow the theme.
- **Pixel**: the palette names exact colours in the sheets; a theme swaps those pixels (keeping their alpha) in
  every themed layer (all but `fx`, unless a layer says `"themed"`). No two palette names may share a colour (RGB),
  and the sheets' colours must match the palette exactly. `accent` and `glow` may be `$name`.
- **Painted**: no palette and no colour themes (its colours are blended). Use sheet themes.

Name colours for what they are (`body`, `shade`, `rim`, `outline`, `eye`, …), not for their hue: a theme makes
`"green": "#C08FD8"` confusing. `avatars/reference/vector_pack.py`'s `named(pack, colours, theme, themes)` turns a
generated pack's hex colours into `$name`s and writes the palette, the default theme and the themes (keeping only the
names the pack uses).

## Sheet themes (pixel and painted)

```json
"themes": { "Lilac": { "sheets": { "body.png": "body-lilac.png", "arms.png": "arms-lilac.png" } } }
```

Each key is a sheet the pack's frames use; each value a file in the pack, the same width and height. A theme changes
colours or sheets, never both. Pip (painted)'s Lilac is one (`rust/crates/aipet-sprite/packs/pip-painted/`).

## Theme files (the user's own themes)

A theme for any pet with named colours, built-ins included, without touching its pack: a JSON file in the data
folder's `avatars/themes/` (`~/.local/share/AiPet/avatars/themes/` on Linux, `%LOCALAPPDATA%\AiPet\avatars\themes\` on
Windows):

```json
{ "pet": "Hood (vector)", "theme": "Red",
  "palette": { "body": "#441010", "hi": "#FF4A3A", "eye": "#FFB15C", "glow": "#FF5A2B" } }
```

- Exactly the keys `pet`, `theme` and `palette` (others are refused).
- `pet` is the pet's `name`; `theme` a name the pet doesn't have yet (not empty, no space at either end); `palette`
  some of the pet's named colours.
- Files load in sorted order after every pack. One that doesn't fit is listed on the Avatars page with its reason.
- In the app: Settings → Avatars shows the theme editor on the worn pet's theme. A pet's own themes are locked, and
  **Copy as new theme** makes a new one from them; the user's theme files can be edited (**Save**) and deleted
  (**Delete**), which write or remove these files.
- Example: `avatars/samples/themes/hood-red.json`.
- Check one: `AiPet --check-pack <file>`. In a `themes/` folder it is checked against the pets of the avatars
  folder above it, with the other theme files there; elsewhere against the built-ins and the pack folders beside it.
  It prints `OK <pet> in <theme> (a theme)`.

To find a pet's colour names, read its `pack.json`'s `palette` (built-ins: `rust/crates/aipet-sprite/packs/`). The
baked Sprout and Hood are pixel packs with palettes too.

## Recolouring a pet

- A new colourway of an existing pet for one user: a theme file.
- A new colourway that ships with the pet: an entry in its generator's `themes` (vector), or a sheet theme
  (painted). Run the generator, then `--check-pack`; the `OK` line lists the themes.
- Review every theme on the three backgrounds: a dark theme's outline may vanish on near-black
  (`design-conventions.md`).
