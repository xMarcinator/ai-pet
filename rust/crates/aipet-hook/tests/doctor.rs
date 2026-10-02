//! `aipet-hook --doctor claude|codex [--probe]`, as a user runs it, held against the C#: rust/golden's `doctor` mode
//! (rust/golden/Doctor.cs) ran each scenario of tests/golden/doctor/doctor.json through the built C# hook in a sandbox
//! of its own. Here the built Rust hook runs it in the same sandbox, and must print the same lines and exit with the
//! same code; and, as a check, change nothing there.
//!
//! The sandbox ({root}) is every folder the hook could read or write: home/, claude/ (CLAUDE_CONFIG_DIR), codex/
//! (CODEX_HOME), data/, tmp/ and bin/, the first folder on PATH, where the scenario's fake codex is. bin/aipet-hook
//! (aipet-hook.cmd on Windows) runs the hook under test, here the Rust one, and the scenarios register it. A scenario
//! with a pet gets one of this test's own ([`Pet`]) at an endpoint of the test's, never the user's: it answers a ping
//! with the lines the C#'s pet had (and those of what it gets itself), and takes events as the pet does. What varies
//! between runs is a token on both sides: the sandbox, the endpoint, the pet's pid and clock, and durations.
//!
//! A corpus is written on one OS ("os") and replayed only there: the committed one on Windows. When `AIPET_GOLDEN`
//! names the golden generator's dll (CI sets it), the C# writes a corpus on this OS, which is replayed too; on Linux
//! that is the only replay. Some scenarios run only where they may: `ci` ones start PowerShell (Windows' --probe for
//! Codex), which only a CI runner may; `managed` ones write the organisation's Claude settings, which only a CI runner
//! that sets `AIPET_TEST_MANAGED_SETTINGS=1` may (cross-runtime.yml); `no_codex` ones would find the Codex installed
//! where the doctor looks for it on Windows, so they run only where there's none.
//!
//! When the C# changes: `dotnet build rust/golden -c Release -p:UseAppHost=false`, then
//! `dotnet rust/golden/bin/Release/net10.0/aipet-golden.dll doctor` from the repository root, then fix the port until
//! this passes. Never edit the golden file by hand.

mod common;

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use common::{FAMILY, Failure, HOOK, Scratch, THIS_OS, harness, hook_held, set_env, text};

/// Set to 1 where a scenario may write the organisation's managed Claude settings: on a CI runner, never on anyone's
/// own machine.
const MANAGED_VAR: &str = "AIPET_TEST_MANAGED_SETTINGS";

/// The time every line of this test's pet says, which the replay takes for a token as it does the C#'s.
const PET_TIME: &str = "2000-01-01 00:00:00.000";

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/doctor/doctor.json")
}

fn load(path: &Path) -> Value {
    common::load(path, "doctor")
}

/// Every scenario of a corpus written on this OS that this machine may run, through the built hook. A run must come
/// out as the C#'s, but for one that says nothing of the port: a file another process held, or a hook the loaded
/// machine made slower than the doctor's limit. That run is set aside, said, and run again, twice at most.
fn replay(corpus: &Value, scratch: &Scratch) {
    assert_eq!(text(&corpus["os"]), THIS_OS, "the doctor's answers depend on the OS");
    let mut replayed = 0;
    for (n, case) in corpus["cases"].as_array().unwrap().iter().enumerate() {
        let name = text(&case["name"]);
        // the organisation's settings are the machine's: one scenario at a time reads or writes them
        let _machine = (case["agent"] == "claude").then(machine);
        if let Some(why) = skipped(case) {
            eprintln!("{name}: skipped, {why}");
            continue;
        }
        let mut run = 1;
        loop {
            match replay_case(case, &scratch.0.join(format!("{n:02}-{run}"))) {
                Ok(()) => break,
                Err(Failure::Held(why)) if run < 3 => {
                    eprintln!("{name}, run {run} set aside: {why}");
                    run += 1;
                }
                Err(Failure::Held(why) | Failure::Differs(why)) => panic!("{name}: {why}"),
            }
        }
        replayed += 1;
    }
    assert!(replayed >= 20, "only {replayed} scenarios were replayed");
}

/// Why this machine may not run a scenario, if it may not.
fn skipped(case: &Value) -> Option<String> {
    let set = |key: &str| case.get(key) == Some(&Value::Bool(true));
    if let Some(only) = case.get("only").map(text)
        && only != FAMILY
    {
        return Some(format!("it is for {only}"));
    }
    if set("ci") && cfg!(windows) && std::env::var_os("CI").is_none_or(|ci| ci.is_empty()) {
        return Some("it starts PowerShell, which only a CI runner may (CI isn't set)".into());
    }
    if case.get("managed").is_some() && !managed_allowed() {
        return Some(format!(
            "it writes the organisation's settings, which only a CI runner may ({MANAGED_VAR} isn't 1)"
        ));
    }
    if set("no_codex") && installed_codex().is_some_and(|codex| codex.is_file()) {
        return Some("the doctor would find the Codex installed on this machine".into());
    }
    if case["agent"] == "claude" && managed_path().exists() {
        return Some(format!(
            "{} is there, which the scenarios were written without",
            managed_path().display()
        ));
    }
    None
}

fn managed_allowed() -> bool {
    std::env::var_os(MANAGED_VAR).as_deref() == Some(OsStr::new("1"))
}

/// Where the doctor looks for Codex when it isn't on PATH (Windows only).
fn installed_codex() -> Option<PathBuf> {
    cfg!(windows).then(|| {
        aipet_ipc::paths::local_app_data()
            .join("Programs")
            .join("OpenAI")
            .join("Codex")
            .join("bin")
            .join("codex.exe")
    })
}

/// The organisation's settings file the scenarios write (as rust/golden/Doctor.cs does).
fn managed_path() -> PathBuf {
    PathBuf::from(if cfg!(windows) {
        r"C:\ProgramData\ClaudeCode\managed-settings.json"
    } else if cfg!(target_os = "macos") {
        "/Library/Application Support/ClaudeCode/managed-settings.json"
    } else {
        "/etc/claude-code/managed-settings.json"
    })
}

/// The machine's Claude settings, for one scenario (or one generator run) at a time.
fn machine() -> MutexGuard<'static, ()> {
    static MACHINE: Mutex<()> = Mutex::new(());
    MACHINE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The organisation's settings a scenario has written, removed when dropped (with the folder, if it made it).
struct Managed {
    path: PathBuf,
    made: Option<PathBuf>,
}

impl Managed {
    fn write(text: &str) -> io::Result<Managed> {
        let path = managed_path();
        let dir = path.parent().unwrap().to_owned();
        let made = (!dir.exists()).then(|| dir.clone());
        fs::create_dir_all(&dir)?;
        let managed = Managed { path, made };
        fs::write(&managed.path, text)?;
        Ok(managed)
    }
}

impl Drop for Managed {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        if let Some(dir) = &self.made {
            let _ = fs::remove_dir(dir);
        }
    }
}

/// One run of a scenario in `root`: what the doctor prints and its exit code against the C#'s, and the sandbox as it
/// was.
fn replay_case(case: &Value, root: &Path) -> Result<(), Failure> {
    set_up(case, root).map_err(|e| harness("setting the scenario up", e))?;
    let _managed = match case.get("managed") {
        Some(settings) => Some(Managed::write(text(settings)).map_err(|e| harness("writing the managed settings", e))?),
        None => None,
    };
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let n = RUNS.fetch_add(1, Ordering::Relaxed);
    let pet = case["setup"].get("pet").map(|pet| {
        let recent = pet["recent"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| text(line).replace("{time}", PET_TIME))
            .collect();
        Pet::start(endpoint("p", n), recent)
    });
    let endpoint = pet
        .as_ref()
        .map_or_else(|| endpoint("n", n), |pet| pet.endpoint.clone());
    let before = tree(root).map_err(|e| harness("reading the sandbox", e))?;
    let out = doctor(case, root, &endpoint);
    drop(pet);
    let after = tree(root).map_err(|e| harness("reading the sandbox", e))?;

    let shown = endpoint.to_string_lossy();
    let said = |bytes: &[u8]| tokens(&String::from_utf8_lossy(bytes), root, &shown);
    let (stdout, stderr) = (said(&out.stdout), said(&out.stderr));
    let (want_out, want_err) = (text(&case["stdout"]), text(&case["stderr"]));
    // a run the loaded machine made slow, or one another process kept from a file, says nothing of the port
    let slow = stdout.contains("took {ms} ms") && !want_out.contains("took {ms} ms");
    let failure = if slow || hook_held(&stdout) {
        Failure::Held
    } else {
        Failure::Differs
    };
    let differ = |what: &str, rust: &dyn std::fmt::Debug, csharp: &dyn std::fmt::Debug| {
        Err(failure(format!(
            "{what}:
  the hook: {rust:#?}
  the C#:   {csharp:#?}"
        )))
    };
    if out.status.code() != case["exit"].as_i64().map(|code| code as i32) {
        return differ("exit", &(out.status, &stdout), &(&case["exit"], want_out));
    }
    if stdout != want_out {
        return differ("stdout", &stdout, &want_out);
    }
    if stderr != want_err {
        return differ("stderr", &stderr, &want_err);
    }
    if before != after {
        return Err(Failure::Differs(format!(
            "the doctor changed the sandbox:
  before: {before:#?}
  after:  {after:#?}"
        )));
    }
    Ok(())
}

/// An endpoint of this test's: a pipe name, or a socket path short enough for one.
fn endpoint(what: &str, n: usize) -> OsString {
    let pid = std::process::id();
    if cfg!(windows) {
        format!("aipet-doctor-{what}-{pid}-{n}").into()
    } else {
        format!("/tmp/aipet-d{what}-{pid}-{n}.sock").into()
    }
}

// ------------------------------------------------------------------ the sandbox
/// The scenario's sandbox as its setup says, the fixtures' tokens filled in as rust/golden/Doctor.cs fills them; and
/// the hook under test where the scenarios run it.
fn set_up(case: &Value, root: &Path) -> io::Result<()> {
    let setup = &case["setup"];
    let list = |key: &str| -> Vec<String> {
        setup[key]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| text(v).to_owned())
            .collect()
    };
    for dir in ["home", "data", "tmp", "bin"]
        .map(String::from)
        .into_iter()
        .chain(list("dirs"))
    {
        fs::create_dir_all(root.join(dir))?;
    }
    for (path, content) in setup["files"].as_object().unwrap() {
        let json = path.ends_with(".json") || path.ends_with(".jsonl");
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap())?;
        fs::write(file, expanded(text(content), root, json))?;
    }
    fs::write(root.join("home").join("bash.exe"), "")?;
    let runs_hook = if cfg!(windows) {
        format!("@\"{HOOK}\" %*\r\n")
    } else {
        format!("#!/bin/sh\nexec '{HOOK}' \"$@\"\n")
    };
    let first = if cfg!(windows) {
        "bin/aipet-hook.cmd"
    } else {
        "bin/aipet-hook"
    };
    let copies: Vec<String> = std::iter::once(first.to_owned()).chain(list("hook_copies")).collect();
    for copy in &copies {
        let file = root.join(copy);
        fs::create_dir_all(file.parent().unwrap())?;
        fs::write(file, &runs_hook)?;
    }
    #[cfg(unix)]
    for path in list("executable").iter().chain(&copies) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join(path), fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// A fixture's text with the sandbox in it: `{root}` (in a JSON file, as a JSON string's text), `{root-json}` and
/// `{root-fwd}` (with `/`).
fn expanded(text: &str, root: &Path, json: bool) -> String {
    let root = root.to_string_lossy();
    let json_root = serde_json::to_string(&root).unwrap();
    let json_root = &json_root[1..json_root.len() - 1];
    text.replace("{root-json}", json_root)
        .replace("{root-fwd}", &root.replace('\\', "/"))
        .replace("{root}", if json { json_root } else { &root })
}

/// The environment the doctor gets: every folder in the sandbox, and PATH with only bin/ and what the fake codex
/// needs (the system's commands). The C# gave the same.
fn doctor_env(root: &Path, endpoint: &OsStr, codex: bool) -> Vec<(&'static str, OsString)> {
    let at = |parts: &[&str]| -> OsString { parts.iter().fold(root.to_owned(), |p, part| p.join(part)).into() };
    let bin = root.join("bin");
    let path: OsString = if cfg!(windows) {
        let windows = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        format!("{};{}", bin.display(), Path::new(&windows).join("System32").display()).into()
    } else if codex {
        format!("{}:/usr/bin:/bin", bin.display()).into()
    } else {
        bin.into()
    };
    vec![
        ("HOME", at(&["home"])),
        ("USERPROFILE", at(&["home"])),
        ("XDG_CONFIG_HOME", at(&["home", ".config"])),
        ("XDG_DATA_HOME", at(&["home", ".local", "share"])),
        ("XDG_STATE_HOME", at(&["home", ".local", "state"])),
        ("CLAUDE_CONFIG_DIR", at(&["claude"])),
        ("CODEX_HOME", at(&["codex"])),
        ("AIPET_DATA_DIR", at(&["data"])),
        ("TMPDIR", at(&["tmp"])),
        ("TMP", at(&["tmp"])),
        ("TEMP", at(&["tmp"])),
        ("AIPET_PIPE", endpoint.to_owned()),
        ("SHELL", "/bin/sh".into()),
        ("CLAUDE_CODE_GIT_BASH_PATH", at(&["home", "bash.exe"])),
        ("PATH", path),
    ]
}

/// `aipet-hook --doctor <agent> [--probe]` in the sandbox, which must finish within 2 minutes (its own limits are
/// 10 s, 30 s and 30 s at most).
fn doctor(case: &Value, root: &Path, endpoint: &OsStr) -> Output {
    let mut doctor = Command::new(HOOK);
    doctor.args(["--doctor", text(&case["agent"])]);
    if case["probe"] == true {
        doctor.arg("--probe");
    }
    doctor
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let codex = case["setup"]["files"]
        .as_object()
        .unwrap()
        .keys()
        .any(|k| k.starts_with("bin/codex"));
    for (name, value) in doctor_env(root, endpoint, codex) {
        set_env(&mut doctor, name, Some(value.as_os_str()));
    }
    // the tests run inside agents too
    for name in aipet_ipc::protocol::CLAUDE_ENV {
        set_env(&mut doctor, name, None);
    }
    let mut child = doctor.spawn().unwrap();
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = read(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let stderr = read(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let until = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the doctor didn't finish within 2 minutes in {}", root.display());
        }
        thread::sleep(Duration::from_millis(20));
    };
    Output {
        status,
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    }
}

/// What varies between runs, as tokens (`Tokens` in rust/golden/Doctor.cs): the sandbox, the pet's endpoint, pid and
/// clock, and durations.
fn tokens(text: &str, root: &Path, endpoint: &str) -> String {
    let root = root.to_string_lossy();
    let text = text
        .replace(root.as_ref(), "{root}")
        .replace(&root.replace('\\', "/"), "{root-fwd}")
        .replace(endpoint, "{endpoint}");
    let digits = |s: &[char], from: usize| s[from.min(s.len())..].iter().take_while(|c| c.is_ascii_digit()).count();
    // \(pid \d+\)
    let text = replace(&text, "(pid {pid})", |s, i| {
        let after = i + "(pid ".len();
        let n = digits(s, after);
        (starts(s, i, "(pid ") && n > 0 && s.get(after + n) == Some(&')')).then_some(after + n + 1 - i)
    });
    // \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}
    let text = replace(&text, "{time}", |s, i| {
        const SHAPE: &str = "0000-00-00 00:00:00.000";
        let fits = SHAPE.chars().enumerate().all(|(k, c)| {
            s.get(i + k)
                .is_some_and(|&got| if c == '0' { got.is_ascii_digit() } else { got == c })
        });
        fits.then_some(SHAPE.len())
    });
    // \b\d+ ms\b
    let word = |c: &char| c.is_alphanumeric() || *c == '_';
    replace(&text, "{ms} ms", |s, i| {
        let n = digits(s, i);
        let end = i + n + " ms".len();
        (n > 0 && (i == 0 || !word(&s[i - 1])) && starts(s, i + n, " ms") && s.get(end).is_none_or(|c| !word(c)))
            .then_some(n + " ms".len())
    })
}

fn starts(s: &[char], at: usize, prefix: &str) -> bool {
    prefix.chars().enumerate().all(|(k, c)| s.get(at + k) == Some(&c))
}

/// The text with what `matches` finds at a position (how many characters) in place of `with`, left to right.
fn replace(text: &str, with: &str, matches: impl Fn(&[char], usize) -> Option<usize>) -> String {
    let s: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < s.len() {
        match matches(&s, i) {
            Some(n) => {
                out.push_str(with);
                i += n;
            }
            None => {
                out.push(s[i]);
                i += 1;
            }
        }
    }
    out
}

/// The startup caches PowerShell 7 and Windows PowerShell write under their LocalApplicationData, which the --probe
/// scenarios' USERPROFILE puts in the sandbox's home: the shell's own files, not the doctor's.
const POWERSHELL_CACHES: [&str; 2] = [
    "home/AppData/Local/Microsoft/PowerShell",
    "home/AppData/Local/Microsoft/Windows/PowerShell",
];

/// What is in the sandbox but tmp/ (where the hook may leave a trace log) and PowerShell's caches: each file's bytes,
/// and the folders.
fn tree(root: &Path) -> io::Result<BTreeMap<String, Option<Vec<u8>>>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Option<Vec<u8>>>) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            if rel == "tmp" || POWERSHELL_CACHES.contains(&rel.as_str()) {
                continue;
            }
            if path.is_dir() {
                out.insert(rel, None);
                walk(root, &path, out)?;
            } else {
                out.insert(rel, Some(fs::read(&path)?));
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out)?;
    // and the folders PowerShell made only for its caches, deepest first
    for dir in [
        "home/AppData/Local/Microsoft/Windows",
        "home/AppData/Local/Microsoft",
        "home/AppData/Local",
        "home/AppData",
    ] {
        let inside = format!("{dir}/");
        if out.get(dir) == Some(&None) && !out.keys().any(|rel| rel.starts_with(&inside)) {
            out.remove(dir);
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------ the pet
/// A pet of the test's own, at an endpoint of the test's: it takes one request a connection and answers it as the
/// C#'s HookServer does for what the doctor sends. A ping gets `{ok, app, pid, recent}`, the recent lines being the
/// C#'s pet's (`{time}` as [`PET_TIME`]) and then one for each event it got since, which names the session by its
/// first 13 characters, as the pet's do. It stops when dropped.
struct Pet {
    endpoint: OsString,
    stop: Arc<AtomicBool>,
    serving: Option<thread::JoinHandle<()>>,
}

impl Pet {
    fn start(endpoint: OsString, recent: Vec<String>) -> Pet {
        let stop = Arc::new(AtomicBool::new(false));
        let serving = sys::serve(&endpoint, Mutex::new(recent), stop.clone());
        Pet {
            endpoint,
            stop,
            serving: Some(serving),
        }
    }
}

impl Drop for Pet {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        sys::wake(&self.endpoint);
        if let Some(serving) = self.serving.take() {
            let _ = serving.join();
        }
        sys::clean(&self.endpoint);
    }
}

/// The pet's answer to one request line.
fn answer(line: &str, lines: &Mutex<Vec<String>>) -> String {
    let request: Value = serde_json::from_str(line).unwrap_or_default();
    let mut lines = lines.lock().unwrap();
    match request["type"].as_str() {
        Some("ping") => json!({ "ok": true, "app": "AiPet", "pid": std::process::id(), "recent": lines.clone() }),
        Some("event") => {
            let payload = &request["payload"];
            let session: String = payload["session_id"]
                .as_str()
                .unwrap_or("default")
                .chars()
                .take(13)
                .collect();
            lines.push(format!(
                "{PET_TIME} {} pid={} {} {session} where=? -> ignored",
                request["agent"].as_str().unwrap_or("?"),
                request["pid"],
                payload["hook_event_name"].as_str().unwrap_or_default()
            ));
            json!({ "ok": true, "outcome": "ignored" })
        }
        _ => json!({ "ok": false, "error": "unknown request" }),
    }
    .to_string()
}

#[cfg(unix)]
mod sys {
    use super::*;
    use std::os::unix::net::UnixListener;

    pub(super) fn serve(endpoint: &OsStr, lines: Mutex<Vec<String>>, stop: Arc<AtomicBool>) -> thread::JoinHandle<()> {
        let _ = fs::remove_file(endpoint);
        let listener = UnixListener::bind(endpoint).unwrap();
        listener.set_nonblocking(true).unwrap();
        thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                };
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let max = aipet_ipc::protocol::MAX_REQUEST;
                if let Ok(Some(line)) = aipet_ipc::connect::read_line(&mut stream, max) {
                    let _ = stream.write_all(format!("{}\n", answer(&line, &lines)).as_bytes());
                }
            }
        })
    }

    /// The pet's thread looks at `stop` between connections by itself.
    pub(super) fn wake(_: &OsStr) {}

    pub(super) fn clean(endpoint: &OsStr) {
        let _ = fs::remove_file(endpoint);
    }
}

#[cfg(windows)]
mod sys {
    //! A named pipe as the pet makes one: its owner is this user, whom the hook checks, and it alone may open it.
    //! The next instance is made before the one served is let go, so a client never finds none.

    use super::*;
    use std::fs::File;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_PIPE_CONNECTED, FALSE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn path(endpoint: &OsStr) -> String {
        format!(r"\\.\pipe\{}", endpoint.to_string_lossy())
    }

    /// This process's user, as an SDDL SID.
    fn me() -> String {
        // SAFETY: each call gets live buffers of the sizes given; the token and the string are freed
        unsafe {
            let mut token: HANDLE = 0;
            assert_ne!(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token), 0);
            let mut len = 0;
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len);
            // u64s, for TOKEN_USER's alignment
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            let got = GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), len, &mut len);
            CloseHandle(token);
            assert_ne!(got, 0);
            let user = &*buf.as_ptr().cast::<TOKEN_USER>();
            let mut sid = std::ptr::null_mut();
            assert_ne!(ConvertSidToStringSidW(user.User.Sid, &mut sid), 0);
            let len = (0..).take_while(|&i| *sid.add(i) != 0).count();
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(sid, len));
            LocalFree(sid.cast());
            text
        }
    }

    /// An instance of the pipe, owned by this user and open to it alone.
    fn instance(name: &[u16], sddl: &[u16], first: bool) -> File {
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: sddl is NUL-terminated; descriptor is written only
        let made = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(made, 0, "the pipe's security descriptor");
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: FALSE,
        };
        let mode = PIPE_ACCESS_DUPLEX | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
        // SAFETY: name is NUL-terminated; attributes lives through the call
        let pipe = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                64 << 10,
                64 << 10,
                0,
                &attributes,
            )
        };
        // SAFETY: ConvertStringSecurityDescriptorToSecurityDescriptorW's allocation
        unsafe { LocalFree(descriptor) };
        assert_ne!(pipe, INVALID_HANDLE_VALUE, "the pet's pipe");
        // SAFETY: a handle just made, owned from here on
        File::from(unsafe { OwnedHandle::from_raw_handle(pipe as _) })
    }

    pub(super) fn serve(endpoint: &OsStr, lines: Mutex<Vec<String>>, stop: Arc<AtomicBool>) -> thread::JoinHandle<()> {
        let name = wide(&path(endpoint));
        let me = me();
        let sddl = wide(&format!("O:{me}D:(A;;GA;;;{me})"));
        // made before the pet is said to run
        let mut waiting = instance(&name, &sddl, true);
        thread::spawn(move || {
            loop {
                // SAFETY: a synchronous instance of ours
                let connected = unsafe { ConnectNamedPipe(waiting.as_raw_handle() as HANDLE, std::ptr::null_mut()) } != 0
                    // SAFETY: reads this thread's last error
                    || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let mut served = std::mem::replace(&mut waiting, instance(&name, &sddl, false));
                if connected {
                    let max = aipet_ipc::protocol::MAX_REQUEST;
                    if let Ok(Some(line)) = aipet_ipc::connect::read_line(&mut served, max) {
                        let _ = served.write_all(format!("{}\n", answer(&line, &lines)).as_bytes());
                        // FlushFileBuffers: until the client has read it
                        let _ = served.sync_all();
                    }
                }
                // SAFETY: an instance of ours
                unsafe { DisconnectNamedPipe(served.as_raw_handle() as HANDLE) };
            }
        })
    }

    /// Opens the pipe, so the pet's thread stops waiting for a client and sees it is to stop.
    pub(super) fn wake(endpoint: &OsStr) {
        for _ in 0..50 {
            if fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path(endpoint))
                .is_ok()
            {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    pub(super) fn clean(_: &OsStr) {}
}

// ------------------------------------------------------------------ the tests
/// Every scenario of the committed corpus gives what the C#'s gave, where it was written.
#[test]
fn the_doctor_is_the_csharps() {
    let corpus = load(&golden());
    if text(&corpus["os"]) != THIS_OS {
        eprintln!(
            "skipped: doctor.json was written on {} (the live replay covers this OS)",
            corpus["os"]
        );
        return;
    }
    replay(&corpus, &Scratch::new("doctor", "golden"));
}

/// With `AIPET_GOLDEN`, the C# writes the corpus on this OS (building the C# hook first), and that is replayed too.
#[test]
fn the_csharp_on_this_os_is_replayed() {
    let Some(dll) = std::env::var_os("AIPET_GOLDEN") else {
        eprintln!("skipped: AIPET_GOLDEN doesn't name the golden generator's dll (see this file's doc)");
        return;
    };
    let scratch = Scratch::new("doctor", "live");
    let written = scratch.0.join("golden");
    let out = {
        // the C# writes the organisation's settings meanwhile, where it may
        let _machine = managed_allowed().then(machine);
        Command::new("dotnet")
            .arg(dll)
            .arg("doctor")
            .arg(&written)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    eprint!("{}", String::from_utf8_lossy(&out.stderr));
    replay(
        &load(&written.join("doctor.json")),
        &Scratch::new("doctor", "live-replay"),
    );
}

/// The pet this test gives the scenarios answers as the C#'s: a ping lists what it got, an event is taken.
#[test]
fn the_tests_pet_answers_as_the_pet() {
    let pet = Pet::start(
        endpoint("t", 0),
        vec![format!("{PET_TIME} claude pid=1 SessionStart x where=? -> idle")],
    );
    let ask = |line: &str| {
        let mut connection = aipet_ipc::connect::connect_to(&pet.endpoint, Duration::from_secs(2)).unwrap();
        serde_json::from_str::<Value>(&connection.ask(line).unwrap().unwrap()).unwrap()
    };
    let event = json!({
        "v": 1, "type": "event", "agent": "codex", "pid": 42,
        "payload": { "hook_event_name": "AipetDoctor", "session_id": "doctor1234567-aipet" }
    });
    assert_eq!(ask(&event.to_string()), json!({ "ok": true, "outcome": "ignored" }));
    let pong = ask(r#"{"v":1,"type":"ping"}"#);
    assert_eq!(pong["ok"], true);
    assert_eq!(pong["pid"], std::process::id());
    assert_eq!(
        pong["recent"],
        json!([
            format!("{PET_TIME} claude pid=1 SessionStart x where=? -> idle"),
            format!("{PET_TIME} codex pid=42 AipetDoctor doctor1234567 where=? -> ignored"),
        ])
    );
    assert_eq!(
        tokens(
            &format!("pid 7 (pid 123) at {PET_TIME}: 12 ms, x12 ms, 3 ms."),
            Path::new("/nowhere"),
            "no-such-endpoint"
        ),
        "pid 7 (pid {pid}) at {time}: {ms} ms, x12 ms, {ms} ms."
    );
}
