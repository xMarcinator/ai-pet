//! The hook's contract against sockets that aren't a pet, ported from tests/AiPet.Tests/HookContractTests.cs
//! (HookContractTests' trace-log cases, HookSocketTests and HookRequestSizeTests): a plain Unix socket the test
//! controls stands in for the pet. Whatever comes on stdin the hook exits 0 and prints nothing; it traces only once the
//! pet was reached, into a log that is the user's own. The cases with the real pet's server are aipet-core's
//! tests/server.rs. Also NoPetTests' stale socket (its other case is aipet-hook's tests/event.rs). The tests are in
//! `unix`, with their docs; each is skipped elsewhere.

#[cfg_attr(windows, ignore = "a Unix socket stands in for the pet")]
#[test]
fn trace_log_on_unix_is_per_user_and_private() {
    #[cfg(unix)]
    unix::trace_log_on_unix_is_per_user_and_private();
}

#[cfg_attr(windows, ignore = "a Unix socket stands in for the pet")]
#[test]
fn trace_log_on_unix_leaves_a_file_it_didnt_make_alone() {
    #[cfg(unix)]
    unix::trace_log_on_unix_leaves_a_file_it_didnt_make_alone();
}

#[cfg_attr(windows, ignore = "a Unix socket stands in for the pet")]
#[test]
fn socket_that_never_accepts_ends_the_hook_within_its_budget() {
    #[cfg(unix)]
    unix::socket_that_never_accepts_ends_the_hook_within_its_budget();
}

#[cfg_attr(windows, ignore = "a Unix socket stands in for the pet")]
#[test]
fn lone_surrogates_are_mended_and_the_rest_kept() {
    #[cfg(unix)]
    unix::lone_surrogates_are_mended_and_the_rest_kept();
}

#[cfg_attr(windows, ignore = "a Unix socket stands in for the pet")]
#[test]
fn a_line_just_under_the_limit_stays_within_it_once_sent_is_added() {
    #[cfg(unix)]
    unix::a_line_just_under_the_limit_stays_within_it_once_sent_is_added();
}

#[cfg_attr(not(target_os = "linux"), ignore = "Linux only")]
#[test]
fn hook_with_a_stale_socket_file_leaves_no_trace() {
    #[cfg(target_os = "linux")]
    unix::hook_with_a_stale_socket_file_leaves_no_trace();
}

#[cfg(unix)]
mod unix {
    use std::ffi::OsString;
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, SystemTime};

    use aipet_ipc::connect::read_line;
    use aipet_ipc::protocol::{MAX_REQUEST, MAX_STRING, unix_time};
    use aipet_testkit::{Run, Sandbox, finish, hook, no_pet};
    use serde_json::{Value, json};

    /// A socket path of the test's own (under /tmp: a socket's path must stay under 104 bytes), removed with it.
    struct Socket(OsString);

    impl Socket {
        fn new() -> Socket {
            Socket(no_pet())
        }

        fn path(&self) -> &Path {
            Path::new(&self.0)
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    /// A fake pet: takes `count` connections at `socket`, reads each one's line as the real pet does (at most
    /// MAX_REQUEST bytes) and answers `{"ok":true,...}`. The lines it got.
    fn fake_pet(socket: &Path, count: usize) -> JoinHandle<Vec<String>> {
        let listener = UnixListener::bind(socket).unwrap();
        thread::spawn(move || {
            let mut lines = Vec::new();
            for _ in 0..count {
                let (mut hook, _) = listener.accept().unwrap();
                lines.push(read_line(&mut hook, MAX_REQUEST).unwrap().unwrap_or_default());
                hook.write_all(b"{\"ok\":true,\"outcome\":\"thinking\"}\n").unwrap();
            }
            lines
        })
    }

    /// Runs the hook for claude at `pipe` with `stdin`, its temp folder `tmp` (`TestEnv.StartHook`); when it started and
    /// ended, in Unix seconds.
    fn run_hook(box_: &Sandbox, stdin: &str, pipe: &Path, tmp: &Path) -> (Run, f64, f64) {
        let mut command = box_.command(hook(), None);
        command
            .args(["--agent", "claude"])
            .env("AIPET_PIPE", pipe)
            .env("TMPDIR", tmp)
            .stdin(Stdio::piped());
        let started = unix_time(SystemTime::now());
        let mut child = command.spawn().unwrap();
        child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
        let run = finish(child, Duration::from_secs(20), "the hook");
        (run, started, unix_time(SystemTime::now()))
    }

    #[track_caller]
    fn silent(r: &Run) {
        assert_eq!(
            (r.exit, r.stdout.as_str(), r.stderr.as_str()),
            (Some(0), "", ""),
            "the hook wasn't silent"
        );
    }

    fn contents(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect()
    }

    /// The hook's trace log in `dir`: named after the effective uid.
    fn trace_log(dir: &Path) -> PathBuf {
        // SAFETY: geteuid has no preconditions
        dir.join(format!("aipet-hook-{}.log", unsafe { libc::geteuid() }))
    }

    /// The temp folder is shared (/tmp), so the trace log is the user's own there, and only they can read it.
    pub fn trace_log_on_unix_is_per_user_and_private() {
        let box_ = Sandbox::new();
        let socket = Socket::new();
        let pet = fake_pet(socket.path(), 1);
        let tmp = box_.dir("trace");
        let (r, ..) = run_hook(&box_, "{not json", socket.path(), &tmp);
        silent(&r);
        pet.join().unwrap();
        let log = trace_log(&tmp);
        assert_eq!(contents(&tmp), std::slice::from_ref(&log));
        let traced = fs::read_to_string(&log).unwrap();
        assert!(traced.contains("isn't JSON"), "{traced}");
        assert_eq!(fs::metadata(&log).unwrap().permissions().mode() & 0o777, 0o600);
    }

    /// Another user can put a file others can read, or a link, at the log's name first. The hook writes to neither (the
    /// test owns both, which is as close as one user gets).
    pub fn trace_log_on_unix_leaves_a_file_it_didnt_make_alone() {
        let box_ = Sandbox::new();
        let open = box_.dir("open");
        let log = trace_log(&open);
        fs::write(&log, "").unwrap();
        fs::set_permissions(&log, fs::Permissions::from_mode(0o666)).unwrap();

        let linked = box_.dir("linked");
        let target = linked.join("keys");
        fs::write(&target, "").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&target, trace_log(&linked)).unwrap();

        let socket = Socket::new();
        let pet = fake_pet(socket.path(), 2);
        for tmp in [&open, &linked] {
            let (r, ..) = run_hook(&box_, "{not json", socket.path(), tmp);
            silent(&r);
        }
        pet.join().unwrap();
        let mode = fs::metadata(&log).unwrap().permissions().mode() & 0o777;
        assert_eq!((fs::read_to_string(&log).unwrap(), mode), (String::new(), 0o666));
        assert_eq!(fs::read_to_string(&target).unwrap(), "");
    }

    /// A pet that has stopped taking connections (suspended, with its queue full): connect() on Unix has no time limit,
    /// so the hook gives up on its own within its budget, and traces it (the socket is there).
    pub fn socket_that_never_accepts_ends_the_hook_within_its_budget() {
        use std::os::fd::{FromRawFd, OwnedFd};
        use std::os::unix::ffi::OsStrExt;

        let socket = Socket::new();
        let new_socket = |flags: libc::c_int| {
            // SAFETY: a plain socket call; the descriptor is owned at once
            let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC | flags, 0) };
            assert!(fd >= 0, "{}", std::io::Error::last_os_error());
            // SAFETY: a descriptor just made, owned by nothing else
            unsafe { OwnedFd::from_raw_fd(fd) }
        };
        // SAFETY: zeroed is a valid sockaddr_un
        let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        address.sun_family = libc::AF_UNIX as libc::sa_family_t;
        for (to, from) in address.sun_path.iter_mut().zip(socket.0.as_bytes()) {
            *to = *from as libc::c_char;
        }
        let length = std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t;
        let address_of = &address as *const libc::sockaddr_un as *const libc::sockaddr;
        let listener = new_socket(0);
        use std::os::fd::AsRawFd;
        // SAFETY: a socket and an address of the given length
        assert_eq!(unsafe { libc::bind(listener.as_raw_fd(), address_of, length) }, 0);
        // a backlog of 0 queues one connection; this one fills it, and nothing ever accepts
        // SAFETY: a bound socket
        assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 0) }, 0);
        let _queued = UnixStream::connect(socket.path()).unwrap();
        let probe = new_socket(libc::SOCK_NONBLOCK);
        // SAFETY: as bind's
        assert_eq!(
            unsafe { libc::connect(probe.as_raw_fd(), address_of, length) },
            -1,
            "the queue isn't full"
        );

        let box_ = Sandbox::new();
        let tmp = box_.dir("trace");
        let stdin = json!({"hook_event_name": "Stop", "session_id": "s"}).to_string();
        let (r, started, ended) = run_hook(&box_, &stdin, socket.path(), &tmp);
        silent(&r);
        // Claude's limit, with room for starting the hook on a busy machine
        assert!(ended - started <= 5.0, "took {} s", ended - started);
        let traced = fs::read_to_string(trace_log(&tmp)).unwrap();
        assert!(traced.contains("stayed busy"), "{traced}");
    }

    /// What the hook sends for escaped lone surrogates: each one as U+FFFD, and everything around it as it was.
    pub fn lone_surrogates_are_mended_and_the_rest_kept() {
        let box_ = Sandbox::new();
        let socket = Socket::new();
        let pet = fake_pet(socket.path(), 1);
        let big = "y".repeat(MAX_STRING - 1);
        let stdin = format!(
            "{{\"hook_event_name\":\"UserPromptSubmit\",\"session_id\":\"s\",\
             \"a\":\"x\\ud800y\",\"b\":\"\\uDC00\",\"c\":\"\\ud83d\\ude00\",\"d\":\"\\\\ud800\",\"e\":\"\\ud800\\ud83d\\ude00\",\
             \"f\":\"\\ud83d\\ud83d\",\"g\":\"end\\ud83d\",\"k\\udfff\":1,\"h\":\"q\\\"\\n\\u0041\",\
             \"big\":\"{big}\\ud800tail\"}}"
        );
        let tmp = box_.dir("trace");
        let (r, started, ended) = run_hook(&box_, &stdin, socket.path(), &tmp);
        silent(&r);
        let line = pet.join().unwrap().remove(0);
        let envelope: Value = serde_json::from_str(&line).unwrap();
        let p = &envelope["payload"];
        let s = |key: &str| p[key].as_str().unwrap_or_else(|| panic!("no {key} in {p}")).to_owned();
        assert_eq!(
            [s("a"), s("b"), s("c"), s("d"), s("e"), s("f"), s("g"), s("h")],
            [
                "x\u{FFFD}y",
                "\u{FFFD}",
                "\u{1F600}",
                "\\ud800",
                "\u{FFFD}\u{1F600}",
                "\u{FFFD}\u{FFFD}",
                "end\u{FFFD}",
                "q\"\nA"
            ]
        );
        assert_eq!(p["k\u{FFFD}"], 1);
        assert_eq!(s("big"), format!("{big}\u{FFFD}"));
        // sent is when it was sent, after it started
        let (at, sent) = (envelope["at"].as_f64().unwrap(), envelope["sent"].as_f64().unwrap());
        assert!(started - 0.01 <= at && at <= sent, "{started} {at} {sent}");
        assert!(sent <= ended + 0.01, "{sent} {ended}");
        assert_eq!(contents(&tmp), Vec::<PathBuf>::new());
    }

    /// The line a hook sends to a fake pet that takes at most MAX_REQUEST bytes, like the real one.
    fn send(stdin: &str) -> String {
        let box_ = Sandbox::new();
        let socket = Socket::new();
        let pet = fake_pet(socket.path(), 1);
        let (r, ..) = run_hook(&box_, stdin, socket.path(), &box_.dir("trace"));
        silent(&r);
        pet.join().unwrap().remove(0)
    }

    /// A tool call whose line is just under MAX_REQUEST before sent is added: it has to be cut down to what the pet
    /// reads, or sent takes it over the limit and the pet drops the whole event.
    pub fn a_line_just_under_the_limit_stays_within_it_once_sent_is_added() {
        let big = "x".repeat(MAX_STRING);
        let stdin = |pad: usize| {
            let mut input = json!({"file_path": "/f", "pad": "p".repeat(pad)});
            for i in 0..15 {
                input[format!("s{i}")] = json!(big);
            }
            json!({"hook_event_name": "PreToolUse", "session_id": "s", "tool_name": "MultiEdit", "tool_input": input})
                .to_string()
        };
        // the first line says how long the rest of it is; the second is 10 bytes under the limit without sent
        let first = send(&stdin(0));
        let rest = first.rfind(",\"sent\":").expect("sent is added") + 1;
        assert!(rest > 1 && first.is_ascii(), "{}", first.len());
        let pad = MAX_REQUEST - 10 - rest;
        assert!((1..=MAX_STRING).contains(&pad), "{pad}");
        let line = send(&stdin(pad));
        assert!((1..=MAX_REQUEST).contains(&line.len()), "{}", line.len());
        let sent: Value = serde_json::from_str(&line).unwrap();
        let keys: Vec<&String> = sent["payload"]["tool_input"].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["file_path"]);
        assert!(!sent["sent"].is_null());
    }

    /// What a crashed pet leaves: a socket file nothing listens on any more. The hook gives up at once, and with no
    /// pet reached it writes nothing, neither a trace nor anything in the data folder.
    #[cfg(target_os = "linux")]
    pub fn hook_with_a_stale_socket_file_leaves_no_trace() {
        let socket = Socket::new();
        drop(UnixListener::bind(socket.path()).unwrap());
        assert!(socket.path().exists());
        let box_ = Sandbox::new();
        for agent in ["claude", "codex"] {
            let tmp = box_.dir(&format!("trace-{agent}"));
            let mut command = box_.command(hook(), None);
            command
                .args(["--agent", agent])
                .env("AIPET_PIPE", socket.path())
                .env("TMPDIR", &tmp)
                .stdin(Stdio::piped());
            let mut child = command.spawn().unwrap();
            let stdin = json!({"hook_event_name": "Stop", "session_id": "s"}).to_string();
            child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
            silent(&finish(child, Duration::from_secs(20), "the hook"));
            assert_eq!(contents(&tmp), Vec::<PathBuf>::new(), "{agent}");
            assert_eq!(contents(&box_.aipet()), Vec::<PathBuf>::new(), "{agent}");
        }
    }
}
