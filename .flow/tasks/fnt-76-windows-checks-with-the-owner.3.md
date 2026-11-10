---
satisfies: [R4]
---
# fnt-76-windows-checks-with-the-owner.3 Where the pet's memory goes on Windows (from fn-1 task 29)

## Description
The Windows half of fn-1 task 29's memory audit, moved here (owner, 2026-11-10) to go with the other Windows checks. The Linux half is done: `rust/proofs/memory.md` (the breakdown by mapping, RSS and PSS, the cuts kept, each measured on its own, and the C# comparison).

**Size:** M
**Files:** `scripts/perf/memory-win.sh` (Git Bash, no PowerShell), the sampler from task 1 or a small helper beside it, `rust/proofs/memory.md`
**Touches:** [scripts/perf/**, rust/proofs/memory.md]

### Approach
- Measure the release Rust pet on the backend it ships with on Windows, calm and lively (fn-1 task 28's workloads), and the C# pet beside it: working set, private bytes, commit, and per module (GPU driver and graphics libraries, the binary, the heap).
- Check which of the Linux cuts in `rust/proofs/memory.md` hold on Windows; try a Windows-only one only where the breakdown points at it, one change at a time.
- Known so far (fn-1 task 13, 2026-09-29, Vulkan on Intel UHD): working set 178 MB and private bytes 140 MB with only the pet; 215 MB and 169 MB with Settings open.

### Investigation targets
**Required:**
- `rust/proofs/memory.md`, `scripts/perf/memory.sh` - the Linux breakdown and its script
- `rust/proofs/windows.md` - the Windows measurement so far
- task 1's sampler

### Key context
- Git Bash only; ask before installing anything; the owner starts or watches every GUI run.

## Acceptance
- [ ] `rust/proofs/memory.md` has the Windows breakdown, calm and lively, beside the C# pet and the Linux figures
- [ ] Each cut kept on Windows is measured on its own and keeps fn-1 R18's calm CPU budget, transparency and the sprite's goldens
- [ ] What can't be reduced on Windows is explained
- [ ] `scripts/perf/memory-win.sh` runs again on demand, without PowerShell

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
