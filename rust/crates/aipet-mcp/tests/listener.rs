//! The listener over real HTTP, on a port of the test's own: its checks (R2, R3), both MCP eras, the port taken, the
//! tools' conformance (R11), which tasks 4-6 extend with their tools, and Claude's HTTP hooks (R4, R5).
//!
//! The requests are written by hand on a TCP stream, so a test can send any Host, Origin or body it likes.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use aipet_core::server::Ingress;
use aipet_core::sessions::{AgentSessions, Entry, Envelope};
use aipet_ipc::event;
use aipet_ipc::json::{self as aipet_json, Node, Object};
use aipet_ipc::protocol::unix_time;
use aipet_mcp::{Listener, Options, TOOLS};
use serde_json::{Value, json};

/// A listener on a free port, with a token file, hook-events.log and log of the test's own.
struct Pet {
    listener: Listener,
    ingress: Arc<Ingress>,
    token: String,
    log: Arc<Mutex<Vec<String>>>,
    root: PathBuf,
}

impl Pet {
    fn start(test: &str) -> Pet {
        let root = std::env::temp_dir().join(format!("aipet-mcp-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let ingress = Arc::new(Ingress::new(
            Arc::new(AgentSessions::with_codex_home(root.join("codex"))),
            || {},
            root.join("hook-events.log"),
            Box::new(|_| {}),
        ));
        let listener = Listener::start(options(&root, 0, &log), Arc::clone(&ingress)).unwrap();
        let token = std::fs::read_to_string(root.join("mcp-token")).unwrap();
        Pet {
            listener,
            ingress,
            token,
            log,
            root,
        }
    }

    fn port(&self) -> u16 {
        self.listener.port()
    }

    fn logged(&self, starting: &str) -> usize {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.starts_with(starting))
            .count()
    }

    /// A POST to `/mcp` with the token, as the other headers say.
    fn mcp(&self, headers: &[(&str, &str)], body: &str) -> Reply {
        let bearer = format!("Bearer {}", self.token);
        let mut all = vec![
            ("Authorization", bearer.as_str()),
            ("Content-Type", "application/json"),
            ("Accept", "application/json, text/event-stream"),
        ];
        all.extend_from_slice(headers);
        request(self.port(), "POST", "/mcp", &all, body.as_bytes())
    }

    /// A 2026-07-28 request: stateless, with the protocol's headers and `_meta`. The JSON-RPC reply.
    fn stateless(&self, method: &str, name: Option<&str>, mut params: Value) -> (u16, Value) {
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientInfo": {"name": "aipet-test", "version": "1"},
            "io.modelcontextprotocol/clientCapabilities": {},
        });
        let body = json!({"jsonrpc": "2.0", "id": 7, "method": method, "params": params}).to_string();
        let mut headers = vec![("MCP-Protocol-Version", "2026-07-28"), ("Mcp-Method", method)];
        if let Some(name) = name {
            headers.push(("Mcp-Name", name));
        }
        let reply = self.mcp(&headers, &body);
        (reply.status, reply.json())
    }
}

impl Drop for Pet {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn options(root: &std::path::Path, port: u16, log: &Arc<Mutex<Vec<String>>>) -> Options {
    let log = Arc::clone(log);
    Options {
        port,
        token_file: root.join("mcp-token"),
        version: "9.9.9-test".into(),
        log: Arc::new(move |line| log.lock().unwrap().push(line.to_owned())),
    }
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The body's JSON-RPC message: the body itself, or an SSE stream's last `data:` that is a message.
    fn json(&self) -> Value {
        if let Ok(value) = serde_json::from_str(&self.body) {
            return value;
        }
        self.body
            .lines()
            .rev()
            .filter_map(|l| l.strip_prefix("data:"))
            .filter_map(|d| serde_json::from_str::<Value>(d.trim()).ok())
            .find(|m| m.get("id").is_some())
            .unwrap_or_else(|| panic!("no JSON in {:?}", self.body))
    }
}

/// One request on a connection of its own. Host is the listener's unless `headers` give one; the body is written
/// from another thread, so a reply that comes before the whole body was taken is still read.
fn request(port: u16, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Reply {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let mut head = format!("{method} {path} HTTP/1.1\r\n");
    if !headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("host")) {
        head.push_str(&format!("Host: 127.0.0.1:{port}\r\n"));
    }
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if !headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("transfer-encoding")) {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("Connection: close\r\n\r\n");
    stream.write_all(head.as_bytes()).unwrap();
    let mut writer = stream.try_clone().unwrap();
    let body = body.to_vec();
    let writing = thread::spawn(move || {
        let _ = writer.write_all(&body);
    });
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let _ = writing.join();
    parse(&raw)
}

fn parse(raw: &[u8]) -> Reply {
    let text = String::from_utf8_lossy(raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("no reply: {text:?}"));
    let mut lines = head.lines();
    let status = lines.next().unwrap().split(' ').nth(1).unwrap().parse().unwrap();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_owned(), v.trim().to_owned()))
        .collect();
    let chunked = headers
        .iter()
        .any(|(n, v)| n.eq_ignore_ascii_case("transfer-encoding") && v.contains("chunked"));
    let body = if chunked { unchunk(body) } else { body.to_owned() };
    Reply { status, headers, body }
}

fn unchunk(mut body: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = body.split_once("\r\n") {
        let size = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if size == 0 || rest.len() < size {
            break;
        }
        out.push_str(&rest[..size]);
        body = rest[size..].trim_start_matches("\r\n");
    }
    out
}

const INITIALIZE_2025: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"aipet-test","version":"1"}}}"#;

// ------------------------------------------------------------------------------------------- R2, R3: the checks

#[test]
fn no_token_or_a_wrong_one_gets_401_logged_once() {
    let pet = Pet::start("token");
    let wrong = format!("Bearer {}", "0".repeat(64));
    for headers in [
        vec![],
        vec![("Authorization", wrong.as_str())],
        vec![("Authorization", "Basic dXNlcjpwYXNz")],
        vec![("Authorization", "Bearer")],
    ] {
        let mut all = vec![("Content-Type", "application/json")];
        all.extend(headers.iter().copied());
        let reply = request(pet.port(), "POST", "/mcp", &all, INITIALIZE_2025.as_bytes());
        assert_eq!((reply.status, reply.body.as_str()), (401, ""), "{headers:?}");
    }
    assert_eq!(pet.logged("mcp: refused"), 1, "{:?}", pet.log.lock().unwrap());
    let log = pet.log.lock().unwrap().join("\n");
    assert!(!log.contains(&pet.token), "the log never holds the token");
    // the right token passes
    assert_eq!(pet.mcp(&[], INITIALIZE_2025).status, 200);
}

#[test]
fn a_bad_origin_gets_403_and_none_or_the_listeners_own_passes() {
    let pet = Pet::start("origin");
    let own = format!("http://localhost:{}", pet.port());
    assert_eq!(pet.mcp(&[("Origin", &own)], INITIALIZE_2025).status, 200);
    let other_port = format!("http://127.0.0.1:{}", pet.port() ^ 1);
    for origin in [
        "http://evil.example",
        "null",
        "http://localhost:3000",
        other_port.as_str(),
    ] {
        let reply = pet.mcp(&[("Origin", origin)], INITIALIZE_2025);
        assert_eq!((reply.status, reply.body.as_str()), (403, ""), "{origin}");
        let hook = request(pet.port(), "POST", "/hook/claude", &[("Origin", origin)], b"{}");
        assert_eq!((hook.status, hook.body.as_str()), (403, ""), "{origin}");
    }
}

#[test]
fn a_host_other_than_loopback_gets_403() {
    let pet = Pet::start("host");
    let port = pet.port();
    for host in [
        format!("evil.example:{port}"),
        format!("127.0.0.1.nip.io:{port}"),
        format!("192.168.1.2:{port}"),
        "127.0.0.1:1".to_owned(),
    ] {
        let reply = pet.mcp(&[("Host", &host)], INITIALIZE_2025);
        assert_eq!((reply.status, reply.body.as_str()), (403, ""), "{host}");
        let hook = request(port, "POST", "/hook/claude", &[("Host", &host)], b"{}");
        assert_eq!((hook.status, hook.body.as_str()), (403, ""), "{host}");
    }
    for host in [format!("localhost:{port}"), format!("[::1]:{port}")] {
        let hook = request(port, "POST", "/hook/claude", &[("Host", &host)], b"{}");
        assert_eq!((hook.status, hook.body.as_str()), (200, "{}"), "{host}");
    }
}

#[test]
fn a_body_over_4_mib_gets_413() {
    let pet = Pet::start("size");
    let port = pet.port();
    let big = vec![b' '; 5 << 20];
    // with a Content-Length: refused before it is read
    let reply = pet.mcp(&[], std::str::from_utf8(&big).unwrap());
    assert_eq!((reply.status, reply.body.as_str()), (413, ""));
    let reply = request(port, "POST", "/hook/claude", &[], &big);
    assert_eq!((reply.status, reply.body.as_str()), (413, ""));
    // without one (chunked): cut off where it is read
    let mut chunked = format!("{:x}\r\n", big.len()).into_bytes();
    chunked.extend_from_slice(&big);
    chunked.extend_from_slice(b"\r\n0\r\n\r\n");
    let reply = request(
        port,
        "POST",
        "/hook/claude",
        &[("Transfer-Encoding", "chunked")],
        &chunked,
    );
    assert_eq!((reply.status, reply.body.as_str()), (413, ""));
    let bearer = format!("Bearer {}", pet.token);
    let headers = [
        ("Authorization", bearer.as_str()),
        ("Content-Type", "application/json"),
        ("Accept", "application/json, text/event-stream"),
        ("Transfer-Encoding", "chunked"),
    ];
    assert_eq!(request(port, "POST", "/mcp", &headers, &chunked).status, 413);
    // just under the limit is a hook body like any other
    let under = vec![b' '; (4 << 20) - 16];
    let reply = request(port, "POST", "/hook/claude", &[], &under);
    assert_eq!((reply.status, reply.body.as_str()), (200, "{}"));
}

#[test]
fn a_second_pet_on_the_same_port_logs_once_and_runs_without_the_listener() {
    let pet = Pet::start("taken");
    let log = Arc::new(Mutex::new(Vec::new()));
    let second = Listener::start(options(&pet.root, pet.port(), &log), Arc::clone(&pet.ingress));
    assert!(second.is_err());
    let lines = log.lock().unwrap().clone();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with(&format!("mcp: can't listen on 127.0.0.1:{}", pet.port())),
        "{lines:?}"
    );
    // the first one serves on
    assert_eq!(pet.mcp(&[], INITIALIZE_2025).status, 200);
}

// ------------------------------------------------------------------------------------------- both MCP eras

#[test]
fn a_2026_stateless_status_call_succeeds() {
    let pet = Pet::start("stateless");
    let (status, reply) = pet.stateless("tools/call", Some("status"), json!({"name": "status", "arguments": {}}));
    assert_eq!(status, 200, "{reply}");
    let result = &reply["result"];
    assert_eq!(result["isError"], false, "{reply}");
    assert_eq!(result["structuredContent"]["version"], "9.9.9-test");
    assert_eq!(result["structuredContent"]["port"], pet.port());
    // no Mcp-Session-Id: stateless
    let reply = pet.mcp(
        &[("MCP-Protocol-Version", "2026-07-28"), ("Mcp-Method", "tools/list")],
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {"_meta": {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientInfo": {"name": "t", "version": "1"},
            "io.modelcontextprotocol/clientCapabilities": {}}}})
        .to_string(),
    );
    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("mcp-session-id"), None);
}

#[test]
fn a_2025_session_succeeds_and_an_unknown_session_gets_404() {
    let pet = Pet::start("session");
    let init = pet.mcp(&[], INITIALIZE_2025);
    assert_eq!(init.status, 200, "{}", init.body);
    assert_eq!(init.json()["result"]["serverInfo"]["name"], "aipet");
    let session = init.header("mcp-session-id").expect("a session id").to_owned();
    let with = |extra: &[(&str, &str)], body: &str| {
        let mut headers = vec![
            ("Mcp-Session-Id", session.as_str()),
            ("MCP-Protocol-Version", "2025-11-25"),
        ];
        headers.extend_from_slice(extra);
        pet.mcp(&headers, body)
    };
    let initialized = with(&[], r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    assert_eq!(initialized.status, 202, "{}", initialized.body);
    let call = with(
        &[],
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"status","arguments":{}}}"#,
    );
    assert_eq!(call.status, 200, "{}", call.body);
    let result = &call.json()["result"];
    assert_eq!(result["structuredContent"]["port"], pet.port(), "{result}");

    let unknown = pet.mcp(
        &[
            ("Mcp-Session-Id", "no-such-session"),
            ("MCP-Protocol-Version", "2025-11-25"),
        ],
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}"#,
    );
    assert_eq!(unknown.status, 404, "{}", unknown.body);
}

// ------------------------------------------------------------------------------------------- R11: conformance

/// What a tool's annotations say.
#[derive(Clone, Copy, Debug)]
enum Kind {
    ReadOnly,
    #[allow(dead_code, reason = "tasks 5 and 6 add such tools")]
    Idempotent,
    #[allow(dead_code, reason = "task 6 adds such a tool")]
    Destructive,
    NotIdempotent,
}

/// A tool of the table: its name and kind, a call that succeeds, and one that is a business failure (`isError`),
/// where the tool has one. Tasks 4-6 add a row for each tool they add.
struct Row {
    name: &'static str,
    kind: Kind,
    ok: Value,
    failing: Option<Value>,
}

fn table() -> Vec<Row> {
    vec![
        Row {
            name: "status",
            kind: Kind::ReadOnly,
            ok: json!({}),
            failing: None,
        },
        Row {
            name: "report_event",
            kind: Kind::NotIdempotent,
            ok: json!({"agent": "codex", "event": {"session_id": "c0dex", "hook_event_name": "SessionStart",
                "cwd": "/w", "model": "gpt-5", "transcript_path": null, "source": "startup"}}),
            failing: Some(json!({"agent": "codex", "event": {"session_id": "c0dex", "hook_event_name": "Teleport"}})),
        },
    ]
}

#[test]
fn every_tool_conforms() {
    let pet = Pet::start("conformance");
    // something for status to show
    let line = r#"{"v":1,"type":"event","agent":"claude","at":1,"pid":1,"env":{},"payload":{"hook_event_name":"UserPromptSubmit","session_id":"c0nf0rm","prompt":"x"}}"#;
    pet.ingress.apply(&Envelope::parse(line).unwrap());

    let table = table();
    let (status, listed) = pet.stateless("tools/list", None, json!({}));
    assert_eq!(status, 200, "{listed}");
    let tools = listed["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    // the fixed order, every tool in the table
    assert_eq!(names, TOOLS);
    assert_eq!(table.iter().map(|r| r.name).collect::<Vec<_>>(), TOOLS);

    for (row, tool) in table.iter().zip(tools) {
        let name = row.name;
        let schema = &tool["outputSchema"];
        assert_eq!(schema["type"], "object", "{name}: {tool}");
        let a = &tool["annotations"];
        let hints = (
            a["readOnlyHint"].as_bool(),
            a["destructiveHint"].as_bool(),
            a["idempotentHint"].as_bool(),
        );
        let expected = match row.kind {
            Kind::ReadOnly => (Some(true), Some(false), Some(true)),
            Kind::Idempotent => (Some(false), Some(false), Some(true)),
            Kind::Destructive => (Some(false), Some(true), Some(true)),
            Kind::NotIdempotent => (Some(false), Some(false), Some(false)),
        };
        assert_eq!(hints, expected, "{name}: {a}");

        let (status, reply) = pet.stateless("tools/call", Some(name), json!({"name": name, "arguments": row.ok}));
        assert_eq!(status, 200, "{name}: {reply}");
        let result = &reply["result"];
        assert_eq!(result["isError"], false, "{name}: {reply}");
        let structured = &result["structuredContent"];
        conforms(structured, schema, name);
        let text = result["content"]
            .as_array()
            .and_then(|c| c.iter().find(|b| b["type"] == "text"))
            .and_then(|b| b["text"].as_str())
            .unwrap_or_else(|| panic!("{name}: no text block: {reply}"));
        assert_eq!(&serde_json::from_str::<Value>(text).unwrap(), structured, "{name}");

        if let Some(failing) = &row.failing {
            let (status, reply) = pet.stateless("tools/call", Some(name), json!({"name": name, "arguments": failing}));
            assert_eq!(status, 200, "{name}: {reply}");
            assert_eq!(reply["result"]["isError"], true, "{name}: {reply}");
            assert!(
                reply.get("error").is_none(),
                "{name}: a business failure isn't a JSON-RPC error"
            );
        }
    }

    // status shows what came in: the event applied, and report_event's once the queue has applied it
    until(|| pet.ingress.recent().len() >= 2);
    let (_, reply) = pet.stateless("tools/call", Some("status"), json!({"name": "status", "arguments": {}}));
    let status = &reply["result"]["structuredContent"];
    assert_eq!(status["registrations_seen"], json!({"claude": true, "codex": true}));
    let events = status["hook_events"].as_array().unwrap();
    assert_eq!(events.len(), 2, "{status}");
    assert!(events[0].as_str().unwrap().contains("c0nf0rm"), "{status}");
    assert!(events[1].as_str().unwrap().contains("SessionStart c0dex"), "{status}");
}

/// The structured content has every property the schema requires, and none it doesn't name; each of a type the schema
/// gives.
fn conforms(value: &Value, schema: &Value, name: &str) {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{name}: {value} isn't an object"));
    // an empty result (`{}`) has a schema without properties
    let none = serde_json::Map::new();
    let properties = schema["properties"].as_object().unwrap_or(&none);
    for required in schema["required"].as_array().into_iter().flatten() {
        assert!(object.contains_key(required.as_str().unwrap()), "{name}: no {required}");
    }
    for (key, field) in object {
        let property = properties
            .get(key)
            .unwrap_or_else(|| panic!("{name}: {key} isn't in the schema"));
        let types: Vec<&str> = match &property["type"] {
            Value::String(t) => vec![t.as_str()],
            Value::Array(ts) => ts.iter().filter_map(Value::as_str).collect(),
            // a $ref or an anyOf: an object of the schema's definitions
            _ => vec!["object"],
        };
        let actual = match field {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(n) if n.is_u64() || n.is_i64() => "integer",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        };
        assert!(
            types.contains(&actual) || (actual == "integer" && types.contains(&"number")),
            "{name}: {key} is {actual}, the schema says {types:?}"
        );
    }
}

#[test]
fn unknown_tools_and_malformed_calls_are_json_rpc_errors() {
    let pet = Pet::start("errors");
    for (what, params) in [
        ("an unknown tool", json!({"name": "no_such_tool", "arguments": {}})),
        ("no name", json!({"arguments": {}})),
        (
            "arguments that aren't an object",
            json!({"name": "status", "arguments": 7}),
        ),
    ] {
        let name = params["name"].as_str();
        let (_, reply) = pet.stateless("tools/call", name, params.clone());
        assert!(reply.get("error").is_some(), "{what}: {reply}");
        assert!(reply.get("result").is_none(), "{what}: {reply}");
    }
    // not JSON at all: refused at the HTTP level, before any JSON-RPC
    let reply = pet.mcp(&[], "{not json");
    assert!((400..500).contains(&reply.status), "{} {}", reply.status, reply.body);
}

/// At most 16 requests are handled at once; the rest wait, and all are answered.
#[test]
fn more_than_16_requests_at_once_all_get_answers() {
    let pet = Pet::start("busy");
    let port = pet.port();
    let answered = Arc::new(AtomicUsize::new(0));
    let threads: Vec<_> = (0..40)
        .map(|_| {
            let answered = Arc::clone(&answered);
            thread::spawn(move || {
                if request(port, "POST", "/hook/claude", &[], b"{}").status == 200 {
                    answered.fetch_add(1, Ordering::SeqCst);
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(answered.load(Ordering::SeqCst), 40);
}

// ------------------------------------------------------------------------------------- R4, R5: Claude's HTTP hooks

impl Pet {
    /// A hook's POST to `/hook/claude`.
    fn hook(&self, headers: &[(&str, &str)], body: &Value) -> Reply {
        let mut all = vec![("Content-Type", "application/json")];
        all.extend_from_slice(headers);
        request(self.port(), "POST", "/hook/claude", &all, body.to_string().as_bytes())
    }

    /// Waits until the record has `lines` lines (at most the 30 it keeps): the events queued so far are applied.
    fn applied(&self, lines: usize) -> Vec<String> {
        until(|| self.ingress.recent().len() >= lines);
        self.ingress.recent()
    }

    fn chat(&self, sid: &str) -> Option<Entry> {
        let id = format!("claude:{sid}");
        self.ingress.sessions().snapshot().into_iter().find(|e| e.id == id)
    }
}

/// Waits up to 10 s for `done`.
fn until(done: impl Fn() -> bool) {
    for _ in 0..1000 {
        if done() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("not done in 10 s");
}

/// What a chat shows (the times aside).
fn shows(e: &Entry) -> impl PartialEq + std::fmt::Debug {
    (
        e.state,
        e.detail.clone(),
        e.prop,
        e.failed,
        e.title.clone(),
        e.cwd.clone(),
        e.r#where.clone(),
        e.host_id.clone(),
        e.ended.is_some(),
    )
}

/// Every event the exe's registration reports, posted as Claude posts it, leaves the chat as the pipe leaves it for
/// the envelope the exe makes of the same input; the two headers land in `env`, so the chat runs in the desktop app
/// with its deep link.
#[test]
fn every_claude_event_over_http_gives_the_pipes_state() {
    let pet = Pet::start("events");
    let host = "local_0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
    let headers = [("X-AiPet-Entrypoint", "claude-desktop"), ("X-AiPet-Host-Session", host)];
    let pipe = AgentSessions::with_codex_home(pet.root.join("codex"));
    let bash = json!({"command": "cargo build"});
    let edit = json!({"file_path": "/w/src/lib.rs", "old_string": "a", "new_string": "b"});
    let events = [
        json!({"hook_event_name": "SessionStart", "source": "startup"}),
        json!({"hook_event_name": "UserPromptSubmit", "prompt": "Fix the build\nplease"}),
        json!({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": bash, "tool_use_id": "t1"}),
        json!({"hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": {"command": "cargo build", "description": "Build"}}),
        json!({"hook_event_name": "Notification", "notification_type": "permission_prompt", "message": "Claude needs your permission"}),
        json!({"hook_event_name": "PostToolUse", "tool_name": "Bash", "tool_input": bash, "tool_use_id": "t1", "tool_response": {"stdout": "x".repeat(300_000)}}),
        json!({"hook_event_name": "PreToolUse", "tool_name": "Edit", "tool_input": edit, "tool_use_id": "t2"}),
        json!({"hook_event_name": "PostToolUseFailure", "tool_name": "Edit", "tool_input": edit, "tool_use_id": "t2", "error": "no match"}),
        json!({"hook_event_name": "PermissionDenied", "tool_name": "Bash", "tool_input": {"command": "rm -rf /"}}),
        json!({"hook_event_name": "Elicitation", "mcp_server_name": "jira"}),
        json!({"hook_event_name": "ElicitationResult", "mcp_server_name": "jira"}),
        json!({"hook_event_name": "PreCompact", "trigger": "auto"}),
        json!({"hook_event_name": "PostCompact", "trigger": "auto"}),
        json!({"hook_event_name": "SubagentStart", "agent_type": "Explore"}),
        json!({"hook_event_name": "SubagentStop", "agent_type": "Explore"}),
        json!({"hook_event_name": "StopFailure", "error_type": "rate_limit"}),
        json!({"hook_event_name": "UserPromptSubmit", "prompt": "Again"}),
        json!({"hook_event_name": "Notification", "notification_type": "idle_prompt", "message": "Claude is waiting for your input"}),
        json!({"hook_event_name": "UserPromptSubmit", "prompt": "/compact"}),
        json!({"hook_event_name": "Stop", "stop_hook_active": false}),
        json!({"hook_event_name": "SessionEnd", "reason": "exit"}),
    ];
    let env = [
        ("CLAUDE_CODE_ENTRYPOINT", "claude-desktop"),
        ("CLAUDE_CODE_HOST_SESSION_ID", host),
    ];
    // the pipe's hooks start a second apart; the posts a few ms apart, never in the same ms, which would order them by
    // their place in a turn
    let start = SystemTime::now();
    for (n, mut event) in events.into_iter().enumerate() {
        thread::sleep(Duration::from_millis(2));
        event["session_id"] = json!("s1");
        event["cwd"] = json!("/w");
        let name = event["hook_event_name"].as_str().unwrap().to_owned();
        let reply = pet.hook(&headers, &event);
        assert_eq!((reply.status, reply.body.as_str()), (200, "{}"), "{name}");
        assert_eq!(reply.header("content-type"), Some("application/json"), "{name}");
        let recent = pet.applied(n + 1);

        // the pipe: the exe's envelope of the same input
        let Ok(Node::Object(payload)) = aipet_json::parse(&event.to_string(), event::READ_DEPTH) else {
            panic!()
        };
        let mut vars = Object::default();
        for (name, value) in env {
            vars.push(name, Node::String(value.into()));
        }
        let at = start + Duration::from_secs(n as u64);
        let line = event::envelope("claude", at, Some(4242), Some(vars), event::trimmed(payload).unwrap()).to_json();
        let applied = pipe.apply(&Envelope::parse(&line).unwrap(), unix_time(at));

        let outcome = recent[n].rsplit("-> ").next().unwrap();
        assert_eq!(outcome, applied.outcome, "{name}: {}", recent[n]);
        let by_pipe = pipe.snapshot().into_iter().find(|e| e.id == "claude:s1").unwrap();
        let by_http = pet.chat("s1").unwrap();
        assert_eq!(shows(&by_http), shows(&by_pipe), "{name}");
        if name == "SubagentStop" {
            assert_eq!(by_http.r#where.as_deref(), Some("desktop"));
            assert_eq!(by_http.host_id.as_deref(), Some(host));
            assert!(recent[n].contains("where=desktop"), "{}", recent[n]);
        }
    }
    assert!(pet.chat("s1").unwrap().ended.is_some(), "SessionEnd left its tombstone");
    assert!(pet.ingress.seen("claude"));
}

/// A body that isn't an event is answered as any other and logged; an unknown event is applied and ignored, and
/// makes no chat.
#[test]
fn malformed_bodies_and_unknown_events_get_200_and_a_log_line() {
    let pet = Pet::start("malformed");
    for body in [
        &b"{not json"[..],
        b"[1,2]",
        br#"{"hook_event_name":"Stop","a":1,"a":2}"#,
    ] {
        let reply = request(pet.port(), "POST", "/hook/claude", &[], body);
        assert_eq!((reply.status, reply.body.as_str()), (200, "{}"));
    }
    // one line, which counts the others within 10 s
    assert_eq!(
        pet.logged("mcp: dropped an event: Claude's hook sent what isn't JSON"),
        1,
        "{:?}",
        pet.log.lock().unwrap()
    );
    let reply = pet.hook(&[], &json!({"hook_event_name": "TeleportStart", "session_id": "u"}));
    assert_eq!((reply.status, reply.body.as_str()), (200, "{}"));
    let recent = pet.applied(1);
    assert!(
        recent[0].ends_with("TeleportStart u where=? -> ignored"),
        "{}",
        recent[0]
    );
    assert!(pet.chat("u").is_none());
}

/// Hooks of many chats posting at once all get their answer, and every chat ends as its own events left it: each
/// chat's events in its order, as one chat's hooks come one after another.
#[test]
fn parallel_posts_all_land_in_their_chats() {
    let pet = Arc::new(Pet::start("parallel"));
    let threads: Vec<_> = (0..24)
        .map(|i| {
            let pet = Arc::clone(&pet);
            thread::spawn(move || {
                let sid = format!("p{i}");
                for ev in ["UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop"] {
                    let event = json!({"hook_event_name": ev, "session_id": sid, "prompt": "Go",
                        "tool_name": "Read", "tool_input": {"file_path": "/w/a.rs"}});
                    assert_eq!(pet.hook(&[], &event).status, 200);
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    until(|| (0..24).all(|i| pet.chat(&format!("p{i}")).is_some_and(|c| c.state == Some("done"))));
    for i in 0..24 {
        let chat = pet.chat(&format!("p{i}")).unwrap();
        assert_eq!(
            (chat.state, chat.detail.as_deref()),
            (Some("done"), Some("Done")),
            "p{i}"
        );
    }
    assert_eq!(pet.logged("mcp: dropped"), 0);
}

// ------------------------------------------------------------------------------------------ R6: Codex's report_event

impl Pet {
    /// A Codex hook's `report_event` call: the tool's result.
    fn report(&self, arguments: Value) -> Value {
        let (status, reply) = self.stateless(
            "tools/call",
            Some("report_event"),
            json!({"name": "report_event", "arguments": arguments}),
        );
        assert_eq!(status, 200, "{reply}");
        reply["result"].clone()
    }

    fn codex_chat(&self, sid: &str) -> Option<Entry> {
        let id = format!("codex:{sid}");
        self.ingress.sessions().snapshot().into_iter().find(|e| e.id == id)
    }
}

/// Every event Codex's hooks report, called as an `mcp_tool` hook calls it (the fields each event's input table
/// has), leaves the chat as the pipe leaves it for the envelope the exe makes of the same input.
#[test]
fn every_codex_event_over_report_event_gives_the_pipes_state() {
    let pet = Pet::start("codex");
    let pipe = AgentSessions::with_codex_home(pet.root.join("codex"));
    let (t1, t2) = (
        "019a6b2c-1d00-7000-8000-000000000001",
        "019a6b2c-2e00-7000-8000-000000000002",
    );
    let bash = json!({"command": "cargo build"});
    let events = [
        json!({"hook_event_name": "SessionStart", "source": "startup"}),
        json!({"hook_event_name": "UserPromptSubmit", "turn_id": t1, "prompt": "Fix the build\nplease"}),
        json!({"hook_event_name": "PreToolUse", "turn_id": t1, "tool_name": "Bash", "tool_use_id": "call_1", "tool_input": bash}),
        json!({"hook_event_name": "PermissionRequest", "turn_id": t1, "tool_name": "Bash", "tool_input": bash}),
        json!({"hook_event_name": "PostToolUse", "turn_id": t1, "tool_name": "Bash", "tool_use_id": "call_1", "tool_input": bash,
            "tool_response": {"output": "x".repeat(300_000)}}),
        json!({"hook_event_name": "PreToolUse", "turn_id": t1, "tool_name": "apply_patch", "tool_use_id": "call_2",
            "tool_input": {"input": "*** Begin Patch\n*** Update File: src/lib.rs\n*** End Patch"}}),
        json!({"hook_event_name": "PostToolUse", "turn_id": t1, "tool_name": "apply_patch", "tool_use_id": "call_2",
            "tool_input": {"input": "*** Begin Patch\n*** Update File: src/lib.rs\n*** End Patch"}}),
        json!({"hook_event_name": "PreCompact"}),
        json!({"hook_event_name": "PostCompact"}),
        json!({"hook_event_name": "SubagentStart"}),
        json!({"hook_event_name": "SubagentStop"}),
        json!({"hook_event_name": "Stop", "turn_id": t1}),
        json!({"hook_event_name": "UserPromptSubmit", "turn_id": t2, "prompt": "Again"}),
        json!({"hook_event_name": "Interrupt"}),
        json!({"hook_event_name": "SessionEnd"}),
    ];
    let start = SystemTime::now();
    for (n, mut event) in events.into_iter().enumerate() {
        thread::sleep(Duration::from_millis(2));
        // the base fields every event's input table has; an ephemeral thread's transcript_path is null
        event["session_id"] = json!("x1");
        event["cwd"] = json!("/w");
        event["model"] = json!("gpt-5.3-codex");
        event["transcript_path"] = Value::Null;
        let name = event["hook_event_name"].as_str().unwrap().to_owned();
        let result = pet.report(json!({"agent": "codex", "event": event}));
        assert_eq!(result["isError"], false, "{name}: {result}");
        assert_eq!(result["structuredContent"], json!({}), "{name}");
        let recent = pet.applied(n + 1);

        // the pipe: the exe's envelope of the same input
        let Ok(Node::Object(payload)) = aipet_json::parse(&event.to_string(), event::READ_DEPTH) else {
            panic!()
        };
        let at = start + Duration::from_secs(n as u64);
        let line = event::envelope("codex", at, Some(4242), None, event::trimmed(payload).unwrap()).to_json();
        let applied = pipe.apply(&Envelope::parse(&line).unwrap(), unix_time(at));

        let outcome = recent[n].rsplit("-> ").next().unwrap();
        assert_eq!(outcome, applied.outcome, "{name}: {}", recent[n]);
        let by_pipe = pipe.snapshot().into_iter().find(|e| e.id == "codex:x1").unwrap();
        let by_http = pet.codex_chat("x1").unwrap();
        assert_eq!(shows(&by_http), shows(&by_pipe), "{name}");
        if name == "PermissionRequest" {
            assert_eq!(by_http.state, Some("attention"), "{name}");
        }
    }
    assert!(
        pet.codex_chat("x1").unwrap().ended.is_some(),
        "SessionEnd left its tombstone"
    );
    assert!(pet.ingress.seen("codex"));
    assert!(!pet.ingress.seen("claude"));
}

/// Another agent, an event Codex's chats don't take, or one without a session, is `isError` with the reason, and
/// changes nothing: no chat, no line in the record, no agent seen. So is an event that isn't an object, or none.
#[test]
fn an_invalid_agent_or_event_is_an_error_and_changes_nothing() {
    let pet = Pet::start("codex-invalid");
    for (what, arguments) in [
        (
            "Claude",
            json!({"agent": "claude", "event": {"session_id": "x", "hook_event_name": "Stop"}}),
        ),
        (
            "no agent",
            json!({"agent": "", "event": {"session_id": "x", "hook_event_name": "Stop"}}),
        ),
        (
            "an unknown event",
            json!({"agent": "codex", "event": {"session_id": "x", "hook_event_name": "Teleport"}}),
        ),
        (
            "Claude's event",
            json!({"agent": "codex", "event": {"session_id": "x", "hook_event_name": "Notification"}}),
        ),
        ("no event name", json!({"agent": "codex", "event": {"session_id": "x"}})),
        (
            "no session",
            json!({"agent": "codex", "event": {"hook_event_name": "UserPromptSubmit", "turn_id": "t"}}),
        ),
        // arguments rmcp can't read as the input schema's: rmcp answers an isError too
        (
            "an event that isn't an object",
            json!({"agent": "codex", "event": "Stop"}),
        ),
        ("no event", json!({"agent": "codex"})),
    ] {
        let result = pet.report(arguments);
        assert_eq!(result["isError"], true, "{what}: {result}");
        let reason = result["content"][0]["text"].as_str().unwrap_or("");
        assert!(!reason.is_empty(), "{what}: no reason: {result}");
    }
    // a valid event after them is the first the queue applied
    let ok =
        json!({"agent": "codex", "event": {"session_id": "y", "hook_event_name": "SessionStart", "source": "startup"}});
    assert_eq!(pet.report(ok)["isError"], false);
    let recent = pet.applied(1);
    assert_eq!(recent.len(), 1, "{recent:?}");
    assert!(recent[0].contains("SessionStart y"), "{}", recent[0]);
    let chats = pet.ingress.sessions().snapshot();
    assert_eq!(chats.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["codex:y"]);
    assert!(!pet.ingress.seen("claude"));
    assert_eq!(
        pet.logged("mcp: dropped an event: report_event refused it: agent is \"claude\""),
        1,
        "{:?}",
        pet.log.lock().unwrap()
    );
}
