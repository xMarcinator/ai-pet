//! `POST /hook/claude`: Claude's HTTP hooks. It takes no token (owner's decision): the guard's Host, Origin and size
//! checks are all it has. Every other answer is 200 `{}`, at once: Claude takes a plain-text body for an error.
//!
//! The body is Claude's hook input, as the exe got it on stdin, and the headers [`CLAUDE_ENV_HEADERS`] carry what it
//! read from its environment. They become the envelope (v1) the exe made ([`aipet_ipc::event`]): mended, without
//! `tool_response`, its strings cut, and `env` from the headers, with the pet's time of arrival as `at` and
//! `"via":"http"` (no `pid`). It is queued ([`Events`](crate::events::Events)) and applied in arrival order through
//! the core's ingress, as the pipe's events are. A body that isn't a JSON object is dropped with a line in the log; an
//! unknown event is applied, and the record says it was ignored.
//!
//! A `transcript_path` outside Claude's and Codex's folders is dropped from the event: the chats read the file it
//! names for their titles, and anything on the machine can post here.

use std::path::{Component, Path};
use std::sync::Arc;
use std::time::SystemTime;

use aipet_ipc::event::{self, READ_DEPTH, mended, trimmed, twice};
use aipet_ipc::json::{self, Node, Object};
use aipet_ipc::paths;
use aipet_ipc::protocol::{CLAUDE_ENV, CLAUDE_ENV_HEADERS, CLAUDE_HOOK_PATH, MAX_REQUEST, VIA};
use axum::Router;
use axum::body::Body;
use axum::extract::State as Shared;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use axum::routing::post;

use crate::State;
use crate::guard::empty;

pub(crate) fn routes() -> Router<Arc<State>> {
    Router::new().route(CLAUDE_HOOK_PATH, post(claude))
}

async fn claude(Shared(state): Shared<Arc<State>>, headers: HeaderMap, body: Body) -> Response {
    let Ok(body) = axum::body::to_bytes(body, MAX_REQUEST).await else {
        return empty(StatusCode::PAYLOAD_TOO_LARGE);
    };
    let at = SystemTime::now();
    match envelope(&body, &headers, at, &agent_file) {
        Ok(envelope) => {
            state.events.push(envelope);
        }
        Err(why) => state.events.drop_event(&format!("Claude's hook sent {why}")),
    }
    let mut answer = Response::new(Body::from("{}"));
    answer.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    answer
}

/// The envelope the exe would have made of the body and the headers, arriving `at`; `Err` says why there is none.
/// `readable` says whether the chats may read a `transcript_path`.
fn envelope(
    body: &[u8],
    headers: &HeaderMap,
    at: SystemTime,
    readable: &dyn Fn(&Path) -> bool,
) -> Result<Object, String> {
    let text = String::from_utf8_lossy(body);
    let payload = match json::parse(&mended(&text), READ_DEPTH) {
        Ok(Node::Object(payload)) => payload,
        Ok(_) => return Err("JSON that isn't an object".into()),
        Err(e) => return Err(format!("what isn't JSON: {e}")),
    };
    if let Some(key) = payload.duplicate() {
        return Err(format!("an event it can't read: {}", twice(key)));
    }
    let mut payload = trimmed(payload).map_err(|why| format!("an event it can't read: {why}"))?;
    if payload
        .str("transcript_path")
        .is_some_and(|path| !path.is_empty() && !readable(Path::new(path)))
    {
        payload.remove("transcript_path");
    }
    // where the chat runs, and the desktop app's own id for it: an unset variable comes as nothing, or as its name
    let mut env = Object::default();
    for (header, name) in CLAUDE_ENV_HEADERS.iter().zip(CLAUDE_ENV) {
        if let Some(value) = headers.get(*header) {
            let value = String::from_utf8_lossy(value.as_bytes());
            if !(value.starts_with("${") && value.ends_with('}')) {
                env.push(name, Node::String(value.into_owned()));
            }
        }
    }
    let mut envelope = event::envelope("claude", at, None, Some(env), payload);
    envelope.push(VIA, Node::String("http".into()));
    Ok(envelope)
}

/// Whether a path lies in Claude's folder (`$CLAUDE_CONFIG_DIR` or `~/.claude`) or Codex's home: absolute, and
/// without `..`.
fn agent_file(path: &Path) -> bool {
    let homes = [paths::claude_home(), paths::home().join(".claude"), paths::codex_home()];
    under(path, &homes)
}

fn under(path: &Path, homes: &[std::path::PathBuf]) -> bool {
    path.is_absolute()
        && !path.components().any(|c| c == Component::ParentDir)
        && homes
            .iter()
            .any(|home| home.is_absolute() && path.starts_with(home) && path != home)
}

#[cfg(test)]
mod tests {
    use aipet_ipc::protocol::{AGENT, AT, ENV, MAX_STRING, PAYLOAD, PID, format_seconds, unix_time};

    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        map
    }

    fn anywhere(_: &Path) -> bool {
        true
    }

    fn payload(envelope: &Object) -> &Object {
        match envelope.get(PAYLOAD) {
            Some(Node::Object(p)) => p,
            other => panic!("no payload: {other:?}"),
        }
    }

    /// The envelope is the exe's, but for `pid`, which there is none of, and `via`; `at` is the arrival.
    #[test]
    fn the_body_and_the_headers_make_the_exes_envelope() {
        let at = SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1_790_000_000_123);
        let body = br#"{"session_id":"s","hook_event_name":"UserPromptSubmit","prompt":"hi"}"#;
        let made = envelope(
            body,
            &headers(&[
                ("X-AiPet-Entrypoint", "claude-desktop"),
                ("x-aipet-host-session", "local_1"),
                ("X-Other", "x"),
            ]),
            at,
            &anywhere,
        )
        .unwrap();
        assert_eq!(
            made.to_json(),
            r#"{"v":1,"type":"event","agent":"claude","at":1790000000.123,"env":{"CLAUDE_CODE_ENTRYPOINT":"claude-desktop","CLAUDE_CODE_HOST_SESSION_ID":"local_1"},"payload":{"session_id":"s","hook_event_name":"UserPromptSubmit","prompt":"hi"},"via":"http"}"#
        );
        assert_eq!(made.get(PID), None);
        assert_eq!(made.str(AGENT), Some("claude"));
        assert_eq!(made.get(AT), Some(&Node::Number(format_seconds(unix_time(at)))));
        // a header left out, empty, or never expanded
        for (pairs, env) in [
            (vec![], "{}"),
            (
                vec![("X-AiPet-Host-Session", "")],
                r#"{"CLAUDE_CODE_HOST_SESSION_ID":""}"#,
            ),
            (vec![("X-AiPet-Entrypoint", "${CLAUDE_CODE_ENTRYPOINT}")], "{}"),
        ] {
            let made = envelope(body, &headers(&pairs), at, &anywhere).unwrap();
            let Some(Node::Object(got)) = made.get(ENV) else {
                panic!()
            };
            assert_eq!(got.to_json(), env, "{pairs:?}");
        }
    }

    /// As the exe trims it: no tool_response, every string cut, escaped lone surrogates mended.
    #[test]
    fn the_event_is_trimmed_as_the_exe_trims_it() {
        let long = "x".repeat(MAX_STRING + 10);
        let body = format!(
            r#"{{"hook_event_name":"PostToolUse","tool_input":{{"command":"{long}"}},"tool_response":{{"stdout":"{long}"}},"cut":"a\ud800b"}}"#
        );
        let made = envelope(body.as_bytes(), &HeaderMap::new(), SystemTime::now(), &anywhere).unwrap();
        let p = payload(&made);
        assert_eq!(p.get("tool_response"), None);
        let Some(Node::Object(input)) = p.get("tool_input") else {
            panic!()
        };
        assert_eq!(input.str("command").map(str::len), Some(MAX_STRING));
        assert_eq!(p.str("cut"), Some("a\u{fffd}b"));
    }

    #[test]
    fn what_isnt_an_event_is_refused() {
        for body in [
            &b"{not json"[..],
            b"",
            b"   ",
            b"[1,2]",
            b"\"Stop\"",
            br#"{"hook_event_name":"Stop","a":1,"a":2}"#,
            br#"{"x":[{"a":1,"a":2}]}"#,
        ] {
            assert!(
                envelope(body, &HeaderMap::new(), SystemTime::now(), &anywhere).is_err(),
                "{}",
                String::from_utf8_lossy(body)
            );
        }
        // too deep for the C#'s reader
        let deep = format!("{{\"x\":{}{}}}", "[".repeat(64), "]".repeat(64));
        assert!(envelope(deep.as_bytes(), &HeaderMap::new(), SystemTime::now(), &anywhere).is_err());
        // a member twice inside the dropped tool_response is no matter, as for the exe
        let body = br#"{"hook_event_name":"PostToolUse","tool_response":{"a":1,"a":2}}"#;
        assert!(envelope(body, &HeaderMap::new(), SystemTime::now(), &anywhere).is_ok());
    }

    /// A transcript_path the chats may not read is dropped; the rest of the event stays.
    #[test]
    fn a_transcript_path_outside_the_agents_folders_is_dropped() {
        let readable = |path: &Path| path.starts_with("/home/u/.claude");
        let with = |path: &str| {
            let body = format!(r#"{{"hook_event_name":"Stop","transcript_path":{path:?}}}"#);
            let made = envelope(body.as_bytes(), &HeaderMap::new(), SystemTime::now(), &readable).unwrap();
            payload(&made).str("transcript_path").map(str::to_owned)
        };
        assert_eq!(
            with("/home/u/.claude/projects/p/s.jsonl").as_deref(),
            Some("/home/u/.claude/projects/p/s.jsonl")
        );
        assert_eq!(with("/etc/shadow"), None);
        assert_eq!(with("").as_deref(), Some(""));
    }

    #[test]
    fn under_takes_files_in_a_home_and_nothing_else() {
        let base = std::env::temp_dir();
        let (claude, codex) = (base.join(".claude"), base.join(".codex"));
        let homes = [claude.clone(), codex.clone(), "".into()];
        for (path, inside) in [
            (claude.join("projects").join("s.jsonl"), true),
            (codex.join("sessions").join("r.jsonl"), true),
            (claude.clone(), false),
            (claude.join("..").join(".ssh").join("id_ed25519"), false),
            (base.join(".claudette").join("s.jsonl"), false),
            ("relative/.claude/s.jsonl".into(), false),
            (base.join("passwd"), false),
        ] {
            assert_eq!(under(&path, &homes), inside, "{}", path.display());
        }
    }
}
