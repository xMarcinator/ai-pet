# Linux hands-on pass: the Rust pet with real data

The record for task fn-1-migrate-aipet-from-net-to-rust.22: the hands-on checks with the real-data pet on the Linux
desktops it supports (`rust/SPIKE.md`'s hands-on list), what failed and how it was fixed, and the two decisions the
task makes. Each desktop gets a Result: **Pass**, **Fail** (with what was seen), or **Not run** (with why).

## Where it stands

| Desktop | Shell | Result |
|---|---|---|
| Hyprland 0.57.0 (Arch, the developer's machine), scale 1 | Wayland (layer-shell) | **Pass**: approved by the user on 2026-11-07, after the drag fixes below |
| Hyprland at scales 1.25 and 1.5 | Wayland | **Not run**: the machine's only connected output is at scale 1 |
| KDE Plasma (Wayland) | Wayland | **Not run**: not installed on the machine |
| Sway | Wayland | **Not run**: not installed |
| GNOME (Wayland, the desktop-shell fallback through XWayland) | desktop | **Not run**: not installed |
| An X11 session (GNOME on Xorg, KDE X11 or XFCE) | desktop | **Not run**: no X11 session installed |

So the task can't close yet: its acceptance asks for every check to pass on each listed desktop.

## Hyprland 0.57.0

The pet was a release build of the work branch (`cargo build --release -p aipet`), started with `AIPET_DEBUG=1` on
the Wayland shell on eDP-1 (1920 × 1200, scale 1). The user tried it and approved it ("the pet looks great").

What the user reported, in order, and what changed:

1. **The drag overshot.** Hyprland's `layers` animation slides every change of the layer surface's place, so each move
   the pet made glided past the pointer and back. **Fix:** the packaged rules now have a layer rule for the pet's
   namespace, `hl.layer_rule({ name = "aipet-pet", match = { namespace = "^aipet$" }, no_anim = true })`. Tried live
   with `hyprctl eval` first (a layer rule applies when the surface opens, so the pet was restarted), then appended to
   the user's installed `~/.local/share/AiPet/hyprland/aipet.lua`. The user confirmed: fixed.
2. **With the animation off, the margin drag jumped.** The log showed a move every ~32 ms of up to ~100 px on a fast
   drag: the margin strategy waits two frames for each move to land, so it moves every other frame, which the
   animation used to blur. **Fix:** the overlay strategy (the surface covers the output during the drag and the pet is
   drawn under the pointer every frame). The user: "smooth".
3. **At the overlay drop, a small stretched ghost** of the pet showed up and to the left for a frame. At the release
   the surface is still anchored to all four edges, and its first change, the four margins, already makes its box
   380 × 600; Hyprland 0.57 squeezes the last output-sized frame into that box until the first frame at the new size
   comes (0.56 cropped it, which the overlay strategy relied on). Holding the input-region commit back didn't help.
   **Fix:** the pet isn't drawn from the release until the surface is its home size (`Drag::shows_pet`), a blink of
   about 45 ms. Offered the choice (patch iced_exwlshell, try a smoother margin drag, keep the ghost, keep the blink),
   the user kept the blink. `rust/notes/exwlshell.md`, known bug 20, says what to check on each iced_exwlshell upgrade
   to remove it. Hyprland 0.57.1 (2026-11-03) has no fix for this: its one renderer change (#16711) is about surfaces
   without a viewport at a scale above 1.

The other items of `SPIKE.md`'s list (poke, the menu and its edges, hover, bubbles, click-through between bubbles,
Settings) were part of the user's look at the pet and the approval, not reported item by item. Not checked on this
machine: scales 1.25 and 1.5, losing the output mid-drag, and task 15's checks 3 and 6 (never two pets in each start
order, and runs with `AIPET_BACKEND=wayland` and `AIPET_BACKEND=desktop`).

## The decisions

- **The default shell on Hyprland:** layer-shell, as planned (the spec's parked unknown is marked resolved).
- **The default drag strategy:** `overlay` (`AIPET_DRAG=margin` still picks the other). Tried only on Hyprland, so
  the other compositors' checks must cover it.

## The Hyprland rule files

`packaging/linux/hyprland/aipet.lua` (0.55 and later) and `aipet.conf` (0.53 and 0.54, with the 0.52 lines in
comments) now hold:

- the layer rule for the pet's `aipet` namespace, `no_anim` (layers aren't blurred unless a rule asks, so it needs
  nothing else);
- the Settings window rule, matched by its title `AiPet · Settings` alone (no border, rounding, blur or shadow).

The .NET pet's window rule (class and title `AiPet`) is gone, so these files are for the Rust pet. The installers'
include line is unchanged, and the file is still loaded through `pcall(dofile, …)`. The user's installed copy keeps
the .NET pet's rules too, with the layer rule appended, while the .NET pet is installed there.
