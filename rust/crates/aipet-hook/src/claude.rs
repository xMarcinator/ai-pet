//! Claude Code's registration (`ClaudeConfig`, `src/AiPet.Hook/Install.cs:73-246`): AiPet's hooks in settings.json,
//! in `$CLAUDE_CONFIG_DIR` or else `~/.claude`. Claude runs the exe directly with its arguments (no shell), so the
//! path needs no quoting on any OS.
//!
//! Re-running is safe: AiPet's own entries (and older AiPet and ClaudePet ones) are replaced, everything else is
//! kept, and nothing is written when the registration is already exactly right. The file is read, compared and
//! written as System.Text.Json does it ([`crate::json_out`]), and a change keeps a backup: the newest three.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::install::{NEWLINE, is_file, read_text, save, thrown, upper};
use crate::json::{Node, Object};
use crate::json_out::{self, deep_equals, member, read};

/// One of Claude's hook events AiPet registers for.
pub(crate) struct Event {
    pub(crate) name: &'static str,
    /// The group matches every tool (`"*"`); without a matcher Claude takes every one of the others (all
    /// compactions, sub-agent types, MCP servers).
    pub(crate) matcher: bool,
    /// Claude doesn't wait for it (`"async": true`).
    pub(crate) background: bool,
}

const fn event(name: &'static str, matcher: bool, background: bool) -> Event {
    Event {
        name,
        matcher,
        background,
    }
}

/// `ClaudeConfig.Events`. All run in the background except Stop, which runs in line so the final "done" state is
/// written after the turn's other events (the hook never returns anything to Claude).
pub(crate) const EVENTS: [Event; 17] = [
    event("SessionStart", false, true),
    event("UserPromptSubmit", false, true),
    event("PreToolUse", true, true),
    event("PostToolUse", true, true),
    event("PostToolUseFailure", true, true),
    event("PermissionRequest", true, true),
    event("PermissionDenied", false, true),
    event("Notification", false, true),
    event("Elicitation", false, true),
    event("ElicitationResult", false, true),
    event("PreCompact", false, true),
    event("PostCompact", false, true),
    event("SubagentStart", false, true),
    event("SubagentStop", false, true),
    event("Stop", false, false),
    event("StopFailure", false, true),
    event("SessionEnd", false, true),
];

/// The environment the registration reads: the process's ([`process_vars`]), or a test's.
pub(crate) type Vars<'a> = &'a dyn Fn(&str) -> Option<OsString>;

pub(crate) fn process_vars(name: &str) -> Option<OsString> {
    std::env::var_os(name)
}

/// `ClaudeConfig.Dir`: `$CLAUDE_CONFIG_DIR` when it is set, else `.claude` in the user's home folder.
fn dir(vars: Vars) -> PathBuf {
    match vars("CLAUDE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => dir.into(),
        _ => aipet_ipc::paths::home().join(".claude"),
    }
}

/// `ClaudeConfig.Settings`.
pub(crate) fn settings(vars: Vars) -> PathBuf {
    dir(vars).join("settings.json")
}

/// `ClaudeConfig.IsOurs`: a hook whose command runs aipet-hook or ClaudePet.exe (in any case), or the old Python
/// hook, in its command or its arguments. A command or an argument that isn't a string is read as its JSON; a hook
/// that isn't an object throws, as reading it does in the C#.
pub(crate) fn is_ours(hook: &Node) -> Result<bool, String> {
    if *hook == Node::Null {
        return Ok(false);
    }
    let command = member(hook, "command")?.map_or_else(String::new, |c| json_out::to_string(c, NEWLINE));
    let up = upper(&command);
    if up.contains("AIPET-HOOK") || up.contains("CLAUDEPET.EXE") || legacy_hook(&command) {
        return Ok(true);
    }
    Ok(match member(hook, "args")? {
        Some(Node::Array(args)) => args
            .iter()
            .any(|a| *a != Node::Null && legacy_hook(&json_out::to_string(a, NEWLINE))),
        _ => false,
    })
}

/// `LegacyHook`: the old Python hook in its own place, `[\\/]\.claude[\\/]pet[\\/]hook\.py(?=$|["'\s])` with
/// .NET's `IgnoreCase` (where `k` is also the Kelvin sign). Other tools' pet/hook.py are left alone.
fn legacy_hook(s: &str) -> bool {
    const PATTERN: &str = "/.claude/pet/hook.py";
    let chars: Vec<char> = s.chars().collect();
    let matches = |c: char, p: char| match p {
        '/' => c == '/' || c == '\\',
        'k' => c == 'k' || c == 'K' || c == '\u{212A}',
        _ => c.to_ascii_lowercase() == p,
    };
    (0..chars.len()).any(|at| {
        let rest = &chars[at..];
        rest.len() >= PATTERN.len()
            && PATTERN.chars().zip(rest).all(|(p, &c)| matches(c, p))
            && rest
                .get(PATTERN.len())
                .is_none_or(|&c| c == '"' || c == '\'' || c.is_whitespace())
    })
}

/// `ClaudeConfig.Plugin`: the AiPet plugin (`aipet@<marketplace>`) enabled in settings.json and installed, the first
/// such. It brings its own hooks (plugins/aipet/hooks/hooks.json), which Claude runs as well as any in settings.json.
/// A settings.json synced from another machine can enable a plugin this one never installed, and then nothing runs
/// its hooks.
pub(crate) fn plugin(dir: &Path, root: Option<&Object>) -> Result<Option<String>, String> {
    let Some(root) = root else { return Ok(None) };
    read(root)?;
    let Some(Node::Object(plugins)) = root.get("enabledPlugins") else {
        return Ok(None);
    };
    read(plugins)?;
    Ok(plugins
        .iter()
        .find(|(id, on)| id.starts_with("aipet@") && **on == Node::Bool(true) && installed(dir, id))
        .map(|(id, _)| id.to_owned()))
}

/// `Installed`: Claude Code lists what's installed in plugins/installed_plugins.json
/// (`{"version":2,"plugins":{"<id>":[..]}}`), and keeps each plugin in plugins/cache/<marketplace>/<plugin>/<version>/.
/// A list it can't read is no answer: the cache still is.
fn installed(dir: &Path, id: &str) -> bool {
    let listed = |text: &str| -> Result<bool, String> {
        let root = json_out::parse(text)?;
        if root == Node::Null {
            return Ok(false);
        }
        let Some(Node::Object(plugins)) = member(&root, "plugins")? else {
            return Ok(false);
        };
        read(plugins)?;
        Ok(match plugins.get(id) {
            None | Some(Node::Null) => false,
            Some(Node::Array(list)) => !list.is_empty(),
            Some(_) => true,
        })
    };
    let plugins = dir.join("plugins");
    if read_text(&plugins.join("installed_plugins.json")).is_ok_and(|text| listed(&text) == Ok(true)) {
        return true;
    }
    let marketplace = &id[id.find('@').map_or(0, |at| at + 1)..];
    let cache = plugins.join("cache").join(marketplace).join("aipet");
    fs::read_dir(cache).is_ok_and(|mut versions| versions.any(|v| v.is_ok_and(|v| v.path().is_dir())))
}

/// `ClaudeConfig.PluginRuns`: whether the plugin's hooks can run here. On Windows Claude runs them in Git Bash
/// (`"shell": "bash"`).
pub(crate) fn plugin_runs(vars: Vars) -> bool {
    !cfg!(windows) || git_bash(vars).is_some()
}

/// `ClaudeConfig.GitBash`: Git Bash where Claude Code looks for it (as install.ps1's Find-GitBash does):
/// `$CLAUDE_CODE_GIT_BASH_PATH`, next to the first git.exe on PATH (`<Git>\cmd\git.exe`), then the usual install
/// folders.
fn git_bash(vars: Vars) -> Option<PathBuf> {
    let set = |name: &str| vars(name).filter(|v| !v.is_empty());
    let mut candidates: Vec<PathBuf> = set("CLAUDE_CODE_GIT_BASH_PATH")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    let separator = if cfg!(windows) { ';' } else { ':' };
    for dir in vars("PATH").unwrap_or_default().to_string_lossy().split(separator) {
        let git = Path::new(dir.trim_matches('"')).join("git.exe");
        if !is_file(&git) {
            continue;
        }
        // a git.exe with no folder above its own has no Git folder: the C# fails there, and looks on
        let Some(root) = git.parent().and_then(Path::parent) else {
            continue;
        };
        candidates.push(root.join("bin").join("bash.exe"));
        break;
    }
    if let Some(programs) = set("ProgramFiles") {
        candidates.push(Path::new(&programs).join("Git").join("bin").join("bash.exe"));
    }
    if let Some(local) = set("LOCALAPPDATA") {
        candidates.push(
            Path::new(&local)
                .join("Programs")
                .join("Git")
                .join("bin")
                .join("bash.exe"),
        );
    }
    candidates.into_iter().find(|c| !c.as_os_str().is_empty() && is_file(c))
}

/// `ClaudeConfig.Install(exe)`: registers `exe`, or with the plugin enabled and able to run, takes out the hooks it
/// replaces. Either way the pet gets every event, so this isn't a failure: installers run --install unchecked. What it
/// says goes to `say`, a line at a time (`Console.WriteLine`).
pub(crate) fn install(exe: &Path, vars: Vars, say: &mut dyn FnMut(&str)) -> Result<i32, String> {
    let dir = dir(vars);
    let settings = dir.join("settings.json");
    let text = if is_file(&settings) {
        read_text(&settings)?
    } else {
        String::new()
    };
    let root = if blank(&text) {
        None
    } else {
        Some(json_out::parse(&text)?)
    };
    let root = match &root {
        Some(Node::Object(root)) => Some(root),
        _ => None,
    };
    let plugin = plugin(&dir, root)?;
    let shown = settings.display();
    if let Some(plugin) = &plugin
        && plugin_runs(vars)
    {
        // hooks registered here earlier would report every event a second time: the plugin replaces them
        say(&if edit(&settings, None)? {
            format!(
                "The {plugin} plugin is enabled in {shown} and reports every event, so the AiPet hooks registered \
                 there earlier were removed (the plugin replaces them)."
            )
        } else {
            format!(
                "The {plugin} plugin is enabled in {shown} and already reports every event, so no hooks were \
                 registered (they would report each one twice)."
            )
        });
        say(&format!(
            "To use hooks in settings.json instead, disable the plugin (claude plugin disable {plugin}) and install \
             again."
        ));
        return Ok(0);
    }
    let command = exe.to_string_lossy().replace('\\', "/");
    say(&if edit(&settings, Some(&command))? {
        format!("AiPet hooks registered for Claude Code ({shown})")
    } else {
        format!("AiPet hooks for Claude Code are already registered ({shown}, unchanged)")
    });
    if let Some(plugin) = &plugin {
        say(&format!(
            "The {plugin} plugin is enabled too, but its hooks need Git Bash, which wasn't found. Once Git for \
             Windows is installed, run aipet-hook --uninstall claude, or every event is reported twice."
        ));
    }
    say("Check with: aipet-hook --doctor claude");
    Ok(0)
}

/// `ClaudeConfig.Uninstall`.
pub(crate) fn uninstall(vars: Vars, say: &mut dyn FnMut(&str)) -> Result<i32, String> {
    say(if edit(&settings(vars), None)? {
        "AiPet hooks removed from Claude Code"
    } else {
        "No AiPet hooks were registered with Claude Code"
    });
    Ok(0)
}

/// `string.IsNullOrWhiteSpace`.
fn blank(text: &str) -> bool {
    text.chars().all(char::is_whitespace)
}

/// `ClaudeConfig.Edit`: drops AiPet's entries, adds new ones for `command` if given, and saves only if that changed
/// anything, with a backup. Whether it changed anything.
///
/// What the C# reads throws as it does there: an object that names a member twice once it is read, a hook that isn't
/// an object. Nothing is written then.
fn edit(path: &Path, command: Option<&str>) -> Result<bool, String> {
    let exists = is_file(path);
    if !exists && command.is_none() {
        return Ok(false);
    }
    let before = if exists { read_text(path)? } else { String::new() };
    let mut root = match (!blank(&before)).then(|| json_out::parse(&before)).transpose()? {
        Some(Node::Object(root)) => root,
        _ => Object::default(),
    };
    read(&root)?;
    let had_hooks = matches!(root.get("hooks"), Some(Node::Object(_)));
    if !had_hooks {
        // in its place when it is there as something else
        match root.get_mut("hooks") {
            Some(hooks) => *hooks = Node::Object(Object::default()),
            None => root.push("hooks", Node::Object(Object::default())),
        }
    }
    let Some(Node::Object(hooks)) = root.get_mut("hooks") else {
        unreachable!("hooks is an object now")
    };
    read(hooks)?;
    for value in hooks.values_mut() {
        let Node::Array(groups) = value else { continue };
        let mut kept = Vec::with_capacity(groups.len());
        for group in groups.iter_mut() {
            let Node::Object(group) = group else {
                kept.push(true);
                continue;
            };
            read(group)?;
            let Some(Node::Array(list)) = group.get_mut("hooks") else {
                kept.push(true);
                continue;
            };
            let ours = list.iter().map(is_ours).collect::<Result<Vec<bool>, String>>()?;
            let mut ours = ours.into_iter();
            list.retain(|_| !ours.next().unwrap_or(false));
            // a group left empty goes, the user's own empty ones too
            kept.push(!list.is_empty());
        }
        let mut kept = kept.into_iter();
        groups.retain(|_| kept.next().unwrap_or(true));
    }
    if let Some(command) = command {
        for event in &EVENTS {
            let mut hook = Object::default();
            hook.push("type", Node::String("command".into()));
            hook.push("command", Node::String(command.into()));
            hook.push(
                "args",
                Node::Array(vec![Node::String("--agent".into()), Node::String("claude".into())]),
            );
            let group = group(event, hook);
            match hooks.get_mut(event.name) {
                Some(Node::Array(groups)) => groups.push(group),
                Some(other) => *other = Node::Array(vec![group]),
                None => hooks.push(event.name, Node::Array(vec![group])),
            }
        }
    }
    let empty: Vec<String> = hooks
        .iter()
        .filter(|(_, v)| matches!(v, Node::Array(a) if a.is_empty()))
        .map(|(k, _)| k.to_owned())
        .collect();
    hooks.retain(|k| !empty.iter().any(|e| e == k));
    // leave no empty "hooks" behind
    if hooks.iter().len() == 0 && (!had_hooks || command.is_none()) {
        root.remove("hooks");
    }

    let root = Node::Object(root);
    let was = json_out::parse(if blank(&before) { "{}" } else { &before })?;
    if deep_equals(&was, &root)? {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| thrown(&e, parent))?;
    }
    if is_file(path) {
        backup(path);
    }
    // a symlinked settings.json (dotfiles): replace the file it points at, not the link
    save(&real_path(path), &json_out::indented(&root, NEWLINE))?;
    Ok(true)
}

/// `ClaudeConfig.Group`: an event's group around its one hook, which comes with its type and command; the timeout
/// and async follow them. The plugin's hooks (plugin_hooks) are the same with another command.
pub(crate) fn group(event: &Event, mut hook: Object) -> Node {
    hook.push("timeout", Node::Number("5".into()));
    if event.background {
        hook.push("async", Node::Bool(true));
    }
    let mut group = Object::default();
    group.push("hooks", Node::Array(vec![Node::Object(hook)]));
    if event.matcher {
        group.push("matcher", Node::String("*".into()));
    }
    Node::Object(group)
}

/// `RealPath`: the file a symlink ends at (`File.ResolveLinkTarget(path, returnFinalTarget: true)`), each link's
/// relative target taken from where that link is, and `..` taken out of the text. Not a link, or a chain past
/// .NET's 40 links: the path itself.
fn real_path(path: &Path) -> PathBuf {
    const MAX_FOLLOWED_LINKS: usize = 40;
    let Ok(mut target) = fs::read_link(path) else {
        return path.to_owned();
    };
    let mut current = path.to_owned();
    for _ in 0..MAX_FOLLOWED_LINKS {
        current = match current.parent() {
            Some(dir) if target.is_relative() => dir.join(&target),
            _ => target,
        };
        match fs::read_link(&current) {
            Ok(next) => target = next,
            Err(_) => return full_path(&current),
        }
    }
    path.to_owned()
}

/// `Path.GetFullPath`, for a path that is already absolute.
#[cfg(unix)]
fn full_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut full = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                full.pop();
            }
            Component::CurDir => {}
            other => full.push(other),
        }
    }
    full
}

#[cfg(not(unix))]
fn full_path(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}

/// `Backup`: settings.json.aipet-<local time>.bak before each change, and the newest three of them kept. It never
/// fails the change.
fn backup(path: &Path) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return;
    };
    let mut copy = path.as_os_str().to_owned();
    copy.push(format!(".aipet-{}.bak", local_stamp()));
    if fs::copy(path, copy).is_err() {
        return;
    }
    // Directory.GetFiles(dir, "settings.json.aipet-*.bak"): the name's case counts on Unix only
    let fold = |s: &str| if cfg!(windows) { upper(s) } else { s.to_owned() };
    let prefix = fold(&format!("{}.aipet-", name.to_string_lossy()));
    let suffix = fold(".bak");
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut backups: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .filter(|e| {
            let name = fold(&e.file_name().to_string_lossy());
            name.len() >= prefix.len() + suffix.len() && name.starts_with(&prefix) && name.ends_with(&suffix)
        })
        .map(|e| e.path())
        .collect();
    backups.sort_unstable_by(|a, b| b.as_os_str().cmp(a.as_os_str()));
    for old in backups.iter().skip(3) {
        if fs::remove_file(old).is_err() {
            return;
        }
    }
}

/// The local time as `yyyyMMdd-HHmmss`.
#[cfg(unix)]
fn local_stamp() -> String {
    // SAFETY: time only returns the time when given no pointer; localtime_r writes only tm
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    // SAFETY: an all-zero tm is a valid value to be filled in
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are to live locals
    if unsafe { libc::localtime_r(&now, &mut tm) }.is_null() {
        return "00000000-000000".to_owned();
    }
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

#[cfg(windows)]
fn local_stamp() -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;

    // SAFETY: an all-zero SYSTEMTIME is a valid value to be filled in
    let mut now = unsafe { std::mem::zeroed() };
    // SAFETY: GetLocalTime only writes the SYSTEMTIME it is given
    unsafe { GetLocalTime(&mut now) };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute, now.wSecond
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder of the test's own, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("aipet-hook-claude-{test}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn touch(&self, parts: &[&str]) -> PathBuf {
            let file = parts.iter().fold(self.0.clone(), |p, part| p.join(part));
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, "").unwrap();
            file
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// RegistrationTests.GitBash_IsFoundWhereClaudeLooks: `$CLAUDE_CODE_GIT_BASH_PATH`, next to git.exe on PATH
    /// (`<Git>\cmd\git.exe`), then the usual install folders; each wins over the ones after it.
    #[test]
    fn git_bash_is_found_where_claude_looks() {
        let root = Scratch::new("git-bash");
        let nowhere = root.0.join("nowhere");
        let mut vars: Vec<(&str, OsString)> = vec![
            ("PATH", nowhere.clone().into()),
            ("ProgramFiles", nowhere.clone().into()),
            ("LOCALAPPDATA", nowhere.clone().into()),
        ];
        let found =
            |vars: &[(&str, OsString)]| git_bash(&|name| vars.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone()));
        let set = |vars: &mut Vec<(&str, OsString)>, name: &'static str, value: PathBuf| {
            vars.retain(|(n, _)| *n != name);
            vars.push((name, value.into()));
        };
        assert_eq!(found(&vars), None);

        let local = root.touch(&["local", "Programs", "Git", "bin", "bash.exe"]);
        set(&mut vars, "LOCALAPPDATA", root.0.join("local"));
        assert_eq!(found(&vars), Some(local));
        let programs = root.touch(&["pf", "Git", "bin", "bash.exe"]);
        set(&mut vars, "ProgramFiles", root.0.join("pf"));
        assert_eq!(found(&vars), Some(programs));
        root.touch(&["scoop", "Git", "cmd", "git.exe"]);
        let scoop = root.touch(&["scoop", "Git", "bin", "bash.exe"]);
        let separator = if cfg!(windows) { ";" } else { ":" };
        let path = format!(
            "{}{separator}\"{}\"",
            nowhere.display(),
            root.0.join("scoop").join("Git").join("cmd").display()
        );
        set(&mut vars, "PATH", path.into());
        assert_eq!(found(&vars), Some(scoop));
        let custom = root.touch(&["custom", "bash.exe"]);
        set(&mut vars, "CLAUDE_CODE_GIT_BASH_PATH", custom.clone());
        assert_eq!(found(&vars), Some(custom));
    }

    /// The golden plugin-no-git-bash case (tests/golden/registration/claude.json), which tests/registration.rs can't
    /// run through the hook: Windows gives every process its ProgramFiles, where Git Bash usually is. Here the
    /// environment has none, as the C# had when it wrote the case.
    #[cfg(windows)]
    #[test]
    fn a_plugin_without_git_bash_is_the_golden_case() {
        use serde_json::Value;

        let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/registration/claude.json");
        let corpus: Value = serde_json::from_str(&fs::read_to_string(golden).unwrap()).unwrap();
        assert_eq!(corpus["os"], "windows", "the case gives another answer elsewhere");
        let case = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "plugin-no-git-bash")
            .unwrap();
        let root = Scratch::new("no-git-bash");
        let (exe, written) = (r"C:\Users\Ann\aipet-hook.exe", "C:/Users/Ann/aipet-hook.exe");
        for (path, text) in case["setup"]["files"].as_object().unwrap() {
            let path = root.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text.as_str().unwrap().replace("{exe}", written)).unwrap();
        }
        let config = root.0.join("claude");
        let empty = root.0.join("nobin");
        fs::create_dir_all(&empty).unwrap();
        let vars = |name: &str| match name {
            "CLAUDE_CONFIG_DIR" => Some(config.clone().into()),
            "PATH" | "ProgramFiles" | "LOCALAPPDATA" => Some(empty.clone().into()),
            _ => None,
        };
        let settings = config.join("settings.json");
        for step in case["steps"].as_array().unwrap() {
            let mut said = String::new();
            let say = &mut |line: &str| said.push_str(&format!("{line}{NEWLINE}"));
            let exit = match step["action"].as_str().unwrap() {
                "install" => install(Path::new(exe), &vars, say),
                _ => uninstall(&vars, say),
            };
            assert_eq!(exit, Ok(step["exit"].as_i64().unwrap() as i32), "{step}");
            assert_eq!(
                said.replace(&settings.display().to_string(), "{settings}"),
                step["stdout"],
                "{step}"
            );
            let file = fs::read_to_string(&settings).unwrap().replace(written, "{exe}");
            assert_eq!(file, step["files"]["claude/settings.json"]["text"], "{step}");
        }
    }

    /// A chain of links ends at the file, each relative target taken from its own link's folder, `.` and `..` out.
    #[cfg(unix)]
    #[test]
    fn real_path_follows_a_chain_of_links() {
        use std::os::unix::fs::symlink;

        let root = Scratch::new("real-path");
        let claude = root.0.join("claude");
        fs::create_dir_all(&claude).unwrap();
        let real = root.touch(&["dotfiles", "real.json"]);
        symlink("link2.json", claude.join("settings.json")).unwrap();
        symlink("../dotfiles/./real.json", claude.join("link2.json")).unwrap();
        assert_eq!(real_path(&claude.join("settings.json")), real);
        // not a link, or a loop past the limit: the path itself
        assert_eq!(real_path(&real), real);
        symlink("loop.json", claude.join("loop.json")).unwrap();
        assert_eq!(real_path(&claude.join("loop.json")), claude.join("loop.json"));
    }
}
