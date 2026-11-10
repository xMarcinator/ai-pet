//! Where the tests that run programs run: the repository's shell scripts, release.yml's steps and the built binaries.
//! The port of the C# tests' `TestEnv` and `Scripts.Sandbox` (tests/AiPet.Tests/TestEnv.cs and Scripts.cs).
//!
//! A [`Sandbox`] is a folder of the test's own, removed with it. A program run in it gets fresh folders for HOME, the
//! XDG folders, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `AIPET_DATA_DIR` and `TMPDIR`, an `AIPET_PIPE` no pet listens on,
//! and a PATH of the test's own stand-ins ([`Sandbox::bin`]) before links to an allow-list of the system's tools
//! ([`TOOLS`]). Nothing else of the user's environment reaches it, so the real claude and codex are never found,
//! pkill never reaches the user's pet, and nothing is read from or written to the user's home, config or data. The
//! testkit's own tests (tests/sandbox.rs) hold it to that.
//!
//! Tests for one kind of platform are skipped elsewhere with `#[cfg_attr(..., ignore = "why")]`, so cargo reports
//! them as ignored rather than as passed (which a test that returns early is): `#[cfg_attr(windows, ignore = "runs
//! sh scripts")]` for the C#'s `[UnixFact]`, `#[cfg_attr(not(target_os = "linux"), ignore = "Linux only")]` for its
//! `[LinuxFact]`, `#[cfg_attr(not(windows), ignore = "Windows only")]` for its `[WindowsFact]`.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// The tools a script may run, linked into every sandbox's PATH (`Scripts.ToolNames`).
pub const TOOLS: &[&str] = &[
    "sh",
    "bash",
    "cat",
    "cp",
    "mv",
    "rm",
    "mkdir",
    "chmod",
    "ln",
    "sed",
    "awk",
    "grep",
    "tr",
    "head",
    "tail",
    "od",
    "uname",
    "id",
    "dirname",
    "basename",
    "mktemp",
    "tar",
    "gzip",
    "sha256sum",
    "env",
    "sleep",
    "setsid",
    "nohup",
    "base64",
    "sort",
    "ls",
    "touch",
    "find",
    "jq",
    "git",
    "printf",
    "wc",
    "cut",
    "readlink",
    "diff",
    "cmp",
    "tee",
    "true",
    "false",
];

/// How long a program run in a sandbox may take.
pub const RUN_LIMIT: Duration = Duration::from_secs(60);

/// The repository's root: rust/crates/aipet-testkit is three folders down.
pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("aipet-testkit is at rust/crates/aipet-testkit")
        .to_path_buf()
}

/// A name no other sandbox in any test process uses: the process, a counter, and `what`.
pub fn unique(what: &str) -> String {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let n = COUNT.fetch_add(1, Ordering::SeqCst);
    format!("aipet-{what}-{:x}-{n}", std::process::id())
}

/// An endpoint nothing listens on (`TestEnv.NoPet`): a socket path under /tmp (a socket's path must stay under 104
/// bytes, and TMPDIR can be long), or a pipe name nobody made.
pub fn no_pet() -> OsString {
    if cfg!(windows) {
        format!("AiPet-test-none-{}", unique("pipe")).into()
    } else {
        format!("/tmp/{}.sock", unique("n")).into()
    }
}

/// A binary of the workspace's, built before the tests (`cargo build --workspace`: `cargo test` builds a package's
/// binaries only for that package's own tests): `name` in the target folder above this test's own executable.
pub fn built(name: &str) -> PathBuf {
    // <target>/<profile>/deps/<test>-<hash>: the binaries are a folder up
    let exe = std::env::current_exe().unwrap();
    let path = exe
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "{name} isn't built at {}: run cargo build --workspace first",
        path.display()
    );
    path
}

/// The hook the black-box suites run: the one `AIPET_TEST_HOOK` names (`TestEnv.TestHook`), else the Rust hook built
/// with the tests, which must not be older than its sources.
pub fn hook() -> PathBuf {
    if let Some(asked) = std::env::var_os("AIPET_TEST_HOOK").filter(|h| !h.is_empty()) {
        let path = std::path::absolute(&asked).unwrap();
        assert!(path.is_file(), "AIPET_TEST_HOOK: there's no hook at {}", path.display());
        return path;
    }
    let hook = built("aipet-hook");
    assert_fresh(&hook, &["aipet-hook/src", "aipet-ipc/src"]);
    hook
}

/// A binary built before its sources last changed would test code that isn't there any more.
fn assert_fresh(binary: &Path, sources: &[&str]) {
    let built = fs::metadata(binary).unwrap().modified().unwrap();
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for dir in sources {
        for source in fs::read_dir(crates.join(dir)).unwrap() {
            let source = source.unwrap().path();
            let changed = fs::metadata(&source).unwrap().modified().unwrap();
            assert!(
                changed <= built,
                "{} changed after {} was built: cargo build --workspace",
                source.display(),
                binary.display()
            );
        }
    }
}

// ------------------------------------------------------------------ running programs
/// What a program did.
#[derive(Debug)]
pub struct Run {
    /// None when a signal ended it.
    pub exit: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    /// Both, as one text for a test's message.
    pub fn output(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    /// Exit 0, or the test fails with what the program printed.
    #[track_caller]
    pub fn ok(&self) -> &Run {
        assert_eq!(self.exit, Some(0), "{}", self.output());
        self
    }
}

/// Waits for `child`, which must exit within `limit`, reading its stdout and stderr meanwhile (a full pipe would
/// hold it up); kills it and fails the test otherwise.
pub fn finish(mut child: Child, limit: Duration, what: &str) -> Run {
    fn drain(pipe: Option<impl Read + Send + 'static>) -> thread::JoinHandle<String> {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            String::from_utf8_lossy(&bytes).into_owned()
        })
    }
    let (stdout, stderr) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let until = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{what} didn't exit within {limit:?}");
        }
        thread::sleep(Duration::from_millis(5));
    };
    Run {
        exit: status.code(),
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    }
}

/// Writes an executable file (a stand-in, a fake hook): only its user may run it.
pub fn executable(path: &Path, text: &str) {
    write(path, text, "700");
}

/// Writes a file with `mode`, by a shell of its own: a program this process wrote itself can't be run while any child
/// it is starting meanwhile (the other tests start hooks and scripts) still holds the file open (ETXTBSY, "Text file
/// busy"). Elsewhere, only the text.
pub fn write(path: &Path, text: &str, mode: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    if cfg!(windows) {
        fs::write(path, text).unwrap();
        return;
    }
    let mut sh = Command::new("sh")
        .args(["-c", "cat > \"$1\" && chmod \"$2\" \"$1\"", "sh"])
        .arg(path)
        .arg(mode)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    sh.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "couldn't write {}", path.display());
}

// ------------------------------------------------------------------ the sandbox
/// One test's sandbox (`Scripts.Sandbox`), removed when dropped.
pub struct Sandbox {
    /// Every folder of the sandbox is in here.
    pub root: PathBuf,
    /// Variables the test adds to (or changes in) [`Sandbox::environment`].
    pub env: BTreeMap<String, String>,
    pipe: OsString,
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Sandbox {
    pub fn new() -> Sandbox {
        let root = std::env::temp_dir().join(unique("sandbox"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let sandbox = Sandbox {
            root,
            env: BTreeMap::new(),
            pipe: no_pet(),
        };
        sandbox.link_tools();
        sandbox
    }

    /// The folder `name` in the sandbox, made if it isn't there.
    pub fn dir(&self, name: &str) -> PathBuf {
        let dir = self.root.join(name);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub fn home(&self) -> PathBuf {
        self.dir("home")
    }

    /// XDG_CONFIG_HOME.
    pub fn config(&self) -> PathBuf {
        self.dir("config")
    }

    /// XDG_DATA_HOME.
    pub fn data(&self) -> PathBuf {
        self.dir("data")
    }

    /// CLAUDE_CONFIG_DIR.
    pub fn claude(&self) -> PathBuf {
        self.dir("claude")
    }

    /// CODEX_HOME.
    pub fn codex(&self) -> PathBuf {
        self.dir("codex")
    }

    /// AIPET_DATA_DIR.
    pub fn aipet(&self) -> PathBuf {
        self.dir("aipet")
    }

    /// TMPDIR.
    pub fn tmp(&self) -> PathBuf {
        self.dir("tmp")
    }

    /// Where the stand-ins write what they saw (AIPET_TEST_OUT).
    pub fn out(&self) -> PathBuf {
        self.dir("out")
    }

    /// The stand-ins, first on PATH.
    pub fn bin(&self) -> PathBuf {
        self.dir("bin")
    }

    /// Links to the system's copies of [`TOOLS`], and nothing else; second on PATH.
    pub fn tools(&self) -> PathBuf {
        self.dir("tools")
    }

    /// The endpoint the sandbox's programs get as AIPET_PIPE, where no pet listens.
    pub fn pipe(&self) -> &OsStr {
        &self.pipe
    }

    /// A tool of the allow-list, as the sandbox's PATH has it.
    pub fn tool(&self, name: &str) -> PathBuf {
        assert!(TOOLS.contains(&name), "{name} isn't one of the sandbox's tools");
        let tool = self.tools().join(name);
        assert!(tool.exists(), "{name} isn't on this system's PATH");
        tool
    }

    fn link_tools(&self) {
        let dir = self.tools();
        #[cfg(unix)]
        {
            let path = std::env::var_os("PATH").unwrap_or_default();
            let path: Vec<PathBuf> = std::env::split_paths(&path).filter(|p| p.is_absolute()).collect();
            for name in TOOLS {
                if let Some(exe) = path.iter().map(|p| p.join(name)).find(|p| p.is_file()) {
                    std::os::unix::fs::symlink(exe, dir.join(name)).unwrap();
                }
            }
        }
        #[cfg(not(unix))]
        let _ = dir;
    }

    /// A stand-in command on PATH (a sh script).
    pub fn stub(&self, name: &str, body: &str) {
        executable(&self.bin().join(name), &format!("#!/bin/sh\n{body}\n"));
    }

    /// The environment a program runs with: nothing of the user's, apart from what the test adds.
    pub fn environment(&self) -> BTreeMap<String, String> {
        let text = |p: PathBuf| p.to_string_lossy().into_owned();
        let path = std::env::join_paths([self.bin(), self.tools()]).unwrap();
        let mut env: BTreeMap<String, String> = [
            ("PATH", path.to_string_lossy().into_owned()),
            ("HOME", text(self.home())),
            ("XDG_CONFIG_HOME", text(self.config())),
            ("XDG_DATA_HOME", text(self.data())),
            ("XDG_STATE_HOME", text(self.dir("state"))),
            ("XDG_CACHE_HOME", text(self.dir("cache"))),
            ("CLAUDE_CONFIG_DIR", text(self.claude())),
            ("CODEX_HOME", text(self.codex())),
            ("AIPET_DATA_DIR", text(self.aipet())),
            ("TMPDIR", text(self.tmp())),
            ("AIPET_PIPE", self.pipe.to_string_lossy().into_owned()),
            ("AIPET_TEST_OUT", text(self.out())),
            ("LC_ALL", "C".into()),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        if cfg!(windows) {
            // where Windows programs look for the same folders
            for (name, dir) in [("USERPROFILE", self.home()), ("TMP", self.tmp()), ("TEMP", self.tmp())] {
                env.insert(name.into(), text(dir));
            }
            // the system's own folders, which a program can't start without (nothing of the user's)
            for name in ["SystemRoot", "SystemDrive", "windir", "ComSpec", "PATHEXT"] {
                if let Ok(value) = std::env::var(name) {
                    env.insert(name.into(), value);
                }
            }
        }
        env.extend(self.env.clone());
        env
    }

    /// `program` with the sandbox's environment and no other, in `cwd` (else the sandbox's root), stdin closed.
    pub fn command(&self, program: impl AsRef<OsStr>, cwd: Option<&Path>) -> Command {
        let mut command = Command::new(program);
        command
            .env_clear()
            .envs(self.environment())
            .current_dir(cwd.unwrap_or(&self.root))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    /// Runs a program (by path) in the sandbox, which must exit within [`RUN_LIMIT`].
    pub fn run<S: AsRef<OsStr>>(&self, program: impl AsRef<OsStr>, args: &[S], cwd: Option<&Path>) -> Run {
        let program = program.as_ref();
        let mut command = self.command(program, cwd);
        command.args(args);
        let child = command
            .spawn()
            .unwrap_or_else(|e| panic!("can't start {}: {e}", program.to_string_lossy()));
        finish(child, RUN_LIMIT, &program.to_string_lossy())
    }

    /// `sh <script> <args>`.
    pub fn sh(&self, script: &Path, args: &[&str]) -> Run {
        let mut all: Vec<&OsStr> = vec![script.as_os_str()];
        all.extend(args.iter().map(OsStr::new));
        self.run(self.tool("sh"), &all, None)
    }

    /// Runs a script as GitHub Actions runs a `run:` step with shell bash.
    pub fn step(&self, script: &str, cwd: &Path) -> Run {
        let file = self.root.join(format!("{}.sh", unique("step")));
        fs::write(&file, script).unwrap();
        let args: Vec<&OsStr> = vec![
            "--noprofile".as_ref(),
            "--norc".as_ref(),
            "-eo".as_ref(),
            "pipefail".as_ref(),
            file.as_os_str(),
        ];
        self.run(self.tool("bash"), &args, Some(cwd))
    }

    /// git in a folder, which must succeed; what it printed, trimmed.
    pub fn git(&self, cwd: &Path, args: &[&str]) -> String {
        let mut all = vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "init.defaultBranch=main",
        ];
        all.extend(args);
        let r = self.run(self.tool("git"), &all, Some(cwd));
        assert_eq!(r.exit, Some(0), "git {}: {}", args.join(" "), r.output());
        r.stdout.trim().to_owned()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        if cfg!(unix) {
            let _ = fs::remove_file(&self.pipe);
        }
    }
}

// ------------------------------------------------------------------ workflows
fn workflow_lines(workflow: &str) -> Vec<String> {
    let path = repo().join(".github").join("workflows").join(workflow);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines().map(str::to_owned).collect()
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The lines of the step with this name, from its `- name:` to the next step.
fn step_lines(workflow: &str, step: &str) -> Vec<String> {
    let lines = workflow_lines(workflow);
    let at = lines
        .iter()
        .position(|l| l.trim() == format!("- name: {step}"))
        .unwrap_or_else(|| panic!("{workflow} has no step named '{step}'"));
    let step_indent = lines[at].find('-').unwrap();
    let mut text = vec![lines[at].clone()];
    for line in &lines[at + 1..] {
        if !line.trim().is_empty() && indent(line) <= step_indent {
            break;
        }
        text.push(line.clone());
    }
    text
}

/// The `run: |` script of the step with this name in a workflow, as the runner gets it (`Scripts.WorkflowStep`).
pub fn workflow_step(workflow: &str, step: &str) -> String {
    let lines = step_lines(workflow, step);
    let run = lines
        .iter()
        .position(|l| l.trim() == "run: |")
        .unwrap_or_else(|| panic!("step '{step}' in {workflow} has no run: | block"));
    let run_indent = indent(&lines[run]);
    let body: Vec<&String> = lines[run + 1..]
        .iter()
        .take_while(|l| l.trim().is_empty() || indent(l) > run_indent)
        .collect();
    let least = body
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| indent(l))
        .min()
        .unwrap_or(0);
    let mut script: String = body
        .iter()
        .map(|l| if l.len() >= least { &l[least..] } else { "" })
        .collect::<Vec<_>>()
        .join("\n");
    script.push('\n');
    script
}

/// The lines of the step with this name, for checking its keys (`if:`, `env:`) (`Scripts.WorkflowStepText`).
pub fn workflow_step_text(workflow: &str, step: &str) -> String {
    step_lines(workflow, step).join("\n")
}
