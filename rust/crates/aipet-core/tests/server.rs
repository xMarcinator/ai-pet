//! The hook server (`aipet_core::server`) against the C#'s own tests of HookServer, case for case:
//! tests/AiPet.Tests/HookServerTests.cs (EndToEndTests and HookServerTests), HookServerUnixTests.cs and
//! StarvedPoolTests.cs. NoPetTests, the hook with no pet at all, are aipet-hook's (tests/event.rs).
//!
//! The hooks are real: the Rust hook built with these tests (`cargo test --workspace` builds it first), or the one
//! `AIPET_TEST_HOOK` names, run through `dotnet` when it's a .dll: the C# hook, as cross-runtime.yml runs it against
//! this server. Every server has an endpoint and folders of its own: nothing here reaches the user's pet, data or
//! `~/.codex`.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use aipet_core::server::{HookServer, Options};
use aipet_core::sessions::{AgentSessions, Entry};
use aipet_ipc::connect::connect_to;
use aipet_ipc::protocol::{CONNECT, MAX_DEPTH, MAX_REQUEST, TIMEOUT, unix_time};
use serde_json::{Value, json};

/// How long a hook may take to run, starting included (a fresh exe can be scanned first).
const HOOK_LIMIT: Duration = Duration::from_secs(20);

// ------------------------------------------------------------------ where the tests run

/// The tests that count the process's threads or keep every CPU busy run alone; the others run side by side.
static EXCLUSIVE: RwLock<()> = RwLock::new(());

fn side_by_side() -> RwLockReadGuard<'static, ()> {
    EXCLUSIVE.read().unwrap_or_else(PoisonError::into_inner)
}

fn alone() -> RwLockWriteGuard<'static, ()> {
    EXCLUSIVE.write().unwrap_or_else(PoisonError::into_inner)
}

/// A test's own endpoint and folders, removed with it.
struct Env {
    root: PathBuf,
    endpoint: OsString,
}

impl Env {
    fn new(test: &str) -> Env {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let n = COUNT.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let root = std::env::temp_dir().join(format!("aipet-core-server-{pid}-{test}"));
        let _ = fs::remove_dir_all(&root);
        for sub in ["data", "tmp", "codex"] {
            fs::create_dir_all(root.join(sub)).unwrap();
        }
        // a socket's path must stay under 104 bytes, so it isn't in the temp folder (TMPDIR can be long)
        let endpoint = if cfg!(windows) {
            format!("AiPet-test-{pid}-{n}-{test}")
        } else {
            format!("/tmp/aipet-t-{pid:x}-{n}.sock")
        };
        Env {
            root,
            endpoint: endpoint.into(),
        }
    }

    fn data(&self) -> PathBuf {
        self.root.join("data")
    }

    fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    fn codex(&self) -> PathBuf {
        self.root.join("codex")
    }

    /// A fresh empty folder in the test's own.
    fn dir(&self, name: &str) -> PathBuf {
        let dir = self.root.join(name);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A chat store that reads Codex's chat names from the test's Codex folder.
    fn sessions(&self) -> Arc<AgentSessions> {
        Arc::new(AgentSessions::with_codex_home(self.codex()))
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        if cfg!(unix) {
            let _ = fs::remove_file(&self.endpoint);
        }
    }
}

/// A server at a test's endpoint, with what it logged and how often `changed` ran.
struct Pet {
    server: HookServer,
    sessions: Arc<AgentSessions>,
    endpoint: OsString,
    events_log: PathBuf,
    log: Arc<Mutex<Vec<String>>>,
    changes: Arc<AtomicUsize>,
}

impl Pet {
    /// Made, not started.
    fn new(env: &Env, sessions: &Arc<AgentSessions>) -> Pet {
        let log = Arc::new(Mutex::new(Vec::new()));
        let changes = Arc::new(AtomicUsize::new(0));
        let (logged, changed) = (Arc::clone(&log), Arc::clone(&changes));
        let events_log = env.data().join("hook-events.log");
        let server = HookServer::with_options(
            Arc::clone(sessions),
            move || {
                changed.fetch_add(1, Ordering::SeqCst);
            },
            Options {
                endpoint: env.endpoint.clone(),
                events_log: events_log.clone(),
                log: Box::new(move |line| logged.lock().unwrap().push(line.to_owned())),
            },
        );
        Pet {
            server,
            sessions: Arc::clone(sessions),
            endpoint: env.endpoint.clone(),
            events_log,
            log,
            changes,
        }
    }

    /// Started, and known to serve: an event of its own was answered (`TestEnv.StartServer`).
    fn start(env: &Env, sessions: &Arc<AgentSessions>) -> Pet {
        let pet = Pet::new(env, sessions);
        pet.server.start().unwrap();
        let probe = format!("probe-{}", uuid());
        let reply = ask_json(
            &pet.endpoint,
            &envelope("claude", now(), payload(&probe, "SessionStart", json!({}))),
        );
        assert_eq!(reply.as_ref().and_then(|r| r["ok"].as_bool()), Some(true), "{reply:?}");
        assert!(pet.chat(&format!("claude:{probe}")).is_some());
        pet
    }

    fn chat(&self, key: &str) -> Option<Entry> {
        self.sessions.snapshot().into_iter().find(|e| e.id == key)
    }

    /// What hook-events.log recorded for a session.
    fn outcomes(&self, sid: &str) -> Vec<String> {
        outcomes(&logged(&self.events_log), sid)
    }

    /// An event for a new chat; its outcome, which must be an accepted one (`HookServerTests.Serve`).
    fn serve(&self) -> String {
        let sid = uuid();
        let reply = ask_json(
            &self.endpoint,
            &envelope("claude", now(), payload(&sid, "UserPromptSubmit", json!({}))),
        )
        .expect("no reply");
        assert_eq!(reply["ok"], true, "{reply}");
        assert!(self.chat(&format!("claude:{sid}")).is_some());
        reply["outcome"].as_str().unwrap().to_owned()
    }

    fn logged(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

// ------------------------------------------------------------------ requests

fn now() -> f64 {
    unix_time(SystemTime::now())
}

/// One request; the reply line, or none when the pet closed without one.
fn ask(endpoint: &OsStr, line: &str) -> io::Result<Option<String>> {
    let mut pet = connect_to(endpoint, CONNECT).map_err(|no| io::Error::other(format!("no pet: {no:?}")))?;
    pet.ask(line)
}

fn ask_json(endpoint: &OsStr, line: &str) -> Option<Value> {
    let reply = ask(endpoint, line).unwrap()?;
    Some(serde_json::from_str(&reply).unwrap_or_else(|e| panic!("{reply}: {e}")))
}

/// An event as the hook sends it (`Events.Envelope`).
fn envelope(agent: &str, at: f64, payload: Value) -> String {
    json!({"v": 1, "type": "event", "agent": agent, "at": at, "sent": at + 0.01, "pid": 4242, "env": {}, "payload": payload})
        .to_string()
}

/// A payload: hook_event_name and session_id, and `extra`'s fields (`Events.Payload`).
fn payload(sid: &str, event: &str, extra: Value) -> Value {
    let mut p = json!({"hook_event_name": event, "session_id": sid});
    if let Value::Object(extra) = extra {
        for (k, v) in extra {
            p[k] = v;
        }
    }
    p
}

/// A Bash tool call's fields.
fn bash(command: &str, tool_use_id: &str) -> Value {
    json!({"tool_name": "Bash", "tool_use_id": tool_use_id, "tool_input": {"command": command}})
}

/// 128 bits that differ from call to call and run to run: std's randomly keyed hasher on a counter.
fn random() -> u128 {
    use std::hash::{BuildHasher, RandomState};
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let n = COUNT.fetch_add(1, Ordering::SeqCst);
    let keys = RandomState::new();
    (u128::from(keys.hash_one((n, 1))) << 64) | u128::from(keys.hash_one((n, 2)))
}

fn guid(bits: u128) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        bits >> 96,
        (bits >> 80) & 0xffff,
        (bits >> 64) & 0xffff,
        (bits >> 48) & 0xffff,
        bits & 0xffff_ffff_ffff
    )
}

/// `Guid.NewGuid()`: version 4.
fn uuid() -> String {
    guid((random() & !(0xf << 76) & !(0b11 << 62)) | (0x4 << 76) | (0b10 << 62))
}

/// `Guid.CreateVersion7(at)`: version 7, made at `at` (Unix seconds).
fn uuid7(at: f64) -> String {
    let ms = (at * 1000.0) as u128 & 0xffff_ffff_ffff;
    guid((ms << 80) | (0x7 << 76) | (random() & (0xfff << 64)) | (0b10 << 62) | (random() & ((1 << 62) - 1)))
}

/// hook-events.log's lines.
fn logged(path: &Path) -> Vec<String> {
    fs::read_to_string(path).map_or_else(|_| Vec::new(), |t| t.lines().map(str::to_owned).collect())
}

/// The outcomes lines record for a session, in the order the pet got them. Lines name a session by its first 13
/// characters (`TestEnv.Outcomes`).
fn outcomes(lines: &[String], sid: &str) -> Vec<String> {
    let session = format!(" {} ", &sid[..13]);
    lines
        .iter()
        .filter(|l| l.contains(&session))
        .map(|l| l[l.rfind("-> ").unwrap() + 3..].trim().to_owned())
        .collect()
}

// ------------------------------------------------------------------ the hook

/// The hook the tests run, and how.
struct Hook {
    program: OsString,
    args: Vec<OsString>,
}

fn hook() -> &'static Hook {
    static HOOK: OnceLock<Hook> = OnceLock::new();
    HOOK.get_or_init(|| {
        if let Some(asked) = std::env::var_os("AIPET_TEST_HOOK").filter(|h| !h.is_empty()) {
            let path = std::path::absolute(&asked).unwrap();
            assert!(path.is_file(), "AIPET_TEST_HOOK: there's no hook at {}", path.display());
            return if path.extension() == Some(OsStr::new("dll")) {
                Hook {
                    program: "dotnet".into(),
                    args: vec![path.into()],
                }
            } else {
                Hook {
                    program: path.into(),
                    args: vec![],
                }
            };
        }
        // <target>/<profile>/deps/server-<hash>: the hook is two folders up
        let exe = std::env::current_exe().unwrap();
        let path = exe
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .join(format!("aipet-hook{}", std::env::consts::EXE_SUFFIX));
        assert!(
            path.is_file(),
            "the Rust hook isn't built at {}: cargo build -p aipet-hook (cargo test --workspace does)",
            path.display()
        );
        assert_fresh(&path);
        Hook {
            program: path.into(),
            args: vec![],
        }
    })
}

/// A hook built before its sources last changed would test code that isn't there any more.
fn assert_fresh(hook: &Path) {
    let built = fs::metadata(hook).unwrap().modified().unwrap();
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for dir in ["aipet-hook/src", "aipet-ipc/src"] {
        for source in fs::read_dir(crates.join(dir)).unwrap() {
            let source = source.unwrap().path();
            let changed = fs::metadata(&source).unwrap().modified().unwrap();
            assert!(
                changed <= built,
                "{} changed after {} was built: cargo build -p aipet-hook",
                source.display(),
                hook.display()
            );
        }
    }
}

struct HookRun {
    exit: Option<i32>,
    stdout: String,
    stderr: String,
    started: f64,
    ended: f64,
}

impl HookRun {
    /// Exit 0, and nothing printed.
    #[track_caller]
    fn silent(&self) {
        assert_eq!(
            (self.exit, self.stdout.as_str(), self.stderr.as_str()),
            (Some(0), "", ""),
            "the hook wasn't silent"
        );
    }
}

/// Starts the hook for `agent` at the test's endpoint and folders, its traces in `tmp`, with `stdin` already written
/// and closed and without the agent's own variables (these tests run in agents too), `vars` added
/// (`TestEnv.StartHook`).
fn start_hook(env: &Env, agent: &str, stdin: &str, tmp: &Path, vars: &[(&str, &str)]) -> (Child, f64) {
    let hook = hook();
    let mut command = Command::new(&hook.program);
    command
        .args(&hook.args)
        .args(["--agent", agent])
        .env("AIPET_PIPE", &env.endpoint)
        .env("AIPET_DATA_DIR", env.data())
        .env("TMPDIR", tmp)
        .env("TMP", tmp)
        .env("TEMP", tmp)
        .env("CODEX_HOME", env.codex())
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .env_remove("CLAUDE_CODE_HOST_SESSION_ID")
        .envs(vars.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let started = now();
    let mut child = command
        .spawn()
        .unwrap_or_else(|e| panic!("can't start {:?}: {e}", hook.program));
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    (child, started)
}

/// Waits for a hook to exit, without anything that would need the test's other threads.
fn finish((mut child, started): (Child, f64), limit: Duration) -> HookRun {
    let until = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the hook didn't exit within {limit:?}");
        }
        thread::sleep(Duration::from_millis(5));
    };
    let ended = now();
    // the hook prints nothing, so reading after its exit can't have filled a pipe and held it up
    let (mut stdout, mut stderr) = (String::new(), String::new());
    child.stdout.take().unwrap().read_to_string(&mut stdout).unwrap();
    child.stderr.take().unwrap().read_to_string(&mut stderr).unwrap();
    HookRun {
        exit: status.code(),
        stdout,
        stderr,
        started,
        ended,
    }
}

fn run_hook(env: &Env, agent: &str, payload: &Value, tmp: &Path, vars: &[(&str, &str)]) -> HookRun {
    finish(start_hook(env, agent, &payload.to_string(), tmp, vars), HOOK_LIMIT)
}

/// Every file and folder in `dir`.
fn contents(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect()
}

/// The hook's trace logs in `dir`: aipet-hook.log on Windows, aipet-hook-<euid>.log elsewhere.
fn traces(dir: &Path) -> Vec<PathBuf> {
    contents(dir)
        .into_iter()
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("aipet-hook") && name.ends_with(".log")
        })
        .collect()
}

// ------------------------------------------------------------------ clients that misbehave

#[cfg(windows)]
type Client = fs::File;
#[cfg(unix)]
type Client = std::os::unix::net::UnixStream;

/// A connection that sends nothing: a silent client. Windows waits for a free instance, as `Connect(5000)`.
fn connect_silent(endpoint: &OsStr) -> Client {
    #[cfg(windows)]
    {
        let mut path = OsString::from(r"\\.\pipe\");
        path.push(endpoint);
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            match fs::OpenOptions::new().read(true).write(true).open(&path) {
                Ok(pipe) => return pipe,
                // ERROR_PIPE_BUSY: every instance taken for a moment
                Err(e) if e.raw_os_error() == Some(231) && Instant::now() < until => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("can't connect to {endpoint:?}: {e}"),
            }
        }
    }
    #[cfg(unix)]
    {
        Client::connect(endpoint).unwrap()
    }
}

/// Waits on a thread of its own for the pet to close a connection; sends how long after `since` it did. A read
/// returns 0 once the pet closes; a broken connection counts as closed too.
fn closed(mut client: Client, since: Instant) -> mpsc::Receiver<Duration> {
    let (sent, got) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = [0u8; 256];
        while matches!(client.read(&mut buf), Ok(n) if n > 0) {}
        let _ = sent.send(since.elapsed());
    });
    got
}

/// When the pet closed it, or none if it didn't within 12 s.
fn close_time(closed: &mpsc::Receiver<Duration>) -> Option<Duration> {
    closed.recv_timeout(TIMEOUT + Duration::from_secs(10)).ok()
}

/// Every CPU busy, twice over, until dropped: threads spinning below normal priority. They take every cycle nothing
/// else wants, so the pet's threads, at normal priority, are what is being held to the hooks' 2 s. At normal priority,
/// on a 2-CPU CI runner, they timed the OS's scheduling of 20 new processes against 4 spinners rather than the pet:
/// the C# test this ports starves .NET's thread pool, not the CPUs.
struct Busy {
    stop: Arc<AtomicBool>,
    spinning: Arc<AtomicUsize>,
    threads: Vec<JoinHandle<()>>,
}

impl Busy {
    fn new() -> Busy {
        let n = 2 * thread::available_parallelism().map_or(2, NonZero::get);
        let (stop, spinning) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)));
        let threads = (0..n)
            .map(|_| {
                let (stop, spinning) = (Arc::clone(&stop), Arc::clone(&spinning));
                thread::spawn(move || {
                    below_normal_priority();
                    spinning.fetch_add(1, Ordering::SeqCst);
                    while !stop.load(Ordering::Relaxed) {
                        std::hint::spin_loop();
                    }
                    spinning.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();
        let busy = Busy {
            stop,
            spinning,
            threads,
        };
        let since = Instant::now();
        while !busy.all_spinning() {
            assert!(
                since.elapsed() < Duration::from_secs(20),
                "the busy threads never all ran"
            );
            thread::sleep(Duration::from_millis(10));
        }
        busy
    }

    fn all_spinning(&self) -> bool {
        self.spinning.load(Ordering::SeqCst) == self.threads.len()
    }
}

/// The calling thread below normal priority. On Linux a nice value is per thread; elsewhere on Unix it would be the
/// whole test process's, so there the thread is left as it is.
fn below_normal_priority() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };
        // SAFETY: the calling thread's pseudo handle
        let ok = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) };
        assert_ne!(ok, 0, "{}", std::io::Error::last_os_error());
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: gettid has no preconditions
        let tid = unsafe { libc::syscall(libc::SYS_gettid) } as libc::id_t;
        // SAFETY: this thread's own id
        let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, tid, 10) };
        assert_eq!(rc, 0, "{}", std::io::Error::last_os_error());
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

// ------------------------------------------------------------------ EndToEndTests: real hooks, a real server

/// Codex's hooks finish in any order (PowerShell starts them late at random): run them in a scrambled order and the
/// pet still ends up at the latest turn, going by Codex's ids rather than by when each hook ran.
#[test]
fn codex_scrambled_hooks_end_at_the_latest_turn() {
    let _side = side_by_side();
    let env = Env::new("codex-scrambled");
    let pet = Pet::start(&env, &env.sessions());
    let sid = uuid();
    // the Board shows a Codex chat only with a transcript; its first line names the client
    let transcript = env
        .dir("codex/sessions")
        .join(format!("rollout-2026-09-25T10-00-00-{sid}.jsonl"));
    fs::write(
        &transcript,
        format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{sid}\",\"originator\":\"codex-tui\"}}}}\n"),
    )
    .unwrap();
    let start = now() - 20.0;
    let (turn1, turn2) = (uuid7(start), uuid7(start + 5.0));
    let event = |ev: &str, turn: &str, extra: Value| {
        let mut p = payload(&sid, ev, extra);
        p["turn_id"] = json!(turn);
        p["transcript_path"] = json!(transcript);
        p["cwd"] = json!("/work/project");
        p
    };
    let patch = "*** Begin Patch\n*** Update File: src/Main.cs\n@@\n-a\n+b\n*** End Patch\n";
    let steps = [
        (event("Stop", &turn1, json!({})), "done"),
        (event("PreToolUse", &turn1, bash("ls", "call_1")), "stale"),
        (
            event("UserPromptSubmit", &turn2, json!({"prompt": "second turn\nmore"})),
            "thinking",
        ),
        (event("PostToolUse", &turn1, bash("ls", "call_1")), "stale"),
        (
            event(
                "PreToolUse",
                &turn2,
                json!({"tool_name": "apply_patch", "tool_use_id": "call_2", "tool_input": {"command": patch}}),
            ),
            "working",
        ),
        (event("Stop", &turn1, json!({})), "stale"),
    ];
    for (payload, _) in &steps {
        run_hook(&env, "codex", payload, &env.tmp(), &[]).silent();
    }

    let expected: Vec<&str> = steps.iter().map(|(_, outcome)| *outcome).collect();
    assert_eq!(pet.outcomes(&sid), expected);
    let ping = ask_json(&pet.endpoint, r#"{"v":1,"type":"ping"}"#).unwrap();
    assert_eq!(ping["ok"], true);
    assert_eq!(ping["pid"], std::process::id());
    let recent: Vec<String> = ping["recent"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(outcomes(&recent, &sid), expected);
    // where the chat runs comes from the transcript's first line
    let session = format!(" {} ", &sid[..13]);
    for line in logged(&pet.events_log).iter().filter(|l| l.contains(&session)) {
        assert!(line.contains(" where=terminal "), "{line}");
    }

    let chat = pet.chat(&format!("codex:{sid}")).unwrap();
    assert_eq!(
        (chat.state, chat.detail.as_deref(), chat.title.as_deref()),
        (Some("working"), Some("Editing Main.cs"), Some("Second turn"))
    );
    assert_eq!(
        (chat.r#where.as_deref(), chat.cwd.as_deref(), chat.has_transcript),
        (Some("terminal"), Some("/work/project"), true)
    );
}

/// Claude's `at` is when its hook started, which the real binary can't be made to scramble reliably; envelopes sent
/// over the real socket can. `changed` runs only for the events that changed the chat.
#[test]
fn claude_late_envelopes_over_the_socket_are_stale() {
    let _side = side_by_side();
    let env = Env::new("claude-late");
    let pet = Pet::start(&env, &env.sessions());
    let changes = pet.changes.load(Ordering::SeqCst);
    let sid = uuid();
    let t0 = now();
    let send = |ev: &str, at: f64, extra: Value| {
        let reply = ask_json(&pet.endpoint, &envelope("claude", at, payload(&sid, ev, extra))).unwrap();
        assert_eq!(reply["ok"], true, "{reply}");
        reply["outcome"].as_str().unwrap().to_owned()
    };
    assert_eq!(
        send("UserPromptSubmit", t0, json!({"prompt": "fix the bug"})),
        "thinking"
    );
    assert_eq!(send("Stop", t0 + 2.0, json!({})), "done");
    assert_eq!(send("PreToolUse", t0 + 1.0, bash("ls", "call_1")), "stale");
    assert_eq!(send("PostToolUse", t0 + 2.0, bash("ls", "call_1")), "stale");
    assert_eq!(pet.chat(&format!("claude:{sid}")).unwrap().state, Some("done"));
    assert_eq!(send("SessionEnd", t0 + 3.0, json!({})), "removed");
    assert_eq!(
        send(
            "Notification",
            t0 + 2.5,
            json!({"notification_type": "permission_prompt"})
        ),
        "stale"
    );
    assert_eq!(pet.chat(&format!("claude:{sid}")).unwrap().ended, Some(t0 + 3.0));
    assert_eq!(
        pet.outcomes(&sid),
        ["thinking", "done", "stale", "stale", "removed", "stale"]
    );
    // thinking, done and removed
    assert_eq!(pet.changes.load(Ordering::SeqCst) - changes, 3);
}

/// What the Claude hook passes on: the event, when it started, and the environment that says where the chat runs.
/// It prints nothing, exits 0, and traces nothing when all went well.
#[test]
fn claude_hook_forwards_the_event_and_its_environment_silently() {
    let _side = side_by_side();
    let env = Env::new("claude-hook");
    let pet = Pet::start(&env, &env.sessions());
    let sid = uuid();
    let host = format!("local_{}", uuid());
    let tmp = env.dir("tmp-claude");
    let transcript = tmp.join("missing.jsonl");
    let event = payload(
        &sid,
        "UserPromptSubmit",
        json!({"prompt": "hello there", "cwd": "/work/claude", "transcript_path": transcript}),
    );
    let vars = [
        ("CLAUDE_CODE_ENTRYPOINT", "claude-desktop"),
        ("CLAUDE_CODE_HOST_SESSION_ID", host.as_str()),
    ];
    let run = run_hook(&env, "claude", &event, &tmp, &vars);
    run.silent();
    assert_eq!(contents(&tmp), Vec::<PathBuf>::new());

    let chat = pet.chat(&format!("claude:{sid}")).unwrap();
    assert_eq!(
        (chat.state, chat.title.as_deref(), chat.cwd.as_deref()),
        (Some("thinking"), Some("Hello there"), Some("/work/claude"))
    );
    assert_eq!(
        (chat.r#where.as_deref(), chat.host_id.as_deref()),
        (Some("desktop"), Some(host.as_str()))
    );
    // at is the hook's start: after it was launched, before it exited
    assert!(
        run.started - 0.01 <= chat.ts && chat.ts <= run.ended + 0.01,
        "{} {} {}",
        run.started,
        chat.ts,
        run.ended
    );
    assert_eq!(pet.outcomes(&sid), ["thinking"]);

    // without the variables the chat's place is unknown, not taken from anywhere else
    let sid = uuid();
    run_hook(&env, "claude", &payload(&sid, "SessionStart", json!({})), &tmp, &[]).silent();
    let chat = pet.chat(&format!("claude:{sid}")).unwrap();
    assert_eq!((chat.state, chat.r#where, chat.host_id), (Some("idle"), None, None));
}

/// Stdin that isn't JSON still reaches the pet (as an empty event), and the hook still exits 0 silently. With the pet
/// running it may trace why.
#[test]
fn a_hook_with_garbage_on_stdin_exits_0_silently() {
    let _side = side_by_side();
    let env = Env::new("garbage-stdin");
    let _pet = Pet::start(&env, &env.sessions());
    let tmp = env.dir("tmp-garbage");
    for agent in ["claude", "codex"] {
        finish(start_hook(&env, agent, "{not json", &tmp, &[]), HOOK_LIMIT).silent();
    }
    let traces = traces(&tmp);
    assert_eq!(traces.len(), 1, "{traces:?}");
    let traced = fs::read_to_string(&traces[0]).unwrap();
    assert!(traced.contains("isn't JSON"), "{traced}");
}

// ------------------------------------------------------------------ HookServerTests: clients that misbehave, and Stop

#[test]
fn silent_clients_are_cut_off_while_the_pet_keeps_serving() {
    let _side = side_by_side();
    let env = Env::new("silent");
    let pet = Pet::start(&env, &env.sessions());
    let since = Instant::now();
    // more than the pet's listeners: each connection gets a thread of its own, and its listener makes the next
    // instance at once
    let cut: Vec<_> = (0..12).map(|_| closed(connect_silent(&pet.endpoint), since)).collect();

    assert_eq!(pet.serve(), "thinking");
    let served = since.elapsed();
    // a real hook too (it may take long to start on a slow machine, so it isn't timed)
    let sid = uuid();
    run_hook(
        &env,
        "codex",
        &payload(&sid, "SessionStart", json!({})),
        &env.tmp(),
        &[],
    )
    .silent();
    assert_eq!(pet.outcomes(&sid), ["idle"]);

    let times: Vec<Duration> = cut
        .iter()
        .map(|c| close_time(c).expect("a silent client wasn't cut off"))
        .collect();
    for t in &times {
        assert!(
            *t >= TIMEOUT - Duration::from_millis(50) && *t <= TIMEOUT + Duration::from_secs(5),
            "cut off after {t:?}"
        );
    }
    // the others were answered while the silent ones were still waiting to be cut off
    let first = *times.iter().min().unwrap();
    assert!(
        served < first,
        "served after {served:?}, the first cut-off came at {first:?}"
    );
    assert_eq!(pet.serve(), "thinking");
}

#[test]
fn stop_returns_promptly_with_clients_connected() {
    let _side = side_by_side();
    let env = Env::new("stop");
    let pet = Pet::start(&env, &env.sessions());
    let mut clients: Vec<Client> = (0..4).map(|_| connect_silent(&pet.endpoint)).collect();
    // and one halfway through a request
    let mut half = connect_silent(&pet.endpoint);
    half.write_all(br#"{"v":1,"type":"eve"#).unwrap();
    clients.push(half);
    let since = Instant::now();
    let watched: Vec<_> = clients.into_iter().map(|c| closed(c, since)).collect();

    pet.server.stop();
    assert!(
        since.elapsed() < Duration::from_millis(1500),
        "Stop took {:?}",
        since.elapsed()
    );
    // nothing listens any more
    let gone = Instant::now();
    while connect_to(&pet.endpoint, CONNECT).is_ok() {
        assert!(
            gone.elapsed() < Duration::from_secs(10),
            "the pet still listens after Stop"
        );
        thread::sleep(Duration::from_millis(100));
    }
    // and the clients it was answering are still cut off
    for c in &watched {
        assert!(close_time(c).is_some(), "a client wasn't cut off after Stop");
    }
}

#[test]
fn garbage_is_dropped_without_a_reply() {
    let _side = side_by_side();
    let env = Env::new("garbage");
    let pet = Pet::start(&env, &env.sessions());
    for line in ["not json", "[1,2,3]", "\"a string\"", r#"{"v":1,"type":"event","#, ""] {
        assert_eq!(ask(&pet.endpoint, line).unwrap(), None, "{line:?}");
        assert_eq!(pet.serve(), "thinking");
    }
}

#[test]
fn too_deep_is_dropped_without_a_reply() {
    let _side = side_by_side();
    let env = Env::new("too-deep");
    let pet = Pet::start(&env, &env.sessions());
    let depth = MAX_DEPTH + 10;
    let line = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    assert_eq!(ask(&pet.endpoint, &line).unwrap(), None);
    assert_eq!(pet.serve(), "thinking");
}

#[test]
fn json_it_cant_serve_is_refused() {
    let _side = side_by_side();
    let env = Env::new("refused");
    let pet = Pet::start(&env, &env.sessions());
    let reply = ask_json(&pet.endpoint, r#"{"v":2,"type":"ping"}"#).unwrap();
    assert_eq!(
        (&reply["ok"], &reply["error"]),
        (&json!(false), &json!("unsupported version"))
    );
    let reply = ask_json(&pet.endpoint, r#"{"v":1,"type":"dance"}"#).unwrap();
    assert_eq!(
        (&reply["ok"], &reply["error"]),
        (&json!(false), &json!("unknown request"))
    );
    assert_eq!(pet.serve(), "thinking");
}

#[test]
fn an_oversized_request_is_dropped_without_a_reply() {
    let _side = side_by_side();
    let env = Env::new("oversized");
    let pet = Pet::start(&env, &env.sessions());
    let sid = uuid();
    let line = envelope(
        "claude",
        now(),
        payload(&sid, "UserPromptSubmit", json!({"prompt": "x".repeat(MAX_REQUEST)})),
    );
    assert!(line.len() > MAX_REQUEST);
    let since = Instant::now();
    // the pet may close while it's still being written: an error, not a reply
    let reply = ask(&pet.endpoint, &line).ok().flatten();
    assert_eq!(reply, None);
    // dropped as soon as it was too big, not left to the silent-client cut-off
    assert!(since.elapsed() < TIMEOUT, "dropped after {:?}", since.elapsed());
    assert!(pet.chat(&format!("claude:{sid}")).is_none());
    assert_eq!(pet.serve(), "thinking");
}

// ------------------------------------------------------------------ beyond the C#'s tests: its answers and errors

#[derive(Clone, Copy)]
enum Answer {
    Exactly(&'static str),
    Ping,
    Dropped,
}

/// The C#'s HookServer's answers to lines of every kind, taken from .NET 10 once: the same bytes, or no answer. Left
/// out are the few events serde_json can't read but the C# applies (a number past a double's range, a lone
/// surrogate), and a payload with a member twice, which the C#'s Apply throws on and the port's applies: no hook
/// sends any of them.
#[test]
fn the_answers_are_the_csharps() {
    use Answer::*;
    let _side = side_by_side();
    let env = Env::new("answers");
    let pet = Pet::start(&env, &env.sessions());
    let unsupported = Exactly(r#"{"ok":false,"error":"unsupported version"}"#);
    let unknown = Exactly(r#"{"ok":false,"error":"unknown request"}"#);
    let deep = |depth: usize| {
        format!(
            r#"{{"v":1,"type":"ping","x":{}{}}}"#,
            "[".repeat(depth - 1),
            "]".repeat(depth - 1)
        )
    };
    let cases = [
        (r#"{"v":2,"type":"ping"}"#.to_owned(), unsupported),
        (r#"{"v":1.0,"type":"dance"}"#.into(), unknown),
        (r#"{"v":1e0,"type":"dance"}"#.into(), unknown),
        (r#"{"v":10e-1,"type":"dance"}"#.into(), unknown),
        (r#"{"v":1.0000000000000001,"type":"dance"}"#.into(), unknown),
        (r#"{"v":"1","type":"ping"}"#.into(), unsupported),
        (r#"{"v":true,"type":"ping"}"#.into(), unsupported),
        (r#"{"v":null,"type":"ping"}"#.into(), unsupported),
        (r#"{"type":"ping"}"#.into(), unsupported),
        (r#"{"v":[1],"type":"ping"}"#.into(), unsupported),
        (r#"{"v":1e400,"type":"ping"}"#.into(), unsupported),
        (r#"{"v":-0,"type":"ping"}"#.into(), unsupported),
        (r#"{"v":1}"#.into(), unknown),
        (r#"{"v":1,"type":1}"#.into(), unknown),
        (r#"{"v":1,"type":null}"#.into(), unknown),
        (r#"{"v":1,"type":"Ping"}"#.into(), unknown),
        (r#"{"V":1,"type":"dance"}"#.into(), unsupported),
        (r#"{"v":2,"v":1,"type":"dance"}"#.into(), Dropped),
        (r#"{"v":1,"v":2,"type":"dance"}"#.into(), Dropped),
        (r#"{"v":1,"type":"dance","type":"event","agent":"x"}"#.into(), Dropped),
        ("null".into(), Dropped),
        ("1".into(), Dropped),
        ("{} x".into(), Dropped),
        (r#"  {"v":1,"type":"dance"}  "#.into(), unknown),
        (r#"{ "v" : 1 , "type" : "ping" }"#.into(), Ping),
        ("\u{feff}{\"v\":1,\"type\":\"dance\"}".into(), Dropped),
        (r#"{"v":1,"type":"dance",}"#.into(), Dropped),
        (r#"{"v":1,/*c*/"type":"dance"}"#.into(), Dropped),
        (deep(126), Ping),
        (deep(127), Ping),
        (deep(128), Ping),
        (deep(129), Dropped),
        (r#"{"v":1,"type":"dance","x":"\ud800"}"#.into(), unknown),
        (
            r#"{"v":1,"type":"event","agent":"nobody","payload":{"hook_event_name":"<é\"'&+\u2028\ud83d\ude00\u0001>"}}"#.into(),
            Exactly(r#"{"ok":true,"outcome":"ignored"}"#),
        ),
        (
            r#"{"v":1,"type":"event","agent":"claude","at":1758800000.5,"pid":7,"payload":{"hook_event_name":"UserPromptSubmit","session_id":"0123456789abcdef","prompt":"hi"}}"#.into(),
            Exactly(r#"{"ok":true,"outcome":"thinking"}"#),
        ),
        (r#"{"v":1,"type":"event"}"#.into(), Exactly(r#"{"ok":true,"outcome":"ignored"}"#)),
        (
            r#"{"v":1,"type":"event","agent":"codex","payload":{"hook_event_name":"UserPromptSubmit","session_id":"019a0000-0000-7000-8000-000000000001","turn_id":"019a0000-0+00-7000-8000-000000000001"}}"#.into(),
            Exactly(r#"{"ok":true,"outcome":"thinking"}"#),
        ),
    ];
    let ping = format!(r#"{{"ok":true,"app":"AiPet","pid":{},"recent":["#, std::process::id());
    for (line, answer) in cases {
        let got = ask(&pet.endpoint, &line).unwrap();
        let shown: String = line.chars().take(80).collect();
        match answer {
            Exactly(reply) => assert_eq!(got.as_deref(), Some(reply), "{shown}"),
            Ping => assert!(got.as_deref().is_some_and(|r| r.starts_with(&ping)), "{shown}: {got:?}"),
            Dropped => assert_eq!(got, None, "{shown}"),
        }
    }
    // the events' lines, as the ping gives them: System.Text.Json's escapes, after each line's time
    let reply = ask(&pet.endpoint, r#"{"v":1,"type":"ping"}"#).unwrap().unwrap();
    for line in [
        r#" nobody pid=0 \u003C\u00E9\u0022\u0027\u0026\u002B\u2028\uD83D\uDE00?\u003E -\u003E ignored""#,
        r#" claude pid=7 UserPromptSubmit 0123456789abc where=? -\u003E thinking""#,
        r#" ? pid=0  -\u003E ignored""#,
        r#" codex pid=0 UserPromptSubmit 019a0000-0000 lag=? where=? -\u003E thinking""#,
    ] {
        assert!(reply.contains(line), "{line} isn't in {reply}");
    }
}

/// An event the C#'s Apply throws on (a Codex turn id its Guid parser takes but its hex reader doesn't) is refused
/// with its `error:` outcome, logged, and changes nothing.
#[test]
fn an_event_the_csharp_throws_on_is_refused() {
    let _side = side_by_side();
    let env = Env::new("error-outcome");
    let pet = Pet::start(&env, &env.sessions());
    let changes = pet.changes.load(Ordering::SeqCst);
    let sid = uuid();
    let at = now();
    let send = |at: f64, turn: &str| {
        let event = payload(&sid, "UserPromptSubmit", json!({"turn_id": turn}));
        ask(&pet.endpoint, &envelope("codex", at, event)).unwrap().unwrap()
    };
    assert_eq!(
        send(at - 2.0, "019a2b3c-0x4d-7e6f-8000-000000000001"),
        r#"{"ok":true,"outcome":"thinking"}"#
    );
    assert_eq!(
        send(at - 1.0, "01a0ed3f-1b7e-7003-8000-0001daa66d13"),
        r#"{"ok":false,"error":"error:FormatException"}"#
    );
    assert_eq!(pet.changes.load(Ordering::SeqCst) - changes, 1);
    let lines = logged(&pet.events_log);
    assert!(
        lines.last().unwrap().ends_with(" event -> error:FormatException"),
        "{lines:?}"
    );
    assert!(
        pet.logged()
            .contains(&"hooks: an event failed: FormatException".to_owned())
    );
}

/// A second pet on the same endpoint can't take it, and its Stop leaves the live one alone
/// (`HookServerUnixTests.ALivePet_IsNotReplaced`; on Windows the name's first instance is taken).
#[test]
fn a_live_pet_is_not_replaced() {
    let _side = side_by_side();
    let env = Env::new("live-pet");
    let pet = Pet::start(&env, &env.sessions());
    let other = Pet::new(&env, &Arc::new(AgentSessions::with_codex_home(env.codex())));
    assert!(other.server.start().is_err());
    let endpoint = env.endpoint.to_string_lossy();
    assert!(
        other.logged()[0].starts_with(&format!("hooks: can't listen on {endpoint}: ")),
        "{:?}",
        other.logged()
    );
    other.server.stop();
    #[cfg(unix)]
    assert!(Path::new(&env.endpoint).exists());
    assert_eq!(pet.serve(), "thinking");
    assert_eq!(pet.logged(), [format!("hooks: listening on {endpoint}")]);
}

// ------------------------------------------------------------------ StarvedPoolTests: the pet busy

/// `Listeners_AnswerHooks_WhileThePoolIsStarved`. The Rust pet has no thread pool: its listeners and handlers are
/// threads of their own, which nothing else of the pet's holds up. So here every CPU is busy, twice over, while 20
/// hooks run at once.
///
/// What is timed is the pet's part of each run, by the hook itself: one that finds no free instance within its budget,
/// or gets no answer within `TIMEOUT` of sending, gives up and traces why. The C#'s ceiling on the whole batch (15 s)
/// isn't kept: on CPUs this busy it mostly timed how long 20 processes take to start, `dotnet` ones above all, and
/// missed by a second now and then with every answer in time.
#[test]
fn listeners_answer_hooks_while_every_cpu_is_busy() {
    let _alone = alone();
    let env = Env::new("busy-hooks");
    let pet = Pet::start(&env, &env.sessions());
    let tmp = env.dir("tmp-busy");
    // the first start of the hook (read in, and maybe scanned) before the CPUs are taken
    finish(start_hook(&env, "claude", "{}", &env.dir("tmp-first"), &[]), HOOK_LIMIT);
    let busy = Busy::new();
    let sids: Vec<String> = (0..20).map(|i| format!("{i:02x}busy-{}", uuid())).collect();
    // all at once
    let runs: Vec<_> = sids
        .iter()
        .map(|sid| {
            let event = payload(sid, "UserPromptSubmit", json!({})).to_string();
            start_hook(&env, "claude", &event, &tmp, &[])
        })
        .collect();
    let results: Vec<HookRun> = runs
        .into_iter()
        .map(|run| finish(run, Duration::from_secs(30)))
        .collect();
    assert!(busy.all_spinning(), "the CPUs weren't busy throughout");
    drop(busy);

    for r in &results {
        r.silent();
    }
    // a hook that gave up, on a busy pipe or a late answer, traces why
    let traces = traces(&tmp);
    assert!(
        traces.is_empty(),
        "a hook gave up: {}",
        fs::read_to_string(&traces[0]).unwrap()
    );
    for sid in &sids {
        let chat = pet
            .chat(&format!("claude:{sid}"))
            .unwrap_or_else(|| panic!("the pet never got {sid}"));
        assert_eq!(chat.state, Some("thinking"));
        assert_eq!(pet.outcomes(sid), ["thinking"]);
    }
}

/// `SilentClients_AreCutOff_AndStopReturns_WhileThePoolIsStarved`, on Windows too: the cut-off is a thread of the
/// server's own there, where the C#'s waits for the pool.
#[test]
fn silent_clients_are_cut_off_and_stop_returns_while_every_cpu_is_busy() {
    let _alone = alone();
    let env = Env::new("busy-silent");
    let pet = Pet::start(&env, &env.sessions());
    let busy = Busy::new();
    let since = Instant::now();
    let took = close_time(&closed(connect_silent(&pet.endpoint), since)).expect("not cut off");
    assert!(
        took >= TIMEOUT - Duration::from_millis(50) && took <= TIMEOUT + Duration::from_secs(2),
        "cut off after {took:?}"
    );

    let since = Instant::now();
    pet.server.stop();
    assert!(
        since.elapsed() < Duration::from_millis(1500),
        "Stop took {:?}",
        since.elapsed()
    );
    #[cfg(unix)]
    assert!(!Path::new(&env.endpoint).exists(), "Stop left the socket file");
    assert!(busy.all_spinning());
}

// ------------------------------------------------------------------ HookServerUnixTests: the socket file

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    #[cfg(target_os = "linux")]
    use std::os::unix::net::UnixStream;

    fn mode(path: &OsStr) -> u32 {
        fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[test]
    fn the_socket_is_the_users_only_and_goes_with_stop() {
        let _side = side_by_side();
        let env = Env::new("socket-mode");
        let pet = Pet::start(&env, &env.sessions());
        assert_eq!(mode(&env.endpoint), 0o600);
        let since = Instant::now();
        pet.server.stop();
        assert!(
            since.elapsed() < Duration::from_millis(1500),
            "Stop took {:?}",
            since.elapsed()
        );
        assert!(!Path::new(&env.endpoint).exists(), "Stop left the socket file");
        assert!(connect_to(&env.endpoint, CONNECT).is_err());
        // a second Stop has nothing left to take down
        pet.server.stop();
    }

    /// A crashed pet leaves its socket file, which refuses connections; whatever is at the path and doesn't listen is
    /// replaced.
    #[test]
    fn what_a_crash_left_is_replaced() {
        let _side = side_by_side();
        for left in ["a closed socket", "a socket not listening", "a file"] {
            let env = Env::new("crash-left");
            let path = Path::new(&env.endpoint);
            let _bound = match left {
                "a closed socket" => {
                    drop(UnixListener::bind(path).unwrap());
                    None
                }
                "a socket not listening" => {
                    let socket = socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None).unwrap();
                    socket.bind(&socket2::SockAddr::unix(path).unwrap()).unwrap();
                    Some(socket)
                }
                _ => {
                    fs::write(path, "").unwrap();
                    None
                }
            };
            assert!(path.exists(), "{left}");
            let pet = Pet::start(&env, &env.sessions());
            assert_eq!(pet.serve(), "thinking", "{left}");
            assert_eq!(mode(&env.endpoint), 0o600, "{left}");
        }
    }

    /// Some program of this user opens connections by the hundred and says nothing: past MAX_CONNECTIONS they're
    /// dropped at once instead of each taking a thread, and once they're gone the pet answers as before.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_flood_of_silent_connections_is_capped_and_the_pet_still_answers() {
        use aipet_core::server::MAX_CONNECTIONS;

        fn threads() -> usize {
            fs::read_dir("/proc/self/task").unwrap().count()
        }

        /// The pet closed it: readable with nothing to read, or broken. (It never sends anything on these.)
        fn is_closed(mut c: &UnixStream) -> bool {
            !matches!(c.read(&mut [0]), Err(e) if e.kind() == io::ErrorKind::WouldBlock)
        }

        const FLOOD: usize = 400;
        let _alone = alone();
        let env = Env::new("flood");
        let pet = Pet::start(&env, &env.sessions());
        let before = threads();
        let since = Instant::now();
        let mut clients: Vec<UnixStream> = (0..FLOOD)
            .map(|_| UnixStream::connect(&env.endpoint).unwrap())
            .collect();
        for c in &clients {
            c.set_nonblocking(true).unwrap();
        }
        // those past the cap are closed long before the silent-client cut-off
        loop {
            let closed = clients.iter().filter(|c| is_closed(c)).count();
            if closed >= FLOOD - MAX_CONNECTIONS {
                break;
            }
            assert!(
                since.elapsed() < TIMEOUT - Duration::from_millis(500),
                "{closed} of {FLOOD} closed after {:?}",
                since.elapsed()
            );
            thread::sleep(Duration::from_millis(20));
        }
        let grew = threads() - before;
        assert!(
            grew < MAX_CONNECTIONS + 30,
            "{grew} more threads for {FLOOD} connections"
        );

        clients.clear();
        // the connections being handled see the end and go
        let gone = Instant::now();
        while threads() > before + 8 {
            assert!(
                gone.elapsed() < Duration::from_secs(5),
                "{} more threads than before the flood",
                threads() - before
            );
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(pet.serve(), "thinking");
        let sid = uuid();
        run_hook(
            &env,
            "codex",
            &payload(&sid, "SessionStart", json!({})),
            &env.tmp(),
            &[],
        )
        .silent();
        assert_eq!(pet.outcomes(&sid), ["idle"]);
    }

    /// Root gets past the socket's mode, so only the peer check keeps a second user out: as root (through sudo, where
    /// it needs no password, as in CI) this test's binary connects and sends an event, which is dropped unanswered.
    /// The same line from this user is answered.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_peer_running_as_another_user_is_refused() {
        let sudo = Command::new("sudo").args(["-n", "true"]).output();
        if !sudo.is_ok_and(|o| o.status.success()) {
            eprintln!("skipped: no sudo without a password, so no second user");
            return;
        }
        let _side = side_by_side();
        let env = Env::new("root-peer");
        let pet = Pet::start(&env, &env.sessions());
        let sid = uuid();
        let line = envelope("claude", now(), payload(&sid, "UserPromptSubmit", json!({})));
        let out = Command::new("sudo")
            .arg("-n")
            .arg("env")
            .arg(format!("AIPET_TEST_ROOT_PEER={}", env.endpoint.to_str().unwrap()))
            .arg(format!("AIPET_TEST_ROOT_LINE={line}"))
            .arg(std::env::current_exe().unwrap())
            .args([
                "unix::root_peer",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && said.contains("1 passed"),
            "{said}{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(pet.chat(&format!("claude:{sid}")).is_none());
        assert_eq!(pet.outcomes(&sid), Vec::<String>::new());
        assert_eq!(
            ask(&pet.endpoint, &line).unwrap().as_deref(),
            Some(r#"{"ok":true,"outcome":"thinking"}"#)
        );
    }

    /// The other user's half of `a_peer_running_as_another_user_is_refused`.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "run as root by a_peer_running_as_another_user_is_refused"]
    fn root_peer() {
        let endpoint = std::env::var("AIPET_TEST_ROOT_PEER").unwrap();
        let line = std::env::var("AIPET_TEST_ROOT_LINE").unwrap();
        // SAFETY: geteuid has no preconditions
        assert_eq!(
            unsafe { libc::geteuid() },
            0,
            "not root: the socket's mode would refuse it, proving nothing"
        );
        let mut pet = UnixStream::connect(&endpoint).unwrap();
        // the pet may have closed it already
        let _ = pet.write_all(format!("{line}\n").as_bytes());
        pet.set_read_timeout(Some(TIMEOUT + Duration::from_secs(5))).unwrap();
        let mut reply = Vec::new();
        match pet.read_to_end(&mut reply) {
            Ok(_) => {}
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::ConnectionReset, "{e}"),
        }
        assert_eq!(String::from_utf8_lossy(&reply), "", "the pet answered root");
    }
}
