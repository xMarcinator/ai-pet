---
satisfies: [R1, R2, R3]
---
# fnt-73-ui-framework-spike-gpui-or-something.1 Spike: gpui, a minimal stack and one toolkit against iced for the pet's windows

## Description
Build a throwaway prototype of the pet window per candidate and compare them with today's iced stack on R1's criteria, ending with R3's recommendation. Findings only.

**Size:** M
**Files:** `rust/proofs/ui-framework.md` (new); prototypes on a scratch branch `spike/ui-framework`, never merged
**Touches:** [rust/proofs/ui-framework.md]

### Approach
- Start from what the pet needs: `rust/crates/aipet-ui/src/{lib,view,sprite,vector_view,geometry,platform}.rs`, the shells in `rust/crates/aipet-desktop` and `rust/crates/aipet-wayland`, and the earlier iced spike's notes (`rust/SPIKE.md`, `rust/notes/{iced,exwlshell,native}.md`) for the hard parts already solved: click-through, layer-shell placement, dragging on Wayland, frame pacing.
- Reuse the existing drawing: `aipet_sprite::compose` and `vector` render a pack to pixels with tiny-skia, so each prototype only has to put those pixels in a window and draw one bubble's text.
- gpui: check its Linux backend (Wayland, X11), whether it can make a transparent layer-shell surface and an input region, and its Windows status; record the versions used.
- Minimal stack: a window crate plus a CPU buffer (for example winit with softbuffer) and smithay-client-toolkit's layer-shell on Wayland, with the existing native click-through code; text through a small shaper.
- Toolkit: Slint or egui, whichever can do a transparent click-through window.
- Measure each with `scripts/perf/pet-calm.sh` (60 s settle, 5 x 30 s, medians) and `scripts/perf/memory.sh` on Linux, beside the current release pet; record release binary sizes.
- Ask the owner before downloading anything beyond crates; Windows trials need the owner present (Git Bash only, never PowerShell).

### Investigation targets
**Required:**
- `rust/SPIKE.md` - what the iced spike had to solve
- `rust/notes/native.md`, `rust/notes/exwlshell.md`
- `rust/proofs/memory.md` - where the memory goes today
**Optional:**
- `rust/Cargo.toml` - the iced feature comments

### Key context
- The pet window must stay transparent and click-through on the art only; anything that can't is out early, with the reason.

## Acceptance
- [ ] `rust/proofs/ui-framework.md` has R1's table for iced and each candidate, with measured CPU, RSS and binary size on Linux
- [ ] Each candidate's prototype is described (branch, commit, what worked, what didn't)
- [ ] A recommendation with its cost in tasks and its effect on fn-1's budgets

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
