//! The pet's operating-system services on the desktop ([`Platform`]), which both shells use: the C#'s
//! `WindowsPlatform`, `LinuxPlatform` and `MacPlatform` (`src/AiPet.UI/Platform/`), a file each:
//! - Windows (`windows.rs`): an agent's windows found by their exe with `EnumWindows` and brought forward with
//!   `SetForegroundWindow` (with `AttachThreadInput` when that fails), `ShellExecuteW` for links and folders, and
//!   Spotify through its window's title and `WM_APPCOMMAND`;
//! - Linux (`linux.rs`): xdotool, else wmctrl, to bring a window forward, xdg-open for links and folders, and MPRIS
//!   through playerctl, else dbus-send;
//! - macOS (`mac.rs`): a stub, as in the C#.
//!
//! Each is still a stub that does nothing and has no player. Elsewhere (the BSDs) `NoPlatform` stands in. The modules
//! are public so the crate's own tests (`tests/`) can reach what they make public.

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod mac;
#[cfg(windows)]
pub mod windows;

use std::sync::Arc;

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
use aipet_ui::platform::NoPlatform;
use aipet_ui::platform::Platform;

/// This operating system's platform.
pub fn new() -> Arc<dyn Platform> {
    #[cfg(windows)]
    return Arc::new(windows::Windows::new());
    #[cfg(target_os = "linux")]
    return Arc::new(linux::Linux::new());
    #[cfg(target_os = "macos")]
    return Arc::new(mac::Mac::new());
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    return Arc::new(NoPlatform);
}
