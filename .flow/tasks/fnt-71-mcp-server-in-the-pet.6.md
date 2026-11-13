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
- Name allowlist and path containment per the spec's Edge Cases; delete moves to `<data>/trash/` (outside `<data>/avatars/`, so the loader never lists it). Deleting the worn theme sends `SelectTheme` default through task 5's channel.
- `check_pack` calls `load::check` (`aipet-sprite/src/load.rs:986`), the same function `--check-pack` uses (`aipet/src/main.rs:62-76`).
- `install_pack`: canonicalize, require under the home dir, copy into `<data>/staging/<uuid>` without following symlinks, counting bytes and entries as it walks and stopping at 64 MiB or 2,000 entries. Then `check`. Refuse an existing pet and style, or name, unless `replace` (FNT-74: `load::groups`, the loader's clash rules); replacing a built-in style is refused; the folder to swap is the existing user style's own folder, found from where it loaded, not `<data>/avatars/<name>`. Install with a serialized swap (one install at a time behind a mutex): rename an existing pack to `<data>/staging/<uuid>.old`, rename the staged pack into `<data>/avatars/<name>`, restore the backup if that rename fails, delete the backup only after success. Then reload via the UI channel. Clean staging on every exit path.

### Investigation targets
**Required:**
- `rust/crates/aipet-ui/src/settings/avatars.rs:103-120,372-470,589-640`
- `rust/crates/aipet-sprite/src/load.rs:141,189,986` - all, THEMES, check
- `rust/crates/aipet-sprite/src/avatar.rs:135` - custom_dir
**Optional:**
- `.claude/skills/aipet-avatar-pack/SKILL.md` - how packs and themes are made today

### Key context
- Rename across volumes fails: staging and trash live under `<data>/`, the same volume as `<data>/avatars/`.
- `std::fs::rename` cannot replace a non-empty directory on every OS: hence the backup swap.

## Acceptance
- [ ] Settings → Avatars New/Edit/Delete produce the same files as before the move
- [ ] Tests for each R9 and R10 error case, each leaving `<data>/avatars/` unchanged, including a replacement whose final rename fails (the old pack is restored) and a source with oversized or too many unreferenced files
- [ ] `list_avatars` never shows staging or trash, during an install and after a theme delete
- [ ] The five tools are in task 2's conformance table
- [ ] `check_pack` output equals `AiPet --check-pack` output for the fixture packs
- [ ] An installed pack appears in `list_avatars` and can be worn
- [ ] Tests: replacing a user style whose folder is named differently swaps that folder; replacing a built-in style is `isError`

## Done summary
Theme naming, save and delete moved to aipet-sprite::themes (allowlist, trash under <data>/trash); the Avatars page uses it. Five tools: list_themes, save_theme, delete_theme, check_pack (load::check, same as --check-pack, tested against the binary), install_pack (staging copy with no links and the 64 MiB / 2,000 entry caps, check, clash rules with replace, backup swap of the style's own folder, restored on failure, one at a time, then the pet reloads). Conformance table has all 13 tools.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 301d403
- Tests: cargo test -p aipet-sprite -p aipet-ui -p aipet-mcp -p aipet, aipet-mcp tests/listener.rs every_tool_conforms, aipet tests/check_pack.rs
- PRs: