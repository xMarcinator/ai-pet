//! The `aipet-hook` binary on its event path, as the agents run it: whatever comes on stdin it exits 0 and prints
//! nothing, and it writes nothing unless the pet was reached. The C#'s own hook suites run against it too, with the
//! .NET pet (cross-runtime.yml); these need neither.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const HOOK: &str = env!("CARGO_BIN_EXE_aipet-hook");

/// Folders of its own for a test's run, removed with it.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("aipet-hook-event-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for sub in ["tmp", "data"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        Scratch(dir)
    }

    fn tmp(&self) -> PathBuf {
        self.0.join("tmp")
    }

    fn data(&self) -> PathBuf {
        self.0.join("data")
    }

    /// An endpoint nothing listens on: a socket path in the scratch folder, or a pipe name nobody made.
    fn no_pet(&self) -> OsString {
        if cfg!(windows) {
            format!("aipet-hook-event-none-{}", std::process::id()).into()
        } else {
            self.0.join("none.sock").into()
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs the hook with `stdin`, at `pipe`, with the scratch folders as its temp and data folders; the child's pid too.
fn run(agent: &str, stdin: &[u8], pipe: &OsString, s: &Scratch, env: &[(&str, &str)]) -> (u32, Output) {
    let mut hook = Command::new(HOOK);
    hook.args(["--agent", agent])
        .env("AIPET_PIPE", pipe)
        .env("AIPET_DATA_DIR", s.data())
        .env("TMPDIR", s.tmp())
        .env("TMP", s.tmp())
        .env("TEMP", s.tmp())
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .env_remove("CLAUDE_CODE_HOST_SESSION_ID")
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = hook.spawn().unwrap();
    let pid = child.id();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    (pid, child.wait_with_output().unwrap())
}

#[track_caller]
fn silent(out: &Output) {
    assert_eq!(
        (out.status.code(), out.stdout.as_slice(), out.stderr.as_slice()),
        (Some(0), &b""[..], &b""[..]),
        "{out:?}"
    );
}

fn contents(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect()
}

/// With no pet it writes nothing anywhere, whatever came on stdin.
#[test]
fn with_no_pet_it_leaves_no_trace() {
    let s = Scratch::new("no-pet");
    for agent in ["claude", "codex"] {
        for stdin in [
            &br#"{"hook_event_name":"UserPromptSubmit","session_id":"s","prompt":"hi"}"#[..],
            b"{not json",
            b"",
            br#"{"a":1,"a":2}"#,
        ] {
            let (_, out) = run(agent, stdin, &s.no_pet(), &s, &[]);
            silent(&out);
            assert_eq!(
                (contents(&s.tmp()), contents(&s.data())),
                (vec![], vec![]),
                "{agent} {stdin:?}"
            );
        }
    }
}

/// A fake pet on a Unix socket: the line the hook sends is the envelope, with `sent` last, and nothing is traced
/// when the pet takes it. What isn't JSON reaches the pet as `{}`, and the trace log, private to the user, says why.
#[cfg(unix)]
#[test]
fn the_pet_gets_the_envelope() {
    use aipet_ipc::connect::read_line;
    use aipet_ipc::protocol::{MAX_REQUEST, unix_time};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    use std::time::SystemTime;

    let s = Scratch::new("pet");
    let socket = s.0.join("pet.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let pet = std::thread::spawn(move || {
        let mut lines = Vec::new();
        for _ in 0..2 {
            let (mut hook, _) = listener.accept().unwrap();
            lines.push(read_line(&mut hook, MAX_REQUEST).unwrap().unwrap());
            hook.write_all(b"{\"ok\":true,\"outcome\":\"thinking\"}\n").unwrap();
        }
        lines
    });

    let started = unix_time(SystemTime::now());
    let event = br#"{"hook_event_name":"PostToolUse","session_id":"s","tool_name":"Bash","tool_input":{"command":"ls","n":1.0},"tool_response":{"stdout":"a lot"}}"#;
    let env = [
        ("CLAUDE_CODE_ENTRYPOINT", "claude-desktop"),
        ("CLAUDE_CODE_HOST_SESSION_ID", "local_1"),
    ];
    let (pid, out) = run("claude", event, &socket.clone().into(), &s, &env);
    silent(&out);
    assert_eq!(
        contents(&s.tmp()),
        Vec::<PathBuf>::new(),
        "traced though the pet took it"
    );
    let (_, out) = run("codex", b"{not json", &socket.clone().into(), &s, &[]);
    silent(&out);
    let ended = unix_time(SystemTime::now());

    let lines = pet.join().unwrap();
    let head = r#"{"v":1,"type":"event","agent":"claude","at":"#;
    assert!(lines[0].starts_with(head), "{}", lines[0]);
    let payload = r#""payload":{"hook_event_name":"PostToolUse","session_id":"s","tool_name":"Bash","tool_input":{"command":"ls","n":1.0}},"sent":"#;
    assert!(lines[0].contains(&format!(r#","pid":{pid},"env":{{"CLAUDE_CODE_ENTRYPOINT":"claude-desktop","CLAUDE_CODE_HOST_SESSION_ID":"local_1"}},{payload}"#)), "{}", lines[0]);
    let sent: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    let (at, sent_at) = (sent["at"].as_f64().unwrap(), sent["sent"].as_f64().unwrap());
    assert!(
        started - 0.001 <= at && at <= sent_at && sent_at <= ended + 0.001,
        "{started} {at} {sent_at} {ended}"
    );

    assert!(
        lines[1].starts_with(r#"{"v":1,"type":"event","agent":"codex","at":"#),
        "{}",
        lines[1]
    );
    assert!(lines[1].contains(r#","payload":{},"sent":"#), "{}", lines[1]);
    // SAFETY: geteuid has no preconditions
    let log = s.tmp().join(format!("aipet-hook-{}.log", unsafe { libc::geteuid() }));
    assert_eq!(contents(&s.tmp()), std::slice::from_ref(&log));
    assert_eq!(std::fs::metadata(&log).unwrap().permissions().mode() & 0o7777, 0o600);
    let traced = std::fs::read_to_string(&log).unwrap();
    assert!(
        traced.contains(" codex: the event isn't JSON, so the pet gets an empty one: "),
        "{traced}"
    );
    assert!(traced.contains(&format!(" pipe={} ", socket.display())), "{traced}");
}
