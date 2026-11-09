# Performance gate (fn-1 task 28, R18)

The Rust pet against the C# pet, both wearing Sprout, under the same scripted workload on the same machine.
**Linux: PASS** (one run so far; the task asks for two with the same verdict). **Windows: to do with the owner.**

## Method

`scripts/perf/pet-bench.sh [rust] [C#] [hook]`. Both pets run side by side, each in a sandbox data folder with its own
socket, wearing Sprout. Two workloads, one after the other:

- **calm:** no events. Both settle for 60 s, falling asleep.
- **lively:** `scripts/perf/workload.jsonl` replayed through the Rust `aipet-hook` into both pets on a 10 s loop, which
  keeps two Claude chats working and one needing attention. Codex chats are left out: one shows only once it has a
  transcript, which the hook alone doesn't give.

After the 60 s settle the script takes 5 paired 30 s samples, alternating Rust and C#, so drift hits both equally.
CPU is the process's `utime+stime` over a sample, as a percentage of one core, and memory is `VmRSS` and `Pss` after
the settle. Each pet's figure is the median of its 5 samples. The script refuses to measure while the 1-minute load
average is over 1.0. For this run, the owner's other Claude session paused its work.

Budgets (R18): calm CPU, Rust median ≤ C# median + 0.5 points; RSS, Rust ≤ 1.5 × C#; lively CPU recorded only.

## Linux, 2026-11-09

Arch Linux (kernel 7.2.14), Hyprland 0.57.0, on AC power. The Rust pet draws on the Intel UHD 770 through GL
(`WGPU_POWER_PREF=low`, see [memory.md](memory.md)). The Rust pet is a release build of `fn-2-layered-sprite-and-vector-pets` at
`e5ef0ef`; the C# pet is the installed 0.1.0 release (`~/.local/share/AiPet/app`).

| Workload | Pet | CPU median (% of a core) | Samples | RSS | PSS |
|---|---|---|---|---|---|
| calm | Rust | **4.70** | 5.70, 4.97, 4.70, 4.27, 4.30 | 157 MB | 50 MB |
| calm | C# | 12.23 | 12.26, 12.23, 12.23, 12.13, 12.03 | 283 MB | 167 MB |
| lively | Rust | **10.30** | 9.03, 10.60, 12.00, 10.26, 10.30 | 160 MB | 52 MB |
| lively | C# | 20.59 | 20.59, 23.32, 21.86, 20.39, 20.56 | 301 MB | 183 MB |

- **Calm CPU:** 4.70 % against a budget of 12.73 % (C# + 0.5). **Pass**, at 38 % of the C# pet's CPU.
- **RSS:** 157 MB against a budget of 425 MB (calm) and 160 MB against 451 MB (lively). **Pass**, at 55 % of the C#'s.
- **Lively CPU:** 10.30 % against the C#'s 20.59 %.

## Still to do

- A second Linux run, to show two runs give the same verdict.
- Windows: `pet-bench.ps1` (the same method, through the dotnet sampler task 13 used, since PowerShell is off
  limits and there is no wmic) and a run with the owner present.
- Task 27's frame drops while a bubble stack spreads (E-x9), which this gate doesn't measure.
