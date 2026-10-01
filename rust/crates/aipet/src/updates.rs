//! The app's own updates, through Velopack's Rust SDK (`src/AiPet.UI/Updates.cs`): Velopack's startup first of all,
//! the checks once this is the only pet (only in the copy Velopack installed, on Windows), Check for updates and
//! Restart to update in Settings, and the update installed when the pet quits.
//!
//! Still to come: Velopack isn't called yet, so this copy doesn't update itself (its status is Off).

use aipet_core::update_status::{Kind, UpdateStatus};

/// Velopack's startup (`Updates.App().Run()`), the first thing `main` does: the installer, the updater and the
/// uninstaller start the app with arguments of their own, which it handles before it exits. It must leave no thread
/// running: `main` sets the environment after it.
pub fn startup() {}

/// Starts the checks, once this is the only pet (`Updates.Start`). False when an update downloaded earlier is
/// installed first: the pet then exits at once, and the updater starts it again.
pub fn start() -> bool {
    true
}

/// The app's version, as released.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Where the update stands now, for Settings.
pub fn status() -> UpdateStatus {
    UpdateStatus::new(Kind::Off)
}

/// Settings' Check for updates: a check now, off the UI thread.
pub fn check() {}

/// Settings' Restart to update: the update that is ready installs once the pet has quit, and the updater starts it
/// again. True when the pet should now quit.
pub fn restart() -> bool {
    false
}

/// The pet quits: an update that is ready installs once it has (`Updates.InstallOnQuit`).
pub fn install_on_quit() {}
