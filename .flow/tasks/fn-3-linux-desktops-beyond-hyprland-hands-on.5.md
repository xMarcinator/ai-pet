---
satisfies: [R4, R5]
---
# fn-3-linux-desktops-beyond-hyprland-hands-on.5 Hyprland at 1.25 and 1.5, a lost output, and never two pets

## Description
On Hyprland: at scales 1.25 and 1.5, check placement, the input region, both drags and the menu. Disable or unplug the output mid-drag, and once with the pointer resting on the pet: it must come back at home, neither held nor hovered. Then fn-1 task 15's checks 3 and 6: the Rust and .NET pets are never both running, in each start order and when started at the same moment; and AIPET_BACKEND=wayland and AIPET_BACKEND=desktop runs both work (the desktop one through XWayland, which the old .NET window rule no longer covers: note what Hyprland does to it).

Needs a person at the screen. Build the pet with `cd rust && cargo build --release -p aipet` and run `AIPET_DEBUG=1 target/release/AiPet`. Record results in `rust/proofs/linux-hands-on.md` (Pass, Fail with what was seen, or Not run with why). Fix failures here, or make a new task under this spec for one too big.

## Acceptance
- [ ] Scales 1.25 and 1.5 pass on Hyprland
- [ ] A lost output, mid-drag and at rest, brings the pet back at home
- [ ] Never two pets, in each start order and at the same moment
- [ ] AIPET_BACKEND=wayland and AIPET_BACKEND=desktop runs recorded

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
