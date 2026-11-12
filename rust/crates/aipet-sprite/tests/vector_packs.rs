//! The built-in vector packs drawn from the reference generators (`avatars/reference/*_gen.py --pack`) are what the
//! generators write now, and every built-in vector pack has every state the reference pets show
//! (`avatars/reference/pet_states.py`, R22), the baked pixel packs too, fnt-72's moods among them.

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
    for (generator, folder) in [("hood_gen.py", "hood-vector"), ("sprout_gen.py", "sprout-vector")] {
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
        &[
            "open", "closed", "blink", "happy", "squint", "wide", "up", "down", "sleepy",
        ],
    ),
    (
        Channel::Arms,
        &["rest", "raised", "wave-a", "wave-b", "run-a", "run-b", "hold", "lens"],
    ),
    (
        Channel::Feet,
        &["stand", "tap", "dangle-a", "dangle-b", "run-a", "run-b"],
    ),
    (Channel::Prop, &["none", "laptop", "lens", "envelope"]),
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
            "tears",
            "aura",
            "heart",
        ],
    ),
];

/// Values that draw nothing on purpose: a layer may name them with a `null` fallback alone.
const NOTHING: [&str; 2] = ["none", "off"];

/// Whether `layer` draws `value` with art of its own: it names it ([`explicit`]) and it resolves to something drawn.
/// A `null` fallback names a value but draws nothing.
fn drawn(layer: &Layer, value: &str) -> bool {
    explicit(layer, value) && layer.resolve_art(value).is_some_and(|v| layer.has(v))
}

/// fnt-72's moods (the state channel) and what shows each where the pack has no state art of its own: the pet shows
/// a mood the pack draws neither way as attention or idle instead.
const MOODS: [(&str, Channel, &str); 3] = [
    ("sad", Channel::Fx, "tears"),
    ("news", Channel::Prop, "envelope"),
    ("tired", Channel::Eyes, "sleepy"),
];

#[test]
fn every_built_in_pack_drawn_to_pet_states_has_every_state() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-custom-pets");
    let pets = aipet_sprite::load::all(&dir).pets;
    assert!(pets.len() >= 6, "{} packs", pets.len());
    assert!(pets.iter().any(|p| p.pack.kind == Kind::Pixel) && pets.iter().any(|p| p.pack.kind == Kind::Vector));
    for loaded in pets.iter().flat_map(|p| {
        // every theme too: a sheet theme swaps in its own art
        std::iter::once(p.clone()).chain(p.pack.palette.names().filter_map(|t| p.in_theme(t)).collect::<Vec<_>>())
    }) {
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
            for layer in &layers {
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
            // and some layer draws each value that shows something
            for value in values.iter().filter(|v| !NOTHING.contains(v)) {
                assert!(
                    layers.is_empty() || layers.iter().any(|l| drawn(l, value)),
                    "{}: no layer draws {} {value:?}",
                    pack.name,
                    channel.name()
                );
            }
        }
        for (mood, channel, sign) in MOODS {
            let draws = |c: Channel, v: &str| pack.layers.iter().any(|l| l.channel == c && drawn(l, v));
            assert!(
                draws(Channel::State, mood) || draws(channel, sign),
                "{}: draws nothing for the state {mood:?}, neither its own art nor {} {sign:?}",
                pack.name,
                channel.name()
            );
        }
        // the eyes follow the pointer in every direction
        let eyes = pack.layers.iter().find(|l| l.channel == Channel::Eyes).unwrap();
        for look in Look::ALL {
            assert!(eyes.look.contains_key(&look), "{}: no look {}", pack.name, look.key());
        }
    }
}

#[test]
fn a_value_removed_or_made_null_is_caught() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-custom-pets");
    let sprout = aipet_sprite::load::all(&dir)
        .pets
        .into_iter()
        .find(|p| p.pack.name == "Sprout")
        .unwrap();
    let mut tears = sprout.pack.layers.iter().find(|l| l.name == "tears").unwrap().clone();
    assert!(drawn(&tears, "tears"));
    tears.animations.remove("tears");
    assert!(!explicit(&tears, "tears"), "removed");
    tears.fallback.insert("tears".into(), None);
    assert!(
        explicit(&tears, "tears") && !drawn(&tears, "tears"),
        "a null fallback alone"
    );
}
