//! `AiPet --doctor claude|codex` (fnt-71 R12): whether the pet's listener answers (its `status` tool, with the token),
//! whether the agent's registrations are AiPet's HTTP or MCP ones, whether the exe's old hooks are still there, and
//! for Codex whether it trusts AiPet's hooks (a warning: Codex skips untrusted hooks quietly, nothing breaks). Exit 1
//! when something is wrong, 0 otherwise.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use crate::claude;
use crate::codex;
use crate::install::{is_file, read_text, say};
use crate::json_out::{self, member};
use crate::mcp_install::{SERVER, claude_json};
use aipet_ipc::json::Node;

/// The listener's port: `AIPET_PORT` when it is one (as the pet takes it, for tests), else `MCP_PORT`.
fn port() -> u16 {
    std::env::var("AIPET_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(aipet_ipc::protocol::MCP_PORT)
}

/// What the doctor found, line by line, and whether anything failed.
struct Report {
    failed: bool,
}

impl Report {
    fn section(&self, name: &str) {
        say("");
        say(&format!("== {name} =="));
    }
    fn ok(&self, line: &str) {
        say(&format!("  ok    {line}"));
    }
    fn warn(&self, line: &str) {
        say(&format!("  warn  {line}"));
    }
    fn fail(&mut self, line: &str) {
        self.failed = true;
        say(&format!("  FAIL  {line}"));
    }
    fn info(&self, line: &str) {
        say(&format!("  info  {line}"));
    }
}

pub(crate) fn run(agent: &str) -> i32 {
    let mut r = Report { failed: false };
    say(&format!(
        "AiPet doctor for {}",
        if agent == "claude" { "Claude Code" } else { "Codex" }
    ));
    let seen = pet(&mut r, agent);
    if agent == "claude" {
        claude_registrations(&mut r);
    } else {
        codex_registrations(&mut r);
    }
    match seen {
        Some(true) => r.info(&format!("the pet got events from {agent}'s hooks since it started")),
        Some(false) => r.info(&format!(
            "no events from {agent}'s hooks since the pet started (start or use a chat to see one)"
        )),
        None => {}
    }
    i32::from(r.failed)
}

/// The listener and the token: whether `status` answers. Whether the agent's hooks reported, when it does.
fn pet(r: &mut Report, agent: &str) -> Option<bool> {
    r.section("Pet");
    let token_file = aipet_ipc::paths::mcp_token();
    let token = std::fs::read_to_string(&token_file).unwrap_or_default();
    let token = token.trim();
    if token.is_empty() {
        r.fail(&format!(
            "no token in {} (the pet makes it when it starts)",
            token_file.display()
        ));
    } else {
        r.ok(&format!("the token is in {}", token_file.display()));
    }
    match status(token) {
        Err(why) => {
            r.fail(&format!("the pet doesn't answer on 127.0.0.1:{}: {why}", port()));
            r.info("is AiPet running? Agents report nothing while it is closed, and wait for nothing either");
            None
        }
        Ok(status) => {
            let version = member(&status, "version")
                .ok()
                .flatten()
                .and_then(as_str)
                .unwrap_or("?");
            r.ok(&format!("the pet answers on 127.0.0.1:{} (AiPet {version})", port()));
            let seen = member(&status, "registrations_seen")
                .ok()
                .flatten()
                .and_then(|s| member(s, agent).ok().flatten().cloned());
            Some(seen == Some(Node::Bool(true)))
        }
    }
}

fn as_str(node: &Node) -> Option<&str> {
    match node {
        Node::String(s) => Some(s),
        _ => None,
    }
}

/// The pet's `status`, over MCP with the token (a stateless 2026-07-28 request), or why not.
fn status(token: &str) -> Result<Node, String> {
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"status","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"aipet-doctor","version":"1"},"io.modelcontextprotocol/clientCapabilities":{}}}}"#;
    let port = port();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: \
         application/json\r\nAccept: application/json, text/event-stream\r\nMCP-Protocol-Version: 2026-07-28\r\n\
         Mcp-Method: tools/call\r\nMcp-Name: status\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    let mut reply = Vec::new();
    let _ = stream.take(1 << 20).read_to_end(&mut reply);
    let reply = String::from_utf8_lossy(&reply);
    let code = reply.split(' ').nth(1).unwrap_or("");
    match code {
        "200" => {}
        "401" => return Err("it refused the token (another user's pet on the port, or an old token)".into()),
        "" => return Err("it closed the connection without answering".into()),
        other => return Err(format!("it answered HTTP {other}")),
    }
    let body = reply.split_once("\r\n\r\n").map_or("", |(_, b)| b);
    // JSON, or an event stream with it on a data line
    let json = body
        .lines()
        .find_map(|l| l.strip_prefix("data:").map(str::trim))
        .filter(|l| l.starts_with('{'))
        .or_else(|| body.find('{').map(|at| body[at..].trim()))
        .ok_or("its answer had no JSON")?;
    let reply = json_out::parse(json).map_err(|e| format!("its answer isn't JSON: {e}"))?;
    let result = member(&reply, "result")?.ok_or("its answer has no result")?;
    member(result, "structuredContent")?
        .cloned()
        .ok_or_else(|| "its status had no structuredContent".to_owned())
}

// ------------------------------------------------------------------ Claude
fn claude_registrations(r: &mut Report) {
    r.section("Claude Code");
    let vars = &claude::process_vars;
    let settings = claude::settings(vars);
    let root = if is_file(&settings) {
        match read_text(&settings).and_then(|t| json_out::parse(&t)) {
            Ok(root) => Some(root),
            Err(e) => {
                r.fail(&format!("{} isn't valid JSON: {e}", settings.display()));
                return;
            }
        }
    } else {
        None
    };
    let object = match &root {
        Some(Node::Object(o)) => Some(o),
        _ => None,
    };
    let plugin = claude::plugin(&claude::dir(vars), object).ok().flatten();
    let (mut http, mut exe) = (Vec::new(), 0);
    if let Some(Node::Object(hooks)) = object.and_then(|o| o.get("hooks")) {
        for (event, groups) in hooks.iter() {
            let Node::Array(groups) = groups else { continue };
            for group in groups {
                let Ok(Some(Node::Array(list))) = member(group, "hooks") else {
                    continue;
                };
                for hook in list {
                    if claude::is_http_ours(hook).unwrap_or(false) {
                        http.push(event.to_owned());
                    } else if claude::is_ours(hook).unwrap_or(false) {
                        exe += 1;
                    }
                }
            }
        }
    }
    let missing: Vec<&str> = claude::http_events()
        .map(|e| e.name)
        .filter(|e| !http.iter().any(|h| h == e))
        .collect();
    match (&plugin, missing.is_empty()) {
        (_, true) => r.ok(&format!("HTTP hooks for every event in {}", settings.display())),
        (Some(plugin), false) if http.is_empty() => {
            r.ok(&format!("the {plugin} plugin is enabled and brings its own hooks"));
        }
        (_, false) if http.is_empty() => r.fail(&format!(
            "no AiPet HTTP hooks in {}: run AiPet --install claude",
            settings.display()
        )),
        _ => r.fail(&format!(
            "AiPet's HTTP hooks miss {}: run AiPet --install claude",
            missing.join(", ")
        )),
    }
    if exe > 0 {
        r.warn(&format!(
            "{exe} of the old aipet-hook hooks are still in {}: run AiPet --install claude to replace them",
            settings.display()
        ));
    }
    let users = claude_json(vars);
    let server = read_text(&users)
        .ok()
        .and_then(|t| json_out::parse(&t).ok())
        .and_then(|root| member(&root, "mcpServers").ok().flatten().cloned())
        .and_then(|servers| member(&servers, SERVER).ok().flatten().cloned());
    match (server, &plugin) {
        (Some(server), _) => {
            let url = member(&server, "url")
                .ok()
                .flatten()
                .and_then(as_str)
                .unwrap_or_default()
                .to_owned();
            if url == claude::mcp_url() {
                r.ok(&format!("the MCP server {SERVER} is registered in {}", users.display()));
            } else {
                r.fail(&format!(
                    "the MCP server {SERVER} in {} has the URL {url:?}, not {}: run AiPet --install claude",
                    users.display(),
                    claude::mcp_url()
                ));
            }
        }
        (None, Some(plugin)) => r.ok(&format!("the {plugin} plugin brings the MCP server")),
        (None, None) => r.fail(&format!(
            "no MCP server {SERVER} in {}: run AiPet --install claude",
            users.display()
        )),
    }
}

// ------------------------------------------------------------------ Codex
fn codex_registrations(r: &mut Report) {
    r.section("Codex");
    let home = aipet_ipc::paths::codex_home();
    let config = home.join("config.toml");
    let text = read_text(&config).unwrap_or_default();
    if let Err(e) = text.parse::<toml::Table>() {
        r.fail(&format!("{} isn't valid TOML: {}", config.display(), e.message()));
        return;
    }
    let plugin = codex::enabled_plugin().ok().flatten();
    let table: toml::Table = text.parse().unwrap_or_default();
    let server = table
        .get("mcp_servers")
        .and_then(|s| s.get(SERVER))
        .and_then(|s| s.get("url"))
        .and_then(toml::Value::as_str);
    match (server, &plugin) {
        (Some(url), _) if url == claude::mcp_url() => {
            r.ok(&format!("the MCP server {SERVER} is in {}", config.display()));
        }
        (Some(url), _) => r.fail(&format!(
            "the MCP server {SERVER} in {} has the URL {url:?}: run AiPet --install codex",
            config.display()
        )),
        (None, Some(plugin)) => r.ok(&format!("the {plugin} plugin brings the MCP server")),
        (None, None) => r.fail(&format!(
            "no [mcp_servers.{SERVER}] in {}: run AiPet --install codex",
            config.display()
        )),
    }
    let hooks: Vec<String> = table
        .get("hooks")
        .and_then(toml::Value::as_table)
        .map(|events| {
            events
                .iter()
                .filter(|(_, groups)| {
                    groups.as_array().is_some_and(|groups| {
                        groups.iter().any(|g| {
                            g.get("hooks").and_then(toml::Value::as_array).is_some_and(|list| {
                                list.iter().any(|h| {
                                    h.get("type").and_then(toml::Value::as_str) == Some("mcp_tool")
                                        && h.get("server").and_then(toml::Value::as_str) == Some(SERVER)
                                })
                            })
                        })
                    })
                })
                .map(|(event, _)| event.clone())
                .collect()
        })
        .unwrap_or_default();
    let missing: Vec<&str> = codex::MCP_EVENTS
        .iter()
        .map(|e| e.name)
        .filter(|e| !hooks.iter().any(|h| h == e))
        .collect();
    match (&plugin, missing.is_empty()) {
        (_, true) => r.ok(&format!("mcp_tool hooks for every event in {}", config.display())),
        (Some(plugin), false) if hooks.is_empty() => r.ok(&format!("the {plugin} plugin brings its own hooks")),
        (_, false) if hooks.is_empty() => r.fail(&format!(
            "no AiPet hooks in {}: run AiPet --install codex",
            config.display()
        )),
        _ => r.fail(&format!(
            "AiPet's hooks miss {}: run AiPet --install codex",
            missing.join(", ")
        )),
    }
    let exe = codex::all_handlers()
        .iter()
        .filter(|h| codex::is_ours(Some(&h.command)))
        .count();
    if exe > 0 {
        r.warn(&format!(
            "{exe} of the old aipet-hook hooks are still registered: run AiPet --install codex to replace them"
        ));
    }
    trust(r);
}

/// Whether Codex trusts AiPet's hooks, from Codex itself (`codex app-server`, `hooks/list`). Untrusted hooks are a
/// warning: Codex skips them quietly until the user trusts them.
fn trust(r: &Report) {
    let Some(listed) = crate::doctor::hooks_list_on_path() else {
        r.warn("couldn't ask Codex (codex app-server hooks/list), so whether it trusts the hooks is unknown");
        return;
    };
    let ours: Vec<&Node> = listed
        .iter()
        .filter(|h| {
            let text = |k: &str| member(h, k).ok().flatten().and_then(as_str).map(str::to_owned);
            text("handlerType").as_deref() == Some("mcp_tool")
                && json_out::to_string(h, "\n").contains(&format!("\"{SERVER}\""))
        })
        .collect();
    if ours.is_empty() {
        r.warn("Codex lists none of AiPet's hooks yet");
        return;
    }
    let untrusted: Vec<String> = ours
        .iter()
        .filter(|h| {
            let trust = member(h, "trustStatus")
                .ok()
                .flatten()
                .and_then(as_str)
                .unwrap_or_default();
            trust == "untrusted" || trust == "modified"
        })
        .map(|h| {
            member(h, "eventName")
                .ok()
                .flatten()
                .and_then(as_str)
                .unwrap_or("?")
                .to_owned()
        })
        .collect();
    if untrusted.is_empty() {
        r.ok(&format!("Codex trusts AiPet's {} hooks", ours.len()));
    } else {
        r.warn(&format!(
            "Codex doesn't trust AiPet's hooks for {} yet, and skips them: run codex and choose Review hooks (or \
             /hooks), once",
            untrusted.join(", ")
        ));
    }
}
