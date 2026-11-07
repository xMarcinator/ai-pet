//! The built-in pets' packs (`packs/<folder>/`) are what the bake makes of their avatars, byte for byte. With
//! `AIPET_BAKE=1` the test writes them instead: `AIPET_BAKE=1 cargo test -p aipet-sprite --test bake`.

use std::path::{Path, PathBuf};

use aipet_sprite::Avatar;
use aipet_sprite::bake::bake;
use aipet_sprite::manifest::files;

fn packs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("packs")
}

/// The built-in pets, by pack folder: Sprout and Hood, and the green Hood from the repo's `avatars/hood-green.json`.
fn pets() -> Vec<(&'static str, Avatar)> {
    let avatars = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../avatars");
    let green = Avatar::all(&avatars)
        .into_iter()
        .find(|a| a.name == "Hood (green)")
        .expect("avatars/hood-green.json loads");
    vec![
        ("sprout", Avatar::sprout()),
        ("hood", Avatar::hood()),
        ("hood-green", green),
    ]
}

#[test]
fn the_built_in_packs_are_the_bake_of_their_avatars() {
    let write = std::env::var_os("AIPET_BAKE").is_some_and(|v| v == "1");
    for (folder, avatar) in pets() {
        let baked = bake(&avatar);
        let dir = packs().join(folder);
        if write {
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
        }
        for (name, bytes) in files(&baked.pack, &baked.sheets) {
            let path = dir.join(&name);
            if write {
                std::fs::write(&path, &bytes).unwrap();
            } else {
                let committed = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                assert!(
                    committed == bytes,
                    "{} isn't what the bake makes: run AIPET_BAKE=1 cargo test -p aipet-sprite --test bake",
                    path.display()
                );
            }
        }
    }
}
