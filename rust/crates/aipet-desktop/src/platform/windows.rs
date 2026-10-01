//! The Windows platform (`src/AiPet.UI/Platform/WindowsPlatform.cs`): an agent's windows found by their exe and brought
//! forward, links and folders opened with `ShellExecuteW`, and Spotify through its window. Still a stub: it does
//! nothing, and has no player.

use std::path::Path;

use aipet_ui::platform::{MediaPlayer, Platform};

#[derive(Debug, Default)]
pub struct Windows {}

impl Windows {
    pub fn new() -> Windows {
        Windows::default()
    }
}

impl Platform for Windows {
    fn open_url(&self, _url: &str) {}

    fn open_folder(&self, _path: &Path) {}

    fn focus_agent(&self, _agent: &str, _link: Option<&str>) {}

    fn media(&self) -> Option<&dyn MediaPlayer> {
        None
    }
}
