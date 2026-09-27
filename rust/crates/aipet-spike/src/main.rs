//! The spike's entry point: picks a shell and runs the pet in it.
//!
//! `AIPET_BACKEND=wayland|desktop` chooses; otherwise the Wayland shell runs when the session is Wayland and its
//! compositor has layer-shell, and the desktop shell everywhere else (through XWayland in a Wayland session).
//! `AIPET_DEBUG=1` says which shell runs and makes it log pointer events and input-region updates to stderr;
//! `AIPET_OPEN_SETTINGS=1` opens Settings at start (desktop shell). The pet renders on the low-power GPU unless
//! `WGPU_POWER_PREF` says otherwise and, on Linux, through GL unless `WGPU_BACKEND` does.

use std::env;
use std::process::ExitCode;

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
    let wayland_session = env::var_os("WAYLAND_DISPLAY").is_some();
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
        aipet_wayland::run()
            .map_err(|e| format!("the Wayland shell failed: {e} (AIPET_BACKEND=desktop runs the pet through XWayland)"))
    } else {
        if debug {
            eprintln!("aipet: the desktop shell ({why})");
        }
        if wayland_session {
            // winit on Wayland can neither place the window nor keep it on top, so the pet goes through XWayland
            // SAFETY: nothing else runs yet: no other thread reads or writes the environment (the probe above
            // starts none)
            unsafe { env::remove_var("WAYLAND_DISPLAY") };
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
