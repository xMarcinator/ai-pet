//! `report_event`: Codex's hooks. Codex's `mcp_tool` hooks call it on their `aipet` MCP connection with the fields of
//! the hook's input that event has (one input table per event: a placeholder an event lacks fails its hook), as
//! `{agent: "codex", event: {...}}`.
//!
//! The event becomes the envelope (v1) the exe made of Codex's input ([`aipet_ipc::event`]): without
//! `tool_response`, its strings cut, no `env`, with the pet's time of arrival as `at` and `"via":"http"` (no `pid`).
//! It is queued ([`Events`](crate::events::Events)) and applied in arrival order through the core's ingress, as the
//! pipe's events are. Codex waits for each hook's call before it goes on, and its chats are ordered by its own turn
//! and tool ids first, so they need none of the arrival rules Claude's HTTP events have.
//!
//! What the chats wouldn't take is refused before it is queued (the queue answers nothing back), as `isError` with
//! the reason and a line in the log: another agent than Codex, an event Codex's chats don't take, or one without a
//! session. A `transcript_path` outside Claude's and Codex's folders is dropped, as for Claude's hooks: the tool is
//! the model's to call too.
//!
//! The tool can't be hidden from the model: Codex refuses a hook's call to a tool in `disabled_tools`. Its
//! description tells the model to leave it alone.

use std::path::Path;
use std::time::SystemTime;

use aipet_core::sessions::codex_takes;
use aipet_ipc::event::{self, READ_DEPTH, trimmed};
use aipet_ipc::json::{self, Node, Object};
use aipet_ipc::protocol::VIA;
use serde_json::{Map, Value};

/// The envelope the exe would have made of Codex's `event`, arriving `at`; `Err` says why there is none. `readable`
/// says whether the chats may read a `transcript_path`.
pub(crate) fn envelope(
    agent: &str,
    event: &Map<String, Value>,
    at: SystemTime,
    readable: &dyn Fn(&Path) -> bool,
) -> Result<Object, String> {
    if agent != "codex" {
        return Err(format!(
            "agent is {agent:?}: only \"codex\" reports here (Claude's hooks post to the pet themselves)"
        ));
    }
    let name = match event.get("hook_event_name") {
        Some(Value::String(name)) => name,
        _ => return Err("the event has no hook_event_name".into()),
    };
    if !codex_takes(name) {
        return Err(format!("{name:?} isn't a Codex event the pet takes"));
    }
    if !matches!(event.get("session_id"), Some(Value::String(sid)) if !sid.is_empty()) {
        return Err("the event has no session_id".into());
    }
    // the pet's own JSON tree, from the text: rmcp has the arguments as serde_json's map, so their keys come sorted
    let payload = match json::parse(&Value::Object(event.clone()).to_string(), READ_DEPTH) {
        Ok(Node::Object(payload)) => payload,
        Ok(_) => unreachable!("an object's text is an object"),
        Err(e) => return Err(format!("the event can't be read: {e}")),
    };
    let mut payload = trimmed(payload).map_err(|why| format!("the event can't be read: {why}"))?;
    if payload
        .str("transcript_path")
        .is_some_and(|path| !path.is_empty() && !readable(Path::new(path)))
    {
        payload.remove("transcript_path");
    }
    let mut envelope = event::envelope("codex", at, None, None, payload);
    envelope.push(VIA, Node::String("http".into()));
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use aipet_ipc::protocol::{ENV, MAX_STRING, PAYLOAD, PID};
    use serde_json::json;

    use super::*;

    fn anywhere(_: &Path) -> bool {
        true
    }

    fn map(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            other => panic!("not an object: {other}"),
        }
    }

    fn payload(envelope: &Object) -> &Object {
        match envelope.get(PAYLOAD) {
            Some(Node::Object(p)) => p,
            other => panic!("no payload: {other:?}"),
        }
    }

    /// The exe's envelope of the same input, but for `pid`, which there is none of, and `via`; `at` is the arrival.
    #[test]
    fn the_event_makes_the_exes_envelope() {
        let at = SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1_790_000_000_123);
        let event = map(
            json!({"session_id": "s", "hook_event_name": "PreToolUse", "turn_id": "t",
            "tool_name": "Bash", "tool_use_id": "c1", "tool_input": {"command": "ls", "timeout": 1.5},
            "transcript_path": null}),
        );
        let made = envelope("codex", &event, at, &anywhere).unwrap();
        assert_eq!(
            made.to_json(),
            r#"{"v":1,"type":"event","agent":"codex","at":1790000000.123,"payload":{"hook_event_name":"PreToolUse","session_id":"s","tool_input":{"command":"ls","timeout":1.5},"tool_name":"Bash","tool_use_id":"c1","transcript_path":null,"turn_id":"t"},"via":"http"}"#
        );
        assert_eq!(made.get(PID), None);
        assert_eq!(made.get(ENV), None);
    }

    /// As the exe trims it: no tool_response, every string cut.
    #[test]
    fn the_event_is_trimmed_as_the_exe_trims_it() {
        let long = "x".repeat(MAX_STRING + 10);
        let event = map(json!({"session_id": "s", "hook_event_name": "PostToolUse",
            "tool_input": {"command": long}, "tool_response": {"stdout": long}}));
        let made = envelope("codex", &event, SystemTime::now(), &anywhere).unwrap();
        let p = payload(&made);
        assert_eq!(p.get("tool_response"), None);
        let Some(Node::Object(input)) = p.get("tool_input") else {
            panic!()
        };
        assert_eq!(input.str("command").map(str::len), Some(MAX_STRING));
    }

    #[test]
    fn another_agent_or_an_event_codex_chats_dont_take_is_refused() {
        let stop = map(json!({"session_id": "s", "hook_event_name": "Stop"}));
        for agent in ["claude", "Codex", ""] {
            assert!(envelope(agent, &stop, SystemTime::now(), &anywhere).is_err(), "{agent}");
        }
        for event in [
            json!({"session_id": "s"}),
            json!({"session_id": "s", "hook_event_name": 7}),
            json!({"session_id": "s", "hook_event_name": "Notification"}),
            json!({"session_id": "s", "hook_event_name": "stop"}),
            json!({"hook_event_name": "Stop"}),
            json!({"session_id": "", "hook_event_name": "Stop"}),
            json!({"session_id": 1, "hook_event_name": "Stop"}),
        ] {
            assert!(
                envelope("codex", &map(event.clone()), SystemTime::now(), &anywhere).is_err(),
                "{event}"
            );
        }
        // too deep for the C#'s reader
        let mut deep = json!([]);
        for _ in 0..64 {
            deep = json!([deep]);
        }
        let event = map(json!({"session_id": "s", "hook_event_name": "Stop", "x": deep}));
        assert!(envelope("codex", &event, SystemTime::now(), &anywhere).is_err());
        for ev in ["SessionStart", "Interrupt", "SubagentStop", "SessionEnd"] {
            let event = map(json!({"session_id": "s", "hook_event_name": ev}));
            assert!(envelope("codex", &event, SystemTime::now(), &anywhere).is_ok(), "{ev}");
        }
    }

    /// A transcript_path the chats may not read is dropped; the rest of the event stays.
    #[test]
    fn a_transcript_path_outside_the_agents_folders_is_dropped() {
        let readable = |path: &Path| path.starts_with("/home/u/.codex");
        let with = |path: Value| {
            let event = map(json!({"session_id": "s", "hook_event_name": "Stop", "transcript_path": path}));
            let made = envelope("codex", &event, SystemTime::now(), &readable).unwrap();
            payload(&made).get("transcript_path").cloned()
        };
        let inside = "/home/u/.codex/sessions/2026/11/10/rollout-1.jsonl";
        assert_eq!(with(json!(inside)), Some(Node::String(inside.into())));
        assert_eq!(with(json!("/etc/shadow")), None);
        assert_eq!(with(Value::Null), Some(Node::Null));
    }
}
