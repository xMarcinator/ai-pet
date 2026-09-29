//! The fallback without a keyring: `secrets.json` in the data folder, mode 0600 (`LinuxPlatform.cs:270-330`).

use std::fs::{self, OpenOptions};
use std::hash::{BuildHasher, RandomState};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::SecretStore;
use crate::config::{Dictionary, read_dictionary, read_text, set, write_dictionary};

/// `secrets.json`: a JSON object of key and secret, as `SecretTool` keeps it when there is no keyring. It is only
/// ever written whole, to a new file that is 0600 from the start, and renamed over the old one, so no one else can
/// have it open from a moment it was readable.
#[derive(Clone, Debug)]
pub struct FileSecrets {
    file: PathBuf,
}

impl FileSecrets {
    /// `secrets.json` in this data folder.
    pub fn new(data_dir: &Path) -> Self {
        FileSecrets {
            file: data_dir.join("secrets.json"),
        }
    }

    /// The file's secrets; `None` for `null`. An error for a file that is missing or that can't be read.
    fn all(&self) -> Result<Option<Dictionary>, ()> {
        let bytes = fs::read(&self.file).map_err(|_| ())?;
        read_dictionary(&read_text(&bytes)).map_err(|_| ())
    }

    fn save(&self, all: &Dictionary) -> io::Result<()> {
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut tmp = self.file.clone().into_os_string();
        tmp.push(format!(".{}.tmp", random_suffix()));
        let tmp = PathBuf::from(tmp);
        let saved = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            let mut file = options.open(&tmp)?;
            file.write_all(write_dictionary(all).as_bytes())?;
            drop(file);
            fs::rename(&tmp, &self.file)
        })();
        if saved.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        saved
    }
}

impl SecretStore for FileSecrets {
    /// A file that is missing, can't be read or holds something else than a JSON object of strings has none.
    fn read(&self, key: &str) -> Option<String> {
        self.all().ok()??.into_iter().find(|(k, _)| k == key)?.1
    }

    /// Saves the secret in the file, over whatever the file held when that isn't a JSON object of strings.
    fn write(&self, key: &str, _user: &str, secret: &str) -> io::Result<()> {
        let mut all = self.all().ok().flatten().unwrap_or_default();
        set(&mut all, key, Some(secret.into()));
        self.save(&all)
    }

    /// Takes the key out of the file, and deletes the file when nothing is left in it. A file it can't read is left
    /// as it is.
    fn delete(&self, key: &str) {
        if let Ok(Some(mut all)) = self.all()
            && let Some(at) = all.iter().position(|(k, _)| k == key)
        {
            all.remove(at);
            let _ = if all.is_empty() {
                fs::remove_file(&self.file)
            } else {
                self.save(&all)
            };
        }
    }
}

/// Eight hex digits, new each time (the C# takes a GUID's first eight).
fn random_suffix() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let n = RandomState::new().hash_one((std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
    format!("{:08x}", n as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> Dir {
        let dir = std::env::temp_dir().join(format!("aipet-core-secrets-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Dir(dir)
    }

    #[test]
    fn a_new_file_is_written_whole_and_only_the_file_is_left() {
        let d = dir("whole");
        let store = FileSecrets::new(&d.0.join("data"));
        store.write("AiPet:Jira", "ann", "one").unwrap();
        store.write("AiPet:GitHub", "github", "two").unwrap();
        let names: Vec<_> = fs::read_dir(d.0.join("data"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["secrets.json"]);
        assert_eq!(store.read("AiPet:Jira").as_deref(), Some("one"));
        store.delete("AiPet:Jira");
        store.delete("AiPet:GitHub");
        assert!(!d.0.join("data").join("secrets.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_the_users_only() {
        use std::os::unix::fs::PermissionsExt;
        let d = dir("mode");
        let store = FileSecrets::new(&d.0);
        store.write("AiPet:Jira", "ann", "secret").unwrap();
        let mode = fs::metadata(d.0.join("secrets.json")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_failed_write_leaves_no_temp_file() {
        let d = dir("failed");
        fs::create_dir_all(d.0.join("secrets.json")).unwrap();
        let store = FileSecrets::new(&d.0);
        assert!(store.write("AiPet:Jira", "ann", "secret").is_err());
        let names: Vec<_> = fs::read_dir(&d.0).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["secrets.json"]);
    }
}
