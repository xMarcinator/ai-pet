//! The macOS platform (`src/AiPet.UI/Platform/MacPlatform.cs`): a stub, as in the C#. macOS isn't a release target;
//! this keeps it compiling.

use std::path::Path;

use aipet_ui::platform::{MediaPlayer, Platform};

#[derive(Debug, Default)]
pub struct Mac {}

impl Mac {
    pub fn new() -> Mac {
        Mac::default()
    }
}

impl Platform for Mac {
    fn open_url(&self, _url: &str) {}

    fn open_folder(&self, _path: &Path) {}

    fn focus_agent(&self, _agent: &str, _link: Option<&str>) {}

    fn media(&self) -> Option<&dyn MediaPlayer> {
        None
    }
}
