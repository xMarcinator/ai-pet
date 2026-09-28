//! X11 and XWayland: the SHAPE extension and EWMH hints, sent over a connection of our own to the display winit
//! uses (LinuxPlatform's calls, through x11rb instead of libX11).
//!
//! Hyprland's XWayland ignores input shapes; the Wayland shell is the one for it.

use std::sync::OnceLock;

use iced::Point;
use iced::window::raw_window_handle::RawWindowHandle;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::cookie::VoidCookie;
use x11rb::errors::ConnectionError;
use x11rb::protocol::shape::{self, ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ClipOrdering, ConnectionExt as _, EventMask, KeyButMask, MapState, PropMode,
    Rectangle, Window,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use super::{NativeError, Pointer, PxRect};

/// The X window id.
pub type Handle = Window;

x11rb::atom_manager! {
    Atoms: AtomsCookie {
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
    root: Window,
    atoms: Atoms,
    has_shape: bool,
}

/// The connection, opened on first use from $DISPLAY, as winit's is.
fn x() -> Result<&'static X, NativeError> {
    static X: OnceLock<Result<X, String>> = OnceLock::new();
    X.get_or_init(|| {
        let (conn, screen) = x11rb::connect(None).map_err(|e| format!("cannot open the X display: {e}"))?;
        let root = conn.setup().roots[screen].root;
        let atoms = Atoms::new(&conn)
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| format!("X11 atoms: {e}"))?;
        let has_shape = conn
            .extension_information(shape::X11_EXTENSION_NAME)
            .map_err(|e| e.to_string())?
            .is_some();
        Ok(X {
            conn,
            root,
            atoms,
            has_shape,
        })
    })
    .as_ref()
    .map_err(|e| NativeError::Os(e.clone()))
}

fn os(e: impl std::fmt::Display) -> NativeError {
    NativeError::Os(format!("X11: {e}"))
}

/// Sends a request without waiting for its answer. An error it causes (a BadWindow once the window is gone) is
/// dropped, rather than left to queue up on a connection that never reads its events.
fn fire(cookie: Result<VoidCookie<'_, RustConnection>, ConnectionError>) -> Result<(), NativeError> {
    cookie.map_err(os)?.ignore_error();
    Ok(())
}

pub fn connect() -> Result<(), NativeError> {
    x().map(|_| ())
}

pub fn handle(raw: RawWindowHandle) -> Result<Handle, NativeError> {
    match raw {
        // winit's X11 windows hand out Xlib handles
        RawWindowHandle::Xlib(h) => {
            Window::try_from(h.window).map_err(|_| NativeError::Handle("XID out of range".into()))
        }
        _ => Err(NativeError::Unsupported("the pet window is not an X11 window")),
    }
}

pub fn setup(win: Window, keep_above: bool) -> Result<(), NativeError> {
    let x = x()?;
    let (c, a) = (&x.conn, &x.atoms);
    // utility rather than winit's normal: tiling window managers float it, most keep it out of the taskbar. Checked:
    // the one round trip, which also proves the window id is good on this connection.
    c.change_property32(
        PropMode::REPLACE,
        win,
        a._NET_WM_WINDOW_TYPE,
        AtomEnum::ATOM,
        &[a._NET_WM_WINDOW_TYPE_UTILITY, a._NET_WM_WINDOW_TYPE_NORMAL],
    )
    .map_err(os)?
    .check()
    .map_err(os)?;
    // no drop shadow around the whole window from compositors that honour it (picom)
    fire(c.change_property32(PropMode::REPLACE, win, a._COMPTON_SHADOW, AtomEnum::CARDINAL, &[0]))?;
    let mut states = vec![a._NET_WM_STATE_SKIP_TASKBAR, a._NET_WM_STATE_SKIP_PAGER];
    if keep_above {
        states.push(a._NET_WM_STATE_ABOVE);
    }
    add_states(x, win, &states)?;
    c.flush().map_err(os)
}

/// EWMH: a window that is not mapped sets its _NET_WM_STATE itself; a mapped one asks the window manager with a
/// message to the root window. winit only ever sends the message, and iced makes every window unmapped, so a window
/// manager that drops messages about unmapped windows would never see winit's "above".
fn add_states(x: &X, win: Window, states: &[Atom]) -> Result<(), NativeError> {
    let (c, a) = (&x.conn, &x.atoms);
    let attrs = c.get_window_attributes(win).map_err(os)?.reply().map_err(os)?;
    if attrs.map_state == MapState::UNMAPPED {
        let current = c
            .get_property(false, win, a._NET_WM_STATE, AtomEnum::ATOM, 0, 64)
            .map_err(os)?
            .reply()
            .map_err(os)?;
        let mut list: Vec<Atom> = current.value32().map(|v| v.collect()).unwrap_or_default();
        for state in states {
            if !list.contains(state) {
                list.push(*state);
            }
        }
        fire(c.change_property32(PropMode::REPLACE, win, a._NET_WM_STATE, AtomEnum::ATOM, &list))
    } else {
        // _NET_WM_STATE_ADD (1), two states per message, from a normal application (1)
        for pair in states.chunks(2) {
            let data = [1, pair[0], pair.get(1).copied().unwrap_or(0), 1, 0];
            let event = ClientMessageEvent::new(32, win, a._NET_WM_STATE, data);
            let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
            fire(c.send_event(false, x.root, mask, event))?;
        }
        Ok(())
    }
}

/// X rectangles, clamped to their 16-bit fields.
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

pub fn set_region(win: Window, input: &[PxRect], drawn: &[PxRect]) -> Result<(), NativeError> {
    let x = x()?;
    if !x.has_shape {
        return Err(NativeError::Unsupported("the X server has no SHAPE extension"));
    }
    // the bounding shape: no compositor shadow box around the whole window, and without a compositor only these
    // boxes go black instead of the whole window
    let bounding = rects(drawn);
    fire(
        x.conn
            .shape_rectangles(SO::SET, SK::BOUNDING, ClipOrdering::UNSORTED, win, 0, 0, &bounding),
    )?;
    let input = rects(input);
    fire(
        x.conn
            .shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, win, 0, 0, &input),
    )?;
    x.conn.flush().map_err(os)
}

pub fn keep_on_top(win: Window) -> Result<(), NativeError> {
    let x = x()?;
    add_states(x, win, &[x.atoms._NET_WM_STATE_ABOVE])?;
    x.conn.flush().map_err(os)
}

pub fn pointer(scale: f32) -> Result<Pointer, NativeError> {
    let x = x()?;
    let p = x.conn.query_pointer(x.root).map_err(os)?.reply().map_err(os)?;
    Ok(Pointer {
        // the root window's coordinates are the screen's physical pixels
        at: Point::new(f32::from(p.root_x) / scale, f32::from(p.root_y) / scale),
        // button 1 after the server's button mapping, which is winit's left
        left_held: p.mask.contains(KeyButMask::BUTTON1),
    })
}
