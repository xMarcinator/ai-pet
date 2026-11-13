//! `AiPet --install|--uninstall claude|codex` and `--print-token-header` (fnt-71 R12): the agents report to the pet's
//! own listener, with no hook process per event.
//!
//! - Claude: HTTP hooks in settings.json for every event but SessionStart ([`claude::http_events`]), and the pet's MCP
//!   server in the user's `.claude.json` (`mcpServers.aipet`, as `claude mcp add --scope user` keeps it), whose token
//!   header comes from `AiPet --print-token-header`.
//! - Codex: `[mcp_servers.aipet]` and `mcp_tool` hooks calling its `report_event` in config.toml ([`codex::Wanted`]).
//!   Codex runs its header helper with no arguments, so it is a small script in the pet's data folder that runs
//!   `AiPet --print-token-header`.
//!
//! Either install takes out the exe's command hooks (`aipet-hook --install`'s, which the old installers made), and
//! uninstall takes out both kinds. A file that isn't valid JSON or TOML is left as it is, with a message. The exe's
//! own `--install` stays as the C# had it (its goldens), for one release, as does the exe.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::claude::{self, Vars};
use crate::codex::{self, Wanted};
use crate::install::{backup, is_file, read_text, real_path, save, say, warn};
use crate::json_out::{self, deep_equals, read};
use aipet_ipc::json::{Node, Object};

pub(crate) const USAGE: &str = "usage: AiPet --install|--uninstall|--doctor claude|codex";

/// The MCP server's name in both agents' configs.
pub(crate) const SERVER: &str = codex::SERVER;

/// AiPet's modes, or `None` for arguments that aren't one.
pub(crate) fn mode(args: &[OsString], exe: &Path) -> Option<i32> {
    let first = args.first()?.to_str()?;
    if first == "--print-token-header" {
        return Some(print_token_header());
    }
    if !matches!(first, "--install" | "--uninstall" | "--doctor") {
        return None;
    }
    let agent = args.get(1).map(|a| a.to_string_lossy().to_lowercase());
    let Some(agent) = agent.filter(|a| a == "claude" || a == "codex") else {
        warn(USAGE);
        return Some(2);
    };
    if first == "--doctor" {
        return Some(crate::mcp_doctor::run(&agent));
    }
    let install = first == "--install";
    let (out, err) = (&mut |line: &str| say(line), &mut |line: &str| warn(line));
    let data = aipet_ipc::paths::data_dir();
    let done = match (agent.as_str(), install) {
        ("claude", true) => install_claude(exe, &claude::process_vars, out),
        ("claude", false) => uninstall_claude(&claude::process_vars, out),
        (_, true) => install_codex(exe, data, out, err),
        _ => uninstall_codex(data, out, err),
    };
    Some(done.unwrap_or_else(|message| {
        warn(&format!("Couldn't update {agent} settings: {message}"));
        1
    }))
}

// ------------------------------------------------------------------ the token's header
/// `--print-token-header`: `{"Authorization":"Bearer <token>"}` from the pet's token file, or `{}` when there is
/// none yet (the pet makes it at its first start). Always 0: an agent starting while the pet is down gets on with it.
pub(crate) fn print_token_header() -> i32 {
    let token = fs::read_to_string(aipet_ipc::paths::mcp_token()).unwrap_or_default();
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{}", token_header(token.trim()));
    0
}

/// The header object for `token`.
pub(crate) fn token_header(token: &str) -> String {
    // the pet's tokens are hex: anything else (a damaged file) would need escaping, and isn't a token
    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return "{}".to_owned();
    }
    format!(r#"{{"Authorization":"Bearer {token}"}}"#)
}

/// How Claude runs the header helper: `exe` with `--print-token-header`, through a shell (`/` on every OS).
pub(crate) fn claude_helper(exe: &Path) -> String {
    format!("\"{}\" --print-token-header", exe.to_string_lossy().replace('\\', "/"))
}

// ------------------------------------------------------------------ Claude
/// Claude's user config, which holds user-scope MCP servers: `$CLAUDE_CONFIG_DIR/.claude.json`, else
/// `~/.claude.json`.
pub(crate) fn claude_json(vars: Vars) -> PathBuf {
    match vars("CLAUDE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join(".claude.json"),
        _ => aipet_ipc::paths::home().join(".claude.json"),
    }
}

/// The MCP entry Claude gets.
pub(crate) fn claude_server(exe: &Path) -> Node {
    let mut server = Object::default();
    server.push("type", Node::String("http".into()));
    server.push("url", Node::String(claude::mcp_url()));
    server.push("headersHelper", Node::String(claude_helper(exe)));
    Node::Object(server)
}

pub(crate) fn install_claude(exe: &Path, vars: Vars, say: &mut dyn FnMut(&str)) -> Result<i32, String> {
    let dir = claude::dir(vars);
    let settings = dir.join("settings.json");
    let users = claude_json(vars);
    // both files read first: one that isn't JSON stops the install before anything is written
    let root = parse_json(&settings)?;
    parse_json(&users)?;
    let root = match &root {
        Some(Node::Object(root)) => Some(root),
        _ => None,
    };
    let (shown, shown_users) = (settings.display(), users.display());
    if let Some(plugin) = claude::plugin(&dir, root)?
        && claude::plugin_runs(vars)
    {
        let removed = claude::edit_as(&settings, None)? | set_server(&users, None)?;
        say(&if removed {
            format!(
                "The {plugin} plugin is enabled in {shown} and reports every event, so the AiPet hooks and MCP server \
                 registered earlier were removed (the plugin replaces them)."
            )
        } else {
            format!("The {plugin} plugin is enabled in {shown} and already reports every event: nothing to register.")
        });
        say(&format!(
            "To register them in settings.json instead, disable the plugin (claude plugin disable {plugin}) and \
             install again."
        ));
        return Ok(0);
    }
    let hooks = claude::edit_as(&settings, Some(&claude::Wanted::Http))?;
    let server = set_server(&users, Some(claude_server(exe)))?;
    say(&if hooks {
        format!("AiPet's HTTP hooks registered for Claude Code ({shown})")
    } else {
        format!("AiPet's HTTP hooks for Claude Code are already registered ({shown}, unchanged)")
    });
    say(&if server {
        format!("AiPet's MCP server registered for Claude Code ({shown_users})")
    } else {
        format!("AiPet's MCP server for Claude Code is already registered ({shown_users}, unchanged)")
    });
    say("Claude Code sessions started before this pick it up when they restart.");
    say("Check with: AiPet --doctor claude");
    Ok(0)
}

pub(crate) fn uninstall_claude(vars: Vars, say: &mut dyn FnMut(&str)) -> Result<i32, String> {
    let settings = claude::settings(vars);
    let users = claude_json(vars);
    parse_json(&settings)?;
    parse_json(&users)?;
    let changed = claude::edit_as(&settings, None)? | set_server(&users, None)?;
    say(if changed {
        "AiPet hooks and MCP server removed from Claude Code"
    } else {
        "No AiPet hooks or MCP server were registered with Claude Code"
    });
    Ok(0)
}

/// A JSON file as it is (`None` when it isn't there or is blank), or why it is left alone.
fn parse_json(path: &Path) -> Result<Option<Node>, String> {
    if !is_file(path) {
        return Ok(None);
    }
    let text = read_text(path)?;
    if text.chars().all(char::is_whitespace) {
        return Ok(None);
    }
    json_out::parse(&text)
        .map(Some)
        .map_err(|e| format!("{} isn't valid JSON ({e}); it was left as it is", path.display()))
}

/// Sets (or with `None` removes) `mcpServers.aipet` in Claude's user config, keeping everything else. Whether it
/// changed the file.
pub(crate) fn set_server(path: &Path, server: Option<Node>) -> Result<bool, String> {
    let Some(before) = parse_json(path)? else {
        if server.is_none() {
            return Ok(false);
        }
        let mut root = Object::default();
        let mut servers = Object::default();
        servers.push(SERVER, server.expect("checked above"));
        root.push("mcpServers", Node::Object(servers));
        write_json(path, &Node::Object(root), "")?;
        return Ok(true);
    };
    let Node::Object(mut root) = before.clone() else {
        return Err(format!("{} isn't a JSON object; it was left as it is", path.display()));
    };
    read(&root)?;
    match (root.get_mut("mcpServers"), server) {
        (Some(Node::Object(servers)), Some(server)) => match servers.get_mut(SERVER) {
            Some(old) => *old = server,
            None => servers.push(SERVER, server),
        },
        (Some(Node::Object(servers)), None) => {
            servers.remove(SERVER);
        }
        (Some(_), Some(_)) => {
            return Err(format!(
                "{}'s mcpServers isn't an object; it was left as it is",
                path.display()
            ));
        }
        (None, Some(server)) => {
            let mut servers = Object::default();
            servers.push(SERVER, server);
            root.push("mcpServers", Node::Object(servers));
        }
        (_, None) => return Ok(false),
    }
    let after = Node::Object(root);
    if deep_equals(&before, &after)? {
        return Ok(false);
    }
    write_json(path, &after, &read_text(path).unwrap_or_default())?;
    Ok(true)
}

/// Writes `root` as `before` was laid out (its line endings), with a backup of what was there.
fn write_json(path: &Path, root: &Node, before: &str) -> Result<(), String> {
    let nl = if before.contains('\r') { "\r\n" } else { "\n" };
    let mut text = json_out::indented(root, nl);
    if before.is_empty() || before.ends_with('\n') {
        text.push_str(nl);
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| crate::install::thrown(&e, dir))?;
    }
    if is_file(path) {
        backup(path);
    }
    save(&real_path(path), &text)
}

// ------------------------------------------------------------------ Codex
/// The script Codex runs for the token's header: `<data>/mcp-header` (`.cmd` on Windows), which runs
/// `AiPet --print-token-header`.
pub(crate) fn codex_helper(data: &Path) -> PathBuf {
    data.join(if cfg!(windows) { "mcp-header.cmd" } else { "mcp-header" })
}

/// The helper's text, running `exe`.
pub(crate) fn codex_helper_text(exe: &Path) -> String {
    if cfg!(windows) {
        format!("@\"{}\" --print-token-header\r\n", exe.display())
    } else {
        format!(
            "#!/bin/sh\nexec '{}' --print-token-header\n",
            exe.to_string_lossy().replace('\'', "'\\''")
        )
    }
}

/// Writes the helper when it isn't there as it should be.
fn write_codex_helper(path: &Path, exe: &Path) -> Result<(), String> {
    let text = codex_helper_text(exe);
    if fs::read_to_string(path).is_ok_and(|t| t == text) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| crate::install::thrown(&e, dir))?;
    }
    save(path, &text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| crate::install::thrown(&e, path))?;
    }
    Ok(())
}

/// config.toml, when it is valid TOML, else why it is left alone.
fn check_toml(path: &Path) -> Result<(), String> {
    if !is_file(path) {
        return Ok(());
    }
    let text = read_text(path)?;
    text.parse::<toml::Table>().map(|_| ()).map_err(|e| {
        format!(
            "{} isn't valid TOML ({}); it was left as it is",
            path.display(),
            e.message()
        )
    })
}

pub(crate) fn install_codex(
    exe: &Path,
    data: &Path,
    say: &mut dyn FnMut(&str),
    warn: &mut dyn FnMut(&str),
) -> Result<i32, String> {
    let home = aipet_ipc::paths::codex_home();
    let config = home.join("config.toml");
    check_toml(&config)?;
    let helper = codex_helper(data);
    write_codex_helper(&helper, exe)?;
    let url = claude::mcp_url();
    let helper_text = helper.to_string_lossy().into_owned();
    let wanted = Wanted::Mcp {
        url: &url,
        helper: &helper_text,
    };
    let changed = match codex::install_as(&wanted, say, warn)? {
        Ok(changed) => changed,
        Err(code) => return Ok(code),
    };
    let shown = config.display();
    say(&if changed {
        format!("AiPet's MCP server and hooks registered for Codex in {shown}")
    } else {
        format!("AiPet's MCP server and hooks for Codex are already registered in {shown} (unchanged)")
    });
    let events: Vec<&str> = codex::MCP_EVENTS.iter().map(|e| e.name).collect();
    say(&format!("  events:  {}", events.join(", ")));
    if changed {
        say("");
        say("Codex runs new hooks only after you trust them, once:");
        say("  1. In a terminal, run: codex");
        say("  2. At \"Hooks need review\" choose Review hooks and trust the AiPet ones (report_event on aipet).");
        say("     \"Trust all\" would also trust any other hooks waiting for review. Later: /hooks.");
        say("  3. Restart the ChatGPT/Codex desktop app so it picks up the trust.");
    }
    say("Then check with: AiPet --doctor codex");
    Ok(0)
}

pub(crate) fn uninstall_codex(
    data: &Path,
    say: &mut dyn FnMut(&str),
    warn: &mut dyn FnMut(&str),
) -> Result<i32, String> {
    check_toml(&aipet_ipc::paths::codex_home().join("config.toml"))?;
    let code = codex::uninstall(say, warn)?;
    let _ = fs::remove_file(codex_helper(data));
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_is_the_token_or_nothing() {
        assert_eq!(token_header("abc123"), r#"{"Authorization":"Bearer abc123"}"#);
        assert_eq!(token_header(""), "{}");
        assert_eq!(token_header("two words"), "{}");
    }

    #[test]
    fn the_helpers_run_aipet() {
        let exe = Path::new(if cfg!(windows) {
            r"C:\Program Files\AiPet\AiPet.exe"
        } else {
            "/opt/it's/AiPet"
        });
        if cfg!(windows) {
            assert_eq!(
                claude_helper(exe),
                "\"C:/Program Files/AiPet/AiPet.exe\" --print-token-header"
            );
        } else {
            assert_eq!(claude_helper(exe), "\"/opt/it's/AiPet\" --print-token-header");
            assert_eq!(
                codex_helper_text(exe),
                "#!/bin/sh\nexec '/opt/it'\\''s/AiPet' --print-token-header\n"
            );
        }
    }
}
