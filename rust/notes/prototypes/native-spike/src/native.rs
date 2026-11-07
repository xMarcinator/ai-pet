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
