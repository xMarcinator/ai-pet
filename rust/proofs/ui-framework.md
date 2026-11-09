# UI framework spike: gpui, a minimal stack and Slint against iced

Scope: task fnt-73-ui-framework-spike-gpui-or-something.1, the whole of spec FNT-73. Does gpui, or something simpler
than iced, fit the pet's small UI better: one transparent, click-through, always-on-top pet window, plus a Settings
window of forms? Findings only; the pet's crates are untouched.

Machine: Arch Linux, Hyprland 0.56 (Lua config), eDP-1 1920×1200 at scale 1, Intel UHD 770 driving the screen plus an
NVIDIA RTX A2000. Mesa 26.2.6; the only Vulkan driver installed is NVIDIA's (no `vulkan-intel`). Rust 1.99.1. The
machine was busy (another session building: 1-minute load 4–10) during the prototype runs; the calm runs came later, with only the
Claude desktop app running (see "Calm CPU and RSS").

## The prototypes

Throwaway code in `spikes/ui-framework/` on this task's scratch branch `worktree-agent-af0c406e01f20bf2e`, commit
df8c5f6 (never to be merged; the task names the branch `spike/ui-framework`, so rename it when keeping it). Each candidate is a
standalone cargo project (its own `[workspace]`, not a member of `rust/`), and they share `common/`, which wears
Sprout through `aipet_sprite::Pet` (path dependency on `rust/crates/aipet-sprite`) and provides one bubble's box, the
sprite's transform (hop, squash and stretch), the shadow, a per-pixel hit test and the input rects (the bubble, plus
one rect per painted row run of the sprite, padded by 2 px). Every prototype ticks and draws at the iced pet's calm
pace, 30 Hz, and draws the whole surface each tick (no damage tracking), so per-frame work is comparable.

| Folder | Stack | Lines |
|---|---|---|
| `common/` | Sprout, bubble box, hit mask, input rects, tiny-skia scene drawing | 200 |
| `minimal/` | smithay-client-toolkit 0.21.1 layer surface, wl_shm ARGB8888 buffer, tiny-skia, cosmic-text 0.19 | 390 |
| `slint/` | the same Wayland code as `minimal/`, but the content is a Slint 1.18.1 component drawn by Slint's software renderer through a custom platform (`MinimalSoftwareWindow`) | 440 |
| `gpui/` | gpui-pre 0.3.12 (a crates.io snapshot of Zed's gpui, zed@d2a4fb6) with `WindowKind::LayerShell` and `Window::set_input_region` | 250 |

Run any of them with `SPIKE_SECONDS=10 SPIKE_DEBUG=1 SPIKE_STATE=idle target/release/spike-<name>` after
`CARGO_TARGET_DIR=../target cargo build --release` in its folder. They open at the top of the screen, away from the
real pet. Screenshots of each were taken with `grim -g` and checked by eye; the input region was checked in
`WAYLAND_DEBUG=1` traces (`wl_region.add` with the bubble and 22 sprite rows, re-sent only when the art changed and at
most every 100 ms).

What was not tried, for every candidate: a real click or drag (no synthetic input tool is installed, and pointer
warps would have moved the owner's cursor), X11, XWayland, Windows, and scales other than 1. Hyprland honouring a
layer surface's input region was already verified by the iced spike (`rust/SPIKE.md`, "What's verified").

### Which gpui

- **gpui 0.2.2**, the last release of the official `gpui` crate on crates.io: no layer-shell, no input region; on
  Linux it renders with blade (Vulkan only). Out on Hyprland: a normal xdg toplevel needs window rules.
- **gpui-ce 0.3.0**, the community fork: has layer-shell, but `set_input_region` is the trait's no-op on Linux.
- **gpui-pre 0.3.12**, weekly crates.io snapshots of Zed's own `crates/gpui` (owner: huacnlee, the gpui-component
  author; 13 versions since 2026-09-03, the latest published today). It has `WindowKind::LayerShell`, a real
  `set_input_region` on Wayland (a no-op on X11, Windows and macOS), and renders through wgpu (Vulkan or GL on Linux,
  DirectX 11 + DirectComposition on Windows). This is the one prototyped. Its `gpui-pre-platform` needs the
  `wayland` and `x11` features: without them it silently runs headless (the first build did, with no window and no
  error).

## Results per candidate

### iced 0.14 (today)

Today's pet, from `rust/SPIKE.md`, `rust/notes/` and `rust/proofs/memory.md`: everything the pet needs works on
Hyprland (layer surface through iced_exwlshell, exact input region, both drag modes, xdg popup menu), on X11 (SHAPE
input and bounding regions, EWMH above, through x11rb) and is written for Windows (`SetWindowRgn`, `HWND_TOPMOST`,
`WS_EX_TOOLWINDOW`). The cost is the GPU: iced's tiny-skia renderer can't draw a transparent window (softbuffer has no
alpha), so the pet needs wgpu, and Mesa's GL driver with LLVM is about 124 MB of its RSS even after fn-1.29's cuts.

### Minimal stack: a layer surface and a CPU buffer

**Works on Hyprland:** a transparent layer surface on the Top layer with no window rules (`hyprctl layers` shows
it), crisp ×5 cells (tiny-skia `FilterQuality::Nearest`, at any scale, squash included), the bubble with real text
(cosmic-text, the shaper iced already uses; 28 ms to load fonts and shape at start), an exact input region, and the
iced pet's margin drag (move the surface by the pointer's delta, one change in flight until the next frame
callback). Its RSS is about 14 MB: no GL, no Vulkan, no NVIDIA mappings. The binary is 6.6 MB (5.0 MB stripped).

**Findings:**
- tiny-skia draws premultiplied RGBA, and wl_shm's `ARGB8888` is premultiplied BGRA in memory: one R/B swap per
  pixel, no conversion of alpha.
- softbuffer, the usual "window + CPU buffer" crate, is no good for the pet: 0.4.8 presents `Xrgb8888` on Wayland,
  `NoneSkipFirst` on macOS and a plain blit on Windows, so there is no alpha except by accident on a 32-bit X11
  visual. The minimal stack therefore presents natively on each platform instead (below).
- Each platform needs its own few hundred lines, but most already exist:
  - **Wayland:** smithay-client-toolkit (the prototype). Still to write for real: wp_fractional_scale + viewporter
    (the prototype handles integer scales only), the cursor (cursor-shape-v1), the right-click menu as an xdg popup,
    and the drag port. `aipet-wayland/src/drag.rs` is a pure state machine and should carry over.
  - **X11:** a 32-bit ARGB window from x11rb with `PutImage` (MIT-SHM for speed), and `aipet-desktop`'s existing
    SHAPE and EWMH code (`native/x11.rs`). Not built in this spike.
  - **Windows:** a layered window updated with `UpdateLayeredWindow` from a premultiplied BGRA DIB, which is exactly
    what tiny-skia produces. Layered windows updated that way hit-test per pixel by themselves (fully transparent
    pixels click through), so `SetWindowRgn` isn't even needed; topmost and toolwindow as today. No GPU, no
    swapchain-alpha question (the open item 12 of `rust/SPIKE.md`). Not built in this spike: Windows trials are the
    owner's.
  - **macOS** (not a release target): an NSView whose layer contents is a CGImage with premultiplied alpha, plus
    the existing `setIgnoresMouseEvents` polling. Or keep the iced desktop shell there.
- No widgets at all: Settings can't move to this stack.
- The CPU draws every frame. The prototype redraws all of its 260×230 surface 30 times a second; the real pet window
  is up to 380×600, and at scale 2 four times that. A real port wants damage tracking (redraw the sprite's box and the
  bubble that changed, not the window) and painted pets' frames pre-scaled once per scale, as `aipet-ui` already
  pre-builds images today.

### Slint 1.18 (software renderer on the minimal stack)

Slint's own Wayland support goes through winit, so on Hyprland it is an xdg toplevel and needs window rules. The two
crates that put Slint on a layer surface are copyleft: layer-shika 0.3.1 is AGPL-3.0-or-later and spell-framework
1.0.6 is GPL-3.0-or-later, so neither can ship in an MIT app. The prototype instead uses Slint's documented
custom-platform API: a `MinimalSoftwareWindow` renders the component into a premultiplied RGBA buffer, which goes
into `minimal/`'s layer surface. Input (hit test, drag) stays ours.

**Works on Hyprland:** transparency, the bubble with text and a 1.5 px border from Slint markup, the input region
and margin drag (shared with `minimal/`), no GPU (RSS about 14 MB). Binary 11.2 MB (9.2 MB stripped).

**Findings:**
- Slint 1.18's software renderer ignores `image-rendering: pixelated` (no reference to it in
  `i-slint-renderer-software`), so the 26×24 sprite came out smoothed. The prototype now scales the cells up itself
  (nearest neighbour, ×5 × output scale) and Slint draws the image at about 1:1; squash and breathing still make the
  scale fractional, and Slint filters that, so the cells' edges soften slightly during a squash. tiny-skia's nearest
  filter in `minimal/` doesn't.
- A `#786ec8`-style colour literal inside `slint::slint!` fails to lex ("expected at least one digit in exponent":
  `6e` reads as an exponent). `rgb(...)` works; a `.slint` file compiled by `slint-build` avoids the macro.
- Settings could move to Slint in its normal winit backend with the software renderer: an opaque window, so
  softbuffer's missing alpha doesn't matter there. Then nothing in the app would need a GPU.
- Licence: GPL-3.0-only, or the Slint Royalty-free License 2.0 (free for desktop apps, with an attribution
  requirement: a "Made with Slint" badge or notice), or a paid licence. The pet is MIT; the royalty-free licence
  allows that, but adds a non-OSI licence and an attribution duty to the third-party notices (fn-1 R19).

### gpui (gpui-pre 0.3.12)

**Works on Hyprland:** a transparent layer surface with no window rules (`WindowKind::LayerShell`, namespace
`aipet-spike-gpui`), crisp cells (one `paint_quad` per painted cell, after the transform), the bubble as a styled
`div` with gpui's own text, and `set_input_region` sending the same 23 rects. Partial alpha looks right in three
checks (`SPIKE_BG=ff000080`, `ff0000f5`, `ffffffc0`: red at 50 and 96 %, white at 75 %). One oddity not chased: the
default bubble, white at 96 % (`fffffff5`), looked much more see-through than that in two screenshots over a dark
window, while red at 96 % was solid.

**Findings:**
- **No way to move a layer surface after it is made.** `LayerShellOptions.margin` is read once at creation; there is
  no `set_margin` (only `set_exclusive_zone` and `set_exclusive_edge`). So the iced pet's margin drag is impossible
  without patching gpui. The prototype works round it: the surface is a fixed 700×500 "playground" and a drag moves
  the pet inside it, with the input region following (and set to the whole surface while dragging). The real pet
  would need either an upstream `set_margin` (a small patch, but on a crate we'd have to vendor or fork) or a
  surface covering the whole output.
- **It picks the discrete GPU.** gpui sorts adapters by "the compositor's GPU, then Discrete > Integrated, then Vulkan
  over GL", and enumerates every backend first. Here, with no Intel Vulkan driver, it rendered on NVIDIA's Vulkan:
  145 NVIDIA mappings, RSS 314 MB, and (as `rust/SPIKE.md` found for wgpu) the dGPU kept awake. There is no
  `WGPU_BACKEND`-style switch; only `ZED_DEVICE_ID`, which can't select a GL adapter (GL reports device 0). Hiding
  the Vulkan drivers from outside (`VK_LOADER_DRIVERS_DISABLE='*'` plus `__EGL_VENDOR_LIBRARY_FILENAMES` at Mesa's
  vendor file, the trick `crates/aipet/src/mesa.rs` uses) puts it on Mesa GL on the Intel GPU: RSS 168 MB, no NVIDIA
  mappings, the same LLVM-backed GL driver iced uses. So gpui saves nothing on memory; it is the same GPU stack.
- **The input region is Wayland-only.** X11, Windows and macOS get the trait's no-op, so click-through there needs
  the same native code the iced desktop shell has (through the raw window handle). On Windows gpui's `PopUp` kind
  is `WS_EX_TOOLWINDOW | WS_EX_TOPMOST` and the window is DirectComposition, so transparency should work; untried.
- **Windows build:** release builds compile HLSL with `fxc.exe` from the Windows SDK (found through `GPUI_FXC_PATH`,
  `PATH` or the SDK folder) and panic without it. The MSVC Build Tools' Windows SDK has it; Git Bash is fine.
- 658 packages in its lockfile (all targets), the slowest build of the four (53 s just to rebuild after turning on
  its Linux features, on 24 threads under load), and the biggest binary: 39.8 MB (29.1 MB stripped).
- Maintenance: Zed's gpui has no stable releases on crates.io (0.2.2 is far behind Zed's main); you either depend on a
  third party's weekly snapshot (gpui-pre) or on a git revision of the whole Zed repository. Its API is shaped by Zed
  and changes without notice; documentation is examples and source.

### egui

Not prototyped; Slint was the toolkit picked (the spec asks for one of the two). In one line: eframe 0.37 renders
through glow or wgpu (the same GPU stack), has only whole-window mouse passthrough, and no layer-shell integration on
crates.io, so on Hyprland it needs window rules, and per-pixel click-through needs the same native code as iced.

## Comparison

| | iced 0.14 (today) | Minimal: sctk + wl_shm + tiny-skia + cosmic-text | Slint 1.18, software renderer on the minimal stack | gpui (gpui-pre 0.3.12) |
|---|---|---|---|---|
| Transparent pet window | Yes (wgpu only) | Yes, CPU buffer with alpha | Yes, CPU buffer with alpha | Yes (wgpu) |
| Click-through on the art only | Wayland input region; X11 SHAPE; Win32 region (native code) | Wayland input region; X11 SHAPE (existing code); Windows: free with `UpdateLayeredWindow` | Same as minimal | Wayland `set_input_region`; X11/Windows/macOS need native code |
| On top without rules: Hyprland | Yes, layer surface (iced_exwlshell) | Yes, layer surface | Yes, layer surface | Yes, but the surface can't move: drag needs a patch or an output-sized surface |
| On top without rules: X11 | EWMH ABOVE (existing) | Same code | Same code | `PopUp` kind, untried |
| On top without rules: Windows | `HWND_TOPMOST` (untried on hardware) | `WS_EX_TOPMOST` layered window (not built) | Same as minimal | `WS_EX_TOPMOST` toolwindow (untried) |
| Crisp pixels | Yes (`crisp`) | Yes, nearest at any scale | Only if we pre-scale the cells; fractional squash is filtered | Yes (a quad per cell) |
| Bubble text | cosmic-text in iced | cosmic-text, drawn by us | Slint text | gpui text |
| Settings window | iced (exists) | None: stays in iced | Slint (rewrite), still no GPU | gpui (rewrite; widgets via gpui-component) |
| Calm CPU, Linux (one run, load 1.3–1.6) | 4.40 % | 4.13 % | 3.27 % | 6.66 % on Mesa GL; 8.63 % by default (NVIDIA) |
| RSS, calm (after a 60 s settle) | 157 MB | 8.9 MB | 14 MB | 164 MB on Mesa GL; 273 MB by default |
| RSS (short run, not calm) | 164–176 MB (memory.md) | 14 MB | 14 MB | 168 MB on Mesa GL with the env trick; 314 MB by default (NVIDIA) |
| Release binary (stripped) | 30.4 MB (21.7 MB): the whole app | 6.6 MB (5.0 MB) | 11.2 MB (9.2 MB) | 39.8 MB (29.1 MB) |
| Packages in Cargo.lock | 487 (the whole workspace) | 103 | 500 | 658 |
| Windows build needs | MSVC (today's) | MSVC; pure Rust plus windows-sys | MSVC (software renderer: no clang/Skia) | MSVC plus `fxc.exe` from the Windows SDK for release builds |
| Licence | MIT | MIT / MIT-Apache | GPL-3.0 or royalty-free with attribution, or paid | Apache-2.0 |
| Maintenance | Pre-1.0, breaks each release; iced_exwlshell has one main maintainer | Smithay (sctk) and System76 (cosmic-text), both mature; platform code is ours | Company-backed, stable 1.x API | No usable release from Zed; weekly third-party snapshots or a git dependency on Zed |

The prototypes are 30–40 % of the iced pet's features (no menu, no stacked cards, no glow halo, no vector or painted
pets), so their binary sizes and package counts are lower bounds, and the AiPet binary includes the whole app (core,
hook server, watchers, TLS).

## Calm CPU and RSS

**Measured 2026-11-09, 19:47–20:05**, with `spikes/ui-framework/calm.sh` against the release iced pet at `6a1feeb`,
all five in one run, each alone, wearing Sprout. The method is fn-1's (`scripts/perf/pet-calm.sh`: a 60 s settle,
then five 30 s samples, the median), with one change: the load limit was raised from 1.0 to 3 (`MAX_LOAD=3`). With
only the Claude desktop app running, the 1-minute load stayed at 1.3–2.5, and three earlier attempts at the 1.0 limit
waited 10 minutes each and gave up. The five are comparable with each other; they are not comparable with the
performance gate's figures (taken at load 1.0 or less).

| Pet | Calm CPU median | Samples | RSS | PSS |
|---|---|---|---|---|
| iced (the real pet) | 4.40 % | 4.07, 4.33, 4.43, 4.40, 4.60 | 157 MB | 53 MB |
| minimal (sctk + tiny-skia) | 4.13 % | 4.17, 4.13, 4.13, 4.17, 4.07 | 8.9 MB | 6.1 MB |
| Slint (software renderer) | 3.27 % | 3.27, 3.30, 3.27, 3.23, 3.27 | 14 MB | 9.0 MB |
| gpui on Mesa GL | 6.66 % | 6.63, 6.56, 6.80, 6.80, 6.66 | 164 MB | 60 MB |
| gpui by default (NVIDIA) | 8.63 % | 8.70, 8.63, 8.50, 8.70, 8.46 | 273 MB | 149 MB |

- **CPU is close to a wash for the minimal stack:** 0.27 points under the iced pet, with 30–40 % of its features, at
  scale 1. It redraws and presents the whole surface every frame (`damage_buffer(0, 0, w, h)`), so damage tracking
  (task 2 below) is where its CPU would fall. Slint is a point under the iced pet; why it beats the minimal prototype
  was not investigated.
- **Memory is the real gain:** 9–14 MB against 157 MB, the GPU stack gone.
- **gpui costs more on both:** 50–100 % more CPU than the iced pet, and the same or more memory.

What was known before the measurement:
- **RSS** from the short runs above (`/proc/<pid>/status` after 5 s, idle, the art animating): minimal 14 MB, Slint
  14 MB, gpui 168 MB on Mesa GL and 314 MB on NVIDIA's Vulkan. The iced pet is 164–176 MB fresh (memory.md, after
  the cuts). The prototypes have no core, hook server or watchers; memory.md puts the iced pet's anonymous memory at
  15.6–27 MB, so a CPU-drawn pet should land around 30–45 MB RSS (an estimate, not a measurement).
- **Frames:** the minimal and Slint prototypes drew 29–30 frames a second (235 draws in 8 s), gpui ticked at 30 Hz.
- **CPU per frame** is the open question for the CPU-drawn stacks, since they redraw on the CPU what the GPU draws
  today. A data point from the Rive proof (`rust/proofs/rive.md`): tiny-skia drawing a 260×240 vector scene with 26
  fills, 13 strokes and 15 gradients took 0.81 ms, 2.4 % of a core at 30 Hz. Sprout is far simpler than that, but
  the real window is bigger and has more bubbles, which is why the first task below is a measurement.

## Recommendation

**Move the pet window only, to the minimal stack, behind a measurement gate. Don't move to gpui. Keep Settings in
iced for now.** The calm runs (above) back this for memory, not for CPU: the move is worth it for a pet that holds 9 MB
instead of 157 MB, and its CPU has to come from damage tracking, which the gate in task 1 must show at the real
window's size.

- **gpui is out.** It keeps the same GPU stack (wgpu on Mesa GL, the same LLVM-backed driver, about the same RSS
  once forced off the dGPU, and twice the RSS if not), it can't move a layer surface (so no drag without patching
  it), its input region is Wayland-only, Zed publishes no usable release, and its binary is the biggest. It answers
  none of the reasons for the spike.
- **The minimal stack** removes the GPU from the pet: about 14 MB instead of about 165 MB in the prototypes, no GL
  or LLVM, no dGPU risk, no swapchain-alpha question on Windows, and on Windows per-pixel click-through for free. Its
  crates are small, mature and MIT-licensed, and most of the platform code already exists in `aipet-desktop` and
  `aipet-wayland`. What it costs is drawing everything ourselves (the pet already draws its sprites with tiny-skia
  for hit masks and vector packs) and per-platform presentation.
- **Slint** would let both windows drop the GPU with one toolkit, but on the pet window it adds little over
  tiny-skia (we keep the layer surface, input and drag ourselves either way), it can't draw pixelated images, and it
  brings a non-OSI licence with an attribution duty. Worth reconsidering only if Settings is rewritten.

### The options and their cost

| Option | Tasks | fn-1 R18 (calm CPU ≤ C# + 0.5 pp; RSS ≤ 1.5 × C#) | Platform code |
|---|---|---|---|
| Keep iced | 0 | Unchanged: both met today (Rust 5.1–5.8 % against C# 12.5–13.6 %, 170 MB against 283–300 MB, memory.md) | Unchanged |
| **Move the pet window only, to the minimal stack** | about 9 (below) | RSS falls to an estimated 30–45 MB while Settings is closed (the prototype alone: 8.9 MB). Calm CPU about level with iced in the prototype (4.13 % against 4.40 %); it must stay within the C# + 0.5 pp budget, and should not exceed today's 5–6 % | `aipet-wayland`: iced_exwlshell replaced by smithay-client-toolkit (drag.rs kept). `aipet-desktop`: the pet window becomes a native ARGB X11 window and a Windows layered window; `native/x11.rs` and parts of `native/win32.rs` reused, `SetWindowRgn` dropped on Windows. `aipet-ui`: split into a framework-free pet scene and the iced Settings. macOS: compile-only today; keep its iced shell or drop it |
| Move both (Slint) | about 14: the 9 below with Slint drawing the bubbles, plus 4–5 to rewrite Settings (general, avatars, Jira, GitHub, updates: about 2,300 lines of iced today) and the licence notice | As above, and no GPU even with Settings open | As above, plus iced removed entirely |
| Move both (gpui) | about 14, plus vendoring gpui for `set_margin` | No gain on RSS; CPU unknown | Native click-through still needed on X11 and Windows |

The tasks for "move the pet window only":
1. **Gate:** grow the minimal prototype to the real window's worst case (380×600, three bubbles, scale 2), with
   damage tracking, and measure it against the iced pet in one run. Stop here if it can't beat the iced pet's calm
   CPU. (The first calm run is done: the prototype is level with iced on CPU and far below it on memory.)
2. The pet scene drawn with tiny-skia: pixel, painted and vector packs, glow halos, shadow, with damage tracking
   (redraw only what changed) and per-scale caches.
3. Bubbles, the reviews header, the music player and the inline menu, with cosmic-text.
4. The Wayland shell on smithay-client-toolkit: layer surface, input region, fractional scale and viewporter,
   cursor shape, both drag modes (port `drag.rs`), output loss.
5. The right-click menu as an xdg popup (or keep it inline only).
6. The X11 shell: a 32-bit ARGB window, MIT-SHM `PutImage`, the existing SHAPE and EWMH code.
7. The Windows shell: a layered window with `UpdateLayeredWindow`, topmost and toolwindow, drag; tried with the
   owner present (Git Bash).
8. Settings decoupled from the pet: iced opened on demand. In the same process the GL libraries stay mapped once
   Settings has opened (until a restart); in a child process (`AiPet --settings`, talking over the existing socket)
   they go away when it closes, which needs the socket to carry Settings' live updates.
9. Measurements on Linux and Windows against R18, hands-on tests, docs.

### For the owner (not done here)

- Real clicks: poke and drag on each prototype, and clicks on the transparent parts reaching the window below.
- Windows: nothing was built for Windows. The minimal stack's layered window, Slint's software renderer and gpui's
  DirectComposition window are all untried there.
- X11 and XWayland: not tried for any candidate.
- Scales other than 1, and other compositors (Sway, KDE).
