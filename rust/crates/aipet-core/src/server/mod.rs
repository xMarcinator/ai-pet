//! The hook server: the pet's end of the endpoint, on threads of its own (`src/AiPet.Core/HookServer.cs`).
//!
//! Each hook run connects, sends its event and waits for the outcome: the event goes to [`AgentSessions`], and one
//! line to hook-events.log and to the recent list the doctor pings for. Garbage, oversized or silent connections are
//! dropped.
//!
//! Listening and answering run on threads of their own, `changed` too, and only ever block: nothing waits for the UI
//! thread, so answers keep coming while the rest of the pet is busy.
//! - Windows (`windows`): synchronous pipe instances, whose waits, reads and writes happen in the kernel. A thread of
//!   the server's own cancels a read or write that waits too long.
//! - Unix (`unix`): one socket, only ever used blocking, and a deadline for each connection.

mod answer;
mod record;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as sys;
#[cfg(windows)]
use windows as sys;

use std::any::Any;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use aipet_ipc::connect::read_line;
use aipet_ipc::protocol::MAX_REQUEST;

use crate::sessions::AgentSessions;

/// Windows: instances waiting for a hook at any time, each with a thread that makes the next one as soon as a hook
/// takes it. Unix: threads waiting in `accept` on the one socket (the kernel queues connects meanwhile).
const LISTENERS: usize = 8;
/// Connections handled at once: far above a burst of real hooks, each answered in milliseconds. Past it a connection
/// is dropped at once, so a flood from some program of this user can't take a thread each for
/// [`TIMEOUT`](aipet_ipc::protocol::TIMEOUT) until none can be started.
pub const MAX_CONNECTIONS: usize = 64;
/// The log gets a line about connections the pet couldn't take at most this often: a flood would fill it.
const WARN_EVERY: Duration = Duration::from_secs(10);

/// Where a server listens and what it writes: the pet's own ([`Default`]), or a test's.
pub struct Options {
    /// The pipe's name (Windows) or the socket's path, as [`aipet_ipc::endpoint::endpoint`] gives it.
    pub endpoint: OsString,
    /// hook-events.log, a line for each event.
    pub events_log: PathBuf,
    /// Where the server's `hooks:` lines go: the app log, aipet.log (`Log.Write`).
    pub log: Box<dyn Fn(&str) + Send + Sync>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            endpoint: aipet_ipc::endpoint::endpoint().to_owned(),
            events_log: aipet_ipc::paths::hook_events_log(),
            log: Box::new(crate::log::write),
        }
    }
}

/// The pet's end of the hooks' endpoint (`HookServer`). Stopped when dropped.
pub struct HookServer {
    shared: Arc<Shared>,
}

/// What the server's threads share.
struct Shared {
    sessions: Arc<AgentSessions>,
    changed: Box<dyn Fn() + Send + Sync>,
    options: Options,
    stopping: AtomicBool,
    /// Connections being handled.
    handling: AtomicUsize,
    /// When the log last got a line about connections the pet couldn't take.
    warned: Mutex<Option<Instant>>,
    /// The lines kept for pings, and hook-events.log: one lock for both, as the C#'s `logLock`.
    record: Mutex<record::Record>,
    sys: sys::State,
}

impl HookServer {
    /// The pet's server: at the pet's endpoint, with its hook-events.log and aipet.log. `changed` runs whenever a hook
    /// event changed a chat (the C#'s `Changed` event), on the thread that answers that hook, before the answer goes.
    pub fn new(sessions: Arc<AgentSessions>, changed: impl Fn() + Send + Sync + 'static) -> Self {
        Self::with_options(sessions, changed, Options::default())
    }

    /// The same at another endpoint, with other files: for tests, which keep off the user's pet and data.
    pub fn with_options(
        sessions: Arc<AgentSessions>,
        changed: impl Fn() + Send + Sync + 'static,
        options: Options,
    ) -> Self {
        let record = record::Record::new(options.events_log.clone());
        Self {
            shared: Arc::new(Shared {
                sessions,
                changed: Box::new(changed),
                options,
                stopping: AtomicBool::new(false),
                handling: AtomicUsize::new(0),
                warned: Mutex::new(None),
                record: Mutex::new(record),
                sys: sys::State::default(),
            }),
        }
    }

    /// Starts listening, once, after the app's single-instance check. Whether it could goes to the log as well; the
    /// pet runs on without hooks when it couldn't.
    /// - Windows: the pipe's first instance is made as the first, so a name that is taken already fails (another pet,
    ///   or a squatter).
    /// - Unix: a live pet at the socket's path fails it; what a crashed one left there is replaced.
    pub fn start(&self) -> io::Result<()> {
        let shared = &self.shared;
        let endpoint = shared.options.endpoint.to_string_lossy();
        let started = sys::start(shared);
        match &started {
            Ok(()) => shared.log(&format!("hooks: listening on {endpoint}")),
            Err(e) => shared.log(&format!("hooks: can't listen on {endpoint}: {e}")),
        }
        started
    }

    /// Takes the endpoint down. Hooks being answered finish, and silent ones are still cut off.
    pub fn stop(&self) {
        self.shared.stopping.store(true, Ordering::SeqCst);
        sys::stop(&self.shared);
    }
}

impl Drop for HookServer {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Shared {
    fn stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    fn log(&self, line: &str) {
        (self.options.log)(line);
    }

    /// A log line about connections the pet couldn't take, at most one every 10 s.
    fn warn(&self, why: &str) {
        {
            let mut warned = lock(&self.warned);
            if warned.is_some_and(|last| last.elapsed() < WARN_EVERY) {
                return;
            }
            *warned = Some(Instant::now());
        }
        self.log(&format!("hooks: {why}"));
    }

    /// Hands a connection to a thread of its own. Past [`MAX_CONNECTIONS`], or with no thread to be had, it's dropped
    /// at once: the hook sees the pet close without an answer.
    fn serve(self: &Arc<Self>, conn: sys::Conn) {
        let slot = Slot::take(self);
        if slot.over {
            self.warn(&format!(
                "more than {MAX_CONNECTIONS} connections at once, dropping some"
            ));
            return;
        }
        let shared = Arc::clone(self);
        // a thread that can't start (the user's task limit, say) drops the connection and its slot with it
        if let Err(e) = run(move || {
            let _slot = slot;
            shared.handle(conn);
        }) {
            self.warn(&e.to_string());
        }
    }

    /// One connection: one request line, and one reply line for a request it can answer. A liveness probe, a hook
    /// that gave up, and anything too big, too slow or not JSON get none.
    fn handle(&self, mut conn: sys::Conn) {
        let Ok(Some(line)) = read_line(&mut conn, MAX_REQUEST) else {
            return;
        };
        if let Some(reply) = answer::answer(self, &line) {
            let _ = conn.write_all(format!("{reply}\n").as_bytes());
        }
    }

    fn record(&self) -> MutexGuard<'_, record::Record> {
        lock(&self.record)
    }

    /// Runs `changed`. A panic in it is logged, and the hook still gets its answer, as the C# catches what its
    /// subscribers throw.
    fn changed(&self) {
        if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.changed)())) {
            self.log(&format!("hooks: {}", panic_message(&*panic)));
        }
    }
}

/// A connection being handled, counted from when a listener took it until its thread is done with it.
struct Slot {
    shared: Arc<Shared>,
    over: bool,
}

impl Slot {
    fn take(shared: &Arc<Shared>) -> Slot {
        let handling = shared.handling.fetch_add(1, Ordering::SeqCst) + 1;
        Slot {
            shared: Arc::clone(shared),
            over: handling > MAX_CONNECTIONS,
        }
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.shared.handling.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A thread of the server's own.
fn run(work: impl FnOnce() + Send + 'static) -> io::Result<()> {
    thread::Builder::new().name("AiPet hooks".into()).spawn(work).map(drop)
}

/// A panic left the lock's value as it was: carry on with it.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn panic_message(panic: &(dyn Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("a panic")
}
