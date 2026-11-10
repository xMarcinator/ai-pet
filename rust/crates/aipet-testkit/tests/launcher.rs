//! The plugin's launcher, plugins/aipet/native/aipet-hook.sh, rehosted from tests/AiPet.Tests/LauncherTests.cs: run
//! with sh as the plugin runs it, with a fake uname first on PATH and fake hooks next to it. Whatever it finds, it
//! prints nothing and exits 0; it runs a hook only where one can load. Unix only (Git Bash on Windows isn't something
//! CI has).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use aipet_testkit::{Sandbox, finish, repo, write};

const EVENT: &str = "{\"hook_event_name\":\"Stop\"}";

/// A plugin folder with the launcher, and a fake hook at each rid that records its arguments and stdin.
struct Plugin {
    box_: Sandbox,
    native: PathBuf,
    ran: PathBuf,
    uname_calls: PathBuf,
}

impl Plugin {
    fn new() -> Plugin {
        let box_ = Sandbox::new();
        let native = box_.dir("plugin/native");
        let ran = box_.root.join("ran");
        let uname_calls = box_.root.join("uname-calls");
        fs::copy(
            repo()
                .join("plugins")
                .join("aipet")
                .join("native")
                .join("aipet-hook.sh"),
            native.join("aipet-hook.sh"),
        )
        .unwrap();
        let plugin = Plugin {
            box_,
            native,
            ran,
            uname_calls,
        };
        for rid in [
            "linux-x64/aipet-hook",
            "linux-arm64/aipet-hook",
            "win-x64/aipet-hook.exe",
        ] {
            let name = rid.split('/').next().unwrap();
            plugin.hook(
                rid,
                &format!(
                    "#!/bin/sh\necho \"{name} $*\" >> '{}'\ncat >> '{}'\n",
                    plugin.ran.display(),
                    plugin.ran.display()
                ),
                true,
            );
        }
        plugin
    }

    /// A file at native/<rid>, executable unless said otherwise.
    fn hook(&self, rid: &str, content: &str, executable: bool) {
        write(&self.native.join(rid), content, if executable { "755" } else { "644" });
    }

    fn windows_hook(&self) -> PathBuf {
        self.native.join("win-x64").join("aipet-hook.exe")
    }

    fn uname(&self, answer: &str) {
        self.box_.stub(
            "uname",
            &format!("echo \"$*\" >> '{}'\necho '{answer}'", self.uname_calls.display()),
        );
    }

    /// Runs `sh <script> --agent claude` with the event on stdin and `OS` as given; the exit code, stdout and stderr.
    fn run(&self, os: Option<&str>, relative: bool) -> (Option<i32>, String, String) {
        let script = if relative {
            Path::new("aipet-hook.sh").to_path_buf()
        } else {
            self.native.join("aipet-hook.sh")
        };
        let cwd = if relative {
            self.native.clone()
        } else {
            self.box_.root.clone()
        };
        let mut command = self.box_.command(self.box_.tool("sh"), Some(&cwd));
        command.arg(script).args(["--agent", "claude"]).stdin(Stdio::piped());
        if let Some(os) = os {
            command.env("OS", os);
        }
        let mut child = command.spawn().unwrap();
        // the launcher may exit (for a platform it doesn't run on) without reading its stdin
        let _ = child.stdin.take().unwrap().write_all(EVENT.as_bytes());
        let r = finish(child, Duration::from_secs(10), "the launcher");
        (r.exit, r.stdout, r.stderr)
    }

    fn ran_with(&self) -> Option<String> {
        fs::read_to_string(&self.ran).ok()
    }

    fn unames(&self) -> usize {
        fs::read_to_string(&self.uname_calls).map_or(0, |t| t.lines().count())
    }
}

fn quiet() -> (Option<i32>, String, String) {
    (Some(0), String::new(), String::new())
}

#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn runs_only_the_hook_this_platform_can_load() {
    let x64_loader = Path::new("/lib64/ld-linux-x86-64.so.2").exists();
    let arm64_loader = Path::new("/lib/ld-linux-aarch64.so.1").exists();
    for (uname, rid) in [
        ("Linux x86_64", Some("linux-x64")),
        ("Linux amd64", Some("linux-x64")),
        ("Linux aarch64", Some("linux-arm64")),
        ("Linux arm64", Some("linux-arm64")),
        ("MINGW64_NT-10.0-19045 x86_64", Some("win-x64")),
        ("MSYS_NT-10.0-19045 x86_64", Some("win-x64")),
        ("CYGWIN_NT-10.0 x86_64", Some("win-x64")),
        ("Linux armv7l", None),
        ("Linux i686", None),
        ("Linux riscv64", None),
        ("Darwin arm64", None),
        ("FreeBSD amd64", None),
        ("", None),
    ] {
        // a Linux build runs only with its glibc loader there (not on musl, nor a 32-bit userland on a 64-bit kernel)
        let rid = rid.filter(|&r| !(r == "linux-x64" && !x64_loader || r == "linux-arm64" && !arm64_loader));
        let plugin = Plugin::new();
        plugin.uname(uname);
        assert_eq!(plugin.run(None, false), quiet(), "{uname:?}");
        assert_eq!(
            plugin.ran_with(),
            rid.map(|r| format!("{r} --agent claude\n{EVENT}")),
            "{uname:?}"
        );
        assert_eq!(plugin.unames(), 1, "{uname:?}");
    }
}

/// Windows sets OS for every program, so Git Bash needs no uname at all.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn on_windows_runs_the_windows_hook_without_uname() {
    let plugin = Plugin::new();
    plugin.uname("Linux x86_64");
    assert_eq!(plugin.run(Some("Windows_NT"), false), quiet());
    assert_eq!(plugin.ran_with().unwrap(), format!("win-x64 --agent claude\n{EVENT}"));
    assert_eq!(plugin.unames(), 0);
}

/// Run as `sh aipet-hook.sh`, from its own folder.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn finds_the_hook_from_a_relative_path() {
    let plugin = Plugin::new();
    assert_eq!(plugin.run(Some("Windows_NT"), true), quiet());
    assert_eq!(plugin.ran_with().unwrap(), format!("win-x64 --agent claude\n{EVENT}"));
}

/// The hook's stdout reaches the agent as it is (the real one prints nothing, but the launcher mustn't eat it).
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn passes_the_hooks_stdout_through() {
    let plugin = Plugin::new();
    plugin.hook("win-x64/aipet-hook.exe", "#!/bin/sh\necho out\n", true);
    assert_eq!(
        plugin.run(Some("Windows_NT"), false),
        (Some(0), "out\n".into(), String::new())
    );
}

/// A hook that isn't there or can't run (a glibc older than the build's, say) still ends in a quiet 0.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn a_broken_hook_ends_in_a_quiet_zero() {
    for (what, content, executable) in [
        ("missing", None, true),
        (
            "not executable, made so",
            Some("#!/bin/sh\necho made >> '{ran}'\n"),
            false,
        ),
        ("fails loudly", Some("#!/bin/sh\necho oops >&2\nexit 3\n"), true),
        ("missing loader", Some("#!/nonexistent/ld-linux.so.2\n"), true),
        ("not a program", Some("\u{7f}ELF\u{2}\u{1}garbage"), true),
        ("a folder", None, true),
    ] {
        let plugin = Plugin::new();
        let hook = plugin.windows_hook();
        fs::remove_file(&hook).unwrap();
        if what == "a folder" {
            fs::create_dir_all(&hook).unwrap();
        } else if let Some(content) = content {
            let content = content.replace("{ran}", &plugin.ran.display().to_string());
            plugin.hook("win-x64/aipet-hook.exe", &content, executable);
        }
        assert_eq!(plugin.run(Some("Windows_NT"), false), quiet(), "{what}");
        let expected = (what == "not executable, made so").then(|| "made\n".to_owned());
        assert_eq!(plugin.ran_with(), expected, "{what}");
    }
}
