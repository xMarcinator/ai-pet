//! The pet on Wayland compositors with layer-shell (Hyprland, Sway, KDE…): a layer surface above the windows with
//! an exact input region, dragged by its margins or over an output-sized surface, with its menu in an xdg popup and
//! Settings in an xdg window, through iced_exwlshell. Elsewhere (GNOME, X11, Windows, macOS) the desktop shell runs.
//!
//! `AIPET_DRAG=margin|overlay` picks how a drag moves the pet (margin by default, see `drag`),
//! `AIPET_OPEN_SETTINGS=1` opens Settings at start, and `AIPET_DEBUG=1` logs the surfaces, presses and drags.

#[cfg(target_os = "linux")]
mod drag;
#[cfg(target_os = "linux")]
mod probe;
#[cfg(target_os = "linux")]
mod shell;

/// Whether the session's compositor can host the pet: a Wayland connection whose registry offers everything
/// iced_exwlshell needs, layer-shell (v3+), viewporter and xdg-shell among them. It never panics; off Linux it is
/// always false.
pub fn available() -> bool {
    #[cfg(target_os = "linux")]
    return probe::connect().is_ok();
    #[cfg(not(target_os = "linux"))]
    return false;
}

/// Runs the pet until it quits.
pub fn run() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    return shell::run(probe::connect()?);
    #[cfg(not(target_os = "linux"))]
    return Err("the Wayland shell only runs on Linux".to_owned());
}
