//! AgentSessions against the C#:
//! - the golden data (tests/golden/sessions/claude.json), which rust/golden wrote with the C#'s AgentSessions, is
//!   replayed through the port: every outcome, hook-events.log line and snapshot must match, the times bit for bit;
//! - tests/AiPet.Tests/OrderingTests.cs's Claude cases, ported (`ordering`).
//!
//! When the C# changes: `dotnet run --project rust/golden -c Release -- sessions` from the repository root, then fix
//! the port until this passes. Never edit the golden file by hand.

use std::fs;
use std::io;
use std::path::Path;
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, SystemTime};

use aipet_core::sessions::{AgentSessions, Entry};
use aipet_ipc::protocol::unix_time;
use serde_json::{Value, json};

/// A transcript_path in the case's own folder, which the replay puts in its place.
const FILES: &str = "{files}";

fn golden() -> &'static Value {
    static GOLDEN: OnceLock<Value> = OnceLock::new();
    GOLDEN.get_or_init(|| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/sessions/claude.json");
        let text = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{}: {e} (write it with: dotnet run --project rust/golden -c Release -- sessions)",
                path.display()
            )
        });
        serde_json::from_str(&text).expect("claude.json parses")
    })
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
fn every_golden_case_matches_the_csharp() {
    let golden = golden();
    let os = golden["os"].as_str().expect("the golden file names its OS");
    let (mut failures, mut replayed) = (Vec::new(), 0);
    for (i, case) in array(golden, "cases").iter().enumerate() {
        // Path.GetFileName's rules differ by OS, and the file holds the ones of the OS it was written on
        if case["os_specific"] == json!(true) && os != this_os() {
            continue;
        }
        replayed += 1;
        if let Err(e) = replay(case, i) {
            failures.push(e);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {replayed} cases differ from the C#:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The golden file holds all it was written with: each ordering test, the tables and the random chats, and every
/// outcome and Claude event among them. (A file cut short would replay too.)
#[test]
fn the_golden_data_covers_the_claude_path() {
    let cases = array(golden(), "cases");
    let named = |prefix: &str| {
        cases
            .iter()
            .filter(|c| c["name"].as_str().unwrap().starts_with(prefix))
            .count()
    };
    assert_eq!(named("ordering/"), 13, "ClaudeOrderingTests has 13 cases");
    assert!(named("random/") >= 30);
    let steps: Vec<&Value> = cases
        .iter()
        .flat_map(|c| array(c, "steps"))
        .filter(|s| s.get("envelope").is_some())
        .collect();
    for outcome in [
        "idle",
        "thinking",
        "working",
        "attention",
        "done",
        "removed",
        "ignored",
        "stale",
    ] {
        assert!(steps.iter().any(|s| s["outcome"] == outcome), "no step gives {outcome}");
    }
    for ev in [
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
    ] {
        assert!(
            steps.iter().any(|s| s["envelope"]["payload"]["hook_event_name"] == ev),
            "no step has {ev}"
        );
    }
}

/// Replays one case in a folder of its own; the first difference is the error.
fn replay(case: &Value, index: usize) -> Result<(), String> {
    let name = case["name"].as_str().expect("a case has a name");
    let dir = std::env::temp_dir().join(format!("aipet-sessions-golden-{}-{index}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let result = replay_in(case, name, &dir);
    let _ = fs::remove_dir_all(&dir);
    result
}

fn replay_in(case: &Value, name: &str, dir: &Path) -> Result<(), String> {
    let sessions = AgentSessions::new();
    for (n, step) in array(case, "steps").iter().enumerate() {
        let at = format!("{name}, step {n}");
        if let Some(file) = step["write"].as_str() {
            let bytes = file_bytes(array(step, "parts"));
            retry(|| fs::write(dir.join(file), &bytes));
            continue;
        }
        if let Some(file) = step["delete"].as_str() {
            retry(|| fs::remove_file(dir.join(file)));
            continue;
        }
        let mut envelope = step["envelope"].clone();
        if let Some(path) = envelope.pointer_mut("/payload/transcript_path")
            && let Some(rest) = path.as_str().and_then(|p| p.strip_prefix(FILES))
        {
            *path = Value::String(format!("{}{rest}", dir.display()));
        }
        let now = step["now"].as_f64().expect("a step has its now");
        let applied = sessions.apply(envelope.as_object().expect("an envelope is an object"), now);
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

/// tests/AiPet.Tests/OrderingTests.cs's ClaudeOrderingTests: Claude's events, placed by `at` (when the hook started)
/// and Rank for the same tick, and a call's PreToolUse and PermissionRequest by each other.
mod ordering {
    use super::*;

    /// One chat in a store of its own, and when the test began (Board.Unix).
    struct Chat {
        sessions: AgentSessions,
        t0: f64,
    }

    fn now() -> f64 {
        unix_time(SystemTime::now())
    }

    impl Chat {
        fn new() -> Self {
            Chat {
                sessions: AgentSessions::new(),
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
            let envelope = json!({
                "v": 1, "type": "event", "agent": "claude", "at": at, "sent": at + 0.01, "pid": 4242, "env": {},
                "payload": payload,
            });
            self.sessions.apply(envelope.as_object().unwrap(), now()).outcome
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
