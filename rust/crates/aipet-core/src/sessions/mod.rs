//! AgentSessions: each chat's state from the hooks' events, in the order the agents meant them
//! (`src/AiPet.Core/AgentSessions.cs`).
//!
//! The chats as their hooks report them, kept by the running pet alone: every hook run forwards its event (see
//! [`crate::server`]) and [`AgentSessions::apply`] places it. Events run in the background and can finish out of
//! order, so each is placed by when the agent started it: Claude's by `at`, and a call's PreToolUse and
//! PermissionRequest by each other (`claude`); Codex's by its own turn and tool ids first (`codex`, `turns`). A
//! chat's `ts` only moves when what it shows changes.
//!
//! Thread-safe: the hook server applies events from its handler threads and the Board takes a
//! [`snapshot`](AgentSessions::snapshot) on the UI thread. Files (chat titles, Codex's transcripts and session index)
//! are read on the caller's thread before the lock is taken, and only when the event will use them, which a copy of
//! the chat's entry decides.

mod claude;
mod codex;
mod describe;
mod titles;
mod turns;

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use aipet_ipc::protocol;
use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

/// A request line as the hook server parsed it: `HookServer.Answer`'s JsonObject.
///
/// `fields` are its members as serde_json reads them. The line is kept for what they lose and the C# still has: the
/// order of an object's keys and how its numbers are spelled, which make a tool call's whole input one call or another
/// (`claude::call_key`). That input is read from the line, so changing `fields` (as the golden replay changes a
/// transcript_path) doesn't change it.
pub struct Envelope<'a> {
    pub fields: Map<String, Value>,
    line: &'a str,
}

impl<'a> Envelope<'a> {
    /// The line's members; none when it isn't a JSON object, as the C# takes none.
    pub fn parse(line: &'a str) -> Option<Self> {
        let fields = serde_json::from_str(line).ok()?;
        Some(Self { fields, line })
    }

    /// The payload's `tool_input` as the line writes it: the last of each key, as `fields` keep them.
    fn tool_input_text(&self) -> Option<&'a str> {
        let payload = last(members(self.line)?, protocol::PAYLOAD)?;
        last(members(payload.get())?, "tool_input").map(RawValue::get)
    }
}

/// A chat, keyed `claude:<sid>` or `codex:<sid>`. After SessionEnd only `id`, `ended` and `ts` are left, for a day.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub id: String,
    /// `claude` or `codex`.
    pub agent: Option<&'static str>,
    /// `idle`, `thinking`, `working`, `attention` or `done`.
    pub state: Option<&'static str>,
    /// The bubble's second line: "Running command", "Needs your permission", …
    pub detail: Option<String>,
    /// `laptop`, `lens` or none.
    pub prop: Option<&'static str>,
    /// The first line of the chat's latest prompt that isn't a /command.
    pub title: Option<String>,
    /// The agent's own name for the chat.
    pub chat_title: Option<String>,
    pub cwd: Option<String>,
    /// Where the chat runs: `desktop`, `terminal` or the client's own name. Claude's comes from the hook's
    /// environment, Codex's from the transcript's originator (none until it names one).
    pub r#where: Option<String>,
    /// The Claude app's own id for the chat (`local_<uuid>`), for its deep link.
    pub host_id: Option<String>,
    /// When the hook of the event behind the last visible change started (Unix seconds).
    pub ts: f64,
    /// When the hook of the chat's latest event started, and how far into a turn that event comes: what a late
    /// event is placed against.
    pub dispatched: f64,
    pub dispatched_rank: i32,
    /// When the chat ended: the entry is a tombstone, so an event from before the end that finishes late can't bring
    /// the chat back.
    pub ended: Option<f64>,
    /// Codex: the chat has a transcript (chats without one are exec runs or Codex's own helper threads).
    pub has_transcript: bool,
    /// Codex: the chat's own current turn, and whether its end is in, for the Board, which orders the hook's report
    /// against the Codex log watcher's by turn.
    pub turn: Option<String>,
    pub turn_ended: bool,
    /// Claude: its latest PreToolUses and PermissionRequests, to pair a call's two. Only `apply` uses it.
    asks: Vec<claude::Ask>,
    /// Codex: how far each of the chat's threads has come, by thread (`codex::order`). Only `apply` uses it.
    threads: HashMap<String, turns::Turns>,
}

/// What [`AgentSessions::apply`] made of one event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    /// The chat's state, `removed` after SessionEnd, `ignored` or `stale`: the hook's answer. Or
    /// `error:FormatException` for an event the C#'s Apply throws on (a Codex turn id that its Guid parser takes but
    /// its hex reader doesn't), which its hook server refuses.
    pub outcome: &'static str,
    /// The line for hook-events.log, which never holds prompt text.
    pub log: String,
}

/// What the C#'s Apply throws on an event, as its hook server reports it: `error:<exception type>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Thrown(&'static str);

impl Thrown {
    const FORMAT: Thrown = Thrown("error:FormatException");
}

/// The agent chats, in the order they first appeared: the Board breaks exact ties by it.
#[derive(Default)]
pub struct AgentSessions {
    chats: Mutex<Chats>,
    /// Where Codex's session index is read: `$CODEX_HOME` (read each time, as the C#'s `Paths.CodexHome` is), unless
    /// the store was given a folder of its own.
    codex_home: Option<PathBuf>,
}

impl AgentSessions {
    pub fn new() -> Self {
        Self::default()
    }

    /// A store that reads Codex's chat names from `<codex_home>/session_index.jsonl` rather than `$CODEX_HOME`'s: for
    /// tests, which keep off the user's data.
    pub fn with_codex_home(codex_home: impl Into<PathBuf>) -> Self {
        Self {
            codex_home: Some(codex_home.into()),
            ..Self::default()
        }
    }

    /// Copies of the chats' entries, in the order the chats first appeared.
    pub fn snapshot(&self) -> Vec<Entry> {
        self.lock().0.clone()
    }

    /// Places one hook event: an envelope (`aipet_ipc::protocol`) as the hook server parsed it, at `now` (Unix
    /// seconds, as `protocol::unix_time` gives them). An envelope without a numeric `at` counts as started at `now`.
    pub fn apply(&self, envelope: &Envelope, now: f64) -> Applied {
        let fields = &envelope.fields;
        let empty = Map::new();
        let p = fields
            .get(protocol::PAYLOAD)
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let agent = str_of(Some(fields), protocol::AGENT);
        let ev = str_of(Some(p), "hook_event_name").unwrap_or("");
        let at = fields.get(protocol::AT).and_then(Value::as_f64).unwrap_or(now);
        // `as` truncates and saturates, as the C#'s (long) cast does
        let pid = format!("pid={}", num(fields, protocol::PID) as i64);
        match agent {
            Some("claude") => {
                let sid = str_of(Some(p), "session_id").unwrap_or("default");
                let key = format!("claude:{sid}");
                let env = fields.get(protocol::ENV).and_then(Value::as_object);
                let place = claude::place(str_of(env, "CLAUDE_CODE_ENTRYPOINT"));
                // read before the lock, and only when the event will look at it (None: not read)
                let title = (claude::EVENTS.contains(&ev)
                    && (claude::names_chat(ev) || !self.peek(&key, now).is_some_and(|e| has_text(&e.chat_title))))
                .then(|| titles::chat_title(str_of(Some(p), "transcript_path")));
                let event = claude::Event {
                    key: &key,
                    envelope,
                    p,
                    ev,
                    at,
                    place: place.as_deref(),
                    env,
                    title: title.as_deref(),
                };
                let outcome = claude::apply(&mut self.lock(), &event, now);
                let place = clean(place.as_deref().unwrap_or("?"));
                Applied {
                    outcome,
                    log: format!("claude {pid} {} {} where={place} -> {outcome}", clean(ev), sid13(sid)),
                }
            }
            Some("codex") => {
                // an older hook's "packaged" and "parent" (which process ran it) are ignored
                let sid = str_of(Some(p), "session_id");
                let (mut outcome, mut place) = ("ignored", None);
                if let Some(sid) = sid.filter(|s| !s.is_empty()) {
                    let key = format!("codex:{sid}");
                    // read before the lock, and only when the event will look at them
                    let (mut originator, mut name) = (None, None);
                    if codex::EVENTS.contains(&ev) {
                        let e = self.peek(&key, now);
                        if e.as_ref().is_none_or(|e| e.r#where.is_none()) {
                            originator = codex::originator(str_of(Some(p), "transcript_path"));
                        }
                        if codex::names_chat(ev) || !e.is_some_and(|e| has_text(&e.chat_title)) {
                            let home = self.codex_home.clone().unwrap_or_else(aipet_ipc::paths::codex_home);
                            name = titles::thread_name(&home, sid);
                        }
                    }
                    let event = codex::Event {
                        key: &key,
                        envelope,
                        p,
                        ev,
                        at,
                        originator: originator.as_deref(),
                        name: name.as_deref(),
                    };
                    let mut chats = self.lock();
                    outcome = match codex::apply(&mut chats, &event, now) {
                        Ok(outcome) => outcome,
                        // as the C#'s hook server reports an exception
                        Err(Thrown(error)) => {
                            return Applied {
                                outcome: error,
                                log: format!("event -> {error}"),
                            };
                        }
                    };
                    place = chats.get(&key).and_then(|s| s.r#where.clone());
                }
                // lag: from the hook's start to its sending the event (reading stdin, waiting for the pet)
                let sent = num(fields, protocol::SENT);
                let lag = if sent > 0.0 {
                    format!("{}ms", whole((sent - at) * 1000.0))
                } else {
                    "?".into()
                };
                let place = clean(place.as_deref().unwrap_or("?"));
                Applied {
                    outcome,
                    log: format!(
                        "codex {pid} {} {} lag={lag} where={place} -> {outcome}",
                        clean(ev),
                        sid13(sid.unwrap_or(""))
                    ),
                }
            }
            _ => Applied {
                outcome: "ignored",
                log: format!("{} {pid} {} -> ignored", clean(agent.unwrap_or("?")), clean(ev)),
            },
        }
    }

    /// A copy of the chat's live entry, to decide which files to read before taking the lock; none for no chat, one
    /// that ended, or one the event is about to prune.
    fn peek(&self, key: &str, now: f64) -> Option<Entry> {
        self.lock()
            .get(key)
            .filter(|e| e.ended.is_none() && now - e.ts <= 86400.0)
            .cloned()
    }

    /// The chats. A panic while they were held left them as they were: carry on with them.
    fn lock(&self) -> MutexGuard<'_, Chats> {
        self.chats.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The chats' entries, in the order the chats first appeared.
#[derive(Default)]
struct Chats(Vec<Entry>);

impl Chats {
    fn index(&self, key: &str) -> Option<usize> {
        self.0.iter().position(|e| e.id == key)
    }

    fn get(&self, key: &str) -> Option<&Entry> {
        self.0.iter().find(|e| e.id == key)
    }

    fn get_mut(&mut self, key: &str) -> Option<&mut Entry> {
        self.0.iter_mut().find(|e| e.id == key)
    }

    /// Puts the entry in its chat's place, or last for a chat that has none.
    fn put(&mut self, entry: Entry) -> &mut Entry {
        let i = match self.index(&entry.id) {
            Some(i) => {
                self.0[i] = entry;
                i
            }
            None => {
                self.0.push(entry);
                self.0.len() - 1
            }
        };
        &mut self.0[i]
    }

    /// A chat's entry, or a fresh one (true) for a new chat or one taken up again after it ended.
    fn open(&mut self, key: &str) -> (&mut Entry, bool) {
        if let Some(i) = self.index(key)
            && self.0[i].ended.is_none()
        {
            return (&mut self.0[i], false);
        }
        let fresh = Entry {
            id: key.into(),
            state: Some("idle"),
            detail: Some("Ready".into()),
            title: Some(String::new()),
            ..Entry::default()
        };
        (self.put(fresh), true)
    }

    /// SessionEnd: the chat goes, leaving its tombstone for a day.
    fn end(&mut self, key: &str, ev: &str, at: f64, now: f64) -> &'static str {
        if self.get(key).is_some_and(|last| claude::stale(last, ev, at, now)) {
            return "stale";
        }
        self.put(Entry {
            id: key.into(),
            ended: Some(at),
            ts: at,
            ..Entry::default()
        });
        "removed"
    }

    /// Chats not heard of for a day go.
    fn prune(&mut self, now: f64) {
        self.0.retain(|e| now - e.ts <= 86400.0);
    }
}

// ------------------------------------------------------------------ JSON and log helpers
/// A string field of a JSON object; none when it's missing or not a string.
fn str_of<'a>(o: Option<&'a Map<String, Value>>, key: &str) -> Option<&'a str> {
    o?.get(key)?.as_str()
}

/// A JSON text's members in order, duplicates too, their values as the text writes them; none when it isn't an
/// object.
fn members(text: &str) -> Option<Vec<(String, &RawValue)>> {
    struct Members<'a>(Vec<(String, &'a RawValue)>);
    impl<'de> Deserialize<'de> for Members<'de> {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Each;
            impl<'de> Visitor<'de> for Each {
                type Value = Members<'de>;

                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("a JSON object")
                }

                fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Members<'de>, A::Error> {
                    let mut members = Vec::new();
                    while let Some(member) = map.next_entry()? {
                        members.push(member);
                    }
                    Ok(Members(members))
                }
            }
            deserializer.deserialize_map(Each)
        }
    }
    serde_json::from_str::<Members>(text).ok().map(|m| m.0)
}

/// The value of the last member with the key.
fn last<'a>(members: Vec<(String, &'a RawValue)>, key: &str) -> Option<&'a RawValue> {
    members.into_iter().rev().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// A numeric field of a JSON object; 0 when it's missing or not a number.
fn num(o: &Map<String, Value>, key: &str) -> f64 {
    o.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn has_text(s: &Option<String>) -> bool {
    s.as_deref().is_some_and(|s| !s.is_empty())
}

/// A number as .NET writes it with the custom format "0" in the invariant culture: to 15 significant digits, then
/// rounded half away from zero to a whole number, which keeps its sign even when it is 0 (-0.4 gives "-0").
fn whole(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x.is_infinite() {
        return format!("{sign}Infinity");
    }
    // d.dddddddddddddde<exponent>: 15 digits, correctly rounded (ties to even), as .NET's own conversion gives them
    let scientific = format!("{:.14e}", x.abs());
    let (mantissa, exponent) = scientific.split_once('e').expect("a number in scientific notation");
    let mut digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    let exponent: i64 = exponent.parse().expect("an exponent");
    // how many of the digits come before the point: none under 1, which rounds to 1 from 0.5 up and else to 0
    let n = usize::try_from(exponent + 1).ok();
    let up = n.is_some_and(|n| digits.get(n).is_some_and(|&d| d >= b'5'));
    digits.resize(n.unwrap_or(0), b'0');
    if up {
        // carry: 999.5 becomes 1000
        match digits.iter().rposition(|&d| d != b'9') {
            Some(i) => {
                digits[i] += 1;
                digits[i + 1..].fill(b'0');
            }
            None => {
                digits.fill(b'0');
                digits.insert(0, b'1');
            }
        }
    }
    if digits.is_empty() {
        digits.push(b'0');
    }
    format!("{sign}{}", String::from_utf8(digits).expect("ASCII digits"))
}

/// A session id as the log shows it: its first 13 UTF-16 units, or `-` for none.
fn sid13(sid: &str) -> String {
    if sid.is_empty() {
        "-".into()
    } else {
        clean(&describe::utf16_prefix(sid, 13))
    }
}

/// A value from an event as the log shows it: on one line, and at most 60 UTF-16 units.
fn clean(s: &str) -> String {
    describe::utf16_prefix(s, 60)
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}
