---
satisfies: [R1]
---
# fn-3-linux-desktops-beyond-hyprland-hands-on.2 Sway: the Wayland shell's hands-on list

## Description
Run rust/SPIKE.md's hands-on list on Sway: poke, the overlay and margin drags, the menu and its edges, hover, bubbles, click-through between bubbles, Settings. Sway can also run nested in a Wayland session for a first look, but the pass is on a real Sway session.

Needs a person at the screen. Build the pet with `cd rust && cargo build --release -p aipet` and run `AIPET_DEBUG=1 target/release/AiPet`. Record results in `rust/proofs/linux-hands-on.md` (Pass, Fail with what was seen, or Not run with why). Fix failures here, or make a new task under this spec for one too big.

## Acceptance
- [ ] Every check passes on Sway, recorded with Sway's version
- [ ] Any Sway config the pet needs is shipped in packaging/linux/ and documented

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
