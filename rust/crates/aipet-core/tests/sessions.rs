//! AgentSessions against the C#:
//! - the golden data (tests/golden/sessions/claude.json and codex.json), which rust/golden wrote with the C#'s
//!   AgentSessions, is replayed through the port: every outcome, hook-events.log line and snapshot must match, the
//!   times bit for bit;
//! - tests/AiPet.Tests/OrderingTests.cs's cases, ported (`ordering` for Claude's, `codex_ordering` for Codex's).
//!
//! When the C# changes: `dotnet run --project rust/golden -c Release -- sessions` from the repository root, then fix
//! the port until this passes. Never edit the golden files by hand.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, SystemTime};

use aipet_core::sessions::{AgentSessions, Entry, Envelope};
use aipet_ipc::protocol::unix_time;
use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{Value, json};

/// A transcript_path in the case's own folder, which the replay puts in its place.
const FILES: &str = "{files}";

/// A golden file: its cases, and each case's envelopes as the lines HookServer parsed, whose text a whole tool input's
/// call key reads (the order of its keys, the spelling of its numbers).
struct Golden {
    name: &'static str,
    value: Value,
    lines: Lines<'static>,
}

fn golden(name: &'static str) -> &'static Golden {
    static CLAUDE: OnceLock<Golden> = OnceLock::new();
    static CODEX: OnceLock<Golden> = OnceLock::new();
    let cell = match name {
        "claude.json" => &CLAUDE,
        "codex.json" => &CODEX,
        _ => unreachable!("there's no golden file {name}"),
    };
    cell.get_or_init(|| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden/sessions")
            .join(name);
        let text = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{}: {e} (write it with: dotnet run --project rust/golden -c Release -- sessions)",
                path.display()
            )
        });
        // the lines borrow the text for as long as the tests run
        let text: &'static str = Box::leak(text.into_boxed_str());
        Golden {
            name,
            value: serde_json::from_str(text).unwrap_or_else(|e| panic!("{name} parses: {e}")),
            lines: serde_json::from_str(text).unwrap_or_else(|e| panic!("{name} parses: {e}")),
        }
    })
}

#[derive(Deserialize)]
struct Lines<'a> {
    #[serde(borrow)]
    cases: Vec<CaseLines<'a>>,
}

#[derive(Deserialize)]
struct CaseLines<'a> {
    #[serde(borrow)]
    steps: Vec<StepLine<'a>>,
}

#[derive(Deserialize)]
struct StepLine<'a> {
    #[serde(borrow)]
    envelope: Option<&'a RawValue>,
}

fn array<'a>(v: &'a Value, key: &str) -> &'a Vec<Value> {
    v[key].as_array().unwrap_or_else(|| panic!("{key} is not an array"))
}

/// The OS, by the golden file's name for it.
fn this_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

#[test]
fn every_claude_case_matches_the_csharp() {
    replay_all(golden("claude.json"));
}

#[test]
fn every_codex_case_matches_the_csharp() {
    replay_all(golden("codex.json"));
}

fn replay_all(golden: &Golden) {
    let os = golden.value["os"].as_str().expect("the golden file names its OS");
    let (mut failures, mut replayed) = (Vec::new(), 0);
    for (i, (case, lines)) in array(&golden.value, "cases")
        .iter()
        .zip(&golden.lines.cases)
        .enumerate()
    {
        // Path.GetFileName's rules differ by OS, and the file holds the ones of the OS it was written on
        if case["os_specific"] == json!(true) && os != this_os() {
            continue;
        }
        replayed += 1;
        let folder = format!("{}-{i}", golden.name.trim_end_matches(".json"));
        if let Err(e) = replay(case, lines, &folder) {
            failures.push(e);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {replayed} cases in {} differ from the C#:\n{}",
        failures.len(),
        golden.name,
        failures.join("\n")
    );
}

/// Each golden file holds all it was written with: each ordering test, the tables and the random chats, and every
/// outcome and event among them. (A file cut short would replay too.)
#[test]
fn the_golden_data_covers_the_claude_path() {
    covers(
        "claude.json",
        ("ordering/", 13),
        "random/",
        &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "PermissionRequest",
            "PermissionDenied",
            "Notification",
            "Elicitation",
            "ElicitationResult",
            "PreCompact",
            "PostCompact",
            "SubagentStart",
            "SubagentStop",
            "Stop",
            "StopFailure",
            "SessionEnd",
        ],
        &[],
    );
}

#[test]
fn the_golden_data_covers_the_codex_path() {
    covers(
        "codex.json",
        // CodexOrderingTests: 27 facts and 2 theories of 3
        ("ordering/codex/", 33),
        "random-codex/",
        &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PermissionRequest",
            "PostToolUse",
            "Stop",
            "Interrupt",
            "PreCompact",
            "PostCompact",
            "SubagentStart",
            "SubagentStop",
            "SessionEnd",
        ],
        &["error:FormatException"],
    );
}

fn covers(file: &'static str, (ordering, tests): (&str, usize), random: &str, events: &[&str], more: &[&str]) {
    let cases = array(&golden(file).value, "cases");
    let named = |prefix: &str| {
        cases
            .iter()
            .filter(|c| c["name"].as_str().unwrap().starts_with(prefix))
            .count()
    };
    assert_eq!(named(ordering), tests, "{file}: the ordering tests");
    assert!(named(random) >= 30, "{file}: the random cases");
    let steps: Vec<&Value> = cases
        .iter()
        .flat_map(|c| array(c, "steps"))
        .filter(|s| s.get("envelope").is_some())
        .collect();
    let outcomes = [
        "idle",
        "thinking",
        "working",
        "attention",
        "done",
        "removed",
        "ignored",
        "stale",
    ];
    for outcome in outcomes.iter().chain(more) {
        assert!(
            steps.iter().any(|s| s["outcome"] == *outcome),
            "{file}: no step gives {outcome}"
        );
    }
    for ev in events {
        assert!(
            steps.iter().any(|s| s["envelope"]["payload"]["hook_event_name"] == *ev),
            "{file}: no step has {ev}"
        );
    }
}

/// Replays one case in a folder of its own, which is its CODEX_HOME too; the first difference is the error.
fn replay(case: &Value, lines: &CaseLines, folder: &str) -> Result<(), String> {
    let name = case["name"].as_str().expect("a case has a name");
    let dir = std::env::temp_dir().join(format!("aipet-sessions-golden-{}-{folder}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let result = replay_in(case, lines, name, &dir);
    let _ = fs::remove_dir_all(&dir);
    result
}

fn replay_in(case: &Value, lines: &CaseLines, name: &str, dir: &Path) -> Result<(), String> {
    let sessions = AgentSessions::with_codex_home(dir);
    for (n, (step, line)) in array(case, "steps").iter().zip(&lines.steps).enumerate() {
        let at = format!("{name}, step {n}");
        let no_device = |file: &str| {
            if is_dos_device_name(file) {
                return Err(format!(
                    "{at}: the fixture {file:?} is named as a DOS device, which Windows before 11 (CI's windows-2022) \
                     opens instead of a file: rename its case in rust/golden/Sessions.cs"
                ));
            }
            Ok(())
        };
        if let Some(file) = step["write"].as_str() {
            no_device(file)?;
            let bytes = file_bytes(array(step, "parts"));
            retry(|| fs::write(dir.join(file), &bytes));
            continue;
        }
        if let Some(file) = step["delete"].as_str() {
            no_device(file)?;
            retry(|| fs::remove_file(dir.join(file)));
            continue;
        }
        let line = line.envelope.expect("a step is a write, a delete or an envelope").get();
        let mut envelope = Envelope::parse(line).expect("an envelope is an object");
        if let Some(path) = envelope
            .fields
            .get_mut("payload")
            .and_then(|p| p.get_mut("transcript_path"))
            && let Some(rest) = path.as_str().and_then(|p| p.strip_prefix(FILES))
        {
            no_device(rest)?;
            *path = Value::String(format!("{}{rest}", dir.display()));
        }
        let now = step["now"].as_f64().expect("a step has its now");
        let applied = sessions.apply(&envelope, now);
        if step["outcome"] != applied.outcome {
            return Err(format!(
                "{at}: the outcome is {:?}, the C#'s {}",
                applied.outcome, step["outcome"]
            ));
        }
        if step["log"] != applied.log.as_str() {
            return Err(format!(
                "{at}: the log line is {:?}, the C#'s {}",
                applied.log, step["log"]
            ));
        }
        if let Some(expected) = step.get("snapshot") {
            same_snapshot(expected, &sessions.snapshot()).map_err(|e| format!("{at}: {e}"))?;
        }
    }
    Ok(())
}

/// Whether Windows before 11 takes a file of this name for a device: CON, PRN, AUX, NUL, COM0-9 or LPT0-9 (the
/// digit may be a superscript ¹, ² or ³), in any case, with any extension (from the first `.` or `:`), even with
/// spaces before the extension. There `nul.jsonl` is the NUL device: what is written to it goes nowhere, and a read
/// finds nothing. On Windows 11 and Linux it is a plain file, so a fixture of such a name replays there and fails
/// only on the older Windows of CI's runner; refused everywhere, it fails on every OS.
fn is_dos_device_name(file: &str) -> bool {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let stem = name.split(['.', ':']).next().unwrap_or(name).trim_end_matches(' ');
    let stem = stem.to_ascii_uppercase();
    let port = |prefix: &str| {
        stem.strip_prefix(prefix).is_some_and(|digit| {
            let mut chars = digit.chars();
            matches!(
                (chars.next(), chars.next()),
                (Some('0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None)
            )
        })
    };
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") || port("COM") || port("LPT")
}

#[test]
fn fixtures_named_as_dos_devices_are_refused() {
    for device in [
        "NUL",
        "nul.jsonl",
        "COM1.txt",
        "LPT\u{b9}",
        "Aux.tar.gz",
        "nul .jsonl",
        "{files}/con.jsonl",
    ] {
        assert!(is_dos_device_name(device), "{device}");
    }
    for file in [
        "null.jsonl",
        "nul-char.jsonl",
        "con2",
        "COM10",
        "LPT",
        "a\u{0}b.jsonl",
        "",
    ] {
        assert!(!is_dos_device_name(file), "{file}");
    }
}

/// A file as the golden data describes it: UTF-8 text or hex bytes, each part repeated.
fn file_bytes(parts: &[Value]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for part in parts {
        let one = match part["text"].as_str() {
            Some(text) => text.as_bytes().to_vec(),
            None => {
                let hex = part["hex"].as_str().expect("a part is text or hex");
                (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect()
            }
        };
        for _ in 0..part["repeat"].as_u64().unwrap_or(1) {
            bytes.extend_from_slice(&one);
        }
    }
    bytes
}

/// A file just written can be held for a moment by a virus scanner (Windows): try again for a while.
fn retry(mut change: impl FnMut() -> io::Result<()>) {
    for attempt in 1.. {
        match change() {
            Ok(()) => return,
            Err(e) if attempt >= 100 => panic!("{e}"),
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// The fields of a snapshot's entry in the golden data.
const FIELDS: [&str; 17] = [
    "agent",
    "chat_title",
    "cwd",
    "detail",
    "dispatched",
    "dispatched_rank",
    "ended",
    "has_transcript",
    "host_id",
    "id",
    "prop",
    "state",
    "title",
    "ts",
    "turn",
    "turn_ended",
    "where",
];

fn same_snapshot(expected: &Value, actual: &[Entry]) -> Result<(), String> {
    let expected = expected.as_array().ok_or("the snapshot isn't a list")?;
    let ids = |ids: Vec<&str>| ids.join(" ");
    if expected.len() != actual.len() {
        return Err(format!(
            "the chats are {}, the C#'s {}",
            ids(actual.iter().map(|e| e.id.as_str()).collect()),
            ids(expected.iter().map(|e| e["id"].as_str().unwrap_or("?")).collect())
        ));
    }
    for (e, a) in expected.iter().zip(actual) {
        same_entry(e, a).map_err(|m| format!("{}: {m}", a.id))?;
    }
    Ok(())
}

fn same_entry(expected: &Value, a: &Entry) -> Result<(), String> {
    let keys: Vec<&str> = expected
        .as_object()
        .ok_or("an entry isn't an object")?
        .keys()
        .map(String::as_str)
        .collect();
    if keys != FIELDS {
        return Err(format!(
            "the golden entry has the fields {keys:?}, the test compares {FIELDS:?}"
        ));
    }
    let texts = [
        ("id", Some(a.id.as_str())),
        ("agent", a.agent),
        ("state", a.state),
        ("detail", a.detail.as_deref()),
        ("prop", a.prop),
        ("title", a.title.as_deref()),
        ("chat_title", a.chat_title.as_deref()),
        ("cwd", a.cwd.as_deref()),
        ("where", a.r#where.as_deref()),
        ("host_id", a.host_id.as_deref()),
        ("turn", a.turn.as_deref()),
    ];
    for (key, actual) in texts {
        let same = match actual {
            Some(text) => expected[key] == text,
            None => expected[key].is_null(),
        };
        if !same {
            return Err(format!("{key} is {actual:?}, the C#'s {}", expected[key]));
        }
    }
    // bit for bit: System.Text.Json writes a double so that it reads back the same
    for (key, actual) in [
        ("ts", Some(a.ts)),
        ("dispatched", Some(a.dispatched)),
        ("ended", a.ended),
    ] {
        if expected[key].as_f64().map(f64::to_bits) != actual.map(f64::to_bits)
            || expected[key].is_null() != actual.is_none()
        {
            return Err(format!("{key} is {actual:?}, the C#'s {}", expected[key]));
        }
    }
    let others = [
        ("dispatched_rank", json!(a.dispatched_rank)),
        ("has_transcript", json!(a.has_transcript)),
        ("turn_ended", json!(a.turn_ended)),
    ];
    for (key, actual) in others {
        if expected[key] != actual {
            return Err(format!("{key} is {actual}, the C#'s {}", expected[key]));
        }
    }
    Ok(())
}

/// Board.Unix.
fn now() -> f64 {
    unix_time(SystemTime::now())
}

/// A CODEX_HOME that isn't there, so no test reads the user's session index.
fn no_codex_home() -> PathBuf {
    std::env::temp_dir().join(format!("aipet-sessions-no-codex-home-{}", std::process::id()))
}

fn none() -> Value {
    json!({})
}

/// A Bash call, as Events.Bash makes it: its tool_use_id when given, and a description in its input (which
/// PermissionRequest adds) when given.
fn bash(command: &str, tool_use_id: Option<&str>, description: Option<&str>) -> Value {
    let mut call = json!({"tool_name": "Bash", "tool_input": {"command": command}});
    if let Some(d) = description {
        call["tool_input"]["description"] = json!(d);
    }
    if let Some(id) = tool_use_id {
        call["tool_use_id"] = json!(id);
    }
    call
}

fn shows(state: &'static str, detail: &str) -> (Option<&'static str>, Option<String>) {
    (Some(state), Some(detail.into()))
}

/// tests/AiPet.Tests/OrderingTests.cs's ClaudeOrderingTests: Claude's events, placed by `at` (when the hook started)
/// and Rank for the same tick, and a call's PreToolUse and PermissionRequest by each other.
mod ordering {
    use super::*;

    /// One chat in a store of its own, and when the test began (Board.Unix).
    struct Chat {
        sessions: AgentSessions,
        t0: f64,
    }

    impl Chat {
        fn new() -> Self {
            Chat {
                sessions: AgentSessions::with_codex_home(no_codex_home()),
                t0: now(),
            }
        }

        /// An event as the tests' Events.Envelope makes it, with `extra` in its payload.
        fn apply(&self, ev: &str, at: f64, extra: Value) -> &'static str {
            let mut payload = json!({"hook_event_name": ev, "session_id": "sid"});
            payload
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let line = json!({
                "v": 1, "type": "event", "agent": "claude", "at": at, "sent": at + 0.01, "pid": 4242, "env": {},
                "payload": payload,
            })
            .to_string();
            self.sessions.apply(&Envelope::parse(&line).unwrap(), now()).outcome
        }

        fn chat(&self) -> Option<Entry> {
            self.sessions.snapshot().into_iter().find(|e| e.id == "claude:sid")
        }

        /// The chat's state and detail.
        fn shows(&self) -> (Option<&'static str>, Option<String>) {
            let chat = self.chat().expect("the chat is there");
            (chat.state, chat.detail)
        }
    }

    #[test]
    fn pre_tool_use_after_the_stop_it_preceded_is_stale() {
        let c = Chat::new();
        let t0 = c.t0;
        assert_eq!(c.apply("UserPromptSubmit", t0, json!({"prompt": "fix it"})), "thinking");
        assert_eq!(c.apply("Stop", t0 + 2.0, none()), "done");
        assert_eq!(c.apply("PreToolUse", t0 + 1.0, bash("ls", None, None)), "stale");
        assert_eq!(c.apply("PostToolUse", t0 + 1.5, bash("ls", None, None)), "stale");
        assert_eq!(c.shows(), shows("done", "Done"));
        assert_eq!(c.chat().unwrap().ts, t0 + 2.0);
    }

    #[test]
    fn same_tick_goes_by_rank() {
        let c = Chat::new();
        let t0 = c.t0;
        c.apply("UserPromptSubmit", t0, none());
        // a PreToolUse and its PermissionRequest can start in the same millisecond; the request is later in a turn
        assert_eq!(c.apply("PermissionRequest", t0 + 1.0, none()), "attention");
        assert_eq!(c.apply("PreToolUse", t0 + 1.0, bash("ls", None, None)), "stale");
        // within half a millisecond is the same tick
        assert_eq!(c.apply("PreToolUse", t0 + 1.0004, bash("ls", None, None)), "stale");
        assert_eq!(c.chat().unwrap().state, Some("attention"));
        // the same rank isn't older: a Stop in that tick is taken
        assert_eq!(c.apply("Stop", t0 + 1.0, none()), "done");
    }

    #[test]
    fn same_tick_in_turn_order_is_taken() {
        let c = Chat::new();
        let t0 = c.t0;
        c.apply("UserPromptSubmit", t0, none());
        assert_eq!(c.apply("PreToolUse", t0 + 1.0, bash("ls", None, None)), "working");
        assert_eq!(c.apply("PermissionRequest", t0 + 1.0, none()), "attention");
        assert_eq!(c.chat().unwrap().state, Some("attention"));
    }

    #[test]
    fn session_end_then_a_late_event_does_not_bring_the_chat_back() {
        let c = Chat::new();
        let t0 = c.t0;
        c.apply("UserPromptSubmit", t0, none());
        assert_eq!(c.apply("SessionEnd", t0 + 2.0, none()), "removed");
        assert_eq!(c.apply("PreToolUse", t0 + 1.0, bash("ls", None, None)), "stale");
        assert_eq!(c.apply("Stop", t0 + 2.0, none()), "stale");
        assert_eq!(c.apply("SessionEnd", t0 + 1.5, none()), "stale");
        let chat = c.chat().unwrap();
        assert_eq!(chat.ended, Some(t0 + 2.0));
        assert_eq!(chat.state, None);
        // a chat taken up again after it ended starts over
        assert_eq!(c.apply("SessionStart", t0 + 3.0, none()), "idle");
        let chat = c.chat().unwrap();
        assert_eq!(chat.ended, None);
        assert_eq!(chat.state, Some("idle"));
    }

    #[test]
    fn clock_set_back_takes_the_next_event() {
        let c = Chat::new();
        let t0 = c.t0;
        // recorded while the clock was an hour ahead: it can't order what comes after the clock is set right
        assert_eq!(c.apply("Stop", t0 + 3600.0, none()), "done");
        assert_eq!(c.apply("UserPromptSubmit", t0, none()), "thinking");
        let chat = c.chat().unwrap();
        assert_eq!(chat.state, Some("thinking"));
        assert_eq!(chat.ts, t0);
        assert_eq!(c.apply("PreToolUse", t0 - 1.0, bash("ls", None, None)), "stale");
    }

    #[test]
    fn clock_set_back_after_session_end_takes_the_chat_up_again() {
        let c = Chat::new();
        let t0 = c.t0;
        assert_eq!(c.apply("SessionEnd", t0 + 3600.0, none()), "removed");
        assert_eq!(c.apply("SessionStart", t0, none()), "idle");
        assert_eq!(c.chat().unwrap().ended, None);
    }

    #[test]
    fn permission_request_started_just_before_its_pre_tool_use_arriving_after_it_is_taken() {
        let c = Chat::new();
        let t0 = c.t0;
        c.apply("UserPromptSubmit", t0, none());
        // a plugin install's shell and wrapper started the PreToolUse's hook a few ms later than the request's
        assert_eq!(
            c.apply("PreToolUse", t0 + 1.003, bash("rm -rf build", Some("toolu_1"), None)),
            "working"
        );
        assert_eq!(
            c.apply(
                "PermissionRequest",
                t0 + 1.001,
                bash("rm -rf build", None, Some("clean"))
            ),
            "attention"
        );
        assert_eq!(c.shows(), shows("attention", "Needs your permission"));
    }

    #[test]
    fn pre_tool_use_started_just_after_its_permission_request_arriving_after_it_is_stale() {
        let c = Chat::new();
        let t0 = c.t0;
        c.apply("UserPromptSubmit", t0, none());
        assert_eq!(
            c.apply(
                "PermissionRequest",
                t0 + 1.001,
                bash("rm -rf build", None, Some("clean"))
            ),
            "attention"
        );
        // later by at (the request's hook started first), but the request for its call is in
        assert_eq!(
            c.apply("PreToolUse", t0 + 1.003, bash("rm -rf build", Some("toolu_1"), None)),
            "stale"
        );
        assert_eq!(c.chat().unwrap().state, Some("attention"));
        // its PostToolUse moves the chat on, and the same command run again right after is a call of its own
        assert_eq!(
            c.apply("PostToolUse", t0 + 1.4, bash("rm -rf build", Some("toolu_1"), None)),
            "thinking"
        );
        assert_eq!(
            c.apply("PreToolUse", t0 + 1.5, bash("rm -rf build", Some("toolu_2"), None)),
            "working"
        );
    }

    #[test]
    fn same_command_again_after_a_paired_request_is_taken() {
        let c = Chat::new();
        let t0 = c.t0;
        c.apply("UserPromptSubmit", t0, none());
        assert_eq!(
            c.apply("PreToolUse", t0 + 1.0, bash("npm test", Some("toolu_1"), None)),
            "working"
        );
        assert_eq!(
            c.apply("PermissionRequest", t0 + 1.001, bash("npm test", None, None)),
            "attention"
        );
        assert_eq!(
            c.apply("PostToolUse", t0 + 1.4, bash("npm test", Some("toolu_1"), None)),
            "thinking"
        );
        // within a second of the request, but that request has its PreToolUse already
        assert_eq!(
            c.apply("PreToolUse", t0 + 1.5, bash("npm test", Some("toolu_2"), None)),
            "working"
        );
    }

    #[test]
    fn permission_request_behind_another_calls_pre_tool_use_goes_by_at() {
        let c = Chat::new();
        let t0 = c.t0;
        c.apply("UserPromptSubmit", t0, none());
        assert_eq!(
            c.apply("PreToolUse", t0 + 1.003, bash("rm -rf build", Some("toolu_1"), None)),
            "working"
        );
        assert_eq!(
            c.apply("PreToolUse", t0 + 1.010, bash("ls", Some("toolu_2"), None)),
            "working"
        );
        // its own PreToolUse isn't the latest: something else started after it
        assert_eq!(
            c.apply("PermissionRequest", t0 + 1.001, bash("rm -rf build", None, None)),
            "stale"
        );
        assert_eq!(c.chat().unwrap().state, Some("working"));
    }

    #[test]
    fn pre_tool_use_of_another_command_or_long_after_a_request_is_taken() {
        let c = Chat::new();
        let t0 = c.t0;
        c.apply("UserPromptSubmit", t0, none());
        assert_eq!(
            c.apply("PermissionRequest", t0 + 1.0, bash("rm -rf build", None, None)),
            "attention"
        );
        assert_eq!(
            c.apply("PreToolUse", t0 + 1.002, bash("ls", Some("toolu_1"), None)),
            "working"
        );
        assert_eq!(
            c.apply("PermissionRequest", t0 + 3.0, bash("make", None, None)),
            "attention"
        );
        // more than a second apart isn't the same call: at decides
        assert_eq!(
            c.apply("PreToolUse", t0 + 4.5, bash("make", Some("toolu_2"), None)),
            "working"
        );
    }

    #[test]
    fn paired_request_does_not_move_the_latest_back() {
        let c = Chat::new();
        let t0 = c.t0;
        c.apply("UserPromptSubmit", t0, none());
        assert_eq!(
            c.apply("PreToolUse", t0 + 1.000, bash("ls", Some("toolu_a"), None)),
            "working"
        );
        assert_eq!(
            c.apply("PreToolUse", t0 + 1.004, bash("rm -rf build", Some("toolu_b"), None)),
            "working"
        );
        assert_eq!(
            c.apply("PermissionRequest", t0 + 1.001, bash("rm -rf build", None, None)),
            "attention"
        );
        // started between the request and its PreToolUse: older than the latest, which is still that PreToolUse
        assert_eq!(
            c.apply("PostToolUse", t0 + 1.0025, bash("ls", Some("toolu_a"), None)),
            "stale"
        );
        assert_eq!(c.shows(), shows("attention", "Needs your permission"));
        assert_eq!(
            c.apply("PostToolUse", t0 + 1.5, bash("rm -rf build", Some("toolu_b"), None)),
            "thinking"
        );
    }

    #[test]
    fn unknown_event_is_ignored_and_makes_no_chat() {
        let c = Chat::new();
        assert_eq!(c.apply("SomethingNew", c.t0, none()), "ignored");
        assert!(c.chat().is_none());
    }
}

/// tests/AiPet.Tests/OrderingTests.cs's CodexOrderingTests: Codex's events, placed by its own ids first: turn_id
/// (UUIDv7), tool_use_id.
mod codex_ordering {
    use super::*;

    const AGENT: &str = "019a0000-0000-7000-8000-00000000a1a1";

    /// A UUIDv7 made at `unix` seconds as Guid.CreateVersion7 makes one: its time to the ms, its other bits from n.
    fn v7(unix: f64, n: u16) -> String {
        let ms = (unix * 1000.0).floor() as u64;
        format!(
            "{:08x}-{:04x}-7{:03x}-8000-{:012x}",
            ms >> 16,
            ms & 0xFFFF,
            n & 0xFFF,
            n
        )
    }

    /// One chat in a store of its own, when the test began (Board.Unix), and three turns: real v7 ids, one second
    /// apart and a minute back, so their text sorts by when the turn started.
    struct Chat {
        sessions: AgentSessions,
        t0: f64,
        turn1: String,
        turn2: String,
        turn3: String,
    }

    impl Chat {
        fn new() -> Self {
            let start = now() - 60.0;
            let c = Chat {
                sessions: AgentSessions::with_codex_home(no_codex_home()),
                t0: now(),
                turn1: v7(start, 1),
                turn2: v7(start + 1.0, 2),
                turn3: v7(start + 2.0, 3),
            };
            assert_eq!(c.turn1.as_bytes()[14], b'7');
            assert!(c.turn1 < c.turn2 && c.turn2 < c.turn3);
            c
        }

        /// An event as the tests' Events.Envelope makes it for Codex: its turn_id when given, and `extra` in its
        /// payload.
        fn apply(&self, ev: &str, turn: Option<&str>, at: f64, extra: Value) -> &'static str {
            let mut payload = json!({"hook_event_name": ev, "session_id": "sid"});
            if let Some(turn) = turn {
                payload["turn_id"] = json!(turn);
            }
            payload
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let line = json!({
                "v": 1, "type": "event", "agent": "codex", "at": at, "sent": at + 0.01, "pid": 4242, "env": {},
                "payload": payload,
            })
            .to_string();
            self.sessions.apply(&Envelope::parse(&line).unwrap(), now()).outcome
        }

        fn chat(&self) -> Entry {
            self.sessions
                .snapshot()
                .into_iter()
                .find(|e| e.id == "codex:sid")
                .expect("the chat is there")
        }

        /// The chat's state and detail.
        fn shows(&self) -> (Option<&'static str>, Option<String>) {
            let chat = self.chat();
            (chat.state, chat.detail)
        }

        /// The chat's state, detail and title.
        fn shows_titled(&self) -> (Option<&'static str>, Option<String>, Option<String>) {
            let chat = self.chat();
            (chat.state, chat.detail, chat.title)
        }
    }

    fn titled(
        state: &'static str,
        detail: &str,
        title: &str,
    ) -> (Option<&'static str>, Option<String>, Option<String>) {
        (Some(state), Some(detail.into()), Some(title.into()))
    }

    /// A sub-agent's event: its agent_id on top of `extra`.
    fn helper(agent_id: &str, mut extra: Value) -> Value {
        extra["agent_id"] = json!(agent_id);
        extra
    }

    fn prompt(text: &str) -> Value {
        json!({"prompt": text})
    }

    fn manual() -> Value {
        json!({"trigger": "manual"})
    }

    fn call(command: &str, id: &str) -> Value {
        bash(command, Some(id), None)
    }

    #[test]
    fn earlier_turn_after_the_next_turn_started_is_stale_even_with_a_later_at() {
        let c = Chat::new();
        let (t0, turn1, turn2) = (c.t0, Some(c.turn1.as_str()), Some(c.turn2.as_str()));
        assert_eq!(c.apply("UserPromptSubmit", turn1, t0, none()), "thinking");
        assert_eq!(
            c.apply("UserPromptSubmit", turn2, t0 + 1.0, prompt("second")),
            "thinking"
        );
        // PowerShell started these hooks late: their at is after turn 2's, their ids say turn 1
        assert_eq!(c.apply("PreToolUse", turn1, t0 + 5.0, call("ls", "call_1")), "stale");
        assert_eq!(c.apply("Stop", turn1, t0 + 6.0, none()), "stale");
        assert_eq!(c.shows_titled(), titled("thinking", "Thinking", "Second"));
    }

    #[test]
    fn nothing_of_a_turn_is_taken_after_its_stop() {
        let c = Chat::new();
        let (t0, turn1) = (c.t0, Some(c.turn1.as_str()));
        c.apply("UserPromptSubmit", turn1, t0, none());
        assert_eq!(c.apply("Stop", turn1, t0 + 1.0, none()), "done");
        assert_eq!(c.apply("PreToolUse", turn1, t0 + 2.0, call("ls", "call_1")), "stale");
        assert_eq!(
            c.apply("PermissionRequest", turn1, t0 + 3.0, bash("ls", None, None)),
            "stale"
        );
        assert_eq!(c.apply("PostToolUse", turn1, t0 + 4.0, call("ls", "call_1")), "stale");
        assert_eq!(c.chat().state, Some("done"));
    }

    #[test]
    fn nothing_of_a_turn_is_taken_after_its_interrupt() {
        let c = Chat::new();
        let (t0, turn1) = (c.t0, Some(c.turn1.as_str()));
        c.apply("UserPromptSubmit", turn1, t0, none());
        assert_eq!(c.apply("Interrupt", turn1, t0 + 1.0, none()), "idle");
        assert_eq!(c.apply("PreToolUse", turn1, t0 + 2.0, call("ls", "call_1")), "stale");
        assert_eq!(c.apply("Stop", turn1, t0 + 3.0, none()), "stale");
        assert_eq!(c.shows(), shows("idle", "Interrupted"));
    }

    #[test]
    fn pre_tool_use_after_its_own_post_tool_use_is_stale() {
        let c = Chat::new();
        let (t0, turn1) = (c.t0, Some(c.turn1.as_str()));
        c.apply("UserPromptSubmit", turn1, t0, none());
        assert_eq!(
            c.apply("PostToolUse", turn1, t0 + 2.0, call("ls", "call_1")),
            "thinking"
        );
        // later by at, but its call has come further already
        assert_eq!(c.apply("PreToolUse", turn1, t0 + 3.0, call("ls", "call_1")), "stale");
        assert_eq!(c.chat().state, Some("thinking"));
        // another call of the same turn is taken
        assert_eq!(c.apply("PreToolUse", turn1, t0 + 4.0, call("ls", "call_2")), "working");
    }

    #[test]
    fn pre_tool_use_after_its_permission_request_is_stale() {
        let c = Chat::new();
        let (t0, turn1) = (c.t0, Some(c.turn1.as_str()));
        c.apply("UserPromptSubmit", turn1, t0, none());
        // PermissionRequest has no tool_use_id, and Codex adds a description to a shell command's input
        let request = bash("rm -rf build", None, Some("clean"));
        assert_eq!(c.apply("PermissionRequest", turn1, t0 + 2.0, request), "attention");
        assert_eq!(
            c.apply("PreToolUse", turn1, t0 + 3.0, call("rm -rf build", "call_1")),
            "stale"
        );
        assert_eq!(c.chat().state, Some("attention"));
        // the call now has its id, so its PostToolUse goes with it
        assert_eq!(
            c.apply("PostToolUse", turn1, t0 + 4.0, call("rm -rf build", "call_1")),
            "thinking"
        );
        assert_eq!(
            c.apply("PreToolUse", turn1, t0 + 5.0, call("rm -rf build", "call_1")),
            "stale"
        );
    }

    #[test]
    fn pre_tool_use_of_another_command_after_a_permission_request_is_taken() {
        let c = Chat::new();
        let (t0, turn1) = (c.t0, Some(c.turn1.as_str()));
        c.apply("UserPromptSubmit", turn1, t0, none());
        c.apply("PermissionRequest", turn1, t0 + 2.0, bash("rm -rf build", None, None));
        assert_eq!(c.apply("PreToolUse", turn1, t0 + 3.0, call("ls", "call_2")), "working");
    }

    #[test]
    fn own_new_turn_is_taken_even_when_its_at_is_before_the_previous_stop() {
        let c = Chat::new();
        let (t0, turn1, turn2) = (c.t0, Some(c.turn1.as_str()), Some(c.turn2.as_str()));
        c.apply("UserPromptSubmit", turn1, t0, none());
        assert_eq!(c.apply("Stop", turn1, t0 + 5.0, none()), "done");
        // turn 1's Stop hook started late; turn 2 began before it did
        assert_eq!(
            c.apply("UserPromptSubmit", turn2, t0 + 1.0, prompt("go on")),
            "thinking"
        );
        assert_eq!(
            c.apply("PreToolUse", turn2, t0 + 2.0, call("make", "call_9")),
            "working"
        );
        assert_eq!(c.apply("Stop", turn2, t0 + 1.5, none()), "done");
        let chat = c.chat();
        assert_eq!((chat.state, chat.title.as_deref()), (Some("done"), Some("Go on")));
    }

    #[test]
    fn sub_agent_after_the_chats_stop_is_stale_until_the_next_turn() {
        let c = Chat::new();
        let (t0, turn1, turn2) = (c.t0, Some(c.turn1.as_str()), Some(c.turn2.as_str()));
        let (sub1, sub2) = (v7(now() - 30.0, 11), v7(now() - 10.0, 12));
        let (sub1, sub2) = (Some(sub1.as_str()), Some(sub2.as_str()));
        c.apply("UserPromptSubmit", turn1, t0, none());
        assert_eq!(
            c.apply("PreToolUse", sub1, t0 + 1.0, helper(AGENT, call("ls", "sub_1"))),
            "working"
        );
        assert_eq!(c.apply("Stop", turn1, t0 + 2.0, none()), "done");
        assert_eq!(
            c.apply("PostToolUse", sub1, t0 + 3.0, helper(AGENT, call("ls", "sub_1"))),
            "stale"
        );
        assert_eq!(
            c.apply("PreToolUse", sub2, t0 + 4.0, helper(AGENT, call("pwd", "sub_2"))),
            "stale"
        );
        assert_eq!(c.chat().state, Some("done"));
        assert_eq!(c.apply("UserPromptSubmit", turn2, t0 + 5.0, none()), "thinking");
        assert_eq!(
            c.apply("PreToolUse", sub2, t0 + 6.0, helper(AGENT, call("pwd", "sub_3"))),
            "working"
        );
    }

    #[test]
    fn non_v7_turn_ids_only_tell_a_seen_turn_from_a_new_one() {
        let c = Chat::new();
        let t0 = c.t0;
        let (a, b, d) = (
            Some("3f2504e0-4f89-41d3-9a0c-0305e82c3301"),
            Some("7c9e6679-7425-40de-944b-e07fc1f90ae7"),
            Some("a8098c1a-f86e-41d4-a716-446655440000"),
        );
        assert_eq!(c.apply("UserPromptSubmit", a, t0, none()), "thinking");
        assert_eq!(c.apply("UserPromptSubmit", b, t0 + 1.0, none()), "thinking");
        assert_eq!(c.apply("PreToolUse", a, t0 + 2.0, call("ls", "call_1")), "stale");
        assert_eq!(c.apply("Stop", b, t0 + 3.0, none()), "done");
        assert_eq!(c.apply("PreToolUse", b, t0 + 4.0, call("ls", "call_2")), "stale");
        // a turn never seen is a new one
        assert_eq!(c.apply("UserPromptSubmit", d, t0 + 5.0, none()), "thinking");
        assert_eq!(c.apply("Stop", b, t0 + 6.0, none()), "stale");
        assert_eq!(c.chat().state, Some("thinking"));
    }

    #[test]
    fn events_without_a_turn_go_by_at() {
        let c = Chat::new();
        let (t0, turn1) = (c.t0, Some(c.turn1.as_str()));
        c.apply("UserPromptSubmit", turn1, t0 + 1.0, none());
        // SessionStart has no turn: older by at is stale, and a newer one doesn't reset a busy chat
        assert_eq!(c.apply("SessionStart", None, t0, none()), "stale");
        assert_eq!(c.apply("SessionStart", None, t0 + 2.0, none()), "thinking");
    }

    #[test]
    fn late_user_prompt_submit_of_a_seen_turn_keeps_the_state_but_names_the_chat() {
        let c = Chat::new();
        let (t0, turn1, turn2) = (c.t0, Some(c.turn1.as_str()), Some(c.turn2.as_str()));
        c.apply("UserPromptSubmit", turn1, t0, prompt("first"));
        c.apply("Stop", turn1, t0 + 1.0, none());
        // PowerShell started turn 2's prompt hook last: its call and the call's request came in first
        assert_eq!(
            c.apply("PreToolUse", turn2, t0 + 2.0, call("cargo test", "call_a")),
            "working"
        );
        let request = bash("cargo test", None, None);
        assert_eq!(c.apply("PermissionRequest", turn2, t0 + 2.5, request), "attention");
        assert_eq!(
            c.apply("UserPromptSubmit", turn2, t0 + 3.5, prompt("run the tests")),
            "stale"
        );
        assert_eq!(
            c.shows_titled(),
            titled("attention", "Needs your permission", "Run the tests")
        );
        assert_eq!(c.chat().ts, t0 + 2.5);
        // nor did it move the chat's latest event: a call that started before it is still newer than the request
        assert_eq!(c.apply("PreToolUse", turn2, t0 + 3.0, call("ls", "call_b")), "working");
    }

    #[test]
    fn user_prompt_submit_then_its_turns_events_are_taken() {
        let c = Chat::new();
        let (t0, turn1, turn2) = (c.t0, Some(c.turn1.as_str()), Some(c.turn2.as_str()));
        assert_eq!(c.apply("UserPromptSubmit", turn1, t0, prompt("first")), "thinking");
        assert_eq!(c.apply("PreToolUse", turn1, t0 + 1.0, call("ls", "call_1")), "working");
        c.apply("Stop", turn1, t0 + 2.0, none());
        assert_eq!(
            c.apply("UserPromptSubmit", turn2, t0 + 3.0, prompt("second")),
            "thinking"
        );
        assert_eq!(c.apply("PreToolUse", turn2, t0 + 4.0, call("ls", "call_2")), "working");
        assert_eq!(c.chat().title.as_deref(), Some("Second"));
    }

    #[test]
    fn new_chat_first_event_is_its_prompt_is_taken() {
        let c = Chat::new();
        let (t0, turn1) = (c.t0, Some(c.turn1.as_str()));
        assert_eq!(c.apply("UserPromptSubmit", turn1, t0, prompt("hello")), "thinking");
        let chat = c.chat();
        assert_eq!((chat.state, chat.title.as_deref()), (Some("thinking"), Some("Hello")));
    }

    #[test]
    fn permission_request_of_a_rerun_goes_to_the_rerun_not_the_earlier_call_of_the_same_command() {
        let c = Chat::new();
        let turn1 = Some(c.turn1.as_str());
        // well before now: a time more than 5 s ahead reads as a clock set back (see Stale)
        let b = c.t0 - 100.0;
        c.apply("UserPromptSubmit", turn1, b, none());
        // ran in the sandbox without approval; on Windows no PostToolUse is registered, so it stays at its PreToolUse
        assert_eq!(
            c.apply("PreToolUse", turn1, b + 1.0, call("npm test", "call_1")),
            "working"
        );
        // the rerun with escalated permissions: its request's hook started before its PreToolUse's
        let request = bash("npm test", None, Some("rerun"));
        assert_eq!(c.apply("PermissionRequest", turn1, b + 21.0, request), "attention");
        assert_eq!(
            c.apply("PreToolUse", turn1, b + 22.5, call("npm test", "call_2")),
            "stale"
        );
        assert_eq!(c.shows(), shows("attention", "Needs your permission"));
    }

    #[test]
    fn permission_request_of_a_rerun_arriving_after_its_pre_tool_use_started_before_it_is_taken() {
        let c = Chat::new();
        let turn1 = Some(c.turn1.as_str());
        let b = c.t0 - 100.0;
        c.apply("UserPromptSubmit", turn1, b, none());
        c.apply("PreToolUse", turn1, b + 1.0, call("npm test", "call_1"));
        assert_eq!(
            c.apply("PreToolUse", turn1, b + 22.5, call("npm test", "call_2")),
            "working"
        );
        // goes with call_2, whose PreToolUse started closest to it; that PreToolUse is all that's newer
        let request = bash("npm test", None, None);
        assert_eq!(c.apply("PermissionRequest", turn1, b + 21.0, request), "attention");
        assert_eq!(c.chat().state, Some("attention"));
        // call_2 has come further, so its PreToolUse coming again is stale
        assert_eq!(
            c.apply("PreToolUse", turn1, b + 22.6, call("npm test", "call_2")),
            "stale"
        );
    }

    /// Codex reruns a command that failed in the sandbox with approval a second or two later, and PowerShell starts
    /// each hook 1-4 s late, so the rerun's request can start before or after its PreToolUse and arrive either side.
    const FAST_RERUNS: [(f64, f64); 3] = [(3.5, 4.0), (2.2, 3.9), (4.8, 3.1)];

    #[test]
    fn permission_request_of_a_fast_rerun_arriving_before_its_pre_tool_use_keeps_the_chat_asking() {
        for (request, rerun) in FAST_RERUNS {
            let c = Chat::new();
            let turn1 = Some(c.turn1.as_str());
            let b = c.t0 - 100.0;
            c.apply("UserPromptSubmit", turn1, b, none());
            // no PostToolUse on Windows: call_1 stays at its PreToolUse
            assert_eq!(
                c.apply("PreToolUse", turn1, b + 1.0, call("npm test", "call_1")),
                "working"
            );
            let ask = bash("npm test", None, Some("rerun"));
            assert_eq!(c.apply("PermissionRequest", turn1, b + request, ask), "attention");
            assert_eq!(
                c.apply("PreToolUse", turn1, b + rerun, call("npm test", "call_2")),
                "stale",
                "{rerun}"
            );
            assert_eq!(c.shows(), shows("attention", "Needs your permission"));
            // it came late all the same: coming again is stale too
            assert_eq!(
                c.apply("PreToolUse", turn1, b + rerun + 0.1, call("npm test", "call_2")),
                "stale"
            );
        }
    }

    #[test]
    fn permission_request_of_a_fast_rerun_arriving_after_its_pre_tool_use_is_taken() {
        for (request, rerun) in FAST_RERUNS {
            let c = Chat::new();
            let turn1 = Some(c.turn1.as_str());
            let b = c.t0 - 100.0;
            c.apply("UserPromptSubmit", turn1, b, none());
            c.apply("PreToolUse", turn1, b + 1.0, call("npm test", "call_1"));
            assert_eq!(
                c.apply("PreToolUse", turn1, b + rerun, call("npm test", "call_2")),
                "working"
            );
            // goes with call_2, the latest of that command; call_1 is closer to it in some of these
            let ask = bash("npm test", None, Some("rerun"));
            assert_eq!(
                c.apply("PermissionRequest", turn1, b + request, ask),
                "attention",
                "{request}"
            );
            assert_eq!(c.shows(), shows("attention", "Needs your permission"));
            assert_eq!(
                c.apply("PreToolUse", turn1, b + rerun + 0.1, call("npm test", "call_2")),
                "stale"
            );
        }
    }

    #[test]
    fn same_command_again_after_the_asking_calls_post_tool_use_is_taken() {
        let c = Chat::new();
        let turn1 = Some(c.turn1.as_str());
        let b = c.t0 - 100.0;
        c.apply("UserPromptSubmit", turn1, b, none());
        c.apply("PreToolUse", turn1, b + 1.0, call("npm test", "call_1"));
        let request = bash("npm test", None, None);
        assert_eq!(c.apply("PermissionRequest", turn1, b + 1.2, request), "attention");
        // approved and run at once (a direct install outside Windows has PostToolUse): the request was call_1's
        assert_eq!(
            c.apply("PostToolUse", turn1, b + 2.0, call("npm test", "call_1")),
            "thinking"
        );
        assert_eq!(
            c.apply("PreToolUse", turn1, b + 3.0, call("npm test", "call_2")),
            "working"
        );
    }

    #[test]
    fn paired_request_does_not_move_the_latest_back() {
        let c = Chat::new();
        let turn1 = Some(c.turn1.as_str());
        let b = c.t0 - 100.0;
        c.apply("UserPromptSubmit", turn1, b, none());
        c.apply("PreToolUse", turn1, b + 1.0, call("ls", "call_a"));
        assert_eq!(
            c.apply("PreToolUse", turn1, b + 3.0, call("rm -rf build", "call_b")),
            "working"
        );
        let request = bash("rm -rf build", None, None);
        assert_eq!(c.apply("PermissionRequest", turn1, b + 2.0, request), "attention");
        // started between the request and its PreToolUse, which is still the chat's latest event
        assert_eq!(c.apply("PostToolUse", turn1, b + 2.5, call("ls", "call_a")), "stale");
        assert_eq!(c.shows(), shows("attention", "Needs your permission"));
    }

    #[test]
    fn late_user_prompt_submit_after_its_turns_stop_still_names_the_chat() {
        let c = Chat::new();
        let (t0, turn1, turn2) = (c.t0, Some(c.turn1.as_str()), Some(c.turn2.as_str()));
        c.apply("UserPromptSubmit", turn1, t0, prompt("first"));
        c.apply("Stop", turn1, t0 + 1.0, none());
        // a quick turn finished before PowerShell started its prompt's hook
        assert_eq!(c.apply("PreToolUse", turn2, t0 + 2.0, call("ls", "call_1")), "working");
        assert_eq!(c.apply("Stop", turn2, t0 + 3.0, none()), "done");
        assert_eq!(c.apply("UserPromptSubmit", turn2, t0 + 4.0, prompt("second")), "stale");
        assert_eq!(c.shows_titled(), titled("done", "Done", "Second"));
        assert_eq!(c.chat().ts, t0 + 3.0);
    }

    #[test]
    fn permission_request_behind_another_call_goes_by_at() {
        let c = Chat::new();
        let (t0, turn1) = (c.t0, Some(c.turn1.as_str()));
        c.apply("UserPromptSubmit", turn1, t0, none());
        c.apply("PreToolUse", turn1, t0 + 2.0, call("rm -rf build", "call_1"));
        assert_eq!(c.apply("PreToolUse", turn1, t0 + 3.0, call("ls", "call_2")), "working");
        let request = bash("rm -rf build", None, None);
        assert_eq!(c.apply("PermissionRequest", turn1, t0 + 1.5, request), "stale");
    }

    #[test]
    fn two_identical_calls_far_apart_the_seconds_request_goes_with_the_second() {
        let c = Chat::new();
        let turn1 = Some(c.turn1.as_str());
        let b = c.t0 - 100.0;
        c.apply("UserPromptSubmit", turn1, b, none());
        c.apply("PreToolUse", turn1, b + 1.0, call("npm test", "call_1"));
        c.apply("PreToolUse", turn1, b + 60.0, call("npm test", "call_2"));
        let request = bash("npm test", None, None);
        assert_eq!(c.apply("PermissionRequest", turn1, b + 60.5, request), "attention");
        assert_eq!(
            c.apply("PostToolUse", turn1, b + 70.0, call("npm test", "call_2")),
            "thinking"
        );
        // the request went with call_2, so call_2 has come further
        assert_eq!(
            c.apply("PreToolUse", turn1, b + 71.0, call("npm test", "call_2")),
            "stale"
        );
    }

    #[test]
    fn pending_request_is_not_taken_by_the_same_command_long_after() {
        let c = Chat::new();
        let turn1 = Some(c.turn1.as_str());
        let b = c.t0 - 100.0;
        c.apply("UserPromptSubmit", turn1, b, none());
        // a request whose PreToolUse never came waits on its own, but only for HookSpread
        let request = bash("npm test", None, None);
        assert_eq!(c.apply("PermissionRequest", turn1, b + 1.0, request), "attention");
        assert_eq!(
            c.apply("PreToolUse", turn1, b + 30.0, call("npm test", "call_3")),
            "working"
        );
    }

    #[test]
    fn manual_compact_ends_its_turn() {
        let c = Chat::new();
        let (t0, turn1, turn2, turn3) = (
            c.t0,
            Some(c.turn1.as_str()),
            Some(c.turn2.as_str()),
            Some(c.turn3.as_str()),
        );
        c.apply("UserPromptSubmit", turn1, t0, none());
        assert_eq!(c.apply("Stop", turn1, t0 + 1.0, none()), "done");
        // /compact is a turn of its own, with no Stop after it
        assert_eq!(c.apply("PreCompact", turn2, t0 + 2.0, manual()), "thinking");
        assert_eq!(c.chat().detail.as_deref(), Some("Compacting the conversation"));
        assert_eq!(c.apply("PostCompact", turn2, t0 + 3.0, manual()), "done");
        assert_eq!(c.shows(), shows("done", "Compacted"));
        let chat = c.chat();
        assert_eq!((chat.turn, chat.turn_ended), (Some(c.turn2.clone()), true));
        assert_eq!(c.apply("UserPromptSubmit", turn3, t0 + 4.0, none()), "thinking");
    }

    #[test]
    fn manual_compact_pre_compact_arriving_last_is_stale() {
        let c = Chat::new();
        let (t0, turn1, turn2) = (c.t0, Some(c.turn1.as_str()), Some(c.turn2.as_str()));
        c.apply("UserPromptSubmit", turn1, t0, none());
        c.apply("Stop", turn1, t0 + 1.0, none());
        assert_eq!(c.apply("PostCompact", turn2, t0 + 3.0, manual()), "done");
        assert_eq!(c.apply("PreCompact", turn2, t0 + 4.0, manual()), "stale");
        assert_eq!(c.shows(), shows("done", "Compacted"));
    }

    #[test]
    fn auto_compact_stays_in_its_turn() {
        let c = Chat::new();
        let (t0, turn1) = (c.t0, Some(c.turn1.as_str()));
        c.apply("UserPromptSubmit", turn1, t0, none());
        let auto = json!({"trigger": "auto"});
        assert_eq!(c.apply("PreCompact", turn1, t0 + 1.0, auto.clone()), "thinking");
        assert_eq!(c.apply("PostCompact", turn1, t0 + 2.0, auto), "thinking");
        let chat = c.chat();
        assert_eq!(
            (chat.state, chat.detail.as_deref(), chat.turn_ended),
            (Some("thinking"), Some("Thinking"), false)
        );
        assert_eq!(c.apply("PreToolUse", turn1, t0 + 3.0, call("ls", "call_1")), "working");
        assert_eq!(c.apply("Stop", turn1, t0 + 4.0, none()), "done");
    }

    #[test]
    fn sub_agents_compact_does_not_end_the_chats_turn() {
        let c = Chat::new();
        let (t0, turn1) = (c.t0, Some(c.turn1.as_str()));
        let sub = v7(now() - 30.0, 11);
        let sub = Some(sub.as_str());
        c.apply("UserPromptSubmit", turn1, t0, none());
        assert_eq!(
            c.apply("PostCompact", sub, t0 + 1.0, helper(AGENT, manual())),
            "thinking"
        );
        let chat = c.chat();
        assert_eq!(
            (chat.state, chat.detail.as_deref(), chat.turn_ended),
            (Some("thinking"), Some("Thinking"), false)
        );
        assert_eq!(
            c.apply("PreToolUse", sub, t0 + 2.0, helper(AGENT, call("ls", "sub_1"))),
            "working"
        );
        assert_eq!(c.apply("PreToolUse", turn1, t0 + 3.0, call("ls", "call_1")), "working");
    }

    #[test]
    fn clock_set_back_new_turns_are_taken_and_the_old_turn_stays_stale() {
        let c = Chat::new();
        let t0 = c.t0;
        // turn 1 began while the clock was an hour ahead, so its id sorts after every turn made once it was set right
        let (ahead, after) = (v7(now() + 3600.0, 21), v7(now(), 22));
        assert!(after < ahead);
        let (ahead_turn, after_turn) = (Some(ahead.as_str()), Some(after.as_str()));
        c.apply("UserPromptSubmit", ahead_turn, t0 + 3600.0, none());
        assert_eq!(c.apply("Stop", ahead_turn, t0 + 3601.0, none()), "done");
        assert_eq!(c.apply("UserPromptSubmit", after_turn, t0, prompt("after")), "thinking");
        assert_eq!(
            c.apply("PreToolUse", after_turn, t0 + 1.0, call("ls", "call_1")),
            "working"
        );
        assert_eq!(c.apply("Stop", after_turn, t0 + 2.0, none()), "done");
        assert_eq!(c.chat().title.as_deref(), Some("After"));
        // a late event of the old turn is still stale: it's a turn seen before
        assert_eq!(
            c.apply("PreToolUse", ahead_turn, t0 + 3602.0, call("ls", "call_0")),
            "stale"
        );
        let chat = c.chat();
        assert_eq!((chat.turn, chat.state), (Some(after), Some("done")));
    }

    #[test]
    fn new_chat_starts_from_its_first_events_turn() {
        let c = Chat::new();
        let (t0, turn1, turn2, turn3) = (
            c.t0,
            Some(c.turn1.as_str()),
            Some(c.turn2.as_str()),
            Some(c.turn3.as_str()),
        );
        // the first event to arrive is turn 2's: turn 1's events after it are stale
        assert_eq!(c.apply("PreToolUse", turn2, t0 + 2.0, call("ls", "call_1")), "working");
        assert_eq!(c.apply("UserPromptSubmit", turn1, t0 + 3.0, none()), "stale");
        assert_eq!(c.apply("Stop", turn2, t0 + 4.0, none()), "done");
        assert_eq!(c.apply("UserPromptSubmit", turn3, t0 + 5.0, none()), "thinking");
    }
}
