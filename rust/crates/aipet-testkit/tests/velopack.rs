//! The Windows app's Velopack side against release.yml and the installers, ported from tests/AiPet.Tests/
//! VelopackTests.cs: the channel, the versions and the repository they must agree on, and the order the pet's `main`
//! does things in. (HookCleanup's cases and the status texts are aipet-core's tests/data.rs; Velopack's startup and
//! the update checks, aipet's src/updates.rs.)

use std::fs;

use aipet_core::update_status;
use aipet_testkit::repo;

fn release_yml() -> String {
    fs::read_to_string(repo().join(".github/workflows/release.yml")).unwrap()
}

fn source(file: &str) -> String {
    fs::read_to_string(repo().join("rust/crates").join(file)).unwrap()
}

/// The app looks for releases.<channel>.json in the releases, which vpk pack names after its --channel.
#[test]
fn channel_is_the_one_release_yml_packs() {
    let release = release_yml();
    let at = release.find("\"$vpk\" pack ").expect("release.yml runs vpk pack");
    let pack = &release[at..at + release[at..].find("--outputDir").expect("vpk pack's --outputDir")];
    assert!(pack.contains("--packId AiPetApp "), "{pack}");
    assert!(
        pack.contains(&format!("--channel {} ", update_status::CHANNEL)),
        "{pack}"
    );
    assert!(!release.contains("--skipVeloAppCheck"));
    assert!(release.contains(&format!("releases.{}.json", update_status::CHANNEL)));
}

/// vpk warns when the app's Velopack library isn't its own version: the velopack crate the pet is built with, pinned
/// exactly, is VPK_VERSION.
#[test]
fn vpk_version_is_the_package_version() {
    let release = release_yml();
    let vpk = release
        .lines()
        .find_map(|l| l.trim().strip_prefix("VPK_VERSION:"))
        .map(str::trim)
        .expect("release.yml sets VPK_VERSION");
    assert!(!vpk.is_empty());
    let manifest = source("aipet/Cargo.toml");
    let pinned = manifest
        .lines()
        .find_map(|l| l.trim().strip_prefix("velopack = \"="))
        .and_then(|rest| rest.split('"').next())
        .expect("the pet pins the velopack crate exactly");
    assert_eq!(pinned, vpk);
}

#[test]
fn repo_is_the_one_the_installers_use() {
    assert_eq!(update_status::REPO, "https://github.com/xMarcinator/ai-pet");
    let install = fs::read_to_string(repo().join("install.ps1")).unwrap();
    assert!(install.contains("$repo = 'xMarcinator/ai-pet'"));
}

/// The lines of `fn <name>` in a source file, to its closing brace at the start of a line.
fn body<'a>(text: &'a str, signature: &str) -> &'a str {
    let at = text.find(signature).unwrap_or_else(|| panic!("no {signature}"));
    let end = text[at..]
        .find("\n}\n")
        .unwrap_or_else(|| panic!("{signature} doesn't end"));
    &text[at..at + end]
}

fn position(body: &str, what: &str) -> usize {
    body.find(what).unwrap_or_else(|| panic!("{what} isn't in {body}"))
}

/// Velopack's own install of an update downloaded earlier would run before the single instance, where a second pet
/// would have the updater stop the running one. It's off, and `updates::start` does it once `main` knows this is the
/// only pet, before the shell starts.
#[test]
fn pending_update_is_installed_by_the_only_pet() {
    let main = source("aipet/src/main.rs");
    let entry = body(&main, "fn main() -> ExitCode {");
    let (startup, only) = (position(entry, "updates::startup();"), position(entry, "only_pet()"));
    assert!(
        startup < only,
        "main takes the single instance before Velopack's startup"
    );
    assert!(position(entry, "pet(wayland, instance)") > only);
    let pet = body(&main, "fn pet(wayland: bool, instance: single::Instance)");
    let (start, shell) = (position(pet, "updates::start()"), position(pet, "shell(wayland, "));
    assert!(start < shell, "the pet starts its shell before the updates");
    let updates = source("aipet/src/updates.rs");
    let app = body(&updates, "fn app(args: Vec<String>, installed: bool, hook: &Path)");
    assert!(app.contains(".set_auto_apply_on_startup(false)"), "{app}");
}
