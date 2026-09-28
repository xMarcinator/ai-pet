//! The spike's entry point: picks a shell and runs the pet in it.
//!
//! `AIPET_BACKEND=wayland|desktop` chooses; otherwise the Wayland shell runs when the session is Wayland and its
//! compositor has layer-shell, and the desktop shell everywhere else (through XWayland in a Wayland session).
//! `AIPET_DEBUG=1` says which shell runs and makes it log pointer events and input-region updates to stderr;
//! `AIPET_OPEN_SETTINGS=1` opens Settings at start (both shells). The pet renders on the low-power GPU unless
//! `WGPU_POWER_PREF` says otherwise and, on Linux, through GL unless `WGPU_BACKEND` does.

use std::any::Any;
use std::env;
use std::process::ExitCode;

/// Whether the desktop shell runs through X11 here: where winit has its X11 backend (Linux and the BSDs).
const X11: bool = cfg!(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
));

fn main() -> ExitCode {
    if env::var_os("WGPU_POWER_PREF").is_none() {
        // a desktop pet has no business waking a discrete GPU (iced asks wgpu for the fastest one). It also dodges
        // an NVIDIA quirk: its Wayland surfaces offer an fp16 format first, which iced (with web colours) picks and
        // then fills with sRGB values the compositor takes for linear ones, washing every colour out.
        // SAFETY: nothing else runs yet: no other thread reads or writes the environment
        unsafe { env::set_var("WGPU_POWER_PREF", "low") };
    }
    if cfg!(target_os = "linux") && env::var_os("WGPU_BACKEND").is_none() {
        // GL only, in either shell: to list Vulkan adapters the loader opens every driver, and NVIDIA's keeps a
        // hybrid laptop's discrete GPU awake the whole time the pet runs, though the pet renders on the integrated
        // one. Through EGL the pet renders with the driver of the GPU the display server uses, on a transparent
        // surface (a layer surface, or XWayland's 32-bit window), and shows at once instead of 2 s later.
        // SAFETY: nothing else runs yet: no other thread reads or writes the environment
        unsafe { env::set_var("WGPU_BACKEND", "gl") };
    }
    // winit's test (an empty value is none). A session with only WAYLAND_SOCKET isn't probed: that socket connects
    // once, and the probe would use it up; AIPET_BACKEND=wayland takes it.
    let wayland_session = env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    let (wayland, why) = match env::var("AIPET_BACKEND").ok().as_deref() {
        Some("wayland") => (true, "AIPET_BACKEND"),
        Some("desktop") => (false, "AIPET_BACKEND"),
        Some(other) => {
            eprintln!("aipet: AIPET_BACKEND must be wayland or desktop, not {other:?}");
            return ExitCode::FAILURE;
        }
        None if !wayland_session => (false, "not a Wayland session"),
        None if aipet_wayland::available() => (true, "the compositor has layer-shell"),
        None => (false, "no layer-shell compositor to connect to"),
    };
    let debug = env::var_os("AIPET_DEBUG").is_some_and(|v| v == "1");
    let result = if wayland {
        if debug {
            eprintln!("aipet: the Wayland shell ({why})");
        }
        // iced_exwlshell panics where it could fail (without a wgpu adapter for the surface), so a panic is a
        // failure too, said after the panic's own message
        std::panic::catch_unwind(aipet_wayland::run)
            .unwrap_or_else(|panic| Err(panic_text(&*panic).to_owned()))
            .map_err(|e| format!("the Wayland shell failed: {e} (AIPET_BACKEND=desktop runs the pet through XWayland)"))
    } else {
        if debug {
            eprintln!("aipet: the desktop shell ({why})");
        }
        if X11 {
            // X11, even in a Wayland session (see aipet_desktop): winit takes an empty WAYLAND_DISPLAY as none, and
            // libwayland, which would try wayland-0 without one, can't connect
            // SAFETY: nothing else runs yet: no other thread reads or writes the environment (the probe above
            // starts none)
            unsafe {
                env::set_var("WAYLAND_DISPLAY", "");
                env::remove_var("WAYLAND_SOCKET");
            }
        }
        aipet_desktop::run().map_err(|e| format!("the desktop shell failed: {e}"))
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("aipet: {e}");
            ExitCode::FAILURE
        }
    }
}

/// What a panic said: the message `panic!` and `expect` give it, a `&str` or a `String`.
fn panic_text(payload: &(dyn Any + Send)) -> &str {
    match (payload.downcast_ref::<&str>(), payload.downcast_ref::<String>()) {
        (Some(text), _) => text,
        (_, Some(text)) => text,
        _ => "it panicked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_says_what_it_panicked_with() {
        let literal: Box<dyn Any + Send> = Box::new("no adapter");
        let formatted: Box<dyn Any + Send> =
            Box::new(format!("Cannot create compositor: {:?}", "GraphicsAdapterNotFound"));
        let other: Box<dyn Any + Send> = Box::new(7);
        assert_eq!(panic_text(&*literal), "no adapter");
        assert_eq!(
            panic_text(&*formatted),
            "Cannot create compositor: \"GraphicsAdapterNotFound\""
        );
        assert_eq!(panic_text(&*other), "it panicked");
    }
}
