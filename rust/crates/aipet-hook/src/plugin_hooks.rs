//! `aipet-hook --print-plugin-hooks claude|codex` (`src/AiPet.Hook/PluginHooks.cs`): the aipet plugin's hook file for
//! an agent (plugins/aipet/hooks/hooks.json or codex.json), made from the tables the direct hooks come from, byte for
//! byte as committed. CI compares the two (scripts/check-plugin.sh --hook), so the plugin can't drift from the code.
//! Codex's file is frozen (Codex trusts a hook by a hash of its definition): this has to print it as it is, never the
//! other way round. It isn't a hook run, so it may print.

use std::io::{self, Write};

use crate::claude::{self, EVENTS};
use crate::install::warn;
use crate::json::{Node, Object};
use crate::json_out;

pub(crate) const USAGE: &str = "usage: aipet-hook --print-plugin-hooks claude|codex";

/// `PluginHooks.Print(agent)`, `agent` in lower case: the file as the plugin has it, two spaces, LF on every OS and a
/// last LF, quotes and non-ASCII text as they are. The bytes themselves: no console encodes them.
pub(crate) fn print(agent: &str) -> i32 {
    let hooks = match agent {
        "claude" => claude(),
        "codex" => codex(),
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

/// Shell form through bash (`"shell": "bash"`, so Claude never falls back to PowerShell), with the launcher that
/// picks the binary for the OS. Claude expands `${CLAUDE_PLUGIN_ROOT}`.
const CLAUDE_COMMAND: &str = "sh \"${CLAUDE_PLUGIN_ROOT}/native/aipet-hook.sh\" --agent claude";

/// Claude's events, each with the group the direct hooks are registered in, but with the plugin's command.
fn claude() -> Object {
    let mut hooks = Object::default();
    for event in &EVENTS {
        let mut hook = Object::default();
        hook.push("type", string("command"));
        hook.push("command", string(CLAUDE_COMMAND));
        hook.push("shell", string("bash"));
        hooks.push(event.name, Node::Array(vec![claude::group(event, hook)]));
    }
    file(
        "Report each Claude Code chat's status to the AiPet desktop pet (observe only: the hook prints nothing and \
         never answers Claude).",
        hooks,
    )
}

/// `CodexConfig.PluginEvents`: the events of the plugin's Codex hooks. A plugin can't pick them per OS, so it has the
/// Windows set everywhere. Frozen with the plugin's hook definitions.
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

/// The shell Codex starts expands `$PLUGIN_ROOT`. On Windows Codex runs `commandWindows` with PowerShell instead: the
/// exe itself, no Git Bash.
const CODEX_COMMAND: &str = "sh \"$PLUGIN_ROOT/native/aipet-hook.sh\" --agent codex";
const CODEX_COMMAND_WINDOWS: &str = r"& (Join-Path $env:PLUGIN_ROOT 'native\win-x64\aipet-hook.exe') --agent codex";

/// Each of [`CODEX_EVENTS`] with the handler `CodexConfig.JsonHandler` writes in a hooks.json, plus
/// `commandWindows`. All run in the background with 30 s, on every OS: frozen with the plugin's definitions, so not
/// taken from the direct hooks' events, which may change per OS.
fn codex() -> Object {
    let mut hooks = Object::default();
    for event in CODEX_EVENTS {
        let mut handler = Object::default();
        handler.push("type", string("command"));
        handler.push("command", string(CODEX_COMMAND));
        handler.push("commandWindows", string(CODEX_COMMAND_WINDOWS));
        handler.push("timeout", Node::Number("30".into()));
        handler.push("async", Node::Bool(true));
        let mut group = Object::default();
        group.push("hooks", Node::Array(vec![Node::Object(handler)]));
        hooks.push(event, Node::Array(vec![Node::Object(group)]));
    }
    file(
        "Report each Codex chat's status to the AiPet desktop pet (observe only: the hook prints nothing).",
        hooks,
    )
}
