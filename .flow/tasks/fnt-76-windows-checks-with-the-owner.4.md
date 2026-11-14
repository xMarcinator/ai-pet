---
satisfies: [R5]
---
# fnt-76-windows-checks-with-the-owner.4 The MCP server on Windows: hands-on and budgets (from FNT-71 task 8)

## Description
The Windows half of FNT-71 task 8 (the MCP server in the pet), moved here on the owner's word, 2026-11-13, so FNT-71 can close on its Linux half and merge. Linux is done: `rust/proofs/mcp-server.md` (real Claude Code and Codex sessions, every tool, the pet restarted mid-session, Codex's interactive re-trust, Settings following tool changes) and `rust/proofs/memory.md` (R13 and R15 met).

**Tooling ready (2026-11-14, tested on Linux only):** `scripts/perf/mcp-memory-windows.sh` (the paired R15 run), `scripts/perf/pet-calm-windows.sh` with `CONNECT=1` / `LISTENER=off` (R13), and `scripts/perf/mcp-client.sh` (curl only; Git Bash has no Python). Judge R15 by the working set: on Linux .NET's private figure counts address space, on Windows it is commit.

**Size:** M
**Files:** `rust/proofs/mcp-server.md` (its Windows section), `rust/proofs/memory.md`, `scripts/perf/` (Windows versions of `mcp-memory.sh` and `pet-calm.sh`'s CONNECT and LISTENER options, on task 1's sampler)

### Approach
- Build first, from Git Bash: `cd rust && cargo build --release -p aipet -p aipet-hook`. The listener's `SO_EXCLUSIVEADDRUSE` bind (`aipet-mcp/src/lib.rs`, `exclusive`) was never compiled for Windows (ring's build needs a Windows C compiler, which the Linux machine lacks); fix it if it doesn't build, and check that a second program can't bind 127.0.0.1:47615 while the pet listens.
- Register with `AiPet --install claude` and `AiPet --install codex` (and `packaging/windows/install.ps1`'s path, read, not run in PowerShell by Claude) into sandbox `CLAUDE_CONFIG_DIR` and `CODEX_HOME` folders, as the Linux pass did; `AiPet --doctor` for each.
- The Linux checklist L1-L11 on Windows, in the same table: both agents' status, each tool, the pet stopped and restarted mid-session, a whole session with the pet down, Codex's re-trust in an interactive `codex`, Settings following a tool's change, `mcp-header.cmd` (Codex runs it with a bare environment: check `AIPET_DATA_DIR` in it reaches AiPet), the token file's ACL (only the user), and the Codex plugin's `hooks/codex-mcp.json` with `${PLUGIN_ROOT}` in its header helper.
- R13: calm CPU and RSS with the listener on and an MCP client connected, beside the C# pet and the listener off, by task 1's method. R15: the paired run (same build, listener on with a client idle vs its port held), then a hook body just under the 4 MiB cap: the working set must settle within 5 MB of the "off" pet within 60 s. Windows' heap returns large frees itself, so the Linux `malloc_trim` has no Windows twin; if it doesn't settle, the remedy is a lower hook-body cap or `HeapCompact`.

### Investigation targets
**Required:**
- `rust/proofs/mcp-server.md` - the Linux checklist and its setup
- `scripts/perf/mcp-memory.sh`, `scripts/perf/mcp-client.py` - the paired R15 run
- task 1's sampler and bench script

### Key context
- Git Bash only, never PowerShell; ask before installing anything; the owner starts or watches every GUI run.
- Never touch the owner's real `%USERPROFILE%\.claude` or `.codex`: sandbox config folders, and the agents pointed at the generated files (`--settings`/`--mcp-config` for Claude, `-c` overrides or a sandbox `CODEX_HOME` the owner signs in to for Codex).

## Acceptance
- [ ] AiPet builds on Windows with the `SO_EXCLUSIVEADDRUSE` bind, and a second program can't take the pet's port
- [ ] The Linux checklist's checks recorded for Windows in `rust/proofs/mcp-server.md`, with the owner
- [ ] R13's numbers on Windows recorded in `rust/proofs/memory.md` and within fn-1 R18's budgets
- [ ] R15 measured paired on Windows and recorded: at most 5 MB idle, and back within it 60 s after a cap-sized hook body


## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
