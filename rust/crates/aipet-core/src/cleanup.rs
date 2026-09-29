//! The uninstaller's hook cleanup: `aipet-hook --uninstall` for each agent this install registered
//! (`src/AiPet.Core/HookCleanup.cs`).
//!
//! install.ps1 registers the hook directly (`aipet-hook --install claude`) when Claude's plugin can't run for lack of
//! Git Bash, and that registration names the installed exe. Once the uninstaller has deleted it, Claude would fail to
//! run it on every event, so Velopack's uninstall hook runs `aipet-hook --uninstall <agent>` first, for each agent
//! whose config names this install's hook. Registrations of another copy (the portable zip, a build from source) are
//! left alone, and so are the plugins, which the agents manage themselves.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// How long each `--uninstall` gets.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The agents' config files the hook registers in, which the uninstaller reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigs {
    /// Claude's `settings.json`, found as the hook's `ClaudeConfig.Settings` finds it.
    pub claude_settings: PathBuf,
    /// Codex's user layer, where the hook's `CodexConfig` registers: `config.toml`, or `hooks.json`.
    pub codex_files: [PathBuf; 2],
}

impl AgentConfigs {
    /// `$CLAUDE_CONFIG_DIR/settings.json` (else `~/.claude`), and `config.toml` and `hooks.json` in Codex's home, as
    /// the environment has them now.
    pub fn current() -> Self {
        let claude = std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| aipet_ipc::paths::home().join(".claude"));
        let codex = aipet_ipc::paths::codex_home();
        AgentConfigs {
            claude_settings: claude.join("settings.json"),
            codex_files: [codex.join("config.toml"), codex.join("hooks.json")],
        }
    }

    /// The agents (`claude`, `codex`) whose config names one of these hook paths (`HookCleanup.Agents`).
    pub fn agents(&self, hooks: &[&str], ignore_case: bool) -> Vec<&'static str> {
        let read = |path: &Path| std::fs::read(path).ok().map(|bytes| crate::config::read_text(&bytes));
        let mut agents = Vec::new();
        if names(read(&self.claude_settings).as_deref(), hooks, ignore_case) {
            agents.push("claude");
        }
        if self
            .codex_files
            .iter()
            .any(|f| names(read(f).as_deref(), hooks, ignore_case))
        {
            agents.push("codex");
        }
        agents
    }
}

/// Whether a config file's text names one of the hook paths (`HookCleanup.Names`): with `/` or `\` (settings.json
/// has `/`, Codex's command `\`), escaped as in JSON and TOML strings (`\\`) or not (TOML literal strings), with a
/// `'` doubled as PowerShell quotes it, and as the whole file name: a path that only starts with it
/// (`aipet-hook.exe.old`) isn't the hook.
///
/// It works on UTF-16 units, as the C# does. Past ASCII, what continues a file name is a letter or a number by
/// Rust's reckoning; .NET's `char.IsLetterOrDigit` leaves out marks and numbers that aren't decimal digits, which
/// never follow a hook's name in an agent's config.
pub fn names(text: Option<&str>, hooks: &[&str], ignore_case: bool) -> bool {
    let Some(text) = text.filter(|t| !t.is_empty()) else {
        return false;
    };
    let fold = |units: Vec<u16>| {
        if ignore_case {
            units.into_iter().map(upper).collect()
        } else {
            units
        }
    };
    let original = slashes(&text.replace("\\\\", "\\"));
    let searched = fold(original.clone());
    for hook in hooks.iter().filter(|h| !h.is_empty()) {
        let plain = slashes(hook);
        let quoted = slashes(&hook.replace('\'', "''"));
        let forms = if quoted == plain {
            vec![plain]
        } else {
            vec![plain, quoted]
        };
        for form in forms.into_iter().map(fold) {
            let mut from = 0;
            while let Some(at) = index_of(&searched, &form, from) {
                let end = at + form.len();
                if end == original.len() || !continues_name(original[end]) {
                    return true;
                }
                from = at + 1;
            }
        }
    }
    false
}

/// `\` as `/`, in UTF-16.
fn slashes(s: &str) -> Vec<u16> {
    s.encode_utf16()
        .map(|u| if u == u16::from(b'\\') { u16::from(b'/') } else { u })
        .collect()
}

/// OrdinalIgnoreCase's upper case of a UTF-16 unit: its simple upper case when that is one unit too.
fn upper(unit: u16) -> u16 {
    let Some(c) = char::from_u32(u32::from(unit)) else {
        return unit;
    };
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) if u.len_utf16() == 1 => u as u16,
        _ => unit,
    }
}

fn index_of(text: &[u16], what: &[u16], from: usize) -> Option<usize> {
    if what.is_empty() || from > text.len() {
        return None;
    }
    text[from..]
        .windows(what.len())
        .position(|w| w == what)
        .map(|at| at + from)
}

/// `char.IsLetterOrDigit(c) || c is '.' or '-' or '_'`; half a surrogate pair is neither.
fn continues_name(unit: u16) -> bool {
    matches!(unit, 0x2E | 0x2D | 0x5F) || char::from_u32(u32::from(unit)).is_some_and(char::is_alphanumeric)
}

/// Runs `<hook> --uninstall <agent>` for each agent whose config names this hook (`HookCleanup.Run`): by its path,
/// or on Windows with its folder, or the whole path, in 8.3 short form, as Codex's command has it when the path has
/// spaces. Each run gets 10 s and no window, and what it says goes to the app's log. Nothing here fails the
/// uninstall.
pub fn run(hook: &Path) {
    run_with(&AgentConfigs::current(), hook, &mut |line| crate::log::write(&line));
}

fn run_with(configs: &AgentConfigs, hook: &Path, log: &mut dyn FnMut(String)) {
    if !hook.is_file() {
        return;
    }
    let paths: Vec<String> = forms(hook).iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let hooks: Vec<&str> = paths.iter().map(String::as_str).collect();
    for agent in configs.agents(&hooks, cfg!(windows)) {
        log(match uninstall(hook, agent) {
            Ok(Some((code, said))) => {
                format!(
                    "uninstall: aipet-hook --uninstall {agent} (exit {code}): {}",
                    one_line(said.trim())
                )
            }
            Ok(None) => format!("uninstall: aipet-hook --uninstall {agent} took too long; stopped it"),
            Err(e) => format!("uninstall: couldn't run aipet-hook --uninstall {agent}: {e}"),
        });
    }
}

/// Runs the hook's `--uninstall`: its exit code and what it said (stdout, then stderr), or `None` when it took too
/// long and was stopped.
fn uninstall(hook: &Path, agent: &str) -> std::io::Result<Option<(i32, String)>> {
    let mut command = Command::new(hook);
    command
        .args(["--uninstall", agent])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn()?;
    // read as it comes, so a full pipe can't hold the hook up
    let output = drain(child.stdout.take());
    let errors = drain(child.stderr.take());
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            stop(child);
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(10));
    };
    let said = output.join().unwrap_or_default() + &errors.join().unwrap_or_default();
    Ok(Some((exit_code(status), said)))
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn stop(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// .NET's `Process.ExitCode`: 128 plus the signal for a process a signal ended.
fn exit_code(status: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return 128 + signal;
    }
    status.code().unwrap_or(-1)
}

/// `ReplaceLineEndings(" ")`: CR LF, CR, LF, NEL, LS, PS and FF each as one space.
fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                chars.next_if_eq(&'\n');
                out.push(' ');
            }
            '\n' | '\u{85}' | '\u{2028}' | '\u{2029}' | '\u{C}' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// The hook's path.
#[cfg(not(windows))]
fn forms(hook: &Path) -> Vec<PathBuf> {
    vec![hook.to_path_buf()]
}

/// The hook's path, then with its folder in 8.3 form, then all of it in 8.3 form (older installs shortened the file
/// name too).
#[cfg(windows)]
fn forms(hook: &Path) -> Vec<PathBuf> {
    let mut forms = vec![hook.to_path_buf()];
    if let Some(dir) = hook.parent().and_then(short_path)
        && let Some(name) = hook.file_name()
    {
        forms.push(dir.join(name));
    }
    forms.extend(short_path(hook));
    forms
}

/// `C:\Users\First Last\…` as `C:\Users\FIRSTL~1\…`; `None` when there's no short name.
#[cfg(windows)]
fn short_path(path: &Path) -> Option<PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    let long: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut buf = [0u16; 1024];
    // SAFETY: long is NUL-terminated; buf's length is what is passed
    let n = unsafe { GetShortPathNameW(long.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    (n > 0 && n < buf.len()).then(|| PathBuf::from(std::ffi::OsString::from_wide(&buf[..n])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> (Dir, AgentConfigs) {
        let dir = std::env::temp_dir().join(format!("aipet-core-cleanup-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let configs = AgentConfigs {
            claude_settings: dir.join("settings.json"),
            codex_files: [dir.join("config.toml"), dir.join("hooks.json")],
        };
        (Dir(dir), configs)
    }

    #[test]
    fn line_endings_become_spaces() {
        assert_eq!(
            one_line("a\r\nb\rc\nd\u{85}e\u{2028}f\u{2029}g\u{C}h\r\n\ni"),
            "a b c d e f g h  i"
        );
    }

    #[test]
    fn the_configs_are_where_the_hook_registers() {
        let configs = AgentConfigs::current();
        assert!(configs.claude_settings.ends_with("settings.json"));
        let codex = aipet_ipc::paths::codex_home();
        assert_eq!(
            configs.codex_files,
            [codex.join("config.toml"), codex.join("hooks.json")]
        );
    }

    /// Run starts `<hook> --uninstall <agent>` for each agent that names it, and only those
    /// (HookCleanupTests.Run_UninstallsFromTheAgentsThatNameTheHook). The hook is a stand-in script.
    #[cfg(unix)]
    #[test]
    fn run_uninstalls_from_the_agents_that_name_the_hook() {
        use std::os::unix::fs::PermissionsExt;
        let (d, configs) = dir("run");
        let hook = d.0.join("aipet-hook");
        let calls = d.0.join("calls");
        fs::write(
            &hook,
            format!(
                "#!/bin/sh\necho \"$*\" >> '{}'\necho removed\necho warned >&2\n",
                calls.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            &configs.claude_settings,
            format!(
                r#"{{"hooks":{{"Stop":[{{"hooks":[{{"type":"command","command":"{}"}}]}}]}}}}"#,
                hook.display()
            ),
        )
        .unwrap();
        fs::write(
            &configs.codex_files[0],
            "[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ncommand = \"'/elsewhere/aipet-hook' --agent codex\"\n",
        )
        .unwrap();

        let mut logged = Vec::new();
        run_with(&configs, &hook, &mut |line| logged.push(line));
        assert_eq!(fs::read_to_string(&calls).unwrap(), "--uninstall claude\n");
        assert_eq!(
            logged,
            ["uninstall: aipet-hook --uninstall claude (exit 0): removed warned"]
        );

        // a hook that's gone (a broken install) has nothing to run
        fs::remove_file(&calls).unwrap();
        run_with(&configs, &d.0.join("missing").join("aipet-hook"), &mut |line| {
            logged.push(line)
        });
        assert!(!calls.exists());
        assert_eq!(logged.len(), 1);
    }

    /// A hook that hangs is stopped, and Run returns well within the 30 s Velopack gives the uninstall hook
    /// (HookCleanupTests.Run_StopsAHookThatHangs).
    #[cfg(unix)]
    #[test]
    fn run_stops_a_hook_that_hangs() {
        use std::os::unix::fs::PermissionsExt;
        let (d, configs) = dir("hang");
        let hook = d.0.join("aipet-hook");
        fs::write(&hook, "#!/bin/sh\nexec sleep 60\n").unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            &configs.claude_settings,
            format!(r#"{{"command":"{}"}}"#, hook.display()),
        )
        .unwrap();
        let started = Instant::now();
        let mut logged = Vec::new();
        run_with(&configs, &hook, &mut |line| logged.push(line));
        let took = started.elapsed().as_secs_f64();
        assert!((9.0..20.0).contains(&took), "{took}");
        assert_eq!(
            logged,
            ["uninstall: aipet-hook --uninstall claude took too long; stopped it"]
        );
    }

    /// On Windows a program that is surely there stands in for the hook: it refuses the arguments, and what it says
    /// is logged on one line with its exit code. Paths are matched whatever their case.
    #[cfg(windows)]
    #[test]
    fn run_logs_what_the_hook_said() {
        let (_d, configs) = dir("run");
        let windows = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| r"C:\Windows".into());
        let hook = windows.join("System32").join("whoami.exe");
        let named = hook.to_string_lossy().replace('\\', "/").to_uppercase();
        fs::write(&configs.claude_settings, format!(r#"{{"command":"{named}"}}"#)).unwrap();
        let mut logged = Vec::new();
        run_with(&configs, &hook, &mut |line| logged.push(line));
        assert_eq!(logged.len(), 1, "{logged:?}");
        assert!(
            logged[0].starts_with("uninstall: aipet-hook --uninstall claude (exit 1): "),
            "{}",
            logged[0]
        );
        assert!(!logged[0].contains(['\r', '\n']), "{}", logged[0]);
    }
}
