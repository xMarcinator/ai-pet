# iced 0.14 API notes for the AiPet front-end spike

Source-verified against the crates in the local cargo registry. Every claim cites `crate/src/file.rs:line`.
Everything about 0.14 is read from source. The few "migration" remarks about older iced versions are marked
"(0.13, from memory)" and are only hints for readers of old examples.

## 0. Scope, versions, how this was checked

Resolved versions (the compile-check crate's lockfile at the time, not kept; the same versions as rust/Cargo.lock): iced 0.14.0, iced_core 0.14.0,
iced_runtime 0.14.0, iced_program 0.14.0, iced_futures 0.14.0, iced_graphics 0.14.0, iced_renderer 0.14.0,
iced_widget **0.14.2**, iced_winit **0.14.1**, iced_wgpu 0.14.0, iced_tiny_skia **0.14.1**, winit **0.30.13**,
wgpu 27.0.1, softbuffer 0.4.8, raw-window-handle 0.6.2, x11rb 0.13.2, lilt 0.8.2. iced's MSRV is 1.88
(`iced/Cargo.toml` `rust-version = "1.88"`), so rustc 1.89 is fine.

Path prefixes used below (all under `~/.cargo/registry/src/index.crates.io-*/`):
`iced/` = iced-0.14.0, `iced_winit/` = iced_winit-0.14.1, `iced_runtime/` = iced_runtime-0.14.0,
`iced_core/` = iced_core-0.14.0, `iced_widget/` = iced_widget-0.14.2, `iced_program/` = iced_program-0.14.0,
`iced_futures/` = iced_futures-0.14.0, `iced_graphics/` = iced_graphics-0.14.0, `iced_renderer/` = iced_renderer-0.14.0,
`iced_wgpu/` = iced_wgpu-0.14.0, `iced_tiny_skia/` = iced_tiny_skia-0.14.1, `winit/` = winit-0.30.13,
`rwh/` = raw-window-handle-0.6.2, `softbuffer/` = softbuffer-0.4.8, `lilt/` = lilt-0.8.2.

**Compile-checked snippets.** All Rust snippets marked "(compiled)" come from a scratch crate that passes
`cargo check --offline` with rustc 1.89 on Linux:
`rust/notes/prototypes/iced-check/`
(`src/main.rs` = daemon with pet + settings windows; `src/hit_regions.rs` = custom widget that reports clickable
rectangles; `src/x11.rs` = x11rb input shape and EWMH hints; `src/glow.rs` = CPU shadow/glow bitmaps, with a
passing unit test (`cargo test --offline --bin iced-check glow`); `src/pixel_canvas.rs` = canvas sprite;
`src/sprite_widget.rs` = minimal leaf widget; `src/api_probe.rs` = extra signatures). `cfg(windows)` / `cfg(macos)`
branches were **not** compiled (no cross targets installed) and are marked "(not compiled)". Nothing was run on
screen; runtime behaviour that could only be checked by running is listed in section 11.

Cargo features used by the check crate:

```toml
iced = { version = "=0.14.0", features = ["image-without-codecs", "canvas", "advanced", "tokio"] }
[target.'cfg(target_os = "linux")'.dependencies]
x11rb = { version = "0.13", features = ["shape"] }
```

- `default = ["wgpu","tiny-skia","crisp","web-colors","thread-pool","linux-theme-detection","x11","wayland"]` (`iced/Cargo.toml:64-73`).
- `image-without-codecs` gives `widget::image` without PNG/JPEG decoders (`iced/Cargo.toml:87-90`); enough for `Handle::from_rgba`.
- `canvas` (`iced/Cargo.toml:55`), `advanced` (custom widgets, `iced/Cargo.toml:49-52`), `tokio` (needed for `time::every`, see 7.3).

---

## 1. Program builders: `iced::application` vs `iced::daemon`

### 1.1 Signatures

```rust
// iced/src/application.rs:79-89
pub fn application<State, Message, Theme, Renderer>(
    boot: impl BootFn<State, Message>,
    update: impl UpdateFn<State, Message>,
    view: impl for<'a> ViewFn<'a, State, Message, Theme, Renderer>,   // Fn(&State) -> impl Into<Element>
) -> Application<impl Program<State = State, Message = Message, Theme = Theme>>
where State: 'static, Message: Send + 'static, Theme: theme::Base, Renderer: program::Renderer;

// iced/src/daemon.rs:27-37
pub fn daemon<State, Message, Theme, Renderer>(
    boot: impl application::BootFn<State, Message>,
    update: impl application::UpdateFn<State, Message>,
    view: impl for<'a> daemon::ViewFn<'a, State, Message, Theme, Renderer>, // Fn(&State, window::Id) -> impl Into<Element>
) -> Daemon<impl Program<State = State, Message = Message, Theme = Theme>>
```

- `BootFn` is any `Fn() -> State` or `Fn() -> (State, Task<Message>)` (`iced/src/application.rs:535-566`).
  (0.13, from memory: this argument used to be `new`.)
- `UpdateFn`: `Fn(&mut State, Message) -> C where C: Into<Task<Message>>` (so `()` or `Task`) (`iced/src/application.rs:609-617`).
- `application`'s `ViewFn` has **no window id**: `Fn(&'a State) -> Widget` (`iced/src/application.rs:623-638`);
  its internal `Program::view` ignores the id (`iced/src/application.rs:136-142`).
  `daemon`'s `ViewFn` is `Fn(&'a State, window::Id) -> Widget` (`iced/src/daemon.rs:393-416`).

Builder methods (both return a new builder):

| method | `Application` (`iced/src/application.rs`) | `Daemon` (`iced/src/daemon.rs`) |
|---|---|---|
| `title` | `impl TitleFn<State>` = `&'static str` or `Fn(&State) -> String` (346-360, 574-592) | `&'static str` or `Fn(&State, window::Id) -> String` (190-203, 369-387) |
| `subscription` | `Fn(&State) -> Subscription<Message>` (363-375) | same (206-217) |
| `theme` | `Theme` or `Fn(&State) -> impl Into<Option<Theme>>` (378-392, 648-670) | `Theme` or `Fn(&State, window::Id) -> impl Into<Option<Theme>>` (220-233, 426-448) |
| `style` | `Fn(&State, &Theme) -> theme::Style` (395-407) | same, **no window id** (236-247) |
| `scale_factor` | `Fn(&State) -> f32` (410-424) | `Fn(&State, window::Id) -> f32` (250-261) |
| `settings(Settings)`, `antialiasing`, `default_font`, `font`, `executor::<E>()`, `presets` | 218-248, 427-456 | 157-187, 264-292 |
| window settings | `window(window::Settings)` 253-255, `centered` 258, `exit_on_close_request` 269, `window_size` 280, `transparent` 291, `resizable` 302, `decorations` 313, `position` 324, `level` 335 | **none** — daemons open windows with `window::open` |
| `run(self) -> iced::Result` | 186-215 | 132-154 |

- `Application::window()` returns `Some(settings)` so the runtime opens one window at start
  (`iced/src/application.rs:474-476`, `iced_winit/src/lib.rs:106-116`). `Daemon::window()` returns `None`
  (`iced/src/daemon.rs:310-312`): "will not open a window by default ... will not stop running when all its
  windows are closed" (`iced/src/daemon.rs:19-24`). Exit a daemon with `iced::exit()` (`iced/src/lib.rs:542`,
  `iced_runtime/src/lib.rs:124-126`).
- An `application` exits when its last window is destroyed (`iced_winit/src/lib.rs:1044-1057`, guarded by `!is_daemon`).
- `iced::Settings` (the program-level one): `id`, `fonts`, `default_font`, `default_text_size`, `antialiasing` (default true), `vsync` (default true) (`iced_core/src/settings.rs:8-55`).
- The underlying `Program` trait is public and can be implemented directly instead of using builders
  (`iced_program/src/lib.rs:27-119`); its `style` default is `theme::Base::base(theme)` (108-110).

**Recommendation: use `iced::daemon`** for pet + Settings. The pet window is opened from `boot`, Settings on
demand, and closing Settings never quits the app.

### 1.2 Per-window background: the `style` gotcha

The window clear colour is `theme::Style::background_color` (`iced_core/src/theme.rs:224-230`), computed by
`program.style(theme)` per window (`iced_winit/src/window/state.rs:61, 130-132, 219-220`) and used as the clear
colour at present (`iced_winit/src/lib.rs:982-988`). `style` receives only the theme, not the window id
(`iced/src/daemon.rs:236-247`). `theme` does receive the id. So give the pet window its own named custom theme
and branch on the name in `style` (`Theme::custom` `iced_core/src/theme.rs:91-96`, `Base::name`
`iced_core/src/theme.rs:250-254, 299-325`). (compiled, `src/main.rs`)

```rust
const PET_THEME: &str = "AiPet transparent";

pub fn main() -> iced::Result {
    iced::daemon(State::boot, State::update, State::view)
        .title(State::title)          // fn(&self, window::Id) -> String
        .theme(State::theme)          // fn(&self, window::Id) -> Theme
        .style(State::style)          // fn(&self, &Theme) -> theme::Style
        .subscription(State::subscription)
        .run()
}

fn theme(&self, id: window::Id) -> Theme {
    if id == self.pet { Theme::custom(PET_THEME, Theme::Dark.palette()) } else { Theme::Dark }
}

fn style(&self, theme: &Theme) -> theme::Style {
    let base = theme::Base::base(theme);
    if theme::Base::name(theme) == PET_THEME {
        theme::Style { background_color: Color::TRANSPARENT, ..base }
    } else { base }
}
```

`theme()` and `style()` are re-evaluated for every window on every rebuild (`iced_winit/src/lib.rs:1826-1828` →
`iced_winit/src/window/state.rs:219-220`); `Theme::custom` regenerates the extended palette each call, so in the
real code build the pet `Theme` once, store it in `State`, and return a clone (it is an `Arc`, `iced_core/src/theme.rs:60`).
Env var `ICED_THEME` can force a built-in theme for `Base::default` (`iced_core/src/theme.rs:264-275`).

### 1.3 Opening / closing windows at runtime

```rust
// iced_runtime/src/window.rs
pub fn open(settings: Settings) -> (Id, Task<Id>)          // 272-281: Id is allocated immediately
pub fn close<T>(id: Id) -> Task<T>                          // 284-286
pub fn gain_focus<T>(id: Id) -> Task<T>                     // 431-433
pub fn set_mode<T>(id: Id, mode: Mode) -> Task<T>           // 395-397  (Windowed | Fullscreen | Hidden)
pub fn oldest() / latest() -> Task<Option<Id>>              // 289-296
// subscriptions
pub fn events() -> Subscription<(Id, Event)>                // 216-224
pub fn open_events() -> Subscription<Id>                    // 227-235
pub fn close_events() -> Subscription<Id>                   // 238-246
pub fn close_requests() -> Subscription<Id>                 // 260-268
pub fn resize_events() -> Subscription<(Id, Size)>          // 249-257
```

- The `Task<Id>` from `open` completes when the window exists (`on_open.send(id)`, `iced_winit/src/lib.rs:713`).
  The runtime creates every window with `with_visible(false)` and shows it after the compositor/surface exist if
  `settings.visible` (`iced_winit/src/lib.rs:318-327, 697-699`), then emits `window::Event::Opened { position, size }`
  (`iced_winit/src/lib.rs:701-707`; position is `None` on Wayland, `iced_core/src/window/event.rs:10-20`).
- `Close(id)` drops the window, emits `window::Event::Closed`, and destroys the compositor when no window is left
  (`iced_winit/src/lib.rs:1400-1422`).
- With `exit_on_close_request: true` (default, `iced_core/src/window/settings.rs:100, 121`) a title-bar close
  immediately runs `Close` (`iced_winit/src/lib.rs:1085-1106`); with `false` you get `window::Event::CloseRequested`
  (use `close_requests()`) and must call `window::close` yourself (`iced_core/src/window/settings.rs:92-100`).
  For the pet window use `false` (it has no title bar anyway); for Settings the default is fine.

Pattern (compiled, `src/main.rs`):

```rust
fn boot() -> (Self, Task<Message>) {
    let (pet, open) = window::open(pet_window_settings());
    (State { pet, settings: None, /* ... */ }, open.map(Message::PetOpened))
}
// in update:
Message::OpenSettings => {
    if let Some(id) = self.settings { return window::gain_focus(id); }
    let (id, open) = window::open(settings_window_settings());
    self.settings = Some(id);
    open.map(Message::SettingsOpened)
}
Message::WindowClosed(id) => { if Some(id) == self.settings { self.settings = None; } Task::none() }
// in subscription:  window::close_events().map(Message::WindowClosed)
```

### 1.4 Multi-window cost: every message rebuilds and redraws every window

After any batch of messages, `update` runs, then **all** windows' views are rebuilt (`build_user_interfaces`
iterates every window, `iced_winit/src/lib.rs:1215-1233, 1817-1853`) and **all** windows get `request_redraw()`
(`iced_winit/src/lib.rs:1253-1255`). While the pet animates at 60 Hz and Settings is open, Settings is also rebuilt
and redrawn at 60 Hz. Acceptable for a small form, but keep the Settings view cheap (or cache parts with `lazy`,
feature `lazy`, `iced/Cargo.toml:91`)
and do not treat its redraws as a bug.

---

## 2. `window::Settings`

Struct and defaults (`iced_core/src/window/settings.rs:34-125`):

| field | type | default | winit mapping (`iced_winit/src/conversion.rs:14-166`) |
|---|---|---|---|
| `size` | `Size` (logical) | 1024x768 | `with_inner_size(LogicalSize(size * program scale_factor))` (39-42) |
| `maximized`, `fullscreen` | bool | false | 43-48 |
| `position` | `Position::{Default, Centered, Specific(Point), SpecificWith(fn(Size, Size) -> Point)}` (`iced_core/src/window/position.rs:5-25`) | Default | `position()` (58-62, 375-442); `Specific` is logical desktop coords |
| `min_size`, `max_size` | `Option<Size>` | None | 64-76 |
| `visible` | bool | true | window is always created hidden, then shown (see 1.3) |
| `resizable` | bool | true | 25-27, 49 |
| `closeable`, `minimizable` | bool | true | enabled buttons (23-35) |
| `decorations` | bool | true | 51 |
| `transparent` | bool | false | 52 |
| `blur` | bool | false | 53 (macOS + KDE Wayland only, `winit/src/window.rs:950-958`) |
| `level` | `Level::{Normal, AlwaysOnBottom, AlwaysOnTop}` (`iced_core/src/window/level.rs:7-19`) | Normal | `window_level()` 55, 362-370 |
| `icon` | `Option<Icon>` | None | 54 |
| `platform_specific` | `PlatformSpecific` | see below | 78-163 |
| `exit_on_close_request` | bool | true | see 1.3 |

Pet window settings (compiled, `src/main.rs`):

```rust
fn pet_window_settings() -> window::Settings {
    window::Settings {
        size: Size::new(WIN_W, WIN_H),
        position: window::Position::Specific(Point::new(200.0, 200.0)),
        resizable: false,
        decorations: false,
        transparent: true,
        level: window::Level::AlwaysOnTop,
        exit_on_close_request: false,
        #[cfg(target_os = "linux")]
        platform_specific: window::settings::PlatformSpecific {
            application_id: "aipet".to_owned(),
            override_redirect: false,
        },
        #[cfg(target_os = "windows")]   // (not compiled)
        platform_specific: window::settings::PlatformSpecific {
            skip_taskbar: true,
            undecorated_shadow: false,
            corner_preference: window::settings::platform::CornerPreference::DoNotRound,
            drag_and_drop: false,
        },
        ..window::Settings::default()
    }
}
```

### 2.1 `platform_specific`

- **Linux** (`iced_core/src/window/settings/linux.rs:5-17`): `application_id: String` and `override_redirect: bool`.
  That is all. With the `x11` feature, iced calls `with_override_redirect` and `with_name(app_id, app_id)` (WM_CLASS);
  with `wayland`, `with_name(app_id, app_id)` (xdg app_id) (`iced_winit/src/conversion.rs:139-163`).
  There is **no X11 window-type or skip-taskbar option** in iced, even though winit has `with_x11_window_type`
  (`winit/src/platform/x11.rs:165, 225`); iced builds `WindowAttributes` itself, so it cannot be reached.
  `override_redirect: true` bypasses the WM entirely (no WM stacking/`_NET_WM_STATE_ABOVE`, no focus management,
  no taskbar entry); not recommended for the pet (keyboard and always-on-top become our problem).
- **Windows** (`iced_core/src/window/settings/windows.rs:5-33`): `drag_and_drop` (default true), `skip_taskbar`
  (default false), `undecorated_shadow` (default false), `corner_preference: CornerPreference::{Default, DoNotRound,
  Round, RoundSmall}` (38-59). Applied at `iced_winit/src/conversion.rs:92-123`. Note the Windows `platform` module
  is `pub` (`iced_core/src/window/settings.rs:2-4`) so `window::settings::platform::CornerPreference` is only
  nameable on Windows.
- **macOS** (`iced_core/src/window/settings/macos.rs:5-12`): `title_hidden`, `titlebar_transparent`,
  `fullsize_content_view` (applied `iced_winit/src/conversion.rs:125-137`). No level/dock options here.

### 2.2 "Skip taskbar" per platform

| platform | how |
|---|---|
| Windows | `platform_specific.skip_taskbar = true` → winit `with_skip_taskbar` (`iced_winit/src/conversion.rs:102-103`), implemented with `ITaskbarList` (`winit/src/platform_impl/windows/window.rs:985-988, 1498-1500`). |
| X11 | Not in iced. Set EWMH hints ourselves with x11rb on the XID: `_NET_WM_STATE = [_SKIP_TASKBAR, _SKIP_PAGER, _ABOVE]` and optionally `_NET_WM_WINDOW_TYPE = _UTILITY`. EWMH lets a client write `_NET_WM_STATE` directly only **before** the window is mapped (EWMH spec; not in these sources), so open the pet with `visible: false`, set hints in the `window::run` callback, then `window::set_mode(id, Mode::Windowed)` (maps it: `iced_winit/src/lib.rs:1563-1571`). Snippet below. |
| macOS | Windows never appear in a taskbar; the Dock icon is per-app. winit only honours an activation policy passed at event-loop build time (`winit/src/platform_impl/macos/app_state.rs:118-131`, default `Regular`), and iced builds the event loop itself (`iced_winit/src/lib.rs:79-81`), so use (macOS platform knowledge, not in these sources) `LSUIElement=true` in the bundle's Info.plist, or call `NSApplication.setActivationPolicy(.accessory)` via objc2-app-kit after startup (not compiled). |
| Wayland (winit) | No concept; the compositor decides. With layer-shell (iced_exwlshell) the surface is not a toplevel, so no taskbar entry. |

X11 hints (compiled, `src/x11.rs`):

```rust
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, PropMode};
use x11rb::wrapper::ConnectionExt as _;

pub fn set_pet_hints(conn: &impl Connection, window: u32) -> Result<(), x11rb::errors::ReplyOrIdError> {
    let atom = |n: &[u8]| -> Result<u32, x11rb::errors::ReplyOrIdError> { Ok(conn.intern_atom(false, n)?.reply()?.atom) };
    let states = [atom(b"_NET_WM_STATE_SKIP_TASKBAR")?, atom(b"_NET_WM_STATE_SKIP_PAGER")?, atom(b"_NET_WM_STATE_ABOVE")?];
    conn.change_property32(PropMode::REPLACE, window, atom(b"_NET_WM_STATE")?, AtomEnum::ATOM, &states)?;
    conn.change_property32(PropMode::REPLACE, window, atom(b"_NET_WM_WINDOW_TYPE")?, AtomEnum::ATOM,
                           &[atom(b"_NET_WM_WINDOW_TYPE_UTILITY")?])?;
    conn.flush()?;
    Ok(())
}
```

### 2.3 Level (always on top) per platform

- X11: `AlwaysOnTop` toggles `_NET_WM_STATE_ABOVE` (`winit/src/platform_impl/linux/x11/window.rs:1073-1076`), also at
  creation (`winit/src/platform_impl/linux/x11/window.rs:554`).
- Windows: `HWND_TOPMOST` (`winit/src/platform_impl/windows/window_state.rs:347`).
- macOS: `kCGFloatingWindowLevel` (`winit/src/platform_impl/macos/window_delegate.rs:1531-1538`).
- Wayland (winit xdg toplevel): **no-op** (`winit/src/platform_impl/linux/wayland/window/mod.rs:430`). Layer-shell is required.
- Runtime change: `window::set_level(id, Level)` (`iced_runtime/src/window.rs:436-438`).

### 2.4 Transparency per platform (window side)

- X11: winit picks a 32-bit TrueColor visual when `transparent` (`winit/src/platform_impl/linux/x11/window.rs:241-254`);
  `set_transparent` is creation-time only on X11 (`winit/src/window.rs:942-943`). A compositing manager must be running.
- The renderer must also produce alpha: see section 8 (tiny-skia cannot on Wayland/Windows/macOS).

---

## 3. Native handles

```rust
// iced_runtime/src/window.rs:463-478
pub fn run<T>(id: Id, f: impl FnOnce(&dyn Window) -> T + Send + 'static) -> Task<T>
where T: Send + 'static;

// iced_runtime/src/window.rs:196-198
pub trait Window: HasWindowHandle + HasDisplayHandle {}
impl<T> Window for T where T: HasWindowHandle + HasDisplayHandle {}

// iced_runtime/src/window.rs:449-453  -- note the unused generic
pub fn raw_id<Message>(id: Id) -> Task<u64>;
```

- There is no `run_with_handle` or `on_event` anywhere in iced_runtime/iced_core/iced 0.14 (grep); `window::run` is the handle API. (0.13, from memory: `run_with_handle` existed.) raw-window-handle is re-exported as
  `iced::window::raw_window_handle` (`iced_runtime/src/window.rs:13`, via `iced/src/window.rs:7-8`).
- What `f` receives: the runtime's `Window<P, C>` (`iced_winit/src/lib.rs:1633-1636`), whose `HasWindowHandle` /
  `HasDisplayHandle` forward to the `winit::window::Window` (`iced_winit/src/window.rs:307-335`). The closure is not
  run if the window is already gone (`iced_runtime/src/window.rs:460-462`).
- **Thread:** window actions run inside `run_instance`, a future polled from winit's `ApplicationHandler`
  callbacks (`iced_winit/src/lib.rs:173-259, 266-278`), i.e. on the event-loop (main) thread. So `f` runs on the main
  thread. (`Send` is only required because the closure travels through the action channel.)
  During a redraw, window actions are deferred to the next loop turn (`iced_winit/src/lib.rs:877-883`).
- `raw_id`: calling `window::raw_id(id)` fails with E0282 "cannot infer type of the type parameter `Message`"
  (checked); write `window::raw_id::<Message>(id)`. It returns `winit::WindowId` as `u64`
  (`iced_winit/src/lib.rs:1628-1631`, `winit/src/window.rs:90-94`): the XID on X11
  (`winit/src/platform_impl/linux/mod.rs:149-161`), the HWND on Windows (`winit/src/platform_impl/windows/window.rs:645-647`).
  `window::run` is more explicit; prefer it.

Handle types (`rwh/src/unix.rs:48-52, 122-126, 184-186`, `rwh/src/windows.rs:52-56`, `rwh/src/appkit.rs:88-90`):
`XlibWindowHandle { window: c_ulong, visual_id }`, `XcbWindowHandle { window: NonZeroU32, .. }`,
`WaylandWindowHandle { surface: NonNull<c_void> }`, `Win32WindowHandle { hwnd: NonZeroIsize, hinstance }`,
`AppKitWindowHandle { ns_view: NonNull<c_void> }`. winit's X11 backend returns **Xlib** variants
(`winit/src/platform_impl/linux/x11/window.rs:1883-1902`).

Snippet (compiled, `src/main.rs`; the Win32 arm compiles on Linux because the enum variant exists everywhere):

```rust
#[derive(Debug, Clone, Copy)]
enum NativeHandle { X11 { window: u32 }, Win32 { hwnd: isize }, AppKit, Wayland, Other }

Message::PetOpened(id) => window::run(id, |w| {
    use iced::window::raw_window_handle::RawWindowHandle;
    // supertrait methods are callable on `dyn Window` without importing HasWindowHandle
    match w.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Xlib(h))  => NativeHandle::X11 { window: h.window as u32 },
        Ok(RawWindowHandle::Xcb(h))   => NativeHandle::X11 { window: h.window.get() },
        Ok(RawWindowHandle::Win32(h)) => NativeHandle::Win32 { hwnd: h.hwnd.get() },
        Ok(RawWindowHandle::AppKit(_)) => NativeHandle::AppKit,
        Ok(RawWindowHandle::Wayland(_)) => NativeHandle::Wayland,
        _ => NativeHandle::Other,
    }
}).map(Message::Native),
```

On X11, keep a **separate** x11rb connection (`x11rb::connect(None)`) and use the XID; X window ids are server-global,
so no need to borrow winit's Xlib `Display*`. winit itself uses the same x11rb shape call internally (see 5.1).
On Windows, store `hwnd` as `isize` and rebuild `HWND` when calling Win32 (`windows-sys` is in the registry,
0.52/0.59/0.61). `update`/`view` also run on the main thread: `program.update` is called from `update()` inside
`run_instance` (`iced_winit/src/lib.rs:1301-1314`), which is polled from the winit callbacks. Only `Task` futures and
subscription streams run on the executor's threads. So Win32/AppKit calls made directly in `update` (e.g.
`GetCursorPos`, `SetWindowPos` on the stored HWND) happen on the window's thread; `window::run` is needed only to
obtain the handle.

---

## 4. Moving windows, position, monitor, scale

```rust
// iced_runtime/src/window.rs
pub fn move_to<T>(id: Id, position: Point) -> Task<T>          // 383-385, logical outer position
pub fn drag<T>(id: Id) -> Task<T>                               // 299-301, WM/compositor move while LMB held
pub fn position(id: Id) -> Task<Option<Point>>                  // 369-373, logical outer position
pub fn size(id: Id) -> Task<Size>                               // 338-342, logical inner size
pub fn resize<T>(id: Id, new_size: Size) -> Task<T>             // 309-311
pub fn scale_factor(id: Id) -> Task<f32>                        // 376-380
pub fn monitor_size(id: Id) -> Task<Option<Size>>               // 504-508
// events: window::Event::Moved(Point) (logical), Rescaled(f32), Resized(Size) (iced_core/src/window/event.rs:25-32)
```

Implementation (`iced_winit/src/lib.rs`): `Move` → `set_outer_position(LogicalPosition)` (1553-1561);
`GetPosition` → `outer_position()` converted with the window scale factor, `.ok()` so errors become `None` (1530-1545);
`Drag` → `drag_window()` (1435-1439); `GetScaleFactor` → `winit` scale factor only (not multiplied by the program's
`scale_factor`) (1546-1551); `GetMonitorSize` → `current_monitor().size()` (physical) converted to logical with the
**window's** scale factor (1665-1676) — no monitor position/work area is available. `Moved` events come from
`WindowEvent::Moved` (`iced_winit/src/conversion.rs:346-351`).

Platform support (winit 0.30.13):

| | X11 | Windows | macOS | Wayland (winit toplevel) |
|---|---|---|---|---|
| `move_to` | yes | yes | yes (iced also fixes the macOS inner/outer quirk at creation, `iced_winit/src/lib.rs:340-357`) | **no-op** (`winit/src/platform_impl/linux/wayland/window/mod.rs:273-275`, `winit/src/window.rs:730`) |
| `position` / `Moved` | yes | yes | yes | **None** (`winit/src/platform_impl/linux/wayland/window/mod.rs:263-265`, `winit/src/window.rs:699`) |
| `drag` | `_NET_WM_MOVERESIZE`; ungrabs the pointer (`winit/src/window.rs:1517`, x11 `window.rs:1661-1663`) | `WM_NCLBUTTONDOWN(HTCAPTION)` modal move (`winit/src/platform_impl/windows/window.rs:523-529`) | may swallow the button release (`winit/src/window.rs:1519`) | `xdg_toplevel.move` (`winit/src/platform_impl/linux/wayland/window/state.rs:446-456`) |
| `set_level` | yes | yes | yes | no-op |
| `monitor_size` | yes | yes | yes | yes |

Two drag strategies:

1. **`window::drag(id)` on left press** (compiled, `src/main.rs`): smooth, WM-native, but the app may never see
   the button release (X11 ungrab, macOS), and on X11 the WM may snap/constrain. Refresh position afterwards from
   `window::Event::Moved` (subscribe via `window::events()`, compiled in `src/api_probe.rs`).
2. **App-driven `move_to`** (needed anyway for "pet physics" like falling/walking): on press remember the cursor
   position in window coordinates; on every `CursorMoved`, `move_to(window_pos + cursor - grab)`. Pointer events
   keep arriving outside the window while the button is held: winit calls `SetCapture` on Windows
   (`winit/src/platform_impl/windows/event_loop.rs:1781-1786`), X11 has an implicit grab. Use
   `event::listen_with` for `CursorMoved`/`ButtonReleased` rather than `mouse_area::on_move`, because
   `on_move` only fires while hovered and `on_release` only when the cursor is over the area
   (`iced_widget/src/mouse_area.rs:357-361, 369-371`). Because window-local coordinates change as the window moves,
   expect some jitter; the robust variant reads the **global** pointer (X11 `query_pointer`, compiled in `src/x11.rs`;
   Win32 `GetCursorPos`; macOS `NSEvent.mouseLocation`) and sets `move_to(global - grab)`.

```rust
// compiled, src/main.rs
let cursor = iced::event::listen_with(|event, _status, id| match event {
    Event::Mouse(mouse::Event::CursorMoved { position }) => Some(Message::CursorMoved(id, position)),
    _ => None,
});
// update:
Message::CursorMoved(id, p) => {
    if let (true, Some(drag), Some(pos)) = (id == self.pet, &self.drag, self.window_pos) {
        let next = Point::new(pos.x + p.x - drag.grab.x, pos.y + p.y - drag.grab.y);
        self.window_pos = Some(next);
        return window::move_to(self.pet, next);
    }
    Task::none()
}
```

`listen_with` takes a plain `fn` pointer (`iced_futures/src/event.rs:26-28`; a non-capturing closure coerces, a capturing one does not compile), and drops
`RedrawRequested` events (36-40). `Subscription::map`/`filter_map` closures must be zero-sized (non-capturing);
this is a compile-time `const` panic (`iced_futures/src/subscription.rs:282-290, 341-349, 496-504`). Pass data
with `Subscription::with(value)` (226-229).

---

## 5. Mouse passthrough

```rust
pub fn enable_mouse_passthrough<Message>(id: Id) -> Task<Message>   // iced_runtime/src/window.rs:491-493
pub fn disable_mouse_passthrough<Message>(id: Id) -> Task<Message>  // iced_runtime/src/window.rs:499-501
```

These map to `set_cursor_hittest(false/true)` (`iced_winit/src/lib.rs:1655-1664`). Semantics: **whole window**
on/off; "If `false`, events are passed through the window such that any other window behind it receives them"
(`winit/src/window.rs:1565-1578`). While enabled, the window receives no pointer events at all, so the app cannot
tell by itself when to turn it off again.

Per platform (winit 0.30.13):

- X11: sets the **input shape** to empty (`false`) or one full-window rectangle (`true`) with
  `shape_rectangles(SO::SET, SK::INPUT, ..)` (`winit/src/platform_impl/linux/x11/window.rs:1623-1658`).
- Wayland: `wl_surface.set_input_region(empty)` / `None` (`winit/src/platform_impl/linux/wayland/window/mod.rs:585-597`).
- Windows: toggles `WS_EX_TRANSPARENT | WS_EX_LAYERED` (`winit/src/platform_impl/windows/window.rs:632-642`,
  `winit/src/platform_impl/windows/window_state.rs:300-302`). winit never calls `SetLayeredWindowAttributes`
  (only these two uses of `WS_EX_LAYERED` exist in the Windows backend); whether a wgpu/DXGI-presented window stays
  visible after gaining `WS_EX_LAYERED` must be tested (section 11).
- macOS: `NSWindow.setIgnoresMouseEvents(!hittest)` (`winit/src/platform_impl/macos/window_delegate.rs:1187-1190`).

### 5.1 Exact input regions per target

| target | mechanism | who computes rects |
|---|---|---|
| Wayland layer-shell (iced_exwlshell) | `wl_surface.set_input_region` via exwlshell's `SetInputRegion(ActionCallback::new(|region: &WlRegion| ..))` (`iced_exwlshell-0.20.1/src/actions.rs:129-144, 167`; applied `iced_exwlshell-0.20.1/src/multi_window.rs:887-908`, which first subtracts the whole surface, then calls our callback to `region.add(..)`) | `HitRegions` message (section 10) |
| X11 (winit) | our own x11rb connection: `shape_rectangles(SO::SET, SK::INPUT, ..)` with a rectangle list — the exact call winit uses, but with our rectangles; X11 coordinates are **physical** pixels | same |
| Windows / macOS (winit) | no shaped input in iced/winit. Poll the global cursor (e.g. 30-60 Hz timer; `GetCursorPos` / `NSEvent.mouseLocation`), test it against `window_pos + rects`, and call `disable_mouse_passthrough` when inside, `enable_mouse_passthrough` when outside (send only on change). Alternative on Windows: `SetWindowRgn` clips hit-testing **and painting** to the region, which would cut off the shadow/glow, so not recommended. `WM_NCHITTEST → HTTRANSPARENT` only forwards to windows of the same thread, so it cannot pass clicks to other apps (Win32 platform behaviour, not verifiable in these sources). | same |

X11 input shape (compiled, `src/x11.rs`):

```rust
use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{ClipOrdering, Rectangle as XRect};

pub fn set_input_region(conn: &impl Connection, window: u32, rects: &[iced::Rectangle], scale: f32)
    -> Result<(), x11rb::errors::ReplyOrIdError>
{
    let rects: Vec<XRect> = rects.iter().map(|r| XRect {
        x: (r.x * scale).floor() as i16, y: (r.y * scale).floor() as i16,
        width: (r.width * scale).ceil() as u16, height: (r.height * scale).ceil() as u16,
    }).collect();
    conn.shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, window, 0, 0, &rects)?; // empty = click-through
    conn.flush()?;
    Ok(())
}
```

`scale` = winit scale factor × program `scale_factor` (the viewport uses their product,
`iced_winit/src/window/state.rs:63-70`). Do not call winit's `set_cursor_hittest(true)` afterwards on X11: it would
overwrite our shape with a full rectangle.

---

## 6. Widgets

All in `iced::widget` (`iced/src/lib.rs:634-645` re-exports `iced_widget::*`).

### 6.1 `mouse_area` (`iced_widget/src/mouse_area.rs`)

`mouse_area(content)` (`iced_widget/src/helpers.rs:2010`). Builders: `on_press(Message)` (38), `on_release` (45),
`on_double_click` (61), `on_right_press` (68), `on_right_release` (75), `on_middle_press`/`release` (82, 89),
`on_scroll(Fn(mouse::ScrollDelta) -> Message)` (96-102), `on_enter(Message)` (106), `on_move(Fn(Point) -> Message)` (113),
`on_exit(Message)` (120), `interaction(mouse::Interaction)` cursor icon (127). Semantics (`update`, 329-437):

- `on_move` gets the position **relative to the area's bounds** (`cursor.position_in(bounds)`) and only while hovered
  (357-361). On the event where the cursor enters, `on_enter` wins and `on_move` is not emitted (match order, 349-366).
- All button handlers require the cursor to be over the bounds at that moment (369-371) → a release outside is lost.
- `on_press`, `on_right_press`, `on_middle_press`, `on_scroll` and `on_double_click` **capture** the event (378, 398, 410, 421, 432),
  so layers underneath do not see it. `on_release` does not capture.
- Messages must be `Clone` (165-170).

### 6.2 Layering and absolute positioning

- `stack![a, b, ..]` (`iced_widget/src/helpers.rs:107-114`) / `stack(iter)` (561-568) / `Stack::push`, `push_under`
  (`iced_widget/src/stack.rs:87-114`). The first child sets the stack's intrinsic size; every layer is laid out with
  limits `0..=stack size` at the stack origin (`iced_widget/src/stack.rs:164-208`). Events go top layer first and stop
  when captured; a hovered upper layer with a non-`None` mouse interaction "levitates" the cursor for layers below
  (249-275). Each layer after the first is drawn in its own renderer layer (345-364). `.width(Fill).height(Fill)` makes
  it fill the window.
- **`pin`** exists in 0.14: `pin(content).x(..).y(..)` / `.position(Point)` places content at fixed coordinates
  inside its bounds; `Pin` defaults to `Length::Fill` (`iced_widget/src/pin.rs:55-109`; layout moves the child node
  to `position`, 140-159). This is the absolute-positioning primitive: `stack![pin(a).x(..).y(..), pin(b)..]`.
- **`float`** (`iced_widget/src/float.rs`, helper `iced_widget/src/helpers.rs:2084`): `.scale(f32)` and
  `.translate(Fn(Rectangle, Rectangle) -> Vector)` (42-59). It only "floats" when `scale > 1.0` or the translation is
  non-zero (79-84); then the content is drawn **as an overlay** with a `Transformation`, and events are routed
  through the overlay with the inverse transform (210-250, 288-309). A scale below 1.0 with zero translation is
  ignored. Good for slide-in animations (compiled example in `src/api_probe.rs`), but floating content leaves the
  normal draw order (drawn above everything) and is not visited by `operate` at its translated position.
- **`opaque(content)`** captures any mouse press inside its bounds (`iced_widget/src/helpers.rs:577-730`, capture at 671-683).
- **`sensor`** (new in 0.14) reports `on_show(Size)`, `on_resize(Size)`, `on_hide` — sizes only, no position
  (`iced_widget/src/sensor.rs:63-83, 198-263`). Not enough for input regions (see section 10).
- `responsive(|Size| ..)` (`iced_widget/src/helpers.rs:2100`) if layout depends on window size.

### 6.3 `container` styling, shadows

`container(content)`: `.id(impl Into<widget::Id>)` (`iced_widget/src/container.rs:109`), `.padding`, `.width/.height`,
`.max_width`, `.center*/align_*`, `.clip(bool)` (207), `.style(Fn(&Theme) -> container::Style)` (214-220).

```rust
// iced_widget/src/container.rs:462-473
pub struct Style { pub text_color: Option<Color>, pub background: Option<Background>,
                   pub border: Border, pub shadow: Shadow, pub snap: bool }
// iced_core/src/shadow.rs:5-14
pub struct Shadow { pub color: Color, pub offset: Vector, pub blur_radius: f32 }
// iced_core/src/border.rs:6-14, 81-89
pub struct Border { pub color: Color, pub width: f32, pub radius: Radius }  // Radius has per-corner f32s; border::rounded(r)
```

Helpers: `Style::background/border/shadow/color` (`iced_widget/src/container.rs:489-519`), preset fns
`container::rounded_box`, `bordered_box`, `dark`, `primary`, ... (573-653). The container draws one quad with border +
shadow when any of background/border/shadow is set (`iced_widget/src/container.rs:435-458`).

Shadow support:
- **wgpu**: in the quad shader, analytic rounded-box SDF + smoothstep over `blur_radius`, drawn only outside the quad
  (`iced_wgpu/src/shader/quad/solid.wgsl:32-33, 90-98`).
- **tiny-skia**: yes, CPU per-pixel SDF into a temporary pixmap every time the quad is drawn
  (`iced_tiny_skia/src/engine.rs:67-145`); the shadow rectangle is cast with `as u32` (81-86), so a shadow crossing
  the window's left/top edge (negative coords) is clamped/shifted. Keep ~`blur_radius` of padding from the window edge.
  The tiny-skia shadow is painted under the full quad, wgpu only outside it: visible difference with translucent backgrounds.
- There is **no general blur** (no image blur, no backdrop blur); `Shadow` on quads is the only blur-like primitive.

Fading a whole card: there is no subtree opacity. Scale the alpha of every colour you use
(`Color::scale_alpha`, `iced_core/src/color.rs:157`) and `image(..).opacity(..)`.

### 6.4 `image` (feature `image` or `image-without-codecs`)

- `Handle::from_rgba(width: u32, height: u32, pixels: impl Into<Bytes>) -> Handle` (`iced_core/src/image.rs:147-158`):
  "RGBA pixels ... length `width * height * 4`" (140-146). **Straight (non-premultiplied) alpha**: tiny-skia premultiplies
  on upload (`iced_tiny_skia/src/raster.rs:116-121`), wgpu blends the texture with `SrcAlpha, OneMinusSrcAlpha`
  (`iced_wgpu/src/image/mod.rs:166-177`, shader multiplies only alpha by opacity, `iced_wgpu/src/shader/image.wgsl:121`).
- Every `from_rgba` call allocates a **new unique id** (`iced_core/src/image.rs:152-153, 209-216`) = a new texture
  upload. Create frame handles once (at load) and `clone()` them in `view` (clones keep the id).
- `FilterMethod::{Linear (default), Nearest}` (`iced_core/src/image.rs:231-238`); `image(h).filter_method(image::FilterMethod::Nearest)`
  (`iced_widget/src/image.rs:124`, `FilterMethod` re-exported at `iced_widget/src/image.rs:33`). wgpu has a dedicated
  nearest sampler (`iced_wgpu/src/image/mod.rs:42-48`); tiny-skia uses `FilterQuality::Nearest` (`iced_tiny_skia/src/raster.rs:62-65`).
  The widget always draws with `snap: true` (`iced_widget/src/image.rs:338-346`).
- Widget builders (`iced_widget/src/image.rs:58-180`): `width/height`, `expand`, `content_fit` (default `Contain`),
  `filter_method`, `rotation(impl Into<Rotation>)` (`Rotation::{Floating, Solid}`, `iced_core/src/rotation.rs:9`),
  `opacity(f32)`, `scale(f32)` (scales the drawn region from the centre; layout size unchanged), `crop(Rectangle<u32>)`
  (sprite-sheet cell without re-uploading), `border_radius`.

Sprite (compiled, `src/main.rs`):

```rust
image(self.frames[self.frame].clone())            // Handle created once in boot()
    .width(SPRITE_W as f32 * SCALE)                // 26*5 = 130
    .height(SPRITE_H as f32 * SCALE)               // 24*5 = 120
    .filter_method(image::FilterMethod::Nearest)
```

### 6.5 `canvas` (feature `canvas`)

- `canvas(program).width(..).height(..)` (`iced_widget/src/helpers.rs:1951`). `canvas::Program<Message, Theme, Renderer>`:
  `type State: Default + 'static`; `update(&self, &mut State, &Event, Rectangle, mouse::Cursor) -> Option<Action<Message>>`;
  `draw(&self, &State, &Renderer, &Theme, Rectangle, mouse::Cursor) -> Vec<Geometry<Renderer>>`;
  `mouse_interaction(..)` (`iced_widget/src/canvas/program.rs:13-73`).
- `Frame::fill_rectangle(top_left: Point, size: Size, fill: impl Into<Fill>)` (`iced_graphics/src/geometry/frame.rs:60-67`);
  `Path::rectangle/rounded_rectangle/circle` (`iced_graphics/src/geometry/path.rs:47-63`), builder `ellipse(arc::Elliptical)`
  (`iced_graphics/src/geometry/path/builder.rs:105`). `fill_text` always renders on top of all canvas layers (frame.rs:86-94).
- `canvas::Cache` (`iced_widget/src/canvas.rs:76-80`): `Cache::new()`, `cache.draw(renderer, size, |frame| ..)` only
  re-runs the closure when the size changed or after `cache.clear()` (`iced_graphics/src/geometry/cache.rs:29-69`).
  Keep the `Cache` in app state (outside `view`) and `clear()` it when the sprite frame changes.
- Compiled pixel-art program: `src/pixel_canvas.rs` (one `fill_rectangle` per opaque pixel). For a 26x24 sprite the
  `image` widget with `Nearest` is simpler and cheaper (one textured quad vs up to 624 triangles pairs per frame change),
  so use canvas only if per-pixel effects are needed.

### 6.6 Text, tooltips, buttons

- `text(..)` (`iced_widget/src/helpers.rs:1143`): `.size`, `.line_height`, `.font`, `.width/.height`, `.align_x/.align_y`,
  `.shaping`, `.wrapping(text::Wrapping)`, `.color`, `.style` (`iced_core/src/widget/text.rs:74-189`).
  `Wrapping::{None, Word (default), Glyph, WordOrGlyph}` (`iced_core/src/text.rs:181-193`).
- **No ellipsis / max-lines** anywhere in iced_core, iced_widget or iced_graphics (grep for "ellipsis" finds nothing).
  Options: truncate the string ourselves (char budget per line × max lines) and put the text in a fixed-height
  `container(..).clip(true)`; or measure with `advanced::text::Paragraph` inside a custom widget for exact fitting.
- `tooltip(content, tooltip, tooltip::Position::{Top, Bottom, Left, Right, FollowCursor})`
  (`iced_widget/src/helpers.rs:1110-1121`, `iced_widget/src/tooltip.rs:407-419`); `.gap`, `.padding`, `.delay(Duration)`,
  `.snap_within_viewport(bool)`, `.style` (112-153). Tooltips are overlays inside the same window: the window must be
  big enough, and the tooltip area does not need to be clickable.
- `button(content)` (`iced_widget/src/helpers.rs:1074`): `.on_press(Message)` (145), `.on_press_with(Fn() -> Message)` (158),
  `.on_press_maybe(Option<Message>)` (170), `.padding`, `.width/.height`, `.clip`, `.style(Fn(&Theme, button::Status) -> button::Style)` (184);
  preset styles `button::primary/secondary/success/warning/danger/text/background/subtle` (598-713).

---

## 7. Animation and redraw scheduling

### 7.1 `iced::Animation<T>` (`iced_core/src/animation.rs`, re-exported `iced/src/lib.rs:534-538`)

```rust
pub struct Animation<T: Clone + Copy + PartialEq + animation::Float>    // 11-17, wraps lilt::Animated<T, Instant>
Animation::new(state: T) -> Self                  // 24-29, default duration 100 ms
.easing(animation::Easing) / .very_quick() 100ms / .quick() 200ms / .slow() 400ms / .very_slow() 500ms / .duration(Duration)  // 35-65
.delay(Duration) / .repeat(u32) / .repeat_forever() / .auto_reverse()   // 68-91
.go(new_state, at: Instant) -> Self / .go_mut(&mut self, new_state, at)  // 95-104
.is_animating(at: Instant) -> bool                // 109-111
.interpolate_with(|T| -> I, at) -> I              // 119-124  (I: Interpolable)
.value() -> T                                     // 127-129  (the target state)
// only for Animation<bool>:
.interpolate(start: I, end: I, at) -> I           // 135-140
.remaining(at) -> Duration                        // 143-149
```

`Float` is implemented for `bool` and `f32` (`lilt/src/traits.rs:30, 40`); `Interpolable` for `f32` and `Option<T>`
(`lilt/src/traits.rs:51, 57`) — iced does not implement it for `Color`/`Point`, so interpolate an `f32` and derive colours
and offsets from it. Easings: `Linear, EaseIn/Out/InOut, *Quad, *Cubic, *Quart, *Quint, *Expo, *Circ, *Back, *Elastic, *Bounce`
(lilt `Easing` enum). Time is passed in explicitly; the animation does not tick itself.

### 7.2 Reactive rendering in 0.14

- Idle means idle: in `AboutToWait`, if there are no events, no messages and no window has a pending timed redraw,
  nothing runs (`iced_winit/src/lib.rs:1129-1134`) and the loop waits (`ControlFlow::Wait`, 1263-1266).
- Events are fed to the UI; a redraw happens only if some widget asked for it via `Shell::request_redraw()` /
  `request_redraw_at(..)` (`iced_core/src/shell.rs:66-77` → `window.request_redraw(redraw_request)`,
  `iced_winit/src/lib.rs:1171-1183`, `iced_winit/src/window.rs:199-210`), or if messages were processed (then every
  window redraws, 1253-1255). Timed requests become `ControlFlow::WaitUntil` (1258-1262, 723-747).
- Feature `unconditional-rendering` (`iced/Cargo.toml:109`) forces a redraw after every event (`iced_winit/src/lib.rs:1166-1169`).

### 7.3 `window::frames()` vs `time::every`

- `window::frames() -> Subscription<Instant>` emits the `Instant` of each `RedrawRequested` (`iced_runtime/src/window.rs:208-213`),
  broadcast after each window draw (`iced_winit/src/lib.rs:973-977`). Emitting a message causes an update, which redraws
  every window, which emits another frame: subscribing to `frames()` therefore yields a continuous vsync-paced loop
  (paced by `pre_present_notify` frame callbacks on Wayland, `winit/src/window.rs:603-640`, and by the present mode
  elsewhere). **Subscribe only while something animates**; unsubscribing stops the loop.
- It fires for the redraw of **every** window: with Settings open you get two frames per vsync. A per-window variant
  (compiled, `src/api_probe.rs`):

```rust
pub fn frames_of(pet: window::Id) -> Subscription<Msg> {
    iced::event::listen_raw(|event, _status, id| match event {
        Event::Window(window::Event::RedrawRequested(at)) => Some((id, at)),
        _ => None,
    })
    .with(pet)
    .filter_map(|(pet, (id, at))| (id == pet).then_some(Msg::PetFrame(at)))
}
```

- `iced::time::every(Duration) -> Subscription<Instant>` exists **only with the `tokio` or `smol` feature** (or wasm):
  `iced::time` re-exports the backend's `time` module (`iced/src/time.rs:2, 13`), and the default `thread-pool` backend's
  `time` module is empty (`iced_futures/src/backend/native/thread_pool.rs:20-22`; tokio `every` at
  `iced_futures/src/backend/native/tokio.rs:38-57`, smol at `smol.rs:29`). The first tick comes after one `duration`.
- Pattern (compiled, `src/main.rs`): an 8 fps `time::every(125 ms)` drives sprite frames (cheap), and `window::frames()`
  is added only while a UI tween runs:

```rust
fn subscription(&self) -> Subscription<Message> {
    let sprite = iced::time::every(Duration::from_millis(125)).map(Message::Frame);
    let tweens = if self.is_animating() { window::frames().map(Message::Frame) } else { Subscription::none() };
    Subscription::batch([sprite, tweens, window::close_events().map(Message::WindowClosed)])
}
fn is_animating(&self) -> bool {
    self.menu.is_animating(self.now) || self.bubbles.iter().any(|b| b.shown.is_animating(self.now))
}
// update: Message::Frame(now) => { self.now = now; /* advance sprite if due; drop finished bubbles */ }
```

Alternative without a timer: a custom sprite widget stores the `Instant` from `RedrawRequested(now)` in its tree
state during `update`, calls `shell.request_redraw_at(next_frame_instant)` (as `sensor` does,
`iced_widget/src/sensor.rs:258-260`), and picks the frame from the stored instant in `draw`. A widget redraw request
only redraws its own window (`iced_winit/src/lib.rs:961-971`, `iced_winit/src/window.rs:199-210`), so this animates
without app messages and without rebuilding/redrawing Settings.

---

## 8. Renderers

- Default features enable both `wgpu` and `tiny-skia` (`iced/Cargo.toml:64-73`), giving
  `Renderer = fallback::Renderer<iced_wgpu::Renderer, iced_tiny_skia::Renderer>` (`iced_renderer/src/lib.rs:25-36`).
  Only `wgpu` → wgpu only (38-42); only `tiny-skia` → tiny-skia only (44-48).
- Selection (`iced_renderer/src/fallback.rs:267-330`): backend list = `ICED_BACKEND` env var, comma separated
  (276-289); for each candidate try wgpu then tiny-skia (297-327). wgpu accepts `None | "wgpu"`
  (`iced_wgpu/src/window/compositor.rs:271-290`), tiny-skia `None | "tiny-skia" | "tiny_skia"`
  (`iced_tiny_skia/src/window/compositor.rs:38-48`). So `ICED_BACKEND=tiny-skia` forces software rendering;
  unset = wgpu, falling back to tiny-skia if wgpu fails to initialise.
- wgpu env vars: `WGPU_BACKEND` (`vulkan|vk`, `dx12|d3d12`, `metal|mtl`, `gl|gles|opengl`; `wgpu-types-27.0.1/src/lib.rs:301-340`,
  used at `iced_wgpu/src/window/compositor.rs:275-277`; default `Backends::all()`, `iced_wgpu/src/settings.rs:36`),
  `WGPU_POWER_PREF` (`iced_wgpu/src/window/compositor.rs:85-86`, `wgpu-types-27.0.1/src/lib.rs:223`),
  `ICED_PRESENT_MODE = vsync|no_vsync|immediate|fifo|fifo_relaxed|mailbox` (`iced_wgpu/src/settings.rs:60-84`).
- **One compositor for all windows**, created with the first window that opens (`iced_winit/src/lib.rs:580-644`)
  and dropped when the last window closes (1419-1421). Its surface format and alpha mode are chosen from that first
  window's surface (`iced_wgpu/src/window/compositor.rs:81-82, 98-141`) and reused for every later surface (334).
  Open the transparent pet window first.
- Clear colour: wgpu `LoadOp::Clear(background_color)` (`iced_wgpu/src/lib.rs:434-446`); tiny-skia fills damaged
  rects with `BlendMode::Source` (`iced_tiny_skia/src/lib.rs:79-104`). A transparent `background_color` is required.
- tiny-skia only repaints damaged regions (`iced_tiny_skia/src/window/compositor.rs:163-216`); wgpu redraws the whole
  surface each frame.

### 8.1 Transparency by renderer

| | wgpu | tiny-skia (softbuffer 0.4.8) |
|---|---|---|
| alpha source | surface `CompositeAlphaMode`: iced prefers `PostMultiplied`, else `PreMultiplied`, else `Auto` (`iced_wgpu/src/window/compositor.rs:123-137`). If the driver only offers `Opaque`/`Inherit`, `Auto` gives an opaque window. | softbuffer's pixel format |
| Wayland | works when the Vulkan/GL WSI exposes `PreMultiplied` (Mesa normally does — to be confirmed on the user's machine, see 11) | **No**: buffers are `wl_shm::Format::Xrgb8888` (`softbuffer/src/backends/wayland/buffer.rs:102, 142`), alpha ignored |
| X11 | needs the ARGB visual (winit does it, 2.4) + compositor + a driver alpha mode other than opaque; historically weak on NVIDIA proprietary (test) | Likely yes: softbuffer accepts depth-32 visuals (`softbuffer/src/backends/x11.rs:896-905`) and iced writes premultiplied BGRA with alpha in the top byte (`iced_tiny_skia/src/raster.rs:116-121`); verify at runtime |
| Windows | DX12/Vulkan alpha modes vary; test | **No**: 32-bit `BI_BITFIELDS` GDI blit (`softbuffer/src/backends/win32.rs:62-63`), no per-pixel alpha |
| macOS | Metal supports `PostMultiplied`/`PreMultiplied` in wgpu (to confirm) | **No**: `CGImageAlphaInfo::NoneSkipFirst` (`softbuffer/src/backends/cg.rs:328`) |

Consequence: the transparent pet needs **wgpu** everywhere except possibly X11. Do not rely on the tiny-skia fallback
for the pet window; if wgpu fails, the app should say so rather than render a black rectangle. iced_exwlshell also
depends on `iced_renderer` 0.14 (`iced_exwlshell-0.20.1/Cargo.toml:91-92`), so the same fallback rules
(and the tiny-skia Wayland limitation) apply to layer-shell.

---

## 9. Custom widgets

### 9.1 Trait (0.14; `iced_core/src/widget.rs:37-150`, re-exported as `iced::advanced::Widget` with feature `advanced`, `iced/src/advanced.rs:9-27`)

```rust
pub trait Widget<Message, Theme, Renderer> where Renderer: iced::advanced::Renderer {
    fn size(&self) -> Size<Length>;
    fn size_hint(&self) -> Size<Length> { self.size() }
    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node;   // &mut self in 0.14
    fn draw(&self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style,
            layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle);
    fn tag(&self) -> tree::Tag { tree::Tag::stateless() }
    fn state(&self) -> tree::State { tree::State::None }
    fn children(&self) -> Vec<Tree> { Vec::new() }
    fn diff(&self, tree: &mut Tree) { tree.children.clear(); }
    fn operate(&mut self, _tree: &mut Tree, _layout: Layout<'_>, _renderer: &Renderer, _operation: &mut dyn Operation) {}
    // no on_event in 0.14: update() returns nothing; capture with shell.capture_event() (0.13, from memory: on_event -> Status)
    fn update(&mut self, _tree: &mut Tree, _event: &Event, _layout: Layout<'_>, _cursor: mouse::Cursor,
              _renderer: &Renderer, _clipboard: &mut dyn Clipboard, _shell: &mut Shell<'_, Message>, _viewport: &Rectangle) {}
    fn mouse_interaction(&self, _tree: &Tree, _layout: Layout<'_>, _cursor: mouse::Cursor,
                         _viewport: &Rectangle, _renderer: &Renderer) -> mouse::Interaction { mouse::Interaction::None }
    fn overlay<'a>(&'a mut self, _tree: &'a mut Tree, _layout: Layout<'a>, _renderer: &Renderer,
                   _viewport: &Rectangle, _translation: Vector) -> Option<overlay::Element<'a, Message, Theme, Renderer>> { None }
}
```

`Shell` (`iced_core/src/shell.rs`): `publish(Message)` (41), `capture_event()` (49), `is_event_captured()` (61),
`request_redraw()` (66), `request_redraw_at(impl Into<RedrawRequest>)` (71-77), `invalidate_layout()` (128),
`invalidate_widgets()` (152). Renderer (`iced_core/src/renderer.rs`): `fill_quad(Quad { bounds, border, shadow, snap }, bg)`
(62, `Quad` 79-102), `with_layer(bounds, f)` (24), `with_transformation(Transformation, f)` (39), `with_translation` (50).
The root widget receives the `RedrawRequested` event every frame before drawing (`iced_winit/src/lib.rs:815-836`,
`iced_runtime/src/user_interface.rs:310-329`); messages published there are processed in the same frame (up to 3 passes,
`iced_winit/src/lib.rs:828-947`).

A complete compiled wrapper widget is `src/hit_regions.rs` (section 10). Minimal leaf widget for the sprite
(compiled, `src/sprite_widget.rs`; mirrors `iced_widget/src/image.rs:338-349`; `image::Renderer::draw_image(image,
bounds, clip_bounds)` is `iced_core/src/image.rs:331-336`, `Image::new/filter_method/snap` 41-76):

```rust
use iced::advanced::image as adv_image;
pub struct Sprite { pub handle: image::Handle, pub w: f32, pub h: f32 }

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Sprite
where Renderer: adv_image::Renderer<Handle = image::Handle>
{
    fn size(&self) -> Size<Length> { Size::new(Length::Fixed(self.w), Length::Fixed(self.h)) }
    fn layout(&mut self, _tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(self.w, self.h, Size::new(self.w, self.h)))
    }
    fn draw(&self, _tree: &Tree, renderer: &mut Renderer, _theme: &Theme, _style: &renderer::Style,
            layout: Layout<'_>, _cursor: mouse::Cursor, _viewport: &Rectangle) {
        let bounds = layout.bounds();
        renderer.draw_image(
            adv_image::Image::new(self.handle.clone()).filter_method(image::FilterMethod::Nearest).snap(true),
            bounds, bounds);
    }
}
impl<'a, Message, Theme, Renderer> From<Sprite> for Element<'a, Message, Theme, Renderer>
where Renderer: adv_image::Renderer<Handle = image::Handle> + 'a
{ fn from(s: Sprite) -> Self { Element::new(s) } }
```

Add `update` with `if let Event::Window(window::Event::RedrawRequested(now)) = event { ...; shell.request_redraw_at(next) }`
to make it self-animating (7.3).

### 9.2 Blur / glow

iced 0.14 has no image blur or filter pipeline; the only blur is the quad `Shadow` (6.3). Options:

1. **Precompute on the CPU** (recommended): take the sprite's glow layer (RGBA mask), upscale ×5 nearest, pad by the
   blur radius, run 3 separable box-blur passes (≈ gaussian) in premultiplied space, convert back to straight alpha,
   and upload once per glow frame with `Handle::from_rgba`. Draw it with `image(..).opacity(pulse)` in a stack layer
   under the sprite (Linear filter). Compiled implementation: `src/glow.rs::blurred_glow`. For the ground shadow,
   `src/glow.rs::soft_ellipse` builds a smooth-edged ellipse bitmap (iced gradients are linear only, and a rounded quad
   gives a stadium, not an ellipse).
2. A quad with `Shadow { blur_radius, color }` behind a rounded rectangle gives a cheap rectangular glow (works in both
   renderers).
3. A wgpu `shader` widget (`iced_widget/src/shader.rs`, feature `wgpu`) could blur on the GPU but does not exist in
   tiny-skia and is overkill here.

---

## 10. Recommended design for the shared pet UI

### 10.1 Split

- `pet_ui` crate/module, shell-agnostic: `PetState` (sprite frames as pre-built `image::Handle`s, bubble list with
  `Animation<bool>`, menu `Animation<bool>`, `now: Instant`), `PetMessage`, `fn view(&PetState) -> Element<PetMessage>`,
  `fn update(&mut PetState, PetMessage) -> PetEffect` (effects such as `StartDrag`, `OpenSettings`, `Quit`,
  `InputRegion(Vec<Rectangle>)` are interpreted by the shell).
- Shells: (a) `iced_exwlshell` layer-shell on Wayland/Hyprland, (b) `iced::daemon` + winit on Windows/macOS/X11.
  Each maps `PetEffect`s to its own window/region/move APIs. The Settings window is a normal decorated window in both
  (in the winit shell: `window::open(settings_window_settings())`, 1.3).

### 10.2 Window and layout

- One fixed-size transparent window (e.g. 360×420 logical) anchored bottom-centre on the pet; everything else is
  transparent and click-through. Avoid resizing it at runtime (on X11/Windows resizing keeps the top-left, so the pet
  would jump unless move+resize are paired).
- Root: `HitRegions::new(stack![..].width(Fill).height(Fill), PetMessage::HitRegions)`.
- Layers, bottom to top:
  1. shadow: `pin(image(shadow_handle).width(120).height(28)).x(..).y(..)` — the `soft_ellipse` bitmap; not clickable (no id).
  2. glow (optional): `pin(image(glow_handle).opacity(p))` — precomputed blur (9.2); not clickable.
  3. sprite: `pin(container(mouse_area(image(frame).filter_method(Nearest)).on_press(..).on_right_press(..)
     .on_double_click(..).interaction(mouse::Interaction::Grab)).id("hit:sprite"))`. For pixel-exact hit testing
     (transparent corners), let `pet_ui` replace the reported sprite bounding box with the current frame's opaque
     pixel runs (per row, merged, × 5, offset by the sprite's bounds origin) before handing rects to the shell;
     X11 shapes and Wayland regions both accept many rectangles.
  4. bubbles: a `column` of cards (iced computes variable text heights) inside a `pin` placed above the sprite and
     aligned to the bottom (`container(column).align_bottom(..)`), each card a `container` with background + rounded
     `Border` + `Shadow` (fits both renderers; keep ≥ blur radius from window edges for tiny-skia, 6.3).
  5. context menu (only while open): `pin(container(column![button(..).style(button::text), ..]).style(rounded_box).id("hit:menu"))`,
     position clamped inside the window.

### 10.3 Enter / leave animations

- Each bubble has `shown: Animation<bool>` (`.slow()` / `.easing(EaseOutCubic)`); on add `go(true, now)`, on dismiss
  `go_mut(false, now)` + `removing = true`; drop it once `!is_animating(now)` (compiled pattern in `src/main.rs`).
- `t = shown.interpolate(0.0, 1.0, now)`; fade by scaling every colour's alpha by `t` (background, text, border, shadow);
  slide by `float(card).translate(move |_, _| Vector::new(0.0, 12.0 * (1.0 - t)))` (compiled in `src/api_probe.rs`), or
  by the `pin` y offset when the bubble is placed manually (compiled in `src/main.rs`). Scale-in below 1.0 is not
  possible with `float` (scale must be > 1, 6.2); do it with `renderer.with_transformation` in a custom wrapper if wanted.
- Give a bubble its `id` (= clickable) only when fully shown and remove the id as soon as it starts leaving; this keeps
  the input region stable during tweens (no per-frame region updates) and avoids clicks on fading cards.
- Drive tweens with `window::frames()` (or the per-window `frames_of`, 7.3) only while `is_animating`; sprite frames
  with `time::every` (tokio/smol feature) or a self-scheduling sprite widget (`request_redraw_at`).

### 10.4 In-window context menu

- Right-press on the sprite toggles `menu: Animation<bool>`; draw the menu as a normal stack layer (not an iced overlay),
  so its bounds are visited by `operate` and end up in the input region.
- Clicks outside the input region go to other apps, so the pet never sees an "outside click". Close the menu on: item
  click, Escape (`keyboard::listen`, needs keyboard focus), `window::Event::Unfocused`, pointer leaving the window
  (`mouse::Event::CursorLeft`) after a short grace period, and a timeout. Optionally, while the menu is open add a
  full-window transparent `container(..).id(..)` scrim so any click inside the window area closes it.

### 10.5 How the shell learns the exact clickable rectangles every frame

`src/hit_regions.rs` (compiled). Every `container` that has an `.id(..)` is a hit region. On each `RedrawRequested`
the root wrapper runs an `Operation` over its subtree; `Container::operate` reports `container(self.id, layout.bounds())`
(`iced_widget/src/container.rs:280-296`; `Operation` trait `iced_core/src/widget/operation.rs:20-30`, `Send` required).
The wrapper collects rectangles for `Some(id)`, rounds them outwards to whole logical pixels, and publishes
`Message::HitRegions(Vec<Rectangle>)` only when the list changed. The message is handled in the same frame (9.1), and
`update` forwards it to the shell:

```rust
// src/hit_regions.rs (compiled), core of update():
if let Event::Window(window::Event::RedrawRequested(_)) = event {
    let mut collect = Collect(Vec::new());
    self.content.as_widget_mut().operate(&mut tree.children[0], layout, renderer, &mut collect);
    let state = tree.state.downcast_mut::<State>();
    if state.last != collect.0 {
        state.last = collect.0.clone();
        shell.publish((self.on_change)(collect.0));
    }
}
// then forward the event to the content as usual

struct Collect(Vec<Rectangle>);
impl Operation for Collect {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<()>)) { operate(self); }
    fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
        if id.is_some() { self.0.push(/* bounds rounded outwards */ bounds); }
    }
}
```

Shell side:
- Wayland layer-shell: `SetInputRegion(ActionCallback::new(move |r| for q in &rects { r.add(q.x as i32, q.y as i32, q.width as i32, q.height as i32) }))`
  (logical surface coordinates) (`iced_exwlshell-0.20.1/src/actions.rs:137-144`, `multi_window.rs:887-908`).
- X11: `x11::set_input_region(&conn, xid, &rects, scale)` (5.1), physical pixels.
- Windows/macOS: store `rects`; a cursor-polling timer toggles `enable/disable_mouse_passthrough` (5.1).
- Rects are in window-local logical coordinates. Rects from overlays (tooltips, floating `float`s) are not visited;
  keep clickable things out of overlays.
- Why not other options: `sensor` gives only sizes (6.2); `widget::selector::find` needs iced's `selector` feature and
  the `iced_selector` crate (`iced_runtime/src/widget/selector.rs:1-25`), which is not in the local registry; recomputing
  layout by hand breaks with wrapped text.

### 10.6 Dragging

- Layer-shell: the shell moves the surface via margins (exwlshell `MarginChange`, `iced_exwlshell-0.20.1/src/actions.rs:155`)
  from pointer deltas; compositor move is not available for layer surfaces.
- winit shells: `move_to` with global pointer coordinates (section 4, strategy 2) so the pet always knows where it is;
  `window::drag` as a fallback where WM-native feel matters.

---

## 11. Must be tested at runtime (not verifiable from source)

1. wgpu surface alpha modes and actual transparency: Hyprland 0.56 via winit (Vulkan and `WGPU_BACKEND=gl`), X11 (Mesa
   and NVIDIA), Windows (DX12 and Vulkan), macOS (Metal). Log `RUST_LOG=iced_wgpu=info` shows "Available alpha modes"
   and "Selected format ... alpha mode" (`iced_wgpu/src/window/compositor.rs:125, 143-145`); add a `log` backend to see it.
2. tiny-skia transparency on X11 with a compositor (expected to work, 8.1).
3. Windows: window visibility after `enable_mouse_passthrough` (adds `WS_EX_LAYERED` without
   `SetLayeredWindowAttributes`, 5), and flicker/latency of the polling toggle approach.
4. X11: that the WM honours `_NET_WM_STATE_SKIP_TASKBAR/ABOVE` set before mapping with the `visible: false` →
   `set_mode(Windowed)` sequence (2.2), and that winit's own `_NET_WM_STATE_ABOVE` toggle at creation does not fight it.
5. Drag jitter with window-local vs global pointer coordinates (4).
6. CPU/GPU cost of the 60 Hz tween loop with Settings open (1.4, 7.3).
