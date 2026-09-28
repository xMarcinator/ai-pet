---
satisfies: [R18]
---
# fn-1-migrate-aipet-from-net-to-rust.28 Performance gate: scripted, repeatable CPU and RSS comparison with the C# pet

## Description
A repeatable benchmark that runs the Rust pet and the C# pet under the same scripted workload on the same machine. It
applies R18's budgets on Linux and Windows. The comparison was split out of the hands-on pass so the milestone 2 gate
gives the same verdict on every run.

**Size:** M
**Files:** `scripts/perf/pet-bench.sh`, `scripts/perf/pet-bench.ps1`, `scripts/perf/workload.jsonl` (a fixed hook-event script), `rust/proofs/performance.md` (results, new)
**Touches:** [scripts/perf/**, rust/proofs/performance.md, rust/crates/aipet-ui/src/**, rust/crates/aipet-wayland/src/**, rust/crates/aipet-desktop/src/**]

### Approach
- Workloads, the same for both pets. Both speak the same protocol, so events are replayed through `aipet-hook`.
  - **Calm:** no events. The pet has been asleep for 60 s.
  - **Lively:** a fixed script of Claude and Codex events, replayed at recorded offsets. It keeps two chats working
    and one needing attention. Review bubbles come from the Jira and GitHub watchers, not from hooks, so they aren't
    part of the workload; both pets run with Jira and GitHub unconfigured.
- Procedure per workload:
  1. Start the pet in a sandbox data folder, and let it settle for 60 s.
  2. Take 5 paired samples of 30 s each, alternating Rust and C# (A B A B …), so drift hits both equally.
  3. CPU is process CPU time over the window, as a percentage of one core: `utime+stime` from `/proc/<pid>/stat` on
     Linux, `TotalProcessorTime` on Windows.
  4. Memory is resident memory after the settle: `VmRSS`, and `Pss` for context, on Linux; the working set on Windows.
- Aggregate as the median of the 5 samples.
- Budgets:
  - calm CPU: Rust median ≤ C# median + 0.5 percentage points (noise tolerance);
  - RSS: Rust ≤ 1.5 × C#;
  - lively CPU: recorded, with no budget.
- Conditions: AC power, no other user load, the same compositor and scale, and the GPU backend recorded. The script
  refuses to run if the 1-minute load average exceeds 1.0.
- If a budget fails, fix it (frame pacing, redraw scope) and re-run.

### Investigation targets
**Required:**
- `rust/SPIKE.md` — the frame pacing and the earlier measurements
- `rust/crates/aipet-ui/src/lib.rs` (`frame_interval`)
**Optional:**
- `rust/notes/iced.md` §7 — redraw behaviour
## Acceptance
- [ ] `pet-bench.sh` and `pet-bench.ps1` run end to end and print both pets' medians and the verdict.
- [ ] R18's budgets pass on Linux and Windows. The numbers and conditions are recorded in `rust/proofs/performance.md`.
- [ ] Two runs on the same machine give the same verdict.
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
