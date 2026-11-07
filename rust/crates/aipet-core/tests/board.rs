//! The Board and the Codex log watcher against the C#:
//! - the golden data (tests/golden/board/board.json and watcher.json), which rust/golden wrote with the C#'s Board and
//!   CodexWatcher, is replayed through the port: every refresh's bubbles, order, sections, links, caps and mood, every
//!   poll's sessions, the times bit for bit;
//! - tests/AiPet.Tests/BoardTests.cs's cases, ported (`merge` and `reviews`), with the port's AgentSessions and
//!   CodexWatcher on a fixture Codex home.
//!
//! When the C# changes: `dotnet run --project rust/golden -c Release -- board` from the repository root, then fix the
//! port until this passes. Never edit the golden files by hand.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aipet_core::board::{self, Board, Media, Session, Sources};
use aipet_core::codex_watcher::CodexWatcher;
use aipet_core::github::{self, GitHubSettings, GitHubWatcher, PullRequest};
use aipet_core::jira::{self, Http, Issue, JiraSettings, JiraWatcher};
use aipet_core::secrets::MemorySecrets;
use aipet_core::sessions::{AgentSessions, Entry, Envelope};
use aipet_ipc::protocol::unix_time;
use serde_json::{Number, Value, json};

fn golden(name: &'static str) -> &'static Value {
    static BOARD: OnceLock<Value> = OnceLock::new();
    static WATCHER: OnceLock<Value> = OnceLock::new();
    let cell = match name {
        "board.json" => &BOARD,
        "watcher.json" => &WATCHER,
        _ => unreachable!("there's no golden file {name}"),
    };
    cell.get_or_init(|| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden/board")
            .join(name);
        let text = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{}: {e} (write it with: dotnet run --project rust/golden -c Release -- board)",
                path.display()
            )
        });
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name} parses: {e}"))
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

/// The cases to replay here: those that don't depend on the OS, and on the OS the file was written on, all.
fn cases(golden: &'static Value) -> impl Iterator<Item = (usize, &'static Value)> {
    let os = golden["os"].as_str().expect("the golden file names its OS");
    array(golden, "cases")
        .iter()
        .enumerate()
        .filter(move |(_, case)| case["os_specific"] != json!(true) || os == this_os())
}

/// The golden data's vocabularies (states, agents, kinds, props) are the port's `&'static str`s.
fn leak(s: &str) -> &'static str {
    Box::leak(s.to_owned().into_boxed_str())
}

fn text(v: &Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}

fn word(v: &Value) -> Option<&'static str> {
    v.as_str().map(leak)
}

fn number(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("{v} is not a number"))
}

/// The same value with every number a double, as the C# wrote them (a whole one without its `.0`).
fn doubles(v: &Value) -> Value {
    match v {
        Value::Number(n) => Value::Number(Number::from_f64(n.as_f64().unwrap()).expect("a finite number")),
        Value::Array(items) => Value::Array(items.iter().map(doubles).collect()),
        Value::Object(members) => Value::Object(members.iter().map(|(k, v)| (k.clone(), doubles(v))).collect()),
        other => other.clone(),
    }
}

/// A bubble as the golden data has it: the C#'s null name and cwd are empty here.
fn expected_session(v: &Value) -> Value {
    let mut v = doubles(v);
    for key in ["name", "cwd"] {
        if v[key].is_null() {
            v[key] = json!("");
        }
    }
    v
}

fn session_json(s: &Session) -> Value {
    doubles(&json!({
        "id": s.id, "eff": s.eff, "name": s.name, "detail": s.detail, "prop": s.prop, "cwd": s.cwd, "agent": s.agent,
        "kind": s.kind, "url": s.url, "ticket_url": s.ticket_url, "pr_url": s.pr_url, "where": s.r#where,
        "link": s.link, "source": s.source, "turn": s.turn, "turn_ended": s.turn_ended, "ts": s.ts,
        "section": s.section(),
    }))
}

// ------------------------------------------------------------------ board.json
fn entry(v: &Value) -> Entry {
    let mut e = Entry::default();
    e.id = v["id"].as_str().expect("an entry has an id").into();
    e.agent = word(&v["agent"]);
    e.state = word(&v["state"]);
    e.detail = text(&v["detail"]);
    e.prop = word(&v["prop"]);
    e.title = text(&v["title"]);
    e.chat_title = text(&v["chat_title"]);
    e.cwd = text(&v["cwd"]);
    e.r#where = text(&v["where"]);
    e.host_id = text(&v["host_id"]);
    e.ts = number(&v["ts"]);
    e.ended = v["ended"].as_f64();
    e.has_transcript = v["has_transcript"] == json!(true);
    e.turn = text(&v["turn"]);
    e.turn_ended = v["turn_ended"] == json!(true);
    e
}

/// A watcher's session as the golden data has it.
fn session(v: &Value) -> Session {
    let or_empty = |key: &str| v[key].as_str().unwrap_or("").to_owned();
    Session {
        id: or_empty("id"),
        eff: leak(v["eff"].as_str().unwrap_or("")),
        name: or_empty("name"),
        detail: or_empty("detail"),
        prop: word(&v["prop"]),
        cwd: or_empty("cwd"),
        agent: leak(v["agent"].as_str().unwrap_or("claude")),
        kind: leak(v["kind"].as_str().unwrap_or("chat")),
        url: text(&v["url"]),
        ticket_url: text(&v["ticket_url"]),
        pr_url: text(&v["pr_url"]),
        r#where: text(&v["where"]),
        link: text(&v["link"]),
        source: word(&v["source"]),
        turn: text(&v["turn"]),
        turn_ended: v["turn_ended"] == json!(true),
        ts: number(&v["ts"]),
    }
}

fn sources(i: &Value) -> Sources {
    let or_empty = |v: &Value| v.as_str().unwrap_or("").to_owned();
    let (jira, github) = (&i["jira"], &i["github"]);
    Sources {
        chats: array(i, "chats").iter().map(entry).collect(),
        codex: array(i, "codex").iter().map(session).collect(),
        jira: JiraSettings {
            enabled: jira["enabled"] == json!(true),
            site: text(&jira["site"]),
            ..JiraSettings::default()
        },
        issues: array(jira, "issues")
            .iter()
            .map(|x| Issue {
                key: or_empty(&x["key"]),
                summary: or_empty(&x["summary"]),
                status: or_empty(&x["status"]),
                updated: number(&x["updated"]),
                url: or_empty(&x["url"]),
            })
            .collect(),
        jira_error: text(&jira["error"]),
        github: GitHubSettings {
            enabled: github["enabled"] == json!(true),
            ..GitHubSettings::default()
        },
        review_requests: array(github, "review_requests")
            .iter()
            .map(|p| PullRequest {
                repo: or_empty(&p["repo"]),
                number: p["number"]
                    .as_i64()
                    .and_then(|n| i32::try_from(n).ok())
                    .expect("a number"),
                title: or_empty(&p["title"]),
                url: or_empty(&p["url"]),
                updated: number(&p["updated"]),
                keys: array(p, "keys").iter().map(or_empty).collect(),
            })
            .collect(),
        pr_for_key: github["pr_for_key"]
            .as_object()
            .expect("pr_for_key is an object")
            .iter()
            .map(|(k, v)| (k.clone(), or_empty(v)))
            .collect(),
        github_error: text(&github["error"]),
        media: (!i["media"].is_null()).then(|| Media {
            name: or_empty(&i["media"]["name"]),
            song: text(&i["media"]["song"]),
            artist: or_empty(&i["media"]["artist"]),
            playing: i["media"]["playing"] == json!(true),
            track_since: number(&i["media"]["track_since"]),
        }),
        music_on: i["music_on"] == json!(true),
    }
}

/// Where the board differs from the C#'s, if it does.
fn same_board(expected: &Value, board: &Board) -> Result<(), String> {
    let all = array(expected, "all");
    if all.len() != board.all().len() {
        return Err(format!(
            "{} bubbles, the C#'s {}: {:?} against {:?}",
            board.all().len(),
            all.len(),
            board.all().iter().map(|s| &s.id).collect::<Vec<_>>(),
            all.iter().map(|s| &s["id"]).collect::<Vec<_>>()
        ));
    }
    for (n, (e, s)) in all.iter().zip(board.all()).enumerate() {
        let (e, s) = (expected_session(e), session_json(s));
        if e != s {
            return Err(format!("bubble {n} is\n  {s}\nthe C#'s\n  {e}"));
        }
    }
    let cards: Vec<&str> = board.cards().iter().map(|s| s.id.as_str()).collect();
    let expected_cards: Vec<&str> = array(expected, "cards").iter().map(|c| c.as_str().unwrap()).collect();
    if cards != expected_cards {
        return Err(format!("the cards are {cards:?}, the C#'s {expected_cards:?}"));
    }
    for section in ["chats", "reviews", "music"] {
        if json!(board.extra(section)) != expected["extra"][section] {
            return Err(format!(
                "{section} leaves out {}, the C#'s {}",
                board.extra(section),
                expected["extra"][section]
            ));
        }
    }
    if json!(board.state()) != expected["state"] || json!(board.prop()) != expected["prop"] {
        return Err(format!(
            "the mood is {:?} with {:?}, the C#'s {} with {}",
            board.state(),
            board.prop(),
            expected["state"],
            expected["prop"]
        ));
    }
    let mut dismissed: Vec<(&String, &board::Dismissal)> = board.dismissed().iter().collect();
    dismissed.sort_by_key(|(id, _)| id.as_str());
    let dismissed = doubles(&Value::Array(
        dismissed
            .into_iter()
            .map(|(id, d)| json!({"id": id, "ts": d.ts, "eff": d.eff, "detail": d.detail}))
            .collect(),
    ));
    if dismissed != doubles(&expected["dismissed"]) {
        return Err(format!("dismissed are {dismissed}, the C#'s {}", expected["dismissed"]));
    }
    Ok(())
}

fn replay_board(case: &Value) -> Result<(), String> {
    let name = case["name"].as_str().expect("a case has a name");
    let mut board = Board::new();
    for (n, step) in array(case, "steps").iter().enumerate() {
        let now = number(&step["now"]);
        if let Some(id) = step["dismiss"].as_str() {
            board.dismiss(id, now);
            continue;
        }
        board.refresh(&sources(&step["inputs"]), now);
        same_board(&step["board"], &board).map_err(|e| format!("{name}, step {n}: {e}"))?;
    }
    Ok(())
}

#[test]
fn every_board_case_matches_the_csharp() {
    let (mut failures, mut replayed) = (Vec::new(), 0);
    for (_, case) in cases(golden("board.json")) {
        replayed += 1;
        if let Err(e) = replay_board(case) {
            failures.push(e);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {replayed} board cases differ from the C#:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn app_agent_and_pull_request_labels_match_the_csharp() {
    let labels = &golden("board.json")["labels"];
    for x in array(labels, "app_of") {
        let s = Session {
            agent: leak(x["agent"].as_str().unwrap()),
            r#where: text(&x["where"]),
            ..Session::default()
        };
        let (label, color) = board::app_of(&s);
        assert_eq!(
            (json!(label), json!(color)),
            (x["label"].clone(), x["color"].clone()),
            "app_of {x}"
        );
    }
    for x in array(labels, "agent_label") {
        assert_eq!(
            json!(board::agent_label(x["agent"].as_str().unwrap())),
            x["label"],
            "{x}"
        );
    }
    for x in array(labels, "pr_label") {
        assert_eq!(json!(board::pr_label(x["url"].as_str().unwrap())), x["label"], "{x}");
    }
}

/// The golden file holds what it was written with: each Board test, the tables and the generated merges.
#[test]
fn the_golden_data_covers_the_board() {
    let all = array(golden("board.json"), "cases");
    let named = |prefix: &str| {
        all.iter()
            .filter(|c| c["name"].as_str().unwrap().starts_with(prefix))
            .count()
    };
    // BoardMergeTests' 6 and BoardReviewTests' 4
    assert_eq!(named("tests/"), 10);
    assert!(named("random-merges/") >= 12);
    for table in [
        "states-by-age",
        "hook-chats",
        "folders",
        "log-chats",
        "reviews",
        "caps",
        "music",
        "mood",
        "dismiss",
    ] {
        assert!(all.iter().any(|c| c["name"] == table), "no case {table}");
    }
    let steps: Vec<&Value> = all.iter().flat_map(|c| array(c, "steps")).collect();
    let bubbles: Vec<&Value> = steps
        .iter()
        .filter(|s| s.get("board").is_some())
        .flat_map(|s| array(&s["board"], "all"))
        .collect();
    // both sides win merges, links of both apps, and every kind of bubble
    for (key, value) in [
        ("source", "hook"),
        ("source", "log"),
        ("kind", "jira"),
        ("kind", "github"),
        ("kind", "music"),
        ("kind", "jira-error"),
        ("kind", "github-error"),
        ("eff", "paused"),
    ] {
        assert!(bubbles.iter().any(|b| b[key] == value), "no bubble has {key} {value}");
    }
    for link in ["claude://claude.ai/epitaxy/", "codex://threads/"] {
        assert!(
            bubbles
                .iter()
                .any(|b| b["link"].as_str().is_some_and(|l| l.starts_with(link))),
            "no {link} link"
        );
    }
    assert!(steps.iter().any(|s| s.get("dismiss").is_some()));
}

// ------------------------------------------------------------------ watcher.json
/// Unix ms as Codex writes a time: `2026-09-30T12:34:56.789Z`.
fn iso(ms: i64) -> String {
    let (days, ms_of_day) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    // days since 1970 to a civil date (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let (era, doe) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        ms_of_day / 3_600_000,
        ms_of_day / 60_000 % 60,
        ms_of_day / 1000 % 60,
        ms_of_day % 1000
    )
}

/// A file's text with each `%T<seconds>%` the time T plus those seconds.
fn times(text: &str, t0: i64) -> String {
    let (mut out, mut rest) = (String::new(), text);
    while let Some(i) = rest.find("%T") {
        out.push_str(&rest[..i]);
        let end = rest[i + 2..].find('%').expect("a time ends in %") + i + 2;
        let seconds: f64 = rest[i + 2..end].parse().expect("a time's seconds");
        out.push_str(&iso(t0 + (seconds * 1000.0).round() as i64));
        rest = &rest[end + 1..];
    }
    out + rest
}

fn file_bytes(parts: &[Value], t0: i64) -> Vec<u8> {
    let mut bytes = Vec::new();
    for part in parts {
        let one = match part["text"].as_str() {
            Some(text) => times(text, t0).into_bytes(),
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

fn set_time(path: &Path, ms: i64) -> io::Result<()> {
    File::options()
        .write(true)
        .open(path)?
        .set_modified(UNIX_EPOCH + Duration::from_millis(ms.try_into().expect("a time after 1970")))
}

fn replay_watcher(case: &Value, home: &Path) -> Result<(), String> {
    let name = case["name"].as_str().expect("a case has a name");
    // T: a whole second, as the C#'s was
    let t0 = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64 * 1000;
    let watcher = CodexWatcher::with_codex_home(home);
    for (n, step) in array(case, "steps").iter().enumerate() {
        if let Some(file) = step["write"].as_str() {
            let path = home.join(file);
            retry(|| fs::create_dir_all(path.parent().unwrap()));
            let bytes = file_bytes(array(step, "parts"), t0);
            let mtime = step["mtime"].as_i64().expect("a written file has its time");
            retry(|| fs::write(&path, &bytes).and_then(|()| set_time(&path, t0 + mtime * 1000)));
        } else if let Some(folder) = step["folder"].as_str() {
            retry(|| fs::create_dir_all(home.join(folder)));
        } else {
            watcher.poll();
            let got: Vec<Value> = watcher
                .sessions()
                .iter()
                .map(|s| {
                    json!({
                        "id": s.id, "eff": s.eff, "name": s.name, "detail": s.detail, "prop": s.prop, "cwd": s.cwd,
                        "agent": s.agent, "kind": s.kind, "where": s.r#where, "source": s.source, "turn": s.turn,
                        "turn_ended": s.turn_ended, "ts_ms": (s.ts * 1000.0).round() as i64 - t0,
                    })
                })
                .collect();
            let expected: Vec<Value> = array(step, "poll")
                .iter()
                .map(|s| {
                    let mut s = s.clone();
                    if s["name"].is_null() {
                        s["name"] = json!("");
                    }
                    s
                })
                .collect();
            if got != expected {
                return Err(format!(
                    "{name}, step {n}: the watcher gives\n  {}\nthe C#'s\n  {}",
                    Value::Array(got),
                    Value::Array(expected)
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn every_watcher_case_matches_the_csharp() {
    let (mut failures, mut replayed) = (Vec::new(), 0);
    for (i, case) in cases(golden("watcher.json")) {
        replayed += 1;
        let home = std::env::temp_dir().join(format!("aipet-board-golden-{}-{i}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        if let Err(e) = replay_watcher(case, &home) {
            failures.push(e);
        }
        let _ = fs::remove_dir_all(&home);
    }
    assert!(
        failures.is_empty(),
        "{} of {replayed} watcher cases differ from the C#:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn the_golden_data_covers_the_watcher() {
    let all = array(golden("watcher.json"), "cases");
    for name in [
        "basic",
        "meta",
        "events",
        "names",
        "index-tail",
        "recency",
        "discovery",
        "incremental",
        "tails",
    ] {
        assert!(all.iter().any(|c| c["name"] == name), "no case {name}");
    }
    let sessions: Vec<&Value> = all
        .iter()
        .flat_map(|c| array(c, "steps"))
        .filter_map(|s| s["poll"].as_array())
        .flatten()
        .collect();
    for eff in ["thinking", "working", "done", "idle"] {
        assert!(sessions.iter().any(|s| s["eff"] == eff), "no session is {eff}");
    }
    for place in ["desktop", "terminal", "exec"] {
        assert!(
            sessions.iter().any(|s| s["where"] == place),
            "no session runs in {place}"
        );
    }
}

// ------------------------------------------------------------------ tests/AiPet.Tests/BoardTests.cs
fn now() -> f64 {
    unix_time(SystemTime::now())
}

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

/// BoardMergeTests: a Codex chat as both the hook and Codex's session files (the watcher) report it. The Board orders
/// the two by turn before time, because the hook's `at` is PowerShell's late start on Windows and the log's is Codex's
/// own.
mod merge {
    use super::*;

    struct Chat {
        home: PathBuf,
        hooks: AgentSessions,
        sid: String,
        /// now minus a little, so a done bubble isn't old enough yet to count as idle
        t: f64,
        turn_x: String,
        turn_y: String,
        rollout: PathBuf,
    }

    impl Drop for Chat {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.home);
        }
    }

    impl Chat {
        fn new(test: &str) -> Chat {
            let home = std::env::temp_dir().join(format!("aipet-board-{}-{test}", std::process::id()));
            let _ = fs::remove_dir_all(&home);
            let day = home.join("sessions/2026/09/30");
            retry(|| fs::create_dir_all(&day));
            let n = test.len() as u64;
            let sid = format!("00000000-0000-4000-8000-{n:012}");
            let t = now() - 8.0;
            Chat {
                rollout: day.join(format!("rollout-2026-09-30T12-00-00-{sid}.jsonl")),
                hooks: AgentSessions::with_codex_home(&home),
                turn_x: v7(now() - 20.0, 1),
                turn_y: v7(now() - 10.0, 2),
                home,
                sid,
                t,
            }
        }

        /// An event as the tests' Events.Envelope makes it for Codex, the chat's rollout file its transcript.
        fn hook(&self, ev: &str, turn: &str, at: f64, extra: Value) -> &'static str {
            let mut payload = json!({"hook_event_name": ev, "session_id": self.sid, "turn_id": turn});
            let payload_map = payload.as_object_mut().unwrap();
            payload_map.extend(extra.as_object().unwrap().clone());
            payload_map.insert("transcript_path".into(), json!(self.rollout.to_string_lossy()));
            let line = json!({
                "v": 1, "type": "event", "agent": "codex", "at": at, "sent": at + 0.01, "pid": 4242, "env": {},
                "payload": payload,
            })
            .to_string();
            self.hooks.apply(&Envelope::parse(&line).unwrap(), now()).outcome
        }

        /// Writes the chat's rollout file and has a watcher read it once.
        fn log(&self, events: &[(f64, &str, Option<&str>)]) -> CodexWatcher {
            let mut lines = vec![
                json!({"type": "session_meta", "payload": {"id": self.sid, "cwd": "/work/app", "originator": "codex-tui"}})
                    .to_string(),
            ];
            for (ts, kind, turn) in events {
                let mut payload = json!({ "type": kind });
                if let Some(turn) = turn {
                    payload["turn_id"] = json!(turn);
                }
                let when = iso((ts * 1000.0).round() as i64);
                lines.push(json!({"timestamp": when, "type": "event_msg", "payload": payload}).to_string());
            }
            let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
            retry(|| fs::write(&self.rollout, &text));
            let watcher = CodexWatcher::with_codex_home(&self.home);
            watcher.start();
            let until = Instant::now() + Duration::from_secs(15);
            let id = format!("codex:{}", self.sid);
            while !watcher.sessions().iter().any(|s| s.id == id) {
                assert!(Instant::now() < until, "the watcher didn't read the rollout file");
                thread::sleep(Duration::from_millis(50));
            }
            watcher.stop();
            watcher
        }

        fn merged(&self, watcher: &CodexWatcher) -> Session {
            let mut board = Board::new();
            let sources = Sources {
                chats: self.hooks.snapshot(),
                codex: watcher.sessions(),
                ..Sources::default()
            };
            board.refresh(&sources, now());
            board
                .find(&format!("codex:{}", self.sid))
                .cloned()
                .expect("the chat has a bubble")
        }
    }

    fn bash(command: &str, tool_use_id: &str) -> Value {
        json!({"tool_name": "Bash", "tool_input": {"command": command}, "tool_use_id": tool_use_id})
    }

    #[test]
    fn logs_interrupt_beats_a_later_started_hook_of_that_turn() {
        let c = Chat::new("interrupt");
        let (t, x) = (c.t, c.turn_x.as_str());
        c.hook("UserPromptSubmit", x, t - 3.0, json!({}));
        // Codex sent it just before the user pressed Esc; PowerShell started its hook after
        assert_eq!(
            c.hook("PreToolUse", x, t + 2.0, bash("rm -rf build", "call_1")),
            "working"
        );
        let s = c.merged(&c.log(&[(t - 3.5, "task_started", Some(x)), (t, "turn_aborted", Some(x))]));
        assert_eq!(
            (s.source, s.eff, s.detail.as_str()),
            (Some("log"), "idle", "Interrupted")
        );
        // it keeps its own time, so it doesn't look freshly changed
        assert!((s.ts - t).abs() < 0.001, "{} against {t}", s.ts);
    }

    #[test]
    fn logs_error_end_beats_the_hooks_late_prompt() {
        let c = Chat::new("error-end");
        let (t, x) = (c.t, c.turn_x.as_str());
        // the turn ended in an error (no Stop hook) before the prompt's hook had even started
        assert_eq!(
            c.hook("UserPromptSubmit", x, t + 2.0, json!({"prompt": "go"})),
            "thinking"
        );
        let s = c.merged(&c.log(&[(t, "task_started", Some(x)), (t + 0.5, "task_complete", Some(x))]));
        assert_eq!((s.source, s.eff, s.detail.as_str()), (Some("log"), "done", "Done"));
        // the hook's first prompt still names it
        assert_eq!(s.name, "Go");
    }

    #[test]
    fn hook_of_a_later_turn_beats_the_logs_older_end() {
        let c = Chat::new("later-turn");
        let (t, x, y) = (c.t, c.turn_x.as_str(), c.turn_y.as_str());
        c.hook("UserPromptSubmit", x, t - 5.0, json!({}));
        assert_eq!(c.hook("UserPromptSubmit", y, t, json!({})), "thinking");
        // written after the hook's at, but of the turn before
        let s = c.merged(&c.log(&[(t - 5.5, "task_started", Some(x)), (t + 1.0, "turn_aborted", Some(x))]));
        assert_eq!((s.source, s.eff), (Some("hook"), "thinking"));
        // the hook's own time: the chat showed "Thinking" already, so turn 2's prompt didn't move it
        assert!((s.ts - (t - 5.0)).abs() < 0.001, "{} against {}", s.ts, t - 5.0);
    }

    #[test]
    fn both_have_the_turns_end_the_newer_report_wins() {
        let c = Chat::new("both-ended");
        let (t, x) = (c.t, c.turn_x.as_str());
        c.hook("UserPromptSubmit", x, t - 2.0, json!({}));
        assert_eq!(c.hook("PostCompact", x, t + 2.0, json!({"trigger": "manual"})), "done");
        let s = c.merged(&c.log(&[(t - 2.5, "task_started", Some(x)), (t + 1.0, "task_complete", Some(x))]));
        assert_eq!(
            (s.source, s.eff, s.detail.as_str()),
            (Some("hook"), "done", "Compacted")
        );
    }

    #[test]
    fn without_turn_ids_the_newer_report_wins() {
        let c = Chat::new("no-turns");
        let (t, x) = (c.t, c.turn_x.as_str());
        // an older Codex whose log names no turns
        c.hook("UserPromptSubmit", x, t - 3.0, json!({}));
        c.hook("PreToolUse", x, t + 2.0, bash("ls", "call_1"));
        let s = c.merged(&c.log(&[(t - 3.5, "task_started", None), (t, "turn_aborted", None)]));
        assert_eq!((s.source, s.eff), (Some("hook"), "working"));
    }

    #[test]
    fn log_of_the_same_turn_still_going_goes_by_time() {
        let c = Chat::new("same-turn");
        let (t, x) = (c.t, c.turn_x.as_str());
        c.hook("UserPromptSubmit", x, t - 3.0, json!({}));
        c.hook("PreToolUse", x, t + 2.0, bash("ls", "call_1"));
        let s = c.merged(&c.log(&[(t - 3.5, "task_started", Some(x))]));
        assert_eq!((s.source, s.eff), (Some("hook"), "working"));
    }
}

/// BoardReviewTests: the reviews stack, Jira issues, GitHub review requests and the watchers' errors.
mod reviews {
    use super::*;

    fn issues(now: f64, n: u32, status: &str) -> Vec<Issue> {
        (1..=n)
            .map(|i| Issue {
                key: format!("ABC-{i}"),
                summary: format!("Task {i}"),
                status: status.into(),
                updated: now - f64::from(i),
                url: format!("https://example.atlassian.net/browse/ABC-{i}"),
            })
            .collect()
    }

    fn refresh(sources: &Sources) -> Board {
        let mut board = Board::new();
        board.refresh(sources, now());
        board
    }

    fn reviews(board: &Board) -> Vec<&Session> {
        board.cards().iter().filter(|c| c.section() == "reviews").collect()
    }

    /// The reviews header's count (MainWindow.LayoutCards).
    fn header(board: &Board) -> usize {
        board
            .cards()
            .iter()
            .filter(|x| matches!(x.kind, "jira" | "github"))
            .count()
            + board.extra("reviews")
    }

    fn github_error(error: &str) -> (GitHubSettings, Option<String>) {
        (
            GitHubSettings {
                enabled: true,
                ..GitHubSettings::default()
            },
            Some(error.into()),
        )
    }

    #[test]
    fn watchers_error_is_shown_first_and_not_counted_as_a_review() {
        let (github, github_error) = github_error("GitHub didn't accept the token");
        let board = refresh(&Sources {
            issues: issues(now(), 4, "In Progress"),
            github,
            github_error,
            ..Sources::default()
        });
        let reviews = reviews(&board);
        assert_eq!(reviews.len(), 4);
        assert_eq!(reviews[0].id, "gh:_error");
        assert_eq!(board.extra("reviews"), 1);
        assert_eq!(header(&board), 4);
    }

    #[test]
    fn both_watchers_errors_are_shown_and_only_reviews_overflow() {
        let (github, github_error) = github_error("GitHub didn't accept the token");
        let board = refresh(&Sources {
            issues: issues(now(), 5, "In Progress"),
            jira: JiraSettings {
                enabled: true,
                ..JiraSettings::default()
            },
            jira_error: Some("Jira didn't accept the email/API token".into()),
            github,
            github_error,
            ..Sources::default()
        });
        let reviews = reviews(&board);
        assert_eq!(reviews.len(), 4);
        let mut errors: Vec<&str> = reviews.iter().take(2).map(|c| c.kind).collect();
        errors.sort();
        assert_eq!(errors, ["github-error", "jira-error"]);
        assert_eq!(board.extra("reviews"), 3);
        assert_eq!(header(&board), 5);
    }

    #[test]
    fn few_reviews_and_an_error_all_show() {
        let (github, github_error) = github_error("GitHub didn't accept the token");
        let board = refresh(&Sources {
            issues: issues(now(), 2, "In Progress"),
            github,
            github_error,
            ..Sources::default()
        });
        assert_eq!(reviews(&board).len(), 3);
        assert_eq!(board.extra("reviews"), 0);
        assert_eq!(header(&board), 2);
    }

    #[test]
    fn jira_issue_says_its_status_not_that_a_review_was_requested() {
        let board = refresh(&Sources {
            issues: issues(now(), 2, "In Progress"),
            pr_for_key: HashMap::from([("ABC-1".into(), "https://github.com/owner/app/pull/12".into())]),
            ..Sources::default()
        });
        assert_eq!(board.find("jira:ABC-1").unwrap().detail, "In Progress · PR app#12");
        assert_eq!(board.find("jira:ABC-2").unwrap().detail, "In Progress");
    }
}

/// `Sources::read` takes what the live sources hold: here fresh watchers, which have polled nothing, and a chat in the
/// hooks' store and in Codex's logs.
#[test]
fn sources_are_read_from_the_live_watchers() {
    let dir = std::env::temp_dir().join(format!("aipet-board-sources-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    retry(|| fs::create_dir_all(dir.join("codex/sessions/2026/09/30")));
    let sid = "00000000-0000-4000-8000-000000000042";
    let rollout = dir.join(format!(
        "codex/sessions/2026/09/30/rollout-2026-09-30T12-00-00-{sid}.jsonl"
    ));
    let started = iso((now() * 1000.0) as i64 - 5000);
    let text = format!(
        "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{sid}\",\"cwd\":\"/work/app\"}}}}\n\
         {{\"timestamp\":\"{started}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\"}}}}\n"
    );
    retry(|| fs::write(&rollout, &text));
    let secrets = Arc::new(MemorySecrets::default());
    let (jira_events, _) = channel::<jira::Event>();
    let (github_events, _) = channel::<github::Event>();
    let jira = JiraWatcher::new(&dir, secrets.clone(), Http::local(Duration::from_secs(1)), jira_events);
    let github = GitHubWatcher::new(&dir, secrets, Http::local(Duration::from_secs(1)), github_events);
    let codex = CodexWatcher::with_codex_home(dir.join("codex"));
    codex.poll();
    let hooks = AgentSessions::with_codex_home(dir.join("codex"));
    let line = json!({
        "v": 1, "type": "event", "agent": "claude", "at": now(), "pid": 1, "env": {},
        "payload": {"hook_event_name": "UserPromptSubmit", "session_id": "s", "prompt": "hello"},
    })
    .to_string();
    hooks.apply(&Envelope::parse(&line).unwrap(), now());
    let media = Media {
        song: Some("Song".into()),
        playing: true,
        ..Media::default()
    };
    let sources = Sources::read(&hooks, &jira, &github, Some(&codex), Some(media.clone()), true);
    assert_eq!(
        sources.chats.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["claude:s"]
    );
    assert_eq!(
        sources.codex.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
        [format!("codex:{sid}")]
    );
    assert_eq!(
        (sources.jira.clone(), sources.github.clone()),
        (JiraSettings::default(), GitHubSettings::default())
    );
    assert!(sources.issues.is_empty() && sources.review_requests.is_empty() && sources.pr_for_key.is_empty());
    assert_eq!((sources.jira_error.clone(), sources.github_error.clone()), (None, None));
    assert_eq!((sources.media.clone(), sources.music_on), (Some(media), true));
    let mut board = Board::new();
    board.refresh(&sources, now());
    let ids: Vec<&str> = board.all().iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["claude:s", &format!("codex:{sid}"), "music"]);
    let _ = fs::remove_dir_all(&dir);
}
