# The migration, paused on 2026-09-28

Work on the flow-next spec `fn-1-migrate-aipet-from-net-to-rust` (migrating AiPet from .NET to Rust) stopped here for
the day, to continue on another machine. The work branch is `fn-1-migrate-aipet-from-net-to-rust`.

| Task | State | Where |
|---|---|---|
| 1 Workspace foundations | done | `03b1404` (code), `b01f8a3` (receipt), on the work branch |
| 2 Rust hook event path | in progress, paused | branch `wave/fn-1.2` at `3d914e6` (a `wip:` commit); see [handover-2.md](handover-2.md) |
| 14 Velopack proof | in progress, paused | branch `wave/fn-1.14` at `e0bdff0`; see [handover-14.md](handover-14.md) |
| 7 Claude chat ordering | reset to todo | stopped before writing any code; start it from scratch |
| all others | todo | |

**Task 14's proof passed**: an installed .NET 0.1.x updates in place to a Rust-packed release, by delta and by full
package. Run: https://github.com/xMarcinator/ai-pet/actions/runs/36477504598 (branch `proof/velopack`). Left: write
`rust/proofs/velopack.md` from the results in the handover, run the checks, write the evidence.

**Task 2's early proof point holds**: all 32 C# hook tests pass against the Rust hook talking to the .NET pet. Left:
gate one test import to Linux (a macOS lint), write `.github/workflows/cross-runtime.yml`, run the checks. The
handover has the details.

## 1. Restore flowctl's task state

flowctl keeps each task's live status in `.git/flow-state/`, which git never pushes. `state/` here is a copy of it at
the pause. In a fresh clone, on the work branch, copy it back before running flow-next:

```bash
mkdir -p .git/flow-state/tasks && cp .flow/paused/state/*.state.json .git/flow-state/tasks/
```

In PowerShell:

```powershell
New-Item -ItemType Directory -Force .git\flow-state\tasks | Out-Null; Copy-Item .flow\paused\state\*.state.json .git\flow-state\tasks\
```

Check it: `flowctl show fn-1-migrate-aipet-from-net-to-rust.1` reads `done`, and tasks 2 and 14 read `in_progress`.

## 2. Resume tasks 2 and 14

They ran as a parallel wave, so each lives on its own branch, and neither has had `flowctl done` or its review:

1. Recreate their worktrees: `git worktree add ../ai-pet-waves/task-2 wave/fn-1.2`, and the same for task 14.
2. Continue each from its handover with a flow-next worker (`PARALLEL_WAVE: true`, the same worktree), or finish the
   steps it lists by hand.
3. Then join the wave: bring both branches' commits onto the work branch, run each task's Codex review from its
   integrated head, verify the combined tree, and run `flowctl done` for each.

Next come tasks 7 and 10 (and 13, see below), then 3, 8, 9 and 11.

## 3. On the Windows machine

- **Needed:** Git for Windows, Rust through rustup (the MSVC toolchain, 1.89 or newer, and Visual Studio Build Tools
  with the C++ workload), the .NET 10 SDK, the GitHub CLI signed in, the Codex CLI (for reviews), and Claude Code
  with the flow-next plugin.
- **Task 13**, the Windows proof, needs a person at a Windows 10 or 11 machine. This is its chance.
- **Task 2's Windows side** (the named pipe, the trace log's path) has only been compiled so far. Running its tests on
  Windows is its first real check.
- **In a fresh clone**, run `dotnet build AiPet.slnx` before `dotnet test AiPet.slnx --no-build`. Five Resource and
  Velopack tests need the UI to have been built.
- The handovers name paths from the Linux machine (`/home/marcinator/...`). Read them as the new clone's paths.

## Left behind on the Linux machine

- Local `spike/iced` has one commit that was never pushed (`74fa11d`), from an interrupted command.
  `git branch -f spike/iced b658465` undoes it.
- The worktrees `../ai-pet-waves/task-2` and `../ai-pet-waves/task-14`.
