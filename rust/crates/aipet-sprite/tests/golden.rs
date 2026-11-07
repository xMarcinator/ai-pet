//! Replays the golden data the C# pet wrote (tests/golden/frames.json, from rust/golden) through the Rust port. Every
//! pixel, PixelsChanged, AnimState and idle act must match exactly, and the doubles bit for bit (System.Text.Json
//! writes a double so that it reads back exactly).
//!
//! When the C# changes: `dotnet run --project rust/golden -c Release` from the repository root, then fix the port
//! until this passes. Never edit frames.json by hand.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use aipet_sprite::{Avatar, GH, GW, PIXELS, Pet, PetInput, PetRng, pm, try_parse_color};
use serde_json::Value;

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn golden() -> &'static Value {
    static GOLDEN: OnceLock<Value> = OnceLock::new();
    GOLDEN.get_or_init(|| {
        let path = manifest_dir().join("tests/golden/frames.json");
        let text = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{}: {e} (write it with: dotnet run --project rust/golden -c Release)",
                path.display()
            )
        });
        serde_json::from_str(&text).expect("frames.json parses")
    })
}

fn array<'a>(v: &'a Value, key: &str) -> &'a Vec<Value> {
    v[key].as_array().unwrap_or_else(|| panic!("{key} is not an array"))
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or_else(|| panic!("{key} is not a string"))
}

fn f64_of(v: &Value, key: &str) -> f64 {
    v[key].as_f64().unwrap_or_else(|| panic!("{key} is not a number"))
}

fn hex_u32(s: &str) -> u32 {
    u32::from_str_radix(s, 16).unwrap_or_else(|e| panic!("{s}: {e}"))
}

fn hex_bytes(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// The random source of a case, as in rust/golden: `None` panics (the C# Random threw), `Scripted` is the golden
/// program's ScriptedRandom: call k (counting on from the seed) gives next_double() = k * 0.6180339887498949 % 1
/// and next_int(max) = k * 7 % max.
enum TestRng {
    None,
    Scripted(i32),
}

impl PetRng for TestRng {
    fn next_double(&mut self) -> f64 {
        match self {
            TestRng::None => panic!("a golden case reached the pet's random source"),
            TestRng::Scripted(k) => {
                *k += 1;
                f64::from(*k) * 0.6180339887498949 % 1.0
            }
        }
    }

    fn next_int(&mut self, max: i32) -> i32 {
        match self {
            TestRng::None => panic!("a golden case reached the pet's random source"),
            TestRng::Scripted(k) => {
                *k += 1;
                *k * 7 % max
            }
        }
    }
}

/// Avatar::all over a fresh folder holding the same avatar files the C# loaded (the example avatar straight from the
/// repository's avatars folder).
fn avatars() -> &'static Vec<Avatar> {
    static AVATARS: OnceLock<Vec<Avatar>> = OnceLock::new();
    AVATARS.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("aipet-sprite-golden-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for f in array(golden(), "avatar_files") {
            let mut bytes = hex_bytes(str_of(f, "hex"));
            if let Some(repo) = f["repo"].as_str() {
                let file = fs::read(manifest_dir().join("../../..").join(repo)).unwrap();
                assert!(
                    file == bytes,
                    "{repo} changed since frames.json was written: run rust/golden again"
                );
                bytes = file;
            }
            fs::write(dir.join(str_of(f, "file")), bytes).unwrap();
        }
        let all = Avatar::all(&dir);
        let _ = fs::remove_dir_all(&dir);
        all
    })
}

fn avatar(name: &str) -> Avatar {
    avatars()
        .iter()
        .find(|a| a.name == name)
        .unwrap_or_else(|| panic!("the Rust port didn't load the avatar {name:?}"))
        .clone()
}

/// A layer from the file's run-length hex: "AARRGGBB" or "AARRGGBB*count", space-separated.
fn decode_layer(s: &str) -> Vec<u32> {
    let mut px = Vec::with_capacity(PIXELS);
    for token in s.split(' ') {
        let (color, count) = token
            .split_once('*')
            .map_or((token, 1), |(c, n)| (c, n.parse().unwrap()));
        px.extend(std::iter::repeat_n(hex_u32(color), count));
    }
    assert_eq!(px.len(), PIXELS, "a layer has {} pixels", px.len());
    px
}

const INPUT_KEYS: [&str; 14] = [
    "state",
    "state_since",
    "prop",
    "hover",
    "mouse",
    "dragging",
    "moved",
    "last_move",
    "facing",
    "poke_until",
    "land_until",
    "wave_until",
    "alert_until",
    "music",
];

fn parse_input(v: &Value) -> PetInput {
    // a field the C# PetInput gained would show up here
    let keys: BTreeSet<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        INPUT_KEYS.into_iter().collect(),
        "the golden PetInput has different fields"
    );
    let flag = |k: &str| v[k].as_bool().unwrap_or_else(|| panic!("{k} is not a bool"));
    PetInput {
        state: str_of(v, "state").to_owned(),
        state_since: f64_of(v, "state_since"),
        prop: v["prop"].as_str().map(str::to_owned),
        hover: flag("hover"),
        mouse: v["mouse"]
            .as_array()
            .map(|m| (m[0].as_f64().unwrap(), m[1].as_f64().unwrap())),
        dragging: flag("dragging"),
        moved: flag("moved"),
        last_move: f64_of(v, "last_move"),
        facing: i32::try_from(v["facing"].as_i64().unwrap()).unwrap(),
        poke_until: f64_of(v, "poke_until"),
        land_until: f64_of(v, "land_until"),
        wave_until: f64_of(v, "wave_until"),
        alert_until: f64_of(v, "alert_until"),
        music: flag("music"),
    }
}

/// Where two layers differ, as a map ('.' empty, 'o' same colour, 'X' different) and the first few cells.
fn diff(expected: &[u32], actual: &[u32]) -> String {
    let mut map = String::new();
    let mut cells = Vec::new();
    for y in 0..GH as usize {
        for x in 0..GW as usize {
            let (e, a) = (expected[y * GW as usize + x], actual[y * GW as usize + x]);
            map.push(if e != a {
                'X'
            } else if e == 0 {
                '.'
            } else {
                'o'
            });
            if e != a && cells.len() < 6 {
                cells.push(format!("({x}, {y}): C# {e:08X}, Rust {a:08X}"));
            }
        }
        map.push('\n');
    }
    format!("{map}{}", cells.join("\n"))
}

/// Whether this platform's sin and cos are the ones the golden data was made with (.NET on glibc), so motion must
/// match bit for bit. Elsewhere it may differ in the last bits, and `near` decides.
const EXACT_LIBM: bool = cfg!(all(target_os = "linux", target_env = "gnu"));

/// Equal to 1 part in 10⁹, far below anything the pet could show (it moves in DIPs).
fn near(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() <= 1e-9 * actual.abs().max(expected.abs()).max(1.0)
}

/// Replays one case; the first difference is the error.
fn replay(case: &Value, layers: &[Vec<u32>]) -> Result<usize, String> {
    let name = str_of(case, "name");
    let times: Vec<f64> = match case["times"].as_array() {
        Some(times) => {
            let bits = array(case, "times_bits");
            times
                .iter()
                .zip(bits)
                .map(|(t, b)| {
                    let t = t.as_f64().unwrap();
                    assert_eq!(
                        t.to_bits(),
                        u64::from_str_radix(b.as_str().unwrap(), 16).unwrap(),
                        "{name}: {t} parsed inexactly"
                    );
                    t
                })
                .collect()
        }
        None => {
            let rate = f64_of(case, "rate");
            (0..case["steps"].as_u64().unwrap()).map(|i| i as f64 / rate).collect()
        }
    };
    let phases = array(case, "phases");
    // a phase or a sample past the last step would be skipped without a word
    for ph in phases {
        let step = ph["step"].as_u64().unwrap();
        if step >= times.len() as u64 {
            return Err(format!("{name}: a phase at step {step} is past the last step"));
        }
    }
    let mut samples = array(case, "samples").iter().peekable();
    let mut checked = 0;

    let rng = match case["random_seed"].as_i64() {
        Some(seed) => TestRng::Scripted(i32::try_from(seed).unwrap()),
        None => TestRng::None,
    };
    let mut pet = Pet::with_rng(rng);
    pet.set_avatar(avatar(str_of(case, "avatar")));
    let mut input = None;
    for (i, &t) in times.iter().enumerate() {
        for ph in phases.iter().filter(|p| p["step"].as_u64() == Some(i as u64)) {
            input = Some(parse_input(&ph["input"]));
            if let Some(a) = ph["avatar"].as_str() {
                pet.set_avatar(avatar(a));
            }
        }
        let input = input.as_ref().unwrap_or_else(|| panic!("{name}: no input at step {i}"));
        let f = pet.update(input, t);
        let Some(s) = samples.next_if(|s| s["step"].as_u64() == Some(i as u64)) else {
            continue;
        };

        let at = format!("{name}, step {i} (t = {t})");
        if t.to_bits() != f64_of(s, "t").to_bits() {
            return Err(format!("{at}: the C# used t = {}", f64_of(s, "t")));
        }
        for (layer, actual) in [("body", f.body), ("glow", f.glow), ("fx", f.fx)] {
            let expected = &layers[s[layer].as_u64().unwrap() as usize];
            if expected[..] != actual[..] {
                return Err(format!("{at}: the {layer} layer differs\n{}", diff(expected, actual)));
            }
        }
        if f.pixels_changed != s["changed"].as_bool().unwrap() {
            return Err(format!(
                "{at}: pixels_changed is {}, the C# says {}",
                f.pixels_changed, s["changed"]
            ));
        }
        let motion = [
            ("x", f.x),
            ("y", f.y),
            ("scale_x", f.scale_x),
            ("scale_y", f.scale_y),
            ("shadow_scale", f.shadow_scale),
            ("shadow_opacity", f.shadow_opacity),
        ];
        for (key, actual) in motion {
            let expected = f64_of(s, key);
            // bit for bit (-0.0 is not 0.0) where the golden data was made: sin and cos come from the platform's C
            // library, and elsewhere (the MSVC CRT) their last bit can differ from glibc's
            if actual.to_bits() != expected.to_bits() && !(!EXACT_LIBM && near(actual, expected)) {
                return Err(format!("{at}: {key} is {actual:?}, the C# says {expected:?}"));
            }
        }
        if pet.anim_state() != str_of(s, "anim_state") {
            return Err(format!(
                "{at}: anim_state is {:?}, the C# says {}",
                pet.anim_state(),
                s["anim_state"]
            ));
        }
        if pet.idle_act() != s["idle_act"].as_str() {
            return Err(format!(
                "{at}: the idle act is {:?}, the C# says {}",
                pet.idle_act(),
                s["idle_act"]
            ));
        }
        checked += 1;
    }
    match samples.next() {
        Some(s) => Err(format!("{name}: a sample at step {} is past the last step", s["step"])),
        None if checked == 0 => Err(format!("{name}: no samples")),
        None => Ok(checked),
    }
}

#[test]
fn every_frame_matches_the_csharp() {
    let g = golden();
    let layers: Vec<Vec<u32>> = array(g, "layers")
        .iter()
        .map(|l| decode_layer(l.as_str().unwrap()))
        .collect();
    let cases = array(g, "cases");
    let mut failures = Vec::new();
    let mut checked = 0;
    for case in cases {
        match replay(case, &layers) {
            Ok(n) => checked += n,
            Err(e) => failures.push(e),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from the C#:\n\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n\n")
    );
    assert!(checked > 1000, "only {checked} samples checked");
}

#[test]
fn avatar_files_load_like_the_csharp() {
    let expected = array(golden(), "avatars");
    let actual = avatars();
    let actual_names: BTreeSet<&str> = actual.iter().map(|a| a.name.as_str()).collect();
    let expected_names: BTreeSet<&str> = expected.iter().map(|a| str_of(a, "name")).collect();
    assert_eq!(
        actual_names, expected_names,
        "Avatar::all loads other files than Avatar.All()"
    );
    assert_eq!(actual.len(), expected.len(), "an avatar name appears twice");
    // the built-ins come first; the files follow in directory order, which differs between the two folders
    assert_eq!(actual[0].name, str_of(&expected[0], "name"));
    assert_eq!(actual[1].name, str_of(&expected[1], "name"));

    for e in expected {
        let name = str_of(e, "name");
        let a = actual.iter().find(|a| a.name == name).unwrap();
        assert_eq!(a.shading.as_deref(), e["shading"].as_str(), "{name}: shading");
        assert_eq!(a.blush, e["blush"].as_bool().unwrap(), "{name}: blush");
        assert_eq!(
            i64::from(a.face_top),
            e["face_top"].as_i64().unwrap(),
            "{name}: face_top"
        );
        assert_eq!(
            i64::from(a.face_bottom),
            e["face_bottom"].as_i64().unwrap(),
            "{name}: face_bottom"
        );
        // the file has the four numbers of each shape the pet reads (the C# keeps any more, and so does AvatarJson,
        // but Avatar drops them)
        let shapes: Vec<[f64; 4]> = array(e, "shapes")
            .iter()
            .map(|s| {
                let s: Vec<f64> = s.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
                s.try_into().unwrap()
            })
            .collect();
        assert_eq!(a.shapes, shapes, "{name}: shapes");
        let colors = e["colors"].as_object().unwrap();
        let rust = [
            ("outline", a.outline),
            ("deep", a.deep),
            ("shade", a.shade),
            ("body", a.body),
            ("hi", a.hi),
            ("spec", a.spec),
            ("blush", a.blush_c),
            ("screen", a.screen),
            ("bezel", a.bezel),
            ("glint", a.glint),
            ("eye", a.eye),
            ("eyeHi", a.eye_hi),
            ("eyeLo", a.eye_lo),
            ("glow", a.glow),
            ("accent", a.accent),
        ];
        assert_eq!(colors.len(), rust.len());
        for (key, value) in rust {
            assert_eq!(format!("{value:08X}"), colors[key].as_str().unwrap(), "{name}: {key}");
        }
    }
}

#[test]
fn colors_parse_like_the_csharp() {
    for c in array(golden(), "colors") {
        let input = str_of(c, "input");
        assert_eq!(
            try_parse_color(input),
            c["argb"].as_str().map(hex_u32),
            "TryParseColor({input:?})"
        );
    }
}

#[test]
fn pm_matches_the_csharp() {
    for pair in array(golden(), "pm") {
        let [input, output] = [0, 1].map(|i| u32::try_from(pair[i].as_u64().unwrap()).unwrap());
        assert_eq!(pm(input), output, "Pm({input:08X})");
    }
}

/// The input in force at each sample of a case, with the sample.
fn samples_with_inputs(case: &Value) -> Vec<(&Value, PetInput)> {
    let phases = array(case, "phases");
    array(case, "samples")
        .iter()
        .map(|s| {
            let step = s["step"].as_u64().unwrap();
            // as replay applies them: the latest step up to this one, and of its phases the last
            let phase = phases
                .iter()
                .filter(|p| p["step"].as_u64().unwrap() <= step)
                .max_by_key(|p| p["step"].as_u64().unwrap())
                .unwrap();
            (s, parse_input(&phase["input"]))
        })
        .collect()
}

/// The cells a layer draws on.
fn drawn(layer: &[u32]) -> Vec<(usize, usize)> {
    (0..PIXELS)
        .filter(|&i| layer[i] != 0)
        .map(|i| (i % GW as usize, i / GW as usize))
        .collect()
}

/// The file must keep covering what the port has to get right, whoever regenerates it.
#[test]
fn the_golden_data_covers_the_pet() {
    let cases = array(golden(), "cases");
    let samples: Vec<&Value> = cases.iter().flat_map(|c| array(c, "samples")).collect();
    let states: BTreeSet<&str> = samples.iter().map(|s| str_of(s, "anim_state")).collect();
    for state in ["sleep", "idle", "thinking", "working", "attention", "done", "listen"] {
        assert!(states.contains(state), "no sample animates {state}");
    }
    assert!(samples.iter().any(|s| s["changed"] == true) && samples.iter().any(|s| s["changed"] == false));
    let names: Vec<&str> = cases.iter().map(|c| str_of(c, "name")).collect();
    for prefix in [
        "hover-",
        "drag-",
        "poke-",
        "land-",
        "wave-",
        "blink-",
        "midpoint-z/",
        "midpoint-notes/",
        "still/",
        "scripted-",
    ] {
        assert!(names.iter().any(|n| n.starts_with(prefix)), "no {prefix}* case");
    }
    let inputs: Vec<PetInput> = cases
        .iter()
        .flat_map(|c| array(c, "phases"))
        .map(|p| parse_input(&p["input"]))
        .collect();
    assert!(inputs.iter().any(|i| i.facing == -1 && i.dragging && i.moved));
    assert!(inputs.iter().any(|i| i.music) && inputs.iter().any(|i| i.prop.as_deref() == Some("lens")));
    // mouse positions on the rounding midpoints of (m.X - 65) / 28
    let mice: HashMap<i64, ()> = inputs
        .iter()
        .filter_map(|i| i.mouse)
        .map(|m| (m.0 as i64, ()))
        .collect();
    for x in [79, 51, 107, 23] {
        assert!(mice.contains_key(&x), "no hover at m.X = {x}");
    }

    // what the scripted Random decides, and the pixels that fade, must show in a sample whose pixels were drawn at
    // that step (not merely be reached between two samples)
    let layers: Vec<Vec<u32>> = array(golden(), "layers")
        .iter()
        .map(|l| decode_layer(l.as_str().unwrap()))
        .collect();
    let layer = |s: &Value, key: &str| &layers[s[key].as_u64().unwrap() as usize];
    let drawn_now: Vec<(&Value, PetInput)> = cases
        .iter()
        .flat_map(samples_with_inputs)
        .filter(|(s, _)| s["changed"] == true)
        .collect();
    let state = |s: &Value| str_of(s, "anim_state").to_owned();
    for act in ["look", "hop", "stretch", "wiggle", "tap"] {
        // shown: idle, and neither hovered nor held (which keep the act without showing it)
        assert!(
            drawn_now.iter().any(|(s, i)| s["idle_act"].as_str() == Some(act)
                && state(s) == "idle"
                && !i.hover
                && !(i.dragging && i.moved)),
            "no sample shows the idle act {act}"
        );
    }
    // a blink the Random scheduled (the first one, at 3 s, is fixed): four eye cells on row 12
    assert!(
        drawn_now
            .iter()
            .any(|(s, _)| f64_of(s, "t") > 4.0 && state(s) != "working" && {
                let cells = drawn(layer(s, "glow"));
                cells.len() == 4 && cells.iter().all(|&(_, y)| y == 12)
            }),
        "no sample shows a blink after the first"
    );
    // listening: the eyes open for 2 beats of 8 (12 eye cells), not hovered (hovering opens them too)
    assert!(
        drawn_now
            .iter()
            .any(|(s, i)| state(s) == "listen" && !i.hover && drawn(layer(s, "glow")).len() == 12),
        "no sample shows listen's open eyes"
    );
    // a note fading out (alpha 0x88) and a z fading out (alpha 0x99)
    let fx_alpha = |s: &Value, alpha: u32| layer(s, "fx").iter().any(|&px| px >> 24 == alpha);
    assert!(
        drawn_now.iter().any(|(s, _)| state(s) == "listen" && fx_alpha(s, 0x88)),
        "no sample shows a fading note"
    );
    assert!(
        drawn_now.iter().any(|(s, _)| state(s) == "sleep" && fx_alpha(s, 0x99)),
        "no sample shows a fading z"
    );
}
