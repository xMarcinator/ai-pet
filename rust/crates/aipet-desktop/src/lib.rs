//! The pet in an ordinary transparent, always-on-top window (iced on winit), for everywhere the Wayland shell can't
//! run: X11 and XWayland, Windows, macOS.
//!
//! On Linux it runs through X11. In a Wayland session the caller removes `WAYLAND_DISPLAY` before any thread starts,
//! so that winit takes XWayland: winit's own Wayland windows can neither place themselves nor stay on top.

mod native;
mod shell;

/// Runs the pet until it quits.
pub fn run() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return Err("the desktop shell runs through X11: remove WAYLAND_DISPLAY from its environment".to_owned());
        }
        native::connect_x11().map_err(|e| e.to_string())?;
    }
    shell::run()
}
