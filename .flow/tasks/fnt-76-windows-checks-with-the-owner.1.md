---
satisfies: [R1]
---
# fnt-76-windows-checks-with-the-owner.1 The Windows performance gate: bench script and two runs

## Description
The Windows half of fn-1 task 28: a Git Bash bench script with a dotnet sampler, then two runs with the owner.

**Size:** M
**Files:** `scripts/perf/pet-bench-windows.sh`, `scripts/perf/sampler/` (a small dotnet project), `rust/proofs/performance.md`
**Touches:** [scripts/perf/**, rust/proofs/performance.md]

### Approach
- Mirror `scripts/perf/pet-bench.sh` step for step: sandbox data folders (`LOCALAPPDATA`/`APPDATA` per pet as the Windows pet reads them), the pipe name per pet, `workload.jsonl` replayed through `aipet-hook.exe`.
- The sampler prints a process's `TotalProcessorTime` and working set; the script computes the percentages and medians as the Linux script does.
- The release Rust pet on GL; the installed C# 0.1.0.

### Investigation targets
**Required:**
- `scripts/perf/pet-bench.sh`
- `rust/proofs/windows.md`, `rust/proofs/windows-hands-on.md` - how the pet was run on Windows before

### Key context
- Git Bash only; ask before installing anything; the owner starts or watches every GUI run.

## Acceptance
- [ ] The script runs end to end on Windows from Git Bash and prints both pets' medians and the verdict
- [ ] Two runs give the same verdict; R18 passes on Windows, recorded in `rust/proofs/performance.md`

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
