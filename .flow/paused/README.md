# Picking up flow-next (paused 2026-11-09, 22:25)

The work branch is `fn-2-layered-sprite-and-vector-pets`, pushed; CI on it is green on Linux and Windows (run
42700069684; the macOS check is cancelled before it starts, as on every run since 2026-11-06).

| Spec | State | Next |
|---|---|---|
| fn-1 (.NET → Rust) | 26 of 30 done; task 23 (ship) in progress: the release ships the Rust pet, dry run, lock test, Windows update test and notices done | The owner: clean installs on Linux and Windows, update from v0.1.0 and rollback, then cut the release. Then tasks 24-26 |
| fn-2, FNT-72, FNT-73 | every task done | completion review |
| FNT-71 (MCP server) | task 1 (spike) done: Codex `mcp_tool` hooks work; Claude HTTP hooks except SessionStart; R15 caps the server at 5 MB RSS | tasks 2-4 (listener, Claude and Codex status) can start; 5-6 wait for FNT-75's pet loop (owner) |
| FNT-75 (pet window without the GPU) | task 9 (spike) done: Settings stays in-process on iced's CPU renderer | task 1, the gate, after fn-1 ships |
| FNT-76 (Windows checks) | todo | needs the owner at a Windows machine |

Decisions of 2026-11-09 (in the specs): FNT-75 before FNT-71's UI tools; Settings in-process on tiny-skia, no GPU
library in the pet; the pet owns its state and Settings and the MCP server are equal front-ends on it; the Windows
gate moved to FNT-76; ai-pet is public, so CI minutes aren't limited.

## 1. Restore flowctl's task state

flowctl keeps each task's live status in `.git/flow-state/`, which git never pushes. `state/` here is a copy of it
taken on 2026-11-09 at the pause: every task's status across all the specs below, with its evidence.
In a fresh clone, on the work branch, copy it back before running flow-next.

In Git Bash, Linux or macOS:

```bash
mkdir -p .git/flow-state/tasks && cp .flow/paused/state/*.state.json .git/flow-state/tasks/
```

Check it: `flowctl show fn-1-migrate-aipet-from-net-to-rust.28` reads `done`, and `.23` reads `in_progress`. (flowctl is
bundled with the flow-next plugin: `~/.claude/plugins/cache/flow-next/flow-next/<version>/scripts/flowctl`. This
session used 9.9.0; fn-1's earlier tasks ran under 4.12.0 with the rolling-frontier work skill.)

## History

`handover-2.md` and `handover-14.md` are from the first pause (2026-09-28). Both tasks have since been done, so they're
kept only as history.
