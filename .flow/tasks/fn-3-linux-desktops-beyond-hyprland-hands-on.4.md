---
satisfies: [R3]
---
# fn-3-linux-desktops-beyond-hyprland-hands-on.4 A real X11 session: the desktop shell

## Description
On GNOME on Xorg, KDE X11 or XFCE, run rust/SPIKE.md check 10 with AIPET_BACKEND=desktop: click-through beside the pet; xprop _NET_WM_STATE lists ABOVE, SKIP_TASKBAR and SKIP_PAGER; poke and a smooth drag with its squash; a lost release (a WM or screenshot grab takes it) ends the drag within about 0.1 s with 'drag cancelled' in the log; the inline menu (a click on a bubble outside it closes it; its padding keeps it open); no clipped glow, shadow or bubble edges, and no flicker from region updates. Measure the CPU calm and with Settings open.

Needs a person at the screen. Build the pet with `cd rust && cargo build --release -p aipet` and run `AIPET_DEBUG=1 target/release/AiPet`. Record results in `rust/proofs/linux-hands-on.md` (Pass, Fail with what was seen, or Not run with why). Fix failures here, or make a new task under this spec for one too big.

## Acceptance
- [ ] Every check of SPIKE.md's check 10 passes on one X11 desktop, recorded with its name and version
- [ ] CPU calm and with Settings open recorded

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:
