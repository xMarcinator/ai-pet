//! Windows: WindowsPlatform's calls. A window region clips hit-testing and drawing alike, so the region is the
//! `drawn` one; winit draws an undecorated window's client area over the whole window, so the region's
//! window-relative pixels are the client area's. Where the region takes the mouse and the input region does not (the
//! pet's shadows and glow, a tooltip), the window lets the mouse through as the macOS one does: it ignores the mouse
//! while the pointer is outside the input region (WS_EX_TRANSPARENT | WS_EX_LAYERED, the style winit's
//! `set_cursor_hittest(false)` sets). It starts on the hit test of the move that takes the pointer there, before a
//! click can follow, and stops on a timer (about 30 times a second), since a window that ignores the mouse is sent
//! nothing.
//!
//! The subclass also holds back a scale change while the pet is held. Windows gives the window the scale of the
//! monitor that has most of it, and the pet's window changes size with its scale, so a drag along the edge between
//! monitors at different scales would flip the scale back and forth (rust/proofs/windows.md). The pet keeps its size
//! until the drop, and then takes the window's scale, scaled about the pointer so the grabbed spot stays under it.

use std::cell::{Cell, RefCell};

use iced::Point;
use iced::window::raw_window_handle::RawWindowHandle;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CombineRgn, CreateRectRgn, DeleteObject, RGN_OR, SetWindowRgn};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetCursorPos, GetSystemMetrics, GetWindowLongW, GetWindowRect, HTTRANSPARENT, HWND_TOPMOST, KillTimer,
    SM_SWAPBUTTON, STYLESTRUCT, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetTimer,
    SetWindowLongW, SetWindowPos, USER_DEFAULT_SCREEN_DPI, WM_DPICHANGED, WM_KILLFOCUS, WM_LBUTTONUP, WM_NCDESTROY,
    WM_NCHITTEST, WM_STYLECHANGING, WM_TIMER, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

use super::{NativeError, Pointer, PxRect};

pub type Handle = HWND;

const SUBCLASS_ID: usize = 0x4150_4554; // "APET"
/// The timer that takes the mouse again once the pointer is back on the input region (macOS's HIT_TEST_EVERY).
const HIT_TEST_TIMER: usize = SUBCLASS_ID;
const HIT_TEST_EVERY_MS: u32 = 33;
/// The extended style of a window that ignores the mouse (winit's for `set_cursor_hittest(false)`).
const IGNORE_MOUSE: u32 = WS_EX_TRANSPARENT | WS_EX_LAYERED;

// user32 since Windows 10 1607; windows-sys has it behind a feature this crate doesn't enable
#[link(name = "user32")]
unsafe extern "system" {
    fn GetDpiForWindow(hwnd: HWND) -> u32;
}

thread_local! {
    /// The pet window's input region, in physical pixels relative to the window; `None` (the whole window) until the
    /// first is set. The window's procedure runs on the thread that made it, the event loop's, as `set_region` does.
    static REGION: RefCell<Option<Vec<PxRect>>> = const { RefCell::new(None) };
    /// Whether the window ignores the mouse now.
    static IGNORING: Cell<bool> = const { Cell::new(false) };
    /// The pet window's DPI as winit has it, and one held back while the pet is held.
    static DPI: Cell<DpiHold> = const { Cell::new(DpiHold { winit: USER_DEFAULT_SCREEN_DPI, held: None }) };
}

/// The pet window's DPI: the one winit has, and the window's own while a held button keeps it from winit.
#[derive(Clone, Copy, Debug, PartialEq)]
struct DpiHold {
    winit: u32,
    held: Option<u32>,
}

impl DpiHold {
    /// Windows gives the window `dpi` (WM_DPICHANGED): whether winit gets it now. While the drag's button is held it
    /// waits for the drop instead, the latest replacing any that waited before.
    fn changed(&mut self, dpi: u32, button_held: bool) -> bool {
        if button_held {
            self.held = Some(dpi);
            return false;
        }
        self.held = None;
        self.winit = dpi;
        true
    }

    /// The drop: the waiting DPI, which winit gets now, with the one it has; nothing if the window ended at winit's.
    fn dropped(&mut self) -> Option<(u32, u32)> {
        let dpi = self.held.take()?;
        let old = std::mem::replace(&mut self.winit, dpi);
        (dpi != old).then_some((old, dpi))
    }
}

/// The window at `window` (desktop pixels), its scale changed from `old` DPI to `new`, scaled about the pointer at
/// `pointer`, so the grabbed spot stays under it. winit puts the window's top-left corner where this says and works
/// out its size itself, from its logical size at the new scale.
fn scaled_about(window: RECT, pointer: POINT, old: u32, new: u32) -> RECT {
    let k = f64::from(new) / f64::from(old.max(1));
    let scale = |px: i32| (f64::from(px) * k).round() as i32;
    let left = pointer.x - scale(pointer.x - window.left);
    let top = pointer.y - scale(pointer.y - window.top);
    RECT {
        left,
        top,
        right: left + scale(window.right - window.left),
        bottom: top + scale(window.bottom - window.top),
    }
}

/// Hands winit the DPI held back during a drag, if one is: a WM_DPICHANGED with the window scaled about the pointer.
/// Only the subclass procedure calls it, as DefSubclassProc must be.
fn apply_held_dpi(hwnd: HWND) {
    let mut dpi = DPI.get();
    let dropped = dpi.dropped();
    DPI.set(dpi);
    let Some((old, new)) = dropped else {
        return;
    };
    let mut window = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut pointer = POINT { x: 0, y: 0 };
    // SAFETY: plain user32 calls writing to locals
    if unsafe { GetWindowRect(hwnd, &mut window) } == 0 || unsafe { GetCursorPos(&mut pointer) } == 0 {
        // without the pointer, the window keeps its top-left corner
        pointer = POINT {
            x: window.left,
            y: window.top,
        };
    }
    let rect = scaled_about(window, pointer, old, new);
    let both = new | (new << 16);
    // SAFETY: from within the subclass procedure; WM_DPICHANGED's lparam points to the suggested window rectangle,
    // which outlives the call
    unsafe { DefSubclassProc(hwnd, WM_DPICHANGED, both as WPARAM, &rect as *const RECT as LPARAM) };
}

/// Whether the window ignores the mouse with the pointer at (`x`, `y`) (physical pixels relative to the window) and
/// the input region `input` (`None`: the whole window): while the pointer is outside it, but never starting while a
/// button is held (a drag that left the region), as the macOS window does.
fn ignores_mouse(input: Option<&[PxRect]>, x: i32, y: i32, ignoring: bool, button_held: bool) -> bool {
    let inside = input.is_none_or(|rects| {
        rects
            .iter()
            .any(|r| x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h)
    });
    !inside && (ignoring || !button_held)
}

/// Whether the primary button is held: winit's left, which is the right one when they are swapped. GetAsyncKeyState
/// reads the physical buttons (its top bit: held now).
fn primary_held() -> bool {
    // SAFETY: plain user32 calls
    unsafe {
        let primary = if GetSystemMetrics(SM_SWAPBUTTON) != 0 {
            VK_RBUTTON
        } else {
            VK_LBUTTON
        };
        GetAsyncKeyState(i32::from(primary)) < 0
    }
}

fn button_held() -> bool {
    [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON]
        .into_iter()
        // SAFETY: plain user32 call
        .any(|vk| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0)
}

/// Whether the window should ignore the mouse with the pointer at (`sx`, `sy`) on the desktop (physical pixels).
fn should_ignore(hwnd: HWND, sx: i32, sy: i32) -> bool {
    let mut w = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: plain user32 call writing to a local
    if unsafe { GetWindowRect(hwnd, &mut w) } == 0 {
        return IGNORING.get();
    }
    let (x, y) = (sx - w.left, sy - w.top);
    REGION.with(|r| ignores_mouse(r.borrow().as_deref(), x, y, IGNORING.get(), button_held()))
}

/// Makes the window ignore the mouse, or take it again.
fn set_ignoring(hwnd: HWND, ignore: bool) {
    if IGNORING.get() == ignore {
        return;
    }
    // first, so that keep_tool_window keeps the style this sets
    IGNORING.set(ignore);
    // SAFETY: plain user32 calls on our own window, on its thread, as winit changes its extended style
    unsafe {
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let ex = if ignore { ex | IGNORE_MOUSE } else { ex & !IGNORE_MOUSE };
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex as i32);
        SetWindowPos(
            hwnd,
            0,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

/// Compares the pointer with the input region, and makes the window ignore the mouse or take it again.
fn update_hit_test(hwnd: HWND) {
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: plain user32 call writing to a local
    if unsafe { GetCursorPos(&mut p) } != 0 {
        set_ignoring(hwnd, should_ignore(hwnd, p.x, p.y));
    }
}

pub fn handle(raw: RawWindowHandle) -> Result<Handle, NativeError> {
    match raw {
        RawWindowHandle::Win32(h) => Ok(h.hwnd.get()),
        _ => Err(NativeError::Unsupported("the pet window is not a Win32 window")),
    }
}

/// winit rebuilds the whole extended style from its own flags whenever one of them changes (the level, the
/// visibility…), which would drop WS_EX_TOOLWINDOW, bring back WS_EX_APPWINDOW and drop the style that ignores the
/// mouse. This puts them back on every change. It also lets the mouse through where the input region does not take
/// it, from the hit test of the move that brings the pointer there, and on its timer takes the mouse again.
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
        if IGNORING.get() {
            style.styleNew |= IGNORE_MOUSE;
        }
    }
    if msg == WM_NCHITTEST {
        // lparam: the pointer on the desktop, as two signed 16-bit coordinates
        let (sx, sy) = (i32::from(lparam as u16 as i16), i32::from((lparam >> 16) as u16 as i16));
        if should_ignore(hwnd, sx, sy) {
            set_ignoring(hwnd, true);
            return HTTRANSPARENT as LRESULT;
        }
    }
    if msg == WM_DPICHANGED {
        let mut dpi = DPI.get();
        let now = dpi.changed(u32::from(wparam as u16), primary_held());
        DPI.set(dpi);
        if !now {
            return 0;
        }
    }
    // the drop: the release, or a lost one (the focus taken, or the button found up on the timer). The held DPI goes
    // to winit before the release does, so the pet has its new scale when the drag ends.
    if msg == WM_LBUTTONUP || msg == WM_KILLFOCUS {
        apply_held_dpi(hwnd);
    }
    if msg == WM_TIMER && wparam == HIT_TEST_TIMER {
        if !primary_held() {
            apply_held_dpi(hwnd);
        }
        update_hit_test(hwnd);
        return 0;
    }
    if msg == WM_NCDESTROY {
        // SAFETY: stopping our own timer and removing our own subclass from the window they were set on
        unsafe {
            KillTimer(hwnd, HIT_TEST_TIMER);
            RemoveWindowSubclass(hwnd, Some(keep_tool_window), SUBCLASS_ID);
        }
    }
    // SAFETY: passing the message on, as a subclass procedure must
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

pub fn setup(hwnd: HWND, keep_above: bool) -> Result<(), NativeError> {
    // WS_EX_TOOLWINDOW: not in Alt+Tab or the taskbar (WindowsPlatform.SetupPetWindow). Not WS_EX_NOACTIVATE: the
    // pet becomes the foreground app when clicked, as the C#'s does.
    // SAFETY: hwnd is a live window of this thread (iced runs window::run on the event-loop thread)
    unsafe {
        if SetWindowSubclass(hwnd, Some(keep_tool_window), SUBCLASS_ID, 0) == 0 {
            return Err(NativeError::Os("SetWindowSubclass failed".into()));
        }
        if SetTimer(hwnd, HIT_TEST_TIMER, HIT_TEST_EVERY_MS, None) == 0 {
            return Err(NativeError::Os("SetTimer failed".into()));
        }
        // the DPI winit made the window at
        let dpi = GetDpiForWindow(hwnd);
        if dpi != 0 {
            DPI.set(DpiHold { winit: dpi, held: None });
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongW(hwnd, GWL_EXSTYLE, ((ex | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW) as i32);
        SetWindowPos(
            hwnd,
            0,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
    if keep_above {
        keep_on_top(hwnd)?;
    }
    Ok(())
}

/// WindowsPlatform.SetInputRegion: the window region is where the pet is drawn; of that, only `input` takes the mouse.
pub fn set_region(hwnd: HWND, input: &[PxRect], drawn: &[PxRect]) -> Result<(), NativeError> {
    REGION.with(|r| *r.borrow_mut() = Some(input.to_vec()));
    // SAFETY: plain GDI and user32 calls on regions we made; once SetWindowRgn succeeds the system owns the region
    unsafe {
        let rgn = CreateRectRgn(0, 0, 0, 0);
        if rgn == 0 {
            return Err(NativeError::Os("CreateRectRgn failed".into()));
        }
        for r in drawn {
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
    // the pointer may be still while the region changes under it
    update_hit_test(hwnd);
    Ok(())
}

/// WindowsPlatform.KeepOnTop.
pub fn keep_on_top(hwnd: HWND) -> Result<(), NativeError> {
    // SAFETY: plain user32 call
    if unsafe { SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) } == 0 {
        return Err(NativeError::Os("SetWindowPos failed".into()));
    }
    Ok(())
}

pub fn pointer(scale: f32) -> Result<Pointer, NativeError> {
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: plain user32 call writing to a local
    if unsafe { GetCursorPos(&mut p) } == 0 {
        return Err(NativeError::Os("GetCursorPos failed".into()));
    }
    Ok(Pointer {
        // physical pixels: winit makes the process per-monitor DPI aware
        at: Point::new(p.x as f32 / scale, p.y as f32 / scale),
        left_held: primary_held(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUBBLE: PxRect = PxRect {
        x: 10,
        y: 20,
        w: 100,
        h: 50,
    };

    #[test]
    fn the_mouse_goes_through_outside_the_input_region_and_is_taken_inside_it() {
        let input = Some(&[BUBBLE][..]);
        // a tooltip or a shadow: in the window region, as it is drawn, but not in the input region
        assert!(ignores_mouse(input, 60, 80, false, false));
        assert!(ignores_mouse(input, 9, 20, false, false));
        assert!(ignores_mouse(input, 110, 20, false, false), "the right edge is out");
        // back on the bubble the window takes it again, from its top-left pixel to its bottom-right one
        assert!(!ignores_mouse(input, 10, 20, true, false));
        assert!(!ignores_mouse(input, 109, 69, true, false));
        assert!(!ignores_mouse(input, 60, 40, true, true));
    }

    #[test]
    fn a_held_button_never_starts_letting_the_mouse_through_but_does_not_stop_it() {
        let input = Some(&[BUBBLE][..]);
        // a drag that left the region keeps the mouse
        assert!(!ignores_mouse(input, 200, 200, false, true));
        // a press while the mouse goes through is another window's
        assert!(ignores_mouse(input, 200, 200, true, true));
    }

    #[test]
    fn a_scale_change_waits_for_the_drop_and_only_the_last_is_applied_once() {
        let mut dpi = DpiHold { winit: 144, held: None };
        // over the edge, back and over again while held: winit keeps 144 throughout
        assert!(!dpi.changed(96, true));
        assert!(!dpi.changed(144, true));
        assert!(!dpi.changed(96, true));
        assert_eq!(dpi.winit, 144);
        assert_eq!(dpi.dropped(), Some((144, 96)));
        assert_eq!(dpi, DpiHold { winit: 96, held: None });
        assert_eq!(dpi.dropped(), None, "applied once");
        // dropped back where it started: nothing to apply
        assert!(!dpi.changed(144, true));
        assert!(!dpi.changed(96, true));
        assert_eq!(dpi.dropped(), None);
        // with no button held winit gets it at once, and one that waited is dropped
        assert!(!dpi.changed(144, true));
        assert!(dpi.changed(120, false));
        assert_eq!(dpi, DpiHold { winit: 120, held: None });
    }

    #[test]
    fn at_the_drop_the_window_rescales_about_the_pointer() {
        let rect = |left, top, right, bottom| RECT {
            left,
            top,
            right,
            bottom,
        };
        let edges = |r: RECT| (r.left, r.top, r.right, r.bottom);
        // the pet's 380 × 600 window at 100 %, grabbed 190 px in and 500 down, dropped on a 150 % screen: 570 × 900,
        // the grabbed spot 285 px in and 750 down, under the same pointer
        let small = rect(2400, 300, 2780, 900);
        let pointer = POINT { x: 2590, y: 800 };
        let big = scaled_about(small, pointer, 96, 144);
        assert_eq!(edges(big), (2305, 50, 2875, 950));
        assert_eq!(edges(scaled_about(big, pointer, 144, 96)), edges(small), "and back");
        assert_eq!(edges(scaled_about(small, pointer, 96, 96)), edges(small));
        // 125 %: 475 × 750, the grabbed spot 237.5 px in, rounded away from the pointer
        assert_eq!(edges(scaled_about(small, pointer, 96, 120)), (2352, 175, 2827, 925));
    }

    #[test]
    fn until_a_region_is_set_the_whole_window_takes_the_mouse_and_an_empty_one_takes_none() {
        assert!(!ignores_mouse(None, 500, 500, false, false));
        assert!(!ignores_mouse(None, 500, 500, true, false));
        assert!(ignores_mouse(Some(&[]), 0, 0, false, false));
    }
}
