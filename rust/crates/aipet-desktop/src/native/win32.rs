//! Windows: WindowsPlatform's calls. A window region clips hit-testing and drawing alike, so the region is the
//! padded `drawn` one; winit draws an undecorated window's client area over the whole window, so the region's
//! window-relative pixels are the client area's.

use iced::Point;
use iced::window::raw_window_handle::RawWindowHandle;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CombineRgn, CreateRectRgn, DeleteObject, RGN_OR, SetWindowRgn};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetCursorPos, GetWindowLongW, HWND_TOPMOST, STYLESTRUCT, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SetWindowLongW, SetWindowPos, WM_NCDESTROY, WM_STYLECHANGING, WS_EX_APPWINDOW,
    WS_EX_TOOLWINDOW,
};

use super::{NativeError, PxRect};

pub type Handle = HWND;

const SUBCLASS_ID: usize = 0x4150_4554; // "APET"

pub fn handle(raw: RawWindowHandle) -> Result<Handle, NativeError> {
    match raw {
        RawWindowHandle::Win32(h) => Ok(h.hwnd.get()),
        _ => Err(NativeError::Unsupported("the pet window is not a Win32 window")),
    }
}

/// winit rebuilds the whole extended style from its own flags whenever one of them changes (the level, the
/// visibility…), which would drop WS_EX_TOOLWINDOW and bring back WS_EX_APPWINDOW. This puts them back on every
/// change.
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
        // SAFETY: removing our own subclass from the window it was set on
        unsafe { RemoveWindowSubclass(hwnd, Some(keep_tool_window), SUBCLASS_ID) };
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

/// WindowsPlatform.SetInputRegion: the window region is where the pet is drawn and can be clicked.
pub fn set_region(hwnd: HWND, _input: &[PxRect], drawn: &[PxRect]) -> Result<(), NativeError> {
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

pub fn pointer(scale: f32) -> Result<Point, NativeError> {
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: plain user32 call writing to a local
    if unsafe { GetCursorPos(&mut p) } == 0 {
        return Err(NativeError::Os("GetCursorPos failed".into()));
    }
    // physical pixels: winit makes the process per-monitor DPI aware
    Ok(Point::new(p.x as f32 / scale, p.y as f32 / scale))
}
