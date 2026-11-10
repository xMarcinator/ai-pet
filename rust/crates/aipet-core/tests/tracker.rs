//! The tracker watcher against a fake tracker on this computer (fnt-81): enrolling with a one-time code, hello, the
//! change feed's snapshot and changes, resuming by `(feed, seq)` after a dropped connection, the chats' run lookups,
//! and revocation, both by the feed's close and by a refused token. The fake speaks the routes and JSON of the
//! tracker's `docs/integrations/aipet.md` and nothing else; the credential is kept in memory and the settings in a
//! folder of the test's own.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use aipet_core::secrets::{MemorySecrets, SecretStore, TRACKER};
use aipet_core::tracker::watcher::{CREDENTIAL_REVOKED, REVOKED_TEXT};
use aipet_core::tracker::{Action, Event, Status, Tone, TrackerWatcher};
use serde_json::{Value, json};
use tungstenite::Message;
use tungstenite::protocol::CloseFrame;
use tungstenite::protocol::frame::coding::CloseCode;

const CODE: &str = "good-code";
const TOKEN: &str = "token-1";
const PATIENCE: Duration = Duration::from_secs(15);

/// What the fake sends down the open feed.
enum Out {
    Text(String),
    Close(CloseCode, &'static str),
}

#[derive(Default)]
struct Fake {
    revoked: bool,
    actions: bool,
    /// The first message of each new feed (a resumed one gets none).
    snapshot: Value,
    /// Session id → the task its run works on.
    runs: HashMap<String, String>,
    /// Each feed's request target, in order.
    feeds: Vec<String>,
    /// The session ids looked up.
    lookups: Vec<String>,
    /// The actions posted: their target and body.
    posts: Vec<(String, String)>,
    /// The open feed's sender.
    feed: Option<Sender<Out>>,
}

struct Tracker {
    base: String,
    fake: Arc<Mutex<Fake>>,
}

impl Tracker {
    fn start(snapshot: Value) -> Tracker {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let fake = Arc::new(Mutex::new(Fake {
            snapshot,
            ..Fake::default()
        }));
        let shared = Arc::clone(&fake);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let fake = Arc::clone(&shared);
                thread::spawn(move || serve(stream, &fake));
            }
        });
        Tracker { base, fake }
    }

    fn fake(&self) -> std::sync::MutexGuard<'_, Fake> {
        self.fake.lock().unwrap()
    }

    fn send(&self, out: Out) {
        let feed = self.fake().feed.clone().expect("a feed is open");
        feed.send(out).unwrap();
    }
}

/// The request's head, read without taking it from the stream, so a feed's can still be handed to tungstenite.
fn peek_head(stream: &TcpStream) -> String {
    let mut buf = vec![0; 8192];
    let deadline = Instant::now() + PATIENCE;
    loop {
        let n = stream.peek(&mut buf).unwrap_or(0);
        let text = String::from_utf8_lossy(&buf[..n]).into_owned();
        if let Some(end) = text.find("\r\n\r\n") {
            return text[..end].to_owned();
        }
        assert!(Instant::now() < deadline, "no request head");
        thread::sleep(Duration::from_millis(5));
    }
}

fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).find_map(|line| {
        let (k, v) = line.split_once(':')?;
        k.trim().eq_ignore_ascii_case(name).then_some(v.trim())
    })
}

fn serve(mut stream: TcpStream, fake: &Mutex<Fake>) {
    let head = peek_head(&stream);
    let mut words = head.split_whitespace();
    let (method, target) = (
        words.next().unwrap_or("").to_owned(),
        words.next().unwrap_or("").to_owned(),
    );
    let authorised =
        header(&head, "authorization") == Some(&format!("Bearer {TOKEN}")) && !fake.lock().unwrap().revoked;
    if target.starts_with("/api/v1/companion/feed") {
        if !authorised {
            let _ = stream.write_all(b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n");
            return;
        }
        let Ok(mut socket) = tungstenite::accept(stream) else {
            return;
        };
        let (tx, rx): (Sender<Out>, Receiver<Out>) = mpsc::channel();
        let first = {
            let mut fake = fake.lock().unwrap();
            fake.feeds.push(target.clone());
            fake.feed = Some(tx);
            (!target.contains("since=")).then(|| fake.snapshot.to_string())
        };
        if let Some(first) = first {
            socket.send(Message::text(first)).unwrap();
        }
        for out in rx {
            match out {
                Out::Text(text) => {
                    if socket.send(Message::text(text)).is_err() {
                        return;
                    }
                }
                Out::Close(code, reason) => {
                    let _ = socket.close(Some(CloseFrame {
                        code,
                        reason: reason.into(),
                    }));
                    let _ = socket.flush();
                    return;
                }
            }
        }
        return;
    }
    // a plain request: take the head and the body
    let length: usize = header(&head, "content-length")
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    let mut taken = vec![0; head.len() + 4 + length];
    stream.read_exact(&mut taken).unwrap();
    let body = String::from_utf8_lossy(&taken[head.len() + 4..]).into_owned();
    let (status, reply) = match (method.as_str(), target.as_str()) {
        ("POST", "/enroll") => {
            let request: Value = serde_json::from_str(&body).unwrap_or_default();
            let json = header(&head, "content-type") == Some("application/json");
            if json && request["code"] == CODE && request["role"] == "companion" {
                (200, json!({"id": "pet-1", "token": TOKEN, "certificate": null}))
            } else {
                (403, json!({"error": "code already used"}))
            }
        }
        _ if !authorised => (401, json!({"error": "Device credential no longer valid"})),
        ("GET", "/api/attach/hello") => {
            let actions = fake.lock().unwrap().actions;
            (200, json!({"versions": [], "companion": {"actions_allowed": actions}}))
        }
        ("GET", path) if path.starts_with("/api/v1/runs/by-session/") => {
            let sid = path.trim_start_matches("/api/v1/runs/by-session/").to_owned();
            let mut fake = fake.lock().unwrap();
            fake.lookups.push(sid.clone());
            match fake.runs.get(&sid) {
                Some(task) => (200, json!({"run_id": "run-1", "task_id": task, "status": "running"})),
                None => (404, json!({"error": "no run has that session"})),
            }
        }
        ("POST", path) if path.starts_with("/api/runs/") => {
            let mut fake = fake.lock().unwrap();
            if !fake.actions {
                (403, json!({"error": "companion actions are off for this device"}))
            } else if path.contains("/permissions/") && fake.posts.iter().any(|(p, _)| p == path) {
                (409, json!({"error": "already answered by desk-client"}))
            } else {
                fake.posts.push((path.to_owned(), body));
                (204, Value::Null)
            }
        }
        _ => (404, json!({"error": "not found"})),
    };
    let reply = if status == 204 {
        String::new()
    } else {
        reply.to_string()
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
        reply.len()
    );
}

fn task(id: &str, short: &str, tone: &str, at: &str) -> Value {
    json!({
        "task_id": id, "short_id": short, "title": format!("{short} title"), "repository": "demo",
        "status": "in_progress", "phase": "implementing",
        "subtasks": {"done": 1, "total": 3}, "acceptance": {"done": 0, "total": 2},
        "agents": {"runs": 1, "sessions": 0, "subagents": 0, "total": 1},
        "needs": [], "tone": tone,
        "last_change": {"at": at, "kind": "task.progress"},
        "links": {"task": format!("fnt://task/{short}")}
    })
}

fn change(seq: u64, kind: &str, task_id: &str, task: Option<Value>) -> String {
    let mut change = json!({"type": "change", "feed": "f1", "seq": seq, "kind": kind, "task_id": task_id, "at": ""});
    if let Some(task) = task {
        change["task"] = task;
    }
    change.to_string()
}

struct Pet {
    watcher: TrackerWatcher,
    secrets: Arc<MemorySecrets>,
    _events: Receiver<Event>,
    dir: PathBuf,
}

impl Pet {
    fn new(name: &str) -> Pet {
        let dir = std::env::temp_dir().join(format!("aipet-tracker-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let secrets = Arc::new(MemorySecrets::default());
        let (tx, rx) = mpsc::channel();
        let watcher = TrackerWatcher::new(&dir, Arc::clone(&secrets) as Arc<dyn SecretStore>, tx);
        Pet {
            watcher,
            secrets,
            _events: rx,
            dir,
        }
    }
}

impl Drop for Pet {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Waits until `holds`, failing the test with `what` after [`PATIENCE`].
fn until(what: &str, holds: impl Fn() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !holds() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_pet_enrolls_follows_the_feed_resumes_and_is_revoked() {
    let tracker = Tracker::start(json!({
        "type": "snapshot", "feed": "f1", "seq": 1,
        "tasks": [task("fnt-1-a", "FNT-1", "normal", "2026-11-10T10:00:00Z")]
    }));
    tracker.fake().runs.insert("s1".into(), "fnt-1-a".into());
    let pet = Pet::new("follow");
    pet.watcher.set_chats(|| vec!["claude:s1".into(), "codex:s2".into()]);

    assert_eq!(
        pet.watcher.enroll(&tracker.base, "used"),
        Err("code already used".into())
    );
    assert!(!pet.watcher.is_enrolled());
    assert_eq!(
        pet.watcher.enroll("http://192.168.1.4:3210", CODE).unwrap_err(),
        "Only a tracker on this computer can be followed for now (http://127.0.0.1:<port>)"
    );
    pet.watcher.enroll(&tracker.base, CODE).unwrap();
    assert!(pet.watcher.is_enrolled());
    let saved: Value = serde_json::from_str(&pet.secrets.read(TRACKER).unwrap()).unwrap();
    assert_eq!(
        saved,
        json!({"base_url": tracker.base, "device_id": "pet-1", "token": TOKEN})
    );
    let file = std::fs::read_to_string(pet.dir.join("tracker.json")).unwrap();
    assert!(!file.contains(TOKEN), "the token stays out of tracker.json");

    // the snapshot, and the chat that is the task's run
    until("the snapshot and the run lookup", || {
        let tasks = pet.watcher.tasks();
        tasks.len() == 1 && tasks[0].sessions == ["s1"]
    });
    assert_eq!(pet.watcher.status(), Status::Connected { actions_allowed: false });
    assert_eq!(
        tracker.fake().lookups.iter().filter(|s| *s == "s2").count(),
        1,
        "a miss is asked once"
    );

    // changes: a new task, red, sorted first; then a change already seen, which is skipped
    tracker.send(Out::Text(change(
        2,
        "task.progress",
        "fnt-2-b",
        Some(task("fnt-2-b", "FNT-2", "red", "2026-11-10T09:00:00Z")),
    )));
    until("the red task", || pet.watcher.tasks().len() == 2);
    let tasks = pet.watcher.tasks();
    assert_eq!((tasks[0].short_id.as_str(), tasks[0].tone), ("FNT-2", Tone::Red));
    tracker.send(Out::Text(change(2, "task.removed", "fnt-2-b", None)));
    tracker.send(Out::Text(change(3, "run.started", "fnt-1-a", None)));

    // the connection drops: the pet reconnects and resumes at the last seq it saw, keeping the tasks meanwhile
    tracker.send(Out::Close(CloseCode::Away, "restarting"));
    until("the resumed feed", || tracker.fake().feeds.len() == 2);
    assert_eq!(tracker.fake().feeds[1], "/api/v1/companion/feed?feed=f1&since=3");
    assert_eq!(pet.watcher.tasks().len(), 2);
    tracker.send(Out::Text(change(4, "task.removed", "fnt-2-b", None)));
    until("the removal", || pet.watcher.tasks().len() == 1);

    // revoked: the feed closes with the tracker's reason, the tasks go and the watcher stops
    tracker.fake().revoked = true;
    tracker.send(Out::Close(CloseCode::Policy, CREDENTIAL_REVOKED));
    until("the revocation", || pet.watcher.status() == Status::Revoked);
    assert!(pet.watcher.tasks().is_empty());
    assert_eq!(pet.watcher.last_error().as_deref(), Some(REVOKED_TEXT));
    thread::sleep(Duration::from_millis(1500));
    assert_eq!(tracker.fake().feeds.len(), 2, "no reconnecting after a revocation");
}

#[test]
fn a_refused_token_is_a_revocation_and_forget_turns_it_off() {
    let tracker = Tracker::start(json!({"type": "snapshot", "feed": "f1", "seq": 1, "tasks": []}));
    tracker.fake().actions = true;
    let pet = Pet::new("refused");
    pet.watcher.enroll(&tracker.base, CODE).unwrap();
    until("connected", || {
        pet.watcher.status() == Status::Connected { actions_allowed: true }
    });
    tracker.fake().revoked = true;
    // restarting reads hello again, which the tracker now refuses
    pet.watcher.restart();
    until("the refusal", || pet.watcher.status() == Status::Revoked);
    pet.watcher.forget();
    assert_eq!(pet.watcher.status(), Status::Off);
    assert!(pet.secrets.read(TRACKER).is_none());
    assert!(!pet.watcher.is_enrolled());
}

#[test]
fn an_unreachable_tracker_is_tried_again() {
    // a port nothing listens on
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let base = format!("http://127.0.0.1:{port}");
    let pet = Pet::new("unreachable");
    let error = pet.watcher.enroll(&base, CODE).unwrap_err();
    assert!(
        error.starts_with(&format!("can't reach the tracker at {base}")),
        "{error}"
    );
    // a credential saved earlier, for a tracker that has gone away
    let mut settings = pet.watcher.settings();
    settings.base_url = Some(base.clone());
    pet.watcher.save(settings).unwrap();
    pet.secrets
        .write(
            TRACKER,
            "pet-1",
            &json!({"base_url": base, "device_id": "pet-1", "token": TOKEN}).to_string(),
        )
        .unwrap();
    pet.watcher.restart();
    until("the failure", || matches!(pet.watcher.status(), Status::Unreachable(_)));
    let error = pet.watcher.last_error().unwrap();
    assert!(
        error.starts_with(&format!("can't reach the tracker at {base}")),
        "{error}"
    );
}

#[test]
fn actions_post_the_trackers_routes_only_when_allowed() {
    let tracker = Tracker::start(json!({"type": "snapshot", "feed": "f1", "seq": 1, "tasks": []}));
    let pet = Pet::new("actions");
    pet.watcher.enroll(&tracker.base, CODE).unwrap();
    until("connected", || {
        pet.watcher.status() == Status::Connected { actions_allowed: false }
    });
    let allow = Action::Allow {
        run: "run-7".into(),
        request: "req-3".into(),
    };
    assert_eq!(
        pet.watcher.act(&allow),
        Err("Answers and pause/stop are off for this pet".into())
    );
    assert!(tracker.fake().posts.is_empty(), "nothing is sent while they're off");

    // the user switches them on in the tracker; hello says so on the next connection
    tracker.fake().actions = true;
    pet.watcher.restart();
    until("allowed", || {
        pet.watcher.status() == Status::Connected { actions_allowed: true }
    });
    pet.watcher.act(&allow).unwrap();
    assert_eq!(pet.watcher.act(&allow), Err("already answered by desk-client".into()));
    pet.watcher.act(&Action::Pause { run: "run-7".into() }).unwrap();
    pet.watcher.act(&Action::Stop { run: "run-7".into() }).unwrap();
    let deny = Action::Deny {
        run: "run-7".into(),
        request: "req-4".into(),
    };
    pet.watcher.act(&deny).unwrap();
    assert_eq!(
        tracker.fake().posts,
        [
            (
                "/api/runs/run-7/permissions/req-3".into(),
                r#"{"decision":"allow"}"#.into()
            ),
            ("/api/runs/run-7/pause".into(), "{}".into()),
            ("/api/runs/run-7/cancel".into(), "{}".into()),
            (
                "/api/runs/run-7/permissions/req-4".into(),
                r#"{"decision":"deny"}"#.into()
            ),
        ]
    );
    // the switch turned off again in the tracker: its refusal is what the bubble says
    tracker.fake().actions = false;
    assert_eq!(
        pet.watcher.act(&Action::Stop { run: "run-7".into() }),
        Err("companion actions are off for this device".into())
    );
}
