//! `AiPet --check-pack` prints [`aipet_sprite::load::check`]'s line and exits by its verdict: the same text and verdict
//! the MCP tool `check_pack` gives (fnt-71 R10), for the fixture packs.

use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn check_pack_prints_the_checks_line_and_exits_by_its_verdict() {
    let rust = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths: Vec<PathBuf> = Vec::new();
    for dir in [rust.join("../avatars/samples"), rust.join("crates/aipet-sprite/packs")] {
        let mut found: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
        found.sort();
        paths.extend(found);
    }
    paths.push(rust.join("no-such-pack"));
    for path in paths {
        let (line, ok) = aipet_sprite::load::check(&path);
        let out = Command::new(env!("CARGO_BIN_EXE_AiPet"))
            .arg("--check-pack")
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            format!("{line}\n"),
            "{}",
            path.display()
        );
        assert_eq!(out.status.success(), ok, "{}", path.display());
    }
}
