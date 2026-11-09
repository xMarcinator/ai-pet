---
satisfies: [R7]
---
# fnt-75-the-pet-window-without-the-gpu.9 Spike: Settings in the pet process on iced's CPU renderer

## Description
Settles whether Settings can stay in the pet process without loading a GPU library: iced's tiny-skia renderer for the Settings window, iced owning the main thread with no window open, and the pet's own window on a second thread. If all four checks below hold, task 4 becomes "Settings in-process on iced's CPU renderer" instead of a child process (the owner's question, 2026-11-09). Throwaway code on the spike branch; only the proof lands.

**Size:** M
**Files:** `spikes/ui-framework/**` (spike branch), `rust/proofs/ui-framework.md`
**Touches:** [spikes/ui-framework/**, rust/proofs/ui-framework.md]

### Approach
1. Real Settings on the CPU: build today's app with iced's `tiny-skia` feature and run it with `ICED_BACKEND=tiny-skia` and `AIPET_OPEN_SETTINGS=1`; go through every page (General, Avatars, Jira, GitHub, About): fonts, icons, scrolling, text input, pickers.
2. No GPU library: with Settings open and after it closes, `grep -E 'libGL|libEGL|libvulkan|nvidia|libgallium|dri' /proc/<pid>/maps` is empty.
3. Threads: a prototype with an iced daemon (tiny-skia, no window) on the main thread and the minimal prototype's sctk layer surface on a second thread, exchanging messages over channels; open and close a Settings-like window from the pet's menu while the pet animates.
4. Idle cost: the prototype's calm CPU and RSS with no iced window open, against the minimal prototype alone, by `scripts/perf/pet-calm.sh`.

### Investigation targets
**Required:**
- `rust/Cargo.toml:36-56` - iced features
- `rust/crates/aipet-ui/src/settings/mod.rs` - the pages
- spike branch `spikes/ui-framework/minimal/src/main.rs` - the sctk prototype

### Key context
- softbuffer (under iced's tiny-skia renderer) can't present transparency on X11 or Windows; Settings is opaque, so it doesn't need to. Windows is checked later, with the owner.

## Acceptance
- [ ] Each of the four checks recorded with its result in `rust/proofs/ui-framework.md`
- [ ] A verdict: Settings in-process on the CPU renderer, or the child process, and what task 4 becomes

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
