//! The status events the listener takes in (Claude's HTTP hooks, and Codex's `report_event`), on their way to
//! the core's [`Ingress`]: one bounded queue, and one thread of its own that applies them in the order they arrived.
//!
//! A route only queues its event and answers at once: it never waits on the chats, the transcripts they read, the
//! record or `changed`, and the runtime's one thread never blocks on them either. When the queue is full (the thread
//! is stuck, or a flood) the event is dropped, with a line in the log.

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use aipet_core::server::Ingress;
use aipet_core::sessions::Envelope;
use aipet_ipc::event;
use aipet_ipc::json::Object;
use aipet_ipc::protocol::MAX_REQUEST;

/// Events waiting at most: far above a burst of real hooks, each applied in milliseconds.
pub(crate) const QUEUE: usize = 256;
/// Their bytes at most, so a flood of big bodies can't hold much memory: four times the largest event.
pub(crate) const QUEUED_BYTES: usize = 4 * MAX_REQUEST;
/// The log gets a line about dropped events at most this often; it counts those it didn't mention.
const DROPPED_EVERY: Duration = Duration::from_secs(10);

/// The queue's end the routes push to. Dropping it (with the listener's state) ends the thread once it has applied
/// what is queued.
pub(crate) struct Events {
    queue: SyncSender<String>,
    /// The bytes of the events waiting.
    queued: Arc<AtomicUsize>,
    room: usize,
    log: Arc<dyn Fn(&str) + Send + Sync>,
    /// When the log last got a line about dropped events, and how many were dropped since.
    dropped: Mutex<(Option<Instant>, usize)>,
}

impl Events {
    /// The queue, and the thread that applies its events through `ingress`.
    pub(crate) fn start(ingress: Arc<Ingress>, log: Arc<dyn Fn(&str) + Send + Sync>) -> io::Result<Events> {
        Self::with_room(ingress, log, QUEUE, QUEUED_BYTES)
    }

    pub(crate) fn with_room(
        ingress: Arc<Ingress>,
        log: Arc<dyn Fn(&str) + Send + Sync>,
        events: usize,
        bytes: usize,
    ) -> io::Result<Events> {
        let (queue, waiting) = mpsc::sync_channel::<String>(events);
        let queued = Arc::new(AtomicUsize::new(0));
        let taken = Arc::clone(&queued);
        let apply_log = Arc::clone(&log);
        thread::Builder::new().name("aipet-mcp events".into()).spawn(move || {
            for line in waiting {
                taken.fetch_sub(line.len(), Ordering::SeqCst);
                apply(&ingress, &line, &*apply_log);
            }
        })?;
        Ok(Events {
            queue,
            queued,
            room: bytes,
            log,
            dropped: Mutex::new((None, 0)),
        })
    }

    /// Queues an envelope ([`event::envelope`]) behind those that arrived before it, as one line that keeps its
    /// tool call's whole input unless that would take it past [`MAX_REQUEST`] ([`event::fit`]). False when it was
    /// dropped, which the log says.
    pub(crate) fn push(&self, mut envelope: Object) -> bool {
        let line = event::fit(&mut envelope, MAX_REQUEST);
        let len = line.len();
        if self.queued.fetch_add(len, Ordering::SeqCst) + len > self.room {
            self.queued.fetch_sub(len, Ordering::SeqCst);
            self.drop_event("the queue is full");
            return false;
        }
        match self.queue.try_send(line) {
            Ok(()) => true,
            Err(e) => {
                self.queued.fetch_sub(len, Ordering::SeqCst);
                self.drop_event(match e {
                    TrySendError::Full(_) => "the queue is full",
                    TrySendError::Disconnected(_) => "the pet is stopping",
                });
                false
            }
        }
    }

    /// An event that won't be applied: one line in the log, at most every 10 s, which counts those in between.
    pub(crate) fn drop_event(&self, why: &str) {
        let mut dropped = self.dropped.lock().unwrap_or_else(PoisonError::into_inner);
        dropped.1 += 1;
        if dropped.0.is_some_and(|last| last.elapsed() < DROPPED_EVERY) {
            return;
        }
        let count = std::mem::take(&mut dropped.1);
        dropped.0 = Some(Instant::now());
        drop(dropped);
        let more = match count {
            1 => String::new(),
            n => format!(" ({} more since the last line)", n - 1),
        };
        (self.log)(&format!("mcp: dropped an event: {why}{more}"));
    }
}

/// One queued event, through the ingress as the pipe server's are. A panic in it is logged, and the thread goes on
/// with the next.
fn apply(ingress: &Ingress, line: &str, log: &dyn Fn(&str)) {
    let applied = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match Envelope::parse(line) {
        Some(envelope) => {
            ingress.apply(&envelope);
        }
        // the listener wrote it, but serde_json reads a few numbers System.Text.Json takes (1e400) as none
        None => log("mcp: dropped an event the chats can't read"),
    }));
    if applied.is_err() {
        log("mcp: applying an event panicked");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Receiver;

    use aipet_core::sessions::AgentSessions;
    use aipet_ipc::json::Node;

    use super::*;

    fn prompt(sid: &str) -> Object {
        let mut payload = Object::default();
        payload.push("hook_event_name", Node::String("UserPromptSubmit".into()));
        payload.push("session_id", Node::String(sid.into()));
        event::envelope("claude", std::time::SystemTime::now(), None, None, payload)
    }

    /// An ingress whose `changed` says it was entered and then waits to be let go, and the log's lines.
    struct Stuck {
        events: Events,
        entered: Receiver<()>,
        go: SyncSender<()>,
        lines: Arc<Mutex<Vec<String>>>,
        root: std::path::PathBuf,
    }

    fn stuck(test: &str, events: usize, bytes: usize) -> Stuck {
        let root = std::env::temp_dir().join(format!("aipet-mcp-events-{}-{test}", std::process::id()));
        let (enter, entered) = mpsc::sync_channel(64);
        let (go, wait) = mpsc::sync_channel::<()>(64);
        let wait = Mutex::new(wait);
        let ingress = Arc::new(Ingress::new(
            Arc::new(AgentSessions::with_codex_home(root.join("codex"))),
            move || {
                let _ = enter.send(());
                let _ = wait.lock().unwrap().recv();
            },
            root.join("hook-events.log"),
            Box::new(|_| {}),
        ));
        let lines = Arc::new(Mutex::new(Vec::new()));
        let logged = Arc::clone(&lines);
        let log: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |line| logged.lock().unwrap().push(line.to_owned()));
        Stuck {
            events: Events::with_room(ingress, log, events, bytes).unwrap(),
            entered,
            go,
            lines,
            root,
        }
    }

    impl Drop for Stuck {
        fn drop(&mut self) {
            for _ in 0..64 {
                let _ = self.go.try_send(());
            }
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// With the thread stuck on one event, the queue takes as many more as it has room for, and the next is dropped
    /// with one line in the log; once the thread goes on, the queue takes events again.
    #[test]
    fn a_full_queue_drops_the_event_with_a_log_line() {
        let stuck = stuck("full", 2, QUEUED_BYTES);
        assert!(stuck.events.push(prompt("a")));
        stuck.entered.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(stuck.events.push(prompt("b")));
        assert!(stuck.events.push(prompt("c")));
        assert!(!stuck.events.push(prompt("d")));
        assert!(!stuck.events.push(prompt("e")));
        assert_eq!(
            *stuck.lines.lock().unwrap(),
            ["mcp: dropped an event: the queue is full"],
            "the second drop within 10 s is only counted"
        );
        stuck.go.send(()).unwrap();
        stuck.entered.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(stuck.events.push(prompt("f")));
    }

    /// The queue's bytes are bounded too: a big event doesn't fit behind another.
    #[test]
    fn the_queued_bytes_are_bounded() {
        // room for two and a half (`at`'s digits vary by a few)
        let one = event::fit(&mut prompt("a"), MAX_REQUEST).len();
        let stuck = stuck("bytes", QUEUE, one * 5 / 2);
        assert!(stuck.events.push(prompt("a")));
        stuck.entered.recv_timeout(Duration::from_secs(10)).unwrap();
        // the one being applied has left the queue
        assert!(stuck.events.push(prompt("b")));
        assert!(stuck.events.push(prompt("c")));
        assert!(!stuck.events.push(prompt("d")));
        assert_eq!(stuck.lines.lock().unwrap().len(), 1);
    }
}
