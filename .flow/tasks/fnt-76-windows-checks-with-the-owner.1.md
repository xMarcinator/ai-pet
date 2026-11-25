---
satisfies: [R1]
---
# fnt-76-windows-checks-with-the-owner.1 The Windows performance gate: bench script and two runs

## Description
The Windows half of fn-1 task 28: a Git Bash bench script with a dotnet sampler, then two runs with the owner.

**Tooling ready (2026-11-14, tested on Linux only):** `scripts/perf/pet-bench-windows.sh` and the sampler `scripts/perf/sampler/` (.NET 8, no packages: `proc <pid>…` for CPU ms, working set and private bytes, `system` for the machine-CPU gate, `hold-port`). The script also runs on Linux, where a smoke run with two Rust pets passed end to end. Left for the owner's session: the .NET 8 SDK on the Windows machine (ask first), the Velopack path of the installed C# pet (`%LOCALAPPDATA%\AiPet\current\AiPet.exe` assumed), `/proc/<pid>/winpid` and `kill` on native exes in Git Bash, then the two runs.

**Windows session (2026-11-25):** `/proc/<pid>/winpid` and `kill` work on native exes; the .NET 8 SDK was already installed (8.0.428); the sampler builds without an apphost. The installed C# pet is `%LOCALAPPDATA%\AiPetApp\current` (0.1.0's AiPet.exe under an rc.1 hook), copied to `~/AiPet-0.1.0` so Velopack's update check stays off. Both pets take the mutex `Local\AiPetApp` whatever `AIPET_PIPE` says, so on Windows each pet runs alone (Rust, then C#) instead of side by side; the workload is read with CRLF stripped. A short smoke run (SETTLE=10 SAMPLES=2 WINDOW=5, load gate off) passed end to end with the owner watching. Left: the two full runs on a quiet machine (Postman, the build and the antivirus kept it at 4-9 busy cores). Four full runs that day (gate 10 %, then the owner's 300 %) all refused at the gate: with Postman closed the machine still sat at 2.1-8.7 busy cores (Trend Micro, svchost, vcm2, System Restore). Left: the two full runs in a quieter window.

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
