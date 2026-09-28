//! The event path: stdin read within `STDIN`, the Claude and Codex envelopes, the request line and its size limit, and
//! the one request to the pet (`src/AiPet.Hook/Program.cs:23-191`, `ClaudeHook.cs`, `CodexHook.cs`).
//!
//! One run:
//! - stdin, within [`STDIN`]: nothing in time, and the hook ends having sent and written nothing. Escaped lone
//!   surrogates become U+FFFD ([`mended`]); what isn't a JSON object's text becomes `{}`, and once the pet is reached
//!   the trace log says why.
//! - the envelope: `v`, `type`, `agent`, `at`, `pid`, `env` (Claude's only) and the payload, which is the event
//!   without `tool_response` and with every string cut to [`MAX_STRING`] ([`trimmed`]).
//! - the request line. Over [`MAX_REQUEST`] less the room `sent` needs, `tool_input` keeps only what the pet reads of
//!   it ([`request`]).
//! - the pet, waited for with what the run's budget has left once the reply's [`TIMEOUT`] is set aside. Not there:
//!   the hook ends having written nothing anywhere. There but busy: one trace line.
//! - the request with `sent`, and the reply, which says the pet has the event. Anything else is traced.
//!
//! Nothing is written before the pet is reached, and nothing is ever printed: `main` runs this so that not even a
//! panic reaches stdout or stderr.

use std::borrow::Cow;
use std::ffi::OsString;
use std::io::Read;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime};

use aipet_ipc::connect::{NoPet, connect};
use aipet_ipc::protocol::{
    AGENT, AT, CLAUDE_ENV, CONNECT, ENV, HOOK_BUDGET, KIND, MAX_REQUEST, MAX_STRING, OK, PAYLOAD, PID, SENT, STDIN,
    TIMEOUT, TOOL_INPUT_KEYS, V, VERSION, format_seconds, unix_time,
};

use crate::json::{self, Node, Object};
use crate::trace::trace;

/// When `main` started, before stdin was read: the event's `at` (the hook looks at no process, its own or another),
/// and where the run's time budget starts. For a hook registered directly that is about when the agent started it;
/// the plugin's goes through bash, sh and the launcher first.
pub(crate) struct Launched {
    at: SystemTime,
    started: Instant,
}

impl Launched {
    pub(crate) fn now() -> Launched {
        Launched {
            at: SystemTime::now(),
            started: Instant::now(),
        }
    }
}

/// How deep stdin may nest: System.Text.Json's default depth, which the C# hook reads it with. `MAX_DEPTH` (128)
/// leaves the envelope a level more.
const READ_DEPTH: usize = 64;

/// What the request line leaves under [`MAX_REQUEST`] for `sent`, which is added once the pet is reached
/// (`,"sent":1758800000.123` is 22 bytes).
const SENT_ROOM: usize = 32;

/// `Program.Main` from its event path on: `aipet-hook [--agent claude|codex]`.
pub(crate) fn run(launched: &Launched, args: &[OsString]) {
    let agent = arg_value(args, "--agent").unwrap_or_else(|| "claude".to_owned());
    // no event came in time: the pet would ignore an empty one anyway
    let Some(read) = read_payload(STDIN) else { return };
    let (payload, unreadable) = match read {
        Ok(payload) => (payload, None),
        Err(why) => (Object::default(), Some(why)),
    };
    // the request is made before the pet is reached, so a failure never takes an instance for nothing. It's only
    // traced once the pet is reached, like everything else.
    let (last_event, envelope) = envelope(agent == "codex", payload, launched.at, &|name| std::env::var_os(name));
    let request = envelope.map(|mut envelope| request(&mut envelope));

    // nothing is written before this: with the pet closed the hook leaves no trace. A pet whose instances are all
    // taken is waited for with what the run's budget has left once the reply's time is set aside.
    let busy = busy_wait(launched.started.elapsed());
    let mut pet = match connect(Duration::from_millis(busy.max(0) as u64)) {
        Ok(pet) => pet,
        // the pet runs (its pipe is there), so this may be written: the one trace of an event it never got
        Err(NoPet::Busy) => {
            trace(&format!(
                "{agent} {last_event}: the pet's pipe stayed busy for {} ms, so the event is lost",
                busy.max(0)
            ));
            return;
        }
        Err(NoPet::Closed) => return,
    };
    if let Some(why) = unreadable {
        trace(&format!(
            "{agent}: the event isn't JSON, so the pet gets an empty one: {why}"
        ));
    }
    let request = match request {
        Ok(request) => request,
        Err(why) => {
            trace(&format!("{agent} {last_event}: {why}"));
            return;
        }
    };
    // the reply means the pet has the event
    match pet.ask(&sent(&request, SystemTime::now())) {
        Ok(reply) => match taken(reply.as_deref()) {
            Ok(true) => {}
            Ok(false) => trace(&format!(
                "{agent} {last_event}: the pet didn't take it: {}",
                reply.as_deref().unwrap_or("no reply")
            )),
            Err(why) => trace(&format!("{agent} {last_event}: the reply isn't JSON: {why}")),
        },
        Err(e) => trace(&format!("{agent} {last_event}: {e}")),
    }
}

/// `ArgValue`: the argument after `name`, in lower case.
fn arg_value(args: &[OsString], name: &str) -> Option<String> {
    let at = args.iter().position(|a| a == name)?;
    args.get(at + 1).map(|v| v.to_string_lossy().to_lowercase())
}

/// How long to wait for a free instance of the pet's pipe, in ms, `elapsed` into the run: what the budget has left
/// once the reply's time is set aside, and at most [`CONNECT`]. Below zero when stdin took long.
fn busy_wait(elapsed: Duration) -> i64 {
    let ms = |d: Duration| d.as_millis() as i64;
    ms(CONNECT).min(ms(HOOK_BUDGET) - ms(TIMEOUT) - ms(elapsed))
}

/// `ReadPayload`: the event on stdin, or `None` when none came within `wait` (a hook can never hang, even if stdin
/// stays open). `Err` says why what came isn't a JSON object's text.
///
/// Stdin is read on a thread of its own, which is left blocked when the wait is up: the hook's exit ends it.
fn read_payload(wait: Duration) -> Option<Result<Object, String>> {
    let (sent, got) = mpsc::sync_channel(1);
    let reader = std::thread::Builder::new().name("aipet-stdin".into()).spawn(move || {
        let mut input = Vec::new();
        let read = std::io::stdin().lock().read_to_end(&mut input).map(|_| input);
        let _ = sent.send(read);
    });
    if let Err(e) = reader {
        return Some(Err(format!("stdin can't be read: {e}")));
    }
    match got.recv_timeout(wait) {
        Ok(Ok(input)) => Some(payload_of(&input)),
        Ok(Err(e)) => Some(Err(format!("stdin can't be read: {e}"))),
        Err(RecvTimeoutError::Disconnected) => Some(Err("stdin can't be read".to_owned())),
        Err(RecvTimeoutError::Timeout) => None,
    }
}

/// The event in stdin's bytes: empty or blank input is `{}`, and so is JSON that isn't an object.
fn payload_of(input: &[u8]) -> Result<Object, String> {
    let text = decoded(input);
    if text.trim().is_empty() {
        return Ok(Object::default());
    }
    match json::parse(&mended(&text), READ_DEPTH) {
        Ok(Node::Object(payload)) => Ok(payload),
        Ok(_) => Ok(Object::default()),
        Err(e) => Err(e.to_string()),
    }
}

/// Stdin's text as .NET's `StreamReader(stdin, Encoding.UTF8)` reads it: UTF-8 unless a byte order mark says
/// otherwise (UTF-16 or UTF-32, either byte order), without the mark, and with each invalid sequence as U+FFFD.
fn decoded(input: &[u8]) -> String {
    /// The units of `N` bytes; one cut short at the end is U+FFFD.
    fn units<const N: usize>(bytes: &[u8], unit: fn([u8; N]) -> u32) -> impl Iterator<Item = u32> + '_ {
        let chunks = bytes.chunks_exact(N);
        let rest = (!chunks.remainder().is_empty()).then_some(0xFFFD);
        chunks
            .map(move |c| unit(c.try_into().expect("chunks of N bytes")))
            .chain(rest)
    }
    fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u32) -> String {
        char::decode_utf16(units(bytes, unit).map(|u| u as u16))
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect()
    }
    fn utf32(bytes: &[u8], unit: fn([u8; 4]) -> u32) -> String {
        units(bytes, unit)
            .map(|u| char::from_u32(u).unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect()
    }
    match input {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, |u| u32::from(u16::from_be_bytes(u))),
        [0xFF, 0xFE, 0, 0, rest @ ..] => utf32(rest, u32::from_le_bytes),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, |u| u32::from(u16::from_le_bytes(u))),
        [0, 0, 0xFE, 0xFF, rest @ ..] => utf32(rest, u32::from_be_bytes),
        _ => String::from_utf8_lossy(input).into_owned(),
    }
}

/// `Mended`: the JSON with each escaped lone surrogate as `\uFFFD`. Node's `JSON.stringify` (so Claude's) writes one
/// for text cut in the middle of an emoji. System.Text.Json parses such an escape but then can neither read nor
/// write back the string that holds it, which would lose the whole event; serde_json refuses it outright. The text
/// itself is decoded, so it has no lone surrogate of its own.
///
/// Every character it looks at is ASCII, so it works on bytes where the C# works on UTF-16 units.
fn mended(json: &str) -> Cow<'_, str> {
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

/// `ClaudeHook.Envelope` or `CodexHook.Envelope`, and the event's name for the trace log (`lastEvent`). As in the
/// C#, it is made before the pet is reached, and a failure is traced only after.
///
/// It fails where the C# throws: on an object that names a member twice, anywhere but in `tool_response` (dropped
/// unread). The name is `?` when that object is the event itself, which fails before its name is read.
fn envelope(
    codex: bool,
    payload: Object,
    at: SystemTime,
    vars: &dyn Fn(&str) -> Option<OsString>,
) -> (String, Result<Object, String>) {
    if let Some(key) = payload.duplicate() {
        return ("?".to_owned(), Err(twice(key)));
    }
    let last_event = payload.str("hook_event_name").unwrap_or("").to_owned();
    let mut envelope = Object::default();
    envelope.push(V, Node::Number(VERSION.to_string()));
    envelope.push(KIND, Node::String("event".to_owned()));
    envelope.push(AGENT, Node::String(if codex { "codex" } else { "claude" }.to_owned()));
    envelope.push(AT, Node::Number(format_seconds(unix_time(at))));
    envelope.push(PID, Node::Number(std::process::id().to_string()));
    if !codex {
        // where the chat runs, and the desktop app's own id for it, are only in the hook's environment
        let mut env = Object::default();
        for name in CLAUDE_ENV {
            if let Some(value) = vars(name) {
                env.push(name, Node::String(value.to_string_lossy().into_owned()));
            }
        }
        envelope.push(ENV, Node::Object(env));
    }
    let envelope = trimmed(payload).map(|payload| {
        envelope.push(PAYLOAD, Node::Object(payload));
        envelope
    });
    (last_event, envelope)
}

fn twice(key: &str) -> String {
    format!("the event names the member {key:?} twice")
}

/// `Trimmed`: the event as the pet needs it, without the tool's output (often large, and never used), and with
/// every string cut to [`MAX_STRING`] so the request usually stays well within the pet's limit.
fn trimmed(mut payload: Object) -> Result<Object, String> {
    payload.remove("tool_response");
    cap_object(&mut payload)?;
    Ok(payload)
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

/// `Request`: the envelope as the request line. Cut strings can still add up past the pet's limit (a MultiEdit of
/// several big files, say); then the tool call keeps only what the pet reads of it, rather than being dropped whole.
/// The limit leaves room for `sent`, and counts the bytes this writes.
fn request(envelope: &mut Object) -> String {
    let line = envelope.to_json();
    if line.len() <= MAX_REQUEST - SENT_ROOM {
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

/// `Sent`: the request line with `sent`, when it is sent. The line is made before the pet is reached, and `sent` is
/// when the pet got the event.
fn sent(request: &str, now: SystemTime) -> String {
    format!(
        "{},\"{SENT}\":{}}}",
        &request[..request.len() - 1],
        format_seconds(unix_time(now))
    )
}

/// Whether the reply says the pet has the event: `"ok":true`. `Err` when it isn't JSON.
fn taken(reply: Option<&str>) -> Result<bool, json::Error> {
    let Some(reply) = reply else { return Ok(false) };
    Ok(match json::parse(reply, READ_DEPTH)? {
        Node::Object(r) => r.get(OK) == Some(&Node::Bool(true)),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(text: &str) -> Object {
        match json::parse(text, READ_DEPTH).unwrap() {
            Node::Object(o) => o,
            _ => panic!("not an object: {text}"),
        }
    }

    fn no_vars(_: &str) -> Option<OsString> {
        None
    }

    fn time(ms: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_millis(ms)
    }

    /// The C#'s own cases (HookContractTests.cs, `LoneSurrogates_AreMended_AndTheRestKept`): each escaped lone
    /// surrogate as U+FFFD, and everything around it as it was.
    #[test]
    fn lone_surrogates_are_mended_and_the_rest_kept() {
        let big = "y".repeat(MAX_STRING - 1);
        let stdin = format!(
            r#"{{"hook_event_name":"UserPromptSubmit","session_id":"s","a":"x\ud800y","b":"\uDC00","c":"\ud83d\ude00","d":"\\ud800","e":"\ud800\ud83d\ude00","f":"\ud83d\ud83d","g":"end\ud83d","k\udfff":1,"h":"q\"\n\u0041","big":"{big}\ud800tail"}}"#
        );
        let p = payload_of(stdin.as_bytes()).unwrap();
        for (key, expected) in [
            ("a", "x\u{fffd}y"),
            ("b", "\u{fffd}"),
            ("c", "\u{1F600}"),
            ("d", "\\ud800"),
            ("e", "\u{fffd}\u{1F600}"),
            ("f", "\u{fffd}\u{fffd}"),
            ("g", "end\u{fffd}"),
            ("h", "q\"\nA"),
        ] {
            assert_eq!(p.str(key), Some(expected), "{key}");
        }
        assert_eq!(p.get("k\u{fffd}"), Some(&Node::Number("1".into())));
        // mended before it is cut
        let (_, envelope) = envelope(false, p, time(0), &no_vars);
        let Some(Node::Object(p)) = envelope.unwrap().remove(PAYLOAD) else {
            panic!()
        };
        assert_eq!(p.str("big"), Some(format!("{big}\u{fffd}").as_str()));
        // the text is left alone when there is nothing to mend; a lone half at the very end is mended too
        assert!(matches!(mended(r#"{"a":"\ud83d\ude00 \\ud800 \n"}"#), Cow::Borrowed(_)));
        assert_eq!(mended(r#""\ud800"#), r#""\uFFFD"#);
        assert_eq!(mended(r#""\ud80"#), r#""\ud80"#);
    }

    /// Stdin as the C# reads it (each case also run against the C# hook, see the task's notes). `Err` means the pet
    /// gets `{}` and the trace log says why.
    #[test]
    fn stdin_is_read_as_the_csharp_reads_it() {
        let deep = |n: usize| format!("{{\"x\":{}{}}}", "[".repeat(n - 1), "]".repeat(n - 1));
        let text = "{\"u\":\"é😀\"}";
        let utf16 = |be: bool| -> Vec<u8> {
            let mark: [u8; 2] = if be { [0xFE, 0xFF] } else { [0xFF, 0xFE] };
            let units = text
                .encode_utf16()
                .flat_map(|u| if be { u.to_be_bytes() } else { u.to_le_bytes() });
            mark.into_iter().chain(units).collect()
        };
        let utf32 = |be: bool| -> Vec<u8> {
            let mark: [u8; 4] = if be { [0, 0, 0xFE, 0xFF] } else { [0xFF, 0xFE, 0, 0] };
            let units = text.chars().flat_map(|c| {
                if be {
                    (c as u32).to_be_bytes()
                } else {
                    (c as u32).to_le_bytes()
                }
            });
            mark.into_iter().chain(units).collect()
        };
        let deep64 = format!("{{\"x\":{}{}}}", "[".repeat(63), "]".repeat(63));
        for (input, read) in [
            (b"".to_vec(), Ok("{}")),
            (b" \n\t ".to_vec(), Ok("{}")),
            ("\u{a0}\u{2028}".as_bytes().to_vec(), Ok("{}")),
            (b"[1,2]".to_vec(), Ok("{}")),
            (b"\"x\"".to_vec(), Ok("{}")),
            (b"\xEF\xBB\xBF{\"a\":1}".to_vec(), Ok(r#"{"a":1}"#)),
            (utf16(false), Ok(text)),
            (utf16(true), Ok(text)),
            (utf32(false), Ok(text)),
            (utf32(true), Ok(text)),
            // each invalid sequence as U+FFFD, as .NET's decoder does: ED A0 80 is three
            (
                b"{\"b\":\"a\xED\xA0\x80b\xFFc\"}".to_vec(),
                Ok("{\"b\":\"a\u{fffd}\u{fffd}\u{fffd}b\u{fffd}c\"}"),
            ),
            (b"{not json".to_vec(), Err(())),
            (b"{\"a\":1} x".to_vec(), Err(())),
            (deep(64).into_bytes(), Ok(deep64.as_str())),
            (deep(65).into_bytes(), Err(())),
        ] {
            let got = payload_of(&input).map(|p| p.to_json()).map_err(|_| ());
            assert_eq!(
                got.as_deref().map_err(|_| ()),
                read,
                "{:?}",
                String::from_utf8_lossy(&input)
            );
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

    /// A member named twice fails the envelope, as it throws in the C#: before the event's name is read when it is
    /// the event's own, after it when it is deeper. Inside `tool_response`, which is dropped unread, it doesn't.
    #[test]
    fn a_member_named_twice_fails_the_envelope() {
        for (event, name, fails) in [
            (r#"{"hook_event_name":"Stop","a":1,"a":2}"#, "?", true),
            (r#"{"hook_event_name":"Stop","x":[{"a":1,"a":2}]}"#, "Stop", true),
            (
                r#"{"hook_event_name":"Stop","tool_response":{"a":1,"a":2}}"#,
                "Stop",
                false,
            ),
            (r#"{"hook_event_name":7}"#, "", false),
        ] {
            let (last_event, envelope) = envelope(true, object(event), time(0), &no_vars);
            assert_eq!((last_event.as_str(), envelope.is_err()), (name, fails), "{event}");
        }
    }

    /// The envelope's fields in the C#'s order. Claude's carries the variables that are set (empty ones too);
    /// Codex's has no `env`.
    #[test]
    fn claude_forwards_its_environment() {
        let event = object(r#"{"hook_event_name":"UserPromptSubmit","prompt":"hi"}"#);
        let line = |codex: bool, vars: &[(&str, &str)]| {
            let vars: Vec<(String, OsString)> = vars.iter().map(|(k, v)| (k.to_string(), v.into())).collect();
            let lookup = move |name: &str| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
            let (_, envelope) = envelope(codex, event.clone(), time(1_758_800_000_123), &lookup);
            envelope.unwrap().to_json()
        };
        let pid = std::process::id();
        let head = |agent: &str| format!(r#"{{"v":1,"type":"event","agent":"{agent}","at":1758800000.123,"pid":{pid}"#);
        let payload = r#""payload":{"hook_event_name":"UserPromptSubmit","prompt":"hi"}}"#;
        let claude = |env: &str| format!(r#"{},"env":{env},{payload}"#, head("claude"));
        assert_eq!(
            line(
                false,
                &[
                    ("CLAUDE_CODE_ENTRYPOINT", "claude-desktop"),
                    ("CLAUDE_CODE_HOST_SESSION_ID", "local_1"),
                    ("OTHER", "x"),
                ]
            ),
            claude(r#"{"CLAUDE_CODE_ENTRYPOINT":"claude-desktop","CLAUDE_CODE_HOST_SESSION_ID":"local_1"}"#)
        );
        assert_eq!(
            line(false, &[("CLAUDE_CODE_HOST_SESSION_ID", "")]),
            claude(r#"{"CLAUDE_CODE_HOST_SESSION_ID":""}"#)
        );
        assert_eq!(line(false, &[]), claude("{}"));
        assert_eq!(
            line(true, &[("CLAUDE_CODE_ENTRYPOINT", "cli")]),
            format!("{},{payload}", head("codex"))
        );
    }

    /// A request line up to the limit less `sent`'s room is sent as it is; one byte more and `tool_input` keeps only
    /// the fields the pet reads, in their order. Without a `tool_input` object the line stays as it is.
    #[test]
    fn tool_input_is_trimmed_at_the_limit() {
        let big = Node::String("x".repeat(MAX_STRING));
        let make = |pad: usize, input_is_object: bool| {
            let mut input = Object::default();
            for i in 0..8 {
                input.push(format!("s{i}"), big.clone());
            }
            input.push("file_path", Node::String("/f".into()));
            input.push("pad", Node::String("p".repeat(pad)));
            input.push("command", Node::String("ls".into()));
            input.push("skill", Node::Null);
            let mut payload = Object::default();
            payload.push("hook_event_name", Node::String("PreToolUse".into()));
            for i in 0..7 {
                payload.push(format!("t{i}"), big.clone());
            }
            if input_is_object {
                payload.push("tool_input", Node::Object(input));
            } else {
                // as big, but beside a tool_input that isn't an object
                payload.push("input", Node::Object(input));
                payload.push("tool_input", Node::String("x".into()));
            }
            let (_, envelope) = envelope(true, payload, time(0), &no_vars);
            envelope.unwrap()
        };
        let limit = MAX_REQUEST - SENT_ROOM;
        let pad = limit - make(0, true).to_json().len();
        assert!((1..=MAX_STRING).contains(&pad), "{pad}");

        let at_limit = request(&mut make(pad, true));
        assert_eq!((at_limit.len(), &at_limit), (limit, &make(pad, true).to_json()));

        let over = request(&mut make(pad + 1, true));
        let Node::Object(mut sent) = json::parse(&over, READ_DEPTH).unwrap() else {
            panic!()
        };
        let Some(Node::Object(payload)) = sent.remove(PAYLOAD) else {
            panic!()
        };
        let Some(Node::Object(input)) = payload.get("tool_input") else {
            panic!()
        };
        assert_eq!(input.keys().collect::<Vec<_>>(), ["file_path", "command", "skill"]);
        assert_eq!(payload.keys().count(), 9, "the rest of the event is kept");

        let mut not_an_object = make(pad + 100, false);
        let line = not_an_object.to_json();
        assert!(line.len() > limit);
        assert_eq!(request(&mut not_an_object), line);
    }

    /// `sent` goes at the end of the line, written as the C# writes a double, within the room left for it.
    #[test]
    fn sent_is_added_at_the_end() {
        assert_eq!(
            sent(r#"{"v":1,"payload":{}}"#, time(1_758_800_000_123)),
            r#"{"v":1,"payload":{},"sent":1758800000.123}"#
        );
        assert_eq!(sent("{}", time(1_758_800_000_000)), r#"{,"sent":1758800000}"#);
        // ms precision, so at most 3 decimals: 22 bytes until 2286
        let added = sent("{}", time(4_102_444_799_999)).len() - "{}".len();
        assert_eq!(added, 22);
        assert!(added <= SENT_ROOM);
    }

    /// The reply that says the pet has the event is `"ok":true`, nothing else.
    #[test]
    fn only_ok_true_is_taken() {
        for (reply, ok) in [
            (Some(r#"{"ok":true,"outcome":"thinking"}"#), Ok(true)),
            (Some(r#"{"ok":false,"error":"unknown request"}"#), Ok(false)),
            (Some(r#"{"ok":"true"}"#), Ok(false)),
            (Some("[true]"), Ok(false)),
            (None, Ok(false)),
            (Some("{nope"), Err(())),
        ] {
            assert_eq!(taken(reply).map_err(|_| ()), ok, "{reply:?}");
        }
    }

    #[test]
    fn the_agent_and_the_budget() {
        let args = |a: &[&str]| a.iter().map(OsString::from).collect::<Vec<_>>();
        assert_eq!(
            arg_value(&args(&["--agent", "Codex"]), "--agent").as_deref(),
            Some("codex")
        );
        assert_eq!(arg_value(&args(&["x", "--agent"]), "--agent"), None);
        assert_eq!(arg_value(&args(&[]), "--agent"), None);
        // at most CONNECT; stdin's full 2 s leaves 500 ms; later, less than nothing
        assert_eq!(busy_wait(Duration::ZERO), 2500);
        assert_eq!(busy_wait(Duration::from_millis(2000)), 500);
        assert_eq!(busy_wait(Duration::from_millis(2600)), -100);
    }
}
