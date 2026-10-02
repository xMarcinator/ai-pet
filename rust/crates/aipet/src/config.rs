//! The pet's `config.json` in memory: read once at start, then changed in one of two ways, each of which notes
//! whether anything changed (the dirty flag):
//! - [`Store::change`], for the settings (`Pills`, `OnTop`, `Avatar`, `Music`): written at once;
//! - [`Store::keep`], for the window's place: kept until the next [`Store::flush`], which a drop asks for at once and
//!   the quit at the end (`placement`).
//!
//! Only a real change is ever written: starting and quitting without one write nothing, so a missing or corrupt file
//! stays as it is until the user changes something (the spec's config-write rule, R10).

use std::io;
use std::path::PathBuf;

use aipet_core::config::Config;

/// `config.json`, as the pet has it now.
#[derive(Debug)]
pub struct Store {
    path: PathBuf,
    config: Config,
    /// Changed since it was read or last written.
    dirty: bool,
}

impl Store {
    /// The file at `path`, read as the .NET app reads it: the defaults when it is missing or can't be read, and the
    /// file is left as it is.
    pub fn load(path: PathBuf) -> Store {
        let (config, _) = Config::load_from(&path);
        Store {
            path,
            config,
            dirty: false,
        }
    }

    pub fn get(&self) -> &Config {
        &self.config
    }

    /// Changes a setting, and writes the config now if that changed anything.
    pub fn change(&mut self, change: impl FnOnce(&mut Config)) -> io::Result<()> {
        self.keep(change);
        self.flush()
    }

    /// Changes the config without writing it: a change that changes nothing leaves it as it is, and a real one makes
    /// it dirty, for the next [`Store::flush`] to write.
    pub fn keep(&mut self, change: impl FnOnce(&mut Config)) {
        let before = self.config.clone();
        change(&mut self.config);
        if self.config != before {
            self.dirty = true;
        }
    }

    /// Writes the config if it is dirty. It stays dirty when the write fails, so the next flush tries again.
    pub fn flush(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        self.config.save_to(&self.path)?;
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use aipet_core::config::LoadStatus;

    use super::*;

    /// A folder of the test's own, removed with it.
    pub(crate) struct Scratch(PathBuf);

    impl Scratch {
        pub(crate) fn new(test: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("aipet-config-{}-{test}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        pub(crate) fn file(&self) -> PathBuf {
            self.0.join("config.json")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_change_that_changes_nothing_writes_nothing() {
        let scratch = Scratch::new("no-change");
        let mut store = Store::load(scratch.file());
        store.change(|c| c.pills = true).unwrap();
        store.keep(|c| c.window_height = 420.0);
        store.flush().unwrap();
        assert!(!scratch.file().exists(), "the defaults again are no change");
    }

    #[test]
    fn a_change_is_written_at_once() {
        let scratch = Scratch::new("changed");
        let mut store = Store::load(scratch.file());
        store.change(|c| c.music = false).unwrap();
        let (read, status) = Config::load_from(&scratch.file());
        assert_eq!(status, LoadStatus::Loaded);
        assert_eq!(
            read,
            Config {
                music: false,
                ..Config::default()
            }
        );
    }

    #[test]
    fn a_kept_change_waits_for_the_next_flush() {
        let scratch = Scratch::new("kept");
        let mut store = Store::load(scratch.file());
        store.keep(|c| c.left = Some(100));
        assert!(!scratch.file().exists(), "kept, not written yet");
        store.flush().unwrap();
        assert_eq!(Config::load_from(&scratch.file()).0.left, Some(100));
        // written, so the next flush has nothing to write
        std::fs::remove_file(scratch.file()).unwrap();
        store.flush().unwrap();
        assert!(!scratch.file().exists());
    }
}
