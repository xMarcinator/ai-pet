//! The pet's `config.json` in memory: read once at start, changed through [`Store::change`], which notes whether
//! anything changed (the dirty flag) and for now writes the change at once.
//!
//! Starting and quitting never write it, so a missing or corrupt file stays as it is until the user changes something
//! (the spec's config-write rule, R10). Still to come: the position is kept here after a drop and written on the
//! next drop or on quit, while the settings go on being written at once.

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

    /// Changes the config. A change that changes nothing leaves it as it is; a real one makes it dirty, and is
    /// written now.
    pub fn change(&mut self, change: impl FnOnce(&mut Config)) -> io::Result<()> {
        let before = self.config.clone();
        change(&mut self.config);
        if self.config != before {
            self.dirty = true;
        }
        self.flush()
    }

    /// Writes the config if it is dirty.
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
mod tests {
    use aipet_core::config::LoadStatus;

    use super::*;

    /// A folder of the test's own, removed with it.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("aipet-config-{}-{test}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn file(&self) -> PathBuf {
            self.0.join("config.json")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn reading_and_flushing_without_a_change_write_nothing() {
        let scratch = Scratch::new("untouched");
        // missing: the defaults, and still no file
        let mut store = Store::load(scratch.file());
        assert_eq!(store.get(), &Config::default());
        store.flush().unwrap();
        store.change(|c| c.pills = true).unwrap();
        assert!(!scratch.file().exists(), "the default again is no change");
        // corrupt: the defaults, and the file stays as it is
        std::fs::write(scratch.file(), "{not json").unwrap();
        let mut store = Store::load(scratch.file());
        assert_eq!(store.get(), &Config::default());
        store.flush().unwrap();
        assert_eq!(std::fs::read_to_string(scratch.file()).unwrap(), "{not json");
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
}
