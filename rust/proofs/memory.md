# Memory audit (fn-1 task 29)

Where the Rust pet's memory goes and what was cut, on Linux. **Windows is still to do with the owner** (the same
breakdown through a small helper, no PowerShell).

Machine: Arch Linux, Hyprland, a laptop with an Intel UHD 770 (which drives the screen) and an NVIDIA RTX A2000. The
pet draws on the Intel GPU through GL (`WGPU_POWER_PREF=low`). Release builds, 2026-11-08. Each number comes from
a sandbox pet (its own data folder and socket, beside the owner's) measured after 30–40 s, with
`scripts/perf/memory.sh <pid>`: resident memory grouped by mapping, RSS and PSS, the anonymous memory in detail and
the threads. RSS counts a shared library in full; PSS divides it among the processes that map it.

## Before

The owner's pet after about two hours of hook traffic, wearing the vector Hood: **RSS 300 MB, PSS 166 MB**.

| What | RSS | PSS |
|---|---|---|
| GPU drivers and GL/Vulkan libraries | 222 MB | 97 MB |
| Anonymous: heap, arenas, buffers | 52 MB | 52 MB |
| The AiPet binary | 15 MB | 15 MB |
| Other shared libraries | 10 MB | 1 MB |

A fresh pet had about 25 MB of anonymous memory (19 MB main heap, 6 MB in small blocks: thread stacks and arenas)
and 35 threads, 24 of them tokio workers. The other 27 MB had come from hook traffic.

The C# pet on the same machine, for reference: RSS 299 MB, PSS 204 MB, calm CPU 7.07 % of a core.

## What was cut

Each change was measured on its own, against the build without it, in alternating runs.

| Change | Saving | Calm CPU |
|---|---|---|
| Load only Mesa's EGL driver when an Intel or AMD GPU draws (glvnd loaded NVIDIA's too) | RSS 271 → 171 MB; NVIDIA's 95 MB no longer mapped | 3.5 → 3.7 % (noise) |
| Cap glibc's malloc arenas at 2 | growth over a burst of 3000 hook events 5.2 → 2.1 MB | the same (3.0 %) |
| One tokio worker instead of one per core | threads 35 → 20; RSS 0.8 MB, anonymous 1.7 MB | 5.2, 5.6 % against 5.5, 6.2 % |
| Keep the other pets' sheets deflated, decoding on wear | anonymous 7.1 MB, whichever pet is worn | no per-frame work |

The first two rows were measured in the owner's normal session. The last two were measured after Hyprland crashed
(11:38) and restarted in safe mode, in alternating runs of the two builds; safe mode changes the compositor, not what
the pet allocates (the heap read the same 15.6 MB again in the normal session after a reboot). Compare CPU within a
row only.

Not changed: Vulkan (`WGPU_BACKEND=vulkan`). It avoids LLVM but its loader maps every vendor's driver too, giving
RSS 207 MB at 7.2 % CPU, about twice GL's.

## After

A fresh pet (safe mode): **anonymous 15.6 MB** wearing the vector Hood (RSS 164 MB), 27.2 MB wearing the painted Pip
(RSS 176 MB), 18.5 MB wearing the pixel Sprout (RSS 167 MB). After a burst of 3000 hook events, about 1 MB more.

Again after a reboot, in the normal session, alternating with the C# pet (two rounds, 40 s settle, 3 × 20 s):

| Pet | Calm CPU | RSS | PSS |
|---|---|---|---|
| C# Sprout | 13.6, 12.5 % | 283, 300 MB | 231, 247 MB |
| Rust Sprout | 5.8, 5.1 % | 170 MB | 121 MB |
| Rust vector Hood | 6.3, 6.0 % | 167 MB | 118 MB |
| Rust painted Pip (one run, same hour) | 10.8 % | 127 MB | 88 MB |

After the reboot every pet's calm CPU was about twice the morning's (the C# pet 7.07 % then), so calm CPU is only
comparable within one run, never across a reboot or a compositor restart. Within this one the Rust pets cost under
half the C# pet's CPU and about 57 % of its RSS. RSS also moves with what is already resident: the same fresh pet read
124 MB minutes after boot, since shared driver pages count only once they're touched.

## The MCP listener (fnt-71 task 8, R13 and R15)

Linux, 2026-11-13, the release build of `fnt-71-mcp-server` wearing the pixel Sprout, normal Hyprland session.

**R15, paired** (`scripts/perf/mcp-memory.sh 3`): two pets of the same build side by side, one with the listener on
and an MCP client connected and idle, one with its port held so the listener can't start, read at the same moment
after 60 s. Then the "on" pet got a Claude hook body just under the 4 MiB cap, and the "off" pet the same event through
the old exe and its pipe, so both showed the same state. On minus off:

| | Idle | 10 s after the big body | 60 s after |
|---|---|---|---|
| RSS | +3.3, +3.8, +3.2 MB | +3.5, +3.4, +3.1 MB | +3.5, +3.4, +3.1 MB |
| Anonymous | +0.6, +0.8, +0.5 MB | +0.7, +0.5, +0.5 MB | +0.7, +0.5, +0.5 MB |

Within R15's 5 MB. Before the fix in `aipet-mcp/src/guard.rs` (hand freed memory back with glibc's `malloc_trim`
after a request of 256 kB or more), the big body left **+5.0 MB anonymous, +8.0 MB RSS** for the whole 60 s: glibc
kept the freed body in the listener thread's arena. The rest of the RSS difference is the listener's code and
tokio's pages, touched once.

**R13** (`scripts/perf/pet-calm.sh`, two rounds alternating; `CONNECT=1` keeps an MCP client connected, `LISTENER=off`
holds the port):

| Pet | Calm CPU | RSS | PSS |
|---|---|---|---|
| C# Sprout | 12.26, 12.30 % | 242, 243 MB | 180, 181 MB |
| Rust Sprout, listener on, client connected | 4.93, 5.03 % | 131, 131 MB | 81, 81 MB |
| Rust Sprout, listener off | 5.03, 4.73 % | 127, 127 MB | 77, 77 MB |

Within fn-1 R18's budgets (calm CPU at most the C# pet's + 0.5 points, RSS at most 1.5x its): the listener's CPU is
within the run's noise. **Windows is still to do with the owner.**

## What is left, and what it would take

- **Mesa's GL driver and LLVM (about 124 MB RSS, mostly shared with other GL programs).** Mesa's gallium driver links
  LLVM. Avoiding it means not using GL. Vulkan costs twice the CPU here; a software renderer costs more still.
- **The worn painted Pip (+11 MB against the vector Hood).** The UI keeps each of its frames as an image, plus
  a mirrored copy and halos. Making the mirrored copies only when the pet first faces left would save a few MB for
  painted and pixel pets.
- **The main heap (about 15 MB for a fresh pet).** iced, wgpu's GL backend and text shaping. Not broken down further:
  that needs a heap profiler (heaptrack isn't installed).
