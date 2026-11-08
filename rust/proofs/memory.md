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

The CPU numbers in the last two rows were taken after Hyprland restarted in safe mode, when every pet ran at about
5–6 %; compare within a row only.

Not changed: Vulkan (`WGPU_BACKEND=vulkan`). It avoids LLVM but its loader maps every vendor's driver too, giving
RSS 207 MB at 7.2 % CPU, about twice GL's.

## After

A fresh pet: **RSS 164 MB, anonymous 15.6 MB** wearing the vector Hood; RSS 176 MB, anonymous 27.2 MB wearing the
painted Pip; RSS 167 MB, anonymous 18.5 MB wearing the pixel Sprout. After a burst of 3000 hook events, about 1 MB
more. That is about 55 % of the C# pet's RSS.

## What is left, and what it would take

- **Mesa's GL driver and LLVM (about 124 MB RSS, mostly shared with other GL programs).** Mesa's gallium driver links
  LLVM. Avoiding it means not using GL. Vulkan costs twice the CPU here; a software renderer costs more still.
- **The worn painted Pip (+11 MB against the vector Hood).** The UI keeps each of its frames as an image, plus
  a mirrored copy and halos. Making the mirrored copies only when the pet first faces left would save a few MB for
  painted and pixel pets.
- **The main heap (about 15 MB for a fresh pet).** iced, wgpu's GL backend and text shaping. Not broken down further:
  that needs a heap profiler (heaptrack isn't installed).
