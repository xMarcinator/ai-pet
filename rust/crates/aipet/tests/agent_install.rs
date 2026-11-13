//! `AiPet --install|--uninstall|--doctor claude|codex` and `--print-token-header` (fnt-71 R12), run as a user runs
//! them, on agents' configs in a folder of the test's own (`HOME`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
//! `AIPET_DATA_DIR`): the old exe hooks go and the new HTTP hooks and MCP server come, the user's own entries stay,
//! a second install changes nothing, uninstall takes out both kinds, a file that isn't JSON or TOML is left alone, and
//! the doctor tells a running pet from a stopped one.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;

use aipet_core::control::PetLink;
use aipet_core::server::Ingress;
use aipet_core::sessions::AgentSessions;
use serde_json::{Value, json};

const AIPET: &str = env!("CARGO_BIN_EXE_AiPet");

/// The test's own folder: home, Claude's and Codex's configs and the pet's data.
struct Sandbox(PathBuf);

impl Sandbox {
    fn new(test: &str) -> Sandbox {
        let root = std::env::temp_dir().join(format!("aipet-agent-install-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["claude", "codex", "data", "bin"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        Sandbox(root)
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }

    /// AiPet with these arguments, in the sandbox, on `port` for the doctor, with no codex on PATH.
    fn run(&self, args: &[&str], port: u16) -> Output {
        Command::new(AIPET)
            .args(args)
            .env("HOME", &self.0)
            .env("CLAUDE_CONFIG_DIR", self.path("claude"))
            .env("CODEX_HOME", self.path("codex"))
            .env("AIPET_DATA_DIR", self.path("data"))
            .env("AIPET_PORT", port.to_string())
            .env("PATH", self.path("bin"))
            .output()
            .unwrap()
    }

    fn json(&self, rel: &str) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.path(rel)).unwrap()).unwrap()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A port nothing listens on.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

const OLD_CLAUDE: &str = r#"{
  "model": "opus",
  "hooks": {
    "Stop": [
      { "hooks": [ { "type": "command", "command": "/old/AiPet/aipet-hook", "args": ["--agent", "claude"], "timeout": 5 } ] },
      { "hooks": [ { "type": "command", "command": "notify-send done" } ] }
    ],
    "SessionStart": [
      { "hooks": [ { "type": "command", "command": "/old/AiPet/aipet-hook", "args": ["--agent", "claude"], "timeout": 5, "async": true } ] }
    ]
  }
}
"#;

#[test]
fn claude_install_replaces_the_exe_hooks_and_uninstall_takes_out_both_kinds() {
    let sb = Sandbox::new("claude");
    std::fs::write(sb.path("claude/settings.json"), OLD_CLAUDE).unwrap();
    std::fs::write(
        sb.path("claude/.claude.json"),
        r#"{"numStartups": 3, "mcpServers": {"other": {"type": "stdio", "command": "x"}}}"#,
    )
    .unwrap();
    let port = free_port();

    let out = sb.run(&["--install", "claude"], port);
    assert!(out.status.success(), "{}", text(&out));
    let settings = sb.json("claude/settings.json");
    assert_eq!(settings["model"], "opus");
    let hooks = settings["hooks"].as_object().unwrap();
    // no exe hook anywhere, the user's own kept, an HTTP hook for every event but SessionStart
    let all = settings["hooks"].to_string();
    assert!(!all.contains("aipet-hook"), "{all}");
    assert!(all.contains("notify-send done"));
    assert!(!hooks.contains_key("SessionStart"), "{all}");
    assert_eq!(hooks.len(), 16, "{all}");
    let stop = &hooks["Stop"];
    let http = stop
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["hooks"][0]["type"] == "http")
        .unwrap();
    assert_eq!(http["hooks"][0]["url"], "http://127.0.0.1:47615/hook/claude");
    assert_eq!(http["hooks"][0]["timeout"], 2);
    assert_eq!(
        http["hooks"][0]["allowedEnvVars"],
        json!(["CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_HOST_SESSION_ID"])
    );
    assert!(http["hooks"][0].get("async").is_none());
    assert_eq!(hooks["PreToolUse"][0]["matcher"], "*");
    let users = sb.json("claude/.claude.json");
    assert_eq!(users["numStartups"], 3);
    assert_eq!(users["mcpServers"]["other"]["command"], "x");
    let server = &users["mcpServers"]["aipet"];
    assert_eq!(server["type"], "http");
    assert_eq!(server["url"], "http://127.0.0.1:47615/mcp");
    assert_eq!(
        server["headersHelper"],
        format!("\"{}\" --print-token-header", Path::new(AIPET).display())
    );

    // again: nothing changes
    let before = (
        std::fs::read(sb.path("claude/settings.json")).unwrap(),
        std::fs::read(sb.path("claude/.claude.json")).unwrap(),
    );
    let out = sb.run(&["--install", "claude"], port);
    assert!(text(&out).contains("already registered"), "{}", text(&out));
    assert_eq!(std::fs::read(sb.path("claude/settings.json")).unwrap(), before.0);
    assert_eq!(std::fs::read(sb.path("claude/.claude.json")).unwrap(), before.1);

    let out = sb.run(&["--uninstall", "claude"], port);
    assert!(out.status.success(), "{}", text(&out));
    let settings = sb.json("claude/settings.json");
    assert_eq!(
        settings["hooks"],
        json!({"Stop": [{"hooks": [{"type": "command", "command": "notify-send done"}]}]})
    );
    let users = sb.json("claude/.claude.json");
    assert!(users["mcpServers"].get("aipet").is_none());
    assert_eq!(users["mcpServers"]["other"]["command"], "x");
}

const OLD_CODEX: &str = "model = \"gpt-5\"\n\n[mcp_servers.docs]\nurl = \"https://example.com/mcp\"\n\n\
[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ntype = \"command\"\ncommand = \"notify-send done\"\n\n\
# >>> AiPet hooks (written by aipet-hook --install codex; Codex asks you to trust them again if they change) >>>\n\
[[hooks.SessionStart]]\n[[hooks.SessionStart.hooks]]\ntype = \"command\"\ncommand = \"'/old/AiPet/aipet-hook' --agent codex\"\ntimeout = 30\nasync = true\n\n\
[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ntype = \"command\"\ncommand = \"'/old/AiPet/aipet-hook' --agent codex\"\ntimeout = 30\nasync = true\n\n\
# <<< AiPet hooks <<<\n";

#[test]
fn codex_install_replaces_the_exe_hooks_and_uninstall_takes_out_both_kinds() {
    let sb = Sandbox::new("codex");
    std::fs::write(sb.path("codex/config.toml"), OLD_CODEX).unwrap();
    let port = free_port();

    let out = sb.run(&["--install", "codex"], port);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("trust them"), "{}", text(&out));
    let config = std::fs::read_to_string(sb.path("codex/config.toml")).unwrap();
    assert!(!config.contains("aipet-hook"), "{config}");
    // the user's own, where they were
    assert!(config.starts_with("model = \"gpt-5\"\n\n[mcp_servers.docs]\nurl = \"https://example.com/mcp\"\n\n[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ntype = \"command\"\ncommand = \"notify-send done\"\n"), "{config}");
    let table: toml::Table = config.parse().unwrap();
    let helper = sb.path("data/mcp-header");
    assert_eq!(
        table["mcp_servers"]["aipet"]["url"].as_str(),
        Some("http://127.0.0.1:47615/mcp")
    );
    assert_eq!(
        table["mcp_servers"]["aipet"]["http_headers_helper"].as_str(),
        Some(helper.to_str().unwrap())
    );
    let hooks = table["hooks"].as_table().unwrap();
    let events = [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PermissionRequest",
        "PostToolUse",
        "Stop",
        "PreCompact",
        "PostCompact",
        "SubagentStart",
        "SubagentStop",
        "Interrupt",
    ];
    for event in events {
        let handler = hooks[event]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|g| g["hooks"].as_array().unwrap())
            .find(|h| h["type"].as_str() == Some("mcp_tool"))
            .unwrap_or_else(|| panic!("no mcp_tool hook for {event}: {config}"));
        assert_eq!(handler["server"].as_str(), Some("aipet"));
        assert_eq!(handler["tool"].as_str(), Some("report_event"));
        assert_eq!(handler["input"]["agent"].as_str(), Some("codex"));
        assert_eq!(handler["input"]["event"]["session_id"].as_str(), Some("${session_id}"));
    }
    assert_eq!(hooks.len(), events.len());
    // the helper runs AiPet for the header, in the bare environment Codex gives it: no AIPET_DATA_DIR, so the
    // helper has to name the data folder itself
    std::fs::write(sb.path("data/mcp-token"), "t0ken\n").unwrap();
    let header = Command::new(&helper)
        .env_clear()
        .env("HOME", &sb.0)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&header.stdout).trim(),
        r#"{"Authorization":"Bearer t0ken"}"#
    );

    let before = std::fs::read(sb.path("codex/config.toml")).unwrap();
    let out = sb.run(&["--install", "codex"], port);
    assert!(text(&out).contains("already registered"), "{}", text(&out));
    assert_eq!(std::fs::read(sb.path("codex/config.toml")).unwrap(), before);

    let out = sb.run(&["--uninstall", "codex"], port);
    assert!(out.status.success(), "{}", text(&out));
    let config = std::fs::read_to_string(sb.path("codex/config.toml")).unwrap();
    assert_eq!(
        config,
        "model = \"gpt-5\"\n\n[mcp_servers.docs]\nurl = \"https://example.com/mcp\"\n\n[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ntype = \"command\"\ncommand = \"notify-send done\"\n"
    );
    assert!(!helper.exists());
}

#[test]
fn a_file_that_isnt_json_or_toml_is_left_alone_with_a_message() {
    let sb = Sandbox::new("invalid");
    let port = free_port();
    std::fs::write(sb.path("claude/settings.json"), "{ not json").unwrap();
    let out = sb.run(&["--install", "claude"], port);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("Couldn't update claude settings"), "{}", text(&out));
    assert_eq!(
        std::fs::read_to_string(sb.path("claude/settings.json")).unwrap(),
        "{ not json"
    );
    assert!(!sb.path("claude/.claude.json").exists());

    std::fs::write(sb.path("claude/settings.json"), "{}").unwrap();
    std::fs::write(sb.path("claude/.claude.json"), "[1,").unwrap();
    let out = sb.run(&["--install", "claude"], port);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("isn't valid JSON"), "{}", text(&out));
    assert_eq!(std::fs::read_to_string(sb.path("claude/settings.json")).unwrap(), "{}");

    std::fs::write(sb.path("codex/config.toml"), "model = \n[[[").unwrap();
    for args in [["--install", "codex"], ["--uninstall", "codex"]] {
        let out = sb.run(&args, port);
        assert_eq!(out.status.code(), Some(1), "{}", text(&out));
        assert!(text(&out).contains("isn't valid TOML"), "{}", text(&out));
        assert_eq!(
            std::fs::read_to_string(sb.path("codex/config.toml")).unwrap(),
            "model = \n[[["
        );
    }
}

#[test]
fn the_doctor_tells_a_running_pet_from_a_stopped_one() {
    let sb = Sandbox::new("doctor");
    let port = free_port();
    assert!(sb.run(&["--install", "claude"], port).status.success());
    assert!(sb.run(&["--install", "codex"], port).status.success());

    // stopped
    let out = sb.run(&["--doctor", "claude"], port);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("the pet doesn't answer"), "{}", text(&out));
    assert!(
        text(&out).contains("ok    HTTP hooks for every event"),
        "{}",
        text(&out)
    );
    assert!(
        text(&out).contains("ok    the MCP server aipet is registered"),
        "{}",
        text(&out)
    );

    // running: a listener on the port with the data folder's token
    let ingress = Arc::new(Ingress::new(
        Arc::new(AgentSessions::with_codex_home(sb.path("codex"))),
        || {},
        sb.path("hook-events.log"),
        Box::new(|_| {}),
    ));
    let pet: PetLink = Arc::new(|_| false);
    let listener = aipet_mcp::Listener::start(
        aipet_mcp::Options {
            port,
            token_file: sb.path("data/mcp-token"),
            version: "9.9.9-test".into(),
            log: Arc::new(|_| {}),
            pet,
            data: sb.path("data"),
            home: sb.0.clone(),
            answer_within: aipet_core::control::ANSWER_WITHIN,
        },
        ingress,
    )
    .unwrap();
    let out = sb.run(&["--doctor", "claude"], port);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(text(&out).contains("ok    the pet answers"), "{}", text(&out));
    assert!(text(&out).contains("9.9.9-test"), "{}", text(&out));
    assert!(text(&out).contains("no events from claude's hooks"), "{}", text(&out));

    // Codex: trust can't be asked without codex on PATH, which is a warning, not a failure
    let out = sb.run(&["--doctor", "codex"], port);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        text(&out).contains("ok    mcp_tool hooks for every event"),
        "{}",
        text(&out)
    );
    assert!(text(&out).contains("warn  couldn't ask Codex"), "{}", text(&out));
    drop(listener);

    // the token's header, from the data folder
    let out = sb.run(&["--print-token-header"], port);
    let token = std::fs::read_to_string(sb.path("data/mcp-token")).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!(r#"{{"Authorization":"Bearer {}"}}"#, token.trim())
    );
}
