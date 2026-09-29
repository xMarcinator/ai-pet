//! Codex's events: what each shows (`Codex`, `CodexDescribe`, `AgentSessions.cs:291-396`), where the chat runs, from
//! its transcript (`CodexWhere`, `Originator`, `:398-427`), and their order: by Codex's own ids first (`CodexStale`,
//! `CodexOrder`, `ManualCompact`, `ThreadOf`, `:562-652`), else by `at` and Rank as for Claude.

use std::fs::File;
use std::io::Read;

use serde_json::{Map, Value};

use super::claude::{self, set};
use super::describe::{base_name, contains_ignoring_case, describe as describe_shared, helper, lower_invariant, short};
use super::turns::Turns;
use super::{Chats, Entry, Envelope, Thrown, has_text, str_of};

/// The events a Codex chat takes (SessionEnd, which leaves its tombstone, aside).
pub(super) const EVENTS: [&str; 11] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "Stop",
    "Interrupt",
    "PreCompact",
    "PostCompact",
    "SubagentStart",
    "SubagentStop",
];

/// The events that take the chat's name from Codex's session index even when it has one already.
pub(super) fn names_chat(ev: &str) -> bool {
    matches!(ev, "SessionStart" | "UserPromptSubmit" | "Stop")
}

/// One Codex event, as `AgentSessions::apply` read it from its envelope.
pub(super) struct Event<'a> {
    pub key: &'a str,
    /// The envelope, whose line has a tool call's whole input as the hook wrote it (`call_key`).
    pub envelope: &'a Envelope<'a>,
    /// Its payload.
    pub p: &'a Map<String, Value>,
    pub ev: &'a str,
    pub at: f64,
    /// The originator the chat's transcript names, when it was read and names one.
    pub originator: Option<&'a str>,
    /// The chat's name from Codex's session index, when it was read and has one.
    pub name: Option<&'a str>,
}

/// Places a Codex event; returns the chat's state, `removed`, `ignored` or `stale`, or what the C# throws.
pub(super) fn apply(chats: &mut Chats, e: &Event, now: f64) -> Result<&'static str, Thrown> {
    let (p, ev, at) = (e.p, e.ev, e.at);
    let text = |k: &str| str_of(Some(p), k);
    chats.prune(now);
    if ev == "SessionEnd" {
        return Ok(chats.end(e.key, ev, at, now));
    }
    if !EVENTS.contains(&ev) {
        return Ok("ignored");
    }

    // an event from before what the chat already has arrived late: it's out of date. A turn's prompt that came after
    // the turn's other events (late) still names the chat, but changes nothing else.
    let mut late = false;
    if let Some(old) = chats.get_mut(e.key) {
        let ids = if old.ended.is_none() {
            order(old, e, now)?
        } else {
            Ids::Silent
        };
        match ids {
            Ids::Stale => return Ok("stale"),
            Ids::LatePrompt => late = true,
            Ids::Taken => {}
            Ids::Silent if claude::stale(old, ev, at, now) => return Ok("stale"),
            Ids::Silent => {}
        }
    }
    let (s, is_new) = chats.open(e.key);
    // a new chat (or one taken up again after it ended) starts from this event's turn
    if is_new {
        order(s, e, now)?;
    }
    let chat = s.threads.get("");
    s.turn = chat.map(|t| t.current.clone());
    s.turn_ended = chat.is_some_and(|t| t.ended);
    if !late {
        claude::stamp(s, ev, at, now);
    }
    let before = (s.state, s.detail.clone());
    s.agent = Some("codex");
    if let Some(cwd) = text("cwd").filter(|c| !c.is_empty()) {
        s.cwd = Some(cwd.strip_prefix(r"\\?\").unwrap_or(cwd).into());
    }
    // chats without a transcript are ephemeral runs or Codex's own helper threads (e.g. naming a chat)
    if text("transcript_path").is_some_and(|t| !t.is_empty()) {
        s.has_transcript = true;
    }
    // where the chat runs, once its transcript names the app; until then it's plain "Codex"
    if s.r#where.is_none() {
        s.r#where = place(e.originator);
    }
    if (names_chat(ev) || !has_text(&s.chat_title))
        && let Some(name) = e.name.filter(|n| !n.is_empty())
    {
        s.chat_title = Some(short(name, 44));
    }

    // a sub-agent's event, reported under its parent chat
    let from_helper = text("agent_id").is_some_and(|a| !a.is_empty());
    match ev {
        // also fires on resume/compact/fork in the middle of a chat: don't reset a busy one
        "SessionStart" if is_new || matches!(s.state, Some("idle" | "done")) => set(s, "idle", "Ready", None),
        "SessionStart" => {}
        "UserPromptSubmit" => {
            if !from_helper {
                let prompt = short(text("prompt").unwrap_or("").split('\n').next().unwrap_or(""), 38);
                if !prompt.is_empty() && !prompt.starts_with('/') {
                    s.title = Some(prompt);
                }
            }
            if late {
                return Ok("stale");
            }
            let detail = if from_helper {
                "Delegating to a helper"
            } else {
                "Thinking"
            };
            set(s, "thinking", detail, None);
        }
        "PreToolUse" => {
            let (state, detail, prop) = describe(
                text("tool_name").unwrap_or(""),
                p.get("tool_input").and_then(Value::as_object),
            );
            set(s, state, detail, prop);
        }
        "PermissionRequest" => set(s, "attention", "Needs your permission", None),
        "PostToolUse" | "SubagentStop" => set(s, "thinking", "Thinking", None),
        "Stop" => set(s, "done", "Done", None),
        "Interrupt" => set(s, "idle", "Interrupted", None),
        // an automatic compaction happens inside a turn, whose Stop still follows; /compact is a turn of its own with
        // no Stop, so its PostCompact ends it (see `order`)
        "PreCompact" => set(s, "thinking", "Compacting the conversation", None),
        "PostCompact" if manual_compact(p) => set(s, "done", "Compacted", None),
        "PostCompact" => set(s, "thinking", "Thinking", None),
        "SubagentStart" => set(
            s,
            "working",
            format!("Delegating to {}", helper(text("agent_type"))),
            Some("laptop"),
        ),
        _ => {} // EVENTS has no other
    }
    // ts only moves when what the chat shows changes (so a closed bubble stays closed); one well past now is from
    // before the clock was set back
    if is_new || (s.state, &s.detail) != (before.0, &before.1) || s.ts > now + 5.0 {
        s.ts = at;
    }
    Ok(s.state.unwrap_or_default())
}

/// What Codex's ids tell of an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ids {
    /// Nothing: its time decides.
    Silent,
    /// It's taken, whatever its time.
    Taken,
    /// It's out of date.
    Stale,
    /// A turn's UserPromptSubmit after another event of the turn (even its end): out of date, but its prompt still
    /// names the chat.
    LatePrompt,
}

/// Codex's order. On Windows Codex runs each hook through PowerShell, which starts it 1-4 s late at random, so neither
/// when an event arrives nor `at` (when its hook started) says which came first. Codex's own ids do, as far as they
/// go. Turns are followed per thread (`thread_of`): the chat itself, each of its sub-agents, and any other thread that
/// reports under its session.
///  1. An event of a turn before its thread's current one is stale. Turn ids are UUIDv7s, which begin with the turn's
///     start time, so they sort even when all of a turn's events come late (ids of another shape only tell a turn
///     seen before from a new one; so does a v7 id while the current one's time is well past now, after the clock was
///     set back). A turn's UserPromptSubmit is its first event, so once another event of the turn is in it came late:
///     it only names the chat.
///  2. Nothing of a turn is taken after its end (Stop; SubagentStop for a sub-agent; Interrupt; the PostCompact of a
///     /compact, which is a turn of its own with no Stop). Once the chat's own turn has ended, its other threads'
///     events are stale too, until its next turn.
///  3. Within a turn, a PreToolUse whose call has come further already (its PermissionRequest or PostToolUse is in)
///     is stale (`Turns::call`).
///  4. The chat's own turn starting or ending is taken whatever its time: it comes after everything else of the chat.
///  5. Anything else (SessionStart, which has no turn; one tool call against another; a sub-agent against the chat;
///     an automatic compaction) goes by `at` and Rank as for Claude.
///
/// Wrong when another tool's Stop hook makes Codex go on, or a sub-agent works on past its chat's turn: until the next
/// turn the chat shows done. What an event tells of its thread is recorded even when its time then finds it stale.
fn order(s: &mut Entry, e: &Event, now: f64) -> Result<Ids, Thrown> {
    let p = e.p;
    let Some(turn) = str_of(Some(p), "turn_id").filter(|t| !t.is_empty()) else {
        return Ok(Ids::Silent);
    };
    let thread = thread_of(p);
    let own = thread.is_empty();
    if !own && s.threads.get("").is_some_and(|chat| chat.ended) {
        return Ok(Ids::Stale);
    }
    // a thread's first turn is a new one too
    let mut new_turn = !s.threads.contains_key(&thread);
    let t = s.threads.entry(thread).or_insert_with(|| Turns::new(turn));
    if turn != t.current {
        if t.before(turn, now)? {
            return Ok(Ids::Stale);
        }
        t.next(turn);
        new_turn = true;
    } else if t.seen && e.ev == "UserPromptSubmit" {
        // its turn's prompt, after another event of the turn (even its end)
        return Ok(Ids::LatePrompt);
    } else if t.ended {
        return Ok(Ids::Stale);
    }
    t.seen = true;
    if matches!(e.ev, "Stop" | "SubagentStop" | "Interrupt") || own && manual_compact(p) {
        t.ended = true;
        if !own {
            return Ok(Ids::Silent);
        }
    } else {
        let call = match t.call(e.ev, p, e.envelope, e.at, s.dispatched) {
            Some(true) => Ids::Stale,
            Some(false) => Ids::Taken,
            None => Ids::Silent,
        };
        if call == Ids::Stale || !own || !new_turn {
            return Ok(call);
        }
    }
    // rule 4: the chat's own turn starting or ending is its latest event whatever its time (stamp keeps a later one)
    s.dispatched = e.at;
    Ok(Ids::Taken)
}

/// A PostCompact of /compact, not of an automatic compaction in the middle of a turn.
fn manual_compact(p: &Map<String, Value>) -> bool {
    str_of(Some(p), "hook_event_name") == Some("PostCompact")
        && str_of(Some(p), "trigger") == Some("manual")
        && thread_of(p).is_empty()
}

/// Which of a chat's threads an event is from: "" for the chat itself, `agent:<id>` for a sub-agent, and
/// `transcript:<path>` for any other thread reporting under the chat's session (Codex names the chat's own transcript
/// after the session id).
fn thread_of(p: &Map<String, Value>) -> String {
    if let Some(agent) = str_of(Some(p), "agent_id").filter(|a| !a.is_empty()) {
        return format!("agent:{agent}");
    }
    match str_of(Some(p), "transcript_path") {
        Some(transcript)
            if !transcript.is_empty()
                && !contains_ignoring_case(transcript, str_of(Some(p), "session_id").unwrap_or("")) =>
        {
            format!("transcript:{transcript}")
        }
        _ => String::new(),
    }
}

/// Codex's tool names on top of the shared ones (Bash, web_search, update_plan and mcp__* are already there).
fn describe(tool: &str, input: Option<&Map<String, Value>>) -> (&'static str, String, Option<&'static str>) {
    match tool {
        "apply_patch" => {
            let patch = input
                .and_then(|i| i.get("command"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let files = patched_files(patch);
            let detail = match files.as_slice() {
                [] => "Editing files".into(),
                [file] => format!("Editing {}", base_name(Some(file))),
                many => format!("Editing {} files", many.len()),
            };
            ("working", detail, Some("laptop"))
        }
        "spawn_agent" => ("working", "Delegating to a helper".into(), Some("laptop")),
        "view_image" => ("working", "Looking at an image".into(), Some("lens")),
        _ => describe_shared(tool, input),
    }
}

/// The files a patch changes, each once: what follows `*** Update File: `, `*** Add File: ` or `*** Delete File: ` at
/// the start of a line, trimmed. The C#'s multiline regex `^\*\*\* (?:Update|Add|Delete) File: (.+)$`, whose `.` and
/// `$` stop at `\n` alone.
fn patched_files(patch: &str) -> Vec<&str> {
    let mut files = Vec::new();
    for line in patch.split('\n') {
        let named = ["*** Update File: ", "*** Add File: ", "*** Delete File: "]
            .iter()
            .find_map(|head| line.strip_prefix(head));
        if let Some(file) = named.filter(|f| !f.is_empty()).map(str::trim)
            && !files.contains(&file)
        {
            files.push(file);
        }
    }
    files
}

/// `desktop` (the ChatGPT/Codex desktop app), `terminal` (the Codex CLI), `exec` (codex exec), or another client, from
/// the transcript's originator; none when there's none to read (yet).
fn place(originator: Option<&str>) -> Option<String> {
    match originator? {
        "Codex Desktop" | "codex_work_desktop" => Some("desktop".into()),
        "codex-tui" | "codex_cli_rs" => Some("terminal".into()),
        "codex_exec" => Some("exec".into()),
        "" => None,
        other => Some(lower_invariant(other)),
    }
}

/// How much of the start of a transcript is read for its first line.
const HEAD: u64 = 4 * 1024 * 1024;

/// session_meta.originator from the first line of the chat's transcript ("Codex Desktop", "codex-tui",
/// "codex_exec"). That line also holds the model instructions, so only the first 4 MB of the file are read.
pub(super) fn originator(transcript: Option<&str>) -> Option<String> {
    let path = transcript.filter(|p| !p.is_empty())?;
    // like the C#'s FileStream, it lets Codex go on writing, and even delete the file
    let file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut bytes = Vec::new();
    file.take(len.min(HEAD)).read_to_end(&mut bytes).ok()?;
    // Encoding.UTF8.GetString: a byte order mark stays, and what doesn't decode is U+FFFD
    let text = String::from_utf8_lossy(&bytes);
    let first = text.split('\n').next().unwrap_or_default();
    if !first.contains("\"session_meta\"") {
        return None;
    }
    originator_in(first).map(Into::into)
}

/// The first match of the C#'s regex `"originator"\s*:\s*"([^"]*)"`: the raw text between the quotes, escapes and all.
/// .NET's `\s` is `char.IsWhiteSpace`, the Unicode White_Space property, as `char::is_whitespace` is.
fn originator_in(line: &str) -> Option<&str> {
    const KEY: &str = "\"originator\"";
    let mut from = 0;
    while let Some(i) = line[from..].find(KEY) {
        let start = from + i;
        let value = line[start + KEY.len()..]
            .trim_start_matches(char::is_whitespace)
            .strip_prefix(':')
            .map(|rest| rest.trim_start_matches(char::is_whitespace))
            .and_then(|rest| rest.strip_prefix('"'))
            .and_then(|rest| rest.find('"').map(|end| &rest[..end]));
        if value.is_some() {
            return value;
        }
        // the next try starts one character on (the key's quote), as the regex's does
        from = start + 1;
    }
    None
}
