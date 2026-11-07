//! `--doctor claude|codex [--probe]` (`src/AiPet.Hook/Doctor.cs`): checks that the agent will really run AiPet's
//! hook, without changing anything. It reads the agent's settings and the organisation's, asks Codex which hooks it
//! trusts (`codex app-server`, or else reads Codex's files), flags hook commands that can't work, and runs AiPet's
//! hook with a harmless test event: directly for Claude (as Claude does), and for Codex only with --probe, since that
//! means starting PowerShell or sh the way Codex does. Exit code 1 when something is broken.
//!
//! What it prints is the C#'s, line for line: tests/doctor.rs replays tests/golden/doctor, which rust/golden's
//! `doctor` mode wrote through the built C# hook. Where the C# stops with an unhandled exception instead (a
//! settings.json or hooks.json that names a member twice, a `hooks/list` entry whose fields aren't strings, a
//! config.toml or plugin cache it may not read), the port carries on: the settings.json is reported as unreadable,
//! the hooks.json is left out as one that isn't JSON is (config.toml's hooks still count), and the rest is read as if
//! the odd part weren't there. Writing the test event to a program that already quit isn't a failure either (the C#
//! reports the broken pipe as a start failure, depending on timing).

use std::collections::hash_map::RandomState;
use std::ffi::{OsStr, OsString};
use std::hash::BuildHasher;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use aipet_ipc::connect::connect;
use aipet_ipc::protocol::{CONNECT, KIND, OK, PID, RECENT, V, VERSION};

use crate::claude::{self, Vars};
use crate::codex;
use crate::event::decoded;
use crate::install::{NEWLINE, file_not_found, is_file, say, thrown};
use crate::json::{Node, Object};
use crate::json_out::{self, member, read};
use crate::plugin_hooks::CODEX_EVENTS;

/// `Doctor.Run`'s usage, for an agent it doesn't know.
pub(crate) const USAGE: &str = "usage: aipet-hook --doctor claude|codex";

const NOT_REGISTERED: &str = "AiPet's hooks aren't registered: add the aipet plugin (codex plugin marketplace add \
                              xMarcinator/ai-pet, then codex plugin add aipet@aipet), or run aipet-hook --install codex";

/// `Doctor.Run(agent, probe)`, `agent` in lower case: what it finds, a line at a time, and the verdict's exit code.
pub(crate) fn run(agent: &str, probe: bool) -> i32 {
    let mut out = |line: &str| say(line);
    let mut doctor = Doctor::new(&claude::process_vars, Machine::this(), probe, &mut out);
    match agent {
        "claude" => doctor.claude(),
        "codex" => doctor.codex(),
        _ => {
            crate::install::warn(USAGE);
            return 2;
        }
    }
    doctor.verdict()
}

/// The places on this machine the doctor looks at that no variable moves: the organisation's Claude settings, and the
/// PowerShell that Codex's Windows fallback finds. A test gives its own.
struct Machine {
    managed: Vec<PathBuf>,
    /// `C:\Program Files\PowerShell\7\pwsh.exe`, where PowerShell 7 is when it isn't on PATH.
    #[cfg_attr(not(windows), allow(dead_code))]
    pwsh: PathBuf,
    /// What runs when neither is there: Windows PowerShell, found as Windows finds a program.
    #[cfg_attr(not(windows), allow(dead_code))]
    powershell: OsString,
}

impl Machine {
    fn this() -> Machine {
        Machine {
            managed: managed_claude_settings(),
            pwsh: PathBuf::from(r"C:\Program Files\PowerShell\7\pwsh.exe"),
            powershell: "powershell.exe".into(),
        }
    }
}

/// `ManagedClaudeSettings`: where an organisation puts the Claude Code settings that override the user's.
fn managed_claude_settings() -> Vec<PathBuf> {
    if cfg!(windows) {
        vec![
            PathBuf::from(r"C:\Program Files\ClaudeCode\managed-settings.json"),
            PathBuf::from(r"C:\ProgramData\ClaudeCode\managed-settings.json"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![PathBuf::from(
            "/Library/Application Support/ClaudeCode/managed-settings.json",
        )]
    } else {
        vec![PathBuf::from("/etc/claude-code/managed-settings.json")]
    }
}

/// What the doctor has found so far, and where it says it.
struct Doctor<'a> {
    vars: Vars<'a>,
    machine: Machine,
    probe: bool,
    out: &'a mut dyn FnMut(&str),
    fails: usize,
    warns: usize,
}

/// A program the doctor runs: its file and arguments, how long it may take, and what to set in its environment.
struct Run<'r> {
    file: &'r OsStr,
    args: Args,
    timeout: u64,
    env: &'r [(&'static str, String)],
}

/// A program's arguments: a list, each quoted as the platform needs, or (Windows) the rest of the command line as it
/// is (`ProcessStartInfo.Arguments`).
enum Args {
    List(Vec<String>),
    #[cfg(windows)]
    Raw(String),
}

impl Args {
    fn list(args: &[&str]) -> Args {
        Args::List(args.iter().map(|&a| a.to_owned()).collect())
    }

    /// How the doctor shows them.
    fn shown(&self) -> String {
        match self {
            Args::List(list) => list.join(" "),
            #[cfg(windows)]
            Args::Raw(raw) => raw.clone(),
        }
    }
}

impl<'a> Doctor<'a> {
    fn new(vars: Vars<'a>, machine: Machine, probe: bool, out: &'a mut dyn FnMut(&str)) -> Doctor<'a> {
        Doctor {
            vars,
            machine,
            probe,
            out,
            fails: 0,
            warns: 0,
        }
    }

    fn ok(&mut self, s: &str) {
        (self.out)(&format!("  [ok]   {s}"));
    }

    fn info(&mut self, s: &str) {
        (self.out)(&format!("         {s}"));
    }

    fn warn(&mut self, s: &str) {
        self.warns += 1;
        (self.out)(&format!("  [warn] {s}"));
    }

    fn fail(&mut self, s: &str) {
        self.fails += 1;
        (self.out)(&format!("  [FAIL] {s}"));
    }

    /// A failure, or where `only_warn`, a warning.
    fn bad(&mut self, only_warn: bool, s: &str) {
        if only_warn {
            self.warn(s);
        } else {
            self.fail(s);
        }
    }

    fn section(&mut self, s: &str) {
        (self.out)(&format!("{NEWLINE}{s}"));
    }

    /// The last line, and the exit code: 1 when something is broken.
    fn verdict(&mut self) -> i32 {
        let summary = if self.fails > 0 {
            format!("{} problem(s), {} warning(s).", self.fails, self.warns)
        } else if self.warns > 0 {
            format!("No problems; {} warning(s).", self.warns)
        } else {
            "All good.".to_owned()
        };
        self.section(&summary);
        i32::from(self.fails > 0)
    }

    // ------------------------------------------------------------------ Claude Code
    fn claude(&mut self) {
        let settings = claude::settings(self.vars);
        self.section(&format!("Claude Code settings ({})", settings.display()));
        let root = match fs_read(&settings) {
            Ok(bytes) => match json_out::parse(&decoded(&bytes)) {
                Ok(Node::Object(root)) => root,
                // JSON, but not an object: nothing more to check, and nothing to report either
                Ok(_) => return,
                Err(message) => return self.fail(&format!("settings.json can't be read: {message}")),
            },
            Err(ReadError::NotFound(_)) => {
                return self.fail("settings.json doesn't exist: run aipet-hook --install claude");
            }
            Err(ReadError::Other(message)) => return self.fail(&format!("settings.json can't be read: {message}")),
        };
        let exe = match self.claude_hooks(&root, &claude::dir(self.vars)) {
            Ok(Some(exe)) => exe,
            Ok(None) => return,
            // where the C# stops, on what reading the settings throws
            Err(message) => return self.fail(&format!("settings.json can't be read: {message}")),
        };

        self.section("Policies set by your organisation");
        let managed: Vec<PathBuf> = self.machine.managed.iter().filter(|p| is_file(p)).cloned().collect();
        for path in &managed {
            let shown = path.display();
            let policy = fs_read(path)
                .map_err(ReadError::into_message)
                .and_then(|bytes| json_out::parse(&decoded(&bytes)))
                .and_then(|m| match m {
                    Node::Object(m) => {
                        read(&m)?;
                        Ok(Some(m))
                    }
                    _ => Ok(None),
                });
            match policy {
                Ok(m) if is_true(m.as_ref().and_then(|m| m.get("allowManagedHooksOnly"))) => {
                    self.fail(&format!(
                        "{shown}: allowManagedHooksOnly is on, so hooks in your own settings never run"
                    ));
                }
                Ok(m) if is_true(m.as_ref().and_then(|m| m.get("disableAllHooks"))) => {
                    self.fail(&format!("{shown}: disableAllHooks is on"));
                }
                Ok(_) => self.ok(&format!("{shown} allows your hooks")),
                Err(message) => self.warn(&format!("{shown} can't be read: {message}")),
            }
        }
        if managed.is_empty() {
            self.ok("no managed settings on this machine");
        }

        let running = self.check_aipet();
        self.section("Running the hook the way Claude does");
        match exe {
            None => self.info(
                "skipped: the plugin runs its own copy of the hook from Claude's plugin folder (the events the pet got \
                 show below)",
            ),
            Some(exe) if is_file(Path::new(&exe)) => {
                let run = Run {
                    file: OsStr::new(&exe),
                    args: Args::list(&["--agent", "claude"]),
                    timeout: 5,
                    env: &[],
                };
                self.run_probe(&run, running, false, 1000);
            }
            Some(_) => {}
        }
        self.recent_events("claude");
    }

    /// Where AiPet's hooks come from in settings.json, said as it is checked: the hook to probe (`None` when the plugin
    /// brings its own), or `None` outside it when a problem ends the check there.
    fn claude_hooks(&mut self, root: &Object, dir: &Path) -> Result<Option<Option<String>>, String> {
        read(root)?;
        if is_true(root.get("disableAllHooks")) {
            self.fail("\"disableAllHooks\": true switches every hook off");
        }
        let mut ours: Vec<(&str, &Node)> = Vec::new();
        if let Some(Node::Object(hooks)) = root.get("hooks") {
            read(hooks)?;
            for (event, value) in hooks.iter() {
                let Node::Array(groups) = value else { continue };
                for group in groups.iter().filter(|g| matches!(g, Node::Object(_))) {
                    let Some(Node::Array(list)) = member(group, "hooks")? else {
                        continue;
                    };
                    for hook in list.iter().filter(|h| matches!(h, Node::Object(_))) {
                        if claude::is_ours(hook)? {
                            ours.push((event, hook));
                        }
                    }
                }
            }
        }
        // the AiPet plugin brings its own hooks (plugins/aipet/hooks/hooks.json), so settings.json needs none
        let plugin = claude::plugin(dir, Some(root))?.is_some();
        let runs = claude::plugin_runs(self.vars);
        if ours.is_empty() && plugin && runs {
            self.ok("hooks come from the AiPet plugin (enabledPlugins), not from settings.json");
            return Ok(Some(None));
        }
        if ours.is_empty() && plugin {
            self.fail(
                "the AiPet plugin's hooks need Git Bash, which wasn't found: install Git for Windows, or run aipet-hook \
                 --install claude",
            );
            return Ok(None);
        }
        if ours.is_empty() {
            self.fail("AiPet's hooks aren't registered: run aipet-hook --install claude");
            return Ok(None);
        }
        let missing: Vec<&str> = claude::EVENTS
            .iter()
            .map(|e| e.name)
            .filter(|e| !ours.iter().any(|(event, _)| event == e))
            .collect();
        if missing.is_empty() {
            self.ok(&format!("registered for all {} events", claude::EVENTS.len()));
        } else {
            self.warn(&format!(
                "missing events: {} (run aipet-hook --install claude)",
                missing.join(", ")
            ));
        }
        // a command that isn't a string is its JSON text, as IsOurs read it
        let exe = member(ours[0].1, "command")?.map(|c| json_out::to_string(c, NEWLINE));
        let shown = exe.clone().unwrap_or_default();
        if exe.as_deref().is_some_and(|exe| is_file(Path::new(exe))) {
            self.ok(&format!("hook: {shown}"));
        } else {
            self.fail(&format!("the registered hook doesn't exist: {shown}"));
        }
        if plugin && runs {
            self.warn("the AiPet plugin is also enabled, so every event is reported twice");
        } else if plugin {
            self.warn(
                "the AiPet plugin is also enabled, but its hooks need Git Bash, which wasn't found: once Git for \
                 Windows is installed, run aipet-hook --uninstall claude",
            );
        }
        Ok(Some(exe))
    }

    // ------------------------------------------------------------------ Codex
    fn codex(&mut self) {
        let names: &[&str] = if cfg!(windows) {
            &["codex.exe", "codex.cmd"]
        } else {
            &["codex"]
        };
        let codex = self.find_on_path(names).or_else(|| {
            let installed = aipet_ipc::paths::local_app_data()
                .join("Programs")
                .join("OpenAI")
                .join("Codex")
                .join("bin")
                .join("codex.exe");
            (cfg!(windows) && is_file(&installed)).then_some(installed)
        });
        self.section("Codex");
        match &codex {
            None => self.warn("the codex CLI isn't on PATH (needed to trust hooks, and for this check)"),
            Some(path) => {
                let run = Run {
                    file: path.as_os_str(),
                    args: Args::list(&["--version"]),
                    timeout: 10,
                    env: &[],
                };
                let (code, version, error) = capture(&run, None);
                let shown = path.display();
                if code == 0 {
                    self.ok(&format!("{} ({shown})", version.trim()));
                } else {
                    let why = match code {
                        -1 => "no answer within 10 s".to_owned(),
                        -2 => error,
                        code => format!("exit code {code}"),
                    };
                    self.warn(&format!("codex --version didn't work: {why} ({shown})"));
                }
            }
        }

        let (config, hooks_json) = codex::files();
        self.section(&format!("Codex config ({})", config.display()));
        let toml = if is_file(&config) {
            fs_read(&config).map(|b| decoded(&b)).unwrap_or_default()
        } else {
            String::new()
        };
        let features = features(&toml);
        if features
            .iter()
            .any(|(key, value)| (key == "hooks" || key == "codex_hooks") && value == "false")
        {
            self.fail("hooks are switched off in [features]");
        }
        if features.iter().any(|(key, _)| key == "codex_hooks") {
            self.warn("[features] codex_hooks is deprecated; Codex wants hooks instead");
        }
        if hook_tables(&toml) && is_file(&hooks_json) {
            self.warn("hooks are in both config.toml and hooks.json; Codex warns about that at every start");
        }

        self.section("Hooks Codex knows about");
        match hooks_list(codex.as_deref()) {
            Some(listed) => self.codex_listed(&listed),
            None => self.codex_files(),
        }
    }

    /// Without `hooks/list`: what the files say (trust can only be read from Codex itself).
    fn codex_files(&mut self) {
        self.warn("couldn't ask Codex (codex app-server hooks/list), so whether the hooks are trusted is unknown");
        let files = codex::all_handlers();
        let mine_in_files: Vec<&codex::Handler> = files.iter().filter(|h| codex::is_ours(Some(&h.command))).collect();
        for h in &files {
            let mine = codex::is_ours(Some(&h.command));
            let file = h.file.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
            let line = format!("{} {}  ({file})", pad(&h.event, 18), short(&h.command, 90));
            if mine {
                self.ok(&line);
            } else {
                self.info(&line);
            }
            for problem in lint(&h.command) {
                self.bad(!mine, &format!("   {problem}"));
            }
        }
        // the plugin's hooks are in its own folder, not in these files
        let plugin = codex::enabled_plugin().unwrap_or_default();
        if let Some(plugin) = &plugin {
            self.ok(&format!("the {plugin} plugin is enabled, so the hooks come from it"));
        }
        match &plugin {
            None if mine_in_files.is_empty() => self.fail(NOT_REGISTERED),
            Some(plugin) if !mine_in_files.is_empty() => self.warn(&twice(plugin)),
            None if codex::EVENTS
                .iter()
                .any(|e| !mine_in_files.iter().any(|m| m.event == e.name)) =>
            {
                self.warn("AiPet isn't registered for every event (run aipet-hook --install codex)");
            }
            _ => {}
        }
        let up = self.check_aipet();
        if let Some(first) = mine_in_files.first() {
            self.probe_codex(&first.command, up, &[]);
        } else if plugin.is_some() {
            self.no_probe(
                "without hooks/list it isn't known which command and folder Codex runs the plugin's hook with",
            );
        }
        self.recent_events("codex");
    }

    /// With `hooks/list`: each hook Codex knows about, whether it trusts AiPet's, and where they come from. Without
    /// any of AiPet's that is all: the pet isn't asked.
    fn codex_listed(&mut self, listed: &[Node]) {
        let ours: Vec<&Node> = listed
            .iter()
            .filter(|h| codex::is_ours(text(h, "command").as_deref()))
            .collect();
        let (from_plugin, direct): (Vec<&Node>, Vec<&Node>) = ours.iter().copied().partition(|h| is_plugins(h));
        for h in listed {
            let command =
                text(h, "command").unwrap_or_else(|| format!("({})", text(h, "handlerType").unwrap_or_default()));
            let trust = text(h, "trustStatus").unwrap_or_default();
            let mine = ours.iter().any(|o| std::ptr::eq(*o, h));
            let line = format!(
                "{} {} {}",
                pad(&text(h, "eventName").unwrap_or_default(), 18),
                pad(&trust, 9),
                short(&command, 90)
            );
            if mine && (trust == "untrusted" || trust == "modified") {
                self.fail(&format!(
                    "{line}  <- not trusted yet: run codex and choose Review hooks (or /hooks)"
                ));
            } else if mine && matches!(member(h, "enabled"), Ok(Some(Node::Bool(false)))) {
                self.fail(&format!("{line}  <- disabled in /hooks"));
            } else if mine {
                self.ok(&line);
            } else {
                self.info(&line);
            }
            for problem in lint(&command) {
                self.bad(!mine, &format!("   {problem}"));
            }
        }
        if ours.is_empty() {
            return self.fail(NOT_REGISTERED);
        }
        // the plugin's folder, where its command finds the hook; and why it can't be run
        let (mut root, mut skip) = (None, None);
        if let Some(first) = from_plugin.first() {
            let plugin = text(first, "pluginId").unwrap_or_else(|| "aipet".to_owned());
            let missing = missing(CODEX_EVENTS.iter().copied(), &from_plugin);
            if missing.is_empty() {
                self.ok(&format!(
                    "hooks come from the {plugin} plugin, for all {} of its events",
                    CODEX_EVENTS.len()
                ));
            } else {
                self.warn(&format!(
                    "the {plugin} plugin isn't registered for: {} (update it: codex plugin marketplace upgrade aipet, \
                     then codex plugin add aipet@aipet)",
                    missing.join(", ")
                ));
            }
            // sourcePath is <root>/hooks/codex.json
            match text(first, "sourcePath").filter(|s| !s.is_empty()) {
                Some(source) => root = directory_name(&source).and_then(|hooks| directory_name(&hooks)),
                None => skip = Some("Codex didn't say where the plugin is, and its command needs that ($PLUGIN_ROOT)"),
            }
            // its launcher quietly does nothing when there's no hook for this system
            if let Some(bin) = root.as_deref().and_then(|root| plugin_hook(Path::new(root)))
                && !is_file(&bin)
            {
                self.fail(&format!(
                    "the plugin has no hook for this system ({}), so its hooks do nothing",
                    bin.display()
                ));
                skip = Some("the plugin has no hook for this system");
            }
            if !direct.is_empty() {
                self.warn(&twice(&plugin));
            }
        } else {
            let missing = missing(codex::EVENTS.iter().map(|e| e.name), &direct);
            if !missing.is_empty() {
                self.warn(&format!(
                    "AiPet isn't registered for: {} (run aipet-hook --install codex)",
                    missing.join(", ")
                ));
            }
        }

        let running = self.check_aipet();
        if let Some(first) = direct.first() {
            self.probe_codex(&text(first, "command").unwrap_or_default(), running, &[]);
        } else if let Some(why) = skip {
            self.no_probe(why);
        } else {
            let command = text(from_plugin[0], "command").unwrap_or_default();
            self.probe_codex(&command, running, &[("PLUGIN_ROOT", root.unwrap_or_default())]);
        }
        self.recent_events("codex");
    }

    fn no_probe(&mut self, why: &str) {
        self.section("Running AiPet's hook the way Codex does");
        self.info(&format!("skipped: {why}"));
    }

    /// `ProbeCodex`: with --probe, AiPet's registered command run through the shell Codex uses, with what Codex sets
    /// for it too (a plugin's hook gets `PLUGIN_ROOT`).
    fn probe_codex(&mut self, command: &str, running: bool, env: &[(&'static str, String)]) {
        self.section("Running AiPet's hook the way Codex does");
        if !self.probe {
            return self.info("skipped: add --probe to run it through the shell Codex uses (PowerShell on Windows)");
        }
        #[cfg(windows)]
        {
            let shell: OsString = match self.find_on_path(&["pwsh.exe"]) {
                Some(pwsh) => pwsh.into(),
                None if is_file(&self.machine.pwsh) => self.machine.pwsh.clone().into(),
                None => self.machine.powershell.clone(),
            };
            let run = Run {
                file: &shell,
                args: Args::list(&["-NoProfile", "-Command", command]),
                timeout: 30,
                env,
            };
            self.run_probe(&run, running, false, 0);
            // "& '...'" is how the installer writes a path it can't write plainly: PowerShell only, by design. cmd.exe
            // is only Codex's rare fallback, so a failure there is a warning, not a problem
            if command.trim_start().starts_with('&') {
                self.info(
                    "not through the cmd.exe fallback: the command uses PowerShell's & '...' form, which cmd.exe can't \
                     run",
                );
            } else {
                self.info("and through the cmd.exe fallback:");
                let comspec = (self.vars)("COMSPEC").unwrap_or_else(|| "cmd.exe".into());
                let run = Run {
                    file: &comspec,
                    args: Args::Raw(format!("/C \"{command}\"")),
                    timeout: 30,
                    env,
                };
                self.run_probe(&run, running, true, 0);
            }
        }
        #[cfg(not(windows))]
        {
            let run = Run {
                file: OsStr::new("/bin/sh"),
                args: Args::list(&["-c", command]),
                timeout: 30,
                env,
            };
            self.run_probe(&run, running, false, 0);
            if let Some(shell) = (self.vars)("SHELL").filter(|s| !s.is_empty() && s != "/bin/sh") {
                let run = Run {
                    file: &shell,
                    args: Args::list(&["-lc", command]),
                    timeout: 30,
                    env,
                };
                self.run_probe(&run, running, false, 0);
            }
        }
    }

    // ------------------------------------------------------------------ shared
    /// `Probe`: runs the hook like the agent does, with a test event that changes no state; checks it starts, prints
    /// nothing and exits 0 well within the agent's time limit, and that the running pet got the event (the pet lists
    /// ignored events too). With the pet closed the hook must just return (within `quick_ms`, when given).
    /// `only_warn`: a failure is a warning (a rare fallback).
    fn run_probe(&mut self, run: &Run, running: bool, only_warn: bool, quick_ms: u128) {
        // unique in its first 13 characters, which is what the pet lists
        let id = format!("doctor{}-aipet", random_hex(7));
        let mut payload = Object::default();
        payload.push("hook_event_name", Node::String("AipetDoctor".into()));
        payload.push("session_id", Node::String(id.clone()));
        let cwd = std::env::current_dir()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        payload.push("cwd", Node::String(cwd));
        let started = Instant::now();
        let (code, stdout, stderr) = capture(run, Some(&payload.to_json()));
        let ms = started.elapsed().as_millis();
        let name = Path::new(run.file)
            .file_name()
            .unwrap_or(run.file)
            .to_string_lossy()
            .into_owned();
        let timeout = run.timeout;
        if code == -1 {
            let shown = short(&run.args.shown(), 70);
            self.bad(only_warn, &format!("{name} {shown}: didn't finish within {timeout} s"));
        } else if code != 0 {
            let said = if stderr.is_empty() {
                String::new()
            } else {
                format!(": {}", short(stderr.trim(), 200))
            };
            self.bad(only_warn, &format!("{name}: exit code {code} after {ms} ms{said}"));
        } else if !stdout.trim().is_empty() {
            let shown = short(stdout.trim(), 120);
            self.bad(
                only_warn,
                &format!("{name}: printed output the agent would act on: {shown}"),
            );
        } else if running && !pet_got(&id[..13]) {
            self.bad(
                only_warn,
                &format!("{name}: exited 0 but the pet didn't get the event (it isn't among the pet's recent events)"),
            );
        } else if !running && quick_ms > 0 && ms > quick_ms {
            self.warn(&format!(
                "{name}: exited 0, but took {ms} ms to find the pet isn't running"
            ));
        } else if ms > u128::from(timeout) * 1000 / 2 {
            self.warn(&format!("{name}: works, but took {ms} ms of the {timeout} s limit"));
        } else if running {
            self.ok(&format!(
                "{name}: ran, printed nothing, exit 0, {ms} ms, and the pet got the event"
            ));
        } else {
            self.ok(&format!(
                "{name}: ran, printed nothing, exit 0, {ms} ms (the pet isn't running, so the hook did nothing)"
            ));
        }
    }

    /// `CheckAiPet`: whether the pet is running, by pinging it: without it the hooks do nothing.
    fn check_aipet(&mut self) -> bool {
        self.section("AiPet");
        let (pong, running) = ping();
        let endpoint = aipet_ipc::endpoint::endpoint().to_string_lossy();
        match &pong {
            _ if !running => self.warn("The pet isn't running; hooks do nothing until it is."),
            None => self.fail(&format!(
                "something listens on {endpoint} but doesn't answer like the pet (another version?)"
            )),
            Some(pong) => {
                let pid = member(pong, PID)
                    .ok()
                    .flatten()
                    .map(|pid| json_out::to_string(pid, NEWLINE))
                    .unwrap_or_default();
                self.ok(&format!("the pet is running (pid {pid}), listening on {endpoint}"));
            }
        }
        running
    }

    /// `RecentEvents`: the last events of this agent the running pet got, without the doctor's own.
    fn recent_events(&mut self, agent: &str) {
        let log = aipet_ipc::paths::hook_events_log();
        self.section(&format!(
            "Recent events the pet got (all of them are in {})",
            log.display()
        ));
        let (pong, running) = ping();
        if !running {
            return self.info("none: the pet isn't running");
        }
        let marker = format!(" {agent} pid=");
        let lines: Vec<String> = recent(pong.as_ref())
            .into_iter()
            .filter(|l| l.contains(&marker) && !l.contains("AipetDoctor"))
            .collect();
        let lines = &lines[lines.len().saturating_sub(8)..];
        if lines.is_empty() {
            self.info("none yet");
        }
        for line in lines {
            self.info(line);
        }
    }

    /// `FindOnPath`: the first of `names` in the first folder on PATH that has one of them.
    fn find_on_path(&self, names: &[&str]) -> Option<PathBuf> {
        let path = (self.vars)("PATH").unwrap_or_default();
        for dir in split_path(&path) {
            let dir = dir.to_string_lossy();
            for name in names {
                let file = Path::new(dir.trim_matches('"')).join(name);
                if is_file(&file) {
                    return Some(file);
                }
            }
        }
        None
    }
}

/// `True`: a JSON `true`.
fn is_true(node: Option<&Node>) -> bool {
    matches!(node, Some(Node::Bool(true)))
}

/// `(string)h[key]` on a `hooks/list` entry: its string, and nothing for anything else.
fn text(h: &Node, key: &str) -> Option<String> {
    match member(h, key) {
        Ok(Some(Node::String(s))) => Some(s.clone()),
        _ => None,
    }
}

/// `FromPlugin`: a listed hook of the AiPet plugin rather than one in the user's config.toml or hooks.json.
fn is_plugins(h: &Node) -> bool {
    text(h, "source").as_deref() == Some("plugin")
        || text(h, "pluginId").is_some_and(|p| p.starts_with("aipet@"))
        || text(h, "command").is_some_and(|c| c.contains("PLUGIN_ROOT"))
}

/// `Missing`: the events none of `hooks` is listed for. `hooks/list` names events in camelCase (`preToolUse`).
fn missing<'e>(events: impl Iterator<Item = &'e str>, hooks: &[&Node]) -> Vec<&'e str> {
    events
        .filter(|event| {
            let camel = format!("{}{}", event[..1].to_ascii_lowercase(), &event[1..]);
            !hooks
                .iter()
                .any(|h| text(h, "eventName").as_deref() == Some(camel.as_str()))
        })
        .collect()
}

fn twice(plugin: &str) -> String {
    format!(
        "the {plugin} plugin is also enabled, so every event is reported twice: run aipet-hook --uninstall codex (the \
         plugin replaces those hooks)"
    )
}

/// `PluginHook`: the hook binary the plugin's launcher runs on this system (see plugins/aipet/native/aipet-hook.sh),
/// or none where it runs none.
fn plugin_hook(root: &Path) -> Option<PathBuf> {
    let native = root.join("native");
    if cfg!(windows) {
        Some(native.join("win-x64").join("aipet-hook.exe"))
    } else if cfg!(target_os = "linux") {
        let rid = if cfg!(target_arch = "aarch64") {
            "linux-arm64"
        } else {
            "linux-x64"
        };
        Some(native.join(rid).join("aipet-hook"))
    } else {
        None
    }
}

// ------------------------------------------------------------------ the pet
/// `Ping`: the running pet's answer to a ping (`{ok, app, pid, recent}`), `None` when what listens answers something
/// else; and whether anything listens at all.
fn ping() -> (Option<Node>, bool) {
    let Ok(mut pet) = connect(CONNECT) else {
        return (None, false);
    };
    let request = format!("{{\"{V}\":{VERSION},\"{KIND}\":\"ping\"}}");
    let pong = pet
        .ask(&request)
        .ok()
        .flatten()
        .and_then(|reply| json_out::parse(&reply).ok())
        .filter(|pong| is_true(member(pong, OK).ok().flatten()));
    (pong, true)
}

/// `Recent`: the pet's recent hook-events.log lines, from a ping's answer (both agents').
fn recent(pong: Option<&Node>) -> Vec<String> {
    match pong.map(|pong| member(pong, RECENT)) {
        Some(Ok(Some(Node::Array(lines)))) => lines
            .iter()
            .filter_map(|l| match l {
                Node::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `PetGot`: whether the pet lists an event of this session among its recent ones, waiting up to 5 s for it.
fn pet_got(sid13: &str) -> bool {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if recent(ping().0.as_ref()).iter().any(|l| l.contains(sid13)) {
            return true;
        }
        if Instant::now() > until {
            return false;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

// ------------------------------------------------------------------ Codex's own answer
/// `HooksList`: `codex app-server` over stdio: `initialize`, then `hooks/list` for the home folder, all within 30 s.
/// `None` when Codex can't be asked: it isn't there, doesn't start, quits, answers something else or nothing in time.
/// It is ended, with everything it started, either way.
fn hooks_list(codex: Option<&Path>) -> Option<Vec<Node>> {
    let mut command = Command::new(codex?);
    command
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut tree = Tree::spawn(&mut command).ok()?;
    let listed = ask_hooks(&mut tree);
    tree.kill();
    listed
}

fn ask_hooks(tree: &mut Tree) -> Option<Vec<Node>> {
    let lines = lines(tree.child.stdout.take()?);
    let mut stderr = tree.child.stderr.take()?;
    thread::spawn(move || {
        let _ = io::copy(&mut stderr, &mut io::sink());
    });
    let mut stdin = tree.child.stdin.take()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    // the first line that is an object with this id
    let answer = |id: &str| -> Option<Node> {
        loop {
            let left = deadline
                .checked_duration_since(Instant::now())
                .filter(|d| !d.is_zero())?;
            let line = lines.recv_timeout(left).ok()?;
            let Ok(node @ Node::Object(_)) = json_out::parse(&line) else {
                continue;
            };
            if matches!(member(&node, "id"), Ok(Some(n)) if json_out::to_string(n, NEWLINE) == id) {
                return Some(node);
            }
        }
    };
    let mut send = |request: String| stdin.write_all(format!("{request}\n").as_bytes()).ok();

    let mut client = Object::default();
    client.push("name", Node::String("aipet-doctor".into()));
    client.push("title", Node::String("AiPet doctor".into()));
    client.push("version", Node::String("1.0".into()));
    let mut params = Object::default();
    params.push("clientInfo", Node::Object(client));
    send(request(1, "initialize", Some(params)))?;
    member(&answer("1")?, "result").ok()??;
    send(request(0, "initialized", None))?;
    let home = aipet_ipc::paths::home().to_string_lossy().into_owned();
    let mut params = Object::default();
    params.push("cwds", Node::Array(vec![Node::String(home)]));
    send(request(2, "hooks/list", Some(params)))?;
    let listed = answer("2")?;
    let Ok(Some(result)) = member(&listed, "result") else {
        return None;
    };
    let Node::Array(data) = member(result, "data").ok()?? else {
        return None;
    };
    let mut hooks = Vec::new();
    for d in data.iter().filter(|d| matches!(d, Node::Object(_))) {
        if let Some(Node::Array(list)) = member(d, "hooks").ok()? {
            hooks.extend(list.iter().filter(|h| matches!(h, Node::Object(_))).cloned());
        }
    }
    Some(hooks)
}

/// A JSON-RPC request line; `id` 0 for a notification, which has none.
fn request(id: u32, method: &str, params: Option<Object>) -> String {
    let mut request = Object::default();
    if id > 0 {
        request.push("id", Node::Number(id.to_string()));
    }
    request.push("method", Node::String(method.into()));
    if let Some(params) = params {
        request.push("params", Node::Object(params));
    }
    request.to_json()
}

/// The lines a program writes, as `StreamReader.ReadLine` reads them: a line ends at `\n`, `\r` or `\r\n`, a UTF-8
/// byte order mark at the start is dropped, and what isn't UTF-8 is U+FFFD. Each as it comes; the channel closes at
/// the end of the stream.
fn lines(mut pipe: impl Read + Send + 'static) -> mpsc::Receiver<String> {
    let (send, got) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut line = Vec::new();
        let (mut after_cr, mut first) = (false, true);
        let mut emit = |line: &mut Vec<u8>| {
            let text = String::from_utf8_lossy(line).into_owned();
            let text = match text.strip_prefix('\u{feff}') {
                Some(rest) if std::mem::take(&mut first) => rest.to_owned(),
                _ => {
                    first = false;
                    text
                }
            };
            line.clear();
            send.send(text).is_ok()
        };
        while let Ok(n) = pipe.read(&mut buf) {
            if n == 0 {
                break;
            }
            for &b in &buf[..n] {
                if std::mem::take(&mut after_cr) && b == b'\n' {
                    continue;
                }
                if b == b'\n' || b == b'\r' {
                    after_cr = b == b'\r';
                    if !emit(&mut line) {
                        return;
                    }
                } else {
                    line.push(b);
                }
            }
        }
        if !line.is_empty() {
            emit(&mut line);
        }
    });
    got
}

// ------------------------------------------------------------------ running programs
/// `Capture`: runs the program with `stdin` written to it and closed, and waits at most its timeout. Its exit code,
/// stdout and stderr; -1 when it didn't finish in time (it is ended, with everything it started), -2 with .NET's
/// message when it couldn't be started.
fn capture(run: &Run, stdin: Option<&str>) -> (i64, String, String) {
    let mut command = Command::new(run.file);
    match &run.args {
        Args::List(list) => {
            command.args(list);
        }
        #[cfg(windows)]
        Args::Raw(raw) => {
            std::os::windows::process::CommandExt::raw_arg(&mut command, raw);
        }
    }
    for (name, value) in run.env {
        command.env(name, value);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut tree = match Tree::spawn(&mut command) {
        Ok(tree) => tree,
        Err(e) => return (-2, String::new(), not_started(run.file, &e)),
    };
    let deadline = Instant::now() + Duration::from_secs(run.timeout);
    let out = read_all(tree.child.stdout.take());
    let err = read_all(tree.child.stderr.take());
    if let Some(mut input) = tree.child.stdin.take() {
        // a program that quit without reading it has still run
        let _ = input.write_all(stdin.unwrap_or_default().as_bytes());
    }
    let status = loop {
        match tree.child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            _ => break None,
        }
    };
    // what it wrote, once whatever else has its output (a process it started) is done with it too
    let left = || deadline.saturating_duration_since(Instant::now());
    let written = status.and_then(|status| {
        let out = out.recv_timeout(left()).ok()?;
        let err = err.recv_timeout(left()).ok()?;
        Some((status, out, err))
    });
    match written {
        Some((status, out, err)) => (exit_code(status), decoded(&out), decoded(&err)),
        None => {
            tree.kill();
            (-1, String::new(), String::new())
        }
    }
}

/// Everything a pipe gives until it ends, read on a thread of its own.
fn read_all(pipe: Option<impl Read + Send + 'static>) -> mpsc::Receiver<Vec<u8>> {
    let (send, got) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        let _ = send.send(bytes);
    });
    got
}

/// `Process.ExitCode`: on Unix a process a signal ended has 128 plus the signal's number.
fn exit_code(status: ExitStatus) -> i64 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + i64::from(signal);
        }
    }
    status.code().map_or(-1, i64::from)
}

/// What .NET's `Process.Start` throws when a program can't be started.
fn not_started(file: &OsStr, e: &io::Error) -> String {
    // std's own search found no such program: the system's words for that are a missing file's
    let missing = io::Error::from_raw_os_error(2);
    let e = match e.raw_os_error() {
        None if e.kind() == io::ErrorKind::NotFound => &missing,
        _ => e,
    };
    // Windows: an exe for another system
    if cfg!(windows) && matches!(e.raw_os_error(), Some(193 | 216)) {
        return "The specified executable is not a valid application for this OS platform.".to_owned();
    }
    let reason = system_text(e);
    let file = file.to_string_lossy();
    // Unix: a bare name found nowhere on PATH
    if cfg!(unix) && !file.contains('/') && e.kind() == io::ErrorKind::NotFound {
        return reason;
    }
    let cwd = std::env::current_dir()
        .map(|d| d.display().to_string())
        .unwrap_or_default();
    format!("An error occurred trying to start process '{file}' with working directory '{cwd}'. {reason}")
}

/// An error's text without Rust's " (os error N)".
fn system_text(e: &io::Error) -> String {
    let text = e.to_string();
    match text.rfind(" (os error ") {
        Some(at) => text[..at].to_owned(),
        None => text,
    }
}

/// A child process and everything it starts, which [`Tree::kill`] ends together, as .NET's
/// `Process.Kill(entireProcessTree: true)` does: on Unix the child leads a process group of its own, on Windows it is
/// in a job object.
struct Tree {
    child: Child,
    #[cfg(windows)]
    job: Option<std::os::windows::io::OwnedHandle>,
}

impl Tree {
    fn spawn(command: &mut Command) -> io::Result<Tree> {
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(command, 0);
        let child = command.spawn()?;
        Ok(Tree {
            #[cfg(windows)]
            job: job::assign(&child),
            child,
        })
    }

    /// Ends the child and everything it started, and waits for the child.
    fn kill(&mut self) {
        #[cfg(unix)]
        if let Ok(group) = i32::try_from(self.child.id()) {
            // SAFETY: kill only sends a signal; a negative pid is the child's process group, which it still leads
            // (it isn't waited for until below)
            unsafe { libc::kill(-group, libc::SIGKILL) };
        }
        #[cfg(windows)]
        if let Some(job) = &self.job {
            job::terminate(job);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// kernel32's job objects, which windows-sys has behind a feature this crate isn't built with.
#[cfg(windows)]
mod job {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::process::Child;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const core::ffi::c_void, name: *const u16) -> isize;
        fn AssignProcessToJobObject(job: isize, process: isize) -> i32;
        fn TerminateJobObject(job: isize, exit_code: u32) -> i32;
    }

    /// A job with the child in it, so what it starts is in it too. `None` when the child can't be put in one; ending
    /// it then ends only the child.
    pub(super) fn assign(child: &Child) -> Option<OwnedHandle> {
        // SAFETY: an unnamed job with the default security
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job == 0 {
            return None;
        }
        // SAFETY: a handle just made, owned from here on
        let job = unsafe { OwnedHandle::from_raw_handle(job as _) };
        // SAFETY: both handles are open
        let assigned =
            unsafe { AssignProcessToJobObject(job.as_raw_handle() as isize, child.as_raw_handle() as isize) };
        (assigned != 0).then_some(job)
    }

    pub(super) fn terminate(job: &OwnedHandle) {
        // SAFETY: an open job handle
        unsafe { TerminateJobObject(job.as_raw_handle() as isize, 1) };
    }
}

// ------------------------------------------------------------------ files
/// Why a file couldn't be read: .NET's exception, as its message.
enum ReadError {
    /// `FileNotFoundException`: the file isn't there, but its folder is.
    NotFound(String),
    Other(String),
}

impl ReadError {
    fn into_message(self) -> String {
        match self {
            ReadError::NotFound(message) | ReadError::Other(message) => message,
        }
    }
}

/// `File.ReadAllText`'s bytes, or what .NET throws when it can't read them ([`thrown`]), with
/// `FileNotFoundException` told apart.
fn fs_read(path: &Path) -> Result<Vec<u8>, ReadError> {
    std::fs::read(path).map_err(|e| {
        if file_not_found(&e, path) {
            ReadError::NotFound(thrown(&e, path))
        } else {
            ReadError::Other(thrown(&e, path))
        }
    })
}

/// PATH's folders, split at the platform's separator.
fn split_path(path: &OsStr) -> Vec<OsString> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        path.as_bytes()
            .split(|&b| b == b':')
            .map(|dir| OsString::from_vec(dir.to_vec()))
            .collect()
    }
    #[cfg(not(unix))]
    {
        path.to_string_lossy().split(';').map(OsString::from).collect()
    }
}

/// .NET's `Path.GetDirectoryName`: the text up to the last separator (the separators before it taken out too), then
/// normalised: repeated separators collapse, and on Windows `/` is `\`. `None` for a root or an empty path.
fn directory_name(path: &str) -> Option<String> {
    let sep = |c: char| c == '/' || (cfg!(windows) && c == '\\');
    let chars: Vec<char> = path.chars().collect();
    if chars.is_empty() || (cfg!(windows) && chars.iter().all(|&c| c == ' ')) {
        return None;
    }
    let root = root_length(&chars, sep);
    let mut end = chars.len();
    if end <= root {
        return None;
    }
    loop {
        end -= 1;
        if end <= root || sep(chars[end]) {
            break;
        }
    }
    while end > root && sep(chars[end - 1]) {
        end -= 1;
    }
    let primary = if cfg!(windows) { '\\' } else { '/' };
    let mut out = String::with_capacity(end);
    for (i, &c) in chars[..end].iter().enumerate() {
        if !sep(c) {
            out.push(c);
        } else if (cfg!(windows) && i == 0) || !(i + 1 < end && sep(chars[i + 1])) {
            // Windows keeps a UNC path's two
            out.push(primary);
        }
    }
    Some(out)
}

/// .NET's `PathInternal.GetRootLength`: `/` on Unix; `C:\`, `C:` or `\\server\share` on Windows.
fn root_length(path: &[char], sep: impl Fn(char) -> bool) -> usize {
    if !cfg!(windows) {
        return usize::from(path.first() == Some(&'/'));
    }
    if path.len() > 1 && sep(path[0]) && sep(path[1]) {
        // past \\server\share
        let mut separators = 0;
        for (i, &c) in path.iter().enumerate().skip(2) {
            if sep(c) {
                separators += 1;
                if separators == 2 {
                    return i;
                }
            }
        }
        path.len()
    } else if path.len() > 1 && path[1] == ':' {
        if path.len() > 2 && sep(path[2]) { 3 } else { 2 }
    } else {
        usize::from(path.first().is_some_and(|&c| sep(c)))
    }
}

// ------------------------------------------------------------------ text
/// `Short`: the text cut to `n` UTF-16 units, the last three "...", when it's longer (a surrogate pair cut in two
/// leaves U+FFFD, as .NET writes the half).
fn short(s: &str, n: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    if units.len() <= n {
        return s.to_owned();
    }
    String::from_utf16_lossy(&units[..n - 3]) + "..."
}

/// `{s,-width}`: the text padded with spaces to `width` UTF-16 units.
fn pad(s: &str, width: usize) -> String {
    let len = s.encode_utf16().count();
    format!("{s}{}", " ".repeat(width.saturating_sub(len)))
}

/// `n` random hexadecimal digits (at most 16).
fn random_hex(n: usize) -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let x = RandomState::new().hash_one((now, std::process::id()));
    format!("{x:016x}")[..n].to_owned()
}

/// `Lint`: problems in a hook command that make it fail before the program even starts. Only Windows has them.
fn lint(cmd: &str) -> Vec<&'static str> {
    if cfg!(windows) { windows_lint(cmd) } else { Vec::new() }
}

fn windows_lint(cmd: &str) -> Vec<&'static str> {
    let mut problems = Vec::new();
    if cmd.trim_start().starts_with(['"', '\'']) {
        problems.push(
            "starts with a quoted path: PowerShell rejects that (exit code 1); use the path without quotes or with & \
             in front",
        );
    }
    // %[A-Za-z_]+%
    let bytes = cmd.as_bytes();
    let variable = bytes.iter().enumerate().any(|(i, &b)| {
        let name = bytes[i + 1..]
            .iter()
            .take_while(|c| c.is_ascii_alphabetic() || **c == b'_')
            .count();
        b == b'%' && name > 0 && bytes.get(i + 1 + name) == Some(&b'%')
    });
    if variable {
        problems.push("%VARIABLE%s aren't expanded by PowerShell");
    }
    problems
}

/// `^\s*\[\[\s*hooks\.[A-Za-z]+\s*\]\]` with `RegexOptions.Multiline`: a `[[hooks.<Event>]]` table at the start of a
/// line (the hooks config.toml defines itself).
fn hook_tables(toml: &str) -> bool {
    let chars: Vec<char> = toml.chars().collect();
    let skip = |mut i: usize| {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        i
    };
    let at = |i: usize, s: &str| s.chars().enumerate().all(|(k, c)| chars.get(i + k) == Some(&c));
    (0..chars.len())
        .filter(|&i| i == 0 || chars[i - 1] == '\n')
        .any(|start| {
            let mut i = skip(start);
            if !at(i, "[[") {
                return false;
            }
            i = skip(i + 2);
            if !at(i, "hooks.") {
                return false;
            }
            i += "hooks.".len();
            let letters = chars[i..].iter().take_while(|c| c.is_ascii_alphabetic()).count();
            letters > 0 && at(skip(i + letters), "]]")
        })
}

/// `Features`: the `[features]` settings in config.toml however they're spelled: in a `[features]` table, as a
/// root-level `features.x = ...`, or as an inline `features = { ... }`. Keys in other tables (a profile's, say) don't
/// count, nor do lines in a multi-line string.
fn features(toml: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut table = String::new();
    // the delimiter of the multi-line string we're in
    let mut open: Option<&str> = None;
    for raw in toml.split('\n') {
        let line = raw.trim();
        if let Some(delim) = open {
            if line.contains(delim) {
                open = None;
            }
            continue;
        }
        if is_header(line) {
            table = line
                .split('#')
                .next()
                .unwrap_or_default()
                .chars()
                .filter(|&c| !matches!(c, '[' | ']' | '"' | '\'') && !c.is_whitespace())
                .collect();
        } else if table == "features" {
            found.extend(pair(line));
        } else if table.is_empty() {
            let key = features_key(line);
            if let Some(dotted) = key.and_then(|rest| after(rest, '.')).and_then(pair) {
                found.push(dotted);
            } else if let Some(inline) = key
                .and_then(|rest| after(rest, '='))
                .and_then(|rest| rest.strip_prefix('{'))
            {
                found.extend(inline_pairs(&inline[..inline.find('}').unwrap_or(inline.len())]));
            }
        }
        // a multi-line string opened and not closed on this line: the lines up to its end are text, not keys
        if let Some((delim, rest)) = multiline_opened(line)
            && !rest.contains(delim)
        {
            open = Some(delim);
        }
    }
    found
}

/// `^\[.*\]\s*(#.*)?$`: a table header, maybe with a comment after it.
fn is_header(line: &str) -> bool {
    line.starts_with('[')
        && line.char_indices().skip(1).any(|(i, c)| {
            let rest = line[i + c.len_utf8()..].trim_start();
            c == ']' && (rest.is_empty() || rest.starts_with('#'))
        })
}

/// `^["']?features["']?\s*`: what follows the key `features` at the start of the line.
fn features_key(line: &str) -> Option<&str> {
    let rest = line.strip_prefix(['"', '\'']).unwrap_or(line);
    let rest = rest.strip_prefix("features")?;
    Some(rest.strip_prefix(['"', '\'']).unwrap_or(rest).trim_start())
}

/// What follows `c` (and the white space after it) when the text starts with `c`.
fn after(s: &str, c: char) -> Option<&str> {
    s.strip_prefix(c).map(str::trim_start)
}

/// `["']?([A-Za-z0-9_-]+)["']?\s*=\s*([^\s,#}]+)` at the start of `s`: the key, its value, and where the value ends.
fn pair_at(s: &str) -> Option<((String, String), usize)> {
    let quote = |s: &str| usize::from(s.starts_with(['"', '\'']));
    let mut i = quote(s);
    let key_len = s[i..]
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
        .count();
    if key_len == 0 {
        return None;
    }
    let key = &s[i..i + key_len];
    i += key_len;
    i += quote(&s[i..]);
    i = s.len() - s[i..].trim_start().len();
    s[i..].strip_prefix('=')?;
    i += 1;
    i = s.len() - s[i..].trim_start().len();
    let value_len: usize = s[i..]
        .chars()
        .take_while(|&c| !c.is_whitespace() && c != ',' && c != '#' && c != '}')
        .map(char::len_utf8)
        .sum();
    if value_len == 0 {
        return None;
    }
    Some(((key.to_owned(), s[i..i + value_len].to_owned()), i + value_len))
}

fn pair(s: &str) -> Option<(String, String)> {
    pair_at(s).map(|(pair, _)| pair)
}

/// `Regex.Matches(inline, @"(?:^|,)\s*" + Pair)`: every pair in an inline table's text, each at its start or after a
/// comma, looked for from where the one before it ended.
fn inline_pairs(inline: &str) -> Vec<(String, String)> {
    let at = |start: usize| {
        let rest = &inline[start..];
        let skipped = rest.len() - rest.trim_start().len();
        pair_at(&rest[skipped..]).map(|(pair, end)| (pair, start + skipped + end))
    };
    let mut found = Vec::new();
    let mut from = 0;
    loop {
        let hit = inline[from..].char_indices().find_map(|(i, c)| {
            let p = from + i;
            // `^` first, at the very start; then a comma
            let start = if p == 0 { at(0) } else { None };
            start.or_else(|| (c == ',').then(|| at(p + 1)).flatten())
        });
        let Some((pair, end)) = hit else { break };
        found.push(pair);
        from = end;
    }
    found
}

/// `=\s*("""|''')`: the first multi-line string opened on the line: its delimiter and the text after it.
fn multiline_opened(line: &str) -> Option<(&'static str, &str)> {
    line.match_indices('=').find_map(|(i, _)| {
        let rest = line[i + 1..].trim_start();
        ["\"\"\"", "'''"]
            .into_iter()
            .find(|delim| rest.starts_with(*delim))
            .map(|delim| (delim, &rest[3..]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Short` counts UTF-16 units, as .NET's string length does; so does the padding of the listed hooks.
    #[test]
    fn text_is_cut_and_padded_by_utf16_units() {
        assert_eq!(short("abcdef", 6), "abcdef");
        assert_eq!(short("abcdefg", 6), "abc...");
        // 😀 is two units: the cut between them leaves U+FFFD
        assert_eq!(short("ab😀cdefg", 6), "ab\u{fffd}...");
        assert_eq!(short("😀😀😀😀", 7), "😀😀...");
        assert_eq!(pad("stop", 6), "stop  ");
        assert_eq!(pad("😀", 3), "😀 ");
        assert_eq!(pad("longer than", 4), "longer than");
    }

    /// The command problems PowerShell has (Windows' `Lint`).
    #[test]
    fn windows_commands_are_linted_as_powershell_takes_them() {
        let quoted = "starts with a quoted path: PowerShell rejects that (exit code 1); use the path without quotes \
                      or with & in front";
        let variable = "%VARIABLE%s aren't expanded by PowerShell";
        for (cmd, problems) in [
            (r"C:\x\aipet-hook.exe --agent codex", vec![]),
            (r#"  "C:\Program Files\x.exe" --agent codex"#, vec![quoted]),
            ("'C:/x.exe'", vec![quoted]),
            (r"%LOCALAPPDATA%\AiPet\hooks\aipet-hook.exe", vec![variable]),
            (r#""%USER_PROFILE%\x""#, vec![quoted, variable]),
            ("%%A%", vec![variable]),
            ("%1% %%", vec![]),
            (r"& 'C:\with space\aipet-hook.exe' --agent codex", vec![]),
        ] {
            assert_eq!(windows_lint(cmd), problems, "{cmd}");
        }
    }

    /// `Path.GetDirectoryName`, as the plugin's folder is found from `sourcePath`.
    #[test]
    fn directory_names_are_dotnets() {
        let native = |s: &str| {
            if cfg!(windows) {
                s.replace('/', "\\")
            } else {
                s.to_owned()
            }
        };
        for (path, parent) in [
            (
                "/x/cache/aipet/0.2.0/hooks/codex.json",
                Some("/x/cache/aipet/0.2.0/hooks"),
            ),
            ("/x//hooks//codex.json", Some("/x/hooks")),
            ("/codex.json", Some("/")),
            ("/", None),
            ("codex.json", Some("")),
            ("", None),
        ] {
            assert_eq!(directory_name(path), parent.map(native), "{path}");
        }
        if cfg!(windows) {
            for (path, parent) in [
                (r"C:\x\hooks\codex.json", Some(r"C:\x\hooks")),
                ("C:/x/hooks/codex.json", Some(r"C:\x\hooks")),
                (r"C:\codex.json", Some(r"C:\")),
                (r"C:\", None),
                (r"\\srv\share\x\codex.json", Some(r"\\srv\share\x")),
                (r"\\srv\share\x", Some(r"\\srv\share")),
            ] {
                assert_eq!(directory_name(path).as_deref(), parent, "{path}");
            }
        } else {
            assert_eq!(directory_name("//x/codex.json").as_deref(), Some("/x"));
        }
    }

    /// `Features` and the `[[hooks.<Event>]]` check, which read config.toml with regular expressions: every spelling,
    /// and what doesn't count.
    #[test]
    fn features_are_read_as_the_csharps_expressions_read_them() {
        let pairs =
            |toml: &str| -> Vec<String> { features(toml).into_iter().map(|(k, v)| format!("{k}={v}")).collect() };
        let none: Vec<String> = Vec::new();
        assert_eq!(
            pairs("[features]\nhooks = false\ncodex_hooks=true # old\n"),
            ["hooks=false", "codex_hooks=true"]
        );
        assert_eq!(
            pairs("[ \"features\" ] # the table\n'hooks' = false\n"),
            ["hooks=false"]
        );
        assert_eq!(
            pairs("features.hooks = false\n\"features\" . codex_hooks = 1\n"),
            ["hooks=false", "codex_hooks=1"]
        );
        assert_eq!(
            pairs("features = { hooks = false, 'codex_hooks' = true, x y = 1 ,z=2}\n"),
            ["hooks=false", "codex_hooks=true", "z=2"]
        );
        assert_eq!(pairs("features={,hooks=false}"), ["hooks=false"]);
        // another table's, a string's, and after a table
        assert_eq!(
            pairs("[profiles.x]\nfeatures.hooks = false\n[profiles.x.features]\nhooks = false\n"),
            none
        );
        assert_eq!(pairs("note = \"\"\"\n[features]\nhooks = false\n\"\"\"\n"), none);
        assert_eq!(pairs("note = '''x''' \n[features]\nhooks = false\n"), ["hooks=false"]);
        assert_eq!(pairs("[features] # [x]\nhooks = false"), ["hooks=false"]);
        assert_eq!(pairs("[features]x\nhooks = false"), none);

        assert!(hook_tables("[[hooks.Stop]]\n"));
        assert!(hook_tables("model = 1\n  \n  [[ hooks.PreToolUse ]] # x\n"));
        assert!(!hook_tables("[[hooks.Stop.hooks]]\n"));
        assert!(!hook_tables("x = 1 [[hooks.Stop]]\n"));
        assert!(!hook_tables("[[Hooks.Stop]]\n[hooks]\n"));
    }

    /// Windows: Codex's fallback through cmd.exe, after the PowerShell attempt, which here finds no PowerShell (none is
    /// on the doctor's PATH, and the others it would try are replaced by files that aren't there): that failure is
    /// said, then the command runs as `cmd /C "<command>"`.
    #[cfg(windows)]
    #[test]
    fn the_cmd_fallback_runs_after_powershell() {
        let empty = std::env::temp_dir().join(format!("aipet-doctor-nopwsh-{}", std::process::id()));
        std::fs::create_dir_all(&empty).unwrap();
        let path = OsString::from(&empty);
        let comspec = std::env::var_os("COMSPEC");
        let vars = |name: &str| match name {
            "PATH" => Some(path.clone()),
            "COMSPEC" => comspec.clone(),
            _ => None,
        };
        let machine = Machine {
            managed: Vec::new(),
            pwsh: empty.join("pwsh.exe"),
            powershell: "aipet-doctor-test-no-powershell.exe".into(),
        };
        let mut lines = Vec::new();
        let mut out = |l: &str| lines.push(l.to_owned());
        let mut doctor = Doctor::new(&vars, machine, true, &mut out);
        doctor.probe_codex("exit 0", false, &[]);
        doctor.probe_codex(r"& 'C:\x y\aipet-hook.exe' --agent codex", false, &[]);
        let counted = (doctor.fails, doctor.warns);
        let _ = std::fs::remove_dir_all(&empty);
        // the times, which vary
        let lines: Vec<String> = lines
            .iter()
            .map(|l| match l.find(" ms") {
                Some(at) => {
                    let digits = l[..at].trim_end_matches(|c: char| c.is_ascii_digit()).len();
                    format!("{}{{ms}}{}", &l[..digits], &l[at..])
                }
                None => l.clone(),
            })
            .collect();
        let error = format!(
            "An error occurred trying to start process 'aipet-doctor-test-no-powershell.exe' with working directory \
             '{}'. {}",
            std::env::current_dir().unwrap().display(),
            system_text(&io::Error::from_raw_os_error(2))
        );
        let failed = format!(
            "  [FAIL] aipet-doctor-test-no-powershell.exe: exit code -2 after {{ms}} ms: {}",
            short(&error, 200)
        );
        let section = format!("{NEWLINE}Running AiPet's hook the way Codex does");
        assert_eq!(
            lines,
            [
                section.clone(),
                failed.clone(),
                "         and through the cmd.exe fallback:".to_owned(),
                "  [ok]   cmd.exe: ran, printed nothing, exit 0, {ms} ms (the pet isn't running, so the hook did \
                 nothing)"
                    .to_owned(),
                section,
                failed,
                "         not through the cmd.exe fallback: the command uses PowerShell's & '...' form, which cmd.exe \
                 can't run"
                    .to_owned(),
            ]
        );
        assert_eq!(counted, (2, 0));
    }

    /// A program that outlives its time is ended, with what it started; one that can't start is -2, with the reason.
    #[test]
    fn a_program_is_given_its_time_and_no_more() {
        let (file, args): (&str, &[&str]) = if cfg!(windows) {
            ("cmd.exe", &["/C", "ping -n 30 127.0.0.1 > nul"])
        } else {
            ("/bin/sh", &["-c", "sleep 30"])
        };
        let run = Run {
            file: OsStr::new(file),
            args: Args::list(args),
            timeout: 1,
            env: &[],
        };
        let started = Instant::now();
        assert_eq!(capture(&run, None), (-1, String::new(), String::new()));
        assert!(started.elapsed() < Duration::from_secs(10), "{:?}", started.elapsed());
        let run = Run {
            file: OsStr::new("aipet-doctor-test-nothing-here"),
            args: Args::list(&[]),
            timeout: 5,
            env: &[],
        };
        let (code, out, err) = capture(&run, None);
        assert_eq!((code, out.as_str()), (-2, ""));
        assert!(err.contains(&system_text(&io::Error::from_raw_os_error(2))), "{err}");
    }
}
