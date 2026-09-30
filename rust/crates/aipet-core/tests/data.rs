//! The data layer held against the .NET app. rust/golden's `data` mode (rust/golden/Data.cs) wrote
//! tests/golden/data/ with the app's own code, and this replays it:
//! - files.json: config.json, jira.json and github.json read, then written back, byte for byte;
//! - secrets.json: the fallback secrets.json read, written and deleted from;
//! - hook_cleanup.json: which configs name the installed hook (VelopackTests' cases, and configs the hook's own
//!   registration code wrote), and what the C# makes of each character (src/cleanup.rs's tests);
//! - update_status.json: the texts Settings shows;
//! - log.json: each culture's time separator, which the log's lines carry.
//!
//! When `AIPET_GOLDEN` names the golden generator's dll (CI sets it; see aipet-ipc's src/csharp.rs to run it
//! locally), the C# also reads every file the Rust writes (the rollback path), writes a log line in the same culture
//! as the Rust, and on Windows it writes a Credential Manager token the Rust reads, and reads one the Rust writes,
//! under a test-only target. That Credential Manager test runs only with `AIPET_TEST_CREDENTIAL_MANAGER` set, since
//! it writes to the real Credential Manager (CI sets it).
//!
//! When the C# changes: `dotnet run --project rust/golden -c Release -- data` from the repository root, then fix the
//! port until this passes. Never edit the golden files by hand.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use aipet_core::cleanup::{self, AgentConfigs};
use aipet_core::config::{Config, GitHubSettings, JiraSettings, LoadStatus};
use aipet_core::log;
use aipet_core::secrets::{FileSecrets, SecretStore};
use aipet_core::update_status::{self, Kind, UpdateStatus};
use serde_json::Value;

const NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

fn golden(file: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/data")
        .join(file);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (write it with: dotnet run --project rust/golden -c Release -- data)",
            path.display()
        )
    });
    serde_json::from_str(&text).expect("golden JSON")
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..")
}

/// A folder of the test's own under the temp folder, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("aipet-core-data-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn status_name(status: LoadStatus) -> &'static str {
    match status {
        LoadStatus::Loaded => "loaded",
        LoadStatus::Missing => "missing",
        LoadStatus::Corrupt => "corrupt",
    }
}

/// The text of `sealed class <name>` up to its closing brace, each line trimmed: Data.cs's `ClassText`.
fn class_text(source: &str, name: &str) -> String {
    let lines: Vec<&str> = source.split('\n').map(str::trim).collect();
    let declares = |line: &str| {
        line.match_indices(&format!("sealed class {name}")).any(|(at, m)| {
            let before = line[..at].chars().next_back();
            let after = line[at + m.len()..].chars().next();
            !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
    };
    let start = lines
        .iter()
        .position(|l| declares(l))
        .unwrap_or_else(|| panic!("no class {name}"));
    let mut depth = 0i32;
    for (i, line) in lines.iter().enumerate().skip(start) {
        depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
        if depth == 0 && line.contains('}') {
            return lines[start..=i].join("\n");
        }
    }
    panic!("class {name} doesn't end")
}

/// A data file of a kind, loaded and saved by the Rust.
enum Loaded {
    Config(Config),
    Jira(JiraSettings),
    GitHub(GitHubSettings),
}

impl Loaded {
    fn load(kind: &str, path: &Path) -> (Loaded, LoadStatus) {
        match kind {
            "config" => {
                let (v, s) = Config::load_from(path);
                (Loaded::Config(v), s)
            }
            "jira" => {
                let (v, s) = JiraSettings::load_from(path);
                (Loaded::Jira(v), s)
            }
            "github" => {
                let (v, s) = GitHubSettings::load_from(path);
                (Loaded::GitHub(v), s)
            }
            _ => panic!("no kind {kind}"),
        }
    }

    fn save_to(&self, path: &Path) -> std::io::Result<()> {
        match self {
            Loaded::Config(v) => v.save_to(path),
            Loaded::Jira(v) => v.save_to(path),
            Loaded::GitHub(v) => v.save_to(path),
        }
    }
}

/// Each fixture file read with the app's status (missing and corrupt ones giving the defaults), and written back as
/// the app writes it after reading it. Loading never writes: the file stays as it was, and nothing else appears.
#[test]
fn data_files_read_and_write_as_the_app_does() {
    let golden = golden("files.json");
    // the fixtures hold while MainWindow's Config is the class they were written with
    let main_window = repo().join("src").join("AiPet.UI").join("MainWindow.axaml.cs");
    if let Ok(source) = fs::read_to_string(&main_window) {
        assert_eq!(
            class_text(&source, "Config"),
            golden["config_class"].as_str().unwrap(),
            "MainWindow's Config changed: write the golden data again, then fix the port"
        );
    }
    let scratch = Scratch::new("files");
    for (i, case) in golden["cases"].as_array().unwrap().iter().enumerate() {
        let (kind, name) = (case["kind"].as_str().unwrap(), case["name"].as_str().unwrap());
        let folder = scratch.0.join(i.to_string());
        fs::create_dir_all(&folder).unwrap();
        let input = folder.join(format!("{kind}.json"));
        let bytes = case["input"]
            .as_str()
            .map(|s| s.as_bytes().to_vec())
            .or_else(|| case["input_hex"].as_str().map(hex));
        if let Some(bytes) = &bytes {
            fs::write(&input, bytes).unwrap();
        }

        let (loaded, status) = Loaded::load(kind, &input);
        assert_eq!(status_name(status), case["status"], "{kind} {name}: status");
        assert_eq!(fs::read(&input).ok(), bytes, "{kind} {name}: the load wrote");
        assert_eq!(
            fs::read_dir(&folder).unwrap().count(),
            usize::from(bytes.is_some()),
            "{kind} {name}"
        );

        let output = folder.join("written.json");
        let saved = loaded.save_to(&output);
        match case["written"].as_str() {
            Some(expected) => {
                saved.unwrap_or_else(|e| panic!("{kind} {name}: {e}"));
                assert_eq!(
                    fs::read_to_string(&output).unwrap(),
                    expected.replace('\n', NEWLINE),
                    "{kind} {name}: written"
                );
            }
            None => {
                assert!(saved.is_err(), "{kind} {name}: the app can't write this");
                assert!(!output.exists(), "{kind} {name}: nothing is written");
            }
        }
    }

    // a missing folder is missing too, and isn't created
    let (_, status) = Config::load_from(&scratch.0.join("none").join("config.json"));
    assert_eq!(status, LoadStatus::Missing);
    assert!(!scratch.0.join("none").exists());
}

/// secrets.json as SecretTool reads it, writes a secret to it (over anything it can't read) and deletes one from it
/// (the file goes when nothing is left, and a file it can't read stays as it is).
#[test]
fn the_secrets_file_is_the_apps() {
    let scratch = Scratch::new("secrets");
    for (i, case) in golden("secrets.json").as_array().unwrap().iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let dir = scratch.0.join(i.to_string());
        let file = dir.join("secrets.json");
        let input = case["input"].as_str();
        let reset = || {
            fs::create_dir_all(&dir).unwrap();
            match input {
                Some(text) => fs::write(&file, text).unwrap(),
                None => {
                    let _ = fs::remove_file(&file);
                }
            }
        };
        let store = FileSecrets::new(&dir);

        reset();
        for (key, expected) in case["read"].as_object().unwrap() {
            assert_eq!(store.read(key).as_deref(), expected.as_str(), "{name}: read {key}");
        }

        let (key, secret) = (
            case["write"]["key"].as_str().unwrap(),
            case["write"]["secret"].as_str().unwrap(),
        );
        store.write(key, "ann", secret).unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            case["write"]["written"].as_str().unwrap(),
            "{name}: write"
        );

        reset();
        store.delete(case["delete"]["key"].as_str().unwrap());
        match case["delete"]["result"].as_str().unwrap() {
            "unchanged" => assert_eq!(
                fs::read(&file).ok(),
                input.map(|t| t.as_bytes().to_vec()),
                "{name}: delete"
            ),
            "removed" => assert!(!file.exists(), "{name}: delete"),
            text => assert_eq!(fs::read_to_string(&file).unwrap(), text, "{name}: delete"),
        }
    }
}

/// The C# reads every data file the Rust writes, to the same values: what it writes after reading one is the
/// Rust's file byte for byte. The files are what the Rust writes for each fixture it read, and a secrets.json.
#[test]
fn the_app_reads_what_the_rust_writes() {
    let Some(dll) = std::env::var_os("AIPET_GOLDEN") else {
        eprintln!("skipped: AIPET_GOLDEN doesn't name the golden generator's dll (see this file's doc)");
        return;
    };
    let scratch = Scratch::new("rollback");
    let fixtures = golden("files.json");
    let mut files: Vec<(&str, PathBuf)> = Vec::new();
    for (i, case) in fixtures["cases"].as_array().unwrap().iter().enumerate() {
        let kind = case["kind"].as_str().unwrap();
        let input = scratch.0.join(format!("{i}-in.json"));
        match (case["input"].as_str(), case["input_hex"].as_str()) {
            (Some(text), _) => fs::write(&input, text).unwrap(),
            (None, Some(bytes)) => fs::write(&input, hex(bytes)).unwrap(),
            (None, None) => {}
        }
        let output = scratch.0.join(format!("{i}-{kind}.json"));
        if Loaded::load(kind, &input).0.save_to(&output).is_ok() {
            files.push((kind, output));
        }
    }
    // the legacy toolbar, which the app itself never writes
    for (i, toolbar) in [Some(true), Some(false)].into_iter().enumerate() {
        let output = scratch.0.join(format!("toolbar-{i}.json"));
        Config {
            toolbar,
            left: Some(-7),
            ..Config::default()
        }
        .save_to(&output)
        .unwrap();
        files.push(("config", output));
    }
    let secrets = FileSecrets::new(&scratch.0.join("secrets"));
    secrets.write("AiPet:GitHub", "github", "ghp_x").unwrap();
    secrets.write("AiPet:Jira", "ann", "tok+en/é<&>\"'😀").unwrap();
    files.push(("secrets", scratch.0.join("secrets").join("secrets.json")));

    for kind in ["config", "jira", "github", "secrets"] {
        let paths: Vec<&PathBuf> = files.iter().filter(|(k, _)| *k == kind).map(|(_, p)| p).collect();
        let out = Command::new("dotnet")
            .arg(&dll)
            .args(["data", "read", kind])
            .args(&paths)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let lines: Vec<Value> = String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), paths.len());
        for (path, read) in paths.iter().zip(&lines) {
            let rust = fs::read_to_string(path).unwrap();
            assert_eq!(read["status"], "loaded", "{}: {rust}", path.display());
            assert_eq!(read["written"].as_str().unwrap(), rust, "{}", path.display());
        }
    }
}

/// HookCleanup.Names for VelopackTests' cases and more.
#[test]
fn hook_cleanup_names_the_hook_as_the_app_does() {
    for case in golden("hook_cleanup.json")["names"].as_array().unwrap() {
        let text = case["text"].as_str();
        // the C# skips a null hook as it skips an empty one
        let hooks: Vec<&str> = case["hooks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h.as_str().unwrap_or(""))
            .collect();
        let ignore_case = case["ignore_case"].as_bool().unwrap();
        assert_eq!(
            cleanup::names(text, &hooks, ignore_case),
            case["names"].as_bool().unwrap(),
            "{text:?} {hooks:?} ignore case: {ignore_case}"
        );
    }
}

/// HookCleanup.Agents on the configs the hook's own registration code (Install.cs, CodexConfig.cs) wrote in
/// VelopackTests' HookCleanupTests steps.
#[test]
fn hook_cleanup_finds_the_agents_as_the_app_does() {
    let scratch = Scratch::new("agents");
    let configs = AgentConfigs {
        claude_settings: scratch.0.join("claude").join("settings.json"),
        codex_files: [
            scratch.0.join("codex").join("config.toml"),
            scratch.0.join("codex").join("hooks.json"),
        ],
    };
    fs::create_dir_all(scratch.0.join("claude")).unwrap();
    fs::create_dir_all(scratch.0.join("codex")).unwrap();
    for step in golden("hook_cleanup.json")["agents"].as_array().unwrap() {
        for (file, text) in [
            (&configs.claude_settings, &step["claude_settings"]),
            (&configs.codex_files[0], &step["config_toml"]),
            (&configs.codex_files[1], &step["hooks_json"]),
        ] {
            match text.as_str() {
                Some(text) => fs::write(file, text).unwrap(),
                None => {
                    let _ = fs::remove_file(file);
                }
            }
        }
        for result in step["results"].as_array().unwrap() {
            let hooks: Vec<&str> = result["hooks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| h.as_str().unwrap())
                .collect();
            let ignore_case = result["ignore_case"].as_bool().unwrap();
            let expected: Vec<&str> = result["agents"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a.as_str().unwrap())
                .collect();
            assert_eq!(
                configs.agents(&hooks, ignore_case),
                expected,
                "{}: {hooks:?} ignore case: {ignore_case}",
                step["step"]
            );
        }
    }
}

#[test]
fn update_status_texts_are_the_apps() {
    let golden = golden("update_status.json");
    assert_eq!(update_status::REPO, golden["repo"]);
    assert_eq!(update_status::CHANNEL, golden["channel"]);
    assert_eq!(
        update_status::FIRST_CHECK.as_secs_f64(),
        golden["first_check_seconds"].as_f64().unwrap()
    );
    assert_eq!(
        update_status::EVERY.as_secs_f64(),
        golden["every_seconds"].as_f64().unwrap()
    );
    let kinds = [
        Kind::Off,
        Kind::Idle,
        Kind::Checking,
        Kind::UpToDate,
        Kind::Downloading,
        Kind::Ready,
        Kind::Failed,
    ];
    let cases = golden["cases"].as_array().unwrap();
    assert_eq!(cases.len(), kinds.len() * 4);
    for case in cases {
        let kind = kinds
            .into_iter()
            .find(|k| format!("{k:?}") == case["kind"])
            .expect("a kind the C# has");
        let status = UpdateStatus {
            kind,
            version: case["version"].as_str().map(Into::into),
            percent: case["percent"].as_i64().unwrap() as i32,
            error: case["error"].as_str().map(Into::into),
        };
        assert_eq!(status.text(), case["text"].as_str().unwrap(), "{status:?}");
        assert_eq!(status.busy(), case["busy"].as_bool().unwrap(), "{status:?}");
    }
}

/// Each culture's time separator is the one .NET has on ICU (Linux).
#[test]
fn log_time_separators_are_the_apps() {
    let golden = golden("log.json");
    let mut checked = 0;
    for group in golden["time_separators"].as_array().unwrap() {
        let separator = group["separator"].as_str().unwrap();
        for culture in group["cultures"].as_array().unwrap() {
            let culture = culture.as_str().unwrap();
            assert_eq!(log::culture_time_separator(culture), separator, "{culture:?}");
            checked += 1;
        }
    }
    assert!(checked > 500, "{checked} cultures");
}

/// The C#'s log line, written in this environment's culture, has the Rust's time separator: `HH:mm:ss.fff line`
/// with the culture's separator for `:`.
#[test]
fn the_log_has_the_apps_time_separator() {
    let Some(dll) = std::env::var_os("AIPET_GOLDEN") else {
        eprintln!("skipped: AIPET_GOLDEN doesn't name the golden generator's dll (see this file's doc)");
        return;
    };
    let out = Command::new("dotnet").arg(&dll).args(["data", "log"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let line = String::from_utf8(out.stdout).unwrap();
    let time = line.strip_suffix(" golden\n").unwrap_or_else(|| panic!("{line:?}"));
    let separator: String = time[2..].chars().take_while(|c| !c.is_ascii_digit()).collect();
    assert_eq!(separator, log::time_separator(), "{line:?}");
    let sep = log::time_separator();
    let shape: String = time.chars().map(|c| if c.is_ascii_digit() { '0' } else { c }).collect();
    assert_eq!(shape, format!("00{sep}00{sep}00.000"), "{line:?}");
}

/// A token in Credential Manager is the .NET app's: written with the target name as the key and the secret as its
/// UTF-16 blob, it reads back in either app. The target is a test one, and is deleted whatever happens. It writes to
/// the real Credential Manager of whoever runs it, so it runs only when `AIPET_TEST_CREDENTIAL_MANAGER` is set (CI
/// sets it).
#[cfg(windows)]
#[test]
fn credential_manager_tokens_are_the_apps() {
    use aipet_core::secrets::CredentialManager;

    if std::env::var_os("AIPET_TEST_CREDENTIAL_MANAGER").is_none() {
        eprintln!("skipped: it writes to this user's Credential Manager; AIPET_TEST_CREDENTIAL_MANAGER=1 runs it");
        return;
    }

    struct Forget(String);
    impl Drop for Forget {
        fn drop(&mut self) {
            CredentialManager.delete(&self.0);
        }
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let forget = Forget(format!("AiPet:GoldenTest:{}-{nanos}", std::process::id()));
    let target = forget.0.as_str();
    let store = CredentialManager;

    assert_eq!(store.read(target), None);
    store.write(target, "ann@acme.example", "tøken-✓-😀").unwrap();
    assert_eq!(store.read(target).as_deref(), Some("tøken-✓-😀"));
    store.delete(target);
    assert_eq!(store.read(target), None);

    let Some(dll) = std::env::var_os("AIPET_GOLDEN") else {
        eprintln!("skipped the C# half: AIPET_GOLDEN doesn't name the golden generator's dll");
        return;
    };
    let csharp = |args: &[&str]| {
        let out = Command::new("dotnet")
            .arg(&dll)
            .args(["data", "secret"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    };
    let csharp_reads = || {
        let read: Value = serde_json::from_str(csharp(&["read", target]).trim()).unwrap();
        read["secret"].as_str().map(str::to_owned)
    };

    csharp(&["write", target, "ann@acme.example", "from-csharp-✓"]);
    assert_eq!(store.read(target).as_deref(), Some("from-csharp-✓"));
    store.write(target, "ann@acme.example", "from-rust-😀").unwrap();
    assert_eq!(csharp_reads().as_deref(), Some("from-rust-😀"));
    store.delete(target);
    assert_eq!(csharp_reads(), None);
}
