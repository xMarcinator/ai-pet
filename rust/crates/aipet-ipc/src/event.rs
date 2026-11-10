//! An event's envelope, as the hook and the pet's HTTP listener both make it (`ClaudeHook.cs`, `CodexHook.cs` and
//! `Program.cs:139-187`): the event mended, trimmed to what the pet needs, wrapped, and fitted under the pet's limit.
//!
//! - [`mended`]: each escaped lone surrogate in the event's text as U+FFFD, before it is parsed.
//! - [`trimmed`]: the event without `tool_response`, and with every string cut to [`MAX_STRING`].
//! - [`envelope`]: `v`, `type`, `agent`, `at`, `pid` (the hook's only), `env` (Claude's only) and the payload.
//! - [`fit`]: the envelope as one line; over the limit, `tool_input` keeps only what the pet reads of it.

use std::borrow::Cow;
use std::time::SystemTime;

use crate::json::{Node, Object};
use crate::protocol::{
    AGENT, AT, ENV, KIND, MAX_STRING, PAYLOAD, PID, TOOL_INPUT_KEYS, V, VERSION, format_seconds, unix_time,
};

/// How deep an event may nest: System.Text.Json's default depth, which the C# hook reads stdin with. `MAX_DEPTH`
/// (128) leaves the envelope a level more.
///
/// [`MAX_DEPTH`]: crate::protocol::MAX_DEPTH
pub const READ_DEPTH: usize = 64;

/// `Mended`: the JSON with each escaped lone surrogate as `�`. Node's `JSON.stringify` (so Claude's) writes one
/// for text cut in the middle of an emoji. System.Text.Json parses such an escape but then can neither read nor
/// write back the string that holds it, which would lose the whole event; serde_json refuses it outright. The text
/// itself is decoded, so it has no lone surrogate of its own.
///
/// Every character it looks at is ASCII, so it works on bytes where the C# works on UTF-16 units.
pub fn mended(json: &str) -> Cow<'_, str> {
    let b = json.as_bytes();
    // the UTF-16 unit of the 4 hex digits at `at`, when there are 4
    let hex = |at: usize| -> Option<u16> {
        let digits = b.get(at..at + 4)?;
        digits
            .iter()
            .try_fold(0u16, |unit, &d| Some(unit << 4 | (d as char).to_digit(16)? as u16))
    };
    let next = |from: usize| b[from..].iter().position(|&c| c == b'\\').map(|i| from + i);
    let mut mended: Option<String> = None;
    let mut done = 0;
    let mut at = next(0);
    while let Some(i) = at.filter(|&i| i + 1 < b.len()) {
        // a backslash is only valid in a string, where it starts an escape: two characters, or six for \uXXXX
        let len = match (b[i + 1], hex(i + 2)) {
            (b'u', Some(c)) if !(0xD800..=0xDFFF).contains(&c) => 6,
            (b'u', Some(c))
                if c <= 0xDBFF
                    && i + 11 < b.len()
                    && b[i + 6] == b'\\'
                    && b[i + 7] == b'u'
                    && hex(i + 8).is_some_and(|low| (0xDC00..=0xDFFF).contains(&low)) =>
            {
                12
            }
            (b'u', Some(_)) => {
                let out = mended.get_or_insert_with(|| String::with_capacity(json.len()));
                out.push_str(&json[done..i]);
                out.push_str("\\uFFFD");
                done = i + 6;
                6
            }
            _ => 2,
        };
        at = if i + len < b.len() { next(i + len) } else { None };
    }
    match mended {
        Some(mut out) => {
            out.push_str(&json[done..]);
            Cow::Owned(out)
        }
        None => Cow::Borrowed(json),
    }
}

/// `Trimmed`: the event as the pet needs it, without the tool's output (often large, and never used), and with
/// every string cut to [`MAX_STRING`] so the request usually stays well within the pet's limit.
///
/// It fails where the C# throws: on an object that names a member twice, anywhere but in `tool_response` (dropped
/// unread).
pub fn trimmed(mut payload: Object) -> Result<Object, String> {
    payload.remove("tool_response");
    cap_object(&mut payload)?;
    Ok(payload)
}

/// Why an event can't be wrapped: it names a member twice.
pub fn twice(key: &str) -> String {
    format!("the event names the member {key:?} twice")
}

/// `Cap`, which reads every object and array of the event.
fn cap(node: &mut Node) -> Result<(), String> {
    match node {
        Node::String(s) => cut(s),
        Node::Array(items) => items.iter_mut().try_for_each(cap)?,
        Node::Object(object) => cap_object(object)?,
        Node::Null | Node::Bool(_) | Node::Number(_) => {}
    }
    Ok(())
}

fn cap_object(object: &mut Object) -> Result<(), String> {
    if let Some(key) = object.duplicate() {
        return Err(twice(key));
    }
    object.values_mut().try_for_each(cap)
}

/// `Cut`: a string longer than [`MAX_STRING`] UTF-16 units, cut to it (not in the middle of a surrogate pair).
fn cut(s: &mut String) {
    // a string's UTF-16 units are never more than its UTF-8 bytes
    if s.len() <= MAX_STRING {
        return;
    }
    let mut units = 0;
    for (at, c) in s.char_indices() {
        units += c.len_utf16();
        if units > MAX_STRING {
            s.truncate(at);
            return;
        }
    }
}

/// The envelope (v1) around a trimmed event, its fields in the C#'s order. `at` is when the event started: the
/// hook's own start, or when the listener got it. `pid` is the hook's (the listener has none to give), and `env` is
/// Claude's only.
pub fn envelope(agent: &str, at: SystemTime, pid: Option<u32>, env: Option<Object>, payload: Object) -> Object {
    let mut envelope = Object::default();
    envelope.push(V, Node::Number(VERSION.to_string()));
    envelope.push(KIND, Node::String("event".to_owned()));
    envelope.push(AGENT, Node::String(agent.to_owned()));
    envelope.push(AT, Node::Number(format_seconds(unix_time(at))));
    if let Some(pid) = pid {
        envelope.push(PID, Node::Number(pid.to_string()));
    }
    if let Some(env) = env {
        envelope.push(ENV, Node::Object(env));
    }
    envelope.push(PAYLOAD, Node::Object(payload));
    envelope
}

/// `Request`: the envelope as one line. Cut strings can still add up past the pet's limit (a MultiEdit of several
/// big files, say); then the tool call keeps only what the pet reads of it, rather than being dropped whole. `limit`
/// counts the bytes this writes.
pub fn fit(envelope: &mut Object, limit: usize) -> String {
    let line = envelope.to_json();
    if line.len() <= limit {
        return line;
    }
    let Some(Node::Object(payload)) = envelope.get_mut(PAYLOAD) else {
        return line;
    };
    let Some(Node::Object(input)) = payload.get_mut("tool_input") else {
        return line;
    };
    input.retain(|key| TOOL_INPUT_KEYS.contains(&key));
    envelope.to_json()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    fn object(text: &str) -> Object {
        match json::parse(text, 64).unwrap() {
            Node::Object(o) => o,
            _ => panic!("not an object: {text}"),
        }
    }

    /// Every string cut to MAX_STRING UTF-16 units, wherever it is, and never between the halves of a pair; names
    /// aren't cut, and only the event's own `tool_response` is dropped.
    #[test]
    fn strings_are_capped() {
        let n = MAX_STRING;
        let x = |len: usize| "x".repeat(len);
        let mut payload = Object::default();
        payload.push("exact", Node::String(x(n)));
        payload.push("over", Node::String(x(n + 1)));
        payload.push("pair", Node::String(format!("{}😀", x(n - 1))));
        payload.push("wide", Node::String("é".repeat(n + 1)));
        payload.push(
            x(n + 1),
            Node::Array(vec![
                Node::String(x(n + 5)),
                Node::Object(object(r#"{"tool_response":1}"#)),
            ]),
        );
        payload.push("tool_response", Node::String("gone".into()));
        let payload = trimmed(payload).unwrap();
        assert_eq!(payload.str("exact"), Some(x(n).as_str()));
        assert_eq!(payload.str("over"), Some(x(n).as_str()));
        assert_eq!(payload.str("pair"), Some(x(n - 1).as_str()));
        assert_eq!(payload.str("wide"), Some("é".repeat(n).as_str()));
        let Some(Node::Array(items)) = payload.get(&x(n + 1)) else {
            panic!("the long name was cut")
        };
        assert_eq!(items[0], Node::String(x(n)));
        assert_eq!(items[1], Node::Object(object(r#"{"tool_response":1}"#)));
        assert_eq!(payload.get("tool_response"), None);
    }

    /// Without a pid and an env (the listener's), the envelope has neither member.
    #[test]
    fn an_envelope_has_a_pid_and_an_env_only_when_given() {
        let at = SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1_758_800_000_123);
        let payload = || object(r#"{"hook_event_name":"Stop"}"#);
        assert_eq!(
            envelope("claude", at, None, None, payload()).to_json(),
            r#"{"v":1,"type":"event","agent":"claude","at":1758800000.123,"payload":{"hook_event_name":"Stop"}}"#
        );
        assert_eq!(
            envelope("claude", at, Some(7), Some(Object::default()), payload()).to_json(),
            r#"{"v":1,"type":"event","agent":"claude","at":1758800000.123,"pid":7,"env":{},"payload":{"hook_event_name":"Stop"}}"#
        );
    }
}
