//! The Linux platform (`src/AiPet.UI/Platform/LinuxPlatform.cs`), for X11 and Wayland sessions alike: xdotool, else
//! wmctrl, to bring an agent's window forward, xdg-open for links and folders, and MPRIS for the player. Still a stub:
//! it does nothing, and has no player.

use std::path::Path;

use aipet_ui::platform::{MediaPlayer, Platform};

#[derive(Debug, Default)]
pub struct Linux {}

impl Linux {
    pub fn new() -> Linux {
        Linux::default()
    }
}

impl Platform for Linux {
    fn open_url(&self, _url: &str) {}

    fn open_folder(&self, _path: &Path) {}

    fn focus_agent(&self, _agent: &str, _link: Option<&str>) {}

    fn media(&self) -> Option<&dyn MediaPlayer> {
        None
    }
}
