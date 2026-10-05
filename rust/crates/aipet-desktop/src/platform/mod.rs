//! The pet's operating-system services on the desktop ([`Platform`]), which both shells use: the C#'s
//! `WindowsPlatform`, `LinuxPlatform` and `MacPlatform` (`src/AiPet.UI/Platform/`), a file each:
//! - Windows (`windows.rs`): an agent's windows found by their exe with `EnumWindows` and brought forward with
//!   `SetForegroundWindow` (with `AttachThreadInput` when that fails), `ShellExecuteW` for links and folders, and
//!   Spotify through its window's title and `WM_APPCOMMAND`;
//! - Linux (`linux.rs`): xdotool, else wmctrl, to bring a window forward, xdg-open for links and folders, and MPRIS
//!   through playerctl, else dbus-send;
//! - macOS (`mac.rs`): a stub, as in the C#.
//!
//! The Windows and Linux platforms work through a seam (`windows::Win32`, `linux::Tools`): the system calls they make
//! and the tools they run. Their logic compiles on every OS, so the crate's tests (`tests/platform.rs`) drive both
//! over fakes everywhere; only Windows' own calls are Windows-only, and [`new`] picks this OS's platform. Elsewhere
//! (the BSDs) `NoPlatform` stands in. The modules are public so those tests can reach what they make public.

pub mod linux;
#[cfg(target_os = "macos")]
pub mod mac;
pub mod windows;

use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
use aipet_ui::platform::NoPlatform;
use aipet_ui::platform::{Media, Platform};

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

/// What a music player last showed (`MediaPlayerBase`'s Song, Artist, Playing and TrackSince).
#[derive(Debug, Default)]
pub(crate) struct Track {
    song: Option<String>,
    artist: Option<String>,
    playing: bool,
    /// When the song showing started (Unix seconds).
    since: f64,
}

impl Track {
    /// `MediaPlayerBase.Set`: another song or artist starts the track's clock, when there is a song.
    pub(crate) fn set(&mut self, song: Option<String>, artist: Option<String>, playing: bool, now: f64) {
        if (song != self.song || artist != self.artist) && song.is_some() {
            self.since = now;
        }
        self.song = song;
        self.artist = artist;
        self.playing = playing;
    }

    /// A pause keeps the last track (`Set(Song, Artist, false)`).
    pub(crate) fn pause(&mut self) {
        self.playing = false;
    }

    /// The track as the Board reads it, from the player called `name`.
    pub(crate) fn media(&self, name: &str) -> Media {
        Media {
            name: name.to_owned(),
            song: self.song.clone(),
            artist: self.artist.clone().unwrap_or_default(),
            playing: self.playing,
            track_since: self.since,
        }
    }
}

/// Now in Unix seconds, to the millisecond (`DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() / 1000.0`).
pub(crate) fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64 / 1000.0)
}

/// Starts a program and leaves it running (`Sh.Start`): nothing waits for it, but a thread of its own reaps it once
/// it ends, so it never lingers as a zombie. One that can't start is let go, as the C# ignores it.
pub(crate) fn start(command: &mut Command) {
    if let Ok(mut child) = command.spawn() {
        let _ = thread::Builder::new()
            .name("aipet-started".into())
            .spawn(move || child.wait());
    }
}

/// The lock's data, also after a panic poisoned it: the platforms' locks only guard what they last saw, which the
/// next look puts right.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
