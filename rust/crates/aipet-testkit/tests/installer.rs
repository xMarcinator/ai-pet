//! The installers, rehosted from AiPet.Tests/InstallerTests.cs: packaging/linux/install.sh (the installer in
//! the Linux tarball) run from a package folder the test makes, whose AiPet only writes down the environment it got
//! and whose aipet-hook is the real hook (`aipet_testkit::hook`); and the one-line installer (install.sh at the root)
//! against a release the test makes, with curl a stand-in. pkill, pgrep and the menu caches are stand-ins too, so
//! nothing reaches the user's pet or menus.

use std::fs;
use std::path::{Path, PathBuf};

use aipet_testkit::{Run, Sandbox, executable, hook, repo};

/// The package's rid on this machine.
fn rid() -> &'static str {
    if std::env::consts::ARCH == "aarch64" {
        "linux-arm64"
    } else {
        "linux-x64"
    }
}

/// AiPet-<version>-<rid>/ with install.sh, aipet.desktop.in, app/AiPet and app/aipet-hook (`MakePackage`).
fn make_package(box_: &Sandbox, version: &str) -> PathBuf {
    let dir = box_.dir(&format!("AiPet-{version}-{}", rid()));
    for file in ["install.sh", "aipet.desktop.in"] {
        fs::copy(repo().join("packaging").join("linux").join(file), dir.join(file)).unwrap();
    }
    // install.sh compares the ELF header's machine byte (offset 18) with this machine's, so the script has it there
    let mut pet = b"#!/bin/sh\n#1234567".to_vec();
    pet.push(if rid() == "linux-arm64" { 0xb7 } else { 0x3e });
    pet.extend_from_slice(
        b"\nenv > \"$AIPET_TEST_OUT/pet-env.tmp\" && mv \"$AIPET_TEST_OUT/pet-env.tmp\" \"$AIPET_TEST_OUT/pet-env\"\n",
    );
    fs::create_dir_all(dir.join("app")).unwrap();
    fs::write(dir.join("app").join("AiPet"), pet).unwrap();
    executable(
        &dir.join("app").join("aipet-hook"),
        &format!(
            "#!/bin/sh\necho \"$*\" >> \"$AIPET_TEST_OUT/hook-calls\"\nexec '{}' \"$@\"\n",
            hook().display()
        ),
    );
    dir
}

/// A package installer's sandbox (`PackageInstallerTests`).
struct Package {
    box_: Sandbox,
    dir: PathBuf,
}

impl Package {
    fn new() -> Package {
        let box_ = Sandbox::new();
        let dir = make_package(&box_, "0.2.0");
        // pkill and pgrep would stop the user's own pet; the menu caches are the user's too
        box_.stub("pkill", "exit 1");
        box_.stub("pgrep", "exit 1");
        box_.stub("gtk-update-icon-cache", "exit 0");
        box_.stub("update-desktop-database", "exit 0");
        Package { box_, dir }
    }

    fn app(&self) -> PathBuf {
        self.box_.data().join("AiPet").join("app")
    }

    fn install(&self, args: &[&str]) -> Run {
        let r = self.box_.sh(&self.dir.join("install.sh"), args);
        r.ok();
        r
    }

    fn uninstall(&self, script: &Path) {
        self.box_.sh(script, &[]).ok();
    }

    fn pet_env(&self) -> String {
        let file = self.box_.out().join("pet-env");
        for _ in 0..100 {
            if file.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        fs::read_to_string(&file).expect("the installer didn't start the pet")
    }

    fn hook_calls(&self) -> Option<Vec<String>> {
        let text = fs::read_to_string(self.box_.out().join("hook-calls")).ok()?;
        Some(text.lines().map(str::to_owned).collect())
    }
}

/// The pet runs for days and opens browsers and apps, so it mustn't keep a token the installer was given.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn install_starts_the_pet_without_the_token() {
    let mut p = Package::new();
    p.box_.env.insert("GITHUB_TOKEN".into(), "test-token-4242".into());
    p.box_.env.insert("DISPLAY".into(), ":99".into());
    p.install(&[]);
    let env = p.pet_env();
    assert!(env.contains("AIPET_TEST_OUT="), "{env}");
    assert!(!env.contains("GITHUB_TOKEN"), "{env}");
    assert!(!env.contains("test-token-4242"), "{env}");
}

/// Older versions wrote a sign-in entry, which would keep starting the pet: the installer removes it. A copy of the
/// menu entry (Icon=aipet) is the user's own choice and stays until the uninstall.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn install_removes_an_older_sign_in_entry_but_not_the_users_own() {
    let p = Package::new();
    let app = p.app().display().to_string();
    let old = p.box_.home().join(".config").join("autostart").join("aipet.desktop");
    let own = p.box_.config().join("autostart").join("aipet.desktop");
    for file in [&old, &own] {
        fs::create_dir_all(file.parent().unwrap()).unwrap();
    }
    fs::write(
        &old,
        format!("[Desktop Entry]\nType=Application\nName=AiPet\nExec=\"{app}/AiPet\"\n"),
    )
    .unwrap();
    fs::write(
        &own,
        format!("[Desktop Entry]\nType=Application\nName=AiPet\nExec=\"{app}/AiPet\"\nIcon=aipet\nX-GNOME-Autostart-enabled=true\n"),
    )
    .unwrap();

    p.install(&["--no-start"]);
    assert!(!old.exists());
    assert!(own.exists());

    p.uninstall(&p.app().join("uninstall.sh"));
    assert!(!own.exists());
    assert!(!p.app().exists());
}

/// Hooks registered at the app's own aipet-hook would fail on every event once the uninstall deletes it.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn uninstall_removes_the_hooks_registered_at_the_apps_hook() {
    let p = Package::new();
    p.install(&["--no-start"]);
    let hook = p.app().join("aipet-hook").display().to_string();
    let settings = p.box_.claude().join("settings.json");
    fs::write(
        &settings,
        format!(
            r#"{{
  "hooks": {{
    "Stop": [
      {{ "hooks": [ {{ "type": "command", "command": "{hook}", "args": ["--agent", "claude"], "timeout": 5 }} ] }},
      {{ "hooks": [ {{ "type": "command", "command": "/usr/local/bin/other-tool" }} ] }}
    ]
  }}
}}"#
        ),
    )
    .unwrap();
    let hooks_json = p.box_.codex().join("hooks.json");
    fs::write(
        &hooks_json,
        format!(r#"{{ "hooks": {{ "Stop": [ {{ "hooks": [ {{ "type": "command", "command": "'{hook}' --agent codex", "timeout": 5 }} ] }} ] }} }}"#),
    )
    .unwrap();

    p.uninstall(&p.app().join("uninstall.sh"));
    assert!(!p.app().exists());
    assert_eq!(p.hook_calls().unwrap(), ["--uninstall claude", "--uninstall codex"]);
    let after = fs::read_to_string(&settings).unwrap();
    assert!(!after.contains("aipet-hook"), "{after}");
    assert!(after.contains("/usr/local/bin/other-tool"), "{after}");
    // no hooks left: no file
    assert!(!fs::read_to_string(&hooks_json).is_ok_and(|t| t.contains("aipet-hook")));
}

/// aipet-hook --install registers its own path with symlinks resolved (/proc/self/exe), so with the data folder behind
/// a symlink the config names the real folder, not the one the installer uses.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn uninstall_removes_the_hooks_registered_at_the_apps_hook_behind_a_symlink() {
    let mut p = Package::new();
    let link = p.box_.root.join("data-link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(p.box_.data(), &link).unwrap();
    p.box_.env.insert("XDG_DATA_HOME".into(), link.display().to_string());
    p.install(&["--no-start"]);
    let real = p
        .box_
        .run(
            p.box_.tool("sh"),
            &["-c", "cd \"$1\" && pwd -P", "sh", &p.app().display().to_string()],
            None,
        )
        .stdout
        .trim()
        .to_owned();
    assert!(real.starts_with('/'), "{real}");
    let settings = p.box_.claude().join("settings.json");
    fs::write(
        &settings,
        format!(r#"{{ "hooks": {{ "Stop": [ {{ "hooks": [ {{ "type": "command", "command": "{real}/aipet-hook", "args": ["--agent", "claude"] }} ] }} ] }} }}"#),
    )
    .unwrap();

    p.uninstall(&link.join("AiPet").join("app").join("uninstall.sh"));
    assert!(!p.app().exists());
    assert_eq!(p.hook_calls().unwrap(), ["--uninstall claude"]);
    assert!(!fs::read_to_string(&settings).is_ok_and(|t| t.contains("aipet-hook")));
}

/// --uninstall removes every AiPet hook, so hooks that run another aipet-hook (a build from source) keep theirs.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn uninstall_leaves_the_hooks_of_another_aipet_hook_alone() {
    let p = Package::new();
    p.install(&["--no-start"]);
    let settings = p.box_.claude().join("settings.json");
    let text = format!(
        r#"{{ "hooks": {{ "Stop": [ {{ "hooks": [ {{ "type": "command", "command": "{}/AiPet/hooks/aipet-hook", "args": ["--agent", "claude"] }} ] }} ] }} }}"#,
        p.box_.data().display()
    );
    fs::write(&settings, &text).unwrap();

    p.uninstall(&p.app().join("uninstall.sh"));
    assert!(!p.app().exists());
    assert_eq!(p.hook_calls(), None);
    assert_eq!(fs::read_to_string(&settings).unwrap(), text);
}

// ------------------------------------------------------------------ the one-line installer
/// The one-line installer's sandbox (`OneLineInstallerTests`): a release of 0.2.0 that curl serves, whose package's
/// install.sh only writes down how it was run, and a claude that writes down its calls.
fn one_line() -> Sandbox {
    let box_ = Sandbox::new();
    let release = box_.dir("release");
    let package = box_.dir(&format!("AiPet-0.2.0-{}", rid()));
    executable(
        &package.join("install.sh"),
        "#!/bin/sh\nenv > \"$AIPET_TEST_OUT/package-env\"\necho \"$*\" > \"$AIPET_TEST_OUT/package-args\"\n",
    );
    let name = format!("AiPet-0.2.0-{}.tar.gz", rid());
    let tarball = release.join(&name).display().to_string();
    let folder = format!("AiPet-0.2.0-{}", rid());
    box_.run(
        box_.tool("tar"),
        &["-C", &box_.root.display().to_string(), "-czf", &tarball, &folder],
        None,
    )
    .ok();
    let sum = box_.run(box_.tool("sha256sum"), &[tarball.as_str()], None);
    let sum = sum.ok().stdout.split_whitespace().next().unwrap().to_owned();
    fs::write(release.join("SHA256SUMS"), format!("{sum}  {name}\n")).unwrap();
    let api = "https://api.github.com/repos/xMarcinator/ai-pet/releases/assets";
    fs::write(
        release.join("release.json"),
        format!(r#"{{"tag_name":"v0.2.0","assets":[{{"url":"{api}/1","name":"{name}"}},{{"url":"{api}/2","name":"SHA256SUMS"}}]}}"#),
    )
    .unwrap();
    box_.stub(
        "curl",
        &format!(
            r#"out= url=
while [ $# -gt 0 ]; do
  case $1 in
    -o) out=$2; shift ;;
    -H|--retry) shift ;;
    -*) ;;
    *) url=$1 ;;
  esac
  shift
done
case $url in
  */releases/latest|*/releases/tags/v0.2.0) f=release.json ;;
  */releases/assets/1|*/releases/download/v0.2.0/{name}) f={name} ;;
  */releases/assets/2|*/releases/download/v0.2.0/SHA256SUMS) f=SHA256SUMS ;;
  *) echo "curl: (22) The requested URL returned error: 404 ($url)" >&2; exit 22 ;;
esac
cp "{}/$f" "$out""#,
            release.display()
        ),
    );
    box_.stub("claude", "echo \"$*\" >> \"$AIPET_TEST_OUT/claude-calls\"");
    box_
}

fn one_line_install(box_: &Sandbox) -> Run {
    let r = box_.sh(&repo().join("install.sh"), &[]);
    r.ok();
    r
}

/// The package's installer (and the pet it starts) never needs the token, so it doesn't get it.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn install_runs_the_packages_installer_without_the_token() {
    let mut box_ = one_line();
    box_.env.insert("GITHUB_TOKEN".into(), "test-token-4242".into());
    one_line_install(&box_);
    let env = fs::read_to_string(box_.out().join("package-env")).unwrap();
    assert!(env.contains("AIPET_TEST_OUT="), "{env}");
    assert!(!env.contains("test-token-4242"), "{env}");
    let calls = fs::read_to_string(box_.out().join("claude-calls")).unwrap();
    assert!(calls.contains("plugin install aipet@aipet"), "{calls}");
}

/// AIPET_VERSION picks the app's release, but the plugin comes from the marketplace on main (the latest release's):
/// the installer says so.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn aipet_version_says_the_plugin_comes_from_the_latest_release() {
    for pinned in [true, false] {
        let mut box_ = one_line();
        if pinned {
            box_.env.insert("AIPET_VERSION".into(), "0.2.0".into());
        }
        let r = one_line_install(&box_);
        assert_eq!(
            r.stderr.contains("AIPET_VERSION pins only the app"),
            pinned,
            "{}",
            r.output()
        );
    }
}

/// The docs run these as ./build.sh and so on, which needs the executable bit (git records it).
#[cfg_attr(windows, ignore = "Unix modes only")]
#[test]
fn shell_scripts_are_executable() {
    #[cfg(unix)]
    for script in [
        "build.sh",
        "install.sh",
        "packaging/linux/install.sh",
        "scripts/check-plugin.sh",
        "scripts/install-from-source.sh",
        "scripts/check-parity.sh",
    ] {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(repo().join(script)).unwrap().permissions().mode();
        assert!(mode & 0o100 != 0, "{script} isn't executable");
    }
}
