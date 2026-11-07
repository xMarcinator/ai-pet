---
satisfies: [R1]
---
# fn-3-linux-desktops-beyond-hyprland-hands-on.1 KDE Plasma (Wayland): the Wayland shell's hands-on list

## Description
Run rust/SPIKE.md's hands-on list on KDE Plasma's Wayland session: poke, the overlay drag (the default) and the margin drag (AIPET_DRAG=margin), the menu and its edges, hover, bubbles, click-through between bubbles, Settings. Watch for KWin animating or squeezing the layer surface's moves and resizes, as Hyprland did (rust/proofs/linux-hands-on.md; rust/notes/exwlshell.md, known bug 20).

Needs a person at the screen. Build the pet with `cd rust && cargo build --release -p aipet` and run `AIPET_DEBUG=1 target/release/AiPet`. Record results in `rust/proofs/linux-hands-on.md` (Pass, Fail with what was seen, or Not run with why). Fix failures here, or make a new task under this spec for one too big.

## Acceptance
- [ ] Every check passes on KDE Plasma (Wayland), recorded with the Plasma and KWin versions
- [ ] Any KWin rule or setting the pet needs is shipped in packaging/linux/ and documented

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
