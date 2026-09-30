//! `aipet-hook --install|--uninstall codex`, run as the installers run them, held against the C#:
//! tests/golden/codex/codex.json (rust/golden's `registration` mode, rust/golden/Registration.cs) ran each Codex
//! fixture through the hook's own CodexConfig.cs; each step here runs the built hook in the same setup and must give
//! the same exit code, the same lines and the same files, byte for byte.
//!
//! Codex's events depend on the OS, so a corpus is compared only on the OS it was written on ("os"): the committed one
//! on Windows. When `AIPET_GOLDEN` names the golden generator's dll (CI sets it; see aipet-ipc's src/csharp.rs to run
//! it locally), the C# writes the corpus again on this OS and that is replayed too: on Linux that is the only replay,
//! with the symlink and mode fixtures Windows can't make.
//!
//! In the fixtures the command AiPet registers is `{command}` (and `{command-escaped}` as TOML and JSON strings write
//! it), config.toml and hooks.json are `{config}` and `{hooks}`, and the case's folder is `{root}`: each side puts its
//! own values in their place, and back. A new backup is named by the clock, so after each step it is renamed
//! `<file>.aipet-20000101-<n>.bak`, as the C# harness did.
//!
//! When the C# changes: `dotnet run --project rust/golden -c Release -- registration` from the repository root, then
//! fix the port until this passes. Never edit the golden files by hand.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Map, Value, json};

const HOOK: &str = env!("CARGO_BIN_EXE_aipet-hook");
/// The fixtures' "only": what this OS can make.
const FAMILY: &str = if cfg!(windows) { "windows" } else { "unix" };
const THIS_OS: &str = if cfg!(windows) {
    "windows"
} else if cfg!(target_os = "macos") {
    "macos"
} else {
    "linux"
};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/codex/codex.json")
}

fn load(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (write it with: dotnet run --project rust/golden -c Release -- registration)",
            path.display()
        )
    });
    serde_json::from_str(&text).unwrap()
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or_else(|| panic!("not a string: {v}"))
}

/// A folder of the test's own under cargo's target folder, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Scratch {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("codex-{test}-{}", std::process::id()));
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

/// The command the built hook registers: its own path (on Unix the kernel's, symlinks resolved), which needs no
/// quotes or short name on Windows as long as the checkout's path has no spaces, and in single quotes on Unix.
fn hook_command() -> String {
    if cfg!(windows) {
        let hook = HOOK.to_owned();
        assert!(
            hook.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.:/\\~-".contains(&b)),
            "this test needs a checkout whose path needs no quotes: {hook}"
        );
        format!("{hook} --agent codex")
    } else {
        let hook = fs::canonicalize(HOOK).unwrap().to_string_lossy().into_owned();
        format!("'{}' --agent codex", hook.replace('\'', "'\\''"))
    }
}

/// A command or path as a TOML basic string and a JSON string write it.
fn escaped(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// A case's tokens with their values here, in the order the C# puts them in (`CodexTokens`).
fn tokens(root: &Path, command: &str) -> Vec<(&'static str, String)> {
    let config = root.join("codex").join("config.toml").to_string_lossy().into_owned();
    let hooks = root.join("codex").join("hooks.json").to_string_lossy().into_owned();
    vec![
        ("{command-escaped}", escaped(command)),
        ("{command}", command.to_owned()),
        ("{config-escaped}", escaped(&config)),
        ("{config}", config),
        ("{hooks-escaped}", escaped(&hooks)),
        ("{hooks}", hooks),
        ("{root}", root.to_string_lossy().into_owned()),
    ]
}

fn tokenized(text: &str, tokens: &[(&str, String)]) -> String {
    tokens
        .iter()
        .fold(text.to_owned(), |t, (token, value)| t.replace(value, token))
}

fn expanded(text: &str, tokens: &[(&str, String)]) -> String {
    tokens
        .iter()
        .fold(text.to_owned(), |t, (token, value)| t.replace(token, value))
}

// ------------------------------------------------------------------ the fixtures
/// A case's folder made as its setup says: codex/ (CODEX_HOME) and anything around it, and a harness folder for the
/// hook's temp folder, which the golden doesn't list.
fn set_up(case: &Value, root: &Path, tokens: &[(&str, String)]) -> io::Result<()> {
    let setup = &case["setup"];
    fs::create_dir_all(root.join("harness"))?;
    if setup.get("config_dir") != Some(&Value::Bool(false)) {
        fs::create_dir_all(root.join("codex"))?;
    }
    let entries = |key: &str| setup.get(key).and_then(Value::as_object).cloned().unwrap_or_default();
    for dir in setup.get("dirs").and_then(Value::as_array).into_iter().flatten() {
        fs::create_dir_all(root.join(text(dir)))?;
    }
    let file = |path: &str, bytes: Vec<u8>| {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, bytes)
    };
    for (path, content) in entries("files") {
        file(&path, expanded(text(&content), tokens).into_bytes())?;
    }
    for (path, hex) in entries("bytes") {
        file(&path, from_hex(text(&hex)))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        for (path, target) in entries("links") {
            symlink(text(&target), root.join(path))?;
        }
        for (path, mode) in entries("modes") {
            let mode = u32::from_str_radix(text(&mode), 8).unwrap();
            fs::set_permissions(root.join(path), fs::Permissions::from_mode(mode))?;
        }
    }
    for path in setup.get("read_only").and_then(Value::as_array).into_iter().flatten() {
        let path = root.join(text(path));
        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn from_hex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Sets a variable for the child, in place of the one it would inherit in any case: a Windows name keeps the case
/// it came with, and a second spelling wouldn't replace it.
fn set_env(command: &mut Command, name: &str, value: &Path) {
    for (inherited, _) in std::env::vars_os() {
        if inherited.to_string_lossy().eq_ignore_ascii_case(name) {
            command.env_remove(inherited);
        }
    }
    command.env(name, value);
}

/// The hook's `--install|--uninstall codex` with the case's codex/ as CODEX_HOME.
fn step(root: &Path, action: &str) -> Output {
    let mut hook = Command::new(HOOK);
    hook.args([format!("--{action}").as_str(), "codex"])
        .stdin(Stdio::null());
    let harness = root.join("harness");
    for (name, value) in [
        ("CODEX_HOME", root.join("codex")),
        ("TMPDIR", harness.clone()),
        ("TMP", harness.clone()),
        ("TEMP", harness),
    ] {
        set_env(&mut hook, name, &value);
    }
    hook.output().unwrap()
}

/// The backups a step can make: config.toml or hooks.json, `.aipet-<yyyyMMdd-HHmmss>.bak`.
fn backups(home: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(home) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            let Some(stamp) = ["config.toml.aipet-", "hooks.json.aipet-"]
                .iter()
                .find_map(|file| name.strip_prefix(file))
                .and_then(|n| n.strip_suffix(".bak"))
            else {
                return false;
            };
            stamp.len() == 15
                && stamp
                    .char_indices()
                    .all(|(i, c)| if i == 8 { c == '-' } else { c.is_ascii_digit() })
        })
        .collect();
    names.sort();
    names
}

/// Everything under the case's folder but the harness, as the golden has it: a folder, a symlink's target, or a
/// file's text (tokens in) or bytes, and on Unix its mode.
fn tree(root: &Path, tokens: &[(&str, String)]) -> io::Result<BTreeMap<String, Value>> {
    fn walk(
        root: &Path,
        dir: &Path,
        tokens: &[(&str, String)],
        entries: &mut BTreeMap<String, Value>,
    ) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            if rel == "harness" {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                let target = fs::read_link(&path)?;
                entries.insert(rel, json!({ "link": target.to_string_lossy() }));
            } else if kind.is_dir() {
                entries.insert(rel, json!({ "dir": true }));
                walk(root, &path, tokens, entries)?;
            } else {
                let bytes = fs::read(&path)?;
                let mut e = Map::new();
                match String::from_utf8(bytes) {
                    Ok(t) if !t.starts_with('\u{feff}') => e.insert("text".into(), tokenized(&t, tokens).into()),
                    Ok(t) => e.insert("hex".into(), to_hex(t.as_bytes()).into()),
                    Err(e_) => e.insert("hex".into(), to_hex(e_.as_bytes()).into()),
                };
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = fs::metadata(&path)?.permissions().mode() & 0o7777;
                    e.insert("mode".into(), format!("{mode:o}").into());
                }
                entries.insert(rel, Value::Object(e));
            }
        }
        Ok(())
    }
    let mut entries = BTreeMap::new();
    walk(root, root, tokens, &mut entries)?;
    Ok(entries)
}

/// Why a run of a case didn't come out as the C#'s.
enum Failure {
    /// Another process kept a file from the hook, or from the harness around it: a sharing or lock violation. A virus
    /// scanner can hold a file just written for a moment (Windows), and such a run says nothing of the port.
    Held(String),
    /// The hook's answer isn't the C#'s.
    Differs(String),
}

/// A sharing and a lock violation, the errors of a file another process has: Windows' ERROR_SHARING_VIOLATION and
/// ERROR_LOCK_VIOLATION (Unix has neither).
const HELD: [i32; 2] = [32, 33];

fn held(e: &io::Error) -> bool {
    cfg!(windows) && e.raw_os_error().is_some_and(|code| HELD.contains(&code))
}

/// The harness's own work on the case's files failed: a held file sets the run aside, anything else is a bug here.
fn harness(what: &str, e: io::Error) -> Failure {
    assert!(held(&e), "{what}: {e}");
    Failure::Held(format!("{what}: {e}"))
}

/// Whether the hook said it was kept from a file: the system's text for a sharing or lock violation.
fn hook_held(said: &str) -> bool {
    cfg!(windows)
        && HELD.into_iter().any(|code| {
            let error = io::Error::from_raw_os_error(code).to_string();
            let (text, _) = error.rsplit_once(" (os error ").unwrap_or((error.as_str(), ""));
            said.contains(text)
        })
}

/// Every case of a corpus written on this OS that this OS can make, through the built hook. Every run of a case must
/// come out as the C#'s, but for a run another process kept from a file (see [`Failure::Held`]): that run is set
/// aside, said, and the case run again, twice at most. Any other difference fails at once.
fn replay(corpus: &Value, scratch: &Scratch) {
    assert_eq!(text(&corpus["os"]), THIS_OS, "Codex's events depend on the OS");
    let command = hook_command();
    let mut steps = 0;
    for (n, case) in corpus["cases"].as_array().unwrap().iter().enumerate() {
        let name = text(&case["name"]);
        if case.get("only").map(text).is_some_and(|only| only != FAMILY) {
            continue;
        }
        let mut run = 1;
        steps += loop {
            match replay_case(case, &scratch.0.join(format!("{n:02}-{run}")), &command) {
                Ok(count) => break count,
                Err(Failure::Held(why)) if run < 3 => {
                    eprintln!("{name}, run {run} set aside, a file was held: {why}");
                    run += 1;
                }
                Err(Failure::Held(why) | Failure::Differs(why)) => panic!("{name}: {why}"),
            }
        };
    }
    assert!(steps > 100, "only {steps} steps were replayed");
}

/// A case's steps, each held against the C#'s: its exit code, what it printed and the files it left. How many
/// steps, or the first difference.
fn replay_case(case: &Value, root: &Path, command: &str) -> Result<usize, Failure> {
    let tokens = tokens(root, command);
    set_up(case, root, &tokens).map_err(|e| harness("setting the case up", e))?;
    let home = root.join("codex");
    let mut made = 0;
    let steps = case["steps"].as_array().unwrap();
    for (i, expected) in steps.iter().enumerate() {
        let action = text(&expected["action"]);
        let before = backups(&home);
        let out = step(root, action);
        for backup in backups(&home).into_iter().filter(|b| !before.contains(b)) {
            made += 1;
            let file = &backup[..backup.find(".aipet-").unwrap()];
            fs::rename(
                home.join(&backup),
                home.join(format!("{file}.aipet-20000101-{made:06}.bak")),
            )
            .map_err(|e| harness("renaming a new backup", e))?;
        }
        let said = |bytes: &[u8]| tokenized(&String::from_utf8_lossy(bytes), &tokens);
        let files: BTreeMap<String, Value> = expected["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // a step the hook couldn't finish for a held file gave no answer of the port's
        let failure = if hook_held(&said(&out.stderr)) {
            Failure::Held
        } else {
            Failure::Differs
        };
        let differ = |what: &str, rust: &dyn std::fmt::Debug, csharp: &dyn std::fmt::Debug| {
            Err(failure(format!(
                "step {i} ({action}), {what}:
  the hook: {rust:#?}
  the C#:   {csharp:#?}"
            )))
        };
        if out.status.code() != expected["exit"].as_i64().map(|c| c as i32) {
            return differ("exit", &out, &expected["stderr"]);
        }
        if said(&out.stdout) != text(&expected["stdout"]) {
            return differ("stdout", &said(&out.stdout), &text(&expected["stdout"]));
        }
        if said(&out.stderr) != text(&expected["stderr"]) {
            return differ("stderr", &said(&out.stderr), &text(&expected["stderr"]));
        }
        let rust = tree(root, &tokens).map_err(|e| harness("reading the case's files", e))?;
        if rust != files {
            return differ("files", &rust, &files);
        }
    }
    Ok(steps.len())
}

/// Every Codex fixture's --install and --uninstall gives what the C#'s gave, where the corpus was written.
#[test]
fn codex_registration_is_the_csharps() {
    let corpus = load(&golden());
    if text(&corpus["os"]) != THIS_OS {
        eprintln!(
            "skipped: codex.json was written on {} (the live replay covers this OS)",
            corpus["os"]
        );
        return;
    }
    replay(&corpus, &Scratch::new("golden"));
}

/// With `AIPET_GOLDEN`, the C# writes the corpus on this OS, and that is replayed too.
#[test]
fn the_csharp_on_this_os_is_replayed() {
    let Some(dll) = std::env::var_os("AIPET_GOLDEN") else {
        eprintln!("skipped: AIPET_GOLDEN doesn't name the golden generator's dll (see this file's doc)");
        return;
    };
    let scratch = Scratch::new("live");
    let written = scratch.0.join("golden");
    let out = Command::new("dotnet")
        .arg(dll)
        .arg("registration")
        .arg(&written)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let replays = Scratch::new("live-replay");
    replay(&load(&written.join("codex.json")), &replays);
}
