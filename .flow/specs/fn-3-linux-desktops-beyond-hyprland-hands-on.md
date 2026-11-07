# Linux desktops beyond Hyprland: hands-on parity for the Rust pet

## Goal & Context

Task fn-1-migrate-aipet-from-net-to-rust.22 was the Linux hands-on pass for the Rust pet. It passed on Hyprland 0.57.0
at scale 1 and was closed there (`rust/proofs/linux-hands-on.md`), because the developer's machine has no other
desktop installed and the user wanted to move on to the next plan. This spec carries the rest of that pass: the same
checks (`rust/SPIKE.md`'s hands-on list) on KDE Plasma, Sway, GNOME and an X11 session, plus the Hyprland checks that
need a second scale or an output that can be lost.

Everything here needs a person at the screen. Each task records its results in `rust/proofs/linux-hands-on.md`,
fixes what fails, or turns a failure too big for it into a new task under this spec.

Two decisions from task 22 are now the defaults and need checking everywhere: the Wayland shell drags by `overlay`
(`AIPET_DRAG=margin` picks the other), and the pet is hidden for about 45 ms at an overlay drop while its surface
shrinks back (`Drag::shows_pet`; `rust/notes/exwlshell.md`, known bug 20).

## Acceptance Criteria

- **R1:** On KDE Plasma (Wayland), Sway and Hyprland, the Wayland shell passes the hands-on list: poke, the overlay
  drag (smooth, lands without a jump, the grabbed spot stays under the pointer, the sprite stays on screen at every
  edge) and the margin drag, the menu and its edges, hover, bubbles, click-through between bubbles, and Settings.
  Errors: a compositor that animates or squeezes the surface's moves is handled by a rule the packaging ships, or by
  a fix in the shell.
- **R2:** On GNOME (Wayland), the pet starts on the desktop shell through XWayland without a panic, and passes the
  desktop-shell list there. With Settings on another workspace, the pet keeps its frame rate.
- **R3:** On a real X11 session (GNOME on Xorg, KDE X11 or XFCE), the desktop shell passes `SPIKE.md`'s check 10:
  click-through beside the pet; `_NET_WM_STATE` lists ABOVE, SKIP_TASKBAR and SKIP_PAGER; poke and drag; a lost
  release ends the drag within about 0.1 s; the inline menu; and no clipped glow, shadow or bubble edges, or flicker
  from the region updates.
- **R4:** On Hyprland at scales 1.25 and 1.5: placement, the input region, both drags and the menu; and losing the
  output mid-drag and with the pointer resting on the pet brings the pet back at home, neither held nor hovered.
- **R5:** Task fn-1-migrate-aipet-from-net-to-rust.15's checks 3 and 6 on Linux: the Rust and .NET pets are never
  both running, in each start order and at the same moment; and runs with `AIPET_BACKEND=wayland` and
  `AIPET_BACKEND=desktop` both work.

## Boundaries

- No Windows or macOS checks (fn-1 task 27 did Windows; macOS isn't in scope).
- No performance measurement: that is fn-1 task 28.

## Decision Context

- Split out of fn-1 task 22 at the user's request (2026-11-07), so fn-1 and the next plan could go on while the
  other desktops wait for a machine that has them. It isn't a dependency of fn-1's release task 23, but it should be
  done before the Rust pet is recommended on Linux desktops other than Hyprland.
