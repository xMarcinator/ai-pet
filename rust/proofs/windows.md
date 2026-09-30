# Windows proof: the winit shell on Windows 11

Scope: the spike's desktop shell (`aipet-desktop`, iced 0.14 on winit 0.30 with wgpu 27) run on real Windows for the
first time. The checks cover:
- transparency with each wgpu backend;
- the window region (`SetWindowRgn`) for click-through;
- `HWND_TOPMOST`;
- the `WS_EX_TOOLWINDOW` subclass, which keeps the pet out of Alt+Tab and the taskbar;
- dragging with `GetCursorPos` at 100 % and 150 % and across two monitors at different scales;
- the UI font;
- CPU and memory.

It resolves the spec's parked unknown about the Windows transparency route (R13, task
fn-1-migrate-aipet-from-net-to-rust.13).

## Setup

- A laptop with Windows 11 Pro (10.0.26200) and two GPUs:
  - Intel UHD Graphics (driver 32.0.101.7088);
  - NVIDIA RTX 2000 Ada Generation Laptop GPU (driver 32.0.15.9658, Vulkan 596.58).
- Two monitors:
  - the main one, landscape, 2560 × 1440 (by the start position in the logs), at 100 %, and at 150 % for one run;
  - a portrait one to the right of it (by the pointer positions in the `05` log: x 2642–2661, past the main one's
    right edge, at y 916–930), always at 100 %.
- The pet ran over an empty, maximised Notepad, so its white page showed through wherever the pet was transparent.
- The build was the release `aipet-spike.exe` from `wave/fn-1.13` at `db3db70` (SHA-256 `b42ecf60…60df`).
- Each run had `AIPET_DEBUG=1` except the CPU runs. With it, the log names the wgpu adapters, the one chosen for the
  pet window, and that surface's alpha modes and format.
- The user watched, clicked and took the screenshots. The runs were on 2026-09-29.

The screenshots are in [windows/](windows/).

## Verdict

The plain wgpu window works on Windows, so no CPU renderer and no `UpdateLayeredWindow` fallback is needed. It is
transparent with Vulkan (the default on this machine) and with GL. The window region, topmost, and hiding from
Alt+Tab and the taskbar all work as in the C#.

Three things failed. Two are fixed here, and the third has a chosen fix:
- A lost release (the Start menu taking it mid-drag) left the pet following the pointer. Fixed: losing the focus now
  ends a drag.
- A press arrived without a position once, while the debug probe held up the event loop. Fixed: the probe runs
  before the window is shown.
- Dragging across monitors at different scales makes the scale flip back and forth at the crossing. The chosen fix is
  to hold the scale change until the drop (see [What later tasks take from this](#what-later-tasks-take-from-this)).

| Item | Result | Evidence |
|---|---|---|
| Transparency, default (Vulkan, Intel) | Pass | [01-default-100-transparency.png](windows/01-default-100-transparency.png) |
| Transparency, `WGPU_BACKEND=vulkan` | Pass | [03-vulkan-transparency.png](windows/03-vulkan-transparency.png) |
| Transparency, `WGPU_BACKEND=gl` | Pass | [04-gl-transparency.png](windows/04-gl-transparency.png) |
| Transparency, `WGPU_BACKEND=dx12` | **Fail**: black inside the region | [02-dx12-transparency.png](windows/02-dx12-transparency.png) |
| Click-through beside the pet and between bubbles (`SetWindowRgn`) | Pass with the default backend (Vulkan), at 100 % and 150 %; not reported for the `dx12`, `vulkan` and `gl` runs | user |
| Flicker or clipping while the region follows the pet | Pass: none seen at 14–18 region updates a second | user; logs |
| `HWND_TOPMOST`, and unticking "Always on top" | Pass (what ticking it again did wasn't reported) | user; `01` log |
| Out of Alt+Tab, Task View and the taskbar (`WS_EX_TOOLWINDOW` subclass) | Pass | user (Alt+Tab, Task View); `01-taskbar.png`, not kept (taskbar) |
| Drag at 100 % | Pass | user; `01` log |
| Drag onto the other 100 % monitor | Not shown | `01` log |
| Drag at 150 % on one monitor | Pass | user; `05` log |
| Drag across monitors at 150 % and 100 % | **Fail**, transient: the scale flips at the crossing | user; `05` log |
| Lost release (Start menu mid-drag) | **Failed**; fixed here, not yet re-run by hand | `07` log; unit test |
| First click after start; click with the pointer resting on the pet | Pass; one press without a position, caused by the debug probe (fixed) | `01`, `02`, `07` logs |
| Font | Segoe UI on Windows, nothing bundled | [01-settings-font.png](windows/01-settings-font.png); unit test |
| The menu's rows | Off-centre: follow-up | [01-menu-offset.png](windows/01-menu-offset.png) |
| Scale 150 %: crisp pixels and text, nothing cut off (header, bubbles and sprite) | Pass | [05-150-transparency.png](windows/05-150-transparency.png) |
| The menu at 150 % | Not checked: opened only at scale 1 | `05` log |
| CPU and memory | Measured (below) | sampler output |

## Findings

### Transparency and the GPU

| Backend | Adapter chosen | Alpha modes offered | iced gets | Seen | Shown after start |
|---|---|---|---|---|---|
| default (all) | Vulkan, Intel UHD | Opaque, Inherit | Opaque | transparent | 4.5–7.3 s (3 runs: 6.8, 7.3 and 4.5 s) |
| `vulkan` | Vulkan, Intel UHD | Opaque, Inherit | Opaque | transparent | 6.5 s |
| `gl` | GL 4.6, Intel UHD | Opaque | Opaque | transparent | 0.9 s |
| `dx12` | DX12, Intel UHD | Opaque | Opaque | opaque (black) | 14.0 s |

- The alpha modes don't decide transparency here. Every surface offered only `Opaque` (or `Inherit`), and iced took
  `Opaque` every time, yet Vulkan and GL showed the white page smoothly:
  - through the bubbles' rounded corners and soft shadows;
  - through the sprite's glow and sparkles;
  - through its ground shadow.
  Why wasn't established. The likely reason is that winit asks DWM for blur-behind on a transparent window, and the
  Intel driver's Vulkan and GL presentation keeps the alpha through it.
- DX12 makes its swapchain from the window's HWND with the alpha ignored, which is the only mode wgpu offers there.
  Inside the window region everything the pet doesn't paint came out black, in rectangles around the bubble and the
  sprite. Outside the region the desktop still showed. Clicks weren't checked in this run: the user reported only
  "dark boxes around the pet". The black boxes are the window region, which clips hit-testing as well as drawing, so
  clicks outside them would go through.
  - wgpu 27 has a transparent DX12 path, a DirectComposition swapchain (`WGPU_DX12_PRESENTATION_SYSTEM=Visual`). iced
    0.14 can't use it, because iced_wgpu builds its wgpu instance with the default options and ignores that
    variable.
- Start-up time: from the shell's start to the window showing took 0.9 s with GL and 4.5–7.3 s with Vulkan.
  - The Vulkan and DX12 instances open the NVIDIA driver: their adapter lists include the RTX 2000, and GL's lists
    only the Intel GPU. On Linux the same thing kept a hybrid laptop's discrete GPU awake (SPIKE.md, "GPU"). It is
    likely here too, but it wasn't measured.
  - The pet never drew on the NVIDIA GPU: wgpu's low-power preference picked the Intel one every time.
- **The transparency route:** a plain wgpu window through GL or Vulkan, never DX12. GL should be the Windows default,
  as it already is on Linux: it is transparent, it shows in under a second, and it doesn't load the NVIDIA driver.
  Vulkan is the alternative. That default belongs in the pet binary's `main` (see
  [What later tasks take from this](#what-later-tasks-take-from-this)). The CPU cost below was measured on Vulkan, not
  GL.

### The window region, topmost, Alt+Tab and the taskbar

- **Click-through:** with the default backend (Vulkan on the Intel GPU), clicks on the Notepad page beside the pet and
  in a gap between bubbles went to Notepad at 100 %, and a click beside the pet went to Notepad at 150 %. The `dx12`,
  `vulkan` and `gl` runs have no report on clicks, and their logs can't show one: a click that goes through never
  reaches the pet. The window region (`SetWindowRgn` on the HWND) doesn't depend on the backend. As designed, the pet
  takes clicks close to its sprite and bubbles, where its glow and shadows are drawn. The `01` log shows two presses
  just left of the sprite that landed in its glow and were taken by the pet ("not on the sprite").
- **Flicker:** the user watched the pet hop and the bubbles slide, fold and spread, and saw no flicker, flash or
  cut-off edge. At the time the window region changed 14–18 times a second (16.4 a second over run `01`'s 7 minutes),
  each change a `SetWindowRgn` with redraw. The region slack that SPIKE.md holds in reserve for this isn't needed.
- **Topmost:** a Notepad window dragged over the pet stayed behind it, and with "Always on top" unticked Notepad
  covered the pet. "Always on top" was ticked again afterwards (the `01` log's press at 289.518 s lands on that row),
  but what that did wasn't reported.
- **Alt+Tab, Task View, taskbar:** the pet ("AiPet spike") showed in none of them. The taskbar screenshot isn't kept
  here, because it shows the user's own pinned apps.

### Dragging and DPI

- **At 100 % and at 150 %:** dragging slowly and quickly kept the grabbed spot under the pointer.
- **Onto the other monitor at 100 %:** not shown. Every drop in run `01` had the pointer on the main monitor, and the
  user's answers for that run don't mention the second monitor. The log's only window position with a negative y is
  (2022, −329), after a drag up (100.2–100.5 s). The pointer was then at (2225, 195) and the sprite's box at
  y 139–259, on the main monitor: only the top of the window went past the main monitor's top edge.
- **Across 150 % and 100 %:** the pet flickered at the crossing, changing between its two sizes, and could land on
  the far side of the target monitor, away from the pointer. It snapped back under the pointer as soon as the pointer
  moved on.
  - The log shows the scale flipping between 1 and 1.5 three to seven times within 0.10–0.24 s at four of the five
    crossings, all during drags:
    - 87.812–87.912 s: 5 changes in 0.10 s;
    - 93.502–93.672 s: 3 in 0.17 s;
    - 105.945–106.184 s: 7 in 0.24 s;
    - 110.930–111.120 s: 5 in 0.19 s.
  - The other crossing, back onto the 150 % monitor at 108.482 s in the same drag as the last two, was a single change
    with no flipping.
  - Every crossing was sideways, through the main monitor's right edge. On the 100 % monitor the pointer was at
    (2642, 917) and (2661, 930), in physical pixels, when two drags ended there, and at (2651, 916) when one began
    there: right of the main monitor's 2560 px and within its height. The user noted that the second monitor is
    portrait and the main one landscape. Stacked monitors weren't tried.
  - Cause: Windows gives the window the scale of the monitor that holds most of its area, and the pet's window changes
    size with the scale (380 × 600 at 100 %, 570 × 900 at 150 %). Near the edge, the window placed for one scale has
    its larger part on the other monitor.
    - At each change, winit resizes the window and moves it onto the new monitor, nudging it pixel by pixel where the
      new size doesn't fit.
    - The next frame of the drag puts the grabbed spot back under the pointer at the new size, which puts the larger
      part back across the edge.
    - For a band of pointer positions no placement is stable, so the scale keeps flipping while the drag keeps moving
      the window.
  - The drag's arithmetic itself is right at any single scale. Its target, `window + (pointer − press)`, is
    `pointer − grab offset`, both in the scale of the moment.
  - Chosen fix (not done here, since it needs another run by hand):
    - While a drag is on, hold back the scale change, so the pet keeps its size until the drop. The pet window's
      subclass already sees every message: it would hold `WM_DPICHANGED` while the button that started the drag is
      down.
    - At the drop, hand winit the window's current DPI, with the window placed so the grabbed spot stays under the
      pointer.
- **The menu at 150 %** wasn't checked. In run `05` it was opened three times (200.7, 259.0 and 297.9 s), all after
  the last drag had left the window at scale 1 (111.120 s), so the whole menu the user saw then was drawn at 100 %.
- **A lost release:** the user started a drag, pressed Win while still holding the button (the Start menu opened), then
  let go. The pet kept following the pointer until the next click, 9.4 s later (09.1–18.5 s in the `07` log), and no
  `drag cancelled` line came.
  - The drag reads the button with `GetAsyncKeyState`, which kept reporting it held after the Start menu had taken
    the release.
  - Fixed: when the pet loses the focus, which the Start menu, Alt+Tab or any other app takes, the drag ends as a lost
    one, with no poke and no landing. This is platform-neutral, in the shell (`Unfocused`), and a unit test covers it.
    It still needs the Start-menu check by hand.
- **Pressing with the pointer resting on the pet:** three clicks without moving (27.6, 29.7 and 39.8 s) each poked.
- **The first click after start** poked. It arrived with its position (`Left press at Some(..): on the sprite`).
  - The earlier run's click that came without a position didn't recur in any normal run.
  - One did come in the DX12 run: `entered`, `left`, then `Left press at None`, all at 18.167 s.
    - They came with the GPU line, 0.36 s after the event loop was running again: the log's first line after the
      stall is at 17.808 s, so the debug probe had blocked the loop for at most 3.8 s (14.033–17.808 s) with the window
      shown.
    - The user's click had queued up behind the probe, and then, like the probe's own result, behind the frame ticks
      that had queued up during the stall. A "pointer left" message was handled before the queued press, so the press
      had no position. winit's Windows backend drops the position a button message carries, and Windows takes posted
      messages such as `WM_MOUSELEAVE` from the queue before input.
  - Fixed: the probe now runs before the window is shown.
  - If a press without a position ever shows up without the probe, the fix is in the subclass: pass winit a
    `WM_MOUSEMOVE` carrying the press's own position just before the press.

### Font

- Noto Sans isn't on Windows. The pet asks for **Segoe UI** there, and Noto Sans elsewhere
  (`aipet-ui/src/style.rs`).
  - The C# asks for Segoe UI Variable Text, then Segoe UI, so on Windows it never showed Noto Sans either.
  - Segoe UI ships with every Windows 10 and 11, with a semibold face that the text stack (fontdb) files under
    "Segoe UI" at weight 600.
  - Segoe UI Variable Text isn't asked for: it is an instance of Windows 11's variable font, whose family name is
    Segoe UI Variable. It has the same vertical metrics as Segoe UI.
  - Nothing is bundled, so no licence is added. Bundling Noto Sans under the OFL was rejected, because the pet would
    then look unlike the C# pet on the same machine.
- A Windows-only test checks that both the regular and the semibold weight resolve to faces Windows has, at their own
  weights. It failed before the change with "Windows has no \"Noto Sans\"".
- Settings renders in Segoe UI, with semibold headings and regular hints ([01-settings-font.png](windows/01-settings-font.png)).
- The line height stays at Noto Sans' 1.362 em on Windows, because the bubbles' text positions (`view.rs`) are worked
  out from it. Segoe UI's own line height, which Avalonia uses there, is 1.330 em (2210 + 514 per 2048). The
  difference is under half a pixel per line at the pet's sizes.
- **The menu's rows:** the user noticed the menu's text "a bit offset". They didn't mention the check marks or say
  which way the text was off. Measured on [01-menu-offset.png](windows/01-menu-offset.png), the offset is vertical, and
  the check marks share it: each label's capitals sit about 6 px above the centre of its 32 px row, the two check
  marks about 7 px, and the spare room is all below. `menu.rs` places each item's content at the top of its button,
  since iced lays a button's content out from its top-left corner. The C#'s menu items centre theirs. The same view
  draws the Wayland shell's menu.

### CPU and memory

This is the release build with no debug logging, at 100 %, running the spike's busy 40 s demo script (bubbles, moods,
an attention alert). The runbook asked that nobody touch the mouse during the two minutes; that wasn't recorded, since
these runs had no debug logging and the user reported nothing for this step. No build ran (0 of 60 checks saw `rustc`
or `cargo`). Each run had a 60 s settle, then 60 one-second samples, taken with the sampler in
[Re-running it](#re-running-it) (a slightly trimmed copy of it is shown there). CPU is a share of one core, out of 20
logical cores. The backend was the default, Vulkan on the Intel GPU.

| | CPU mean | CPU median | CPU range | Working set | Private bytes |
|---|---|---|---|---|---|
| Pet alone | 12.8 % | 10.7 % | 1.5–30.6 % (middle half 7.7–18.2 %) | 177.9 MB | 140.1 MB |
| Pet with Settings open | 17.5 % | 17.5 % | 3.1–36.8 % | 215.1 MB | 168.7 MB |

- The working set held steady throughout each run.
- For comparison, the desktop shell under XWayland on the Linux laptop took 7.6–7.7 % calm and 12.4 % lively, and
  11.6 % with Settings shown (SPIKE.md).
- The comparison with the C# pet on the same machine, with a fixed workload, is task 28's benchmark, not this.

## Changes made by this task

- `aipet-ui/src/style.rs`: Segoe UI on Windows, with its test.
- `aipet-desktop/src/shell.rs`:
  - With `AIPET_DEBUG=1`, the shell logs two `GPU` lines: the adapters, the one chosen for the pet window, its
    surface's alpha modes, what iced asks for and gets, and the format. The probe runs before the window is shown.
    - iced's API doesn't give the shell this. iced_wgpu logs it at info level through the `log` crate (the adapters,
      the one selected, the surface's formats and alpha modes, and the format and alpha mode it asks for), but the
      spike installs no logger, and `iced::system::information` needs the `sysinfo` feature, which isn't enabled, and
      gives only the adapter's name and backend. So the shell asks wgpu again the same way iced_wgpu does.
    - A `log` backend would show iced_wgpu's own record instead, as `notes/iced.md` §11 suggests.
  - Losing the focus ends a drag, with its test.

## What later tasks take from this

- **Task 15 (the pet binary):** default `WGPU_BACKEND=gl` on Windows as well as Linux, when the variable isn't set.
  - The pet is transparent with GL, shows in 0.9 s, and GL doesn't load the NVIDIA driver.
  - DX12 must never be picked: it is opaque. With every backend allowed, a machine without a Vulkan driver would get
    DX12.
  - Measure GL's CPU cost when this lands (task 28).
- **Task 27 (the Windows parity pass):**
  - Hold the scale change during a drag, as chosen above, and check the crossing between 150 % and 100 % by hand, with
    the monitors side by side, as here, and stacked, which wasn't tried.
  - Check by hand what this run didn't show:
    - the drag onto the other monitor at 100 %: drag the pet onto it, let go there, and drag it back;
    - the menu at 150 %, opened with the pet on the 150 % monitor: whole, crisp and not cut off;
    - click-through with GL and with Vulkan, beside the pet and in a gap between bubbles;
    - ticking "Always on top" again: once Notepad is clicked, the pet is back in front within 2 s.
  - Check the lost release through the Start menu, Alt+Tab and Win+D.
  - Check transparency with the NVIDIA GPU drawing, through Vulkan, the alternative route
    (`WGPU_BACKEND=vulkan WGPU_POWER_PREF=high`). Run with `AIPET_DEBUG=1`, and confirm that the "GPU for the pet" line
    names the RTX 2000.
    - Under the GL default, `WGPU_POWER_PREF` changes nothing, because GL lists only the Intel GPU. The NVIDIA GPU
      could draw GL only if something outside wgpu forces it, and that hasn't been tried.
- **The menu (`aipet-ui/src/menu.rs`):** centre each item's content in its 32 px row, for example with a
  `container(...).height(Fill).align_y(Center)` inside the button. This is for whichever task next touches the menu
  (17 or 27).
- **Exact line heights on Windows (optional):** a per-platform `style::LINE_HEIGHT` of 1.330 em for Segoe UI, with
  `view.rs`'s `TITLE_Y` and `DETAIL_Y` worked out from it instead of the hard-coded 1.362.
- **Known from SPIKE.md and seen again:** the start position uses the whole monitor, so the pet starts over the
  taskbar.

## Limits

- One machine, and only the Intel GPU ever presented; the NVIDIA GPU never drew the pet. No AMD GPU and no Windows 10
  were tried.
- The CPU figures are the spike running its demo script, measured on Vulkan only, not the benchmark R18 asks for.
- The lost-release fix is covered by a unit test and by reading the code, the moved probe only by reading the code, and
  neither has been re-run by hand.
- Not shown by hand:
  - the drag onto the other monitor at 100 %: every drop in run `01` had the pointer on the main monitor;
  - the menu at 150 %: in run `05` it was opened only after the window's last rescale, to 1 at 111.120 s;
  - click-through in the `dx12`, `vulkan` and `gl` runs: no click was reported for them;
  - what ticking "Always on top" again did.
- Only the side-by-side layout of the two monitors was tried, not one above the other.
- The screenshot of the pet after a drag onto the 100 % monitor was not taken.

## Re-running it

From the repository root, in Git Bash (no PowerShell), after `cd rust && cargo build --release -p aipet-spike`:

```bash
SPIKE=rust/target/release/aipet-spike.exe
AIPET_DEBUG=1 timeout -k 5 420 "$SPIKE" 2> default.log            # the default backend (Vulkan here)
AIPET_DEBUG=1 WGPU_BACKEND=dx12 timeout -k 5 180 "$SPIKE" 2> dx12.log
AIPET_DEBUG=1 WGPU_BACKEND=vulkan timeout -k 5 180 "$SPIKE" 2> vulkan.log
AIPET_DEBUG=1 WGPU_BACKEND=gl timeout -k 5 180 "$SPIKE" 2> gl.log
grep GPU default.log                                               # adapters, alpha modes, format
```

Every run ends through `timeout` or the pet's menu (right-click the sprite, then Quit).

For the CPU runs, start the pet and the sampler together:

```bash
timeout -k 5 140 "$SPIKE" 2> perf.log & dotnet sampler/bin/aipet-sampler.dll aipet-spike 60 pet 60; wait
```

The sampler is a small C# console program, built with `dotnet build -c Release -p:UseAppHost=false -o bin` (a
`net10.0` exe project with this file as `Program.cs`). It is run through `dotnet`, so no new exe is made for the
antivirus to catch.

```csharp
// usage: dotnet aipet-sampler.dll [process name] [seconds] [label] [settle seconds]
// CPU is a share of one core, from the process's total processor time; memory is the working set and private bytes
using System.Diagnostics;
using System.Globalization;

var inv = CultureInfo.InvariantCulture;
var name = args.Length > 0 ? args[0] : "aipet-spike";
var seconds = args.Length > 1 ? int.Parse(args[1], inv) : 30;
var label = args.Length > 2 ? args[2] : "";
var settle = args.Length > 3 ? int.Parse(args[3], inv) : 0;
var found = Process.GetProcessesByName(name);
for (var waited = 0; found.Length == 0 && waited < 30; waited++)
{
    Thread.Sleep(1000);
    found = Process.GetProcessesByName(name);
}
if (found.Length != 1)
{
    Console.Error.WriteLine($"sampler: {found.Length} processes named {name}; exactly one must run");
    return 2;
}
var p = found[0];
Thread.Sleep(settle * 1000);
var clock = Stopwatch.StartNew();
p.Refresh();
TimeSpan startCpu = p.TotalProcessorTime, lastCpu = startCpu, startAt = clock.Elapsed, lastAt = startAt;
var cpu = new List<double>();
var ws = new List<double>();
var priv = new List<double>();
for (var i = 1; i <= seconds; i++)
{
    Thread.Sleep(1000);
    p.Refresh();
    if (p.HasExited) break;
    TimeSpan c = p.TotalProcessorTime, at = clock.Elapsed;
    cpu.Add((c - lastCpu).TotalMilliseconds / (at - lastAt).TotalMilliseconds * 100);
    (lastCpu, lastAt) = (c, at);
    ws.Add(p.WorkingSet64 / 1048576.0);
    priv.Add(p.PrivateMemorySize64 / 1048576.0);
    Console.WriteLine(string.Format(inv, "{0,3} s  cpu {1,5:F1} %  working set {2,6:F1} MB  private {3,6:F1} MB",
        i, cpu[^1], ws[^1], priv[^1]));
}
if (cpu.Count == 0) return 1;
static double Median(List<double> xs)
{
    var s = xs.Order().ToList();
    return s.Count % 2 == 1 ? s[s.Count / 2] : (s[s.Count / 2 - 1] + s[s.Count / 2]) / 2;
}
var mean = (lastCpu - startCpu).TotalMilliseconds / (lastAt - startAt).TotalMilliseconds * 100;
Console.WriteLine(string.Format(inv,
    "SUMMARY {0} {1}: {2} s after {3} s settling; cpu mean {4:F1} %, median {5:F1} %, min {6:F1} %, max {7:F1} %; " +
    "working set median {8:F1} MB, max {9:F1} MB; private median {10:F1} MB; {11} logical cores",
    label, name, cpu.Count, settle, mean, Median(cpu), cpu.Min(), cpu.Max(), Median(ws), ws.Max(), Median(priv),
    Environment.ProcessorCount));
return 0;
```
