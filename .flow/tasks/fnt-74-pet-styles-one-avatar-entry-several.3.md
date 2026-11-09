---
satisfies: [R7]
---
# fnt-74-pet-styles-one-avatar-entry-several.3 Docs, the skill, and FNT-71's avatar tools take a style

## Description
Describe pets, styles and themes for pack authors and in the architecture, and amend FNT-71's tool schemas so the MCP server is built with styles (R7).

**Size:** S
**Files:** `docs/MAKING-A-PET.md`, `docs/ARCHITECTURE.md`, `avatars/README.md`, `.claude/skills/aipet-avatar-pack/**`, `.agents/skills/aipet-avatar-pack/**`, `.flow/specs/fnt-71-mcp-server-in-the-pet.md` and its tasks 5 and 6
**Touches:** [docs/**, avatars/README.md, .claude/skills/aipet-avatar-pack/**, .agents/skills/aipet-avatar-pack/**, .flow/specs/fnt-71-mcp-server-in-the-pet.md, .flow/tasks/fnt-71-mcp-server-in-the-pet.*]

### Approach
- MAKING-A-PET: a "Pets and styles" section (the `pet` and `style` keys, pet folders, the default style, the built-in-wins rule) and the manifest table rows; the skill's `references/manifest.md` and `themes.md` the same, both copies identical.
- FNT-71: through `flowctl spec set-plan` and `task set-spec`, change the API table rows for `list_avatars` and `wear_avatar`, R7 and R10's existing-name check to (pet, style), and tasks 5 and 6's approach lines.

## Acceptance
- [ ] The docs and both skill copies describe pets and styles; `diff -r` of the skill copies is clean
- [ ] FNT-71's spec and tasks 5-6 take a style, and validate

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
