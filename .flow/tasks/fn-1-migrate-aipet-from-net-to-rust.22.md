---
satisfies: [R13]
---
# fn-1-migrate-aipet-from-net-to-rust.22 Linux hands-on parity pass and the Hyprland layer rules

## Description
Run the hands-on tests with the real-data pet on the supported Linux desktops, fix what fails, and ship Hyprland rules
for a layer surface. **It needs the user** (real clicks). Windows is task 27 and the performance gate task 28. This task
can't be completed while a Linux check fails. It runs side by side with the Windows pass (task 27), on files of its
own. The shared `aipet-desktop/src/shell.rs` is in neither pass's Touches: a fix there is reported, and the conductor
adds the file to this task's Touches while task 27 isn't running.

**Size:** M
**Files:** `packaging/linux/hyprland/aipet.lua`, `packaging/linux/hyprland/aipet.conf`, `rust/crates/aipet-wayland/src/{drag,shell}.rs`, `rust/crates/aipet-desktop/src/{native/x11,platform/linux}.rs` (X11 and platform fixes), `rust/proofs/linux-hands-on.md` (results, new)
**Touches:** [packaging/linux/hyprland/**, rust/crates/aipet-wayland/src/**, rust/crates/aipet-desktop/src/native/x11.rs, rust/crates/aipet-desktop/src/platform/linux.rs, rust/proofs/linux-hands-on.md]

### Approach
- Go through `rust/SPIKE.md`'s hands-on list on Hyprland, KDE, Sway, GNOME (the winit fallback) and an X11 session:
  poke, both drag strategies, the menu, click-through in the gaps between bubbles, the Settings pages, and scales 1,
  1.25 and 1.5.
- Pick the default drag strategy.
- Hyprland: a layer rule for the pet's namespace (`no_anim`, and no blur if needed), and the Settings window rule
  matched by its title. The pet's old class rules go.
- Resolve the "default shell on Hyprland" parked unknown with the user.
- Fix failures here. A failure too big for this task becomes a new task under this spec. That new task must be added
  as a dependency of this task and of task 23, so milestone 2 can't ship without it.

### Investigation targets
**Required:**
- `rust/SPIKE.md` — hands-on tests and known limits
- `packaging/linux/hyprland/aipet.lua`, `packaging/linux/hyprland/aipet.conf`
**Optional:**
- `rust/notes/exwlshell.md` — the popup and layer quirks on Hyprland
**From task 15 (2026-10-01):** the layer-shell namespace is now `aipet` (was `aipet-spike`), so the spike's Hyprland rule
no longer matches; Settings is `AiPet · Settings` at 800 × 640. Task 15's manual checks 3 and 6 are this pass's: the
two pets never together on Linux in each start order and at the same moment (once task 6's .NET lock is in), and
`AIPET_BACKEND=wayland` and `AIPET_BACKEND=desktop` runs.
## Acceptance
- [ ] Every check passes on each listed desktop, recorded in `rust/proofs/linux-hands-on.md`. No failure is left open.
- [ ] The Hyprland rule files match the layer namespace and the Settings title. The installer's include line still
      works.
- [ ] The default drag strategy and the Hyprland default shell are decided and recorded in the spec.
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
