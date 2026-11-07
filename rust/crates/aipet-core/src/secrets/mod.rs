//! The Jira and GitHub tokens, under the keys `AiPet:Jira` and `AiPet:GitHub`, where the .NET app keeps them
//! (`ISecretStore`, `src/AiPet.Core/Platform.cs`), so a token either app saved is the other's too:
//! - Windows: Credential Manager, a generic credential whose target name is the key ([`CredentialManager`]);
//! - Linux: the Secret Service through `secret-tool`, with the attributes `service=aipet` and `key=<key>`, and
//!   `secrets.json` (0600) in the data folder when that fails ([`SecretTool`], [`FileSecrets`]);
//! - macOS (not supported): kept in memory only, as the C# does ([`MemorySecrets`]).
//!
//! A keyring crate's defaults would name them differently (on Windows `{user}.{service}`, and other Secret Service
//! attributes) and lose every saved token, so the platforms' own interfaces are used.

use std::collections::HashMap;
use std::io;
use std::sync::Mutex;

mod file;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

pub use file::FileSecrets;
#[cfg(target_os = "linux")]
pub use linux::SecretTool;
#[cfg(windows)]
pub use windows::CredentialManager;

/// The Jira API token's key (`JiraWatcher.CredTarget`).
pub const JIRA: &str = "AiPet:Jira";
/// The GitHub token's key (`GitHubWatcher.TokenKey`).
pub const GITHUB: &str = "AiPet:GitHub";

/// Where the tokens are kept.
pub trait SecretStore: Send + Sync {
    /// The secret saved under `key`, if there is one.
    fn read(&self, key: &str) -> Option<String>;
    /// Saves `secret` under `key`, with `user` as its user name where the store keeps one. The .NET app ignores a
    /// failed Credential Manager write, while a failed fallback file reaches its caller; both come back here.
    fn write(&self, key: &str, user: &str, secret: &str) -> io::Result<()>;
    /// Forgets the secret saved under `key`; nothing happens if there is none.
    fn delete(&self, key: &str);
}

/// This platform's store, as the .NET app picks it.
#[cfg(windows)]
pub fn platform() -> Box<dyn SecretStore> {
    Box::new(CredentialManager)
}

/// This platform's store, as the .NET app picks it.
#[cfg(target_os = "linux")]
pub fn platform() -> Box<dyn SecretStore> {
    Box::new(SecretTool::new(aipet_ipc::paths::data_dir()))
}

/// This platform's store, as the .NET app picks it.
#[cfg(not(any(windows, target_os = "linux")))]
pub fn platform() -> Box<dyn SecretStore> {
    Box::new(MemorySecrets::default())
}

/// Secrets kept for as long as the process runs: macOS's store, as in the C#.
#[derive(Debug, Default)]
pub struct MemorySecrets(Mutex<HashMap<String, String>>);

impl SecretStore for MemorySecrets {
    fn read(&self, key: &str) -> Option<String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).get(key).cloned()
    }

    fn write(&self, key: &str, _user: &str, secret: &str) -> io::Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.into(), secret.into());
        Ok(())
    }

    fn delete(&self, key: &str) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
    }
}
