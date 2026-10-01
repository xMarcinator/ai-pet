//! `aipet-hook --install|--uninstall claude` and `--print-plugin-hooks`, run as the installers run them, held against
//! the C#:
//! - tests/golden/registration/claude.json: rust/golden's `registration` mode (rust/golden/Registration.cs) ran each
//!   Claude fixture through the hook's own Install.cs; each step here runs the built hook in the same setup and must
//!   give the same exit code, the same lines and the same files, byte for byte;
//! - the same file's `run` cases: Install.Run's usage errors and its refusal to register a program that isn't
//!   aipet-hook;
//! - the plugin's committed hook files, which --print-plugin-hooks must print as they are.
//!
//! The corpus was written on one OS ("os"). On another, the newline is its (`Environment.NewLine`, in the files the
//! C# writes and in what it prints), fixtures that give another answer there (os_specific) aren't compared, and modes
//! only on Unix. When `AIPET_GOLDEN` names the golden generator's dll (CI sets it; see aipet-ipc's src/csharp.rs to
//! run it locally), the C# writes the corpus again on this OS and that is replayed too: on Linux it holds the
//! symlink and mode fixtures that Windows can't make.
//!
//! When the C# changes: `dotnet run --project rust/golden -c Release -- registration` from the repository root, then
//! fix the port until this passes. Never edit the golden files by hand.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use common::{
    FAMILY, Failure, HOOK, NEWLINE, Scratch, THIS_OS, from_hex, harness, hook_held, repo, set_env, text, to_hex,
};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/registration")
}

fn load(path: &Path) -> Value {
    common::load(path, "registration")
}

fn new_scratch(test: &str) -> Scratch {
    Scratch::new("registration", test)
}

/// The hook as it registers itself: its path with `/`, as the C# writes `Environment.ProcessPath` (on Unix the
/// kernel's, symlinks resolved).
fn hook_written() -> String {
    let hook = if cfg!(unix) {
        fs::canonicalize(HOOK).unwrap()
    } else {
        PathBuf::from(HOOK)
    };
    hook.to_string_lossy().replace('\\', "/")
}

// ------------------------------------------------------------------ the fixtures
/// A case's folder made as its setup says: claude/ (CLAUDE_CONFIG_DIR) and anything around it, and the harness
/// folder the C# had too, with a bash.exe to find and an empty folder for PATH.
fn set_up(case: &Value, root: &Path, exe: &str) -> io::Result<()> {
    let setup = &case["setup"];
    fs::create_dir_all(root.join("harness").join("nobin"))?;
    fs::write(root.join("harness").join("bash.exe"), "")?;
    if setup.get("config_dir") != Some(&Value::Bool(false)) {
        fs::create_dir_all(root.join("claude"))?;
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
        file(&path, text(&content).replace("{exe}", exe).into_bytes())?;
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

/// The hook's `--install|--uninstall claude` in a case's folder, with Git Bash found or not, as the C# ran it.
fn step(root: &Path, action: &str, git_bash: bool) -> Output {
    let harness = root.join("harness");
    let nobin = harness.join("nobin");
    let bash = harness.join("bash.exe");
    let mut hook = Command::new(HOOK);
    hook.args([format!("--{action}").as_str(), "claude"])
        .stdin(Stdio::null());
    let config = root.join("claude");
    for (name, value) in [
        ("CLAUDE_CONFIG_DIR", Some(config.as_path())),
        ("CLAUDE_CODE_GIT_BASH_PATH", git_bash.then_some(bash.as_path())),
        ("PATH", Some(&nobin)),
        ("ProgramFiles", Some(&nobin)),
        ("LOCALAPPDATA", Some(&nobin)),
        ("TMPDIR", Some(&harness)),
        ("TMP", Some(&harness)),
        ("TEMP", Some(&harness)),
    ] {
        set_env(&mut hook, name, value.map(Path::as_os_str));
    }
    hook.output().unwrap()
}

/// The backups a step made: settings.json.aipet-<yyyyMMdd-HHmmss>.bak.
fn backups(config: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(config) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            let Some(stamp) = name
                .strip_prefix("settings.json.aipet-")
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
/// file's text ({exe} for the hook) or bytes, and on Unix its mode.
fn tree(root: &Path, exe: &str) -> io::Result<BTreeMap<String, Value>> {
    fn walk(root: &Path, dir: &Path, exe: &str, entries: &mut BTreeMap<String, Value>) -> io::Result<()> {
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
                walk(root, &path, exe, entries)?;
            } else {
                let bytes = fs::read(&path)?;
                let mut e = Map::new();
                match String::from_utf8(bytes) {
                    Ok(t) if !t.starts_with('\u{feff}') => e.insert("text".into(), t.replace(exe, "{exe}").into()),
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
    walk(root, root, exe, &mut entries)?;
    Ok(entries)
}

/// How a corpus written elsewhere is compared here: its newline as this OS's, and modes only when both have them.
struct Adapt {
    newline: bool,
    modes: bool,
}

impl Adapt {
    fn of(corpus: &Value) -> Adapt {
        Adapt {
            newline: text(&corpus["newline"]) != NEWLINE,
            modes: cfg!(unix) && text(&corpus["os"]) != "windows",
        }
    }

    fn text(&self, s: &str) -> String {
        if self.newline {
            s.replace("\r\n", "\n")
        } else {
            s.to_owned()
        }
    }

    fn files(&self, files: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
        files
            .iter()
            .map(|(path, entry)| {
                let mut entry = entry.as_object().unwrap().clone();
                if let Some(Value::String(t)) = entry.get("text") {
                    let t = self.text(t);
                    entry.insert("text".into(), t.into());
                }
                if !self.modes {
                    entry.remove("mode");
                }
                (path.clone(), Value::Object(entry))
            })
            .collect()
    }
}

/// Every case of a corpus this OS can make, through the built hook. Every run of a case must come out as the C#'s,
/// but for a run another process kept from a file, which says nothing of the port: the hook or the harness met a
/// sharing or lock violation (a virus scanner can hold a file just written for a moment, on Windows). That run is
/// set aside, said, and the case run again, twice at most. Any other difference fails at once.
fn replay(corpus: &Value, scratch: &Scratch) {
    let exe = hook_written();
    let adapt = Adapt::of(corpus);
    let same_os = text(&corpus["os"]) == THIS_OS;
    let mut steps = 0;
    for (n, case) in corpus["cases"].as_array().unwrap().iter().enumerate() {
        let name = text(&case["name"]);
        let only = case.get("only").map(text);
        if only.is_some_and(|only| only != FAMILY) || (case.get("os_specific").is_some() && !same_os) {
            continue;
        }
        let git_bash = case.get("git_bash") != Some(&Value::Bool(false));
        // Windows gives every process its ProgramFiles, so where Git Bash is there, it can't be hidden from the hook:
        // claude.rs's own test runs this case in the process
        let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from);
        if cfg!(windows)
            && !git_bash
            && program_files.is_some_and(|p| p.join("Git").join("bin").join("bash.exe").is_file())
        {
            eprintln!("{name}: skipped, Git Bash is in ProgramFiles");
            continue;
        }
        let mut run = 1;
        steps += loop {
            match replay_case(case, &scratch.0.join(format!("{n:02}-{run}")), &exe, &adapt, git_bash) {
                Ok(count) => break count,
                Err(Failure::Held(why)) if run < 3 => {
                    eprintln!("{name}, run {run} set aside, a file was held: {why}");
                    run += 1;
                }
                Err(Failure::Held(why) | Failure::Differs(why)) => panic!("{name}: {why}"),
            }
        };
    }
    assert!(steps > 50, "only {steps} steps were replayed");
}

/// A case's steps, each held against the C#'s: its exit code, what it printed and the files it left. How many
/// steps, or the first difference.
fn replay_case(case: &Value, root: &Path, exe: &str, adapt: &Adapt, git_bash: bool) -> Result<usize, Failure> {
    set_up(case, root, exe).map_err(|e| harness("setting the case up", e))?;
    let config = root.join("claude");
    let settings = config.join("settings.json");
    let mut made = 0;
    let steps = case["steps"].as_array().unwrap();
    for (i, expected) in steps.iter().enumerate() {
        let action = text(&expected["action"]);
        let before = backups(&config);
        let out = step(root, action, git_bash);
        for backup in backups(&config).into_iter().filter(|b| !before.contains(b)) {
            made += 1;
            fs::rename(
                config.join(backup),
                config.join(format!("settings.json.aipet-20000101-{made:06}.bak")),
            )
            .map_err(|e| harness("renaming a new backup", e))?;
        }
        let said = |bytes: &[u8]| {
            adapt.text(
                &String::from_utf8_lossy(bytes)
                    .replace(&settings.display().to_string(), "{settings}")
                    .replace(&root.display().to_string(), "{root}"),
            )
        };
        let files: BTreeMap<String, Value> = expected["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let exit = expected["exit"].as_i64().map(|c| c as i32);
        // a step the hook couldn't finish for a held file gave no answer of the port's
        let held = hook_held(&said(&out.stderr));
        let failure = if held { Failure::Held } else { Failure::Differs };
        let differ = |what: &str, rust: &dyn std::fmt::Debug, csharp: &dyn std::fmt::Debug| {
            Err(failure(format!(
                "step {i} ({action}), {what}:
  the hook: {rust:#?}
  the C#:   {csharp:#?}"
            )))
        };
        if out.status.code() != exit {
            return differ("exit", &out, &expected["stderr"]);
        }
        let (stdout, stderr) = (
            adapt.text(text(&expected["stdout"])),
            adapt.text(text(&expected["stderr"])),
        );
        if said(&out.stdout) != stdout {
            return differ("stdout", &said(&out.stdout), &stdout);
        }
        if said(&out.stderr) != stderr {
            return differ("stderr", &said(&out.stderr), &stderr);
        }
        let rust = tree(root, exe).map_err(|e| harness("reading the case's files", e))?;
        let (rust, csharp) = (adapt.files(&rust), adapt.files(&files));
        if rust != csharp {
            return differ("files", &rust, &csharp);
        }
    }
    Ok(steps.len())
}

/// Every Claude fixture's --install and --uninstall gives what the C#'s gave.
#[test]
fn claude_registration_is_the_csharps() {
    let scratch = new_scratch("claude");
    replay(&load(&golden_dir().join("claude.json")), &scratch);
}

/// What sets a run aside is known from what the hook says: here settings.json is held as a virus scanner can hold
/// it, open and shared with nobody, and the hook's refusal is a sharing violation. (Unix has none.)
#[cfg(windows)]
#[test]
fn a_held_file_is_known_from_what_the_hook_says() {
    use std::os::windows::fs::OpenOptionsExt;

    let scratch = new_scratch("held");
    let case = json!({ "setup": { "files": { "claude/settings.json": "{}" } } });
    set_up(&case, &scratch.0, &hook_written()).unwrap();
    let settings = scratch.0.join("claude").join("settings.json");
    let held = fs::OpenOptions::new().read(true).share_mode(0).open(&settings).unwrap();
    let out = step(&scratch.0, "install", true);
    drop(held);
    let said = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(hook_held(&said), "{said}");
    // the read-only fixture's refusal is an answer of the port's
    let denied = "Couldn't update claude settings: Access to the path is denied.";
    assert!(!hook_held(denied));
}

/// With `AIPET_GOLDEN`, the C# writes the corpus on this OS, and that is replayed too. The System.Text.Json data
/// (json.json) doesn't depend on the OS: it must come out as committed.
#[test]
fn the_csharp_on_this_os_is_replayed() {
    let Some(dll) = std::env::var_os("AIPET_GOLDEN") else {
        eprintln!("skipped: AIPET_GOLDEN doesn't name the golden generator's dll (see this file's doc)");
        return;
    };
    let scratch = new_scratch("live");
    let written = scratch.0.join("golden");
    let out = Command::new("dotnet")
        .arg(dll)
        .arg("registration")
        .arg(&written)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(
        load(&written.join("json.json")) == load(&golden_dir().join("json.json")),
        "System.Text.Json writes, compares or refuses otherwise than when json.json was written"
    );
    let corpus = load(&written.join("claude.json"));
    assert_eq!(text(&corpus["os"]), THIS_OS);
    let replays = new_scratch("live-replay");
    replay(&corpus, &replays);
}

// ------------------------------------------------------------------ Install.Run, and the modes' usage
/// The hook under another name, a hard link in the scratch folder: what --install must refuse to register.
fn renamed_hook(scratch: &Scratch) -> PathBuf {
    let renamed = scratch
        .0
        .join(format!("aipet-hook-renamed{}", std::env::consts::EXE_SUFFIX));
    fs::hard_link(HOOK, &renamed).unwrap();
    renamed
}

/// Install.Run's answers: usage for an agent it doesn't know, and a refusal to register what runs it when that
/// isn't aipet-hook (the C#'s ran under dotnet; here it is the hook under another name). Neither writes anything.
#[test]
fn install_answers_as_install_run() {
    let corpus = load(&golden_dir().join("claude.json"));
    let adapt = Adapt::of(&corpus);
    let scratch = new_scratch("run");
    let config = scratch.0.join("claude");
    fs::create_dir_all(&config).unwrap();
    let renamed = renamed_hook(&scratch);
    for case in corpus["run"].as_array().unwrap() {
        let args: Vec<&str> = case["args"].as_array().unwrap().iter().map(text).collect();
        let refused = text(&case["stderr"]).contains("{exe}");
        let program = if refused { renamed.clone() } else { PathBuf::from(HOOK) };
        let out = Command::new(&program)
            .args(&args)
            .env("CLAUDE_CONFIG_DIR", &config)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let expected = |key: &str| {
            adapt.text(
                &text(&case[key])
                    .replace("{exe}", &program.display().to_string())
                    .replace("{settings}", &config.join("settings.json").display().to_string()),
            )
        };
        assert_eq!(
            out.status.code(),
            case["exit"].as_i64().map(|c| c as i32),
            "{args:?}: {out:?}"
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected("stdout"), "{args:?}");
        assert_eq!(String::from_utf8_lossy(&out.stderr), expected("stderr"), "{args:?}");
        assert!(
            fs::read_dir(&config).unwrap().next().is_none(),
            "{args:?} wrote something"
        );
    }
}

/// Program.Main: a mode without an agent is a usage error, never a hook run waiting on stdin (left open here). The
/// usage lines are the C#'s own.
///
/// A hook run would wait for stdin and then exit 0 without a word, so the exit code and the usage tell the two apart
/// without a clock: a fresh exe the antivirus scans can take seconds to start on a loaded machine. The bound only
/// keeps a hook that waited for ever from hanging the test.
#[test]
fn a_mode_without_an_agent_is_a_usage_error() {
    let source = |file: &str| fs::read_to_string(repo().join("src").join("AiPet.Hook").join(file)).unwrap();
    let (program, plugin_hooks, install) = (source("Program.cs"), source("PluginHooks.cs"), source("Install.cs"));
    let install_usage = "usage: aipet-hook --install|--uninstall claude|codex";
    for (mode, usage, from) in [
        ("--install", install_usage, &install),
        ("--uninstall", install_usage, &install),
        (
            "--doctor",
            "usage: aipet-hook --doctor claude|codex [--probe]",
            &program,
        ),
        (
            "--print-plugin-hooks",
            "usage: aipet-hook --print-plugin-hooks claude|codex",
            &plugin_hooks,
        ),
    ] {
        assert!(
            from.contains(&format!("\"{usage}\"")),
            "the C# no longer says {usage:?}"
        );
        let mut child = Command::new(HOOK)
            .arg(mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let _stdin = child.stdin.take();
        let until = Instant::now() + Duration::from_secs(60);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() > until {
                let _ = child.kill();
                panic!("{mode} waited on stdin for a minute");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{mode}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "", "{mode}");
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            format!("{usage}{NEWLINE}"),
            "{mode}"
        );
    }
}

// ------------------------------------------------------------------ --print-plugin-hooks
/// PluginHooksTests: the plugin's hook files as committed, byte for byte (a Windows checkout may have given them
/// CRLF, which git doesn't keep), and nothing written anywhere.
#[test]
fn print_plugin_hooks_is_the_plugin_files() {
    for (agent, file) in [
        ("claude", "hooks.json"),
        ("codex", "codex.json"),
        ("CODEX", "codex.json"),
    ] {
        let scratch = new_scratch(&format!("print-{agent}"));
        let out = print(&["--print-plugin-hooks", agent], &scratch);
        let committed: Vec<u8> = fs::read(repo().join("plugins").join("aipet").join("hooks").join(file))
            .unwrap()
            .into_iter()
            .filter(|&b| b != b'\r')
            .collect();
        assert_eq!(String::from_utf8_lossy(&out.stderr), "", "{agent}");
        assert_eq!(out.status.code(), Some(0), "{agent}");
        assert!(
            out.stdout == committed,
            "{agent}: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            fs::read_dir(&scratch.0).unwrap().next().is_none(),
            "{agent}: something was written"
        );
    }
    for rest in [&["gemini"][..], &["--agent", "claude"]] {
        let scratch = new_scratch("print-usage");
        let args: Vec<&str> = ["--print-plugin-hooks"].iter().chain(rest).copied().collect();
        let out = print(&args, &scratch);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            format!("usage: aipet-hook --print-plugin-hooks claude|codex{NEWLINE}"),
            "{args:?}"
        );
    }
}

/// The hook with `args`, its temp and data folders the scratch folder's own, at an endpoint nobody listens on.
fn print(args: &[&str], scratch: &Scratch) -> Output {
    Command::new(HOOK)
        .args(args)
        .env("TMPDIR", &scratch.0)
        .env("TMP", &scratch.0)
        .env("TEMP", &scratch.0)
        .env("AIPET_DATA_DIR", scratch.0.join("data"))
        .env(
            "AIPET_PIPE",
            if cfg!(windows) {
                "aipet-registration-none".into()
            } else {
                scratch.0.join("none.sock")
            },
        )
        .stdin(Stdio::null())
        .output()
        .unwrap()
}
