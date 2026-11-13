# Windows checks with the owner

## Goal & Context

fn-1's performance gate (task 28) and fn-2's verification (task 8) are done on Linux; only their Windows halves are left, and those need the owner at a Windows machine. They are moved here (owner, 2026-11-09) so fn-1 can ship and fn-2 can close without waiting for that session. This spec carries them unchanged: fn-1 R18's budgets on Windows, every built-in pet's calm CPU and memory on Windows, and the Windows hands-on pass. The Windows half of fn-1 task 29's memory audit (where the pet's memory goes) joined them on the owner's word, 2026-11-10, for the same Windows session; its Linux half is in `rust/proofs/memory.md`. FNT-71's Windows checks (the MCP server's hands-on pass and its R13 and R15 budgets) joined on the owner's word, 2026-11-13, so FNT-71 could close on its Linux half; that half is in `rust/proofs/mcp-server.md`.

## Architecture & Data Models

- One bench script for Windows, run from Git Bash (PowerShell is off limits): the same method as `scripts/perf/pet-bench.sh` (sandbox data folders, a 60 s settle, 5 paired 30 s samples alternating the two pets, medians, a load gate), reading process CPU time and the working set through a small `dotnet` sampler, since Windows 11 has no `wmic`.
- The per-pet runs reuse that script with a pet parameter, as `scripts/perf/pets-calm.sh` does on Linux.
- Results go into the existing proofs: `rust/proofs/performance.md` and `rust/proofs/sprite-packs.md`; the hands-on pass into `rust/proofs/windows-hands-on.md`'s checklist shape.

## Edge Cases & Constraints

- The Rust pet is measured on the backend it ships with on Windows (GL), not Vulkan.
- Windows has no load average: the script waits until the machine's total CPU over 30 s is under 10 % of one core, and refuses to measure otherwise; the figure used is recorded.
- Every Windows GUI step needs the owner present; downloads (Rust, the .NET SDK) are asked for first.

## Acceptance Criteria

- **R1:** A Windows bench script run from Git Bash compares the Rust pet with the C# pet by fn-1 task 28's method and prints both pets' medians and the verdict; two runs on the same machine give the same verdict, and fn-1 R18's budgets pass on Windows, recorded in `rust/proofs/performance.md` with the machine and conditions. Errors: a pet that exits during a run fails the run with its log path; a busy machine is refused, as on Linux; a budget miss is recorded and fixed before the task closes.
- **R2:** Every built-in pet, style and theme meets fn-1 R18's calm-CPU and RSS budgets on Windows, recorded in `rust/proofs/sprite-packs.md` beside the Linux figures. Errors: a pet over budget is recorded and goes back to its own spec as a fix.
- **R3:** The Windows hands-on pass is recorded with the owner: every pet's look and the newer moods, eye-follow, props, headphones, effects, mirroring while dragged, click-through on the art, crisp pixels, reduced motion, the Settings page (Style and Theme pickers), and fn-2's leftovers (the closed laptop lid, no fragments round the bubbles, the window coming back when a monitor goes). Errors: each failed item is recorded with what was seen, and fixed or filed.

- **R4:** Where the Rust pet's memory goes on Windows, calm and lively, beside the C# pet: working set, private bytes and commit, and the per-module picture, from a breakdown script run from Git Bash (no PowerShell; a small helper such as task 1's sampler). Recorded in `rust/proofs/memory.md` next to the Linux breakdown, with whichever of the Linux cuts help on Windows and what can't be reduced there (memory the GPU driver maps). Errors: a cut that costs CPU beyond fn-1 R18's calm budget, transparency or the sprite's goldens is recorded and not kept.
- **R5:** FNT-71's Windows half: AiPet's MCP listener and its registrations on Windows pass the hands-on checklist of `rust/proofs/mcp-server.md` with the owner, and meet FNT-71 R13 (fn-1 R18's calm-CPU and RSS budgets with the listener on and an agent connected) and R15 (at most 5 MB of resident memory over the same build with the listener off, measured paired, and back within it 60 s after a hook body at the cap). Errors: a miss blocks FNT-71's next release; the first remedies are FNT-71 R15's.

## Boundaries

- No new measurements beyond fn-1 task 28's, fn-2 task 8's and fn-1 task 29's Windows breakdown and FNT-71's R13 and R15; the bubble stack's dropped frames (task 27's E-x9) move to FNT-75, whose new bubble drawing replaces the code that drops them.
- No Windows work for FNT-75's layered window: that spec's own task does it.

## Decision Context

The owner asked for the Windows halves to move out so the rest of fn-1 and fn-2 can continue. Shipping then happens before the Windows numbers are in; the Linux runs passed by a wide margin (4.7–4.9 % calm CPU against the C# pet's 12.1–12.2 %), and the Windows run comes in this spec.

## Early proof point

Task fnt-76-windows-checks-with-the-owner.1 validates the core approach (the bench runs on Windows from Git Bash and gives a verdict). If it fails, re-evaluate the sampler before continuing with fnt-76-windows-checks-with-the-owner.2.

## Quick commands

```bash
scripts/perf/pet-bench-windows.sh
```

## Requirement coverage

| Req | Description | Task(s) | Gap justification |
| --- | --- | --- | --- |
| R1 | A Windows bench script run from Git Bash compares the Rust pet with the C# pet by fn-1 task 28's method and prints both pets' medians and the verdict; two runs on the same machine give the same verdict, and fn-1 R18's budgets pass on Windows, recorded in `rust/proofs/performance.md` with the machine and conditions. Errors: a pet that exits during a run fails the run with its log path; a busy machine is refused, as on Linux; a budget miss is recorded and fixed before the task closes. | fnt-76-windows-checks-with-the-owner.1 | — |
| R2 | Every built-in pet, style and theme meets fn-1 R18's calm-CPU and RSS budgets on Windows, recorded in `rust/proofs/sprite-packs.md` beside the Linux figures. Errors: a pet over budget is recorded and goes back to its own spec as a fix. | fnt-76-windows-checks-with-the-owner.2 | — |
| R3 | The Windows hands-on pass is recorded with the owner: every pet's look and the newer moods, eye-follow, props, headphones, effects, mirroring while dragged, click-through on the art, crisp pixels, reduced motion, the Settings page (Style and Theme pickers), and fn-2's leftovers (the closed laptop lid, no fragments round the bubbles, the window coming back when a monitor goes). Errors: each failed item is recorded with what was seen, and fixed or filed. | fnt-76-windows-checks-with-the-owner.2 | — |
| R4 | Where the Rust pet's memory goes on Windows, calm and lively, beside the C# pet: working set, private bytes and commit, and the per-module picture, from a breakdown script run from Git Bash (no PowerShell; a small helper such as task 1's sampler). Recorded in `rust/proofs/memory.md` next to the Linux breakdown, with whichever of the Linux cuts help on Windows and what can't be reduced there (memory the GPU driver maps). Errors: a cut that costs CPU beyond fn-1 R18's calm budget, transparency or the sprite's goldens is recorded and not kept. | fnt-76-windows-checks-with-the-owner.3 | — |
| R5 | FNT-71's Windows half: AiPet's MCP listener and its registrations on Windows pass the hands-on checklist of `rust/proofs/mcp-server.md` with the owner, and meet FNT-71 R13 (fn-1 R18's calm-CPU and RSS budgets with the listener on and an agent connected) and R15 (at most 5 MB of resident memory over the same build with the listener off, measured paired, and back within it 60 s after a hook body at the cap). Errors: a miss blocks FNT-71's next release; the first remedies are FNT-71 R15's. | fnt-76-windows-checks-with-the-owner.4 | — |

