//! Claude's events: what each shows (`AgentSessions.cs:118-264`), and their order: by `at` and Rank (`Stale`,
//! `Stamp`), except that a call's PreToolUse and PermissionRequest are paired within a 1 s window (`ClaudePair`,
//! `:487-560`), by what the call runs (`CallKey`, `:735-741`).

use std::borrow::Cow;
use std::hash::{DefaultHasher, Hash, Hasher};

use serde_json::{Map, Value};

use super::describe::{contains_ignoring_case, describe, helper, short};
use super::{Chats, Entry, Envelope, has_text, str_of};

/// The events a Claude chat takes (SessionEnd, which leaves its tombstone, aside).
pub(super) const EVENTS: [&str; 16] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "PermissionDenied",
    "Notification",
    "Elicitation",
    "ElicitationResult",
    "PreCompact",
    "PostCompact",
    "SubagentStart",
    "SubagentStop",
    "Stop",
    "StopFailure",
];

/// The events that take the chat's title from its transcript even when it has one already.
pub(super) fn names_chat(ev: &str) -> bool {
    matches!(
        ev,
        "SessionStart" | "UserPromptSubmit" | "Stop" | "Notification" | "PermissionRequest"
    )
}

/// One Claude event, as `AgentSessions::apply` read it from its envelope.
pub(super) struct Event<'a> {
    pub key: &'a str,
    /// The envelope, whose line has a tool call's whole input as the hook wrote it (`call_key`).
    pub envelope: &'a Envelope<'a>,
    /// Its payload.
    pub p: &'a Map<String, Value>,
    pub ev: &'a str,
    pub at: f64,
    /// Where the chat runs (`place`).
    pub place: Option<&'a str>,
    /// The hook's environment.
    pub env: Option<&'a Map<String, Value>>,
    /// The chat's title from its transcript, when it was read ("" for none).
    pub title: Option<&'a str>,
}

/// Places a Claude event; returns the chat's state, `removed`, `ignored` or `stale`.
pub(super) fn apply(chats: &mut Chats, e: &Event, now: f64) -> &'static str {
    let (p, ev, at) = (e.p, e.ev, e.at);
    let text = |k: &str| str_of(Some(p), k);
    chats.prune(now);
    if ev == "SessionEnd" {
        return chats.end(e.key, ev, at, now);
    }
    if !EVENTS.contains(&ev) {
        return "ignored";
    }

    // an event Claude started before the last one recorded arrived late: it's out of date
    if let Some(old) = chats.get_mut(e.key)
        && is_stale(old, e, now)
    {
        return "stale";
    }
    let (s, is_new) = chats.open(e.key);
    // a new chat (or one taken up again after it ended) starts pairing calls from this event
    if is_new {
        pair(s, e);
    }
    stamp(s, ev, at, now);
    let before = (s.state, s.detail.clone());
    s.agent = Some("claude");
    if let Some(cwd) = text("cwd").filter(|c| !c.is_empty()) {
        s.cwd = Some(cwd.into());
    }
    // where the chat runs (for its border colour, and so Stop only presses Escape in the desktop app for its chats)
    s.r#where = e.place.map(Into::into);
    // the desktop app's own id for the chat: its deep link claude://claude.ai/epitaxy/<id> opens it
    if let Some(host) = str_of(e.env, "CLAUDE_CODE_HOST_SESSION_ID").filter(|h| is_host_id(h)) {
        s.host_id = Some(host.into());
    }
    if (names_chat(ev) || !has_text(&s.chat_title))
        && let Some(title) = e.title.filter(|t| !t.is_empty())
    {
        s.chat_title = Some(title.into());
    }

    match ev {
        // also fires after a compaction in the middle of a turn: don't reset a busy chat
        "SessionStart" if is_new || matches!(s.state, Some("idle" | "done")) => set(s, "idle", "Ready", None),
        "SessionStart" => {}
        "UserPromptSubmit" => {
            let prompt = short(text("prompt").unwrap_or("").split('\n').next().unwrap_or(""), 38);
            if !prompt.is_empty() && !prompt.starts_with('/') {
                s.title = Some(prompt);
            }
            set(s, "thinking", "Thinking", None);
        }
        "PreToolUse" => {
            let (state, detail, prop) = describe(
                text("tool_name").unwrap_or(""),
                p.get("tool_input").and_then(Value::as_object),
            );
            set(s, state, detail, prop);
        }
        "PostToolUse" | "PostToolUseFailure" | "ElicitationResult" | "SubagentStop" => {
            set(s, "thinking", "Thinking", None)
        }
        "PermissionRequest" => set(s, "attention", "Needs your permission", None),
        // auto mode turned a tool call down; Claude carries on without it
        "PermissionDenied" => set(s, "thinking", "A tool call was denied", None),
        "Notification" => notification(
            s,
            text("notification_type").unwrap_or(""),
            text("message").unwrap_or(""),
        ),
        // an MCP server asks you to fill something in
        "Elicitation" => set(s, "attention", "Needs your input", None),
        "PreCompact" => set(s, "thinking", "Compacting the conversation", None),
        // /compact runs between turns (no Stop follows); an automatic one is part of a turn
        "PostCompact" if text("trigger") == Some("manual") => set(s, "done", "Compacted", None),
        "PostCompact" => set(s, "thinking", "Thinking", None),
        "SubagentStart" => set(
            s,
            "working",
            format!("Delegating to {}", helper(text("agent_type"))),
            Some("laptop"),
        ),
        "Stop" => set(s, "done", "Done", None),
        "StopFailure" => set(s, "attention", stop_error(text("error_type")), None),
        _ => {} // EVENTS has no other
    }
    // ts only moves when what the chat shows changes: an idle_prompt on a done chat mustn't show it as just done
    // again (or bring back a bubble the user closed). A ts well past now is from before the clock was set back.
    if is_new || (s.state, &s.detail) != (before.0, &before.1) || s.ts > now + 5.0 {
        s.ts = at;
    }
    s.state.unwrap_or_default()
}

pub(super) fn set(s: &mut Entry, state: &'static str, detail: impl Into<String>, prop: Option<&'static str>) {
    s.state = Some(state);
    s.detail = Some(detail.into());
    s.prop = prop;
}

/// A Notification, by its type; older Claude versions send none, and then the message decides.
fn notification(s: &mut Entry, kind: &str, message: &str) {
    match kind {
        "idle_prompt" if s.state != Some("done") => set(s, "idle", "Waiting for you", None),
        "permission_prompt" => set(s, "attention", "Needs your permission", None),
        "agent_needs_input" | "elicitation_dialog" | "elicitation_url_dialog" => {
            set(s, "attention", "Needs your input", None)
        }
        "agent_completed" => set(s, "done", "Done", None),
        // nothing to show (an idle_prompt never shows over done)
        "idle_prompt"
        | "auth_success"
        | "elicitation_complete"
        | "elicitation_response"
        | "quota_auto_resume_fired"
        | "quota_auto_resume_stale"
        | "quota_auto_resume_disabled" => {}
        _ if contains_ignoring_case(message, "waiting for your input") => {
            if s.state != Some("done") {
                set(s, "idle", "Waiting for you", None);
            }
        }
        _ if contains_ignoring_case(message, "permission") => set(s, "attention", "Needs your permission", None),
        _ => {
            let message = short(message, 40);
            let detail = if message.is_empty() {
                "Needs your attention".into()
            } else {
                message
            };
            set(s, "attention", detail, None);
        }
    }
}

/// What stopped the turn, from StopFailure's error_type.
fn stop_error(kind: Option<&str>) -> &'static str {
    match kind.unwrap_or("") {
        "rate_limit" => "Hit a rate limit",
        "overloaded" => "The service is overloaded",
        "server_error" => "Hit a server error",
        "authentication_failed" | "oauth_org_not_allowed" => "Needs you to sign in again",
        "billing_error" | "account_on_hold" => "Has a billing problem",
        "max_output_tokens" => "Hit the output limit",
        "model_not_found" => "Its model isn't available",
        "invalid_request" => "Hit a request error",
        "cloud_credential_error" => "Has a cloud credentials problem",
        _ => "Hit an error",
    }
}

/// `desktop` (the Claude app), `terminal` (the CLI), or the client's own name (e.g. claude-vscode, sdk-ts), from the
/// hook's CLAUDE_CODE_ENTRYPOINT.
pub(super) fn place(entrypoint: Option<&str>) -> Option<String> {
    match entrypoint? {
        "claude-desktop" => Some("desktop".into()),
        "cli" => Some("terminal".into()),
        "" => None,
        other => Some(other.into()),
    }
}

/// The Claude app's id for a chat, `local_<uuid>`, and nothing else: it goes into a deep link. The uuid is read as
/// .NET's `Guid.TryParseExact(…, "D")` reads it. Its 42 UTF-16 units are 42 bytes here: a character outside ASCII
/// fails either way.
fn is_host_id(id: &str) -> bool {
    id.len() == 42 && id.starts_with("local_") && is_guid_d(&id.as_bytes()[6..])
}

/// `Guid.TryParseExact(id, "D")` on 36 characters: hex digits in groups of 8-4-4-4-12. For compatibility .NET also
/// takes groups that begin with `+` and/or `0x` (their width counting them), except the last eight digits.
pub(super) fn is_guid_d(id: &[u8]) -> bool {
    if id.len() != 36 || [8, 13, 18, 23].iter().any(|&i| id[i] != b'-') {
        return false;
    }
    let hex = |group: &[u8]| group.iter().all(u8::is_ascii_hexdigit);
    let groups = [&id[..8], &id[9..13], &id[14..18], &id[19..23], &id[24..28]];
    if groups.iter().all(|g| hex(g)) && hex(&id[28..]) {
        return true;
    }
    let old_form = |mut group: &[u8]| {
        if let [b'+', rest @ ..] = group {
            group = rest;
        }
        if let [b'0', b'x' | b'X', rest @ ..] = group {
            group = rest;
        }
        hex(group)
    };
    id.iter().any(|c| matches!(c, b'x' | b'X' | b'+')) && groups.iter().all(|g| old_form(g)) && hex(&id[28..])
}

// ------------------------------------------------------------------ ordering
/// How far into a turn an event comes, to order events whose hooks started in the same millisecond. Only a rough
/// guide: a plugin install starts Claude's hooks through bash, sh and a wrapper, a few ms late at random (tens on Git
/// Bash), so the pair it matters most for, a call's PreToolUse and PermissionRequest, is paired instead. A wider tie
/// would make a quick next PreToolUse stale after a PostToolUse.
pub(super) fn rank(ev: &str) -> i32 {
    match ev {
        "SessionStart" => 0,
        "UserPromptSubmit" => 1,
        "PreToolUse" | "PreCompact" | "SubagentStart" => 2,
        "PostToolUse" | "PostToolUseFailure" | "PostCompact" | "SubagentStop" => 3,
        // PermissionRequest, PermissionDenied, Notification, Elicitation(Result), Stop, StopFailure, Interrupt,
        // SessionEnd
        _ => 4,
    }
}

/// True when an event the agent started at `at` is older than what the chat's entry already has, i.e. it finished
/// late: started earlier, or in the same tick but earlier in a turn, or before the chat ended. A recorded time well
/// past `now` means the clock was set back since: it can't order anything.
pub(super) fn stale(s: &Entry, ev: &str, at: f64, now: f64) -> bool {
    if let Some(ended) = s.ended {
        return ended <= now + 5.0 && at <= ended + 0.0005;
    }
    let last = s.dispatched;
    if last > now + 5.0 {
        return false;
    }
    at < last - 0.0005 || (at <= last + 0.0005 && rank(ev) < s.dispatched_rank)
}

/// Records the event as the chat's latest, for `stale`. One taken although it started before the latest (a
/// PermissionRequest paired with its PreToolUse, or one in the same tick) comes after it: the latest stays, so what
/// started between the two is still stale. Not when the clock was set back since the latest.
pub(super) fn stamp(s: &mut Entry, ev: &str, at: f64, now: f64) {
    if at < s.dispatched && s.dispatched <= now + 5.0 {
        s.dispatched_rank = s.dispatched_rank.max(rank(ev));
        return;
    }
    s.dispatched = at;
    s.dispatched_rank = rank(ev);
}

/// Claude's order: by `at` (`stale`), except for a call's PreToolUse and PermissionRequest (`pair`).
fn is_stale(s: &mut Entry, e: &Event, now: f64) -> bool {
    let paired = if s.ended.is_none() { pair(s, e) } else { None };
    paired.unwrap_or_else(|| stale(s, e.ev, e.at, now))
}

/// How far apart the hooks of one Claude call's PreToolUse and PermissionRequest can start, in seconds.
const PAIR_WINDOW: f64 = 1.0;

/// How many halves a chat keeps for pairing.
const ASKS: usize = 16;

/// A PreToolUse or PermissionRequest waiting for its other half.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Ask {
    what: u64,
    request: bool,
    at: f64,
    paired: bool,
}

/// Claude dispatches a call's PreToolUse and then its PermissionRequest a few ms apart at most, and their hooks start
/// a few ms late at random (see `rank`), so `at` can put them either way round. PermissionRequest has no tool_use_id,
/// so the two are paired by what the call runs (`call_key`), the closest within `PAIR_WINDOW`:
///  - a PreToolUse whose PermissionRequest is in already is stale (true);
///  - a PermissionRequest is taken (false) when its own PreToolUse is the chat's latest event: nothing else is newer.
///
/// Anything else goes by `at` (none). Each half pairs once, so the same command run again soon after isn't taken for
/// the earlier call. Recorded even when its time then finds it stale.
fn pair(s: &mut Entry, e: &Event) -> Option<bool> {
    let (ev, at) = (e.ev, e.at);
    let request = ev == "PermissionRequest";
    if !request && ev != "PreToolUse" {
        return None;
    }
    let what = call_key(e.p, e.envelope);
    let mut closest: Option<usize> = None;
    for (j, a) in s.asks.iter().enumerate() {
        if a.what == what
            && a.request != request
            && !a.paired
            && (a.at - at).abs() <= PAIR_WINDOW
            && closest.is_none_or(|i| (a.at - at).abs() < (s.asks[i].at - at).abs())
        {
            closest = Some(j);
        }
    }
    let Some(i) = closest else {
        s.asks.push(Ask {
            what,
            request,
            at,
            paired: false,
        });
        if s.asks.len() > ASKS {
            s.asks.remove(0);
        }
        return None;
    };
    s.asks[i].paired = true;
    if !request {
        return Some(true);
    }
    (s.asks[i].at == s.dispatched && s.dispatched_rank == rank("PreToolUse")).then_some(false)
}

/// What a tool call runs, as a hash (a command can be 256 Ki characters): its tool and command (PermissionRequest adds
/// a description to a shell command's input) or file, or else its whole input. The C#'s hash is seeded per process,
/// so only which calls it takes for the same must match, never the value.
///
/// The whole input is the text System.Text.Json writes for it (`ToJsonString`), which the C# hashes as it hashes a
/// command: its keys in the order the line has them and its numbers spelled as there (1.50 isn't 1.5), which the
/// fields lose, so it's written from the line (`like_stj`).
pub(super) fn call_key(p: &Map<String, Value>, envelope: &Envelope) -> u64 {
    let mut h = DefaultHasher::new();
    str_of(Some(p), "tool_name").hash(&mut h);
    let Some(input) = p.get("tool_input").filter(|v| !v.is_null()) else {
        h.write_u8(0);
        return h.finish();
    };
    h.write_u8(1);
    // the first of them that isn't missing or null, if it's text
    let command = input.as_object().and_then(|o| {
        ["command", "file_path", "notebook_path"]
            .iter()
            .find_map(|k| o.get(*k).filter(|v| !v.is_null()))
    });
    match command.and_then(Value::as_str) {
        Some(command) => h.write(command.as_bytes()),
        None => {
            // the line has it, unless a caller gave the fields a tool_input the line hasn't: then as serde_json
            // writes that
            let text = envelope
                .tool_input_text()
                .map_or_else(|| Cow::Owned(input.to_string()), Cow::Borrowed);
            like_stj(&text, &mut |bytes| h.write(bytes));
        }
    }
    h.finish()
}

/// Writes a JSON text as System.Text.Json's `ToJsonString` writes the node it parses: without whitespace; the members
/// in their order, duplicates too, and the numbers and literals as spelled, since it keeps their text; each string
/// decoded and escaped again (`escape_like_stj`). The text is valid JSON: serde_json has read it.
fn like_stj(json: &str, out: &mut impl FnMut(&[u8])) {
    let bytes = json.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'"' => {
                let start = i;
                i += 1;
                while bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
                let body = &json[start + 1..i - 1];
                let text: Cow<str> = if body.contains('\\') {
                    Cow::Owned(serde_json::from_str(&json[start..i]).expect("serde_json has read the string"))
                } else {
                    Cow::Borrowed(body)
                };
                escape_like_stj(&text, out);
            }
            // punctuation, numbers and literals, as they stand
            _ => {
                let start = i;
                while i < bytes.len() && !matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r' | b'"') {
                    i += 1;
                }
                out(&bytes[start..i]);
            }
        }
    }
}

/// A string as System.Text.Json's writer escapes it with its default encoder: printable ASCII as it is, but for
/// `" & ' + < > \` and the backtick; \b \t \n \f \r and \\ short; the rest (control characters, DEL, everything past
/// ASCII) as \uXXXX in upper case, a surrogate pair past U+FFFF.
fn escape_like_stj(text: &str, out: &mut impl FnMut(&[u8])) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out(b"\"");
    let mut plain = 0;
    for (i, c) in text.char_indices() {
        if matches!(c, ' '..='~') && !matches!(c, '"' | '&' | '\'' | '+' | '<' | '>' | '\\' | '`') {
            continue;
        }
        out(&text.as_bytes()[plain..i]);
        plain = i + c.len_utf8();
        match c {
            '\u{8}' => out(b"\\b"),
            '\t' => out(b"\\t"),
            '\n' => out(b"\\n"),
            '\u{c}' => out(b"\\f"),
            '\r' => out(b"\\r"),
            '\\' => out(b"\\\\"),
            _ => {
                for unit in c.encode_utf16(&mut [0; 2]) {
                    let digit = |shift: u16| HEX[usize::from((*unit >> shift) & 0xF)];
                    out(&[b'\\', b'u', digit(12), digit(8), digit(4), digit(0)]);
                }
            }
        }
    }
    out(&text.as_bytes()[plain..]);
    out(b"\"");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The call key of a payload, sent in a line of its own as written.
    fn key(payload: &str) -> u64 {
        let line = format!(r#"{{"payload":{payload}}}"#);
        let envelope = Envelope::parse(&line).expect("a JSON object");
        let p = envelope.fields["payload"].as_object().expect("an object payload");
        call_key(p, &envelope)
    }

    fn stj(json: &str) -> String {
        let mut text = Vec::new();
        like_stj(json, &mut |bytes| text.extend_from_slice(bytes));
        String::from_utf8(text).unwrap()
    }

    /// The hash is the same whether the text came as a command or as the JSON of a whole input, as the C#'s string
    /// is; and the input is written in pieces, which mustn't matter.
    #[test]
    fn a_command_and_a_whole_input_with_the_same_text_are_the_same_call() {
        let input = r#"{"x":1,"y":[true,null,"a"]}"#;
        let command = serde_json::to_string(input).unwrap();
        assert_eq!(
            key(&format!(r#"{{"tool_name":"Bash","tool_input":{input}}}"#)),
            key(&format!(
                r#"{{"tool_name":"Bash","tool_input":{{"command":{command}}}}}"#
            ))
        );
        assert_ne!(
            key(&format!(r#"{{"tool_name":"Bash","tool_input":{input}}}"#)),
            key(&format!(r#"{{"tool_name":"Grep","tool_input":{input}}}"#))
        );
        assert_ne!(
            key(r#"{"tool_name":"Bash"}"#),
            key(r#"{"tool_name":"Bash","tool_input":{}}"#)
        );
        assert_eq!(
            key(r#"{"tool_name":"Bash"}"#),
            key(r#"{"tool_name":"Bash","tool_input":null}"#)
        );
        assert_ne!(
            key(r#"{"tool_input":{"command":"ls"}}"#),
            key(r#"{"tool_name":"","tool_input":{"command":"ls"}}"#)
        );
    }

    /// Keys in another order and numbers spelled otherwise make other calls, as in the C#; how the line spaces or
    /// escapes them doesn't.
    #[test]
    fn a_whole_input_is_the_call_as_the_line_writes_it() {
        let call = |input: &str| key(&format!(r#"{{"tool_name":"WebFetch","tool_input":{input}}}"#));
        assert_ne!(call(r#"{"url":"x","prompt":"y"}"#), call(r#"{"prompt":"y","url":"x"}"#));
        for (a, b) in [
            ("1.50", "1.5"),
            ("1e2", "100"),
            ("1E2", "1e2"),
            ("1e+2", "1e2"),
            ("-0", "0"),
        ] {
            assert_ne!(
                call(&format!(r#"{{"n":{a}}}"#)),
                call(&format!(r#"{{"n":{b}}}"#)),
                "{a} and {b}"
            );
        }
        assert_eq!(
            call(r#"{"url":"x","prompt":"y"}"#),
            call("{ \"url\" :\t\"\\u0078\",\r\n\"pro\\u006dpt\": \"y\" }")
        );
        assert_eq!(call(r#"{"s":"\u00e9\ud83d\ude00"}"#), call(r#"{"s":"é😀"}"#));
        // the last of a key, as the fields have it
        assert_eq!(
            key(r#"{"tool_name":"WebFetch","tool_input":{"a":1},"tool_input":{"b":2}}"#),
            call(r#"{"b":2}"#)
        );
    }

    #[test]
    fn a_whole_input_is_written_as_system_text_json_writes_it() {
        assert_eq!(
            stj(r#" { "b" : [ 1.50 , -0, 1E+2, true, false, null ], "a" : { } , "a" : "" } "#),
            r#"{"b":[1.50,-0,1E+2,true,false,null],"a":{},"a":""}"#
        );
        assert_eq!(
            stj(r#""<é&'+`\"\\\/\n\t\b\f\r\u0001\u001f\u007f\u0080\u2028\ufffd😀 ~!#$%()*,-.:;=?@[]^_{|}""#),
            concat!(
                r#""\u003C\u00E9\u0026\u0027\u002B\u0060\u0022\\/\n\t\b\f\r\u0001\u001F\u007F\u0080\u2028\uFFFD"#,
                r#"\uD83D\uDE00 ~!#$%()*,-.:;=?@[]^_{|}""#
            )
        );
        assert_eq!(stj(r#""plain text""#), r#""plain text""#);
    }
}
