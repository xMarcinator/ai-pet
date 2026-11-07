# fn-2-layered-sprite-and-vector-pets.17 Avatar themes: named colour themes per pet, for pixel and vector pets, built-in and the user's

## Description
Colours become **themes** (owner's request, 2026-11-07): a pet ships one or more named colour themes, the user picks one per pet in Settings, and can make their own. Pixel pets are themeable too, not only vector ones. Replaces task 16's recolour files (base/name made a separate pet; a theme is a choice on the same pet).

**Size:** M–L
**Files:** `rust/crates/aipet-sprite/src/{pack.rs,manifest.rs,load.rs,builtin.rs,bake.rs}`, `rust/crates/aipet-ui/src/{lib.rs,sprite.rs,settings/avatars.rs}`, prefs/config (the avatar's theme), `avatars/reference/{vector_pack.py,hamster_pixel_gen.py,pip_gen.py,hood_gen.py}`, packs, tests

### Design
- **Manifest:** `palette` = the default theme's named colours; `theme` (optional) = the default theme's name, else "Default"; `themes` = `{ "Blue": { "body": "#16285E", … } }`, each overriding some palette colours. Validated like colours (no URLs, known names).
- **Kinds:** vector shapes use `$name` (task 16). Pixel packs: `palette` names exact sheet colours; a theme swaps those RGB values in the sheets of every layer except effects (fx channel), keeping alpha. Painted: no themes (anti-aliased).
- **Each colour once:** a pack's palette lists each colour once (first name wins), so changing one name never changes another's pixels.
- **Built-ins:** the pixel Hood gets themes Blue (default) and Green (from `avatars/hood-green.json`; a test checks the Green theme is pixel for pixel the old Hood (green) bake). The Hood (green) built-in entry goes away. The vector Hood is renamed "Hood (vector)": default Green plus a Blue theme. The pixel Sprout, the Hamsters and Pip name their colours; other themes are optional.
- **Config:** `AvatarTheme` beside `Avatar`. An old `Avatar` name such as "Hood (green)" that matches "<pet> (<theme>)" of a pet's theme selects that pet and theme. An unknown theme falls back to the pet's default.
- **User themes:** files in `<data>/avatars/themes/*.json`: `{"pet": "Hood", "theme": "Red", "palette": {…}}`. They're attached to the pet (all or nothing; reasons for an unknown pet, an unknown colour name, a bad colour, a clash with a built-in theme, a painted pet). The `themes` folder is never read as a pack. Task 16's `{"base","name"}` files and `avatars/samples/hood-red.json` become this format.
- **Settings → Avatars:** one tile per pet. Under the grid, the worn pet's themes as pills (built-in and user's), with the chosen one marked; "New theme" opens task 16's editor on the worn pet in its current theme; a user theme also gets Edit and Delete. Save writes the theme file, reads the folder again, and wears the pet in that theme.
- **`--check-pack`** checks a theme file, and the themes in a pack.

## Acceptance
- [ ] Packs carry `palette`, `theme` and `themes`; bad themes skip the pack with a reason (tests)
- [ ] Pixel packs are themeable by exact colour swaps, leaving effects alone; the pixel Hood's Green theme equals the old Hood (green) bake pixel for pixel (test)
- [ ] The built-in list has one entry per pet drawing (Hood (green) gone, "Hood (vector)" with Green and Blue themes); an old config naming "Hood (green)" wears Hood in Green
- [ ] User theme files load onto their pet, with reasons in Settings for bad ones; Settings picks, creates, edits and deletes themes, and the choice persists in config (tests)
- [ ] fmt, clippy and every test pass


## Done summary
Themes (commit c60ecc9 by a subagent, plus the tile-dot fix): pack palette/theme/themes; pixel themes swap exact colours in themed layers (a per-layer 'themed' key; the Hood's fx layer is themed so Green equals the old Hood (green) bake pixel for pixel, test); user themes in <data>/avatars/themes/*.json {pet, theme, palette}; AvatarTheme in config; 'Hood (green)' config wears Hood in Green (checked in the app); Settings theme pills with New/Edit/Delete; built-ins: Sprout, Hood (Blue, Green), Pip, Pip (painted), Hood (vector) (Green, Blue), Hamster, Sprout (vector), Hamster (pixel). Docs still say Hood (green) (task 9).

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits:
- Tests:
- PRs: