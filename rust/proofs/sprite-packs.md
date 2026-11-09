# Built-in pets: CPU, memory and size (fn-2 task 8)

Each built-in pet measured against the C# pet, and the binary-size budget. **Linux: done, every pet passes.**
**Windows, and the hands-on pass with the owner: to do.**

## Method

`scripts/perf/pets-calm.sh`, which runs `scripts/perf/pet-calm.sh` once per pet, each pet alone. The C# pet (wearing
Sprout) runs first and again last, so drift shows. Each pet starts in a sandbox data folder with its own socket,
settles for 60 s (falling asleep), then takes 5 samples of 30 s; the figure is the median. CPU is `utime+stime` as a
percentage of one core, and memory is `VmRSS` and `Pss` after the settle. The script refuses to measure while the
1-minute load is over 1.0. This is the same method as fn-1's gate ([performance.md](performance.md)), and the same
machine and conditions.

## Linux, 2026-11-09

The Rust pet is a release build at `e5ef0ef`, with packs as of FNT-72 task 2 (the new states drawn for the Hood,
Sprout and Pip).

| Pet | Calm CPU median (%) | Samples | RSS | PSS |
|---|---|---|---|---|
| C# Sprout (first) | 14.20 | 14.03, 14.33, 14.20, 14.36, 14.16 | 280 MB | 166 MB |
| Sprout | 5.93 | 5.80, 5.93, 5.93, 5.87, 5.93 | 157 MB | 52 MB |
| Hood | 5.93 | 5.93, 6.03, 5.93, 5.93, 5.96 | 157 MB | 52 MB |
| Hood (green theme) | 5.90 | 5.93, 5.90, 5.77, 5.93, 5.83 | 157 MB | 52 MB |
| Hood (vector) | 6.26 | 6.13, 6.23, 6.33, 6.33, 6.26 | 154 MB | 49 MB |
| Pip | 5.26 | 5.26, 5.23, 5.20, 5.26, 5.30 | 154 MB | 49 MB |
| Pip (painted) | 9.30 | 9.03, 9.10, 9.40, 9.30, 9.53 | 165 MB | 60 MB |
| Hamster | 6.83 | 7.03, 6.70, 6.83, 6.76, 6.83 | 154 MB | 49 MB |
| Hamster (pixel) | 4.73 | 4.77, 4.73, 4.77, 4.70, 4.70 | 155 MB | 50 MB |
| Sprout (vector) | 6.36 | 6.30, 6.36, 6.46, 6.26, 6.40 | 154 MB | 49 MB |
| C# Sprout (last) | 12.33 | 12.20, 12.33, 12.30, 12.33, 12.43 | 280 MB | 166 MB |

- **Calm CPU:** every pet passes. The strictest budget is the C# pet's lower figure plus 0.5 points, 12.83 %, and the
  busiest pet, painted Pip, is at 9.30 %. Painted Pip costs the most because its frames are large sheets drawn
  smoothly; the vector pets are at 5.3–6.8 %.
- **RSS:** every pet passes, at 154–165 MB against a budget of 420 MB (1.5 × 280 MB).
- **Binary size:** the embedded art is 427 KB (the `packs/` folders, measured with `du -sb` before FNT-72's new
  states), far under the 3 MB budget.

## Still to do

- Windows: the same measurements with the owner present.
- The Windows hands-on pass with the owner. It covers every pet's look, eye-follow, props, headphones, effects,
  click-through on the art, crisp pixels, reduced motion and the Settings page. It also checks the hands-on items
  left by fn-2 tasks 20 (a closed laptop lid), 21 (no fragments around the bubbles), 22 and 23 (mirroring while dragged), and
  FNT-74 (the Style picker).
