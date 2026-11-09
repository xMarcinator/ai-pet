---
satisfies: [R13, R14]
---
# fnt-71-mcp-server-in-the-pet.8 Docs, budgets and hands-on pass for the MCP server

## Description
Final measurement with every tool built, the Windows hands-on pass with the owner, and the docs R14 lists.

**Size:** M
**Files:** `README.md`, `docs/ARCHITECTURE.md`, `docs/HANDOFF.md`, `docs/MAKING-A-PET.md`, `.claude/skills/aipet-avatar-pack/**`, `.agents/skills/aipet-avatar-pack/**`, `plugins/aipet/README.md`, `CHANGELOG.md`, `THIRD-PARTY-NOTICES.md`, `rust/proofs/memory.md`, `rust/proofs/mcp-server.md` (new)
**Touches:** [README.md, docs/**, .claude/skills/aipet-avatar-pack/**, .agents/skills/aipet-avatar-pack/**, plugins/aipet/README.md, CHANGELOG.md, THIRD-PARTY-NOTICES.md, rust/proofs/**]

### Approach
- Re-run task 2's measurement on Linux and Windows with an agent connected and hook traffic replayed; record in `rust/proofs/memory.md`.
- Hands-on checklist in the shape of `rust/proofs/windows-hands-on.md`: both agents' status, each tool, pet stopped and restarted mid-session, Codex re-trust, Settings reflecting tool changes.
- Docs: follow docs-gap-scout's section list in the spec's Resolved via Research; ARCHITECTURE cites `[file](../path):line`; CHANGELOG under `[Unreleased]`; THIRD-PARTY-NOTICES table rows with versions from `rust/Cargo.lock`. Edit both skill copies identically (`diff -r` clean).
- HANDOFF Decisions: follow the verdict in `rust/proofs/mcp-spike.md`. On success, replace "Codex on Windows keeps its command hooks" and "Don't change the registered command strings" with the `mcp_tool` decision. On the fallback, keep both and add that Codex keeps the exe for status while the tools use MCP.

### Investigation targets
**Required:**
- `docs/ARCHITECTURE.md` §6 and §7
- `rust/proofs/windows-hands-on.md` - checklist format
- `rust/proofs/memory.md` - method

### Key context
- Windows: Git Bash only, never PowerShell; GUI checks need the owner present.

## Acceptance
- [ ] R13's numbers recorded for Linux and Windows and within budget
- [ ] Windows hands-on pass recorded with the owner
- [ ] Every doc in R14 updated; the two skill copies are identical

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
