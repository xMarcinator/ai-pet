# fn-2-layered-sprite-and-vector-pets.18 Sheet themes: alternate sprite sheets as a pet's themes (not editable yet)

## Description
Sheet themes (owner's idea, 2026-11-07): sprite-sheet pets whose art isn't a small named palette (painted packs, Aseprite and grid packs drawn freely) reuse task 17's theme concept, but a theme is an alternate set of sheets instead of colours. They're picked like any theme, and not editable for now; maybe later (tinting, or editing the vector source a painted pack was made from).

**Size:** S–M

### Design
- In `pack.json`, a theme entry may be `{"sheets": {"body.png": "body-night.png", …}}`, naming replacement sheets for some of the pack's sheets (same size, so frames still line up). It can't be mixed with colour overrides in the same theme.
- Loading follows the same rules as the pack's own sheets: contained in the folder, PNG limits, and the decoded total within the pack's 64 MB (all themes counted).
- Settings shows sheet themes beside colour themes. A sheet theme has no colour editor, and "New theme" is hidden for a pet that has only sheet themes.
- The painted Pip could ship a second sheet theme, painted from a vector Pip colour theme by the painted bake.

## Acceptance
- [ ] A pack's theme can name replacement sheets; bad, missing or mismatched sheets skip the pack with a reason (tests)
- [ ] Settings lists sheet themes, picks them, and offers no editor for them
- [ ] fmt, clippy and every test pass


## Done summary
Sheet themes (commit 31a0ab8, by a subagent): a theme may be {"sheets": {"body.png": "body-night.png"}}, read through the pack's own sheet rules; mixed, missing, mismatched or out-of-folder sheets skip the pack. Painted packs may have sheet themes. Settings lists and picks them with no editor; New theme is hidden for a pet without named colours. The painted Pip ships Peach and Lilac (3 Lilac sheets, 13.6 KB), painted from the vector Pip's new Lilac colour theme. Lilac keeps the peach accent (sheet themes don't change accents).

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits:
- Tests:
- PRs: