//! The built-in pets' packs (`packs/<folder>/`) are what the bake makes of their avatars: the manifests as text, the
//! sheets pixel for pixel (a PNG's compressed bytes depend on the deflate build that Cargo picks). With
//! `AIPET_BAKE=1` the test writes them instead: `AIPET_BAKE=1 cargo test -p aipet-sprite --test bake`.

use std::path::{Path, PathBuf};

use aipet_sprite::manifest::{decode_png, files};

/// Whether a committed file is what was made: the same text, or for a PNG the same pixels.
fn same(path: &Path, made: &[u8]) -> bool {
    let Ok(committed) = std::fs::read(path) else {
        return false;
    };
    if path.extension().is_some_and(|e| e == "png") {
        matches!((decode_png(&committed), decode_png(made)), (Ok(a), Ok(b)) if a == b)
    } else {
        committed == made
    }
}

fn packs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("packs")
}

#[test]
fn the_built_in_packs_are_the_bake_of_their_avatars() {
    let write = std::env::var_os("AIPET_BAKE").is_some_and(|v| v == "1");
    // Sprout, and Hood with its Green theme
    for (folder, baked) in aipet_sprite::bake::built_in() {
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
                assert!(
                    same(&path, &bytes),
                    "{} isn't what the bake makes: run AIPET_BAKE=1 cargo test -p aipet-sprite --test bake",
                    path.display()
                );
            }
        }
    }
}

/// Whether two sheets are the same within `by` per channel: tiny-skia's anti-aliasing may round a step apart on
/// another CPU's SIMD.
fn close(a: &aipet_sprite::bake::Sheet, b: &aipet_sprite::bake::Sheet, by: i32) -> bool {
    (a.width, a.height) == (b.width, b.height)
        && a.pixels
            .iter()
            .zip(&b.pixels)
            .all(|(&p, &q)| (0..4).all(|i| ((p >> (8 * i)) as u8 as i32 - (q >> (8 * i)) as u8 as i32).abs() <= by))
}

#[test]
fn painted_pip_is_the_vector_pip_painted() {
    let write = std::env::var_os("AIPET_BAKE").is_some_and(|v| v == "1");
    let vector = aipet_sprite::load::load_folder(&packs().join("pip")).expect("the vector Pip loads");
    let baked = aipet_sprite::painted::paint(&vector.pack, "Pip (painted)", 2.0).unwrap();
    let dir = packs().join("pip-painted");
    if write {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
    }
    for (name, bytes) in files(&baked.pack, &baked.sheets) {
        let path = dir.join(&name);
        if write {
            std::fs::write(&path, &bytes).unwrap();
            continue;
        }
        let committed = std::fs::read(&path).unwrap_or_default();
        let matches = if name.ends_with(".png") {
            matches!((decode_png(&committed), decode_png(&bytes)), (Ok(a), Ok(b)) if close(&a, &b, 2))
        } else {
            committed == bytes
        };
        assert!(
            matches,
            "{} isn't what painting the vector Pip makes: run AIPET_BAKE=1 cargo test -p aipet-sprite --test bake",
            path.display()
        );
    }
}

#[test]
fn painted_pip_in_lilac_is_the_vector_pip_in_lilac_painted() {
    let vector = aipet_sprite::load::load_folder(&packs().join("pip")).unwrap();
    let painted = aipet_sprite::load::all(Path::new("no such folder"))
        .pets
        .into_iter()
        .find(|p| p.name() == "Pip (painted)")
        .expect("the painted Pip is built in");
    assert_eq!(painted.pack.palette.names().collect::<Vec<_>>(), ["Peach", "Lilac"]);
    let lilac = &painted.pack.palette.themes[0];
    assert!(lilac.is_sheets() && lilac.colours.is_empty());
    let mut files: Vec<&str> = lilac.sheets.iter().map(|s| s.file.as_str()).collect();
    files.sort();
    assert_eq!(files, ["arms-lilac.png", "body-lilac.png", "feet-lilac.png"]);
    // worn in Lilac, each layer is the vector Pip in Lilac, painted; in Peach, as it was
    let theirs = aipet_sprite::painted::paint(&vector.in_theme("Lilac").unwrap().pack, "Pip (painted)", 2.0).unwrap();
    let worn = painted.in_theme("Lilac").unwrap();
    assert_eq!(worn.pack, painted.pack, "a sheet theme changes only sheets");
    for (i, layer) in painted.pack.layers.iter().enumerate() {
        assert!(close(&worn.sheets[i], &theirs.sheets[i], 2), "layer {}", layer.name);
    }
    assert_ne!(worn.sheets, painted.sheets);
    assert_eq!(painted.in_theme("Peach").as_ref(), Some(&painted));
    let (line, ok) = aipet_sprite::load::check(&packs().join("pip-painted"));
    assert!(ok && line.ends_with(", themes Peach, Lilac)"), "{line}");
}

#[test]
fn painted_pip_standing_looks_like_the_vector_pip() {
    use aipet_sprite::Pose;
    use aipet_sprite::pose::Values;
    let vector = aipet_sprite::load::load_folder(&packs().join("pip")).unwrap().pack;
    let painted = aipet_sprite::painted::paint(&vector, "Pip (painted)", 2.0).unwrap();
    let pose = Pose::scripted(Values::IDLE, 0.0);
    let (w, h, flat) = aipet_sprite::compose::flatten(&painted.pack, &painted.sheets, &pose, 0.0);
    let (vw, _, drawn) = aipet_sprite::vector::still(&vector, 2.0);
    let pad = 0;
    let mut worst = 0;
    for y in 0..h {
        for x in 0..w {
            let p = flat[(y * w + x) as usize];
            let q = &drawn[(((y + pad) * vw + x + pad) * 4) as usize..][..4];
            let q = u32::from_be_bytes([q[3], q[0], q[1], q[2]]);
            for i in 0..4 {
                worst = worst.max(((p >> (8 * i)) as u8 as i32 - (q >> (8 * i)) as u8 as i32).abs());
            }
        }
    }
    // layer by layer through straight colours rounds a little differently from one pass
    assert!(worst <= 4, "a channel differs by {worst}");
}

/// The basic sample pack in the repo's `avatars/samples/basic/`: a small pack written to read, not baked from an avatar. A round
/// pet with a two-frame idle bob and a sleep frame, and eyes that blink, close and look around.
fn sample() -> (aipet_sprite::pack::Pack, Vec<aipet_sprite::bake::Sheet>) {
    use aipet_sprite::bake::Sheet;
    use aipet_sprite::pack::{Animation, Art, Clock, Frame, Kind, Layer, LoopMode, Mirror, Pack};
    use aipet_sprite::pose::{Channel, Look};
    const W: u32 = 26;
    const H: u32 = 24;
    let (outline, body, shade, eye) = (0xFF2B_1D3Au32, 0xFFB8_8CF0u32, 0xFF8A_62C9u32, 0xFFFF_F2A8u32);
    // a disc, `lift` pixels up, outlined, darker at the bottom
    let disc = |lift: i32| -> Vec<u32> {
        let mut px = vec![0; (W * H) as usize];
        let inside = |x: i32, y: i32| {
            let (dx, dy) = (x as f64 + 0.5 - 13.0, y as f64 + 0.5 - (14.0 - lift as f64));
            dx * dx / 64.0 + dy * dy / 56.25 <= 1.0
        };
        for y in 0..H as i32 {
            for x in 0..W as i32 {
                let i = (y * W as i32 + x) as usize;
                if inside(x, y) {
                    px[i] = if y > 17 - lift { shade } else { body };
                } else if [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dy)| inside(x + dx, y + dy))
                {
                    px[i] = outline;
                }
            }
        }
        px
    };
    let eyes = |rows: &[i32]| -> Vec<u32> {
        let mut px = vec![0; (W * H) as usize];
        for x in [9, 10, 15, 16] {
            for &y in rows {
                px[(y * W as i32 + x) as usize] = eye;
            }
        }
        px
    };
    let sheet = |frames: &[Vec<u32>]| -> Sheet {
        let mut pixels = vec![0; (W * H) as usize * frames.len()];
        let width = W * frames.len() as u32;
        for (n, f) in frames.iter().enumerate() {
            for y in 0..H as usize {
                for x in 0..W as usize {
                    pixels[y * width as usize + n * W as usize + x] = f[y * W as usize + x];
                }
            }
        }
        Sheet {
            width,
            height: H,
            pixels,
        }
    };
    let frames = |sheet: usize, at: &[(u32, u32)], mode: LoopMode| Animation {
        frames: at
            .iter()
            .map(|&(n, ms)| Frame {
                art: Art::Raster {
                    sheet,
                    x: n * W,
                    y: 0,
                    w: W,
                    h: H,
                    pivot: (0.0, 0.0),
                },
                ms,
            })
            .collect(),
        mode,
        clock: Clock::Change,
    };
    let mut body_layer = Layer::new("body", Channel::State, "idle");
    body_layer.hit = true;
    body_layer
        .animations
        .insert("idle".into(), frames(0, &[(0, 400), (1, 400)], LoopMode::Loop));
    body_layer
        .animations
        .insert("sleep".into(), frames(0, &[(2, 1000)], LoopMode::Once));
    body_layer.fallback.insert("listen".into(), Some("sleep".into()));
    let mut eyes_layer = Layer::new("eyes", Channel::Eyes, "open");
    eyes_layer.glow = true;
    eyes_layer
        .animations
        .insert("open".into(), frames(1, &[(0, 1000)], LoopMode::Once));
    eyes_layer
        .animations
        .insert("closed".into(), frames(1, &[(1, 1000)], LoopMode::Once));
    eyes_layer.fallback.insert("blink".into(), Some("closed".into()));
    eyes_layer.fallback.insert("happy".into(), Some("closed".into()));
    for (key, dx) in [("left", -5.0), ("right", 5.0), ("far-left", -10.0), ("far-right", 10.0)] {
        eyes_layer.look.insert(Look::parse(key).unwrap(), (dx, 0.0));
    }
    let pack = Pack {
        name: "Sample".into(),
        kind: Kind::Pixel,
        pixel_size: 5.0,
        anchor: (13.0, 22.0),
        mirror: Mirror::Flip,
        accent: 0xFFB8_8CF0,
        glow: 0xFFFF_F2A8,
        layers: vec![body_layer, eyes_layer],
        palette: Default::default(),
    };
    let sleeping = {
        let mut px = disc(0);
        for x in [9, 10, 15, 16] {
            px[(13 * W as i32 + x) as usize] = outline;
        }
        px
    };
    let sheets = vec![
        sheet(&[disc(0), disc(1), sleeping]),
        sheet(&[eyes(&[11, 12]), eyes(&[13])]),
    ];
    (pack, sheets)
}

#[test]
fn the_sample_pack_is_what_its_code_makes_and_loads_from_disk() {
    let (pack, sheets) = sample();
    assert_eq!(pack.validate(), Ok(()));
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../avatars/samples/basic");
    let write = std::env::var_os("AIPET_BAKE").is_some_and(|v| v == "1");
    if write {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
    }
    for (name, bytes) in files(&pack, &sheets) {
        let path = dir.join(&name);
        if write {
            std::fs::write(&path, &bytes).unwrap();
        } else {
            assert!(
                same(&path, &bytes),
                "{} isn't what the sample's code makes: run AIPET_BAKE=1 cargo test -p aipet-sprite --test bake",
                path.display()
            );
        }
    }
    let loaded = aipet_sprite::load::load_folder(&dir).unwrap();
    assert_eq!((loaded.pack, loaded.sheets), (pack, sheets));
}
