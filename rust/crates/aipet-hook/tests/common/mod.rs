//! What the hook's golden replays share (tests/registration.rs, tests/codex.rs and tests/doctor.rs): the corpus
//! files, scratch folders, the child's environment, and the rule for a file another process held.

// each test file uses its part of it
#![allow(dead_code)]

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

pub const HOOK: &str = env!("CARGO_BIN_EXE_aipet-hook");
pub const NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };
/// The fixtures' "only": what this OS can make.
pub const FAMILY: &str = if cfg!(windows) { "windows" } else { "unix" };
pub const THIS_OS: &str = if cfg!(windows) {
    "windows"
} else if cfg!(target_os = "macos") {
    "macos"
} else {
    "linux"
};

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..")
}

/// A corpus rust/golden wrote, which its `mode` writes again.
pub fn load(path: &Path, mode: &str) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (write it with: dotnet run --project rust/golden -c Release -- {mode})",
            path.display()
        )
    });
    serde_json::from_str(&text).unwrap()
}

pub fn text(v: &Value) -> &str {
    v.as_str().unwrap_or_else(|| panic!("not a string: {v}"))
}

/// A folder of the test's own under cargo's target folder (where a hard link to the hook can be made), removed when
/// dropped.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(suite: &str, test: &str) -> Scratch {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{suite}-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // the read-only fixture's files
        clear_read_only(&self.0);
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn clear_read_only(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => clear_read_only(&path),
            Ok(t) if t.is_file() => {
                if let Ok(m) = fs::metadata(&path)
                    && m.permissions().readonly()
                {
                    let mut p = m.permissions();
                    #[allow(clippy::permissions_set_readonly_false)]
                    p.set_readonly(false);
                    let _ = fs::set_permissions(&path, p);
                }
            }
            _ => {}
        }
    }
}

pub fn from_hex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Sets a variable for the child (or, with `None`, takes it away), in place of the one it would inherit in any case: a
/// Windows name keeps the case it came with (Git Bash gives `PROGRAMFILES`), and a second spelling wouldn't replace
/// it.
pub fn set_env(command: &mut Command, name: &str, value: Option<&OsStr>) {
    for (inherited, _) in std::env::vars_os() {
        if inherited.to_string_lossy().eq_ignore_ascii_case(name) {
            command.env_remove(inherited);
        }
    }
    if let Some(value) = value {
        command.env(name, value);
    }
}

// ------------------------------------------------------------------ a file another process held
/// Why a run of a case didn't come out as the C#'s.
pub enum Failure {
    /// Another process kept a file from the hook, or from the harness around it: a sharing or lock violation. A virus
    /// scanner can hold a file just written for a moment (Windows), and such a run says nothing of the port.
    Held(String),
    /// The hook's answer isn't the C#'s.
    Differs(String),
}

/// A sharing and a lock violation, the errors of a file another process has: Windows' ERROR_SHARING_VIOLATION and
/// ERROR_LOCK_VIOLATION (Unix has neither).
pub const HELD: [i32; 2] = [32, 33];

/// Whether another process has the file.
pub fn held(e: &io::Error) -> bool {
    cfg!(windows) && e.raw_os_error().is_some_and(|code| HELD.contains(&code))
}

/// The harness's own work on the case's files failed: a held file sets the run aside, anything else is a bug here.
pub fn harness(what: &str, e: io::Error) -> Failure {
    assert!(held(&e), "{what}: {e}");
    Failure::Held(format!("{what}: {e}"))
}

/// Whether the hook said it was kept from a file: the system's text for a sharing or lock violation, which
/// install.rs's `thrown` and `moved` pass on. The golden never has one, since its generator sets such runs aside.
pub fn hook_held(said: &str) -> bool {
    cfg!(windows)
        && HELD.into_iter().any(|code| {
            let error = io::Error::from_raw_os_error(code).to_string();
            let (text, _) = error.rsplit_once(" (os error ").unwrap_or((error.as_str(), ""));
            said.contains(text)
        })
}
