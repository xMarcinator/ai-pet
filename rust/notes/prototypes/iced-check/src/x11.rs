//! X11 helpers over a private x11rb connection (window ids are server-global, so a second
//! connection can reshape/annotate the window winit created).
#![allow(dead_code)]
use iced::Rectangle;
use x11rb::connection::Connection;
use x11rb::errors::ReplyOrIdError;
use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{AtomEnum, ClipOrdering, ConnectionExt as _, PropMode, Rectangle as XRect};
use x11rb::wrapper::ConnectionExt as _;

/// Replace the window's input shape with `rects` (logical px * `scale` = physical px).
/// An empty slice makes the whole window click-through.
pub fn set_input_region(
    conn: &impl Connection,
    window: u32,
    rects: &[Rectangle],
    scale: f32,
) -> Result<(), ReplyOrIdError> {
    let rects: Vec<XRect> = rects
        .iter()
        .map(|r| XRect {
            x: (r.x * scale).floor() as i16,
            y: (r.y * scale).floor() as i16,
            width: (r.width * scale).ceil() as u16,
            height: (r.height * scale).ceil() as u16,
        })
        .collect();
    conn.shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, window, 0, 0, &rects)?;
    conn.flush()?;
    Ok(())
}

/// Set EWMH hints BEFORE the window is mapped (open it with `visible: false`, call this from
/// the `window::open` completion, then `window::set_mode(id, window::Mode::Windowed)`).
pub fn set_pet_hints(conn: &impl Connection, window: u32) -> Result<(), ReplyOrIdError> {
    let atom = |name: &[u8]| -> Result<u32, ReplyOrIdError> {
        Ok(conn.intern_atom(false, name)?.reply()?.atom)
    };
    let wm_state = atom(b"_NET_WM_STATE")?;
    let states = [
        atom(b"_NET_WM_STATE_SKIP_TASKBAR")?,
        atom(b"_NET_WM_STATE_SKIP_PAGER")?,
        atom(b"_NET_WM_STATE_ABOVE")?,
    ];
    conn.change_property32(PropMode::REPLACE, window, wm_state, AtomEnum::ATOM, &states)?;

    let wm_type = atom(b"_NET_WM_WINDOW_TYPE")?;
    let utility = atom(b"_NET_WM_WINDOW_TYPE_UTILITY")?;
    conn.change_property32(PropMode::REPLACE, window, wm_type, AtomEnum::ATOM, &[utility])?;
    conn.flush()?;
    Ok(())
}

/// Global pointer position in physical root coordinates.
pub fn pointer(conn: &impl Connection, root: u32) -> Result<(i16, i16), ReplyOrIdError> {
    let r = conn.query_pointer(root)?.reply()?;
    Ok((r.root_x, r.root_y))
}
