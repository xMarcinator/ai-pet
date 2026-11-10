//! release.yml's scripts, rehosted from tests/AiPet.Tests/ReleaseWorkflowTests.cs: run as the runner runs them (bash
//! -eo pipefail), against folders and git repositories the tests make. gh and ssh are stand-ins; nothing reaches
//! GitHub.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use aipet_testkit::{Sandbox, repo, workflow_step, workflow_step_text};

const WORKFLOW: &str = "release.yml";
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn release_yml() -> String {
    fs::read_to_string(repo().join(".github").join("workflows").join(WORKFLOW)).unwrap()
}

fn step(name: &str) -> String {
    workflow_step(WORKFLOW, name)
}

fn step_text(name: &str) -> String {
    workflow_step_text(WORKFLOW, name)
}

/// What a step wrote to GITHUB_OUTPUT.
fn outputs(file: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn set(box_: &mut Sandbox, vars: &[(&str, &str)]) {
    for (k, v) in vars {
        box_.env.insert((*k).into(), (*v).into());
    }
}

fn put(file: &Path, text: &str) {
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, text).unwrap();
}

/// Only a stable release that isn't older than the one main pins moves main's marketplaces (every user's plugin) and
/// becomes GitHub's latest release (the one-line installers' app).
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn latest_check_by_version_and_pin() {
    for (version, pinned_ref, pinned_sha, latest) in [
        ("0.3.0", "v0.2.0", SHA, true),
        ("0.3.0-rc.1", "v0.2.0", SHA, false),
        // an older line: main stays on the newer one
        ("0.1.9", "v0.2.0", SHA, false),
        // by version, not by text
        ("0.10.0", "v0.9.0", SHA, true),
        // a re-run
        ("0.2.0", "v0.2.0", SHA, true),
        // an older workflow pinned a prerelease
        ("0.3.0", "v0.3.0-rc.1", SHA, true),
        // the placeholder: nothing released yet
        ("0.1.0", "v0.2.0", "0000000000000000000000000000000000000000", true),
    ] {
        // The step reads only the first plugin's source ref and sha. Not the repository's own files: every release
        // pins them anew.
        let mut box_ = Sandbox::new();
        let cwd = box_.dir("repo");
        for f in [".claude-plugin/marketplace.json", ".agents/plugins/marketplace.json"] {
            put(
                &cwd.join(f),
                &format!(
                    r#"{{ "plugins": [ {{ "name": "aipet", "source": {{ "source": "git-subdir", "ref": "{pinned_ref}", "sha": "{pinned_sha}" }} }} ] }}"#
                ),
            );
        }
        let output = box_.root.join("output");
        let summary = box_.root.join("summary");
        set(
            &mut box_,
            &[
                ("VERSION", version),
                ("GITHUB_OUTPUT", &output.display().to_string()),
                ("GITHUB_STEP_SUMMARY", &summary.display().to_string()),
            ],
        );
        box_.step(&step("Check whether this release is the latest"), &cwd).ok();
        let expected = if latest { "true" } else { "false" };
        assert_eq!(
            outputs(&output).get("latest").map(String::as_str),
            Some(expected),
            "{version} {pinned_ref}"
        );
    }
}

#[test]
fn pin_runs_only_for_the_latest_release() {
    let pin = step_text("Pin the marketplaces to the plugin commit");
    assert!(pin.contains("if: steps.latest.outputs.latest == 'true'"), "{pin}");
    assert!(step("Create the GitHub release").contains("--latest=\"$LATEST\""));
}

/// A release comes from main; a dry run may come from any branch, since it publishes nothing.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn check_releases_from_main_dry_runs_from_any_branch() {
    for (git_ref, dry_run, error) in [
        ("refs/heads/main", "false", None),
        ("refs/heads/fn-1-rust-hook", "false", Some("Release from main")),
        ("refs/heads/fn-1-rust-hook", "true", None),
    ] {
        let mut box_ = Sandbox::new();
        // the tag isn't there
        box_.stub("gh", "exit 1");
        let output = box_.root.join("output");
        set(
            &mut box_,
            &[
                ("VERSION", "0.2.0-dry.1"),
                ("DRY_RUN", dry_run),
                ("GITHUB_REF", git_ref),
                ("GITHUB_REPOSITORY", "xMarcinator/ai-pet"),
                ("GITHUB_OUTPUT", &output.display().to_string()),
            ],
        );
        assert!(step_text("Check the version").contains("DRY_RUN: ${{ inputs.dry_run }}"));
        let r = box_.step(&step("Check the version"), &box_.root);
        match error {
            None => {
                r.ok();
                assert_eq!(outputs(&output)["version"], "0.2.0-dry.1");
            }
            Some(error) => {
                assert_eq!(r.exit, Some(1), "{}", r.output());
                assert!(r.output().contains(&format!("::error::{error}")), "{}", r.output());
            }
        }
    }
}

/// A dry run commits and tags the plugin, so the jobs after it have its commit, but pushes nothing; and it creates no
/// GitHub release and pins no marketplace.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn dry_run_pushes_and_publishes_nothing() {
    for dry_run in ["true", "false"] {
        let mut box_ = Sandbox::new();
        let origin = box_.dir("server").join("plugin.git");
        let origin_text = origin.display().to_string();
        box_.git(&box_.root, &["init", "-q", "--bare", &origin_text]);
        let first = box_.dir("first");
        box_.git(&first, &["init", "-q"]);
        fs::write(first.join("README.md"), "plugin\n").unwrap();
        box_.git(&first, &["add", "-A"]);
        box_.git(&first, &["commit", "-q", "-m", "first"]);
        box_.git(&first, &["push", "-q", &origin_text, "HEAD:refs/heads/main"]);
        let plugin = box_.root.join("plugin-repo");
        box_.git(
            &box_.root,
            &["clone", "-q", &origin_text, &plugin.display().to_string()],
        );
        fs::write(plugin.join("hook"), "the release's hook\n").unwrap();
        let output = box_.root.join("output");
        let summary = box_.root.join("summary");
        set(
            &mut box_,
            &[
                ("VERSION", "0.2.0-dry.1"),
                ("DRY_RUN", dry_run),
                ("PLUGIN_REPO", "xMarcinator/ai-pet-plugin"),
                ("GITHUB_REPOSITORY", "xMarcinator/ai-pet"),
                ("GITHUB_SHA", SHA),
                ("GITHUB_SERVER_URL", "https://github.com"),
                ("GITHUB_RUN_ID", "1"),
                ("GITHUB_OUTPUT", &output.display().to_string()),
                ("GITHUB_STEP_SUMMARY", &summary.display().to_string()),
            ],
        );

        assert!(step_text("Commit, tag and push").contains("DRY_RUN: ${{ inputs.dry_run }}"));
        box_.step(&step("Commit, tag and push"), &plugin).ok();
        let sha = box_.git(&plugin, &["rev-parse", "HEAD"]);
        assert_eq!(outputs(&output)["sha"], sha);
        assert_eq!(box_.git(&plugin, &["rev-parse", "v0.2.0-dry.1^{commit}"]), sha);
        let pushed = box_.git(&origin, &["for-each-ref", "--format=%(refname)"]);
        let dry = dry_run == "true";
        assert_eq!(
            pushed,
            if dry {
                "refs/heads/main"
            } else {
                "refs/heads/main\nrefs/tags/v0.2.0-dry.1"
            },
            "dry run: {dry_run}"
        );
        assert_eq!(
            box_.git(&origin, &["log", "-1", "--format=%s", "main"]),
            if dry { "first" } else { "AiPet plugin v0.2.0-dry.1" }
        );

        assert!(step_text("Create the GitHub release").contains("if: ${{ !inputs.dry_run }}"));
        assert!(
            step_text("Pin the marketplaces to the plugin commit")
                .contains("if: steps.latest.outputs.latest == 'true' && !inputs.dry_run")
        );
    }
}

/// The Linux binaries' gate: the newest GLIBC_ symbol version the pet or the hook needs is the oldest glibc it runs
/// on, and it may not be newer than 2.27. readelf is a stand-in that lists the versions given, for both.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn glibc_gate_refuses_a_binary_that_needs_a_newer_glibc() {
    assert!(release_yml().contains("GLIBC_FLOOR: \"2.27\""));
    for (versions, error) in [
        ("2.2.5 2.3 2.17 2.27", None),
        ("2.2.5 2.17 2.28", Some("AiPet needs GLIBC_2.28, newer than 2.27")),
        ("2.2.5 2.34", Some("aipet-hook needs GLIBC_2.34, newer than 2.27")),
        ("", Some("readelf found no GLIBC_ versions")),
    ] {
        let mut box_ = Sandbox::new();
        box_.stub(
            "readelf",
            r#"case "$2" in
  -V) for v in $AIPET_TEST_GLIBC; do echo "  0x0010:   Name: GLIBC_$v  Flags: none  Version: 2"; done ;;
  --dyn-syms) for v in $AIPET_TEST_GLIBC; do echo "     1: 0000000000000000     0 FUNC    GLOBAL DEFAULT  UND f@GLIBC_$v (2)"; done ;;
esac"#,
        );
        let summary = box_.root.join("summary");
        set(
            &mut box_,
            &[
                ("AIPET_TEST_GLIBC", versions),
                ("GLIBC_FLOOR", "2.27"),
                ("RID", "linux-x64"),
                ("GITHUB_STEP_SUMMARY", &summary.display().to_string()),
            ],
        );
        let r = box_.step(&step("Check the glibc the binaries need"), &box_.root);
        match error {
            None => {
                r.ok();
                assert_eq!(
                    fs::read_to_string(&summary).unwrap(),
                    "linux-x64 AiPet: needs glibc 2.27 (floor 2.27)\nlinux-x64 aipet-hook: needs glibc 2.27 (floor 2.27)\n"
                );
            }
            Some(error) => {
                assert_eq!(r.exit, Some(1), "{versions:?}: {}", r.output());
                assert!(r.output().contains(&format!("::error::{error}")), "{}", r.output());
            }
        }
    }
}

/// The Linux tarball keeps the layout the releases before had, whatever builds the app: the installer and the menu
/// entry's template at the top, the pet and the hook in app/, the icons and the Hyprland rules.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn tarball_keeps_the_layout() {
    let mut box_ = Sandbox::new();
    let cwd = box_.dir("job");
    for (f, text) in [
        ("out/app/AiPet", "pet\n"),
        ("out/hook/aipet-hook", "hook\n"),
        ("LICENSE", "MIT\n"),
    ] {
        put(&cwd.join(f), text);
    }
    let packaging = repo().join("packaging").display().to_string();
    box_.run(
        box_.tool("cp"),
        &["-R", &packaging, &cwd.join("packaging").display().to_string()],
        None,
    )
    .ok();
    set(&mut box_, &[("VERSION", "0.3.0"), ("RID", "linux-x64")]);

    box_.step(&step("Make the tarball"), &cwd).ok();
    let tarball = cwd
        .join("dist")
        .join("AiPet-0.3.0-linux-x64.tar.gz")
        .display()
        .to_string();
    let list = box_.run(box_.tool("tar"), &["-tzf", &tarball], None);
    let files: Vec<&str> = list
        .ok()
        .stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.ends_with('/'))
        .collect();
    for f in [
        "app/AiPet",
        "app/aipet-hook",
        "install.sh",
        "aipet.desktop.in",
        "hyprland/aipet.lua",
        "hyprland/aipet.conf",
        "icons/hicolor/256x256/apps/aipet.png",
        "LICENSE",
    ] {
        let path = format!("AiPet-0.3.0-linux-x64/{f}");
        assert!(files.contains(&path.as_str()), "{path} isn't in {files:?}");
    }
    assert!(
        !files
            .iter()
            .any(|f| f.contains("/app/") && !f.ends_with("/app/AiPet") && !f.ends_with("/app/aipet-hook")),
        "{files:?}"
    );
    let modes = box_.run(box_.tool("tar"), &["-tvzf", &tarball], None);
    for f in ["app/AiPet", "app/aipet-hook", "install.sh"] {
        let name = format!("AiPet-0.3.0-linux-x64/{f}");
        assert!(
            modes
                .stdout
                .lines()
                .any(|l| l.starts_with("-rwxr-xr-x") && l.ends_with(&name)),
            "{name} isn't 755: {}",
            modes.stdout
        );
    }
}

/// The release builds the Rust pet: no dotnet publish of the app is left, the Windows package carries the hook in
/// current\ (task 14's proof), and Velopack packs AiPet.exe under the packId the .NET installs update from.
#[test]
fn release_builds_the_rust_pet() {
    let text = release_yml();
    assert!(!text.contains("dotnet publish"));
    assert!(text.contains("cargo build -p aipet -p aipet-hook --release --locked --target x86_64-pc-windows-msvc"));
    assert!(
        text.contains("cargo zigbuild -p aipet -p aipet-hook --release --locked --target \"$TARGET.$GLIBC_FLOOR\"")
    );
    assert!(step("Put the hook next to AiPet.exe").contains("cp out/hook/aipet-hook.exe out/app/"));
    let pack = step("Build the installer and update feed (Velopack)");
    assert!(pack.contains("--packId AiPetApp"), "{pack}");
    assert!(pack.contains("--mainExe AiPet.exe"), "{pack}");
}

/// A repository whose codex.json is A at v0.2.0, B at v0.3.0-rc.1, and B at HEAD (one commit later).
fn hooks_changed_in_a_prerelease(box_: &Sandbox) -> PathBuf {
    let repo = box_.dir("repo");
    box_.git(&repo, &["init", "-q"]);
    let codex = repo.join("plugins").join("aipet").join("hooks").join("codex.json");
    put(&codex, "{\"a\":1}\n");
    box_.git(&repo, &["add", "-A"]);
    box_.git(&repo, &["commit", "-q", "-m", "0.2.0"]);
    box_.git(&repo, &["tag", "v0.2.0"]);
    fs::write(&codex, "{\"b\":2}\n").unwrap();
    box_.git(&repo, &["commit", "-q", "-am", "hooks change"]);
    box_.git(&repo, &["tag", "v0.3.0-rc.1"]);
    fs::write(repo.join("README.md"), "more\n").unwrap();
    box_.git(&repo, &["add", "-A"]);
    box_.git(&repo, &["commit", "-q", "-m", "more"]);
    repo
}

/// "Create the GitHub release" with gh a stand-in: the notes it was given, and how it was run.
fn release(box_: &mut Sandbox, repo: &Path, version: &str, latest: &str) -> (String, String) {
    put(&repo.join("dist").join("SHA256SUMS"), "x\n");
    box_.stub(
        "gh",
        r#"case "$1 $2" in
  "release view") exit 1 ;;
  "release create")
    echo "$*" > "$AIPET_TEST_OUT/gh-create"
    while [ $# -gt 0 ]; do
      if [ "$1" = --notes-file ]; then cp "$2" "$AIPET_TEST_OUT/notes.md"; fi
      shift
    done ;;
  *) echo "unexpected: gh $*" >&2; exit 1 ;;
esac"#,
    );
    let head = box_.git(repo, &["rev-parse", "HEAD"]);
    set(
        box_,
        &[
            ("VERSION", version),
            ("GITHUB_SHA", &head),
            ("GITHUB_REPOSITORY", "xMarcinator/ai-pet"),
            ("PLUGIN_REPO", "xMarcinator/ai-pet-plugin"),
            ("PLUGIN_PATH", "plugins/aipet"),
            ("PLUGIN_SHA", SHA),
            ("LATEST", latest),
        ],
    );
    box_.step(&step("Create the GitHub release"), repo).ok();
    (
        fs::read_to_string(box_.out().join("notes.md")).unwrap(),
        fs::read_to_string(box_.out().join("gh-create")).unwrap(),
    )
}

/// Stable users update from the latest stable release (prereleases don't move the marketplaces), so a stable release
/// after a prerelease that changed the Codex hooks still warns that Codex asks to trust them again.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn notes_stable_after_a_prerelease_compares_with_the_latest_stable_release() {
    let mut box_ = Sandbox::new();
    let repo = hooks_changed_in_a_prerelease(&box_);
    let (notes, create) = release(&mut box_, &repo, "0.3.0", "true");
    assert!(notes.contains("**Codex users:**"), "{notes}");
    assert!(create.contains("--latest=true"), "{create}");
    assert!(!create.contains("--prerelease"), "{create}");
}

#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn notes_prerelease_compares_with_the_latest_stable_release() {
    let mut box_ = Sandbox::new();
    let repo = hooks_changed_in_a_prerelease(&box_);
    let (notes, create) = release(&mut box_, &repo, "0.3.0-rc.2", "false");
    assert!(notes.contains("**Codex users:**"), "{notes}");
    assert!(create.contains("--prerelease"), "{create}");
    assert!(create.contains("--latest=false"), "{create}");
}

#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn notes_unchanged_hooks_no_warning() {
    let mut box_ = Sandbox::new();
    let repo = box_.dir("repo");
    box_.git(&repo, &["init", "-q"]);
    put(
        &repo.join("plugins").join("aipet").join("hooks").join("codex.json"),
        "{\"a\":1}\n",
    );
    box_.git(&repo, &["add", "-A"]);
    box_.git(&repo, &["commit", "-q", "-m", "0.2.0"]);
    box_.git(&repo, &["tag", "v0.2.0"]);
    fs::write(repo.join("README.md"), "more\n").unwrap();
    box_.git(&repo, &["add", "-A"]);
    box_.git(&repo, &["commit", "-q", "-m", "more"]);

    let (notes, _) = release(&mut box_, &repo, "0.2.1", "true");
    assert!(!notes.contains("Codex users"), "{notes}");
}

/// actions/checkout can't check out a repository without commits, which the plugin job would find only after the
/// builds: the check job fails at once. It reads the repository over ssh with the deploy key, as the plugin job does,
/// so a private repository is checked too, and a missing or wrong key fails as early.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn check_plugin_repository_without_a_branch_fails_early() {
    for (repo, error) in [
        ("empty", Some("has no branch")),
        ("full", None),
        ("missing", Some("Couldn't read")),
        ("nokey", Some("PLUGIN_DEPLOY_KEY isn't set")),
    ] {
        let mut box_ = Sandbox::new();
        let server = box_.dir("server");
        box_.git(&server, &["init", "-q", "--bare", "owner/empty.git"]);
        box_.git(&server, &["init", "-q", "--bare", "owner/full.git"]);
        let work = box_.dir("work");
        box_.git(&work, &["init", "-q"]);
        fs::write(work.join("README.md"), "plugin\n").unwrap();
        box_.git(&work, &["add", "-A"]);
        box_.git(&work, &["commit", "-q", "-m", "first"]);
        let full = server.join("owner").join("full.git").display().to_string();
        box_.git(&work, &["push", "-q", &full, "HEAD:refs/heads/main"]);
        // github.com's ssh is this folder: the stand-in writes down how it was run and runs the remote command here
        box_.stub(
            "ssh",
            &format!(
                r#"echo "$*" > "$AIPET_TEST_OUT/ssh-args"
key=
while [ $# -gt 1 ]; do
  if [ "$1" = -i ]; then key=$2; fi
  shift
done
ls -l "$key" | cut -c1-10 > "$AIPET_TEST_OUT/ssh-key-mode"
cp "$key" "$AIPET_TEST_OUT/ssh-key"
cd '{}' && eval "git ${{1#git-}}""#,
                server.display()
            ),
        );
        let temp = box_.dir("runner-temp");
        let key = if repo == "nokey" {
            ""
        } else {
            "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----"
        };
        let plugin_repo = format!("owner/{}", if repo == "nokey" { "full" } else { repo });
        set(
            &mut box_,
            &[
                ("RUNNER_TEMP", &temp.display().to_string()),
                ("PLUGIN_REPO", &plugin_repo),
                ("DEPLOY_KEY", key),
                ("GITHUB_HOST_KEY", "ssh-ed25519 AAAA"),
            ],
        );

        let name = "Check the plugin repository has a branch";
        assert!(step_text(name).contains("DEPLOY_KEY: ${{ secrets.PLUGIN_DEPLOY_KEY }}"));
        let r = box_.step(&step(name), &box_.root);
        assert_eq!(
            r.exit,
            Some(if error.is_none() { 0 } else { 1 }),
            "{repo}: {}",
            r.output()
        );
        match error {
            None => assert!(!r.output().contains("::"), "{}", r.output()),
            Some(error) => {
                assert!(r.output().contains("::error::"), "{}", r.output());
                assert!(r.output().contains(error), "{}", r.output());
            }
        }
        assert!(!temp.join("plugin-deploy-key").exists(), "the key stays on disk");
        if repo == "nokey" {
            assert!(!box_.out().join("ssh-args").exists());
            continue;
        }
        let args = fs::read_to_string(box_.out().join("ssh-args")).unwrap();
        for part in ["git@github.com", "IdentitiesOnly=yes", "StrictHostKeyChecking=yes"] {
            assert!(args.contains(part), "{args}");
        }
        assert_eq!(
            fs::read_to_string(box_.out().join("ssh-key-mode")).unwrap().trim(),
            "-rw-------"
        );
        assert_eq!(
            fs::read_to_string(box_.out().join("ssh-key")).unwrap(),
            format!("{key}\n")
        );
        assert_eq!(
            fs::read_to_string(temp.join("plugin-known-hosts")).unwrap(),
            "github.com ssh-ed25519 AAAA\n"
        );
    }
}

/// The plugin's push can't be undone, so the release's checksums are made before it, in the plugin job; the publish
/// job only checks the files against them.
#[test]
fn plugin_sums_the_release_before_it_pushes() {
    let text = release_yml();
    let lines: Vec<&str> = text.lines().collect();
    let line = |text: &str| lines.iter().position(|l| l.trim() == text);
    let (plugin, publish) = (line("plugin:"), line("publish:"));
    let (sums, push) = (line("- name: Checksums"), line("- name: Commit, tag and push"));
    let check = line("- name: Check the files against SHA256SUMS");
    let (Some(plugin), Some(publish)) = (plugin, publish) else {
        panic!("release.yml has a plugin job and then a publish job")
    };
    assert!(publish > plugin, "release.yml has a plugin job and then a publish job");
    let (sums, push) = (sums.unwrap(), push.unwrap());
    assert!(
        plugin < sums && sums < push && push < publish,
        "the plugin job makes SHA256SUMS before it pushes"
    );
    assert!(
        check.unwrap() > publish,
        "the publish job checks the files against SHA256SUMS"
    );
    assert!(!lines[publish..].iter().any(|l| l.contains("sha256sum -- *")));
}

/// The publish job releases exactly the files the plugin job summed: a changed, missing or extra file stops it.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn publish_releases_only_the_summed_files() {
    for (tamper, ok) in [("none", true), ("changed", false), ("missing", false), ("extra", false)] {
        let mut box_ = Sandbox::new();
        let cwd = box_.dir("job");
        let dist = cwd.join("dist");
        put(&dist.join("AiPet-0.2.0-linux-x64.tar.gz"), "linux\n");
        put(&dist.join("AiPet-Setup-0.2.0-win-x64.exe"), "windows\n");
        box_.step(&step("Checksums"), &cwd).ok();
        assert_eq!(fs::read_to_string(dist.join("SHA256SUMS")).unwrap().lines().count(), 2);
        match tamper {
            "changed" => fs::write(dist.join("AiPet-0.2.0-linux-x64.tar.gz"), "other\n").unwrap(),
            "missing" => fs::remove_file(dist.join("AiPet-Setup-0.2.0-win-x64.exe")).unwrap(),
            "extra" => fs::write(dist.join("AiPet-0.2.0-win-x64.zip"), "zip\n").unwrap(),
            _ => {}
        }
        box_.env.insert("PLUGIN_SHA".into(), SHA.into());

        let r = box_.step(&step("Check the files against SHA256SUMS"), &cwd);
        assert_eq!(r.exit == Some(0), ok, "{tamper}: {}", r.output());
    }
}
