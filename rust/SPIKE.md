# The iced front-end spike

A trial rewrite of the pet's front end in Rust with iced 0.14, on branch `spike/iced`. On Wayland compositors with
layer-shell (Hyprland, Sway, KDE, COSMIC) it uses [iced_exwlshell](https://github.com/waycrate/exwlshelleventloop)
0.20.1; everywhere else it uses plain iced (winit) with native code for the clickable area. The goal is to find out
whether this stack can do what Avalonia can't on Hyprland: exact click-through, placement and always-on-top without
window rules. docs/HANDOFF.md, step 3, has the background. Its research advised staying on .NET, but we chose to try
iced anyway.

Only the pet window exists, driven by a demo script. There is no hook server, no watchers and no real chats yet.

**Status (2026-09-27):** paused mid-way. Both shells are built, run and are verified as far as a machine without
synthetic clicks allows. A review found 26 issues, none of them fixed yet (see "Open review findings"). Next come the
hands-on tests below, then the fixes.

## Running it

```bash
cd rust
cargo run -p aipet-spike
```

To build it you need Rust 1.89 or newer, a C linker, pkg-config and the libxkbcommon development files (Arch:
`pkgconf libxkbcommon`; Debian/Ubuntu: `pkg-config libxkbcommon-dev`; Fedora: `pkgconf-pkg-config libxkbcommon-devel`).
The first build downloads about 300 crates. To run it on Linux you need Wayland, EGL/GL (Mesa) and, for the desktop
shell, X11 or XWayland. The spike starts about 420 px left of the bottom-right corner, so it doesn't
cover the real pet. It uses the layer namespace / X11 class `aipet-spike` and never touches the real pet's socket or
config.

| Variable | Effect |
|---|---|
| `AIPET_BACKEND=wayland\|desktop` | Picks the shell. By default: Wayland when the session is Wayland and the compositor has layer-shell, desktop otherwise (through XWayland in a Wayland session). |
| `AIPET_DRAG=margin\|overlay` | Wayland only: how a drag moves the pet (see "Dragging on Wayland"). The default is `margin`. |
| `AIPET_OPEN_SETTINGS=1` | Opens Settings at start (both shells). |
| `AIPET_DEBUG=1` | Logs the chosen shell, surfaces, presses, drags and input-region updates to stderr. |
| `WGPU_BACKEND`, `WGPU_POWER_PREF` | Default to `gl` (on Linux) and `low` (see "GPU"). Set them to override. |

Checks: `cargo test --workspace` (66 tests), `cargo clippy --workspace --all-targets` and `cargo fmt --check`, all
clean. The golden data is regenerated from the C# with `dotnet run --project rust/golden -c Release`, from the repo
root (needs .NET 10).

## Layout

| Path | What it is |
|---|---|
| `crates/aipet-sprite` | The port of `Pet.cs` and `Avatar.cs`: poses, pixels and motion. It is pixel- and bit-exact against 202 golden cases (3,914 frames) that `golden/` renders from the C#. Of the deliberate bugs planted to test those tests, only one survived, and it can't change any result (using UTF-16 length instead of char length in `TryParseColor`). |
| `crates/aipet-ui` | Shared by both shells: the pet (sprite images, CPU-blurred glow halo, shadow), the demo bubbles with their animations, the menu, the Settings view, the layout, and `hit_rects()` (what takes clicks, from the same geometry the view uses). |
| `crates/aipet-wayland` | The layer-shell shell: a registry probe (`available()`), the pet layer surface with an exact input region, dragging (a pure state machine in `drag.rs`, 10 tests), the right-click menu as an xdg popup (inline fallback), and Settings as an xdg toplevel. |
| `crates/aipet-desktop` | The winit shell: a transparent, undecorated, always-on-top window, and `native/` for the clickable area (X11 input shape via x11rb, Win32 `SetWindowRgn` plus a `WS_EX_TOOLWINDOW` subclass, macOS `setIgnoresMouseEvents` polling). Dragging uses the global pointer. The menu is drawn inline, and Settings is a normal window. |
| `crates/aipet-spike` | The binary: chooses the shell and prints a clear line when one fails. |
| `golden/` | The C# program that writes `crates/aipet-sprite/tests/golden/frames.json`. |
| `notes/` | Research notes with file:line citations into the crate sources: `exwlshell.md`, `iced.md`, `native.md`. Also compile-checked prototypes (`notes/prototypes/*`), each built on its own: `cd notes/prototypes/native-spike && cargo check`. Their `target/` and `Cargo.lock` are ignored, and the notes' `--offline` commands need one online run first. The cited crate sources are under `~/.cargo/registry/src/index.crates.io-*/` after `cargo fetch` in `rust/` (a build alone skips a few, such as iced_tiny_skia and softbuffer). The `EX/` and `REPO/` paths in `exwlshell.md` need `git clone --branch v0.20.1 https://github.com/waycrate/exwlshelleventloop`. |

## What's verified

On Hyprland 0.56.2, on a laptop with an Intel UHD 770 plus an NVIDIA RTX A2000 (eDP-1, 1920×1200, scale 1):

- **Wayland shell.**
  - The layer surface is placed as intended (`hyprctl layers`: 380×600, bottom-right minus 420 px).
  - The input region is exact. Pointer warps onto transparent points of the surface logged no enter; onto the sprite
    they did, and the hand cursor showed.
  - It renders correctly: crisp ×5 pixels, glow halo, ground shadow, the bubbles (thinking, working, needs-you in
    amber, done, a folded Codex card, the Jira header) and real transparency.
  - Hover makes it wave. Real clicks from the user were logged as pokes.
  - The Settings window opens with `AIPET_OPEN_SETTINGS=1` and renders fully.
  - The popup menu appears and draws. It was opened from code, in a scratch copy of the shell that opened the menu
    at start, since no click could be synthesised. The scratch copy isn't kept, so repeat it with the right-click in
    the hands-on tests.
- **Desktop shell under XWayland.**
  - It runs, with a transparent 32-bit window.
  - The X11 input shape reads back as the exact rectangles. The EWMH hints are set before mapping, and Hyprland's
    XWayland WM then clears them.
  - Settings opens.
  - One real drag by the user moved the window end to end.
  - Windows and macOS are only clippy-checked (`RUSTC_BOOTSTRAP=1 cargo clippy -Zbuild-std=std,panic_abort --target
    x86_64-pc-windows-msvc`, and the same for `aarch64-apple-darwin`; this needs `rustup component add rust-src`, or
    use `rustup target add` and a plain `cargo clippy --target ...`). They have never run.
- **Cost.**
  - Release builds use about 10–11 % of one core at 60 fps (Wayland) and about 9 % (desktop), 16 % with Settings
    open. Debug builds use 35–42 %.
  - RSS is about 274 MB, of which 246 MB is shared GL/EGL libraries. The first frame shows 0.1 s after start.
  - Our own per-frame work is 0.25 ms. The rest is iced and GL rendering.
- **GPU.**
  - With the default settings wgpu picked NVIDIA's Vulkan. Its Wayland surfaces led iced to an fp16 format, which
    washed out every colour.
  - Even when rendering on the Intel GPU, listing the Vulkan adapters opened NVIDIA's driver. That kept the dGPU
    awake: 50 s active during a 45 s run, against 0 when nothing ran.
  - With `WGPU_BACKEND=gl` and `WGPU_POWER_PREF=low` (now the defaults in `main.rs`), colours are correct, no NVIDIA
    handles are opened, and the dGPU stays suspended.

## Hands-on tests still to do (need real clicks)

Run from `rust/` with `AIPET_BACKEND=wayland AIPET_DEBUG=1 cargo run -p aipet-spike`:

1. **Poke:** left-click the sprite without moving. It should bounce. The log shows `drag: idle -> pressed`, then
   `-> idle`, with no `surface:` line.
2. **Margin drag** (the default): drag the pet around and release.
   - It should follow, with a squash on landing.
   - Stop, then wiggle 1–2 px: the grabbed spot should end up under the pointer, with no drift.
   - Drag past each screen edge: the sprite must stay fully on screen.
   - Expect it to glide behind the pointer; see "Dragging on Wayland".
3. **Overlay drag:** `AIPET_DRAG=overlay`. The log should read growing over the output, then shifting to the
   output's corner, then over the output, returning home, settling, idle. Report any visible jump or slide at the
   start and at the drop.
4. **Menu:** right-click the sprite. The menu should appear centred above it. Then check:
   - "Show bubbles" toggles the bubbles.
   - A click elsewhere closes the menu.
   - "Always on top" moves the pet behind windows (Bottom layer) and back.
   - "Settings…" and "Avatars…" open Settings. Asking again reopens it in front.
   - Quit exits.
   - Near the top of the screen the menu should flip below the sprite.
   - Right-click within about 0.4 s of a drop: the menu must still be placed right.
5. **Click-through:** clicks on the transparent parts, including the gaps between bubbles, must reach the window
   below.
6. **Bubbles:** hovering shows the close button, which dismisses the bubble. A click on a folded stack spreads it
   out.
7. **Desktop shell on a real X11 session** (GNOME on Xorg, KDE X11, XFCE):
   `AIPET_BACKEND=desktop AIPET_DEBUG=1 cargo run -p aipet-spike`. Check:
   - Clicks beside the pet reach the window below.
   - `xprop -id $(xdotool search --name '^AiPet spike$') _NET_WM_STATE` lists ABOVE, SKIP_TASKBAR and SKIP_PAGER.
   - Dragging feels smooth.
   - The inline menu works.
8. **Windows and macOS:** whether the window is transparent (the wgpu swapchain alpha), click-through, staying out
   of Alt+Tab and the taskbar, topmost, and dragging with DPI scaling.
9. **Fractional scaling and other compositors:** only scale 1 on Hyprland was tried. At 1.25 and 1.5, and on Sway or
   KDE, check the placement, the input region, both drag modes and the menu (a Low finding expects the overlay drag
   to misbehave at some fractional scales).

## Dragging on Wayland

A layer surface only knows pointer positions relative to itself, and margin changes are never acknowledged. So a drag
moves the surface by changing its margins, and has to know when a change has landed:

- **`margin`**: one margin change is in flight at a time. Motion is ignored until two frames drawn after the change
  come round, which proves it applied; then the pointer delta is added. It is exact (it was measured that Hyprland
  reports pointer positions for the new geometry straight away), but it moves at most every other frame.
- **`overlay`**: for the duration of the drag, the surface grows to cover the whole output, so pointer positions are
  output-relative. The pet is drawn under the pointer, then the surface shrinks back at the drop.

Hyprland's `layers` animation (in the user's config: speed 3.81, easeOutQuint) animates every change of a layer
surface's position. So the margin drag glides behind the pointer, and the overlay drag slides at its start and end.
The likely fix is a layer rule with `no_anim` for namespace `aipet-spike`; for the real app it would go in
`packaging/linux/hyprland/`. Untested, and the syntax is unconfirmed:
`hyprctl eval 'hl.layer_rule({ name = "aipet-spike", match = { namespace = "^aipet-spike$" }, no_anim = true })'`.

## Open review findings (not fixed yet)

Three reviewers looked at the Wayland shell, the desktop shell with its native code, and the UI's fidelity to the C#
together with performance. Nothing below is fixed yet; each needs re-checking against the code before it's fixed.

**High**
- `aipet-spike/src/main.rs`: the desktop shell crashes on GNOME Wayland, and wherever `$XDG_RUNTIME_DIR/wayland-0`
  exists. `main` removes `WAYLAND_DISPLAY` and forces GL, but libwayland falls back to `wayland-0`, so wgpu's EGL
  picks the Wayland platform and panics on the Xlib window.

**Medium**
- Both shells: the pet never goes idle. Each redraw schedules the next, so it renders at the display rate in every
  state, even when nothing changed. (The C# renders at the display rate too; decide on idle frames or a frame cap.)
- `aipet-ui/src/view.rs`: the inline menu is only a layer drawn over the bubbles. Presses on its padding or
  separator reach the bubble below, and a bubble's dismiss badge can show beside the menu.
- `aipet-wayland/src/shell.rs`:
  - If the compositor closes the pet's surface before its first frame, the surface stays `Opening` forever, with no
    retry.
  - If one popup fails to appear within 500 ms, the menu stays inline for the rest of the session, and there it
    closes only by pressing the pet or choosing an item.

**Low**
- `aipet-spike`:
  - When wgpu is unavailable, the Wayland shell panics (inside iced_exwlshell's `.expect`) instead of printing the
    clear failure line.
  - Only `WAYLAND_DISPLAY` is checked, but winit also takes `WAYLAND_SOCKET`.
  - The doc comment says `AIPET_OPEN_SETTINGS` is for the desktop shell only, but both shells read it.
- `aipet-desktop`:
  - A drag ends only on a left-button release. If that release is lost, the pet follows the pointer until the next
    click.
  - It doesn't compile on the BSDs: the `sys` module and x11rb are gated to Linux, Windows and macOS.
  - The start position uses the whole monitor, not the work area, so the pet can start under or over a bottom panel.
  - A click on a bubble while the inline menu is open acts on the bubble and leaves the menu open.
  - Hit rects are padded 18 px for the X11 bounding shape and the Windows region. On Windows, clicks within 18 px of
    the pet are swallowed (the C# pads by 6). An extent of what is drawn in aipet-ui would fix it.
- `aipet-wayland`:
  - If the surface is lost mid-drag, the drag resets without telling `PetUi`, so the pet stays "held".
  - Pressing the pet doesn't close an open popup menu (the grab is owner-events).
  - The popup can't stay on the output at the left and right edges, and its vertical flip puts the menu over the
    sprite, not below it.
  - The pet-events subscription changes identity at each press and drag end. iced drops events the old stream
    hasn't delivered yet, and a lost release leaves the drag stuck in `Pressed`.
  - The input region is sent on every frame where it changes, up to about 32 per second, each with an extra commit.
    The C# and the desktop shell send at most every 100 ms.
  - At scales where ceil(380·s)/s isn't 380, the overlay drag never recognises the home size, and after each drop it
    swallows presses for a second.
  - The margin drag's `CONFIRM` counts frames from the decision to move, not from the commit.
- Both shells: every message rebuilds every open window's view, so an open Settings window (or popup) is rebuilt and
  laid out 60 times a second, and redrawn on each pointer move over the pet.
- `aipet-ui`:
  - A dismissed bubble stays hidden for the rest of the 40 s demo loop, even when its chat does something new. The
    C# `Board.Dismiss` brings it back then.
  - There is no ghost bubble when every bubble is dismissed: the check runs before dismissed bubbles are removed.
  - Folded bubbles behind the front one take clicks and show the hand cursor. In the C# they aren't hit-testable.
  - About 75 allocations per frame. Cleanup only, not a hotspot.
  - Nothing sets `alert_until`, so the 6 s "attention" alert for a new review never plays in the demo.

## Other known limits

- **Settings on Wayland:**
  - No app_id (Hyprland shows class `''`) and it is tiled, so rules must match the title "AiPet Settings".
  - It can't be raised (no xdg-activation), so it is closed and reopened instead.
  - It is destroyed on close, with no close request to intercept.
- **Popups on Hyprland** are misplaced sideways while the layer is still animating if sliding is allowed. The menu
  uses flips only, so near a side edge it can overhang by up to 50 px.
- **The desktop shell on Hyprland** is only a fallback. XWayland there ignores input shapes and drops
  `_NET_WM_STATE`. The installed Hyprland rules match class `AiPet`, not `aipet-spike`, so it gets a border and blur.
- **The renderer:** only wgpu is built in, because iced's tiny-skia renderer can't draw a transparent window
  (softbuffer has no alpha). So without a working wgpu adapter there is no software fallback, and the pet doesn't
  start (see the Low finding about the Wayland shell's panic).
- **Upstream:** iced is pre-1.0 and breaks its API each release. iced_exwlshell has one main maintainer, thin docs,
  and calls its popups "a toy".

## Verdict so far

It looks viable. Everything the Avalonia version couldn't do on Hyprland works:
- exact click-through;
- visible on every workspace for free (a layer surface);
- no border, blur or shadow from the compositor, so no window rules needed.

The pixel-exact sprite port also shows the pet's animation carries over unchanged. Still undecided: how dragging feels
with Hyprland's layer animation, and the CPU cost of rendering continuously at 60 fps (about 10 % of a core).

## Next steps

1. The hands-on tests above, starting with both drag modes and the menu, then the layer `no_anim` rule.
2. Fix the review findings: the high one first, then the medium ones.
3. Decide on idle rendering or a frame cap (measure it against the C# pet).
4. Then decide between three routes:
   - a full rewrite, porting Board, HookServer and the watchers behind the same UI;
   - a Rust front end that talks to the C# core over the socket (HANDOFF step 3's fallback; it needs a
     subscribe/stream message in HookServer);
   - stopping.

## Continuing on another computer

```bash
git fetch
git switch spike/iced
cd rust && cargo build -p aipet-spike
```

Everything needed is in the repo: this file, `notes/` (research), and the code's doc comments. Before committing,
set the repository's identity (docs/HANDOFF.md, "Commits use this identity").

Watch out for three things on a new machine:
- **GNOME Wayland crashes the spike.** A default run takes the desktop shell there (GNOME has no layer-shell) and
  crashes: this is the High review finding. It also applies to any session with `$XDG_RUNTIME_DIR/wayland-0`. Fix
  that finding first, or test on a layer-shell compositor.
- **The GPU defaults** were tuned on a hybrid Intel+NVIDIA laptop. Elsewhere, check the colours and which GPU renders
  (compare with `WGPU_BACKEND=vulkan`).
- **The Hyprland findings** (layer animation, popup placement) were measured on 0.56.2 with the user's own config.
