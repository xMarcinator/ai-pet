//! A request's answer: the version check, then a ping or an event, else a refusal (`HookServer.cs:266-299`).
//!
//! The line is read as the C#'s `JsonNode.Parse` reads it: valid JSON nesting at most 128 deep, and an object. Nested
//! values are only checked for being JSON (numbers past a double's range, lone surrogates in escapes and repeated
//! members pass), but the request's own members must all differ: the C# throws when it reads the first of them,
//! outside its `try`, so such a line gets no answer either. The replies are written as System.Text.Json writes them.
//!
//! An event is then read by [`Envelope::parse`], which refuses a few lines the C# still applies (a number past a
//! double's range, a lone surrogate, nesting exactly 128 deep): no hook sends one, and they get no answer.

use std::collections::HashSet;
use std::fmt::{self, Write};
use std::time::SystemTime;

use aipet_ipc::protocol::{APP, ERROR, KIND, MAX_DEPTH, OK, OUTCOME, PID, RECENT, V, VERSION, unix_time};
use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::value::RawValue;

use super::Shared;
use crate::sessions::Envelope;

/// The reply to one request line; none (no reply) for a line that isn't a JSON object the C# can read.
pub(super) fn answer(server: &Shared, line: &str) -> Option<String> {
    let request = Request::parse(line)?;
    let version = request.v.and_then(|v| serde_json::from_str::<f64>(v.get()).ok());
    if version != Some(f64::from(VERSION)) {
        return Some(refuse("unsupported version"));
    }
    let kind = match request.kind {
        // a string the C# can't read as UTF-16 (a lone surrogate) throws there: no answer
        Some(kind) if kind.get().starts_with('"') => Some(serde_json::from_str::<String>(kind.get()).ok()?),
        _ => None,
    };
    match kind.as_deref() {
        Some("ping") => Some(ping(server)),
        Some("event") => event(server, line),
        _ => Some(refuse("unknown request")),
    }
}

/// `{"ok":true,"app":"AiPet","pid":…,"recent":[the last 30 hook-events.log lines]}`: the doctor's ping.
fn ping(server: &Shared) -> String {
    let record = server.record();
    let mut recent = String::new();
    for (i, line) in record.recent().enumerate() {
        if i > 0 {
            recent.push(',');
        }
        quote(&mut recent, line);
    }
    format!(
        r#"{{"{OK}":true,"{APP}":"AiPet","{PID}":{},"{RECENT}":[{recent}]}}"#,
        std::process::id()
    )
}

/// An event goes to the chats, and its line to hook-events.log. `changed` runs unless the event changed nothing or
/// failed; a failed one is refused with its `error:` outcome.
fn event(server: &Shared, line: &str) -> Option<String> {
    let envelope = Envelope::parse(line)?;
    let applied = server.sessions.apply(&envelope, unix_time(SystemTime::now()));
    let failed = applied.outcome.strip_prefix("error:");
    if let Some(error) = failed {
        server.log(&format!("hooks: an event failed: {error}"));
    }
    server.record().add(&applied.log, SystemTime::now());
    if failed.is_some() {
        return Some(refuse(applied.outcome));
    }
    if !matches!(applied.outcome, "ignored" | "stale") {
        server.changed();
    }
    let mut reply = format!(r#"{{"{OK}":true,"{OUTCOME}":"#);
    quote(&mut reply, applied.outcome);
    reply.push('}');
    Some(reply)
}

/// `{"ok":false,"error":<why>}`.
fn refuse(why: &str) -> String {
    let mut reply = format!(r#"{{"{OK}":false,"{ERROR}":"#);
    quote(&mut reply, why);
    reply.push('}');
    reply
}

/// A JSON string as System.Text.Json writes it with its default encoder: printable ASCII as it is, except `"`, `&`,
/// `'`, `+`, `<`, `>` and `` ` ``, which are escaped as `"` and so on, like everything outside ASCII (UTF-16
/// units, upper-case hex); `\\`, and `\b`, `\f`, `\n`, `\r`, `\t` for those controls.
fn quote(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ' '..='~' if !matches!(c, '"' | '&' | '\'' | '+' | '<' | '>' | '`') => out.push(c),
            _ => {
                for unit in c.encode_utf16(&mut [0; 2]) {
                    // writing to a String can't fail
                    let _ = write!(out, "\\u{unit:04X}");
                }
            }
        }
    }
    out.push('"');
}

/// The members of a request line that the answer reads, each as its JSON text.
struct Request<'a> {
    v: Option<&'a RawValue>,
    kind: Option<&'a RawValue>,
}

impl<'a> Request<'a> {
    /// None when the C# would get no object from the line, or throw on reading its first member.
    fn parse(line: &'a str) -> Option<Self> {
        let request = serde_json::from_str(line).ok()?;
        (depth(line) <= MAX_DEPTH).then_some(request)
    }
}

impl<'de> Deserialize<'de> for Request<'de> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Members;

        impl<'de> Visitor<'de> for Members {
            type Value = Request<'de>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Request<'de>, A::Error> {
                let mut request = Request { v: None, kind: None };
                let mut names = HashSet::new();
                while let Some(name) = map.next_key::<String>()? {
                    // a value is only checked for being JSON, whatever is in it
                    let value: &RawValue = map.next_value()?;
                    match name.as_str() {
                        V => request.v = Some(value),
                        KIND => request.kind = Some(value),
                        _ => {}
                    }
                    if !names.insert(name) {
                        return Err(de::Error::custom("a member is there twice"));
                    }
                }
                Ok(request)
            }
        }

        d.deserialize_map(Members)
    }
}

/// How deep the arrays and objects of a line of JSON nest.
fn depth(json: &str) -> usize {
    let (mut depth, mut deepest) = (0usize, 0usize);
    let (mut in_string, mut escaped) = (false, false);
    for b in json.bytes() {
        if in_string {
            match b {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
        } else {
            match b {
                b'"' => in_string = true,
                b'[' | b'{' => {
                    depth += 1;
                    deepest = deepest.max(depth);
                }
                b']' | b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    deepest
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What System.Text.Json's default writer (`JsonObject.ToJsonString()`) gives for each string: .NET 10's output,
    /// taken from the C# once. In this table `_` stands for a backslash.
    #[test]
    fn strings_are_written_as_system_text_json_writes_them() {
        let backslash = char::from(92).to_string();
        let chars =
            |code_points: &[u32]| -> String { code_points.iter().map(|&c| char::from_u32(c).unwrap()).collect() };
        for (s, net) in [
            ("plain".to_owned(), r#""plain""#),
            (r#"q"b_s/"#.replace('_', &backslash), r#""q_u0022b__s/""#),
            ("<>&'+`".to_owned(), r#""_u003C_u003E_u0026_u0027_u002B_u0060""#),
            (chars(&[0xe9, 0x20, 0xfc, 0x20, 0xdf]), r#""_u00E9 _u00FC _u00DF""#),
            (chars(&[0x2028, 0x2029]), r#""_u2028_u2029""#),
            (chars(&[0x1f600]), r#""_uD83D_uDE00""#),
            (
                chars(&[0x1, 0x1f, 0x7f, 0x8, 0xc, 0xa, 0xd, 0x9]),
                r#""_u0001_u001F_u007F_b_f_n_r_t""#,
            ),
            (chars(&[0xa0, 0xff, 0x100]), r#""_u00A0_u00FF_u0100""#),
            (
                "~ ^ | { } [ ] @ # $ % * = ? ! : ; ,".to_owned(),
                r#""~ ^ | { } [ ] @ # $ % * = ? ! : ; ,""#,
            ),
            (chars(&[0xfeff, 0xfffd]), r#""_uFEFF_uFFFD""#),
        ] {
            let mut out = String::new();
            quote(&mut out, &s);
            assert_eq!(out, net.replace('_', &backslash), "{s:?}");
        }
    }

    #[test]
    fn depth_counts_arrays_and_objects_outside_strings() {
        for (json, deep) in [
            ("1", 0),
            ("{}", 1),
            (r#"{"a":[1,{"b":[]}],"c":{}}"#, 4),
            (r#"{"a":"[[[{{{\"[[["}"#, 1),
            (r#"{"a":"\\","b":[[]]}"#, 3),
        ] {
            assert_eq!(depth(json), deep, "{json}");
        }
    }
}
