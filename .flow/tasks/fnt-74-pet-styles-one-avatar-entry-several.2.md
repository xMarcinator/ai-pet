---
satisfies: [R3, R4]
---
# fnt-74-pet-styles-one-avatar-entry-several.2 Settings and config: one tile per pet, a Style picker, AvatarStyle

## Description
Move the UI from a flat list of packs to pets with styles: one tile per pet, a Style picker, themes per style, and `AvatarStyle` in config.json with the old names mapped (R3, R4).

**Size:** M
**Files:** `rust/crates/aipet-ui/src/{lib,shelf}.rs`, `rust/crates/aipet-ui/src/settings/avatars.rs`, `rust/crates/aipet-core/src/config.rs`, `rust/crates/aipet/src/main.rs`, `rust/crates/aipet-ui/tests/settings.rs`
**Touches:** [rust/crates/aipet-ui/src/**, rust/crates/aipet-ui/tests/**, rust/crates/aipet-core/src/config.rs, rust/crates/aipet/src/main.rs]

### Approach
- Config: `avatar_style` beside `avatar_theme` (`aipet-core/src/config.rs:51-53,181-204`), written only when not the default; it must be a real field or the Rust config drops it too.
- `choose()` (`aipet-ui/src/lib.rs:465`): map a saved pack name to its pet and style before the `<pet> (<theme>)` parse; then style, then theme, each falling back as R4 says.
- State: `avatar` becomes (pet, style); `stills` per pet in its current style; `SelectAvatar(pet)`, a new `SelectStyle(style)`, `SelectTheme` per style (`lib.rs:892-901`, `wear` at `:1071`).
- Settings → Avatars (`settings/avatars.rs:81-183,240,509`): tiles per pet; a style row of pills beside the theme pills, hidden for one style; reload keeps pet and style.
- Prefs written through `prefs()` (`lib.rs:1031`), which saves only on a real change; the app's `Host::prefs` in `rust/crates/aipet/src/main.rs:288-366` copies each field into `Config`, so it must copy `avatar_style` too.
- Reload: replace the branch in `reload()` (`settings/avatars.rs:509`) that keeps a removed worn pack as a hidden "gone" entry; a removed style falls back to the pet's default style, a removed pet to the first pet, as R4 says.
- Keep `PetUi::update` to one line for `SelectStyle`, the logic in the avatars state code.

### Investigation targets
**Required:**
- `rust/crates/aipet-ui/src/lib.rs:182-193,359-369,465-505,892-905,1031-1090`
- `rust/crates/aipet-ui/src/settings/avatars.rs:59-183,374-520`
- `rust/crates/aipet-ui/tests/settings.rs:377-479,530-670`

### Key context
- The .NET pet reads only `Avatar`: it must stay a name the .NET pet knows for Sprout and Hood.

## Acceptance
- [ ] Tests: Settings shows four built-in tiles; the Hood's Style picker offers Pixel and Vector; switching keeps Green, which both have
- [ ] Tests: configs saying `Hood (vector)`, `Pip (painted)`, `Hamster (pixel)`, `Sprout (vector)` and `Hood (green)` wear the right pet, style and theme, and the file isn't rewritten until something changes
- [ ] Tests: an unknown style or theme falls back to the default; `AvatarStyle` is written only when not the default
- [ ] A test through the app's `Host::prefs` path writes and reads back `AvatarStyle`
- [ ] Tests: after Reload with the worn style's folder removed the pet wears its default style; with the whole pet removed, the first pet
- [ ] The Settings page renders the style row (screenshot)

## Done summary
Settings → Avatars: one tile per pet (load::groups_of), showing the worn style for the worn pet; a Style pill row when a pet has more than one; SelectStyle keeps the theme by name where the new style has it. Config: AvatarStyle (written only when set; the app's copy is apply_prefs in main.rs, tested); Avatar is the pet's name; choose() maps the pet + style, then old drawing names (Hood (vector), Pip (painted), …), then '<pet> (<theme>)'. Reload falls back to the pet's default style, then the first pet (the hidden 'gone' entry is removed). Not checked: a screenshot of the Settings page (no GUI run while the owner is away).

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 26a975d
- Tests: cargo test --workspace
- PRs: