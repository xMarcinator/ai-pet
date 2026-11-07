---
satisfies: [R2]
---
# fn-3-linux-desktops-beyond-hyprland-hands-on.3 GNOME (Wayland): the desktop shell through XWayland

## Description
GNOME has no layer-shell, so plain AiPet takes the desktop shell through XWayland (rust/SPIKE.md check 11). Check it starts without a panic, then the desktop-shell list: click-through beside the pet, poke, drag, the inline menu, bubbles, Settings. Put Settings on another workspace and check the pet keeps its frame rate (it once dropped to about 1 fps).

Needs a person at the screen. Build the pet with `cd rust && cargo build --release -p aipet` and run `AIPET_DEBUG=1 target/release/AiPet`. Record results in `rust/proofs/linux-hands-on.md` (Pass, Fail with what was seen, or Not run with why). Fix failures here, or make a new task under this spec for one too big.

## Acceptance
- [ ] The pet starts on GNOME Wayland's XWayland without a panic, and the desktop-shell checks pass, recorded with GNOME's version
- [ ] With Settings on another workspace the pet keeps its frame rate

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
