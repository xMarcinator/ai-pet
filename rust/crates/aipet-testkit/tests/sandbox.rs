//! The sandbox itself: what runs in it sees nothing of the user's environment, finds only the allowed tools and the
//! test's stand-ins, and writes its home, config, data and temp files into the sandbox, which goes with the test.
//! Every black-box suite here relies on that, so it's asserted rather than assumed.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use aipet_testkit::{Sandbox, TOOLS, repo, workflow_step, workflow_step_text};

/// The variables the sandbox gives a program, and no others.
const GIVEN: &[&str] = &[
    "PATH",
    "HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "XDG_CACHE_HOME",
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
    "AIPET_DATA_DIR",
    "TMPDIR",
    "AIPET_PIPE",
    "AIPET_TEST_OUT",
    "LC_ALL",
    "GIT_CONFIG_NOSYSTEM",
];

/// What `env` printed, as names and values. The shells add PWD, SHLVL and `_` of their own.
fn variables(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .filter(|(k, _)| !["PWD", "OLDPWD", "SHLVL", "_"].contains(&k.as_str()))
        .collect()
}

#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn a_program_gets_only_the_sandboxes_variables() {
    let mut box_ = Sandbox::new();
    box_.env.insert("ONE_OF_THE_TESTS".into(), "1".into());
    let r = box_.sh(Path::new("-c"), &["env"]);
    r.ok();
    let seen = variables(&r.stdout);
    let names: BTreeSet<&str> = seen.iter().map(|(k, _)| k.as_str()).collect();
    let mut expected: BTreeSet<&str> = GIVEN.iter().copied().collect();
    expected.insert("ONE_OF_THE_TESTS");
    assert_eq!(names, expected, "{}", r.stdout);

    // every folder is the sandbox's, never the user's own
    let home = std::env::var_os("HOME").map(PathBuf::from);
    for (name, value) in &seen {
        if [
            "PATH",
            "AIPET_PIPE",
            "LC_ALL",
            "GIT_CONFIG_NOSYSTEM",
            "ONE_OF_THE_TESTS",
        ]
        .contains(&name.as_str())
        {
            continue;
        }
        let path = Path::new(value);
        assert!(path.starts_with(&box_.root), "{name}={value} isn't in the sandbox");
        assert!(path.is_dir(), "{name}={value} isn't there");
        if let Some(home) = &home {
            assert_ne!(path, home, "{name} is the user's home");
        }
    }
    for user in [
        "CLAUDE_CONFIG_DIR",
        "CODEX_HOME",
        "AIPET_DATA_DIR",
        "XDG_RUNTIME_DIR",
        "DISPLAY",
        "WAYLAND_DISPLAY",
    ] {
        if let Ok(theirs) = std::env::var(user) {
            assert!(
                seen.iter().all(|(_, v)| *v != theirs),
                "the user's {user} reached the sandbox"
            );
        }
    }
}

/// AIPET_PIPE is a path of its own, where nothing listens: a hook run in the sandbox never reaches the user's pet.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn the_endpoint_is_one_no_pet_listens_on() {
    let box_ = Sandbox::new();
    let pipe = box_.pipe().to_string_lossy().into_owned();
    let env = box_.environment();
    assert_eq!(env["AIPET_PIPE"], pipe);
    assert!(pipe.starts_with("/tmp/aipet-n-") && pipe.ends_with(".sock"), "{pipe}");
    assert!(!Path::new(&pipe).exists());
    assert_ne!(Sandbox::new().pipe(), box_.pipe());
    if let Ok(theirs) = std::env::var("AIPET_PIPE") {
        assert_ne!(pipe, theirs);
    }
}

/// PATH is the stand-ins, then links to the allowed tools: claude, codex and pkill aren't found unless a test stands
/// one in, and a stand-in comes before a tool of the same name.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn path_has_only_the_allowed_tools_and_the_stand_ins() {
    let box_ = Sandbox::new();
    assert_eq!(
        box_.environment()["PATH"],
        format!("{}:{}", box_.bin().display(), box_.tools().display())
    );
    let linked: BTreeSet<String> = fs::read_dir(box_.tools())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(linked.iter().all(|t| TOOLS.contains(&t.as_str())), "{linked:?}");
    for needed in ["sh", "bash", "git", "tar", "env", "cat"] {
        assert!(linked.contains(needed), "{needed} wasn't found on this system's PATH");
    }

    let found = |name: &str| {
        let r = box_.sh(Path::new("-c"), &["command -v \"$1\" || echo none", "sh", name]);
        r.ok().stdout.trim().to_owned()
    };
    for missing in ["claude", "codex", "pkill", "pgrep", "curl", "AiPet", "aipet-hook"] {
        assert_eq!(found(missing), "none", "{missing} is on the sandbox's PATH");
    }
    assert_eq!(found("cat"), box_.tools().join("cat").to_string_lossy());

    box_.stub("cat", "echo stand-in");
    box_.stub("claude", "echo \"$*\" >> \"$AIPET_TEST_OUT/claude-calls\"");
    assert_eq!(found("cat"), box_.bin().join("cat").to_string_lossy());
    box_.sh(Path::new("-c"), &["claude plugin install aipet@aipet"]).ok();
    assert_eq!(
        fs::read_to_string(box_.out().join("claude-calls")).unwrap(),
        "plugin install aipet@aipet\n"
    );
}

/// What a script writes under ~, the XDG folders, the agents' folders and the temp folder lands in the sandbox.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn a_scripts_files_land_in_the_sandbox_and_go_with_it() {
    let box_ = Sandbox::new();
    let root = box_.root.clone();
    let script = r#"
        mkdir -p ~/.claude "$XDG_CONFIG_HOME/autostart" "$XDG_DATA_HOME/AiPet" "$CODEX_HOME"
        echo home > ~/.claude/settings.json
        echo config > "$XDG_CONFIG_HOME/autostart/aipet.desktop"
        echo data > "$XDG_DATA_HOME/AiPet/x"
        echo claude > "$CLAUDE_CONFIG_DIR/settings.json"
        echo codex > "$CODEX_HOME/config.toml"
        echo aipet > "$AIPET_DATA_DIR/config.json"
        mktemp
    "#;
    let r = box_.sh(Path::new("-c"), &[script]);
    r.ok();
    for (file, text) in [
        ("home/.claude/settings.json", "home"),
        ("config/autostart/aipet.desktop", "config"),
        ("data/AiPet/x", "data"),
        ("claude/settings.json", "claude"),
        ("codex/config.toml", "codex"),
        ("aipet/config.json", "aipet"),
    ] {
        assert_eq!(
            fs::read_to_string(root.join(file)).unwrap(),
            format!("{text}\n"),
            "{file}"
        );
    }
    assert!(
        Path::new(r.stdout.trim()).starts_with(root.join("tmp")),
        "mktemp made {}",
        r.stdout
    );

    drop(box_);
    assert!(!root.exists(), "the sandbox outlived its test");
}

/// A step runs as the runner runs it: bash with -e and pipefail, so a failure anywhere in a pipeline fails it.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn a_step_fails_as_the_runner_fails_it() {
    let box_ = Sandbox::new();
    let cwd = box_.dir("job");
    let r = box_.step("pwd\nfalse | cat\necho after\n", &cwd);
    assert_eq!(r.exit, Some(1), "{}", r.output());
    assert_eq!(r.stdout, format!("{}\n", cwd.display()));
    box_.step("set -u\necho \"$HOME\"\n", &cwd).ok();
}

/// The git a test runs knows nothing of the user's: no global or system config, an identity of the test's.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn git_runs_with_nothing_of_the_users() {
    let box_ = Sandbox::new();
    let repo = box_.dir("repo");
    box_.git(&repo, &["init", "-q"]);
    fs::write(repo.join("a"), "a\n").unwrap();
    box_.git(&repo, &["add", "-A"]);
    box_.git(&repo, &["commit", "-q", "-m", "first"]);
    assert_eq!(
        box_.git(&repo, &["log", "-1", "--format=%an <%ae>"]),
        "t <t@example.com>"
    );
    assert_eq!(box_.git(&repo, &["branch", "--show-current"]), "main");
    let global = box_.run(box_.tool("git"), &["config", "--global", "--list"], Some(&repo));
    assert!(global.stdout.is_empty(), "{}", global.stdout);
}

/// A step's script is its `run: |` block, unindented; its text has its keys.
#[test]
fn a_workflows_step_is_read_as_the_runner_reads_it() {
    let script = workflow_step("release.yml", "Check the version");
    assert!(
        script.starts_with("if [ \"$GITHUB_REF\" != refs/heads/main ] && [ \"$DRY_RUN\" != true ]; then\n  echo "),
        "{script}"
    );
    assert!(
        script.ends_with("echo \"version=$VERSION\" >> \"$GITHUB_OUTPUT\"\n"),
        "{script}"
    );
    let text = workflow_step_text("release.yml", "Check the version");
    assert!(
        text.starts_with("      - name: Check the version\n        id: version\n"),
        "{text}"
    );
    assert!(text.contains("DRY_RUN: ${{ inputs.dry_run }}"), "{text}");
    assert!(repo().join("rust").join("crates").join("aipet-testkit").is_dir());
}
