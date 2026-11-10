//! Custom avatar files, ported from AiPet.Tests/AvatarTests.cs. They are hand-edited: one the pet can't draw is
//! skipped, never handed to the pet (Settings draws every avatar, and the pet wears the saved one at start, so a crash
//! there would repeat on every start). Both lists skip it: the avatars (`Avatar::all`) and the pets the avatars folder
//! gives (`load::all`), which bakes a legacy avatar file into a pack.

use std::fs;
use std::path::PathBuf;

use aipet_sprite::{Avatar, Pet, PetInput, load};

/// An avatars folder of the test's own with one file in it, removed with it.
struct Folder(PathBuf);

impl Folder {
    fn with(test: &str, json: &str) -> Folder {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("avatars-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("test.json"), json).unwrap();
        Folder(dir)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn an_avatar_the_pet_cant_draw_is_skipped() {
    let built_in: Vec<String> = load::all(&PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("no-avatars"))
        .pets
        .iter()
        .map(|p| p.name().to_owned())
        .collect();
    for json in [
        r#"{"name":"Short","shapes":[[13,14,7]]}"#,
        r#"{"name":"NullShape","shapes":[[13,14,7,7],null]}"#,
        r#"{"name":"Empty","shapes":[]}"#,
        r#"{"name":"NoShapes"}"#,
        r#"{"name":"Flat","shapes":[[13,14,0,7]]}"#,
        r#"{"name":"Negative","shapes":[[13,14,7,-2]]}"#,
        r#"{"name":"Huge","shapes":[[13,14,1e400,7]]}"#,
        r#"{"name":"FaceOff","shapes":[[13,14,7,7]],"faceBottom":2147483647}"#,
        r#"{"name":"FaceUp","shapes":[[13,14,7,7]],"faceTop":-5}"#,
        r#"{"name":"FaceFlip","shapes":[[13,14,7,7]],"faceTop":14,"faceBottom":9}"#,
    ] {
        // serde_json refuses 1e400, which the avatar reader takes as Infinity
        let name = json["{\"name\":\"".len()..].split('"').next().unwrap().to_owned();
        let folder = Folder::with(&name, json);
        let avatars: Vec<String> = Avatar::all(&folder.0).into_iter().map(|a| a.name).collect();
        assert!(!avatars.contains(&name), "{json}");
        for b in Avatar::built_in() {
            assert!(avatars.contains(&b.name), "{json}: {} is missing", b.name);
        }
        let found = load::all(&folder.0);
        let pets: Vec<&str> = found.pets.iter().map(|p| p.name()).collect();
        assert!(!pets.contains(&name.as_str()), "{json}");
        assert_eq!(pets, built_in, "{json}");
        assert_eq!(found.skipped.len(), 1, "{json}");
        assert_eq!(found.skipped[0].file, "test.json", "{json}");
    }
}

#[test]
fn a_good_avatar_is_listed_and_every_listed_one_draws() {
    let folder = Folder::with(
        "good",
        r##"{"name":"Mine","shapes":[[13,14,7.3,7.3,1],[13,7,4,4]],"faceTop":9,"faceBottom":14,"palette":{"body":"#3366CC"}}"##,
    );
    let all = Avatar::all(&folder.0);
    let mine: Vec<&Avatar> = all.iter().filter(|a| a.name == "Mine").collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].body, 0xFF3366CC);
    for a in all {
        let name = a.name.clone();
        let mut pet = Pet::new();
        pet.set_avatar(a);
        pet.update(
            &PetInput {
                state: "idle".into(),
                ..PetInput::default()
            },
            0.05,
        );
        assert!(pet.layers()[0].iter().any(|&px| px != 0), "{name} draws nothing");
    }
    let found = load::all(&folder.0);
    assert!(found.pets.iter().any(|p| p.name() == "Mine"));
    assert_eq!(found.skipped, []);
}
