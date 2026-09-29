//! The update status texts Settings shows (`src/AiPet.Core/UpdateStatus.cs`).
//!
//! Only the copy the Windows installer put there updates itself (with Velopack); any other copy is
//! [`Kind::Off`] and never checks.

use std::path::Path;
use std::time::Duration;

/// The releases the installed app updates from: stable ones only, read without a token.
pub const REPO: &str = "https://github.com/xMarcinator/ai-pet";
/// The Velopack channel the Windows app is packed for: its feed is `releases.win.json` in each release.
pub const CHANNEL: &str = "win";
/// The first check comes a while after the pet starts, and the next ones every few hours: unauthenticated GitHub
/// API calls are limited per IP address (60 an hour, shared with everything else on the network), and each check
/// makes one.
pub const FIRST_CHECK: Duration = Duration::from_secs(3 * 60);
pub const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// Where the update stands (`UpdateStatus.Kinds`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Off,
    Idle,
    Checking,
    UpToDate,
    Downloading,
    Ready,
    Failed,
}

/// The app's own update, as Settings shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateStatus {
    pub kind: Kind,
    /// The version found (Downloading, Ready, and Failed while updating).
    pub version: Option<String>,
    pub percent: i32,
    pub error: Option<String>,
}

impl UpdateStatus {
    pub fn new(kind: Kind) -> Self {
        UpdateStatus {
            kind,
            version: None,
            percent: 0,
            error: None,
        }
    }

    /// Whether the app in `app_dir` is the installed one: Velopack's installer puts `Update.exe` in the folder above
    /// the app's (`%LOCALAPPDATA%\AiPetApp\current`). The portable zip and a build run from source have none there.
    pub fn installed(app_dir: &Path) -> bool {
        app_dir.join("..").join("Update.exe").is_file()
    }

    pub fn busy(&self) -> bool {
        matches!(self.kind, Kind::Checking | Kind::Downloading)
    }

    pub fn text(&self) -> String {
        let version = self.version.as_deref().unwrap_or("");
        let error = self.error.as_deref().unwrap_or("");
        match self.kind {
            Kind::Off => "This copy doesn't update itself. To update, install AiPet again.".into(),
            Kind::Idle => "Checks for updates a few minutes after the pet starts, then every few hours.".into(),
            Kind::Checking => "Checking for updates…".into(),
            Kind::UpToDate => "Up to date.".into(),
            Kind::Downloading => format!("Downloading AiPet {version}… {}%", self.percent),
            Kind::Ready => format!("AiPet {version} is ready. It's installed when you quit the pet, or restart now."),
            Kind::Failed if self.version.is_none() => format!("Couldn't check for updates: {error}"),
            Kind::Failed => format!("Couldn't update to AiPet {version}: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// VelopackTests.Installed_IsTheCopyWithUpdateExeAbove.
    #[test]
    fn installed_is_the_copy_with_update_exe_above() {
        let root = std::env::temp_dir()
            .join(format!("aipet-core-update-{}", std::process::id()))
            .join("AiPetApp");
        let current = root.join("current");
        fs::create_dir_all(&current).unwrap();
        let with_separator = current.join("");
        assert!(!UpdateStatus::installed(&with_separator));
        fs::write(root.join("Update.exe"), "").unwrap();
        assert!(UpdateStatus::installed(&with_separator));
        assert!(UpdateStatus::installed(&current));
        assert!(!UpdateStatus::installed(&root));
        fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }
}
