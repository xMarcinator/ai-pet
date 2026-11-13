---
satisfies: [R13, R14, R15]
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
- [ ] R15 measured on Linux and Windows and recorded
- [ ] R13's numbers recorded for Linux and Windows and within budget
- [ ] Windows hands-on pass recorded with the owner
- [ ] Every doc in R14 updated; the two skill copies are identical

## Done summary
Linux half done; the Windows half moved to fnt-76-windows-checks-with-the-owner.4 (FNT-76 R5) on the owner's word, 2026-11-13.

- Docs: every doc R14 lists (3c4d402), both skill copies identical.
- Hands-on on Linux (`rust/proofs/mcp-server.md`): real Claude Code 2.1.322 and Codex 0.167.2 sessions against a sandboxed pet with the files AiPet --install writes; every tool from a real agent; the pet stopped and restarted mid-session; a session with the pet down; Codex's interactive re-trust and Settings following tool changes, with the owner.
- R15 on Linux, paired (`scripts/perf/mcp-memory.sh`): +3.2-3.8 MB RSS with an agent connected and idle, back to +3.1-3.5 MB within 10 s of a cap-sized hook body.
- R13 on Linux (`scripts/perf/pet-calm.sh` CONNECT/LISTENER): 4.9-5.0 % calm CPU and 131 MB RSS with the listener on and a client connected, against the C# pet's 12.3 % and 242 MB; the listener's CPU is within noise.
- Fixes the pass found: the Codex header helper names the data folder (Codex gives it a bare environment); malloc_trim after large requests (a cap-sized body left 5 MB); no "uint16" format in status's schema; the doctor matches app-server's "mcpTool"; SO_EXCLUSIVEADDRUSE on Windows (not yet compiled there).

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 3c4d402, abc535e, ba4656b, f025d49, 690ed11
- Tests: cd rust && cargo test --offline --workspace, cargo clippy --offline --workspace --all-targets -- -D warnings, cargo hakari verify
- PRs: