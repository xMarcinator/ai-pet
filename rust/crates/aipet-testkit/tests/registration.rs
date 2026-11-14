//! Registering AiPet's hooks with Claude Code and Codex, ported from AiPet.Tests/RegistrationTests.cs: the
//! cases the C# ran in-process (ClaudeConfig and CodexConfig compiled into the tests) run the built hook here, as the
//! installers do (`aipet-hook --install|--uninstall|--doctor`), in a sandbox whose CLAUDE_CONFIG_DIR and CODEX_HOME are
//! its own. The cases the golden generator recorded from the C# are aipet-hook's own tests (tests/registration.rs,
//! tests/codex.rs, tests/doctor.rs); these are the ones that held the files' modes, the plugins and the doctor's
//! reading of them.

use std::fs;
use std::path::{Path, PathBuf};

use aipet_testkit::{Run, Sandbox, executable, hook, repo};
use serde_json::{Value, json};

/// A sandbox whose agents the hook registers with, the hook run in it with `args` (`RunHook`).
struct Agents {
    box_: Sandbox,
}

impl Agents {
    fn new() -> Agents {
        let mut box_ = Sandbox::new();
        box_.env.insert("SHELL".into(), "/bin/sh".into());
        Agents { box_ }
    }

    fn hook(&self, args: &[&str]) -> Run {
        self.box_.run(hook(), args, None)
    }

    /// Exit 0, as an installer runs it unchecked.
    fn ok(&self, args: &[&str]) -> Run {
        let r = self.hook(args);
        r.ok();
        r
    }

    fn settings(&self) -> PathBuf {
        self.box_.claude().join("settings.json")
    }

    fn config_toml(&self) -> PathBuf {
        self.box_.codex().join("config.toml")
    }

    fn hooks_json(&self) -> PathBuf {
        self.box_.codex().join("hooks.json")
    }

    /// Claude Code's record of an installed aipet plugin (enabling it in settings.json alone runs nothing).
    fn install_claude_plugin(&self) {
        let plugins = self.box_.claude().join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        fs::write(
            plugins.join("installed_plugins.json"),
            r#"{"version":2,"plugins":{"aipet@aipet":[{"scope":"user","version":"0.2.0"}]}}"#,
        )
        .unwrap();
    }

    /// The aipet plugin as `codex plugin add aipet@aipet` leaves it: in config.toml and in Codex's plugin cache.
    fn add_codex_plugin(&self) {
        let cache = self.box_.codex().join("plugins/cache/aipet/aipet/0.2.0");
        fs::create_dir_all(cache).unwrap();
        let toml = fs::read_to_string(self.config_toml()).unwrap_or_default();
        fs::write(
            self.config_toml(),
            toml + "\n[plugins.\"aipet@aipet\"]\nenabled = true\n",
        )
        .unwrap();
    }

    /// The files of a folder whose names end with `suffix`.
    fn ending(dir: &Path, suffix: &str) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().ends_with(suffix))
            .collect()
    }
}

fn text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[cfg(unix)]
fn chmod(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

const SAID_PLUGIN: &str = "plugin is enabled in";

// ------------------------------------------------------------------ file modes and atomic writes
/// A rewritten settings.json or config.toml keeps its mode: they often hold tokens, and Codex keeps config.toml 0600.
/// Also a mode the umask would narrow (group-writable).
#[cfg_attr(windows, ignore = "Unix modes only")]
#[test]
fn rewrites_keep_the_files_mode() {
    #[cfg(unix)]
    {
        let a = Agents::new();
        for wanted in [0o600, 0o660] {
            fs::write(a.settings(), r#"{"env":{"ANTHROPIC_API_KEY":"secret"}}"#).unwrap();
            chmod(&a.settings(), wanted);
            a.ok(&["--install", "claude"]);
            assert!(text(&a.settings()).contains("aipet-hook"));
            assert_eq!(mode(&a.settings()), wanted);
            a.ok(&["--uninstall", "claude"]);
            assert!(!text(&a.settings()).contains("aipet-hook"));
            assert_eq!(mode(&a.settings()), wanted);

            fs::write(
                a.config_toml(),
                "[mcp_servers.x]\ncommand = \"x\"\nenv = { TOKEN = \"secret\" }\n",
            )
            .unwrap();
            chmod(&a.config_toml(), wanted);
            a.ok(&["--install", "codex"]);
            assert!(text(&a.config_toml()).contains("aipet-hook"));
            assert_eq!(mode(&a.config_toml()), wanted);
            a.ok(&["--uninstall", "codex"]);
            assert!(!text(&a.config_toml()).contains("aipet-hook"));
            assert_eq!(mode(&a.config_toml()), wanted);
        }
        let left = [
            Agents::ending(&a.box_.claude(), ".aipet-tmp"),
            Agents::ending(&a.box_.codex(), ".aipet-tmp"),
        ];
        assert_eq!(left.concat(), Vec::<PathBuf>::new());
    }
}

#[cfg_attr(windows, ignore = "Unix modes only")]
#[test]
fn new_files_are_the_users_only() {
    #[cfg(unix)]
    {
        let a = Agents::new();
        a.ok(&["--install", "claude"]);
        assert_eq!(mode(&a.settings()), 0o600);
        a.ok(&["--install", "codex"]);
        assert_eq!(mode(&a.config_toml()), 0o600);
    }
}

/// A temp file left by an interrupted run is written anew, so it can't pass its own mode on.
#[cfg_attr(windows, ignore = "Unix modes only")]
#[test]
fn a_leftover_temp_file_doesnt_pass_its_mode_on() {
    #[cfg(unix)]
    {
        let a = Agents::new();
        fs::write(a.config_toml(), "model = \"o3\"\n").unwrap();
        chmod(&a.config_toml(), 0o600);
        let tmp = a.box_.codex().join("config.toml.aipet-tmp");
        fs::write(&tmp, "half a file").unwrap();
        chmod(&tmp, 0o646);
        a.ok(&["--install", "codex"]);
        assert_eq!(mode(&a.config_toml()), 0o600);
        assert!(text(&a.config_toml()).starts_with("model = \"o3\""));
        assert!(!tmp.exists());
    }
}

/// hooks.json (used when the user's other hooks are there) is replaced by a new file, not truncated and rewritten: a
/// reader that opened it before still reads it whole. It keeps its mode too.
#[cfg_attr(windows, ignore = "an open file can't be replaced there")]
#[test]
fn hooks_json_is_replaced_at_once() {
    #[cfg(unix)]
    {
        use std::io::Read;
        let a = Agents::new();
        let other = "{\n  \"hooks\": {\n    \"Stop\": [\n      {\n        \"hooks\": [\n          { \"type\": \"command\", \"command\": \"other-tool\" }\n        ]\n      }\n    ]\n  }\n}\n";
        fs::write(a.hooks_json(), other).unwrap();
        chmod(&a.hooks_json(), 0o600);
        {
            let mut reader = fs::File::open(a.hooks_json()).unwrap();
            a.ok(&["--install", "codex"]);
            let mut before = String::new();
            reader.read_to_string(&mut before).unwrap();
            assert_eq!(before, other);
        }
        let now = text(&a.hooks_json());
        assert!(now.contains("other-tool") && now.contains("aipet-hook"), "{now}");
        assert!(!fs::read_to_string(a.config_toml()).is_ok_and(|t| t.contains("aipet-hook")));
        assert_eq!(mode(&a.hooks_json()), 0o600);

        a.ok(&["--uninstall", "codex"]);
        let after = text(&a.hooks_json());
        assert!(!after.contains("aipet-hook") && after.contains("other-tool"), "{after}");
        assert_eq!(mode(&a.hooks_json()), 0o600);
        assert_eq!(Agents::ending(&a.box_.codex(), ".aipet-tmp"), Vec::<PathBuf>::new());
    }
}

// ------------------------------------------------------------------ the plugin
/// With the aipet plugin enabled and installed, hooks in settings.json would report every event a second time:
/// --install registers none, and takes out the ones registered before the plugin. It still succeeds, since the pet
/// gets every event and installers run it unchecked (install-from-source.sh would stop halfway). On Windows without
/// Git Bash the plugin can't run: install_claude_with_a_plugin_that_cant_run_registers.
#[cfg_attr(windows, ignore = "the plugin runs only with Git Bash there")]
#[test]
fn install_claude_with_the_plugin_enabled_registers_none() {
    let a = Agents::new();
    a.install_claude_plugin();
    let enabled = "{\n  \"enabledPlugins\": { \"aipet@aipet\": true }\n}\n";
    fs::write(a.settings(), enabled).unwrap();
    let r = a.ok(&["--install", "claude"]);
    assert!(r.stdout.contains(SAID_PLUGIN), "{}", r.output());
    assert_eq!(text(&a.settings()), enabled);
    // untouched, so no backup either
    assert_eq!(Agents::ending(&a.box_.claude(), ".bak"), Vec::<PathBuf>::new());

    fs::write(a.settings(), r#"{ "enabledPlugins": { "aipet@aipet": false } }"#).unwrap();
    a.ok(&["--install", "claude"]);
    assert!(text(&a.settings()).contains("aipet-hook"));
    // unchanged, and still fine
    a.ok(&["--install", "claude"]);

    // hooks from before the plugin was enabled go: the plugin replaces them
    let mut root: Value = serde_json::from_str(&text(&a.settings())).unwrap();
    root["enabledPlugins"]["aipet@aipet"] = json!(true);
    fs::write(a.settings(), root.to_string()).unwrap();
    let r = a.ok(&["--install", "claude"]);
    assert!(!text(&a.settings()).contains("aipet-hook"));
    assert!(r.stdout.contains("The aipet@aipet plugin"), "{}", r.output());
}

/// A settings.json synced from another machine can enable the plugin where it was never installed: nothing runs its
/// hooks, so --install registers its own.
#[test]
fn install_claude_with_the_plugin_enabled_but_not_installed_registers() {
    let a = Agents::new();
    fs::write(a.settings(), r#"{ "enabledPlugins": { "aipet@aipet": true } }"#).unwrap();
    let r = a.ok(&["--install", "claude"]);
    assert!(!r.stdout.contains(SAID_PLUGIN), "{}", r.output());
    assert!(text(&a.settings()).contains("aipet-hook"));
}

#[test]
fn install_codex_with_the_plugin_enabled_registers_none() {
    let a = Agents::new();
    fs::write(a.config_toml(), "model = \"o3\"\n").unwrap();
    a.add_codex_plugin();
    let before = text(&a.config_toml());
    a.ok(&["--install", "codex"]);
    assert_eq!(text(&a.config_toml()), before);
    assert!(!a.hooks_json().exists());
    assert_eq!(Agents::ending(&a.box_.codex(), ".bak"), Vec::<PathBuf>::new());

    // hooks from before the plugin was added go: the plugin replaces them
    fs::write(a.config_toml(), "model = \"o3\"\n").unwrap();
    a.ok(&["--install", "codex"]);
    a.add_codex_plugin();
    a.ok(&["--install", "codex"]);
    let after = text(&a.config_toml());
    assert!(!after.contains("aipet-hook"), "{after}");
    assert!(after.contains("[plugins.\"aipet@aipet\"]"), "{after}");
}

/// On Windows the plugin's hooks run in Git Bash. Without it they don't run, so --install registers its own.
#[cfg_attr(not(windows), ignore = "Windows only")]
#[test]
fn install_claude_with_a_plugin_that_cant_run_registers() {
    let mut a = Agents::new();
    let empty = a.box_.dir("nobash").display().to_string();
    for name in ["PATH", "ProgramFiles", "LOCALAPPDATA"] {
        a.box_.env.insert(name.into(), empty.clone());
    }
    a.box_.env.remove("CLAUDE_CODE_GIT_BASH_PATH");
    a.install_claude_plugin();
    fs::write(a.settings(), r#"{ "enabledPlugins": { "aipet@aipet": true } }"#).unwrap();
    let r = a.ok(&["--install", "claude"]);
    assert!(!r.stdout.contains(SAID_PLUGIN), "{}", r.output());
    assert!(text(&a.settings()).contains("aipet-hook"));
}

/// Codex runs the plugin's hooks while config.toml has its entry, not disabled, and its cache folder is there: then
/// --install registers none.
#[test]
fn codex_plugin_is_what_codex_runs() {
    for (toml, cache, runs) in [
        ("[plugins.\"aipet@aipet\"]\nenabled = true\n", true, true),
        // Codex takes it as enabled
        ("[plugins.\"aipet@aipet\"]\n", true, true),
        ("[plugins]\n\"aipet@aipet\".enabled = true  # on\n", true, true),
        ("plugins.\"aipet@aipet\".enabled = true\n", true, true),
        ("[plugins.\"aipet@aipet\"]\nenabled = false\n", true, false),
        ("[plugins]\n\"aipet@aipet\" = { enabled = false }\n", true, false),
        ("[plugins.\"aipet@aipet\"]\nenabled = true\n", false, false),
        ("[plugins.\"other@aipet\"]\nenabled = true\n", true, false),
        ("[profiles.x]\nplugins = 1\n", true, false),
        ("", true, false),
    ] {
        let a = Agents::new();
        if cache {
            fs::create_dir_all(a.box_.codex().join("plugins/cache/aipet/aipet/0.2.0")).unwrap();
        }
        fs::write(a.config_toml(), toml).unwrap();
        let r = a.ok(&["--install", "codex"]);
        assert_eq!(
            r.stdout.contains(SAID_PLUGIN),
            runs,
            "{toml:?} cache: {cache}: {}",
            r.output()
        );
        let registered = text(&a.config_toml()).contains("aipet-hook")
            || fs::read_to_string(a.hooks_json()).is_ok_and(|t| t.contains("aipet-hook"));
        assert_eq!(registered, !runs, "{toml:?} cache: {cache}");
    }
}

// ------------------------------------------------------------------ aipet-hook as a program
/// `dotnet aipet-hook.dll --install` would register dotnet itself: the hook refuses to register a program that isn't
/// aipet-hook (a copy of it under another name stands in for dotnet) and writes nothing. --uninstall works from there.
#[test]
fn install_under_another_name_refuses() {
    let a = Agents::new();
    let renamed = a
        .box_
        .dir("renamed")
        .join(format!("hook{}", std::env::consts::EXE_SUFFIX));
    fs::copy(hook(), &renamed).unwrap();
    for agent in ["claude", "codex"] {
        let r = a.box_.run(&renamed, &["--install", agent], None);
        assert_eq!(r.exit, Some(1), "{}", r.output());
        assert!(r.stderr.contains("not aipet-hook"), "{}", r.stderr);
        assert!(r.stderr.contains("Nothing was changed"), "{}", r.stderr);
        for dir in [a.box_.claude(), a.box_.codex(), a.box_.home()] {
            assert!(
                fs::read_dir(&dir).unwrap().next().is_none(),
                "{agent}: {} was written",
                dir.display()
            );
        }
    }
    let u = a.hook(&["--uninstall", "claude"]);
    assert_eq!((u.exit, u.stderr.as_str()), (Some(0), ""));
    assert_eq!(a.hook(&["--install", "cursor"]).exit, Some(2));
}

// ------------------------------------------------------------------ the doctor
/// A plugin from before the MCP server (fnt-71), as Codex caches it under the sandbox's CODEX_HOME: its exe hooks for
/// [`plugin_events`], the real launcher, and (with_hook) a hook binary for this system that runs the built hook. The
/// `aipet-hook --doctor` these tests check is the migration bridge's, which knows that plugin; today's plugin
/// registers `mcp_tool` hooks, which `AiPet --doctor codex` checks.
fn plugin_root(a: &Agents, with_hook: bool) -> PathBuf {
    let root = a.box_.codex().join("plugins/cache/aipet/aipet/0.2.0");
    let plugin = repo().join("plugins").join("aipet");
    fs::create_dir_all(root.join("hooks")).unwrap();
    let command =
        json!([{"hooks": [{"type": "command", "command": "sh \"$PLUGIN_ROOT/native/aipet-hook.sh\" --agent codex"}]}]);
    let hooks: serde_json::Map<String, Value> = plugin_events().into_iter().map(|e| (e, command.clone())).collect();
    fs::write(root.join("hooks/codex.json"), json!({ "hooks": hooks }).to_string()).unwrap();
    fs::create_dir_all(root.join("native")).unwrap();
    fs::copy(plugin.join("native/aipet-hook.sh"), root.join("native/aipet-hook.sh")).unwrap();
    if with_hook {
        for rid in ["linux-x64", "linux-arm64"] {
            let exe = format!("#!/bin/sh\nexec '{}' \"$@\"\n", hook().display());
            executable(&root.join("native").join(rid).join("aipet-hook"), &exe);
        }
    }
    root
}

/// The events of the old plugin's Codex hooks (its codex.json before fnt-71): the ones the bridge's doctor counts as
/// the whole plugin (`plugin_hooks::CODEX_EVENTS`).
fn plugin_events() -> Vec<String> {
    [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PermissionRequest",
        "Stop",
        "PreCompact",
        "PostCompact",
        "SubagentStart",
        "SubagentStop",
    ]
    .map(String::from)
    .to_vec()
}

/// `CodexConfig.Snake`: PreToolUse is pre_tool_use.
fn snake(event: &str) -> String {
    let mut out = String::new();
    for (i, c) in event.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// A hook as hooks/list gives it (Codex 0.157), trusted.
fn listed(event: &str, command: &str, plugin_root: Option<&Path>) -> Value {
    let (key, source_path, source, id) = match plugin_root {
        Some(root) => (
            format!("aipet@aipet:hooks/codex.json:{}:0:0", snake(event)),
            root.join("hooks").join("codex.json").display().to_string(),
            "plugin",
            json!("aipet@aipet"),
        ),
        None => (
            format!("config.toml:{}:0:0", snake(event)),
            "/x/config.toml".into(),
            "user",
            Value::Null,
        ),
    };
    let mut name = event.to_owned();
    name[..1].make_ascii_lowercase();
    json!({
        "key": key, "eventName": name, "handlerType": "command", "command": command, "async": true, "timeoutSec": 30,
        "sourcePath": source_path, "source": source, "pluginId": id, "enabled": true, "isManaged": false,
        "trustStatus": "trusted",
    })
}

/// The plugin's hooks as hooks/list gives them.
fn plugin_hooks(root: &Path) -> Vec<Value> {
    plugin_events()
        .iter()
        .map(|e| listed(e, "sh \"$PLUGIN_ROOT/native/aipet-hook.sh\" --agent codex", Some(root)))
        .collect()
}

/// A codex first on the sandbox's PATH whose app-server answers initialize, and hooks/list with these hooks.
fn fake_codex(a: &Agents, home: &Path, hooks: Vec<Value>) {
    let bin = a.box_.bin();
    let list = json!({"id": 2, "result": {"data": [{"cwd": home.display().to_string(), "hooks": hooks}]}});
    fs::write(bin.join("hooks-list.json"), format!("{list}\n")).unwrap();
    a.box_.stub(
        "codex",
        r#"case "$1" in
  --version) echo "codex-cli 0.0.0-test"; exit 0 ;;
  app-server) ;;
  *) exit 2 ;;
esac
while IFS= read -r line; do
  case "$line" in
    *'"id":1,'*) echo '{"id":1,"result":{}}' ;;
    *'"id":2,'*) while IFS= read -r l; do printf '%s\n' "$l"; done < "${0%/*}/hooks-list.json" ;;
  esac
done"#,
    );
}

/// Without hooks/list (no codex), an older install's AIPET-~1.EXE hook in config.toml is AiPet's.
#[cfg_attr(windows, ignore = "the doctor would find the user's codex.exe there")]
#[test]
fn doctor_knows_legacy_codex_hooks() {
    let a = Agents::new();
    fs::write(
        a.config_toml(),
        "[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ntype = \"command\"\ncommand = 'C:\\Users\\JOHNSM~1\\AppData\\Local\\AiPet\\hooks\\AIPET-~1.EXE --agent codex'\ntimeout = 30\n",
    )
    .unwrap();
    let r = a.hook(&["--doctor", "codex"]);
    assert!(r.stdout.contains("[ok]   Stop"), "{}", r.output());
    assert!(!r.stdout.contains("[FAIL]"), "{}", r.output());
    assert_eq!(r.exit, Some(0));
}

/// Without hooks/list, a plugin enabled in config.toml provides the hooks: no "not registered".
#[cfg_attr(windows, ignore = "the doctor would find the user's codex.exe there")]
#[test]
fn doctor_without_hooks_list_knows_the_plugin() {
    let a = Agents::new();
    fs::create_dir_all(a.box_.codex().join("plugins/cache/aipet/aipet/0.2.0")).unwrap();
    fs::write(a.config_toml(), "[plugins.\"aipet@aipet\"]\nenabled = true\n").unwrap();
    let r = a.hook(&["--doctor", "codex"]);
    assert!(r.stdout.contains("aipet@aipet plugin is enabled"), "{}", r.output());
    assert!(!r.stdout.contains("--install codex"), "{}", r.output());
    assert_eq!(r.exit, Some(0));

    // and hooks in config.toml as well: every event twice
    let toml = text(&a.config_toml())
        + "\n[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ntype = \"command\"\ncommand = \"'/x/aipet-hook' --agent codex\"\n";
    fs::write(a.config_toml(), toml).unwrap();
    let r = a.hook(&["--doctor", "codex"]);
    assert!(r.stdout.contains("reported twice"), "{}", r.output());
    assert!(r.stdout.contains("--uninstall codex"), "{}", r.output());
}

/// The plugin's events (the Windows set, on every OS) are all it needs, and its command is probed with PLUGIN_ROOT
/// set, as Codex runs it.
#[cfg_attr(windows, ignore = "a fake codex on PATH is a shell script")]
#[test]
fn doctor_checks_the_plugins_hooks_as_the_plugins() {
    let a = Agents::new();
    let root = plugin_root(&a, true);
    fake_codex(&a, &a.box_.root, plugin_hooks(&root));
    let r = a.hook(&["--doctor", "codex", "--probe"]);
    let all = format!(
        "hooks come from the aipet@aipet plugin, for all {} of its events",
        plugin_events().len()
    );
    assert!(r.stdout.contains(&all), "{}", r.output());
    assert!(!r.stdout.contains("--install codex"), "{}", r.output());
    assert!(!r.stdout.contains("[FAIL]"), "{}", r.output());
    assert!(
        r.stdout.contains("[ok]   sh: ran, printed nothing, exit 0"),
        "{}",
        r.output()
    );
    assert_eq!(r.exit, Some(0));
}

/// The events the hook knows as the plugin's (`CodexConfig.PluginEvents`) are the ones the plugin's codex.json
/// registers: the doctor takes exactly the file's events as the whole plugin, and one fewer as a plugin that misses
/// one. (--print-plugin-hooks codex printing the file byte for byte is aipet-hook's tests/registration.rs.)
#[cfg_attr(windows, ignore = "a fake codex on PATH is a shell script")]
#[test]
fn plugin_events_are_the_plugins_hooks() {
    let events = plugin_events();
    assert_eq!(events.len(), 9, "{events:?}");
    let a = Agents::new();
    let root = plugin_root(&a, true);
    fake_codex(&a, &a.box_.root, plugin_hooks(&root));
    let r = a.hook(&["--doctor", "codex"]);
    assert!(r.stdout.contains("for all 9 of its events"), "{}", r.output());

    let mut fewer = plugin_hooks(&root);
    fewer.pop();
    fake_codex(&a, &a.box_.root, fewer);
    let r = a.hook(&["--doctor", "codex"]);
    assert!(!r.stdout.contains("for all 9 of its events"), "{}", r.output());
}

#[cfg_attr(windows, ignore = "a fake codex on PATH is a shell script")]
#[test]
fn doctor_plugin_and_direct_hooks_are_reported_twice() {
    let a = Agents::new();
    let root = plugin_root(&a, true);
    let mut hooks = plugin_hooks(&root);
    // the direct hooks: every event the hook registers in config.toml on Unix (`CodexConfig.Events`)
    let direct = [
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
    ];
    hooks.extend(
        direct
            .iter()
            .map(|e| listed(e, "'/opt/aipet/hooks/aipet-hook' --agent codex", None)),
    );
    fake_codex(&a, &a.box_.root, hooks);
    let r = a.hook(&["--doctor", "codex"]);
    let twice =
        "the aipet@aipet plugin is also enabled, so every event is reported twice: run aipet-hook --uninstall codex";
    assert!(r.stdout.contains(twice), "{}", r.output());
    assert!(!r.stdout.contains("--install codex"), "{}", r.output());
}

/// The plugin's launcher quietly does nothing without a hook binary for this system: that's a problem, and isn't
/// probed as if it worked.
#[cfg_attr(windows, ignore = "a fake codex on PATH is a shell script")]
#[test]
fn doctor_plugin_without_a_hook_for_this_system_fails() {
    let a = Agents::new();
    let root = plugin_root(&a, false);
    fake_codex(&a, &a.box_.root, plugin_hooks(&root));
    let r = a.hook(&["--doctor", "codex", "--probe"]);
    assert!(
        r.stdout.contains("the plugin has no hook for this system"),
        "{}",
        r.output()
    );
    assert!(
        r.stdout.contains("skipped: the plugin has no hook for this system"),
        "{}",
        r.output()
    );
    assert_eq!(r.exit, Some(1));
}
