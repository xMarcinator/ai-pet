//! Throwaway spike for fn-2 task 15. See rust/proofs/aseprite.md.
mod reader;
mod writer;

/// The spec's pack limits, applied to a native file.
const LIMITS: reader::Limits = reader::Limits { max_file: 8 << 20, max_side: 2048, max_frames: 512, max_layers: 16, max_decoded: 64 << 20 };

fn compare(path: &Path) {
    let name = path.file_name().unwrap().to_string_lossy();
    let bytes = std::fs::read(path).unwrap();
    let ours = reader::read(&bytes, &LIMITS);
    let theirs = AsepriteFile::read(&bytes[..]);
    let (o, t) = match (ours, theirs) {
        (Ok(o), Ok(t)) => (o, t),
        (o, t) => { println!("{name}: direct={:?} asefile={:?}", o.err().unwrap_or("Ok".into()), t.err().map(|e| e.to_string()).unwrap_or("Ok".into())); return; }
    };
    let mut cels = 0; let mut diff_px = 0; let mut skipped = 0;
    assert_eq!(o.durations, (0..t.num_frames()).map(|i| t.frame(i).duration()).collect::<Vec<_>>());
    assert_eq!(o.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), t.layers().map(|l| l.name().to_string()).collect::<Vec<_>>());
    assert_eq!(o.tags.iter().map(|g| (g.name.as_str(), g.from as u32, g.to as u32)).collect::<Vec<_>>(),
               (0..t.num_tags()).map(|i| (t.tag(i).name(), t.tag(i).from_frame(), t.tag(i).to_frame())).collect::<Vec<_>>());
    for (os, ts) in o.slices.iter().zip(t.slices()) {
        assert_eq!(os.name, ts.name);
        for (ok, tk) in os.keys.iter().zip(&ts.keys) {
            assert_eq!((ok.frame, ok.x, ok.y, ok.w, ok.h, ok.pivot), (tk.from_frame, tk.origin.0, tk.origin.1, tk.size.0, tk.size.1, tk.pivot));
        }
    }
    for l in t.layers() {
        if l.layer_type() != LayerType::Image { continue; }
        if l.blend_mode() != asefile::BlendMode::Normal { skipped += 1; continue; }
        for fr in 0..t.num_frames() {
            let a = l.frame(fr).image();
            let b = o.layer_image(fr as usize, l.id() as usize).unwrap_or_else(|| RgbaImage::new(o.width, o.height));
            cels += 1;
            diff_px += a.pixels().zip(b.pixels()).filter(|(x, y)| x != y && !(x[3] == 0 && y[3] == 0)).count();
        }
    }
    println!("{name}: same frames, durations, layers, tags, slices; {cels} layer cels, {diff_px} px differ{}",
        if skipped > 0 { format!(" ({skipped} non-normal-blend layers not compared)") } else { String::new() });
}

fn rbench(path: &Path, iters: u32) {
    let bytes = std::fs::read(path).unwrap();
    let mut ts = vec![]; let mut peak = 0; let mut kept = 0;
    for _ in 0..iters {
        let b0 = reset_peak();
        let t = Instant::now();
        let f = reader::read(&bytes, &reader::Limits { max_decoded: usize::MAX, max_file: usize::MAX, ..LIMITS });
        ts.push(t.elapsed().as_secs_f64() * 1000.0);
        peak = PEAK.load(Relaxed) - b0;
        kept = CUR.load(Relaxed) - b0;
        drop(f);
    }
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("{}: direct read (all cels decoded to RGBA): median {:.2} ms, heap peak {}, retained {}", path.display(), ts[ts.len() / 2], mb(peak), mb(kept));
    let r = reader::read(&bytes, &LIMITS);
    println!("  with the spec's limits: {}", r.map(|_| "Ok".to_string()).unwrap_or_else(|e| e));
}

use asefile::{AnimationDirection, AsepriteFile, LayerType, PixelFormat};
use image::RgbaImage;
use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;
use writer::*;

// ---- heap accounting: current and peak bytes -------------------------------------------------------------
struct Counting;
static CUR: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = System.alloc(l);
        if !p.is_null() { let c = CUR.fetch_add(l.size(), Relaxed) + l.size(); PEAK.fetch_max(c, Relaxed); }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { System.dealloc(p, l); CUR.fetch_sub(l.size(), Relaxed); }
}
#[global_allocator]
static A: Counting = Counting;
fn reset_peak() -> usize { let c = CUR.load(Relaxed); PEAK.store(c, Relaxed); c }
fn mb(b: usize) -> String { format!("{:.2} MB", b as f64 / 1048576.0) }

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = &args[1..];
    match args[0].as_str() {
        "info" => rest.iter().for_each(|f| info(Path::new(f))),
        "render" => rest[1..].iter().for_each(|f| render(Path::new(f), Path::new(&rest[0]))),
        "fixture" => fixture(Path::new(&rest[0])),
        "map" => map(Path::new(&rest[0])),
        "grid" => grid(Path::new(&rest[0])),
        "typical" => typical(Path::new(&rest[0]), rest[1].parse().unwrap(), rest[2].parse().unwrap(), rest[3].parse().unwrap(), rest[4].parse().unwrap()),
        "bench" => bench(Path::new(&rest[0]), rest[1].parse().unwrap()),
        "hostile" => hostile(Path::new(&rest[0])),
        "compare" => rest.iter().for_each(|f| compare(Path::new(f))),
        "rbench" => rbench(Path::new(&rest[0]), rest[1].parse().unwrap()),
        "direct" => rest.iter().for_each(|f| {
            let b0 = reset_peak();
            let t = Instant::now();
            let r = reader::read(&std::fs::read(f).unwrap(), &LIMITS);
            println!("{f}: {} in {:.1} ms, heap peak {}", r.map(|_| "Ok".to_string()).unwrap_or_else(|e| format!("Err({e})")),
                t.elapsed().as_secs_f64() * 1000.0, mb(PEAK.load(Relaxed) - b0));
        }),
        _ => panic!("unknown command"),
    }
}

// ---- Q2: what asefile reports -----------------------------------------------------------------------------
fn info(path: &Path) {
    let name = path.file_name().unwrap().to_string_lossy();
    let f = match AsepriteFile::read_file(path) {
        Ok(f) => f,
        Err(e) => { println!("{name}: ERROR {e}"); return; }
    };
    let fmt = match f.pixel_format() {
        PixelFormat::Rgba => "rgba".to_string(),
        PixelFormat::Grayscale => "grayscale".to_string(),
        PixelFormat::Indexed { transparent_color_index } => format!("indexed(t={transparent_color_index})"),
    };
    let durs: Vec<u32> = (0..f.num_frames()).map(|i| f.frame(i).duration()).collect();
    println!("{name}: {}x{} {fmt}, {} frames ms={durs:?}, palette={}", f.width(), f.height(), f.num_frames(),
        f.palette().map_or(0, |p| p.num_colors()));
    for l in f.layers() {
        let kind = match l.layer_type() { LayerType::Image => "image".into(), LayerType::Group => "group".into(),
            LayerType::Tilemap(t) => format!("tilemap({t:?})") };
        let parent = l.parent().map(|p| p.name().to_string()).unwrap_or_default();
        let cels = (0..f.num_frames()).filter(|&i| !l.frame(i).is_empty()).count();
        println!("  layer {} {:?} {kind} parent={parent:?} visible={} opacity={} blend={:?} cels={cels} ud={:?}",
            l.id(), l.name(), l.is_visible(), l.opacity(), l.blend_mode(), l.user_data().and_then(|u| u.text.clone()));
    }
    for i in 0..f.num_tags() {
        let t = f.tag(i);
        println!("  tag {:?} {}..={} {:?} repeat={:?} ud={:?}", t.name(), t.from_frame(), t.to_frame(),
            t.animation_direction(), t.repeat(), t.user_data().and_then(|u| u.text.clone()));
    }
    for s in f.slices() {
        let keys: Vec<String> = s.keys.iter().map(|k| format!("f{}@{:?}{:?} pivot={:?} 9={}", k.from_frame, k.origin,
            k.size, k.pivot, k.slice9.is_some())).collect();
        println!("  slice {:?} keys=[{}]", s.name, keys.join(", "));
    }
    if !f.tilesets().is_empty() { println!("  tilesets={}", f.tilesets().iter().count()); }
    if !f.external_files().map().is_empty() { println!("  external files present"); }
    if let Some(u) = f.sprite_user_data() { println!("  sprite ud={:?}", u.text); }
}

fn render(path: &Path, out: &Path) {
    std::fs::create_dir_all(out).unwrap();
    let f = AsepriteFile::read_file(path).unwrap();
    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
    for fr in 0..f.num_frames() {
        let img = f.frame(fr).image();
        img.save(out.join(format!("{stem}-f{fr}.png"))).unwrap();
        for l in f.layers() {
            if l.layer_type() != LayerType::Image || l.frame(fr).is_empty() { continue; }
            let li = l.frame(fr).image();
            let opaque = li.pixels().filter(|p| p[3] > 0).count();
            li.save(out.join(format!("{stem}-f{fr}-{}.png", l.name()))).unwrap();
            println!("{stem} f{fr} layer {:?}: {opaque} opaque px, bbox {:?}", l.name(), bbox(&li));
        }
    }
    // Compare frames with the repo's reference renders: <stem>.png (frame 0) or <stem>_0N.png (frame N-1).
    for fr in 0..f.num_frames() {
        let numbered = path.with_file_name(format!("{stem}_{:02}.png", fr + 1));
        let reference = if numbered.exists() { numbered } else if fr == 0 { path.with_extension("png") } else { continue };
        if !reference.exists() { continue; }
        let r = image::open(&reference).unwrap().to_rgba8();
        let a = f.frame(fr).image();
        let diff = a.pixels().zip(r.pixels()).filter(|(x, y)| x != y && !(x[3] == 0 && y[3] == 0)).count();
        println!("{stem}: frame {fr} vs {}: {} of {} px differ", reference.file_name().unwrap().to_string_lossy(), diff, a.width() * a.height());
    }
}

fn bbox(img: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > 0 { x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y); }
    }
    (x0 != u32::MAX).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

// ---- Q4: the fixture --------------------------------------------------------------------------------------
const CLEAR: [u8; 4] = [0, 0, 0, 0];
const GREEN: [u8; 4] = [92, 200, 110, 255];
const DARK: [u8; 4] = [20, 60, 30, 255];
const WHITE: [u8; 4] = [250, 250, 250, 255];
const BLACK: [u8; 4] = [10, 10, 10, 255];

fn body_px(w: usize, h: usize) -> Vec<[u8; 4]> {
    (0..w * h).map(|i| { let (x, y) = (i % w, i / w);
        if x == 0 || y == 0 || x == w - 1 || y == h - 1 { DARK } else { GREEN } }).collect()
}
/// Both eyes in one 12x4 cel: open = 2x3 white with a black pupil row; closed = one black row.
fn eyes_px(open: bool) -> Vec<[u8; 4]> {
    let (w, h) = (12, 4);
    (0..w * h).map(|i| { let (x, y) = (i % w, i / w);
        let in_eye = (1..3).contains(&x) || (9..11).contains(&x);
        match (open, in_eye, y) { (true, true, 0..=1) => WHITE, (true, true, 2) => BLACK, (false, true, 2) => BLACK, _ => CLEAR }
    }).collect()
}

pub fn fixture_sprite(depth: Depth) -> Sprite {
    let palette = vec![CLEAR, GREEN, DARK, WHITE, BLACK];
    let enc = |px: Vec<[u8; 4]>| -> Vec<u8> {
        match depth {
            Depth::Rgba => px.concat(),
            Depth::Indexed { .. } => px.iter().map(|c| palette.iter().position(|p| p == c).unwrap() as u8).collect(),
        }
    };
    let body = |y: i16, raw: bool| {
        let px = enc(body_px(20, 14));
        Cel { layer: 0, x: 6, y, w: 20, h: 14, data: if raw { CelData::Raw(px) } else { CelData::Zlib(px) } }
    };
    let eyes = |open| Cel { layer: 1, x: 10, y: 15, w: 12, h: 4, data: CelData::Zlib(enc(eyes_px(open))) };
    Sprite {
        width: 32, height: 32, depth, palette: palette.clone(), layer_uuids: false,
        layers: vec![
            Layer { name: "body".into(), group: false, child_level: 0, visible: true },
            Layer { name: "eyes".into(), group: false, child_level: 0, visible: true },
        ],
        frames: vec![
            Frame { ms: 160, cels: vec![body(14, true), eyes(true)] },
            Frame { ms: 120, cels: vec![body(15, false),
                Cel { layer: 1, x: 10, y: 15, w: 0, h: 0, data: CelData::Linked(0) }] },
            Frame { ms: 90, cels: vec![body(14, false), eyes(false)] },
        ],
        tags: vec![
            Tag { name: "body:idle".into(), from: 0, to: 2, direction: 0, repeat: 0 },
            Tag { name: "eyes:open".into(), from: 0, to: 1, direction: 2, repeat: 0 },
            Tag { name: "eyes:closed".into(), from: 2, to: 2, direction: 0, repeat: 1 },
        ],
        slices: vec![
            Slice { name: "eye-left".into(), keys: vec![
                SliceKey { frame: 0, x: 10, y: 14, w: 4, h: 5, pivot: Some((2, 2)) },
                SliceKey { frame: 2, x: 10, y: 15, w: 4, h: 5, pivot: Some((2, 2)) } ] },
            Slice { name: "eye-right".into(), keys: vec![
                SliceKey { frame: 0, x: 18, y: 14, w: 4, h: 5, pivot: Some((2, 2)) } ] },
            Slice { name: "anchor".into(), keys: vec![
                SliceKey { frame: 0, x: 16, y: 31, w: 1, h: 1, pivot: Some((0, 0)) } ] },
        ],
    }
}

fn fixture(out: &Path) {
    std::fs::create_dir_all(out).unwrap();
    let write_one = |name: &str, sp: &Sprite| -> PathBuf {
        let bytes = writer::write(sp);
        let p = out.join(name);
        std::fs::write(&p, &bytes).unwrap();
        println!("wrote {} ({} bytes)", p.display(), bytes.len());
        p
    };
    let rgba = write_one("fixture-rgba.aseprite", &fixture_sprite(Depth::Rgba));
    let idx = write_one("fixture-indexed.aseprite", &fixture_sprite(Depth::Indexed { transparent: 0 }));
    let mut sp = fixture_sprite(Depth::Rgba); sp.layer_uuids = true;
    let uuid = write_one("fixture-uuid.aseprite", &sp);
    let mut sp = fixture_sprite(Depth::Rgba); sp.tags[1].direction = 3;
    let pprev = write_one("fixture-pingpong-reverse.aseprite", &sp);
    let mut sp = fixture_sprite(Depth::Rgba);
    sp.layers.insert(0, Layer { name: "face".into(), group: true, child_level: 0, visible: false });
    for l in &mut sp.layers[1..] { l.child_level = 1; }
    for f in &mut sp.frames { for c in &mut f.cels { c.layer += 1; } }
    let group = write_one("fixture-group.aseprite", &sp);

    for p in [&rgba, &idx, &uuid, &pprev, &group] { info(p); }

    // Read back and check pixels against what was written.
    for p in [&rgba, &idx, &uuid] {
        let f = AsepriteFile::read_file(p).unwrap();
        assert_eq!((f.width(), f.height(), f.num_frames(), f.num_layers()), (32, 32, 3, 2));
        let body = f.layer_by_name("body").unwrap();
        let eyes = f.layer_by_name("eyes").unwrap();
        for fr in 0..3 {
            let b = body.frame(fr).image();
            let y = if fr == 1 { 15 } else { 14 };
            assert_eq!(bbox(&b), Some((6, y, 20, 14)), "body bbox f{fr}");
            assert_eq!(b.get_pixel(6, y).0, DARK);
            assert_eq!(b.get_pixel(7, y + 1).0, GREEN);
            let e = eyes.frame(fr).image();
            if fr < 2 {
                assert_eq!(bbox(&e), Some((11, 15, 10, 3)), "eyes open f{fr} (frame 1 is linked)");
                assert_eq!(e.get_pixel(11, 15).0, WHITE);
                assert_eq!(e.get_pixel(11, 17).0, BLACK);
            } else {
                assert_eq!(bbox(&e), Some((11, 17, 10, 1)), "eyes closed");
            }
        }
        let durs: Vec<u32> = (0..3).map(|i| f.frame(i).duration()).collect();
        assert_eq!(durs, [160, 120, 90]);
        println!("read back OK: {}", p.display());
    }
}

// ---- Q3: the spec's mapping, prototyped -------------------------------------------------------------------
#[derive(Debug)]
struct RasterFrame { x: u32, y: u32, w: u32, h: u32, pivot: (i32, i32), ms: u32 }

fn loop_mode(dir: AnimationDirection, repeat: Option<std::num::NonZeroU32>, n: usize) -> (Vec<usize>, &'static str) {
    let fwd: Vec<usize> = (0..n).collect();
    let rev: Vec<usize> = (0..n).rev().collect();
    // asefile has no PingPongReverse: it rejects direction 3 at parse time.
    let (base, pingpong) = match dir {
        AnimationDirection::Forward => (fwd, false),
        AnimationDirection::Reverse => (rev, false),
        AnimationDirection::PingPong => (fwd, true),
    };
    match (repeat.map(|r| r.get()), pingpong) {
        (None, false) => (base, "loop"),
        (None, true) => (base, "pingpong"),
        // A count: unroll it and hold the last frame. Ping-pong's count is per direction.
        (Some(r), false) => (base.iter().cycle().take(n * r as usize).copied().collect(), "once"),
        (Some(r), true) => {
            let mut seq = base.clone();
            for i in 1..r as usize {
                let leg: Vec<usize> = if i % 2 == 1 { base.iter().rev().skip(1).copied().collect() }
                                      else { base.iter().skip(1).copied().collect() };
                seq.extend(leg);
            }
            (seq, "once")
        }
    }
}

fn map(path: &Path) {
    let f = AsepriteFile::read_file(path).unwrap();
    // What pack.json would say: the eyes layer has two parts.
    let parts_of = |layer: &str| -> Vec<&'static str> { if layer == "eyes" { vec!["eye-left", "eye-right"] } else { vec![] } };
    let slice_at = |name: &str, frame: u32| {
        let s = f.slices().iter().find(|s| s.name == name)?;
        s.keys.iter().filter(|k| k.from_frame <= frame).last()
    };
    let anchor = slice_at("anchor", 0).map(|k| (k.origin.0 + k.pivot.unwrap_or((0, 0)).0, k.origin.1 + k.pivot.unwrap_or((0, 0)).1));
    println!("anchor (from slice): {anchor:?}");
    // Part origins: slice key at frame 0, origin + pivot, in canvas pixels.
    for l in f.layers() {
        for part in parts_of(l.name()) {
            let k = slice_at(part, 0).unwrap();
            let p = k.pivot.unwrap_or((k.size.0 as i32 / 2, k.size.1 as i32 / 2));
            println!("layer {:?} part {part:?}: origin ({}, {})", l.name(), k.origin.0 + p.0, k.origin.1 + p.1);
        }
    }
    for t in (0..f.num_tags()).map(|i| f.tag(i)) {
        let Some((layer, key)) = t.name().split_once(':') else { println!("tag {:?}: not <layer>:<key>, reason line", t.name()); continue };
        let Some(l) = f.layer_by_name(layer) else { println!("tag {:?}: unknown layer, reason line", t.name()); continue };
        let n = (t.to_frame() - t.from_frame() + 1) as usize;
        let (order, mode) = loop_mode(t.animation_direction(), t.repeat(), n);
        let frames: Vec<u32> = order.iter().map(|&i| t.from_frame() + i as u32).collect();
        println!("animation {layer}.{key}: frames {frames:?} loop {mode:?}");
        let parts = parts_of(layer);
        for &fr in &frames {
            let img = l.frame(fr).image();
            let ms = f.frame(fr).duration();
            if parts.is_empty() {
                // Whole-layer frame: crop to the opaque box, pivot = anchor in frame coordinates.
                let (x, y, w, h) = bbox(&img).unwrap_or((0, 0, 0, 0));
                let a = anchor.unwrap_or((0, 0));
                println!("  f{fr}: {:?}", RasterFrame { x, y, w, h, pivot: (a.0 - x as i32, a.1 - y as i32), ms });
            } else {
                for part in &parts {
                    // Part frame: the layer's pixels inside the part's slice rect at this frame; pivot from the slice.
                    let k = slice_at(part, fr).unwrap();
                    let opaque = (0..k.size.1).flat_map(|dy| (0..k.size.0).map(move |dx| (dx, dy)))
                        .filter(|&(dx, dy)| img.get_pixel(k.origin.0 as u32 + dx, k.origin.1 as u32 + dy)[3] > 0).count();
                    println!("  f{fr} {part}: {:?} opaque={opaque}", RasterFrame { x: k.origin.0 as u32, y: k.origin.1 as u32,
                        w: k.size.0, h: k.size.1, pivot: k.pivot.unwrap_or((0, 0)), ms });
                }
            }
        }
        // Pose positions: the slice key in effect on the tag's first frame. Different from the origin -> a pose entry.
        for part in &parts {
            let (k0, k) = (slice_at(part, 0).unwrap(), slice_at(part, t.from_frame()).unwrap());
            let pos = |k: &asefile::SliceKey| { let p = k.pivot.unwrap_or((0, 0)); (k.origin.0 + p.0, k.origin.1 + p.1) };
            if pos(k) != pos(k0) { println!("  poses.{key}.{part} = {:?} (origin {:?})", pos(k), pos(k0)); }
        }
    }
}

// ---- Q5: grid sheets --------------------------------------------------------------------------------------
struct Grid { frame: (i64, i64), origin: (i64, i64), gap: (i64, i64), columns: i64 }

fn grid_rects(g: &Grid, sheet: (i64, i64), frames: &[i64], ms: &[i64]) -> Result<Vec<[i64; 5]>, String> {
    if g.frame.0 <= 0 || g.frame.1 <= 0 { return Err("non-positive frame size".into()); }
    if g.columns <= 0 { return Err("non-positive columns".into()); }
    if g.gap.0 < 0 || g.gap.1 < 0 || g.origin.0 < 0 || g.origin.1 < 0 { return Err("negative origin or gap".into()); }
    if ms.len() != 1 && ms.len() != frames.len() { return Err(format!("ms list has {} entries for {} frames", ms.len(), frames.len())); }
    frames.iter().enumerate().map(|(n, &i)| {
        if i < 0 { return Err(format!("frame index {i} is negative")); }
        let x = g.origin.0 + (i % g.columns) * (g.frame.0 + g.gap.0);
        let y = g.origin.1 + (i / g.columns) * (g.frame.1 + g.gap.1);
        if x + g.frame.0 > sheet.0 || y + g.frame.1 > sheet.1 { return Err(format!("cell {i} ({x},{y}) is outside the {}x{} sheet", sheet.0, sheet.1)); }
        Ok([x, y, g.frame.0, g.frame.1, if ms.len() == 1 { ms[0] } else { ms[n] }])
    }).collect()
}

fn grid(out: &Path) {
    std::fs::create_dir_all(out).unwrap();
    // A 3-row sheet: 5 columns of 24x20 cells, origin (4,2), gap (2,3). Each cell is filled with a colour that
    // encodes its index, with a 1-px magenta border in the gap so an off-by-one shows.
    let g = Grid { frame: (24, 20), origin: (4, 2), gap: (2, 3), columns: 5 };
    let (cols, rows) = (5, 3);
    let w = (g.origin.0 + cols * (g.frame.0 + g.gap.0)) as u32;
    let h = (g.origin.1 + rows * (g.frame.1 + g.gap.1)) as u32;
    let mut sheet = RgbaImage::from_pixel(w, h, image::Rgba([255, 0, 255, 255]));
    for i in 0..cols * rows {
        let x0 = g.origin.0 + (i % cols) * (g.frame.0 + g.gap.0);
        let y0 = g.origin.1 + (i / cols) * (g.frame.1 + g.gap.1);
        for y in 0..g.frame.1 { for x in 0..g.frame.0 {
            sheet.put_pixel((x0 + x) as u32, (y0 + y) as u32, image::Rgba([i as u8 * 10, 100, 200, 255]));
        } }
    }
    sheet.save(out.join("grid-sheet.png")).unwrap();
    println!("sheet {w}x{h}");
    let frames: Vec<i64> = (0..cols * rows).collect();
    let rects = grid_rects(&g, (w as i64, h as i64), &frames, &[160]).unwrap();
    for (i, r) in rects.iter().enumerate() {
        let want = image::Rgba([i as u8 * 10, 100, 200, 255]);
        let inside = (0..r[3]).all(|y| (0..r[2]).all(|x| *sheet.get_pixel((r[0] + x) as u32, (r[1] + y) as u32) == want));
        // The pixel just outside each edge must be the gap colour (or the sheet edge).
        let edge = |x: i64, y: i64| x < 0 || y < 0 || x >= w as i64 || y >= h as i64 || sheet.get_pixel(x as u32, y as u32)[0] == 255;
        let border = edge(r[0] - 1, r[1]) && edge(r[0] + r[2], r[1]) && edge(r[0], r[1] - 1) && edge(r[0], r[1] + r[3]);
        assert!(inside && border, "cell {i}: {r:?}");
    }
    println!("all {} rects match their cells exactly; first {:?}, last {:?}", rects.len(), rects[0], rects[rects.len() - 1]);
    println!("frames [0,1,2,1] ms [100,200,300,400]: {:?}", grid_rects(&g, (w as i64, h as i64), &[0, 1, 2, 1], &[100, 200, 300, 400]).unwrap());
    for (what, r) in [
        ("cell outside", grid_rects(&g, (w as i64, h as i64), &[15], &[160])),
        ("zero frame", grid_rects(&Grid { frame: (0, 20), ..g }, (w as i64, h as i64), &[0], &[160])),
        ("ms mismatch", grid_rects(&g, (w as i64, h as i64), &[0, 1, 2], &[1, 2])),
        ("negative index", grid_rects(&g, (w as i64, h as i64), &[-1], &[160])),
    ] { println!("{what}: {:?}", r.unwrap_err()); }
}

// ---- Q6: performance --------------------------------------------------------------------------------------
/// A pet-sized file: canvas w x h, `layers` layers, `frames` frames, every cel a zlib RGBA image of
/// noisy-ish pixel art covering about half the canvas.
fn typical(out: &Path, w: u16, h: u16, layers: u16, frames: u16) {
    let mut seed = 1u32;
    let mut rnd = || { seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5; seed };
    let pal = [GREEN, DARK, WHITE, BLACK, [240, 160, 60, 255], [200, 80, 80, 255]];
    let (cw, ch) = (w * 3 / 4, h * 2 / 3);
    let sp = Sprite {
        width: w, height: h, depth: Depth::Rgba, palette: vec![], layer_uuids: false,
        layers: (0..layers).map(|i| Layer { name: format!("layer{i}"), group: false, child_level: 0, visible: true }).collect(),
        frames: (0..frames).map(|_| Frame { ms: 100, cels: (0..layers).map(|l| {
            // Runs of one colour, like pixel art, rather than pure noise.
            let mut px = Vec::with_capacity(cw as usize * ch as usize * 4);
            let mut c = CLEAR; let mut run = 0;
            for _ in 0..cw as usize * ch as usize {
                if run == 0 { run = rnd() % 12 + 1; c = if rnd() % 3 == 0 { CLEAR } else { pal[(rnd() % 6) as usize] }; }
                run -= 1; px.extend_from_slice(&c);
            }
            Cel { layer: l, x: (rnd() % 8) as i16, y: (rnd() % 8) as i16, w: cw, h: ch, data: CelData::Zlib(px) }
        }).collect() }).collect(),
        tags: vec![Tag { name: "layer0:idle".into(), from: 0, to: frames - 1, direction: 0, repeat: 0 }],
        slices: vec![],
    };
    let bytes = writer::write(&sp);
    std::fs::write(out, &bytes).unwrap();
    println!("wrote {} ({} bytes, {w}x{h}, {layers} layers, {frames} frames)", out.display(), bytes.len());
}

fn bench(path: &Path, iters: u32) {
    let bytes = std::fs::read(path).unwrap();
    let base = reset_peak();
    let mut parse_t = vec![]; let mut decode_t = vec![];
    let mut parse_peak = 0; let mut retained = 0; let mut decode_peak = 0; let mut kept = 0usize;
    for _ in 0..iters {
        let b0 = reset_peak();
        let t = Instant::now();
        let f = AsepriteFile::read(&bytes[..]).unwrap();
        parse_t.push(t.elapsed().as_secs_f64() * 1000.0);
        parse_peak = PEAK.load(Relaxed) - b0;
        retained = CUR.load(Relaxed) - b0;
        // Decode every non-empty layer cel, as the loader would, and keep it cropped to its opaque box
        // (asefile's Cel::image is always canvas-sized).
        let t = Instant::now();
        let mut frames: Vec<RgbaImage> = Vec::new();
        for l in f.layers() {
            if l.layer_type() != LayerType::Image { continue; }
            for fr in 0..f.num_frames() {
                if l.frame(fr).is_empty() { continue; }
                let img = l.frame(fr).image();
                if let Some((x, y, w, h)) = bbox(&img) { frames.push(image::imageops::crop_imm(&img, x, y, w, h).to_image()); }
            }
        }
        decode_t.push(t.elapsed().as_secs_f64() * 1000.0);
        decode_peak = PEAK.load(Relaxed) - b0;
        kept = frames.iter().map(|i| i.as_raw().len()).sum();
        drop(frames); drop(f);
    }
    let med = |v: &mut Vec<f64>| { v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[v.len() / 2] };
    let f = AsepriteFile::read(&bytes[..]).unwrap();
    println!("{}: {} bytes, {}x{}, {} layers, {} frames, {} iters", path.display(), bytes.len(), f.width(), f.height(),
        f.num_layers(), f.num_frames(), iters);
    println!("  parse (AsepriteFile::read): median {:.2} ms, min {:.2} ms", med(&mut parse_t.clone()), parse_t.iter().cloned().fold(f64::MAX, f64::min));
    println!("  decode all layer cels + crop: median {:.2} ms", med(&mut decode_t));
    println!("  heap: parse peak {}, retained after parse {}, peak incl. decode {}, cropped frames kept {}",
        mb(parse_peak), mb(retained), mb(decode_peak), mb(kept));
    let _ = base;
}

// ---- hostile inputs: what does asefile do before any limit check? --------------------------------------
fn hostile(out: &Path) {
    std::fs::create_dir_all(out).unwrap();
    // 1. A zlib bomb: a 16x16 cel whose data inflates to 256 MB.
    let mut sp = fixture_sprite(Depth::Rgba);
    let zeros = vec![0u8; 256 << 20];
    sp.frames[0].cels[0] = Cel { layer: 0, x: 0, y: 0, w: 16, h: 16, data: CelData::Raw(writer::zlib(&zeros)) };
    // Raw(..) with pre-compressed bytes plus a type patch below: write as type 2.
    let mut bytes = writer::write(&sp);
    patch_first_cel_type(&mut bytes, 2);
    std::fs::write(out.join("bomb.aseprite"), &bytes).unwrap();
    println!("bomb.aseprite: {} bytes", bytes.len());
    // 2. A 65535x65535 compressed cel with a few bytes of data (asks for a 16 GiB buffer up front).
    let mut sp = fixture_sprite(Depth::Rgba);
    sp.frames[0].cels[0] = Cel { layer: 0, x: 0, y: 0, w: 65535, h: 65535, data: CelData::Raw(writer::zlib(&[0; 64])) };
    let mut bytes = writer::write(&sp);
    patch_first_cel_type(&mut bytes, 2);
    std::fs::write(out.join("hugecel.aseprite"), &bytes).unwrap();
    println!("hugecel.aseprite: {} bytes", bytes.len());
    // 3. A header of 65535x65535 with tiny cels (Cel::image allocates canvas-sized images).
    let mut sp = fixture_sprite(Depth::Rgba); sp.width = 65535; sp.height = 65535;
    std::fs::write(out.join("hugecanvas.aseprite"), writer::write(&sp)).unwrap();
    // 4. An unknown chunk type in frame 0 (a future Aseprite chunk).
    let mut bytes = writer::write(&fixture_sprite(Depth::Rgba));
    let at = 128 + 16; // first chunk of frame 0
    bytes[at + 4..at + 6].copy_from_slice(&0x2099u16.to_le_bytes());
    std::fs::write(out.join("unknownchunk.aseprite"), &bytes).unwrap();
    for n in ["bomb", "hugecel", "hugecanvas", "unknownchunk"] {
        let p = out.join(format!("{n}.aseprite"));
        let b0 = reset_peak();
        let t = Instant::now();
        let r = AsepriteFile::read_file(&p);
        println!("{n}: {:?} in {:.0} ms, heap peak {}", r.as_ref().map(|_| "Ok").map_err(|e| e.to_string()),
            t.elapsed().as_secs_f64() * 1000.0, mb(PEAK.load(Relaxed) - b0));
    }
}

/// The first cel chunk's type field: find chunk 0x2005 in frame 0 and set its cel type.
fn patch_first_cel_type(bytes: &mut [u8], ty: u16) {
    let mut at = 128 + 16;
    loop {
        let size = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let kind = u16::from_le_bytes(bytes[at + 4..at + 6].try_into().unwrap());
        if kind == 0x2005 { bytes[at + 6 + 7..at + 6 + 9].copy_from_slice(&ty.to_le_bytes()); return; }
        at += size;
    }
}
