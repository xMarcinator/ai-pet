//! `aipet-hook --print-plugin-hooks claude|codex` (`AiPet.Hook/PluginHooks.cs`): the aipet plugin's hook file for
//! an agent (plugins/aipet/hooks/hooks.json or codex.json), made from the tables the direct hooks come from, byte for
//! byte as committed. CI compares the two (scripts/check-plugin.sh --hook), so the plugin can't drift from the code.
//! Codex trusts a hook by a hash of its definition, so a change to its file makes users trust the hooks again.
//! It isn't a hook run, so it may print.
//!
//! Since fnt-71 the hooks report to the pet's own listener: Claude's are HTTP hooks, Codex's `mcp_tool` hooks calling
//! `report_event` on the plugin's MCP server. The MCP files (`claude-mcp`: the plugin's `.mcp.json`; `codex-mcp`:
//! `hooks/codex-mcp.json`, which `.codex-plugin/plugin.json` names) register that server, with the token's header
//! from the plugin's own aipet-hook (`--print-token-header`).

use std::io::{self, Write};

use crate::claude::{self, EVENTS};
use crate::codex;
use crate::install::warn;
use crate::json_out;
use aipet_ipc::json::{Node, Object};

/// Frozen as the C# had it (tests/registration.rs); `claude-mcp` and `codex-mcp` are for scripts/check-plugin.sh.
pub(crate) const USAGE: &str = "usage: aipet-hook --print-plugin-hooks claude|codex";

/// `PluginHooks.Print(agent)`, `agent` in lower case: the file as the plugin has it, two spaces, LF on every OS and a
/// last LF, quotes and non-ASCII text as they are. The bytes themselves: no console encodes them.
pub(crate) fn print(agent: &str) -> i32 {
    let hooks = match agent {
        "claude" => claude(),
        "codex" => codex(),
        "claude-mcp" => claude_mcp(),
        "codex-mcp" => codex_mcp(),
        _ => {
            warn(USAGE);
            return 2;
        }
    };
    let text = json_out::indented(&Node::Object(hooks), "\n") + "\n";
    let mut stdout = io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes()).and_then(|()| stdout.flush());
    0
}

fn string(s: &str) -> Node {
    Node::String(s.to_owned())
}

/// The file: its description, then its hooks.
fn file(description: &str, hooks: Object) -> Object {
    let mut file = Object::default();
    file.push("description", string(description));
    file.push("hooks", Node::Object(hooks));
    file
}

/// The token's header helper for Claude: shell form through the launcher that picks the binary for the OS. Claude
/// expands `${CLAUDE_PLUGIN_ROOT}`.
const CLAUDE_HELPER: &str = "sh \"${CLAUDE_PLUGIN_ROOT}/native/aipet-hook.sh\" --print-token-header";

/// Codex runs its header helper with no arguments: a script next to the launcher. Codex expands `${PLUGIN_ROOT}`.
const CODEX_HELPER: &str = "${PLUGIN_ROOT}/native/token-header.sh";

/// Claude's events, each with the HTTP hook `AiPet --install claude` registers.
fn claude() -> Object {
    let _ = &EVENTS;
    let mut hooks = Object::default();
    for event in claude::http_events() {
        hooks.push(event.name, Node::Array(vec![claude::http_group(event)]));
    }
    file(
        "Report each Claude Code chat's status to the AiPet desktop pet, over HTTP (observe only: the pet never \
         answers Claude).",
        hooks,
    )
}

/// The events of the plugin's Codex hooks before fnt-71 (the exe's, Windows' set everywhere), which the exe's own
/// doctor (its C# goldens) still checks a plugin against.
pub(crate) const CODEX_EVENTS: [&str; 9] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "Stop",
    "PreCompact",
    "PostCompact",
    "SubagentStart",
    "SubagentStop",
];

/// The plugin's Codex hooks: the `mcp_tool` ones `AiPet --install codex` registers, the same on every OS.
fn codex() -> Object {
    let mut hooks = Object::default();
    for event in codex::MCP_EVENTS {
        let mut group = Object::default();
        group.push("hooks", Node::Array(vec![codex::json_mcp_handler(event)]));
        hooks.push(event.name, Node::Array(vec![Node::Object(group)]));
    }
    file(
        "Report each Codex chat's status to the AiPet desktop pet, through its MCP server (observe only).",
        hooks,
    )
}

/// An MCP file: `{"mcpServers": {"aipet": server}}`.
fn servers(server: Object) -> Object {
    let mut servers = Object::default();
    servers.push(codex::SERVER, Node::Object(server));
    let mut file = Object::default();
    file.push("mcpServers", Node::Object(servers));
    file
}

/// The plugin's `.mcp.json`, for Claude.
fn claude_mcp() -> Object {
    let mut server = Object::default();
    server.push("type", string("http"));
    server.push("url", Node::String(claude::mcp_url()));
    server.push("headersHelper", string(CLAUDE_HELPER));
    servers(server)
}

/// The plugin's `hooks/codex-mcp.json`, for Codex: config.toml's `[mcp_servers.aipet]` keys.
fn codex_mcp() -> Object {
    let mut server = Object::default();
    server.push("url", Node::String(claude::mcp_url()));
    server.push("http_headers_helper", string(CODEX_HELPER));
    servers(server)
}
