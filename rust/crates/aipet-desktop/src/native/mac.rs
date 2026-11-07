//! macOS: AppKit has no input region, so the pet's is kept here and [`update_hit_test`] makes the window ignore the
//! mouse while the pointer is outside it (MacPlatform's plan). Nothing clips drawing. AppKit is main-thread only;
//! iced runs `window::run` and `update` there.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;

use iced::Point;
use iced::window::raw_window_handle::RawWindowHandle;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSFloatingWindowLevel, NSScreen, NSView, NSWindow,
    NSWindowCollectionBehavior,
};
use objc2_foundation::MainThreadMarker;

use super::{NativeError, Pointer, PxRect};

/// winit's AppKit handle: the window's content NSView.
pub type Handle = NonNull<c_void>;

thread_local! {
    /// The pet window's input region, in physical pixels; `None` (the whole window) until the first is set.
    static REGION: RefCell<Option<Vec<PxRect>>> = const { RefCell::new(None) };
}

fn main_thread() -> Result<MainThreadMarker, NativeError> {
    MainThreadMarker::new().ok_or(NativeError::Unsupported("AppKit is called off the main thread"))
}

fn ns_window(view: Handle) -> Result<Retained<NSWindow>, NativeError> {
    main_thread()?;
    // SAFETY: winit's handle is its NSView, alive while the window is; we are on the main thread
    let view: &NSView = unsafe { view.cast::<NSView>().as_ref() };
    view.window()
        .ok_or(NativeError::Handle("the NSView has no NSWindow".into()))
}

pub fn handle(raw: RawWindowHandle) -> Result<Handle, NativeError> {
    match raw {
        RawWindowHandle::AppKit(h) => Ok(h.ns_view),
        _ => Err(NativeError::Unsupported("the pet window is not an AppKit window")),
    }
}

pub fn setup(view: Handle, keep_above: bool) -> Result<(), NativeError> {
    let w = ns_window(view)?;
    // a borderless transparent window's shadow follows what was drawn, a frame late: none
    w.setHasShadow(false);
    // on every Space and over full-screen apps, and not in Cmd+` window cycling
    // SAFETY: a plain property setter
    unsafe {
        w.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
    }
    if keep_above {
        keep_on_top(view)?;
    }
    // no Dock icon or Cmd+Tab entry: the app is an accessory (its Settings window still opens)
    let _ =
        NSApplication::sharedApplication(main_thread()?).setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    Ok(())
}

pub fn set_region(view: Handle, input: &[PxRect], _drawn: &[PxRect]) -> Result<(), NativeError> {
    let w = ns_window(view)?;
    REGION.with(|r| *r.borrow_mut() = Some(input.to_vec()));
    update(&w);
    Ok(())
}

pub fn keep_on_top(view: Handle) -> Result<(), NativeError> {
    // what winit's WindowLevel::AlwaysOnTop sets
    ns_window(view)?.setLevel(NSFloatingWindowLevel);
    Ok(())
}

pub fn update_hit_test(view: Handle) -> Result<(), NativeError> {
    let w = ns_window(view)?;
    update(&w);
    Ok(())
}

fn update(w: &NSWindow) {
    // the pointer in the window's coordinates (points, from the bottom left), even while it ignores the mouse
    // SAFETY: plain getters on a live window, on the main thread
    let (p, ignoring, buttons) = unsafe {
        (
            w.mouseLocationOutsideOfEventStream(),
            w.ignoresMouseEvents(),
            NSEvent::pressedMouseButtons(),
        )
    };
    let height = w
        .contentView()
        .map_or_else(|| w.frame().size.height, |v| v.frame().size.height);
    let scale = w.backingScaleFactor();
    let (x, y) = (p.x * scale, (height - p.y) * scale);
    let inside = REGION.with(|r| {
        r.borrow().as_ref().is_none_or(|rects| {
            rects.iter().any(|r| {
                x >= f64::from(r.x) && y >= f64::from(r.y) && x < f64::from(r.x + r.w) && y < f64::from(r.y + r.h)
            })
        })
    });
    // never start ignoring while a button is held (a drag that left the region)
    let ignore = !inside && (ignoring || buttons == 0);
    if ignore != ignoring {
        w.setIgnoresMouseEvents(ignore);
    }
}

pub fn pointer(_scale: f32) -> Result<Pointer, NativeError> {
    let mtm = main_thread()?;
    // AppKit's desktop is in points (winit's logical px already), from the bottom left of the primary screen, which
    // is the first one; winit's positions are from its top left
    // SAFETY: plain getters, on the main thread
    let (p, primary, buttons) = unsafe {
        (
            NSEvent::mouseLocation(),
            NSScreen::screens(mtm).firstObject(),
            NSEvent::pressedMouseButtons(),
        )
    };
    let primary = primary.ok_or_else(|| NativeError::Os("AppKit lists no screen".into()))?;
    Ok(Pointer {
        at: Point::new(p.x as f32, (primary.frame().size.height - p.y) as f32),
        // bit 0: the left button
        left_held: buttons & 1 != 0,
    })
}
