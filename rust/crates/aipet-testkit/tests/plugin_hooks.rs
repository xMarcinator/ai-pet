//! scripts/check-plugin.sh --hook against the hook, rehosted from tests/AiPet.Tests/PluginHooksTests.cs: CI compares
//! the plugin's hook files with what `aipet-hook --print-plugin-hooks` prints (the printing itself is aipet-hook's
//! tests/registration.rs).

use std::fs;

use aipet_testkit::{Sandbox, hook, repo};

/// check-plugin.sh --hook passes the plugin as it is, and shows the diff when a hook file differs from what the hook
/// prints, but not for CRLF line endings, which only a Windows checkout gives it.
#[cfg_attr(windows, ignore = "runs sh scripts")]
#[test]
fn check_plugin_compares_the_hook_files_with_the_hook() {
    let box_ = Sandbox::new();
    let hook = hook().display().to_string();
    let script = repo().join("scripts").join("check-plugin.sh").display().to_string();
    let plugin = repo().join("plugins").join("aipet").display().to_string();
    let bash = box_.tool("bash");

    let ok = box_.run(&bash, &[script.as_str(), &plugin, "--hook", &hook], None);
    ok.ok();
    assert!(!ok.stdout.contains("JSON only"), "{}", ok.stdout);

    // without --hook its OK says the hook files weren't compared
    let json = box_.run(&bash, &[script.as_str(), &plugin], None);
    json.ok();
    assert!(
        json.stdout.contains("hook files checked as JSON only"),
        "{}",
        json.stdout
    );

    // --hook takes the rest of the arguments, so an option of the script's after it is an error, not lost
    let late = box_.run(&bash, &[script.as_str(), "--hook", &hook, "--release", &plugin], None);
    assert_eq!(late.exit, Some(2), "{}", late.output());
    assert!(late.stderr.contains("put --release before it"), "{}", late.stderr);

    let copy = box_.root.join("plugin");
    box_.run(box_.tool("cp"), &["-R", &plugin, &copy.display().to_string()], None)
        .ok();
    let claude = copy.join("hooks").join("hooks.json");
    let codex = copy.join("hooks").join("codex.json");
    let text = fs::read_to_string(&claude)
        .unwrap()
        .replace("\r\n", "\n")
        .replace('\n', "\r\n");
    fs::write(&claude, text).unwrap();
    let text = fs::read_to_string(&codex)
        .unwrap()
        .replace("\"timeout\": 30", "\"timeout\": 31");
    fs::write(&codex, text).unwrap();
    let bad = box_.run(
        &bash,
        &[script.as_str(), &copy.display().to_string(), "--hook", &hook],
        None,
    );
    assert_eq!(bad.exit, Some(1), "{}", bad.output());
    assert!(!bad.output().contains("hooks.json"), "{}", bad.output());
    let said = format!(
        "{} isn't what aipet-hook --print-plugin-hooks codex prints",
        codex.display()
    );
    assert!(bad.output().contains(&said), "{}", bad.output());
    assert!(bad.stderr.contains("-            \"timeout\": 31,"), "{}", bad.stderr);
    assert!(bad.stderr.contains("+            \"timeout\": 30,"), "{}", bad.stderr);
}
