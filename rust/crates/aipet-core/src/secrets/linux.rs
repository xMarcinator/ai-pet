//! Secret Service through `secret-tool`, with the attributes `service=aipet` and `key=<key>`
//! (`LinuxPlatform.cs:270-330`).
//!
//! CI has no Secret Service session, so the keyring half is checked by hand against the .NET app's commands, on a
//! Linux desktop with `secret-tool` installed:
//!
//! ```text
//! cargo test -p aipet-core --lib secret_service -- --ignored
//! ```
//!
//! It stores a token under a test key exactly as the C# does (`secret-tool store --label='AiPet <key>' service aipet
//! key <key> user <user>`, the secret on stdin) and reads it with this store; then stores one with this store and
//! reads it with the C#'s `secret-tool lookup service aipet key <key>`; then deletes it, and clears the key whatever
//! happened. With both apps at hand the same check is: save a Jira token in one app's Settings, and see the other's
//! Jira page find it.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::{FileSecrets, SecretStore};

/// How long secret-tool gets for each call (`Sh.Capture`'s timeout).
const TIMEOUT: Duration = Duration::from_secs(3);

/// The .NET app's `SecretTool`: the keyring through `secret-tool`, and `secrets.json` (0600) in the data folder
/// when there is no `secret-tool`, or it fails or takes too long.
#[derive(Clone, Debug)]
pub struct SecretTool {
    /// The `secret-tool` found on `PATH`, looked for once, as the C#'s `Sh.Which` caches it.
    tool: Option<PathBuf>,
    file: FileSecrets,
}

impl SecretTool {
    /// The keyring, with the fallback file in `data_dir`.
    pub fn new(data_dir: &Path) -> Self {
        SecretTool {
            tool: on_path("secret-tool"),
            file: FileSecrets::new(data_dir),
        }
    }

    /// Runs secret-tool with `args`, and `stdin` written to it: what it printed when it exited 0 in time. `None`
    /// when there is no secret-tool, it couldn't run, took too long (it is killed) or failed: the C#'s nonzero
    /// `Sh.Capture`.
    fn run(&self, args: &[&str], stdin: Option<&str>) -> Option<String> {
        let tool = self.tool.as_ref()?;
        let mut child = Command::new(tool)
            .args(args)
            // like the C#, only a call with something to say gets its own stdin
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::inherit()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let stop = |mut child: std::process::Child| {
            let _ = child.kill();
            let _ = child.wait();
            None
        };
        if let Some(secret) = stdin {
            // closed once written
            let written = child.stdin.take().map(|mut s| s.write_all(secret.as_bytes()));
            if !matches!(written, Some(Ok(()))) {
                return stop(child);
            }
        }
        let mut stdout = child.stdout.take()?;
        let reader = thread::spawn(move || {
            let mut out = Vec::new();
            let _ = stdout.read_to_end(&mut out);
            out
        });
        let deadline = Instant::now() + TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                _ => return stop(child),
            }
        };
        let out = reader.join().ok()?;
        status.success().then(|| String::from_utf8_lossy(&out).into_owned())
    }
}

/// `Sh.Which`: the first file of that name in a `PATH` folder (an empty entry is the current folder).
fn on_path(tool: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join(tool))
        .find(|p| p.is_file())
}

impl SecretStore for SecretTool {
    /// The keyring's secret, without its trailing newlines, when it has one that isn't empty; else the file's.
    fn read(&self, key: &str) -> Option<String> {
        match self.run(&["lookup", "service", "aipet", "key", key], None) {
            Some(out) if !out.is_empty() => Some(out.trim_end_matches('\n').to_owned()),
            _ => self.file.read(key),
        }
    }

    /// In the keyring, labelled `AiPet <key>`; in the file only when the keyring can't take it.
    fn write(&self, key: &str, user: &str, secret: &str) -> io::Result<()> {
        let label = format!("--label=AiPet {key}");
        let args = ["store", &label, "service", "aipet", "key", key, "user", user];
        if self.run(&args, Some(secret)).is_some() {
            return Ok(());
        }
        self.file.write(key, user, secret)
    }

    /// From the keyring and from the file.
    fn delete(&self, key: &str) {
        let _ = self.run(&["clear", "service", "aipet", "key", key], None);
        self.file.delete(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> Dir {
        let dir = std::env::temp_dir().join(format!("aipet-core-secret-tool-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Dir(dir)
    }

    /// A stand-in secret-tool in `dir`: it notes each call's arguments (one a line, then `--`) and its stdin, and
    /// answers `lookup` with the file `lookup` holds; each command exits with the code in `<command>-exit` (0 if
    /// there is none), and hangs while there is a file `hang`.
    fn stand_in(dir: &Path) -> SecretTool {
        let tool = dir.join("secret-tool");
        fs::write(
            &tool,
            format!(
                "#!/bin/sh\nd='{d}'\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done >> \"$d/calls\"\necho -- >> \"$d/calls\"\n\
                 [ \"$1\" = store ] && cat > \"$d/stdin\"\n[ \"$1\" = lookup ] && cat \"$d/lookup\" 2>/dev/null\n\
                 [ -e \"$d/hang\" ] && exec sleep 10\nexit $(cat \"$d/$1-exit\" 2>/dev/null || echo 0)\n",
                d = dir.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        SecretTool {
            tool: Some(tool),
            file: FileSecrets::new(&dir.join("data")),
        }
    }

    fn calls(dir: &Path) -> String {
        fs::read_to_string(dir.join("calls")).unwrap_or_default()
    }

    /// The C#'s arguments and stdin, and its fallback to the file when secret-tool fails.
    #[test]
    fn the_keyring_is_called_as_the_csharp_calls_it() {
        let d = dir("calls");
        let store = stand_in(&d.0);

        store.write("AiPet:Jira", "ann@acme.example", "tok+en").unwrap();
        assert_eq!(
            calls(&d.0),
            "store\n--label=AiPet AiPet:Jira\nservice\naipet\nkey\nAiPet:Jira\nuser\nann@acme.example\n--\n"
        );
        assert_eq!(fs::read_to_string(d.0.join("stdin")).unwrap(), "tok+en");
        assert!(!d.0.join("data").exists(), "the keyring took it");

        fs::write(d.0.join("lookup"), "tok+en\n\n").unwrap();
        assert_eq!(store.read("AiPet:Jira").as_deref(), Some("tok+en"));
        assert!(calls(&d.0).ends_with("--\nlookup\nservice\naipet\nkey\nAiPet:Jira\n--\n"));

        // a keyring that refuses: the file takes it, and the file answers when lookup has nothing
        fs::write(d.0.join("store-exit"), "1").unwrap();
        store.write("AiPet:GitHub", "github", "ghp_x").unwrap();
        assert_eq!(
            fs::read_to_string(d.0.join("data").join("secrets.json")).unwrap(),
            r#"{"AiPet:GitHub":"ghp_x"}"#
        );
        fs::write(d.0.join("lookup"), "").unwrap();
        assert_eq!(store.read("AiPet:GitHub").as_deref(), Some("ghp_x"));
        fs::write(d.0.join("lookup"), "other").unwrap();
        fs::write(d.0.join("lookup-exit"), "1").unwrap();
        assert_eq!(store.read("AiPet:GitHub").as_deref(), Some("ghp_x"));

        // deleting clears the keyring and the file
        store.delete("AiPet:GitHub");
        assert!(calls(&d.0).ends_with("--\nclear\nservice\naipet\nkey\nAiPet:GitHub\n--\n"));
        assert!(!d.0.join("data").join("secrets.json").exists());
    }

    #[test]
    fn without_secret_tool_the_file_keeps_them() {
        let d = dir("none");
        let store = SecretTool {
            tool: None,
            file: FileSecrets::new(&d.0),
        };
        store.write("AiPet:Jira", "ann", "secret").unwrap();
        assert_eq!(store.read("AiPet:Jira").as_deref(), Some("secret"));
        store.delete("AiPet:Jira");
        assert_eq!(store.read("AiPet:Jira"), None);
    }

    /// A secret-tool that hangs is stopped after 3 s, and the file answers.
    #[test]
    fn a_hanging_secret_tool_is_stopped() {
        let d = dir("hang");
        let store = stand_in(&d.0);
        fs::write(d.0.join("hang"), "").unwrap();
        let started = Instant::now();
        assert_eq!(store.read("AiPet:Jira"), None);
        let took = started.elapsed();
        assert!(took >= TIMEOUT && took < TIMEOUT + Duration::from_secs(3), "{took:?}");
    }

    /// The manual check the module's documentation describes: needs a Secret Service session and `secret-tool`.
    #[test]
    #[ignore = "needs a Secret Service session: run it by hand on a Linux desktop (see the module's documentation)"]
    fn secret_service_round_trip_with_the_csharps_commands() {
        let key = format!("AiPet:Test:{}", std::process::id());
        struct Clear<'a>(&'a str);
        impl Drop for Clear<'_> {
            fn drop(&mut self) {
                let _ = Command::new("secret-tool")
                    .args(["clear", "service", "aipet", "key", self.0])
                    .status();
            }
        }
        let _clear = Clear(&key);
        let d = dir("keyring");
        let store = SecretTool::new(&d.0);
        assert!(store.tool.is_some(), "secret-tool isn't on PATH");

        // stored as the C# stores it
        let mut csharp = Command::new("secret-tool")
            .args([
                "store",
                &format!("--label=AiPet {key}"),
                "service",
                "aipet",
                "key",
                &key,
                "user",
                "test",
            ])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        csharp.stdin.take().unwrap().write_all(b"from-csharp").unwrap();
        assert!(csharp.wait().unwrap().success());
        assert_eq!(store.read(&key).as_deref(), Some("from-csharp"));

        // read as the C# reads it
        store.write(&key, "test", "from-rust").unwrap();
        assert!(
            !d.0.join("secrets.json").exists(),
            "it went to the file, not the keyring"
        );
        let lookup = Command::new("secret-tool")
            .args(["lookup", "service", "aipet", "key", &key])
            .output()
            .unwrap();
        assert!(lookup.status.success());
        assert_eq!(
            String::from_utf8_lossy(&lookup.stdout).trim_end_matches('\n'),
            "from-rust"
        );

        store.delete(&key);
        assert_eq!(store.read(&key), None);
    }
}
