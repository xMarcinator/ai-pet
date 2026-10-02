//! The macOS platform (`src/AiPet.UI/Platform/MacPlatform.cs`): a stub, as in the C#. Links and folders open with
//! `open`, no app is brought forward, and there is no player. macOS isn't a release target; this keeps it compiling.

use std::path::Path;
use std::process::Command;

use aipet_ui::platform::{MediaPlayer, Platform};

#[derive(Debug, Default)]
pub struct Mac {}

impl Mac {
    pub fn new() -> Mac {
        Mac::default()
    }
}

impl Platform for Mac {
    fn open_url(&self, url: &str) {
        super::start(Command::new("open").arg(url));
    }

    /// The folder is one argument: the C#'s `Process.Start("open", path)` split it at its spaces, and the data
    /// folder's path has one (`Application Support`).
    fn open_folder(&self, path: &Path) {
        super::start(Command::new("open").arg(path));
    }

    /// Nothing comes forward: at most the chat's link opens.
    fn focus_agent(&self, _agent: &str, link: Option<&str>) {
        if let Some(link) = link {
            self.open_url(link);
        }
    }

    fn media(&self) -> Option<&dyn MediaPlayer> {
        None
    }
}
