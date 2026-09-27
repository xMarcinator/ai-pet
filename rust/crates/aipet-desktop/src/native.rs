//! The native calls behind the pet window, which iced and winit have no API for: which parts of the window take the
//! mouse (and which are drawn at all), keeping it above other windows and out of the taskbar and window switchers,
//! and where the pointer is on the desktop.
//!
//! One module per platform (`native/*.rs`):
//! - X11 and XWayland: the SHAPE extension's input shape, and its bounding shape for what is drawn; EWMH hints. They
//!   go through a connection of our own to the display winit uses: X window ids are server-wide, so the window can
//!   be changed from there as a window manager or xprop would.
//! - Windows: a window region (SetWindowRgn), which clips hit-testing and drawing alike, as WindowsPlatform does;
//!   WS_EX_TOOLWINDOW, kept through winit's style rewrites; HWND_TOPMOST.
//! - macOS: AppKit has no input region, so [`update_hit_test`] (about 30 times a second) makes the window ignore the
//!   mouse while the pointer is outside the region.
//!
//! The window functions take what `iced::window::run` hands its closure, and are called from there: iced runs it on
//! the event-loop thread, which is the main thread AppKit needs and the window's own thread SetWindowSubclass needs.
//!
//! Never mix this with iced's `window::enable_mouse_passthrough` / `disable_mouse_passthrough` (winit's
//! `set_cursor_hittest`): on X11 they overwrite the input shape, and once it has been turned on winit sets a
//! full-window one again on every resize.

#[cfg(target_os = "linux")]
#[path = "native/x11.rs"]
mod sys;

#[cfg(windows)]
#[path = "native/win32.rs"]
mod sys;

#[cfg(target_os = "macos")]
#[path = "native/mac.rs"]
mod sys;

use std::fmt;

use aipet_ui::Rect;
use iced::Point;
use iced::window::Window;

/// A rectangle in physical pixels, relative to the window's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PxRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Logical rectangles grown by `pad` on every side, in the physical pixels of a window at `scale`: the top-left
/// corner floored and the bottom-right one ceiled, as MainWindow.UpdateInputRegion does, so every pixel they touch
/// is in.
pub fn to_physical(rects: &[Rect], scale: f32, pad: f32) -> Vec<PxRect> {
    let (s, pad) = (f64::from(scale), f64::from(pad));
    rects
        .iter()
        .map(|r| {
            let x0 = ((f64::from(r.x) - pad) * s).floor() as i32;
            let y0 = ((f64::from(r.y) - pad) * s).floor() as i32;
            let x1 = ((f64::from(r.x + r.width) + pad) * s).ceil() as i32;
            let y1 = ((f64::from(r.y + r.height) + pad) * s).ceil() as i32;
            PxRect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
pub enum NativeError {
    /// This window system has no such thing (a native Wayland window, a server without SHAPE…).
    Unsupported(&'static str),
    /// The window's handle could not be had (it is gone, or not ready yet).
    Handle(String),
    /// The OS call failed.
    Os(String),
}

impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
            Self::Handle(e) => write!(f, "no window handle: {e}"),
            Self::Os(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for NativeError {}

fn handle(w: &dyn Window) -> Result<sys::Handle, NativeError> {
    let raw = w
        .window_handle()
        .map_err(|e| NativeError::Handle(e.to_string()))?
        .as_raw();
    sys::handle(raw)
}

/// X11: opens the connection the other calls use, so a missing or unreachable display is found before iced (whose
/// event loop would panic) tries it.
#[cfg(target_os = "linux")]
pub fn connect_x11() -> Result<(), NativeError> {
    sys::connect()
}

/// Makes the window the pet's, once it exists and before it is shown: out of the taskbar and the window switchers,
/// above the other windows if `keep_above`, and (X11) a utility window, which tiling window managers float. X11 window
/// managers read all of it when the window is mapped, so open it hidden, call this, then show it.
pub fn setup_pet_window(w: &dyn Window, keep_above: bool) -> Result<(), NativeError> {
    sys::setup(handle(w)?, keep_above)
}

/// Only `input` takes the mouse; everything else clicks through to what is below. Where the platform also clips
/// drawing to a region (the X11 bounding shape, a Windows window region), that is `drawn`, which must hold `input`:
/// on Windows it is where the mouse is taken, too.
pub fn set_region(w: &dyn Window, input: &[PxRect], drawn: &[PxRect]) -> Result<(), NativeError> {
    sys::set_region(handle(w)?, input, drawn)
}

/// Says "above the other windows" again (MainWindow does every 2 s while "Always on top" is on). Idempotent.
pub fn keep_on_top(w: &dyn Window) -> Result<(), NativeError> {
    sys::keep_on_top(handle(w)?)
}

/// macOS: compares the pointer with the input region and makes the window ignore the mouse while it is outside.
#[cfg(target_os = "macos")]
pub fn update_hit_test(w: &dyn Window) -> Result<(), NativeError> {
    sys::update_hit_test(handle(w)?)
}

/// Where the pointer is on the desktop, in the logical px `window::move_to` takes for a window at `scale`. It is
/// the same wherever the window is, unlike the pointer positions the window's own events carry.
pub fn pointer(scale: f32) -> Result<Point, NativeError> {
    sys::pointer(scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_rects_round_outwards_and_grow_by_the_pad() {
        let r = Rect {
            x: 10,
            y: 20,
            width: 5,
            height: 5,
        };
        // 12.5..18.75 × 25..31.25 -> 12..19 × 25..32
        assert_eq!(
            to_physical(&[r], 1.25, 0.0),
            vec![PxRect {
                x: 12,
                y: 25,
                w: 7,
                h: 7
            }]
        );
        // 8..17 × 18..27 logical, at 1.25: 10..21.25 × 22.5..33.75 -> 10..22 × 22..34
        assert_eq!(
            to_physical(&[r], 1.25, 2.0),
            vec![PxRect {
                x: 10,
                y: 22,
                w: 12,
                h: 12
            }]
        );
        // whole scales are exact
        assert_eq!(
            to_physical(&[r], 2.0, 0.0),
            vec![PxRect {
                x: 20,
                y: 40,
                w: 10,
                h: 10
            }]
        );
    }
}
