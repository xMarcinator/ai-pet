# The iced front-end spike

A trial rewrite of the pet's front end in Rust with iced 0.14, on branch `spike/iced`. On Wayland compositors with
layer-shell (Hyprland, Sway, KDE, COSMIC) it uses [iced_exwlshell](https://github.com/waycrate/exwlshelleventloop)
0.20.1; everywhere else it uses plain iced (winit) with native code for the clickable area. The goal is to find out
whether this stack can do what Avalonia can't on Hyprland: exact click-through, placement and always-on-top without
window rules. docs/HANDOFF.md, step 3, has the background. Its research advised staying on .NET, but we chose to try
iced anyway.

Only the pet window exists, driven by a demo script. There is no hook server, no watchers and no real chats yet.

**Status (2026-09-28):** paused again, after a round of fixes (not committed yet). Of the first review's 26 findings,
23 are fixed, one is fixed for the popup menu but not for its inline fallback, and two are still open. A second review
of the fixes found six more issues: three are fixed, one was this file being out of date, and two are measured
trade-offs left open. See "Review findings". Next come the hands-on tests below, which need real clicks.

## Running it

```bash
cd rust
cargo run -p aipet-spike
```

To build it you need Rust 1.98 or newer, a C linker, pkg-config and the libxkbcommon development files (Arch:
`pkgconf libxkbcommon`; Debian/Ubuntu: `pkg-config libxkbcommon-dev`; Fedora: `pkgconf-pkg-config libxkbcommon-devel`).
The first build downloads about 300 crates. To run it on Linux you need Wayland, EGL/GL (Mesa) and, for the desktop
shell, X11 or XWayland. The spike starts about 420 px left of the bottom-right corner, so it doesn't
cover the real pet. It uses the layer namespace / X11 class `aipet-spike` and never touches the real pet's socket or
config.

| Variable | Effect |
|---|---|
| `AIPET_BACKEND=wayland\|desktop` | Picks the shell. By default: Wayland when `WAYLAND_DISPLAY` is set and not empty and the compositor has layer-shell, desktop otherwise (through XWayland in a Wayland session). A session with only `WAYLAND_SOCKET` gets the desktop shell unless you ask for `wayland`. |
| `AIPET_DRAG=margin\|overlay` | Wayland only: how a drag moves the pet (see "Dragging on Wayland"). The default is `margin`. |
| `AIPET_OPEN_SETTINGS=1` | Opens Settings at start (both shells). |
| `AIPET_FPS=<n>` | Fixes the frame rate at n per second (1 to 1000) instead of the pacing below. Any other value prints one line and is ignored. `AIPET_FPS=60` gives the old always-60 behaviour. |
| `AIPET_DEBUG=1` | Logs the chosen shell, surfaces, presses, drags and input-region updates to stderr. |
| `WGPU_BACKEND`, `WGPU_POWER_PREF` | Default to `gl` (on Linux) and `low` (see "GPU"). Set them to override. |

**Frame pacing.** The pet ticks and draws on a timer, not at the display's rate. It runs at 60 Hz while the pointer is
on the pet or its bubbles, during a drag, and for 1 s after anything moved faster than 30 px/s or a bubble was still
easing (a hop, a poke, a landing, attention's hops, bubbles coming, going, spreading or folding). Otherwise it runs at
30 Hz: asleep, idle, thinking and working. 30 Hz is still above the sprite's own 24 fps, and at 30 px/s every step is
1 px. Only the pet's surface is drawn on a tick. The busy 40 s demo script spends about half its time at each rate.

The desktop shell needs `WAYLAND_DISPLAY` set to an empty value and no `WAYLAND_SOCKET`, so that winit and libwayland
both stay on X11. `main` does that itself before choosing the desktop shell, so nothing changes for you.

Checks: `cargo test --workspace` (93 tests), `cargo clippy --workspace --all-targets` and `cargo fmt --check`, all
clean. The golden data is regenerated from the C# with `dotnet run --project rust/golden -c Release`, from the repo
root (needs .NET 10).

## Layout

| Path | What it is |
|---|---|
| `crates/aipet-sprite` | The port of `Pet.cs` and `Avatar.cs`: poses, pixels and motion. It is pixel- and bit-exact against 202 golden cases (3,914 frames) that `golden/` renders from the C#. Of the deliberate bugs planted to test those tests, only one survived, and it can't change any result (using UTF-16 length instead of char length in `TryParseColor`). |
| `crates/aipet-ui` | Shared by both shells: the pet (sprite images, CPU-blurred glow halo, shadow), the demo bubbles with their animations and dismissal, the menu, the Settings view, the layout, `hit_rects()` (what takes clicks), `drawn_rects()` (what is drawn on, shadows included) and `frame_interval()` (the frame pacing). `SPRITE` and `INLINE_MENU` say where the sprite and the menu are. |
| `crates/aipet-wayland` | The layer-shell shell: a registry probe (`available()`), the pet layer surface with an exact input region, dragging (a pure state machine in `drag.rs`, 14 tests), the right-click menu as an xdg popup (inline fallback), and Settings as an xdg toplevel. |
| `crates/aipet-desktop` | The winit shell: a transparent, undecorated, always-on-top window, and `native/` for the clickable area (X11 input and bounding shapes via x11rb, Win32 `SetWindowRgn` plus a `WS_EX_TOOLWINDOW` subclass, macOS `setIgnoresMouseEvents` polling). Dragging reads the global pointer and its button, so a lost release is noticed. The menu is drawn inline, and Settings is a normal window. |
| `crates/aipet-spike` | The binary: chooses the shell, sets up the environment for it, and prints a clear line when one fails (also when the Wayland shell panics). |
| `golden/` | The C# program that writes `crates/aipet-sprite/tests/golden/frames.json`. |
| `notes/` | Research notes with file:line citations into the crate sources: `exwlshell.md`, `iced.md`, `native.md`. Also compile-checked prototypes (`notes/prototypes/*`), each built on its own: `cd notes/prototypes/native-spike && cargo check`. Their `target/` and `Cargo.lock` are ignored, and the notes' `--offline` commands need one online run first. The cited crate sources are under `~/.cargo/registry/src/index.crates.io-*/` after `cargo fetch` in `rust/` (a build alone skips a few, such as iced_tiny_skia and softbuffer). The `EX/` and `REPO/` paths in `exwlshell.md` need `git clone --branch v0.20.1 https://github.com/waycrate/exwlshelleventloop`. |

## What's verified

On Hyprland 0.56.2, on a laptop with an Intel UHD 770 plus an NVIDIA RTX A2000 (eDP-1, 1920×1200, scale 1). Without
synthetic clicks, the tools were pointer warps, screenshots of the spike's region (`grim -g`) and `WAYLAND_DEBUG=1`
traces. Hyprland 0.56 with a Lua config rejects `hyprctl dispatch movecursor X Y`; the working form is
`hyprctl dispatch 'hl.dsp.cursor.move({ x = X, y = Y })'`.

- **Wayland shell.**
  - The layer surface is placed as intended (`hyprctl layers`: 380×600, bottom-right minus 420 px, level Top). It is
    up 0.06 s after start.
  - The input region is exact. Pointer warps onto transparent points of the surface logged no enter; onto the sprite
    they did, and the hand cursor showed.
  - It renders correctly: crisp ×5 pixels, glow halo, ground shadow, the bubbles (thinking, working, needs-you in
    amber, done, a folded Codex card, the reviews header), the new review's 6 s attention alert, and real
    transparency.
  - Hover makes it wave (in idle, sleep and done; in the other moods the eyes follow the pointer). Real clicks from
    the user were logged as pokes, before the fixes.
  - The Settings window opens with `AIPET_OPEN_SETTINGS=1` and renders fully.
  - The popup menu appears and draws, centred above the sprite. With the pet parked at the output's top-left, it
    flipped below the sprite and stayed on the output. Both were opened from code in a scratch copy, since no click
    could be synthesised; repeat them with the right-click in the hands-on tests.
  - Motion at 30 Hz, measured from bursts of screenshots: steps of 0–1.6 px, no coarse jumps.
- **Desktop shell under XWayland.**
  - It runs, with a transparent 32-bit window. XWayland here reports scale 1.5, so this pet is 1.5× the Wayland one.
  - The X11 shapes read back right: the bounding shape is the banded union of `drawn_rects()`, and every input rect
    lies inside it. The EWMH hints are set before mapping, and Hyprland's XWayland WM then clears them.
  - The GNOME Wayland crash is fixed. It was reproduced with a private runtime dir whose `wayland-0` links to the
    real socket: the old build panicked in wgpu (exit 101), the new one renders through XWayland. The same holds with
    `WAYLAND_DISPLAY` unset, as in an X11 session with a stray `wayland-0`.
  - Settings opens. One real drag by the user moved the window end to end, before the fixes.
  - Windows, macOS and FreeBSD are only clippy-checked (`RUSTC_BOOTSTRAP=1 cargo clippy -Zbuild-std=std,panic_abort
    --target x86_64-pc-windows-msvc`, and the same for `aarch64-apple-darwin` and `x86_64-unknown-freebsd`; this
    needs `rustup component add rust-src`, or use `rustup target add` and a plain `cargo clippy --target ...`). They
    have never run.
- **Cost** (release, share of one core, pointer away unless said).

  | | Before the fixes (always 60 Hz) | Now |
  |---|---|---|
  | Wayland, calm (thinking, working) | 11.0 % | 7.9–8.0 % |
  | Wayland, asleep | 9.1 % | 5.4–6.0 % |
  | Wayland, lively (attention, 60 Hz) | 12.6 % | 13.4–13.6 % |
  | Wayland, pointer resting on the pet (60 Hz) | – | 11.7 % |
  | Wayland, two whole demo passes | 10.9 % | 9.5 % |
  | Wayland, Settings open (10 s) | 12.0 % | 8.5 % |
  | Desktop, calm | 10.1–10.7 % | 7.6–7.7 % |
  | Desktop, lively | 12.1 % | 12.4 % |
  | Desktop, Settings shown | 16.1 % | 11.6 % |

  - Each pair was measured back to back or side by side; single seconds vary by up to ±3 points with load and demo
    content.
  - The savings come only from drawing fewer frames. Forced to 60 Hz (`AIPET_FPS=60`), the new build costs what the
    old one did (10.4 % against 10.0 %). Our own work per frame is small (a tick takes about 0.03 ms); the rest is
    iced and GL.
  - Frames: 30 a second in calm seconds, 56–60 in lively ones; before, 60 in every second.
  - Wayland input-region updates: at most 10 a second, never sooner than 100 ms apart (about 7 a second on average,
    against 22–27 before).
  - Desktop region updates: about 17–19 a second, up to about 43 (8 a second before). What is drawn is now exact and
    sent at once, so it is never cut off, and the input region goes with it (see "Still open").
  - Allocations per frame: 33 in a tick (46 before), 1 in `hit_rects` (9), 43.5 in the view (45).
  - RSS is about 275 MB, of which 247 MB is shared GL/EGL libraries (anonymous memory 28 MB). It stayed flat over
    200 s. Debug builds use 35–42 % of a core.
- **GPU.**
  - With the default settings wgpu picked NVIDIA's Vulkan. Its Wayland surfaces led iced to an fp16 format, which
    washed out every colour.
  - Even when rendering on the Intel GPU, listing the Vulkan adapters opened NVIDIA's driver. That kept the dGPU
    awake: 50 s active during a 45 s run, against 0 when nothing ran.
  - With `WGPU_BACKEND=gl` and `WGPU_POWER_PREF=low` (now the defaults in `main.rs`), colours are correct, no NVIDIA
    handles are opened, and the dGPU stays suspended.

## Hands-on tests still to do (need real clicks)

Everything here was changed by the fixes, or never tried. Run from `rust/` with
`AIPET_BACKEND=wayland AIPET_DEBUG=1 cargo run -p aipet-spike`:

1. **Poke:** left-click the sprite without moving. It should bounce. The log shows `drag: idle -> pressed`, then
   `-> idle`, with no `surface:` line. Quick taps and touchpad taps must never leave the pet following the pointer.
2. **Margin drag** (the default): drag the pet around and release.
   - It should follow, with a squash on landing, and land without a jump.
   - Stop, then wiggle 1–2 px: the grabbed spot should end up under the pointer, with no drift.
   - Drag past each screen edge: the sprite must stay fully on screen.
   - Expect it to glide behind the pointer; see "Dragging on Wayland".
3. **Overlay drag:** `AIPET_DRAG=overlay`. The log should read growing over the output, then shifting to the
   output's corner, then over the output, returning home, settling, idle. Report any visible jump or slide at the
   start and at the drop. At scale 1.25 or 4/3, a press right after a drop must count.
4. **Menu:** right-click the sprite. The menu should appear centred above it. Then check:
   - A left or right click on the pet while it is open only closes it: no poke, no drag, no new menu.
   - A click elsewhere closes it.
   - Near the top of the screen it opens below the sprite, not over it. At the left and right edges it stays fully
     on screen.
   - "Show bubbles" toggles the bubbles. "Always on top" moves the pet behind windows (Bottom layer) and back.
   - "Settings…" and "Avatars…" open Settings. Asking again reopens it in front.
   - Quit exits.
   - Right-click within about 0.4 s of a drop: the menu must still be placed right.
   - If the popup ever fails to appear in 0.5 s, the menu is drawn inline, that time only. It closes 1 s after the
     pointer leaves the pet.
5. **Hover:** the wave, the eyes and a bubble's close button should feel immediate. The clickable area may lag the
   drawing by up to about 130 ms.
6. **Bubbles:** hovering shows the close button, which dismisses the bubble. A dismissed bubble comes back when its
   chat does something new (in the demo, Claude's changes at 6 s). A click on a folded stack spreads it out. The
   folded cards behind the front one show no hand cursor.
7. **Click-through:** clicks on the transparent parts, including the gaps between bubbles, must reach the window
   below.
8. **Settings:** the toggles, mood pills and avatars update both Settings and the pet.
9. **Losing the output** (optional): disable or unplug the output mid-drag, and once with the pointer resting on the
   pet. The pet must come back at home, neither held (arms down) nor hovered (its eyes don't follow a pointer that
   is elsewhere).
10. **Desktop shell on a real X11 session** (GNOME on Xorg, KDE X11, XFCE):
    `AIPET_BACKEND=desktop AIPET_DEBUG=1 cargo run -p aipet-spike`. Check:
    - Clicks beside the pet reach the window below.
    - `xprop -id $(xdotool search --name '^AiPet spike$') _NET_WM_STATE` lists ABOVE, SKIP_TASKBAR and SKIP_PAGER.
    - A poke bounces, and a drag feels smooth and lands with its squash.
    - A lost release: start a drag, then let a WM or screenshot grab (or Super+drag) take the release. Within about
      0.1 s of letting go the pet must stop following, and the log says `drag cancelled`.
    - Inline menu: right-click the sprite, then left-click a bubble outside the menu. The menu closes (the bubble
      still spreads or dismisses). A click on the menu's padding or separator keeps it open, and every item works.
    - While the pet hops and bubbles slide or fold: no clipped glow, shadow or bubble edges, and no flicker from the
      region updates. Measure the CPU, calm and with Settings open.
11. **GNOME Wayland:** plain `cargo run -p aipet-spike` takes the desktop shell and must start through XWayland
    without a panic. Then put Settings on another workspace: does the pet drop to about 1 frame a second?
12. **Windows and macOS:** whether the window is transparent (the wgpu swapchain alpha), click-through, staying out
    of Alt+Tab and the taskbar, topmost, and dragging with DPI scaling. The same checks for clipping and flicker as
    on X11. Windows: clicks up to about 10 px around the sprite and 14–18 px around bubbles (the glow and shadows) are
    taken by the pet, farther ones click through; also drag with swapped mouse buttons, and lose the capture
    mid-drag. macOS: a drag's release, and a missed one, is noticed.
13. **Fractional scaling and other compositors:** only scale 1 on Hyprland was tried for the Wayland shell. At 1.25
    and 1.5, and on Sway or KDE, check the placement, the input region, both drag modes and the menu.

## Dragging on Wayland

A layer surface only knows pointer positions relative to itself, and margin changes are never acknowledged. So a drag
moves the surface by changing its margins, and has to know when a change has landed:

- **`margin`**: one margin change is in flight at a time. Motion is ignored until two frames drawn after the change
  was committed come round, which proves it applied; then the pointer delta is added. It is exact (it was measured
  that Hyprland reports pointer positions for the new geometry straight away), but it moves at most every other frame.
- **`overlay`**: for the duration of the drag, the surface grows to cover the whole output, so pointer positions are
  output-relative. The pet is drawn under the pointer, then the surface shrinks back at the drop.

If the surface is lost mid-drag, the pet is let go without a poke or a landing.

Hyprland's `layers` animation (in the user's config: speed 3.81, easeOutQuint) animates every change of a layer
surface's position. So the margin drag glides behind the pointer, and the overlay drag slides at its start and end.
The likely fix is a layer rule with `no_anim` for namespace `aipet-spike`; for the real app it would go in
`packaging/linux/hyprland/`. Untested, and the syntax is unconfirmed:
`hyprctl eval 'hl.layer_rule({ name = "aipet-spike", match = { namespace = "^aipet-spike$" }, no_anim = true })'`.

## Review findings

Three reviewers looked at the Wayland shell, the desktop shell with its native code, and the UI's fidelity to the C#
together with performance. Each finding was checked against the code and the library sources before it was fixed. A
second review then checked the fixes, and ran both shells.

**Fixed**
- High: the desktop shell crashed on GNOME Wayland, and wherever `$XDG_RUNTIME_DIR/wayland-0` exists (libwayland fell
  back to it, and wgpu's EGL then panicked on the X11 window). `main` now sets `WAYLAND_DISPLAY` to an empty value and
  removes `WAYLAND_SOCKET`, and `aipet_desktop::run()` refuses to start otherwise.
- Medium:
  - Both shells rendered at the display rate in every state. Now the frame pacing above (`PetUi::frame_interval`).
  - The inline menu was only a layer over the bubbles: presses on its padding reached them, and a dismiss badge could
    show beside it. It is now `opaque`, and the bubbles under it don't see the pointer.
  - Wayland: a pet surface closed before its first frame stayed `Opening` forever. The surface's own `Closed` event
    is now heard, and the retry timer makes it again.
  - Wayland: after one popup failed to appear, the menu stayed inline for the whole session. Now the fallback is for
    that time only, and the inline menu closes 1 s after the pointer leaves.
- Low, `aipet-spike`: the Wayland shell's panic without wgpu now ends in the clear failure line; `WAYLAND_SOCKET` is
  handled; the doc of `AIPET_OPEN_SETTINGS` is right.
- Low, `aipet-desktop`:
  - A lost left release left the pet following the pointer. The drag now reads the button's state from the desktop
    and is cancelled 100 ms after the button is up with no release.
  - It builds on the BSDs (the X11 code is gated as winit's is).
  - A press outside the inline menu, even one a bubble takes, closes it.
  - The 18 px padding is gone: the X11 bounding shape and the Windows region are `drawn_rects()`.
- Low, `aipet-wayland`:
  - A surface lost mid-drag now lets the pet go. The second review found that the pet also kept the pointer's last
    position, so it stayed hovered and at 60 Hz. The pointer now leaves with the surface.
  - A press on the pet closes an open popup. The second review found that a left press on the sprite also poked or
    dragged the pet; now it only closes the menu, as in the desktop shell.
  - The popup is anchored to the sprite's box, moved sideways to stay on the output, and flips below the sprite.
  - The pet's event subscription keeps its identity, so no press or release is dropped.
  - The input region goes out at most every 100 ms (at once when the inline menu opens or closes). The second
    review found gaps of 85 ms, since the wait was counted from when a tick was due; it is now counted from when the
    region really went out, in both shells.
  - The overlay drag knows the home size at fractional scales.
  - The margin drag counts its frames from the commit.
- Low, `aipet-ui`: a dismissed bubble comes back when its chat does something new; the ghost bubble shows even when
  every bubble was dismissed; folded cards behind the front one take no clicks and show no hand cursor; fewer
  allocations; a new review now plays the 6 s attention alert.
- This file, which the second review found out of date.

**Rejected**
- Part of the BSD finding: `APP_ID` and the `platform_specific` blocks stay `cfg(target_os = "linux")`, because
  iced_core's `PlatformSpecific` has `application_id` only on Linux. FreeBSD builds with them as they are.

**Still open**
- The desktop start position uses the whole monitor, not the work area, so the pet can start under or over a bottom
  panel. It needs a work-area query on each platform (X11 `_NET_WORKAREA` with the RandR primary monitor,
  `SPI_GETWORKAREA`, `NSScreen.visibleFrame`), none of which can run here.
- Both shells: iced rebuilds and lays out every open window after any message (iced_exwlshell `multi_window.rs`,
  iced_winit `lib.rs`), so an open Settings window or popup is rebuilt on every pet tick. It is now 30 times a second
  when calm, and pointer moves no longer redraw it. Avoiding it needs upstream changes or a redesign.
- The inline menu fallback (Wayland) is drawn at a fixed place, so it can overhang the left and right output edges by
  up to 50 px, or sit above the output when the pet is parked at the top. It is rare (only when a popup fails) and
  closes by itself.
- Desktop region updates are about 2.3 times as frequent as before (see "Cost"). The total cost is lower, but on
  Windows each one is a `SetWindowRgn` that redraws. If flicker or cost shows on Windows or real X11, give the drawn
  region a few px of slack that grows at once and shrinks lazily.
- While lively (60 Hz), the pet costs as much as before, 12–14 % of a core. A pointer resting on the pet keeps it at
  60 Hz; it could drop to 30 Hz a second after the pointer stops.
- Under XWayland, a Settings window that isn't shown (another workspace, off-screen) slows both windows to about one
  frame a second. It is probably Xwayland throttling a surface without frame callbacks, since iced presents every
  window on one thread. It needs a decision, such as closing Settings when it is hidden.

## Other known limits

- **Settings on Wayland:**
  - No app_id (Hyprland shows class `''`) and it is tiled, so rules must match the title "AiPet Settings". Opening
    it re-tiles the current workspace and takes the keyboard focus.
  - It can't be raised (no xdg-activation), so it is closed and reopened instead.
  - It is destroyed on close, with no close request to intercept.
- **Popups on Hyprland** are misplaced sideways while the layer is still animating if sliding is allowed. The menu
  uses a vertical flip only, and its anchor is moved sideways by the shell instead.
- **The desktop shell on Hyprland** is only a fallback. XWayland there ignores input shapes and drops
  `_NET_WM_STATE`. The installed Hyprland rules match class `AiPet`, not `aipet-spike`, so it gets a border and blur.
- **The renderer:** only wgpu is built in, because iced's tiny-skia renderer can't draw a transparent window
  (softbuffer has no alpha). So without a working wgpu adapter there is no software fallback, and the pet doesn't
  start (it says so).
- **Frame pacing:**
  - The pet keeps ticking at 30 Hz or more even when its surface can't be shown (screen locked or off). Before, it
    stopped with the frame callbacks. Untested whether that costs anything there.
  - At 30 Hz the sprite's 24 fps frames stay up for 33 or 66 ms, an uneven cadence, but none is skipped.
  - A switch between 30 and 60 Hz restarts the timer, whose first tick comes one interval later.
  - Listening's nod counts as lively (35 px/s), and each mood change costs about 1 s of 60 Hz.
  - The first frame of a bubble animation that the script starts during a calm stretch can be one 33 ms step late.
- **Wayland commits:** iced_exwlshell makes an extra empty commit per frame because Hyprland offers
  `ext_background_effect_manager_v1`. It is upstream behaviour.
- **Upstream:** iced is pre-1.0 and breaks its API each release. iced_exwlshell has one main maintainer, thin docs,
  and calls its popups "a toy".

## Verdict so far

It looks viable. Everything the Avalonia version couldn't do on Hyprland works:
- exact click-through;
- visible on every workspace for free (a layer surface);
- no border, blur or shadow from the compositor, so no window rules needed.

The pixel-exact sprite port also shows the pet's animation carries over unchanged. With the frame pacing, the pet
costs about 8 % of a core when calm and 5–6 % asleep, but still 12–14 % while lively. Still undecided: how dragging
feels with Hyprland's layer animation, and how the cost compares with the C# pet.

## Next steps

1. The hands-on tests above, starting with both drag modes and the menu, then the layer `no_anim` rule.
2. Measure the C# pet's CPU on the same machine and compare.
3. The open findings: the work area, what to do with a hidden Settings window, and the region slack if Windows or
   X11 needs it.
4. Commit the fixes once the hands-on tests pass.
5. Then decide between three routes:
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
- **GNOME Wayland** has no layer-shell, so a default run takes the desktop shell through XWayland. Its crash there is
  fixed, but it was only reproduced here with a linked `wayland-0`, never on GNOME itself (hands-on test 11).
- **The GPU defaults** were tuned on a hybrid Intel+NVIDIA laptop. Elsewhere, check the colours and which GPU renders
  (compare with `WGPU_BACKEND=vulkan`).
- **The Hyprland findings** (layer animation, popup placement, the warp syntax) were measured on 0.56.2 with the
  user's own config.
