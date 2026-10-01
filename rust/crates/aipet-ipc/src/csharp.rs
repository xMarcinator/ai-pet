//! The Rust held against the C# itself, while the C# is in the repository:
//! - every constant of `src/AiPet.Core/Ipc.cs`, read from its source;
//! - the endpoint, the data paths and the user's folders the C# finds in the same environments. The golden
//!   generator's `ipc` mode (`rust/golden/IpcMode.cs`) prints them, and this test's own binary prints the Rust's,
//!   each a process started in the scenario's environment; this runs both when `AIPET_GOLDEN` names the dll.
//!   CI does; locally: `dotnet build rust/golden -c Release`, then
//!   `AIPET_GOLDEN=$PWD/rust/golden/bin/Release/net10.0/aipet-golden.dll cargo test -p aipet-ipc` from the
//!   repository root.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::protocol::*;
use crate::{endpoint, paths};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..")
}

/// The declarations `Name = value` of every statement in `source` that starts with `prefix`, up to its `;`.
fn declarations(source: &str, prefix: &str) -> HashMap<String, String> {
    let mut found = HashMap::new();
    for (at, _) in source.match_indices(prefix) {
        let statement = &source[at + prefix.len()..];
        let statement = &statement[..statement.find(';').expect("a statement ends in ;")];
        // commas outside quotes and braces part the declarations
        let (mut depth, mut quoted, mut start) = (0, false, 0);
        let mut parts = Vec::new();
        for (i, c) in statement.char_indices() {
            match c {
                '"' => quoted = !quoted,
                '{' if !quoted => depth += 1,
                '}' if !quoted => depth -= 1,
                ',' if !quoted && depth == 0 => {
                    parts.push(&statement[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        parts.push(&statement[start..]);
        for part in parts {
            let (name, value) = part.split_once('=').expect("Name = value");
            found.insert(name.trim().to_owned(), value.trim().to_owned());
        }
    }
    found
}

/// A C# `int` constant's value: a number, or `a << b`.
fn int(value: &str) -> i64 {
    match value.split_once("<<") {
        Some((a, b)) => int(a) << int(b),
        None => value.trim().parse().unwrap_or_else(|_| panic!("not an int: {value}")),
    }
}

/// The quoted strings of a C# string or string array.
fn strings(value: &str) -> Vec<String> {
    value.split('"').skip(1).step_by(2).map(str::to_owned).collect()
}

#[test]
fn every_constant_equals_ipc_cs() {
    let path = repo().join("src").join("AiPet.Core").join("Ipc.cs");
    let source = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}. Once the C# is gone, protocol.rs's constants are the contract as they stand",
            path.display()
        )
    });
    let ms = |d: std::time::Duration| d.as_millis() as i64;

    let ints: HashMap<String, i64> = declarations(&source, "public const int")
        .into_iter()
        .map(|(name, value)| (name, int(&value)))
        .collect();
    let rust: HashMap<String, i64> = [
        ("Version", i64::from(VERSION)),
        ("MaxRequest", MAX_REQUEST as i64),
        ("TimeoutMs", ms(TIMEOUT)),
        ("BufferSize", BUFFER_SIZE as i64),
        ("ConnectMs", ms(CONNECT)),
        ("HookBudgetMs", ms(HOOK_BUDGET)),
        ("StdinMs", ms(STDIN)),
        ("MaxString", MAX_STRING as i64),
        ("MaxDepth", MAX_DEPTH as i64),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value))
    .collect();
    assert_eq!(ints, rust);

    let names: HashMap<String, Vec<String>> = declarations(&source, "public const string")
        .into_iter()
        .map(|(name, value)| (name, strings(&value)))
        .collect();
    let rust: HashMap<String, Vec<String>> = [
        ("V", V),
        ("Kind", KIND),
        ("Agent", AGENT),
        ("At", AT),
        ("Sent", SENT),
        ("Pid", PID),
        ("Env", ENV),
        ("Payload", PAYLOAD),
        ("Ok", OK),
        ("Outcome", OUTCOME),
        ("Error", ERROR),
        ("App", APP),
        ("Recent", RECENT),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), vec![value.to_owned()]))
    .collect();
    assert_eq!(names, rust);

    let lists: HashMap<String, Vec<String>> = declarations(&source, "public static readonly string[]")
        .into_iter()
        .map(|(name, value)| (name, strings(&value)))
        .collect();
    let owned = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect::<Vec<_>>();
    let rust: HashMap<String, Vec<String>> = [
        ("ToolInputKeys".to_owned(), owned(&TOOL_INPUT_KEYS)),
        ("ClaudeEnv".to_owned(), owned(&CLAUDE_ENV)),
    ]
    .into();
    assert_eq!(lists, rust);
}

/// A folder for the scenarios' paths, removed at the end.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An environment: variables set, or removed (None), on top of the test's own.
type Scenario = Vec<(&'static str, Option<OsString>)>;

fn set(name: &'static str, value: impl Into<OsString>) -> (&'static str, Option<OsString>) {
    (name, Some(value.into()))
}

fn scenarios(scratch: &Path) -> Vec<Scenario> {
    let path = |p: &str| scratch.join(p).into_os_string();
    let unset = |name: &'static str| -> (&'static str, Option<OsString>) { (name, None) };
    let no_overrides = || vec![unset("AIPET_PIPE"), unset("AIPET_DATA_DIR"), unset("CODEX_HOME")];
    let mut all = vec![
        no_overrides(),
        vec![set("AIPET_PIPE", ""), set("AIPET_DATA_DIR", ""), set("CODEX_HOME", "")],
        vec![
            set("AIPET_PIPE", "pipes/../aipet-test.sock"),
            set("AIPET_DATA_DIR", "some/./rel/../data"),
            set("CODEX_HOME", "codex/../home"),
        ],
        vec![
            set("AIPET_PIPE", path("x//./y.sock")),
            set("AIPET_DATA_DIR", path("a/b/../data/")),
        ],
    ];
    if cfg!(windows) {
        let forward = scratch.to_str().unwrap().replace('\\', "/");
        all.extend([
            // the temp folder has a short name on the CI runners (RUNNER~1): spelt out for the part that exists
            vec![set("AIPET_DATA_DIR", path(r"missing\data"))],
            vec![set("AIPET_DATA_DIR", scratch.as_os_str())],
            vec![
                set("AIPET_DATA_DIR", format!("{forward}/data")),
                set("CODEX_HOME", format!("{forward}/codex")),
            ],
            vec![set("AIPET_DATA_DIR", r"..\data")],
            // the profile stays, as the profile list gives it. LocalAppData is the registry's
            // %USERPROFILE%\AppData\Local, expanded with the asking process's USERPROFILE; C:\nowhere\AppData\Local
            // doesn't exist, so the shell gives none and LOCALAPPDATA stands in, for the C# and the Rust alike
            [
                no_overrides(),
                vec![set("LOCALAPPDATA", r"C:\nowhere"), set("USERPROFILE", r"C:\nowhere")],
            ]
            .concat(),
        ]);
    } else {
        let long = scratch.join("l".repeat(104));
        all.extend([
            [no_overrides(), vec![set("XDG_DATA_HOME", path("data-home"))]].concat(),
            [no_overrides(), vec![set("XDG_DATA_HOME", "relative/data")]].concat(),
            [no_overrides(), vec![set("XDG_DATA_HOME", path("missing"))]].concat(),
            [no_overrides(), vec![set("HOME", path("home")), unset("XDG_DATA_HOME")]].concat(),
            [
                no_overrides(),
                vec![set("HOME", path("home/../gone")), unset("XDG_DATA_HOME")],
            ]
            .concat(),
            [no_overrides(), vec![unset("HOME"), unset("XDG_DATA_HOME")]].concat(),
            vec![set("XDG_RUNTIME_DIR", path("run")), set("AIPET_DATA_DIR", path("data"))],
            vec![
                set("XDG_RUNTIME_DIR", path("missing/../run")),
                set("AIPET_DATA_DIR", path("data")),
            ],
            vec![
                set("XDG_RUNTIME_DIR", long.into_os_string()),
                set("AIPET_DATA_DIR", path("data")),
            ],
            vec![unset("XDG_RUNTIME_DIR"), set("AIPET_DATA_DIR", path("data"))],
        ]);
    }
    all
}

/// The command, to be started in the scenario's environment: the test's own, with the scenario's variables set or
/// removed.
fn in_scenario<'a>(command: &'a mut Command, scenario: &Scenario) -> &'a mut Command {
    for (name, value) in scenario {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    command
}

/// Set only for the child [`paths_equal_the_csharps_in_the_same_environment`] starts: without it,
/// [`print_this_process_paths`] does nothing.
const CHILD: &str = "AIPET_IPC_TEST_PATHS_CHILD";
/// What the child writes before its values, so the test harness's own output can be told apart.
const MARK: &str = "aipet-ipc paths: ";

/// What the golden generator's ipc mode prints, under the same names: the values the hook and the pet read in this
/// process's environment, through the same calls and caches.
fn rust_paths() -> BTreeMap<String, Option<String>> {
    let text = |p: &Path| Some(p.to_str().expect("a UTF-8 path").to_owned());
    #[cfg(unix)]
    let effective_uid = endpoint::effective_uid();
    // the C#'s reads /proc/self/status
    #[cfg(windows)]
    let effective_uid = None;
    [
        (
            "endpoint",
            Some(endpoint::endpoint().to_str().expect("a UTF-8 endpoint").to_owned()),
        ),
        ("data_dir", text(paths::data_dir())),
        ("config", text(&paths::config())),
        ("log", text(&paths::log())),
        ("hook_events_log", text(&paths::hook_events_log())),
        ("codex_home", text(&paths::codex_home())),
        ("home", text(paths::home())),
        ("local_app_data", text(paths::local_app_data())),
        ("effective_uid", effective_uid),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value))
    .collect()
}

/// The Rust's half of [`paths_equal_the_csharps_in_the_same_environment`]: in the child it starts in the scenario's
/// environment, [`rust_paths`] as one JSON line after [`MARK`]. A process of its own, as the C#'s half is, because
/// the shell gives the known folders from the environment of the process that asks (see `paths::Folders::of`).
#[test]
#[ignore = "run by paths_equal_the_csharps_in_the_same_environment, in each scenario's environment"]
fn print_this_process_paths() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    println!("{MARK}{}", serde_json::to_string(&rust_paths()).unwrap());
}

#[test]
fn paths_equal_the_csharps_in_the_same_environment() {
    let Some(golden) = std::env::var_os("AIPET_GOLDEN") else {
        eprintln!("skipped: AIPET_GOLDEN doesn't name the golden generator's dll (see this module's doc)");
        return;
    };
    let scratch = Scratch(std::env::temp_dir().join(format!("aipet-ipc-golden-{}", std::process::id())));
    for dir in ["data-home", "home", "run", "data"] {
        std::fs::create_dir_all(scratch.0.join(dir)).unwrap();
    }
    std::fs::create_dir_all(scratch.0.join("l".repeat(104))).unwrap();
    // the helper's name as the test harness knows it: its path in the crate
    let (_, module) = module_path!().split_once("::").expect("a module of the crate");
    let helper = format!("{module}::print_this_process_paths");

    for scenario in scenarios(&scratch.0) {
        let out = in_scenario(Command::new("dotnet").arg(&golden).arg("ipc"), &scenario)
            .output()
            .expect("dotnet runs the golden generator");
        assert!(
            out.status.success(),
            "{scenario:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let csharp: BTreeMap<String, Option<String>> = serde_json::from_slice(&out.stdout).expect("one JSON object");

        let mut child = Command::new(std::env::current_exe().expect("this test's binary"));
        child
            .args([helper.as_str(), "--exact", "--ignored", "--nocapture"])
            .env(CHILD, "1");
        let out = in_scenario(&mut child, &scenario)
            .output()
            .expect("this test's binary runs");
        let said = String::from_utf8_lossy(&out.stdout);
        // the harness may have written the test's name before it, on the same line
        let values = said.split_once(MARK).and_then(|(_, rest)| rest.lines().next());
        let rust: BTreeMap<String, Option<String>> = match values {
            Some(line) if out.status.success() => serde_json::from_str(line).expect("one JSON object"),
            _ => panic!(
                "{scenario:?}: the child printed no values:\n{said}{}",
                String::from_utf8_lossy(&out.stderr)
            ),
        };
        assert_eq!(rust, csharp, "{scenario:?}");
    }
}
