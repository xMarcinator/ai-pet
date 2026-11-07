# Native click-through for the plain-iced (winit) shell

Scope: how the pet window on the winit shell (Windows, macOS, X11) gets exact clickable areas, stays on top, and
stays out of the taskbar and Alt+Tab. The Wayland layer-shell shell (iced_exwlshell) is not covered here; it sets a
wl_surface input region itself.

Registry paths below are relative to `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`. C# paths are
relative to `src/` of the repo.

Verified code lives in `rust/notes/prototypes/native-spike/`:
- `src/native.rs`: the module, copied in full at the end of this file.
- `src/main.rs`: a runtime harness. It opens an iced window that is never mapped, runs the native calls through
  `iced::window::run`, and reads the result back from the X server.
- `src/bin/integration.rs`: a compile-checked sketch of how the app would drive the module.

## TL;DR

| Platform | Clickable area | Keep on top | Out of taskbar / Alt+Tab |
|---|---|---|---|
| X11 (real X11, GNOME XWayland) | SHAPE `ShapeInput` rectangles (plus `ShapeBounding`), sent over our own `x11rb::connect(None)` RustConnection | `_NET_WM_STATE_ABOVE`: set as a property before the window is mapped, then a client message every 2 s | `_NET_WM_STATE_SKIP_TASKBAR` and `_SKIP_PAGER`, plus `_NET_WM_WINDOW_TYPE_UTILITY` |
| Windows | `SetWindowRgn` with `CreateRectRgn` + `CombineRgn(RGN_OR)`, as in the C# | `SetWindowPos(HWND_TOPMOST, NOMOVE\|NOSIZE\|NOACTIVATE)` every 2 s, as in the C# | `WS_EX_TOOLWINDOW`, kept through a `SetWindowSubclass` hook (winit rewrites GWL_EXSTYLE), plus iced's `skip_taskbar` |
| macOS | No region API. Every ~33 ms, test `mouseLocationOutsideOfEventStream` against the rectangles and toggle `setIgnoresMouseEvents` | `NSFloatingWindowLevel` (what iced's `Level::AlwaysOnTop` already sets) | Accessory activation policy; collection behaviour CanJoinAllSpaces\|Stationary\|IgnoresCycle\|FullScreenAuxiliary |

The table uses no new crates. `x11rb 0.13.2`, `windows-sys 0.52.0`, `objc2 0.5.2`, `objc2-app-kit 0.2.2`,
`objc2-foundation 0.2.2` and `raw-window-handle 0.6.2` are all already in iced 0.14's dependency tree, with the needed
features already enabled. This was verified: `cargo tree --target all` of iced alone and of iced plus these deps list
the same 294 packages.

## 0. What the C# version does (the behaviour to port)

- `AiPet.Core/Platform.cs:27-37`: `IPlatform` has `FitsPetWindow`, `MoveResize`, `SetupPetWindow`, `KeepOnTop`, and
  `SetInputRegion(handle, rects)`. The rects are "physical pixels, window-relative".
- `AiPet.UI/MainWindow.axaml.cs:776-805` `UpdateInputRegion`:
  - It collects the Sprite padded by 6, the ReviewsHeader by 4, each visible card body by 16 and its close button by 2,
    overlay popups by 2, and the open ContextMenu by 2 (`:792-800`).
  - It converts DIPs to pixels with floor for the top-left corner and ceil for the bottom-right one (`:788-789`).
  - It skips the call when the joined rect string equals `lastRegion` (`:801-803`).
  - Callers: `Opened` (force), `Resized` (force, posted at Render priority) (`:98-106`), `menu.Opened` (`:244`), and
    `OnFrame` at most every 0.1 s while no resize is pending (`:686-688`).
- `MainWindow.axaml.cs:703-737` `FitWindow` is Linux-only (`FitsPetWindow`). The window shrinks to what it shows,
  because some compositors ignore the input shape. While resizing, the whole window takes input (`:731`).
- `MainWindow.axaml.cs:137-138,203-206`: `KeepOnTop` runs every 2 s, only while `Topmost`.
- Windows, `AiPet.UI/Platform/WindowsPlatform.cs`:
  - `SetupPetWindow` ORs `WS_EX_TOOLWINDOW` (0x80) into GWL_EXSTYLE (`:128-131`).
  - `KeepOnTop` is `SetWindowPos(HWND_TOPMOST, 0x13)` (`:133`).
  - `SetInputRegion` builds `CreateRectRgn(0,0,0,0)`, ORs each rect in with `CombineRgn(...,2)`, then calls
    `SetWindowRgn(h, rgn, true)`. On failure it deletes the region (`:135-147`).
  - It uses no WS_EX_LAYERED, WS_EX_TRANSPARENT or WM_NCHITTEST. The region clips both hit-testing and drawing, which
    is why the rects are padded to include the shadows.
- Linux, `AiPet.UI/Platform/LinuxPlatform.cs`, through libX11/libXext P/Invoke:
  - `SetupPetWindow` (`:76-99`) sets `_COMPTON_SHADOW=0`. When fitted, it also sets `SouthGravity` bit gravity on the
    window and its child.
  - `KeepOnTop` is a no-op (`:100`), because Avalonia's `Topmost` handles it.
  - `SetInputRegion` (`:142-166`) calls `XShapeCombineRectangles(ShapeInput, ShapeSet, Unsorted)`. It also calls
    `ShapeBounding` with the same rects when not fitted; when fitted, it clears the bounding shape instead (`:159-161`).
  - `AIPET_NOSHAPE=1` skips all of this.
- macOS, `AiPet.UI/Platform/MacPlatform.cs:6-8,24-26`: all stubs. The plan written there is "NSWindow.ignoresMouseEvents
  toggling for click-through".
- `MainWindow.axaml:4-6`: 380x600 DIPs, `WindowDecorations=None`, `Transparent`, `Topmost=True`,
  `ShowInTaskbar=False`, `CanResize=False`.

## How iced 0.14 hands out the native handle

- `iced_runtime-0.14.0/src/window.rs:463-478`: `window::run(id, f: FnOnce(&dyn Window) -> T)`.
  - `Window` is `HasWindowHandle + HasDisplayHandle` (`:193-198`).
  - `raw_window_handle` is re-exported at `:13`, so `iced::window::raw_window_handle` works.
- `iced_winit-0.14.1/src/lib.rs:1633-1636` runs the closure synchronously on the event-loop thread, with the iced_winit
  `Window`. That type implements both handle traits (`iced_winit-0.14.1/src/window.rs:307-335`). This thread is the
  main thread on macOS and the thread that owns the HWND on Windows, which is exactly what AppKit and
  `SetWindowSubclass` require.
- `iced_winit-0.14.1/src/lib.rs:327`: every window is created with `with_visible(false)`. It is mapped only after
  insertion, if `settings.visible` is set (`:697-698`), and nothing can hook in between. So open the pet with
  `visible: false`, run `setup_pet_window`, then show it with `window::set_mode(id, Mode::Windowed)`
  (`lib.rs:1563-1565` calls winit `set_visible`).
- `window::scale_factor(id)` returns winit's scale factor (`iced_runtime .../window.rs:376`,
  `iced_winit .../lib.rs:1546-1551`). Rects in iced logical units times that factor give physical pixels, as long as
  the app does not override `Program::scale_factor`. On this XWayland it was 1.5: the 380x600 window was 570x900 px.
- Platform settings iced passes to winit (`iced_winit-0.14.1/src/conversion.rs`):
  - Windows: `skip_taskbar` (`:99-103`; field at `iced_core-0.14.0/src/window/settings/windows.rs:9-10`).
  - Linux: `application_id` becomes WM_CLASS and app_id (`:139-162`). The default is `""`, which gave the harness
    window an empty WM_CLASS in xprop, so set it to `"aipet"`.
  - Linux: `override_redirect` (`iced_core .../settings/linux.rs:16`).
  - `level` maps to winit `WindowLevel` (`conversion.rs:55,362-370`).
  - iced exposes no X11 window type and no macOS activation policy, so the native module sets those.

## 1. X11

### Getting the window id

- winit 0.30.13 hands out an **Xlib** handle: `XlibWindowHandle::new(self.xlib_window())`
  (`winit-0.30.13/src/platform_impl/linux/x11/window.rs:1883-1887`).
- `XlibWindowHandle.window` is a `c_ulong` XID (`raw-window-handle-0.6.2/src/unix.rs:48-52`), and
  `XcbWindowHandle.window` is a `NonZeroU32` (`unix.rs:122-126`). The module accepts both and converts to
  `x11rb::protocol::xproto::Window` (u32).
- The display handle (`XlibDisplayHandle { display, screen }`, `unix.rs:8-20`) is not needed.

### Using a separate connection

X resource ids are server-global. Any client can send `ChangeProperty`, `ShapeRectangles` or
`ChangeWindowAttributes` for another client's window; this is how WMs, xprop and xdotool work. So
`x11rb::connect(None)` (`x11rb-0.13.2/src/lib.rs:191`) opens a pure-Rust `RustConnection` on `$DISPLAY`. winit opened
the same display with `XOpenDisplay(NULL)`.

**Verified at runtime** on this machine's XWayland (`DISPLAY=:1`, `WAYLAND_DISPLAY` unset so winit uses X11), with an
iced window that is never mapped:

```
setup: Ok(())
set_input_region: Ok(())
keep_on_top: Ok(())
update_hit_test: Ok(true)
window 0xa00003 map_state=UNMAPPED bit_gravity=SOUTH
  input=["415x120+30+355", "179x166+148+587"]
  bounding=["415x120+30+355", "179x166+148+587"]
  _NET_WM_STATE=["_NET_WM_STATE_ABOVE", "_NET_WM_STATE_SKIP_TASKBAR", "_NET_WM_STATE_SKIP_PAGER"]
  _NET_WM_WINDOW_TYPE=["_NET_WM_WINDOW_TYPE_UTILITY", "_NET_WM_WINDOW_TYPE_NORMAL"]
clear_input_region: Ok(())
after clear: ... input=["570x900+0+0"] bounding=["570x900+0+0"]
```

- The `_NET_WM_WINDOW_TYPE` change is sent with `.check()`, a round trip that returned no BadWindow. This proves the
  winit window id is valid on our connection.
- A third client, `xprop -id 0xa00003`, saw the same `_NET_WM_STATE`, `_NET_WM_WINDOW_TYPE` and `_COMPTON_SHADOW = 0`.
- The rectangle math matches the C#. For example, a sprite at (125,476) sized 130x120, padded 6, at scale 1.25 becomes
  179x166+148+587.

Ordering: requests on two connections are not ordered relative to each other. Each call flushes, so in practice they
arrive in the order made. The window always exists by the time `window::run` gets it.

On the hot path, errors are dropped with `VoidCookie::ignore_error()` rather than left to queue up. Our connection never
reads events, so unread errors would pile up, for example a BadWindow after the window closes.

### Input shape and bounding shape (x11rb 0.13.2, feature `shape`, `x11rb-0.13.2/Cargo.toml:118`)

- `conn.shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, win, 0, 0, &rects)`
  (`x11rb-0.13.2/src/protocol/shape.rs:182`).
- For the bounding shape, do the same with `SK::BOUNDING`. This is optional (`clip_drawing`, on by default, as in the
  C# when the window is not fitted). With it, compositors draw no shadow box around the whole window, and a
  non-compositing X server shows a black box only inside the padded rects.
- To remove a shape: `conn.shape_mask(SO::SET, SK::INPUT | SK::BOUNDING, win, 0, 0, x11rb::NONE)` (`shape.rs:186`).
- To read it back: `shape_get_rectangles` (`shape.rs:212`).
- `x11rb::Rectangle` has i16 x/y and u16 w/h, so values are clamped like the C# `Math.Clamp`.

### Keep on top and skip taskbar/pager

EWMH says a withdrawn window writes `_NET_WM_STATE` itself, and a mapped one sends a client message to the root window
(data `[1=ADD, atom1, atom2, 1=application, 0]`, mask SubstructureRedirect|SubstructureNotify).

winit only ever sends the message: `set_netwm` (`x11/window.rs:657-671`), `set_window_level_inner` (`:1073-1076`). It
sends it at creation (`:554`), while iced still has the window unmapped. A WM that drops messages for unmanaged
windows therefore never sees winit's "above", and winit does not repeat it on `set_visible` (`:1123-1155`). Hence:
- `setup_pet_window` checks `get_window_attributes().map_state`. If the window is unmapped, it merges the atoms into the
  property; if mapped, it sends the client message.
- After showing the window, the app calls `window::set_level(id, AlwaysOnTop)` again. winit then sends the message to a
  mapped window.
- `keep_on_top` repeats the ADD ABOVE message every 2 s. It is idempotent and only runs while "Always on top" is on,
  like the C#.
- `ClientMessageEvent::new` is at `x11rb-protocol-0.13.2/src/protocol/xproto.rs:6177`.
- `change_property32` is at `x11rb-0.13.2/src/wrapper.rs:62`.
- `atom_manager!` is at `x11rb-0.13.2/src/x11_utils.rs:86`.

### Window type

winit writes `_NET_WM_WINDOW_TYPE` at creation from `x11_window_types`, default `[NORMAL]` (`x11/window.rs:419`, `:617-629`).
iced gives no way to change it. `setup_pet_window` replaces it with `[_UTILITY, _NORMAL]` before mapping, because WMs
read it at MapRequest. Tiling WMs float utility windows, and most WMs keep them off the taskbar. `DOCK` is an
alternative if some WM refuses to keep a utility window above.

### Bit gravity

`change_window_attributes(win, &ChangeWindowAttributesAux::new().bit_gravity(Gravity::SOUTH))` sets it. winit
creates a single top-level window with no child (unlike Avalonia), so one call is enough. This is only needed if the Rust
port resizes the window around the pet's bottom-centre (the C# `FitWindow` path). Where input shapes work, the window
can keep its full size, so it is off by default.

### Caveats

- Hyprland's XWayland ignores X11 input shapes (`LinuxPlatform.cs:102-104`). On Hyprland, use the layer-shell shell.
  On GNOME (Mutter honours XWayland input shapes, but has no layer-shell), run the winit shell on X11. Either remove
  `WAYLAND_DISPLAY` before iced starts (`unsafe { std::env::remove_var }` on edition 2024, before any thread), or build
  iced without its `wayland` feature, which gates winit's Wayland arm (`winit-0.30.13/src/platform_impl/linux/mod.rs:735-750`).
  If the exwlshell shell shares the binary, add `iced_renderer/wayland` directly instead.
- Under a reparenting WM, the client window sits inside a frame window. The WM must copy the client's ShapeInput onto
  the frame, or the frame catches the clicks. Current KWin, Mutter, xfwm4 and Openbox are believed to do this; it was
  not tested here. The C# X11 path has the same dependency. The fallback is `platform_specific.override_redirect = true`: no WM,
  so no frame, no taskbar and the shape applies directly. But the app must then raise the window itself and move it
  itself; `window::drag` (`_NET_WM_MOVERESIZE`) does not work without a WM.
- Do **not** use iced's `window::enable_mouse_passthrough` / `disable_mouse_passthrough` on X11. They are winit
  `set_cursor_hittest` (`iced_winit .../lib.rs:1655-1663`), which overwrites the input shape with nothing or the full
  window (`x11/window.rs:1623-1658`). Once it has been set to `true`, winit re-applies a full-window input shape on
  every ConfigureNotify (`x11/event_processor.rs:754-781`) and every resize (`x11/window.rs:1277-1280`).

## 2. Windows

### What winit does to the window (winit 0.30.13 / windows-sys 0.52)

- For an undecorated window, `WM_NCCALCSIZE` returns 0 with the rect unchanged
  (`platform_impl/windows/event_loop.rs:1162-1198`). So the client rect equals the window rect, and SetWindowRgn
  coordinates, which are window-relative, equal client-area physical pixels. The C# rects map across 1:1.
- winit rebuilds **the whole GWL_EXSTYLE** from its own `WindowFlags` whenever any flag changes: `apply_diff` calls
  `SetWindowLongW(GWL_EXSTYLE, ...)` (`window_state.rs:398-408`). Flags change on set_window_level, set_visible,
  set_cursor_hittest, maximise and so on.
  - `to_window_styles` never includes `WS_EX_TOOLWINDOW`, and adds `WS_EX_APPWINDOW` whenever `ON_TASKBAR` is set
    (`window_state.rs:256-314`, `:273-275`).
  - `ON_TASKBAR` is always set for unowned windows (`window.rs:1336-1340`).
  - So the C#'s one-shot `SetWindowLong(WS_EX_TOOLWINDOW)` would be undone the next time iced changes the level or
    visibility.
  - Fix: `SetWindowSubclass(hwnd, keep_tool_window, ...)`. On every `WM_STYLECHANGING` for `GWL_EXSTYLE`, it forces
    `styleNew = (styleNew | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW`, and it removes itself on `WM_NCDESTROY`.
  - Everything used is in windows-sys 0.52 `Win32_UI_Shell` / `Win32_UI_WindowsAndMessaging`:
    `SetWindowSubclass` `Shell/mod.rs:954`, `DefSubclassProc` `:56`, `RemoveWindowSubclass` `:521`, `SUBCLASSPROC`
    `:8643`, `WM_STYLECHANGING` `WindowsAndMessaging/mod.rs:2900`, `STYLESTRUCT` `:4481`, `WS_EX_TOOLWINDOW` `:2969`,
    `WS_EX_APPWINDOW` `:2947`.
- iced's `platform_specific.skip_taskbar` uses ITaskbarList DeleteTab (`winit .../windows/window.rs:985-988,1498-1523`).
  That hides the taskbar button but not the Alt+Tab entry, so set it as well as WS_EX_TOOLWINDOW.
- winit's `set_cursor_hittest(false)` sets `WS_EX_TRANSPARENT | WS_EX_LAYERED` (`window.rs:632-642`,
  `window_state.rs:300-302`). That makes the whole window click-through; there is no region.
- `Level::AlwaysOnTop` makes winit call `SetWindowPos(HWND_TOPMOST, ... SWP_ASYNCWINDOWPOS|NOMOVE|NOSIZE|NOACTIVATE)`
  (`window_state.rs:339-357`).
- For transparency, winit calls `DwmEnableBlurBehindWindow` with an empty region (`window.rs:1231-1244`).

### Simplest faithful equivalent (what the module does)

- `set_input_region`: `CreateRectRgn(0,0,0,0)`, then `CreateRectRgn` + `CombineRgn(RGN_OR)` + `DeleteObject` for each
  rect, then `SetWindowRgn(hwnd, rgn, TRUE)`. On failure it calls `DeleteObject(rgn)`, since on success the system owns
  the region. Cites: `Gdi/mod.rs:102,43,121,607`, `RGN_OR` `:2158`.
- `clear_input_region`: `SetWindowRgn(hwnd, 0, TRUE)`.
- `keep_on_top`: `SetWindowPos(hwnd, HWND_TOPMOST, 0,0,0,0, SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE)`
  (`WindowsAndMessaging/mod.rs:686,1429`).
- It does not use WS_EX_NOACTIVATE. `WindowsPlatform.FocusAgent`/`Bring` relies on the pet being the foreground app
  after a click (`WindowsPlatform.cs:61-62`).
- It does not use WM_NCHITTEST/HTTRANSPARENT, which only passes clicks to windows of the same thread, never to other
  apps.
- It does not use UpdateLayeredWindow per-pixel hit-testing, which would need GDI rendering and does not work with
  wgpu or softbuffer swapchains.
- HWND comes from `Win32WindowHandle.hwnd: NonZeroIsize` (`raw-window-handle-0.6.2/src/windows.rs:52-56`). In
  windows-sys 0.52, `HWND = isize` (`Foundation/mod.rs:10246`), so `h.hwnd.get()` is the HWND. In windows-sys ≥0.59,
  HWND is `*mut c_void`.

### Which crate

Use **`windows-sys = "0.52"`**. winit 0.30.13 always depends on it, and its feature list already includes
`Win32_Foundation`, `Win32_Graphics_Gdi`, `Win32_UI_Shell` and `Win32_UI_WindowsAndMessaging`
(`winit-0.30.13/Cargo.toml:525`).
- The tree also has windows-sys 0.61.2 (from softbuffer, `softbuffer-0.4.8/Cargo.toml:229`, only with `tiny-skia`) and
  `windows 0.58` (from wgpu-hal/gpu-allocator). windows-sys 0.52 is the one that is always present.
- The `cargo tree --target x86_64-pc-windows-msvc -i windows-sys@0.52.0` output shows only winit.

## 3. macOS

`MacPlatform.cs` is all stubs; its plan was ignoresMouseEvents toggling. winit's `set_cursor_hittest` is exactly
`window.setIgnoresMouseEvents(!hittest)` (`winit-0.30.13/src/platform_impl/macos/window_delegate.rs:1187-1190`). macOS
has no shape or region API, so toggling it from a poll is the approach:

- Every ~33 ms (from the app's frame subscription, see the pitfalls), `update_hit_test`:
  - reads `NSWindow::mouseLocationOutsideOfEventStream()`
    (`objc2-app-kit-0.2.2/src/generated/NSWindow.rs:1499`). This gives the pointer in window coordinates (points,
    origin bottom-left) even while the window ignores the mouse.
  - flips y with the content view height (`NSWindow.rs:572`, `NSView.rs:301`) and scales by `backingScaleFactor()`
    (`NSWindow.rs:919`) into the same physical top-left space as the rects.
  - calls `setIgnoresMouseEvents(!inside)` (`NSWindow.rs:1493,1496`) only on change. It never starts ignoring while
    `NSEvent::pressedMouseButtons() != 0` (`NSEvent.rs:865`), so a drag that leaves the region keeps going.
- winit's AppKit handle is the NSView (`window_delegate.rs:1653-1659`, `raw-window-handle .../appkit.rs:88-90`). The
  module gets the NSWindow from `NSView::window()` (`NSView.rs:168`) after checking `MainThreadMarker::new()`.
- `setup_pet_window` on macOS:
  - `setHasShadow(false)` (`NSWindow.rs:988`).
  - `setCollectionBehavior(CanJoinAllSpaces|Stationary|IgnoresCycle|FullScreenAuxiliary)` (`NSWindow.rs:88-124,1042`).
  - `setLevel(NSFloatingWindowLevel)`. This is the same as winit's AlwaysOnTop (`window_delegate.rs:1531-1538`,
    `NSWindow.rs:202`).
  - Optionally `NSApplication::sharedApplication(mtm).setActivationPolicy(Accessory)` (`NSApplication.rs:329,494`),
    which removes the Dock icon and Cmd+Tab entry, the equivalent of `ShowInTaskbar=False`. It is app-wide, but the
    Settings window still opens.
- Use objc2 0.5.2 / objc2-app-kit 0.2.2 / objc2-foundation 0.2.2, which are winit's versions
  (`winit-0.30.13/Cargo.toml:321,462`).
  - objc2 0.6.4 / objc2-foundation 0.3.2 are also in the tree via softbuffer, but objc2-app-kit exists only as 0.2.2.
    Using 0.3 would compile new crates.
  - `cargo tree --target aarch64-apple-darwin -i objc2-app-kit@0.2.2` lists only winit and clipboard_macos.

Alternative: use iced's own `enable_mouse_passthrough`/`disable_mouse_passthrough` tasks for the toggle on macOS. It is
the same call, and winit keeps no state for it on macOS. You still need a native pointer query while the window
ignores the mouse, because winit then delivers no cursor events. So objc2 is needed either way, and the module calls
`setIgnoresMouseEvents` directly.

## 4. Where winit's `set_cursor_hittest` is enough, and where not

`set_cursor_hittest` (iced: `window::enable_mouse_passthrough` / `disable_mouse_passthrough`) is all or nothing on
every backend:

| Backend | What `set_cursor_hittest(false/true)` does | Enough for the pet? |
|---|---|---|
| X11 | Empty input shape / one full-window rect; `true` is re-applied after every resize (`x11/window.rs:1623-1658`, `event_processor.rs:754-781`) | No. It conflicts with our rect shape, so never call it on X11. |
| Windows | Toggles `WS_EX_TRANSPARENT \| WS_EX_LAYERED` (`window_state.rs:300-302`) | No. It is whole-window only; toggling it from a `GetCursorPos` poll would work but is worse than SetWindowRgn. |
| macOS | `setIgnoresMouseEvents(!hittest)` (`window_delegate.rs:1187-1190`) | Yes as the toggle, but it still needs a native pointer query, since winit delivers no cursor events while ignoring. |
| Wayland (winit xdg_toplevel) | Empty region / `set_input_region(None)` (`wayland/window/mod.rs:584-598`) | No. The layer-shell shell sets real wl_region rects. |

It is enough when the whole window should go click-through, for example a "ghost/hide" mode or while fading out, on
every platform.

## 5. Compiling for Windows and macOS from this Linux box (check only)

- A plain `cargo check --target x86_64-pc-windows-gnu` **fails here**, but not because of a linker. The target's std is
  not installed: `error[E0463]: can't find crate for std ... may not be installed`. `rustup target add` was not run, as
  instructed.
  - `cargo check` never links, so no cross linker is needed.
  - With the target installed, `cargo check --target x86_64-pc-windows-msvc` should need nothing else.
  - The `-gnu` target may additionally need a `dlltool` for raw-dylib crates (see below).
- What worked without installing anything. The stable toolchain has `rust-src`, and std's crates.io deps are downloaded
  once:

```sh
cd rust/notes/prototypes/native-spike
RUSTC_BOOTSTRAP=1 cargo clippy -Zbuild-std=std,panic_abort --target x86_64-pc-windows-msvc --bins   # clean
RUSTC_BOOTSTRAP=1 cargo clippy -Zbuild-std=std,panic_abort --target aarch64-apple-darwin  --bins    # clean
RUSTC_BOOTSTRAP=1 cargo clippy -Zbuild-std=std,panic_abort --target x86_64-apple-darwin   --bins    # clean
RUSTFLAGS="-C dlltool=llvm-dlltool" RUSTC_BOOTSTRAP=1 \
  cargo check -Zbuild-std=std,panic_abort --target x86_64-pc-windows-gnu --bins                     # clean
```

- Without `-C dlltool=llvm-dlltool`, the `-gnu` build-std run fails: `Error calling dlltool 'x86_64-w64-mingw32-dlltool'`
  while compiling std, which uses raw-dylib. `/usr/bin/llvm-dlltool` is installed and works. msvc needs no dlltool.
- A deliberate type error placed inside `mod win32` made the msvc check fail. This proves the Windows code really is
  type-checked; the macOS module showed its own warnings until they were fixed.
- `RUSTC_BOOTSTRAP=1` is a check-only trick. For CI, prefer `rustup target add` plus a plain `cargo check`, or real
  Windows/macOS runners.
- The full iced crate (`tiny-skia` + `x11` + `wayland` + `thread-pool`) plus the module checks clean on all four
  targets. On Linux, `cargo clippy --all-targets` and the 2 unit tests pass.

## 6. Integration notes (see `src/bin/integration.rs`, compile-checked on all targets)

- Open the pet with `visible: false`, `decorations: false`, `transparent: true`, `resizable: false`,
  `level: AlwaysOnTop`. On Linux set `application_id: "aipet"`; on Windows set `skip_taskbar: true`.
- On the open task: `window::run(id, setup_pet_window)`, then `set_mode(Windowed)` + `set_level(AlwaysOnTop)` +
  `scale_factor(id)`.
- Region: in the frame handler, compute padded logical rects, `native::to_physical(&rects, scale)`, and compare with
  the last value (the C#'s `lastRegion`). At most every 100 ms, call
  `window::run(id, move |w| native::set_input_region_px(w, &px))`.
- On `window::Event::Rescaled`, recompute.
- While a size change is in flight, call `clear_input_region` (the C# sets the whole window, `MainWindow.axaml.cs:731`).
- Every 2 s while "Always on top" is on: `window::run(id, native::keep_on_top)`.
- macOS only, every ~33 ms: `window::run(id, native::update_hit_test)`.
- `iced::time::every` does **not exist** with only the `thread-pool` executor
  (`iced_futures-0.14.0/src/backend/native/thread_pool.rs:20-22`, `iced-0.14.0/src/time.rs:4-13`). Either drive these
  from `window::frames()`, which the pet's animation needs anyway, or enable iced's `tokio`/`smol` feature.
- The Settings window is a normal `window::open(Settings::default())` and needs no native calls.

## Risks / blockers

1. **Windows transparency is not verified.**
   - `SetWindowRgn` clips drawing, so outside the rects nothing shows whatever the renderer does. Inside the padded
     rects, it depends on the swapchain having per-pixel alpha.
   - iced_wgpu takes `PostMultiplied`/`PreMultiplied` if the surface offers it, and otherwise falls back to `Auto`
     (`iced_wgpu-0.14.0/src/window/compositor.rs:123-137`). DX12/Vulkan HWND swapchains usually offer only opaque.
   - tiny-skia/softbuffer (GDI BitBlt) plus winit's blur-behind hack is the likely working path. Needs a real Windows
     test.
2. **Windows: the WS_EX_APPWINDOW + WS_EX_TOOLWINDOW precedence** for Alt+Tab is why the subclass also clears
   APPWINDOW. Not testable here.
3. **X11 reparenting WMs** must propagate ShapeInput to their frame (see §1 caveats). This is the same risk as the C#
   today.
4. **macOS polling** means a click within ~33 ms of the pointer entering a region can go to the window below. The
   first click after hovering is fine. Nothing was run on macOS; it is type-checked only.
5. On **Hyprland**, the X11 path is useless: input shapes are ignored. This is known, and the layer-shell shell covers
   it. On **GNOME Wayland**, the winit shell must run on XWayland (see §1). Native xdg_toplevel can neither keep above
   nor position itself.
6. Do not call iced's `enable/disable_mouse_passthrough` anywhere on X11: it breaks the input shape until restart,
   because of winit's re-apply on resize.

## Recommended native click-through module

Cargo.toml additions. None of these adds a crate to iced 0.14's tree:

```toml
[dependencies]
# plus iced itself; iced::window::raw_window_handle is the same crate
raw-window-handle = "0.6"

[target.'cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))'.dependencies]
# already in iced's tree via winit/softbuffer (0.13.2, with "shape" enabled); pure-Rust RustConnection, no libxcb
x11rb = { version = "0.13", features = ["shape"] }

[target.'cfg(windows)'.dependencies]
# the version winit 0.30 uses, so nothing new is compiled
windows-sys = { version = "0.52", features = [
    "Win32_Foundation",
    "Win32_Graphics_Gdi",
    "Win32_UI_Shell",
    "Win32_UI_WindowsAndMessaging",
] }

[target.'cfg(target_os = "macos")'.dependencies]
# the versions winit 0.30 uses, so nothing new is compiled
objc2 = "0.5.2"
objc2-foundation = { version = "0.2.2", features = ["NSGeometry", "NSThread"] }
objc2-app-kit = { version = "0.2.2", features = [
    "NSApplication",
    "NSEvent",
    "NSGraphics",
    "NSResponder",
    "NSRunningApplication",
    "NSView",
    "NSWindow",
] }
```

Drop `src/native.rs` into the winit-shell crate as `mod native;`. It is copied verbatim from `rust/notes/prototypes/native-spike/src/native.rs`: clippy-clean on x86_64-linux, x86_64/aarch64-apple-darwin and x86_64-pc-windows-msvc, check-clean on x86_64-pc-windows-gnu, and its X11 part exercised at runtime.

```rust
//! Native glue for the pet window on the plain-iced (winit) shell: which parts of the window take the mouse,
//! keep-on-top, and hiding it from the taskbar / Alt+Tab / Dock. The Wayland layer-shell shell does not use this
//! (it sets a wl_surface input region itself).
//!
//! Every function takes anything that has a raw window + display handle, so it can be called straight from
//! `iced::window::run(id, |w| native::set_input_region(w, &rects, scale))`. iced runs that closure on the event-loop
//! thread (the main thread on macOS, the window's own thread on Windows), which is what AppKit and
//! SetWindowSubclass require.
//!
//! Rectangles are in iced logical units relative to the window's top-left corner; `scale` is the window's scale
//! factor (physical pixels per logical unit). They are turned into physical pixels the way the C# version does it
//! (floor the top-left, ceil the bottom-right), so padding is the caller's job, as in MainWindow.UpdateInputRegion.
//!
//! Per platform:
//! - X11 (real X11 sessions, GNOME's XWayland): SHAPE extension input shape (and bounding shape), through a separate
//!   x11rb connection. Hyprland's XWayland ignores input shapes; use the layer-shell shell there.
//! - Windows: SetWindowRgn (clips drawing and hit-testing to the rectangles, like WindowsPlatform.SetInputRegion).
//! - macOS: no shape API; the rectangles are remembered and `update_hit_test` (call it ~30 times a second) turns
//!   NSWindow.ignoresMouseEvents on while the pointer is outside them and off while it is inside.
//!
//! Do not mix this with iced's `window::enable_mouse_passthrough` / `disable_mouse_passthrough` (winit's
//! `set_cursor_hittest`): on X11 that overwrites the input shape, and once it has been set to `true` winit re-applies a
//! full-window input shape on every resize.

// API surface: not every item is used on every platform (e.g. PxRect::contains is macOS-only, Os errors never
// happen on macOS), and a binary crate warns about unused pub items.
#![allow(dead_code)]

use std::fmt;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawWindowHandle};

/// A rectangle in logical (iced) units, relative to the window's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// The same rectangle grown by `pad` on every side (the C# pads the sprite by 6, cards by 16, the menu by 2).
    pub fn padded(self, pad: f32) -> Self {
        Self::new(self.x - pad, self.y - pad, self.width + 2.0 * pad, self.height + 2.0 * pad)
    }
}

/// A rectangle in physical pixels, relative to the window's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PxRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl PxRect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x as f64 && y >= self.y as f64 && x < (self.x + self.w) as f64 && y < (self.y + self.h) as f64
    }
}

/// Logical -> physical, as MainWindow.UpdateInputRegion does: floor the top-left corner, ceil the bottom-right one.
/// Empty rectangles are dropped.
pub fn to_physical(rects: &[Rect], scale: f32) -> Vec<PxRect> {
    let s = f64::from(scale);
    rects
        .iter()
        .filter(|r| r.width > 0.0 && r.height > 0.0)
        .map(|r| {
            let x0 = (f64::from(r.x) * s).floor() as i32;
            let y0 = (f64::from(r.y) * s).floor() as i32;
            let x1 = ((f64::from(r.x) + f64::from(r.width)) * s).ceil() as i32;
            let y1 = ((f64::from(r.y) + f64::from(r.height)) * s).ceil() as i32;
            PxRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct PetWindowOptions {
    /// X11: _NET_WM_STATE_ABOVE before the window is mapped. macOS: floating window level. Windows: HWND_TOPMOST.
    /// (Also pass `level: Level::AlwaysOnTop` in iced's window Settings.)
    pub keep_above: bool,
    /// X11: _NET_WM_STATE_SKIP_TASKBAR + _SKIP_PAGER. Windows: WS_EX_TOOLWINDOW, kept through winit's style rewrites
    /// (also set `platform_specific.skip_taskbar`). macOS: the app becomes an "accessory" (no Dock icon or Cmd+Tab
    /// entry; the Settings window still opens).
    pub skip_taskbar: bool,
    /// X11 only: also set the bounding shape, so only the rectangles are drawn at all (no compositor shadow box around
    /// the whole window, and no black box where there is no compositor). Windows always clips drawing (a window
    /// region does both); macOS never does.
    pub clip_drawing: bool,
    /// X11 only: _NET_WM_WINDOW_TYPE_UTILITY (falls back to _NORMAL). Tiling WMs float it; set before mapping.
    pub utility_type: bool,
    /// X11 only: South bit gravity, for a window that is resized around the pet's bottom-centre (the C# FitWindow
    /// path). Not needed while the window keeps its size.
    pub south_gravity: bool,
}

impl Default for PetWindowOptions {
    fn default() -> Self {
        Self { keep_above: true, skip_taskbar: true, clip_drawing: true, utility_type: true, south_gravity: false }
    }
}

#[derive(Debug, Clone)]
pub enum NativeError {
    /// This window system has no such thing (e.g. a native Wayland window, or a server without SHAPE).
    Unsupported(&'static str),
    /// The window/display handle could not be obtained.
    Handle(String),
    /// The OS call failed.
    Os(String),
}

impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
            Self::Handle(e) => write!(f, "no window handle: {e}"),
            Self::Os(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for NativeError {}

fn raw<W: HasWindowHandle + HasDisplayHandle + ?Sized>(w: &W) -> Result<RawWindowHandle, NativeError> {
    // the display handle is not needed by any backend (X11 opens its own connection), but asking for it checks that
    // the window is still alive and on a supported display
    let _ = w.display_handle().map_err(|e| NativeError::Handle(e.to_string()))?;
    Ok(w.window_handle().map_err(|e| NativeError::Handle(e.to_string()))?.as_raw())
}

/// Call once the pet window exists and before it is shown (open it with `visible: false`, run this, then
/// `window::set_mode(id, Mode::Windowed)`): window type, keep-above and skip-taskbar are read by X11 window managers
/// when the window is mapped, and WS_EX_TOOLWINDOW is best set before the first show.
pub fn setup_pet_window<W: HasWindowHandle + HasDisplayHandle + ?Sized>(
    w: &W,
    opts: &PetWindowOptions,
) -> Result<(), NativeError> {
    match raw(w)? {
        #[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
        h @ (RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)) => x11::setup(x11::window_id(&h)?, opts),
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(h) => win32::setup(h.hwnd.get(), opts),
        #[cfg(target_os = "macos")]
        RawWindowHandle::AppKit(h) => mac::setup(h.ns_view, opts),
        _ => Err(NativeError::Unsupported("no native pet-window setup for this window system")),
    }
}

/// Only these rectangles take mouse input; everything else in the window clicks through to what is below.
/// An empty list makes the whole window click-through (and, on Windows or with `clip_drawing`, invisible).
pub fn set_input_region<W: HasWindowHandle + HasDisplayHandle + ?Sized>(
    w: &W,
    rects: &[Rect],
    scale: f32,
) -> Result<(), NativeError> {
    set_input_region_px(w, &to_physical(rects, scale))
}

/// The same with rectangles already in physical pixels (e.g. from `to_physical`, so the caller can skip the call when
/// nothing changed, as MainWindow.UpdateInputRegion does with its `lastRegion` key).
pub fn set_input_region_px<W: HasWindowHandle + HasDisplayHandle + ?Sized>(
    w: &W,
    px: &[PxRect],
) -> Result<(), NativeError> {
    match raw(w)? {
        #[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
        h @ (RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)) => x11::set_region(x11::window_id(&h)?, px),
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(h) => win32::set_region(h.hwnd.get(), px),
        #[cfg(target_os = "macos")]
        RawWindowHandle::AppKit(h) => mac::set_region(h.ns_view, px.to_vec()),
        _ => Err(NativeError::Unsupported("no input region for this window system")),
    }
}

/// The whole window takes input (and is drawn) again, e.g. while it changes size (MainWindow.FitWindow does this
/// until the region for the new layout is in).
pub fn clear_input_region<W: HasWindowHandle + HasDisplayHandle + ?Sized>(w: &W) -> Result<(), NativeError> {
    match raw(w)? {
        #[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
        h @ (RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)) => x11::clear_region(x11::window_id(&h)?),
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(h) => win32::clear_region(h.hwnd.get()),
        #[cfg(target_os = "macos")]
        RawWindowHandle::AppKit(h) => mac::clear_region(h.ns_view),
        _ => Err(NativeError::Unsupported("no input region for this window system")),
    }
}

/// Re-assert always-on-top (the C# calls this every 2 s while "Always on top" is on). Idempotent.
pub fn keep_on_top<W: HasWindowHandle + HasDisplayHandle + ?Sized>(w: &W) -> Result<(), NativeError> {
    match raw(w)? {
        #[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
        h @ (RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)) => x11::keep_on_top(x11::window_id(&h)?),
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(h) => win32::keep_on_top(h.hwnd.get()),
        #[cfg(target_os = "macos")]
        RawWindowHandle::AppKit(h) => mac::keep_on_top(h.ns_view),
        _ => Err(NativeError::Unsupported("no keep-on-top for this window system")),
    }
}

/// macOS: compare the pointer with the region and switch ignoresMouseEvents; returns whether the window takes the
/// mouse now. Call it from a ~30 Hz subscription on macOS only. Elsewhere the OS does the hit-testing: Ok(true).
pub fn update_hit_test<W: HasWindowHandle + HasDisplayHandle + ?Sized>(w: &W) -> Result<bool, NativeError> {
    match raw(w)? {
        #[cfg(target_os = "macos")]
        RawWindowHandle::AppKit(h) => mac::update_hit_test(h.ns_view),
        _ => Ok(true),
    }
}

// ------------------------------------------------------------------------------------------------ X11
#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
mod x11 {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};

    use raw_window_handle::RawWindowHandle;
    use x11rb::connection::{Connection, RequestConnection};
    use x11rb::cookie::VoidCookie;
    use x11rb::protocol::shape::{self, ConnectionExt as _, SK, SO};
    use x11rb::protocol::xproto::{
        Atom, AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ClipOrdering, ConnectionExt as _, EventMask,
        Gravity, MapState, PropMode, Rectangle, Window,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    use super::{NativeError, PetWindowOptions, PxRect};

    x11rb::atom_manager! {
        pub Atoms: AtomsCookie {
            _NET_WM_STATE,
            _NET_WM_STATE_ABOVE,
            _NET_WM_STATE_SKIP_TASKBAR,
            _NET_WM_STATE_SKIP_PAGER,
            _NET_WM_WINDOW_TYPE,
            _NET_WM_WINDOW_TYPE_UTILITY,
            _NET_WM_WINDOW_TYPE_NORMAL,
            _COMPTON_SHADOW,
        }
    }

    struct X {
        conn: RustConnection,
        atoms: Atoms,
        has_shape: bool,
    }

    /// Set by setup(): whether set_region also sets the bounding shape.
    static CLIP_DRAWING: AtomicBool = AtomicBool::new(true);

    /// One connection of our own, opened on first use from $DISPLAY (the display winit opened too). X window ids are
    /// server-wide, so the window winit created can be changed from here like any WM or xprop would.
    fn x() -> Result<&'static X, NativeError> {
        static X: OnceLock<Result<X, String>> = OnceLock::new();
        X.get_or_init(|| {
            let (conn, _screen) = x11rb::connect(None).map_err(|e| format!("X11 connect: {e}"))?;
            let atoms = Atoms::new(&conn)
                .map_err(|e| e.to_string())?
                .reply()
                .map_err(|e| format!("X11 atoms: {e}"))?;
            let has_shape = conn
                .extension_information(shape::X11_EXTENSION_NAME)
                .map_err(|e| e.to_string())?
                .is_some();
            Ok(X { conn, atoms, has_shape })
        })
        .as_ref()
        .map_err(|e| NativeError::Os(e.clone()))
    }

    fn os<E: std::fmt::Display>(e: E) -> NativeError {
        NativeError::Os(format!("X11: {e}"))
    }

    /// Hot-path requests: errors (a BadWindow after the window went) are dropped instead of queueing up on a
    /// connection that never reads events.
    fn fire(cookie: Result<VoidCookie<'_, RustConnection>, x11rb::errors::ConnectionError>) -> Result<(), NativeError> {
        cookie.map_err(os)?.ignore_error();
        Ok(())
    }

    pub fn window_id(h: &RawWindowHandle) -> Result<Window, NativeError> {
        match h {
            // winit 0.30 hands out Xlib handles (its x11/window.rs raw_window_handle_rwh_06)
            RawWindowHandle::Xlib(h) => {
                Window::try_from(h.window).map_err(|_| NativeError::Handle("XID out of range".into()))
            }
            RawWindowHandle::Xcb(h) => Ok(h.window.get()),
            _ => Err(NativeError::Unsupported("not an X11 window")),
        }
    }

    fn rects(px: &[PxRect]) -> Vec<Rectangle> {
        px.iter()
            .map(|r| Rectangle {
                x: r.x.clamp(i16::MIN.into(), i16::MAX.into()) as i16,
                y: r.y.clamp(i16::MIN.into(), i16::MAX.into()) as i16,
                width: r.w.clamp(0, u16::MAX.into()) as u16,
                height: r.h.clamp(0, u16::MAX.into()) as u16,
            })
            .collect()
    }

    pub fn setup(win: Window, opts: &PetWindowOptions) -> Result<(), NativeError> {
        let x = x()?;
        let (c, a) = (&x.conn, &x.atoms);
        CLIP_DRAWING.store(opts.clip_drawing, Ordering::Relaxed);
        if opts.utility_type {
            // replaces winit's [_NORMAL]; read by the WM when the window is mapped
            c.change_property32(
                PropMode::REPLACE,
                win,
                a._NET_WM_WINDOW_TYPE,
                AtomEnum::ATOM,
                &[a._NET_WM_WINDOW_TYPE_UTILITY, a._NET_WM_WINDOW_TYPE_NORMAL],
            )
            .map_err(os)?
            .check()
            .map_err(os)?; // one round trip: proves the window id is valid from this connection
        }
        // compositors that honour it (picom/compton): no drop shadow around the window (LinuxPlatform.SetupPetWindow)
        fire(c.change_property32(PropMode::REPLACE, win, a._COMPTON_SHADOW, AtomEnum::CARDINAL, &[0]))?;
        if opts.south_gravity {
            fire(c.change_window_attributes(win, &ChangeWindowAttributesAux::new().bit_gravity(Gravity::SOUTH)))?;
        }
        let mut states = Vec::new();
        if opts.keep_above {
            states.push(a._NET_WM_STATE_ABOVE);
        }
        if opts.skip_taskbar {
            states.push(a._NET_WM_STATE_SKIP_TASKBAR);
            states.push(a._NET_WM_STATE_SKIP_PAGER);
        }
        add_states(x, win, &states)?;
        c.flush().map_err(os)
    }

    /// EWMH: a withdrawn (unmapped) window sets _NET_WM_STATE itself; a mapped one asks the WM with a client message
    /// to the root window. winit only sends the message, and iced creates every window unmapped, so a WM that drops
    /// messages for unmapped windows would never see winit's "above".
    fn add_states(x: &X, win: Window, states: &[Atom]) -> Result<(), NativeError> {
        if states.is_empty() {
            return Ok(());
        }
        let (c, a) = (&x.conn, &x.atoms);
        let attrs = c.get_window_attributes(win).map_err(os)?.reply().map_err(os)?;
        if attrs.map_state == MapState::UNMAPPED {
            let cur = c
                .get_property(false, win, a._NET_WM_STATE, AtomEnum::ATOM, 0, 64)
                .map_err(os)?
                .reply()
                .map_err(os)?;
            let mut list: Vec<Atom> = cur.value32().map(|v| v.collect()).unwrap_or_default();
            for s in states {
                if !list.contains(s) {
                    list.push(*s);
                }
            }
            c.change_property32(PropMode::REPLACE, win, a._NET_WM_STATE, AtomEnum::ATOM, &list)
                .map_err(os)?
                .check()
                .map_err(os)?;
        } else {
            let root = c.query_tree(win).map_err(os)?.reply().map_err(os)?.root;
            // _NET_WM_STATE_ADD = 1; two properties per message; source indication 1 = normal application
            for pair in states.chunks(2) {
                let data = [1, pair[0], pair.get(1).copied().unwrap_or(0), 1, 0];
                let event = ClientMessageEvent::new(32, win, a._NET_WM_STATE, data);
                fire(c.send_event(
                    false,
                    root,
                    EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                    event,
                ))?;
            }
        }
        Ok(())
    }

    pub fn set_region(win: Window, px: &[PxRect]) -> Result<(), NativeError> {
        let x = x()?;
        if !x.has_shape {
            return Err(NativeError::Unsupported("X server has no SHAPE extension"));
        }
        let r = rects(px);
        if CLIP_DRAWING.load(Ordering::Relaxed) {
            fire(x.conn.shape_rectangles(SO::SET, SK::BOUNDING, ClipOrdering::UNSORTED, win, 0, 0, &r))?;
        }
        fire(x.conn.shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, win, 0, 0, &r))?;
        x.conn.flush().map_err(os)
    }

    pub fn clear_region(win: Window) -> Result<(), NativeError> {
        let x = x()?;
        if !x.has_shape {
            return Err(NativeError::Unsupported("X server has no SHAPE extension"));
        }
        // a None mask removes the shape: the window's own rectangle again
        fire(x.conn.shape_mask(SO::SET, SK::BOUNDING, win, 0, 0, x11rb::NONE))?;
        fire(x.conn.shape_mask(SO::SET, SK::INPUT, win, 0, 0, x11rb::NONE))?;
        x.conn.flush().map_err(os)
    }

    pub fn keep_on_top(win: Window) -> Result<(), NativeError> {
        let x = x()?;
        add_states(x, win, &[x.atoms._NET_WM_STATE_ABOVE])?;
        x.conn.flush().map_err(os)
    }

    /// For tests/diagnostics: the input and bounding rectangles as the server has them, and the window's
    /// _NET_WM_STATE / _NET_WM_WINDOW_TYPE atoms by name.
    pub fn describe(win: Window) -> Result<String, NativeError> {
        let x = x()?;
        let c = &x.conn;
        let input = c.shape_get_rectangles(win, SK::INPUT).map_err(os)?.reply().map_err(os)?;
        let bounding = c.shape_get_rectangles(win, SK::BOUNDING).map_err(os)?.reply().map_err(os)?;
        let names = |prop: Atom| -> Result<Vec<String>, NativeError> {
            let reply = c.get_property(false, win, prop, AtomEnum::ATOM, 0, 64).map_err(os)?.reply().map_err(os)?;
            let atoms: Vec<Atom> = reply.value32().map(|v| v.collect()).unwrap_or_default();
            atoms
                .into_iter()
                .map(|atom| {
                    let n = c.get_atom_name(atom).map_err(os)?.reply().map_err(os)?;
                    Ok(String::from_utf8_lossy(&n.name).into_owned())
                })
                .collect()
        };
        let attrs = c.get_window_attributes(win).map_err(os)?.reply().map_err(os)?;
        let fmt = |r: &[Rectangle]| r.iter().map(|r| format!("{}x{}+{}+{}", r.width, r.height, r.x, r.y)).collect::<Vec<_>>();
        Ok(format!(
            "window 0x{win:x} map_state={:?} bit_gravity={:?}\n  input={:?}\n  bounding={:?}\n  _NET_WM_STATE={:?}\n  _NET_WM_WINDOW_TYPE={:?}",
            attrs.map_state,
            attrs.bit_gravity,
            fmt(&input.rectangles),
            fmt(&bounding.rectangles),
            names(x.atoms._NET_WM_STATE)?,
            names(x.atoms._NET_WM_WINDOW_TYPE)?,
        ))
    }
}

/// X11 diagnostics: the window's input/bounding rectangles and EWMH atoms as the server has them.
#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
pub fn x11_describe(window: u32) -> Result<String, NativeError> {
    x11::describe(window)
}

/// X11: the window id behind a handle (for diagnostics).
#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
pub fn x11_window_id<W: HasWindowHandle + HasDisplayHandle + ?Sized>(w: &W) -> Result<u32, NativeError> {
    x11::window_id(&raw(w)?)
}

// ------------------------------------------------------------------------------------------------ Windows
#[cfg(target_os = "windows")]
mod win32 {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{CombineRgn, CreateRectRgn, DeleteObject, RGN_OR, SetWindowRgn};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongW, HWND_TOPMOST, STYLESTRUCT, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, SWP_NOZORDER, SetWindowLongW, SetWindowPos, WM_NCDESTROY, WM_STYLECHANGING, WS_EX_APPWINDOW,
        WS_EX_TOOLWINDOW,
    };

    use super::{NativeError, PetWindowOptions, PxRect};

    const SUBCLASS_ID: usize = 0x4150_4554; // "APET"

    /// winit rebuilds the whole extended style from its own flags whenever one of them changes (set_window_level,
    /// set_visible, ... in its window_state.rs apply_diff), which would drop WS_EX_TOOLWINDOW and bring back
    /// WS_EX_APPWINDOW. This keeps them the way the pet wants them on every change.
    unsafe extern "system" fn keep_tool_window(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if msg == WM_STYLECHANGING && wparam as i32 == GWL_EXSTYLE {
            // SAFETY: for WM_STYLECHANGING, lparam points to the STYLESTRUCT being applied
            let style = unsafe { &mut *(lparam as *mut STYLESTRUCT) };
            style.styleNew = (style.styleNew | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW;
        }
        if msg == WM_NCDESTROY {
            unsafe { RemoveWindowSubclass(hwnd, Some(keep_tool_window), SUBCLASS_ID) };
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    pub fn setup(hwnd: HWND, opts: &PetWindowOptions) -> Result<(), NativeError> {
        if opts.skip_taskbar {
            // WS_EX_TOOLWINDOW: not in Alt+Tab or the taskbar (WindowsPlatform.SetupPetWindow). Not WS_EX_NOACTIVATE:
            // FocusAgent relies on the pet being the foreground app after a click.
            // SAFETY: hwnd is a live window of this thread (iced runs window::run on the event-loop thread).
            unsafe {
                if SetWindowSubclass(hwnd, Some(keep_tool_window), SUBCLASS_ID, 0) == 0 {
                    return Err(NativeError::Os("SetWindowSubclass failed".into()));
                }
                let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                SetWindowLongW(hwnd, GWL_EXSTYLE, ((ex | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW) as i32);
                SetWindowPos(hwnd, 0, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
            }
        }
        if opts.keep_above {
            keep_on_top(hwnd)?;
        }
        Ok(())
    }

    /// WindowsPlatform.SetInputRegion: the window region is where the pet can be clicked (and drawn).
    pub fn set_region(hwnd: HWND, px: &[PxRect]) -> Result<(), NativeError> {
        // SAFETY: plain GDI/user32 calls on handles we own; on success the system owns the region
        unsafe {
            let rgn = CreateRectRgn(0, 0, 0, 0);
            if rgn == 0 {
                return Err(NativeError::Os("CreateRectRgn failed".into()));
            }
            for r in px {
                let part = CreateRectRgn(r.x, r.y, r.x + r.w, r.y + r.h);
                if part != 0 {
                    CombineRgn(rgn, rgn, part, RGN_OR);
                    DeleteObject(part);
                }
            }
            if SetWindowRgn(hwnd, rgn, 1) == 0 {
                DeleteObject(rgn);
                return Err(NativeError::Os("SetWindowRgn failed".into()));
            }
        }
        Ok(())
    }

    pub fn clear_region(hwnd: HWND) -> Result<(), NativeError> {
        // SAFETY: a NULL region removes the window region
        if unsafe { SetWindowRgn(hwnd, 0, 1) } == 0 {
            return Err(NativeError::Os("SetWindowRgn failed".into()));
        }
        Ok(())
    }

    /// WindowsPlatform.KeepOnTop: SetWindowPos(HWND_TOPMOST, NOSIZE | NOMOVE | NOACTIVATE).
    pub fn keep_on_top(hwnd: HWND) -> Result<(), NativeError> {
        // SAFETY: plain user32 call
        if unsafe { SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) } == 0 {
            return Err(NativeError::Os("SetWindowPos failed".into()));
        }
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------------ macOS
#[cfg(target_os = "macos")]
mod mac {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::ptr::NonNull;

    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSEvent, NSFloatingWindowLevel, NSView, NSWindow,
        NSWindowCollectionBehavior,
    };
    use objc2_foundation::MainThreadMarker;

    use super::{NativeError, PetWindowOptions, PxRect};

    thread_local! {
        /// Physical-pixel rectangles per NSWindow (AppKit is main-thread only, so a thread-local is the right scope).
        static REGIONS: RefCell<HashMap<usize, Vec<PxRect>>> = RefCell::new(HashMap::new());
    }

    fn window(ns_view: NonNull<c_void>) -> Result<(Retained<NSWindow>, MainThreadMarker), NativeError> {
        let mtm = MainThreadMarker::new().ok_or(NativeError::Unsupported("AppKit must be called on the main thread"))?;
        // SAFETY: winit's AppKit handle is its NSView, alive while the window is; we are on the main thread
        let view: &NSView = unsafe { ns_view.cast::<NSView>().as_ref() };
        let window = view.window().ok_or(NativeError::Handle("NSView has no NSWindow".into()))?;
        Ok((window, mtm))
    }

    fn key(w: &NSWindow) -> usize {
        w as *const NSWindow as usize
    }

    pub fn setup(ns_view: NonNull<c_void>, opts: &PetWindowOptions) -> Result<(), NativeError> {
        let (w, mtm) = window(ns_view)?;
        // a transparent borderless window's shadow follows what was drawn, a frame late: none
        w.setHasShadow(false);
        // on every Space and over full-screen apps; not in Cmd+` window cycling
        // SAFETY: plain property setter
        unsafe {
            w.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::IgnoresCycle
                    | NSWindowCollectionBehavior::FullScreenAuxiliary,
            );
        }
        if opts.keep_above {
            w.setLevel(NSFloatingWindowLevel); // what winit's WindowLevel::AlwaysOnTop sets
        }
        if opts.skip_taskbar {
            let _ = NSApplication::sharedApplication(mtm).setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        }
        Ok(())
    }

    pub fn set_region(ns_view: NonNull<c_void>, px: Vec<PxRect>) -> Result<(), NativeError> {
        let (w, _) = window(ns_view)?;
        REGIONS.with(|r| r.borrow_mut().insert(key(&w), px));
        update(&w);
        Ok(())
    }

    pub fn clear_region(ns_view: NonNull<c_void>) -> Result<(), NativeError> {
        let (w, _) = window(ns_view)?;
        REGIONS.with(|r| r.borrow_mut().remove(&key(&w)));
        update(&w);
        Ok(())
    }

    pub fn keep_on_top(ns_view: NonNull<c_void>) -> Result<(), NativeError> {
        let (w, _) = window(ns_view)?;
        w.setLevel(NSFloatingWindowLevel);
        Ok(())
    }

    pub fn update_hit_test(ns_view: NonNull<c_void>) -> Result<bool, NativeError> {
        let (w, _) = window(ns_view)?;
        Ok(update(&w))
    }

    fn update(w: &NSWindow) -> bool {
        // pointer in window coordinates (points, origin bottom-left), even while the window ignores the mouse
        // SAFETY: plain getters on a live window, main thread
        let (p, ignoring, buttons) =
            unsafe { (w.mouseLocationOutsideOfEventStream(), w.ignoresMouseEvents(), NSEvent::pressedMouseButtons()) };
        let height = w.contentView().map(|v| v.frame().size.height).unwrap_or_else(|| w.frame().size.height);
        let scale = w.backingScaleFactor();
        let (x, y) = (p.x * scale, (height - p.y) * scale);
        let inside = REGIONS.with(|r| match r.borrow().get(&key(w)) {
            Some(rects) => rects.iter().any(|r| r.contains(x, y)),
            None => true, // no region: the whole window, as before any was set
        });
        // never start ignoring while a button is held on the window (a drag that left the region)
        let ignore = !inside && (ignoring || buttons == 0);
        if ignore != ignoring {
            w.setIgnoresMouseEvents(ignore);
        }
        !ignore
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_rounds_outwards() {
        // 12.8125..19.0625 -> 12..20, 25.625..31.875 -> 25..32; the empty one is dropped
        let px = to_physical(&[Rect::new(10.25, 20.5, 5.0, 5.0), Rect::new(0.0, 0.0, 0.0, 3.0)], 1.25);
        assert_eq!(px, vec![PxRect { x: 12, y: 25, w: 8, h: 7 }]);
    }

    #[test]
    fn contains_is_half_open() {
        let r = PxRect { x: 0, y: 0, w: 10, h: 10 };
        assert!(r.contains(0.0, 9.9));
        assert!(!r.contains(10.0, 5.0));
    }
}
```
