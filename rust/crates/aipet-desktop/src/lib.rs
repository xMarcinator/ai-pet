//! The pet in an ordinary transparent, always-on-top window (iced on winit), for everywhere the Wayland shell can't
//! run: X11 and XWayland, Windows, macOS. [`platform`] is the operating system's services, which the Wayland shell
//! uses too.
//!
//! On Linux and the BSDs it runs through X11, so the caller empties `WAYLAND_DISPLAY` and removes `WAYLAND_SOCKET`
//! before any thread starts. winit then takes X11 even in a Wayland session (its own Wayland windows can neither
//! place themselves nor stay on top), and libwayland can't connect at all. Without `WAYLAND_DISPLAY` it would try
//! `wayland-0`, and if that connects (GNOME's socket), wgpu's EGL renders for Wayland and panics on the X11 window.

mod native;
pub mod platform;
mod shell;

use aipet_ui::Launch;

/// Runs the pet until it quits.
pub fn run(launch: Launch) -> Result<(), String> {
    #[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
    {
        let x11_only = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| v.is_empty())
            && std::env::var_os("WAYLAND_SOCKET").is_none();
        if !x11_only {
            return Err(
                "the desktop shell runs through X11: it needs an empty WAYLAND_DISPLAY, and no WAYLAND_SOCKET".into(),
            );
        }
        native::connect_x11().map_err(|e| e.to_string())?;
    }
    shell::run(launch)
}
