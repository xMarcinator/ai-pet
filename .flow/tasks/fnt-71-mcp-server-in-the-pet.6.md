---
satisfies: [R9, R10, R11]
---
# fnt-71-mcp-server-in-the-pet.6 Theme and pack tools on shared file code

## Description
Move the theme file logic out of the Avatars page into shared code, then build `list_themes`, `save_theme`, `delete_theme`, `check_pack` and `install_pack` on it. The pet does all copying and checking; the agent only names a path.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/settings/avatars.rs`, `rust/crates/aipet-sprite/src/themes.rs` (new), `rust/crates/aipet-sprite/src/install.rs` (new), `rust/crates/aipet-sprite/src/load.rs`, `rust/crates/aipet-mcp/src/tools.rs`
**Touches:** [rust/crates/aipet-ui/src/settings/avatars.rs, rust/crates/aipet-sprite/src/**, rust/crates/aipet-mcp/**]

### Approach
- Extract naming, save and delete from `aipet-ui/src/settings/avatars.rs:103-120,372-470,589-640` into `aipet-sprite`; the page calls the extracted functions (equivalence: the page's existing tests plus a before/after file listing for New/Edit/Delete).
- Name allowlist and path containment per the spec's Edge Cases; delete moves to `<data>/avatars/.trash/`. Deleting the worn theme sends `SelectTheme` default through task 5's channel.
- `check_pack` calls `load::check` (`aipet-sprite/src/load.rs:986`), the same function `--check-pack` uses (`aipet/src/main.rs:62-76`).
- `install_pack`: canonicalize, require under the home dir, copy into `<data>/avatars/.staging/<uuid>` without following symlinks and within the loader's limits, `check`, then rename into place (refuse an existing name unless `replace`), then reload via the UI channel. Clean staging on every exit path.

### Investigation targets
**Required:**
- `rust/crates/aipet-ui/src/settings/avatars.rs:103-120,372-470,589-640`
- `rust/crates/aipet-sprite/src/load.rs:141,189,986` - all, THEMES, check
- `rust/crates/aipet-sprite/src/avatar.rs:135` - custom_dir
**Optional:**
- `.claude/skills/aipet-avatar-pack/SKILL.md` - how packs and themes are made today

### Key context
- Rename across volumes fails: stage inside `<data>/avatars/` so the final move is a same-volume rename.

## Acceptance
- [ ] Settings → Avatars New/Edit/Delete produce the same files as before the move
- [ ] Tests for each R9 and R10 error case, each leaving `<data>/avatars/` unchanged
- [ ] `check_pack` output equals `AiPet --check-pack` output for the fixture packs
- [ ] An installed pack appears in `list_avatars` and can be worn

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
