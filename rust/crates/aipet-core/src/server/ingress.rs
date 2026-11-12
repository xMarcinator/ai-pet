//! Where every hook event goes in, whichever way it came: the pipe or socket ([`HookServer`](super::HookServer)) or
//! the pet's HTTP listener (`aipet-mcp`). One [`Ingress`] holds the chats ([`AgentSessions`]), the record of events
//! (hook-events.log and the last 30 lines for pings and `status`) and `changed`, so events from either side land in
//! the same chats and the same record, in the order they are applied.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use aipet_ipc::protocol::{AGENT, unix_time};

use super::record::Record;
use super::{lock, panic_message};
use crate::sessions::{AgentSessions, Envelope};

/// The chats, the record and `changed`, shared by everything that takes hook events. Made once by the core.
pub struct Ingress {
    sessions: Arc<AgentSessions>,
    changed: Box<dyn Fn() + Send + Sync>,
    log: Box<dyn Fn(&str) + Send + Sync>,
    /// The lines kept for pings, and hook-events.log: one lock for both, as the C#'s `logLock`.
    record: Mutex<Record>,
    /// Whether an event from Claude, then Codex, came in since the pet started.
    seen: [AtomicBool; 2],
}

impl Ingress {
    /// `changed` runs whenever an event changed a chat (the C#'s `Changed` event), on the thread that applied it.
    /// Every event's line is appended to `events_log` (hook-events.log); `log` gets the `hooks:` lines (aipet.log).
    pub fn new(
        sessions: Arc<AgentSessions>,
        changed: impl Fn() + Send + Sync + 'static,
        events_log: PathBuf,
        log: Box<dyn Fn(&str) + Send + Sync>,
    ) -> Self {
        Self {
            sessions,
            changed: Box::new(changed),
            log,
            record: Mutex::new(Record::new(events_log)),
            seen: Default::default(),
        }
    }

    pub fn sessions(&self) -> &Arc<AgentSessions> {
        &self.sessions
    }

    /// Applies an event to the chats and records its line, then runs `changed` unless the event changed nothing or
    /// failed. The outcome: the chat's state, `removed`, `ignored`, `stale`, or `error:<why>` for one that failed
    /// (logged too).
    pub fn apply(&self, envelope: &Envelope) -> &'static str {
        let applied = self.sessions.apply(envelope, unix_time(SystemTime::now()));
        let failed = applied.outcome.strip_prefix("error:");
        if let Some(error) = failed {
            self.log(&format!("hooks: an event failed: {error}"));
        }
        if let Some(seen) = envelope
            .fields
            .get(AGENT)
            .and_then(|agent| agent.as_str())
            .and_then(agent_index)
        {
            self.seen[seen].store(true, Ordering::Relaxed);
        }
        lock(&self.record).add(&applied.log, SystemTime::now());
        if failed.is_none() && !matches!(applied.outcome, "ignored" | "stale") {
            self.changed();
        }
        applied.outcome
    }

    /// The last 30 lines of hook-events.log this pet wrote, oldest first: the doctor's ping and `status` show them.
    pub fn recent(&self) -> Vec<String> {
        lock(&self.record).recent().map(str::to_owned).collect()
    }

    /// Whether an event from `agent` (`claude` or `codex`) came in since the pet started, from either side.
    pub fn seen(&self, agent: &str) -> bool {
        agent_index(agent).is_some_and(|i| self.seen[i].load(Ordering::Relaxed))
    }

    /// A line for the app's log.
    pub fn log(&self, line: &str) {
        (self.log)(line);
    }

    /// Runs `changed`. A panic in it is logged, and the event still gets its answer, as the C# catches what its
    /// subscribers throw.
    fn changed(&self) {
        if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.changed)())) {
            self.log(&format!("hooks: {}", panic_message(&*panic)));
        }
    }
}

fn agent_index(agent: &str) -> Option<usize> {
    match agent {
        "claude" => Some(0),
        "codex" => Some(1),
        _ => None,
    }
}
