//! The built-in vector packs drawn from the reference generators (`avatars/reference/*_gen.py --pack`) are what the
//! generators write now, and every built-in vector pack has every state the reference pets show
//! (`avatars/reference/pet_states.py`, R22).

use std::path::{Path, PathBuf};
use std::process::Command;

use aipet_sprite::pack::{Kind, Layer};
use aipet_sprite::pose::{Channel, Look};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Runs a generator with `--pack <out>`: with python3, else python (Windows).
fn generate(generator: &str, out: &Path) {
    let script = repo().join("avatars/reference").join(generator);
    let mut last = None;
    for python in ["python3", "python"] {
        match Command::new(python)
            .arg("-B")
            .arg(&script)
            .arg("--pack")
            .arg(out)
            .status()
        {
            Ok(status) => {
                assert!(
                    status.success(),
                    "{python} {} --pack failed: {status}",
                    script.display()
                );
                return;
            }
            Err(e) => last = Some(e),
        }
    }
    panic!(
        "can't run Python to regenerate the vector packs (python3 or python on PATH): {}",
        last.unwrap()
    );
}

#[test]
fn the_vector_packs_are_what_their_generators_write() {
    for (generator, folder) in [
        ("hood_gen.py", "hood-green-vector"),
        ("hamster_gen.py", "hamster-vector"),
        ("sprout_gen.py", "sprout-vector"),
        ("hamster_pixel_gen.py", "hamster-pixel"),
    ] {
        let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(folder);
        let _ = std::fs::remove_dir_all(&out);
        generate(generator, &out);
        let made = std::fs::read_to_string(out.join("pack.json")).unwrap();
        let committed = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("packs")
                .join(folder)
                .join("pack.json"),
        )
        .unwrap()
        .replace("\r\n", "\n");
        assert!(
            made == committed,
            "packs/{folder}/pack.json isn't what {generator} writes: run python3 avatars/reference/{generator} --pack"
        );
    }
}

/// Whether `layer` has `value` itself: a drawing, pose or animation of its own, or a fallback that names it (a
/// `null` one draws nothing on purpose). A combined value (`zs+sparkles`) may take its last part. Only the default,
/// reached because nothing names the value, doesn't count.
fn explicit(layer: &Layer, value: &str) -> bool {
    if layer.has(value) || layer.fallback.contains_key(value) {
        return true;
    }
    value.rsplit_once('+').is_some_and(|(_, last)| explicit(layer, last))
}

/// Every value of each channel that `pet_states.STATES` and `PHASES` use, as the pack names them.
const USED: [(Channel, &[&str]); 6] = [
    (
        Channel::Eyes,
        &["open", "closed", "blink", "happy", "squint", "wide", "up", "down"],
    ),
    (
        Channel::Arms,
        &["rest", "raised", "wave-a", "wave-b", "run-a", "run-b", "hold", "lens"],
    ),
    (
        Channel::Feet,
        &["stand", "tap", "dangle-a", "dangle-b", "run-a", "run-b"],
    ),
    (Channel::Prop, &["none", "laptop", "lens"]),
    (Channel::Headphones, &["on", "off"]),
    (
        Channel::Fx,
        &[
            "none",
            "zs",
            "dots",
            "notes",
            "alert",
            "sparkles",
            "dust",
            "zs+sparkles",
            "dots+sparkles",
            "notes+sparkles",
            "alert+sparkles",
            "dust+sparkles",
        ],
    ),
];

#[test]
fn every_built_in_pack_drawn_to_pet_states_has_every_state() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-custom-pets");
    let vector: Vec<_> = aipet_sprite::load::all(&dir)
        .pets
        .into_iter()
        // the vector packs, and the pixel Hamster, drawn to the same list
        .filter(|p| p.pack.kind == Kind::Vector || p.pack.name == "Hamster (pixel)")
        .collect();
    assert!(vector.len() >= 5, "{} packs", vector.len());
    for loaded in vector {
        let pack = &loaded.pack;
        for (channel, values) in USED {
            let layers: Vec<&Layer> = pack.layers.iter().filter(|l| l.channel == channel).collect();
            // a pet may lack feet (the Hood floats on its robe), but not the rest
            assert!(
                !layers.is_empty() || channel == Channel::Feet,
                "{}: no {} layer",
                pack.name,
                channel.name()
            );
            for layer in layers {
                for value in values {
                    assert!(
                        explicit(layer, value),
                        "{}: layer {:?} has nothing of its own for {} {value:?}",
                        pack.name,
                        layer.name,
                        channel.name()
                    );
                }
            }
        }
        // the eyes follow the pointer in every direction
        let eyes = pack.layers.iter().find(|l| l.channel == Channel::Eyes).unwrap();
        for look in Look::ALL {
            assert!(eyes.look.contains_key(&look), "{}: no look {}", pack.name, look.key());
        }
    }
}
