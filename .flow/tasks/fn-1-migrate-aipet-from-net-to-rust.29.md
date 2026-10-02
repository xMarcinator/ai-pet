# fn-1-migrate-aipet-from-net-to-rust.29 Memory audit after the migration: where the Rust pet's memory goes, and how to cut it

## Description
After the migration, find out where the Rust pet's memory goes and cut what can be cut. Task 28 only checks a budget
against the C# pet; this task looks inside.

**Size:** M
**Files:** `scripts/perf/memory.sh`, `scripts/perf/memory-win.sh` (a breakdown script per OS, no PowerShell), `rust/proofs/memory.md` (results, new), and whatever code the improvements touch
**Touches:** [rust/crates/**, rust/Cargo.toml, rust/Cargo.lock, scripts/perf/**, rust/proofs/memory.md]

### What is known so far
- **Windows** (task 13's hands-on run, 2026-09-29, release spike, Vulkan on Intel UHD, scale 1): working set 178 MB
  and private bytes 140 MB with only the pet; 215 MB and 169 MB with Settings open.
- **Linux** (`rust/SPIKE.md`, Hyprland, GL): RSS about 274 MB, of which about 246 MB is shared GL/EGL libraries.
- Task 28 records the C# pet's numbers on the same machines; use them as the reference.

### Approach
- **Measure a breakdown, not just a total**, on Windows and Linux, calm and lively (task 28's workloads):
  - Linux: `/proc/<pid>/smaps_rollup` and `smaps`, grouped by mapping (GPU driver and GL/EGL/Vulkan libraries,
    fonts, the heap, the binary itself). Report Pss as well as RSS, since shared libraries inflate RSS.
  - Windows: working set, private bytes and commit, plus the per-module picture (no PowerShell and no tools that need
    installing; a small helper run with `dotnet` or a Rust example is fine).
- **Look at the likely costs first:**
  - the GPU backend and adapter: Vulkan against GL against DX12, the swapchain size (the full 380×600 window against a
    fitted one), and how many frames are buffered;
  - fonts and text: whether iced/cosmic-text loads the whole system font database when one or two fonts would do;
  - image atlases and caches: the sprite's bitmaps, the glow halo, the avatar previews in Settings;
  - Settings: what stays allocated after it closes;
  - the allocator and build settings (release profile, LTO, panic strategy, codegen units).
- **Change one thing at a time** and measure each change the same way. Keep a change only if it saves memory without
  costing CPU (task 28's calm budget still holds), transparency, or the pixel-exact sprite.

### Investigation targets
**Required:**
- `rust/proofs/performance.md` (task 28) — the workloads, the C# numbers and the harness to reuse
- `rust/proofs/windows.md` (task 13) — the Windows measurement and the transparency route
- `rust/SPIKE.md` — the Linux measurement and the GPU defaults
**Optional:**
- iced, iced_wgpu and cosmic-text sources under `~/.cargo/registry` — font loading and atlas sizes

## Acceptance
- [ ] `rust/proofs/memory.md` records a before-and-after breakdown on Windows and Linux (calm and lively), next to the
      C# pet's numbers from task 28, and says where the memory goes.
- [ ] Each improvement kept is measured on its own, with its saving, and none of them breaks task 28's CPU budget,
      transparency on the chosen backend, or the sprite's golden tests.
- [ ] What couldn't be reduced is explained (for example, memory the GPU driver maps), with what it would take.
- [ ] The breakdown scripts run again on demand and use no PowerShell.


## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
