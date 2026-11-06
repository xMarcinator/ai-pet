//! Rive spike (fn-2 task 13). Subcommands:
//!   info <file.riv>                         artboards, state machines, inputs, assets, view models
//!   render <file.riv> <out-dir> [steps] [dt-ms] [artboard] [state-machine]   PNGs over fixed steps
//!   inputs <file.riv> <out-dir>             each input changed against a control run
//!   bench <file.riv> [w] [h]                timings, idle cost, hit runs, memory, paced CPU at 30/60 fps
//!   load <file.riv>                         parse, instantiate and draw a frame (run as the child below)
//!   fuzz <file.riv> <out-dir>               truncated and corrupted copies, each loaded in a child process

mod inspect;
mod render;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use rive_rs::scene::Scene as _;
use rive_rs::{Handle, Instantiate, Viewport};

use render::Renderer;

type File = rive_rs::File<Renderer>;
type Artboard = rive_rs::Artboard<Renderer>;
type StateMachine = rive_rs::StateMachine<Renderer>;

const W: u32 = 260;
const H: u32 = 240;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let arg = |i: usize| args.get(i).map(String::as_str);
    match arg(1) {
        Some("info") => info(Path::new(arg(2).expect("file"))),
        Some("render") => render_steps(
            Path::new(arg(2).expect("file")),
            Path::new(arg(3).expect("out dir")),
            arg(4).map_or(8, |s| s.parse().unwrap()),
            arg(5).map_or(250.0, |s| s.parse().unwrap()),
            arg(6),
            arg(7),
        ),
        Some("render-scene") => render_scene(
            Path::new(arg(2).expect("file")),
            Path::new(arg(3).expect("out dir")),
            arg(4).map_or(8, |s| s.parse().unwrap()),
            arg(5).map_or(250.0, |s| s.parse().unwrap()),
        ),
        Some("inputs") => inputs(Path::new(arg(2).expect("file")), Path::new(arg(3).expect("out dir"))),
        Some("bench") => bench(
            Path::new(arg(2).expect("file")),
            arg(3).map_or(W, |s| s.parse().unwrap()),
            arg(4).map_or(H, |s| s.parse().unwrap()),
        ),
        Some("load") => return load(Path::new(arg(2).expect("file"))),
        Some("fuzz") => fuzz(Path::new(arg(2).expect("file")), Path::new(arg(3).expect("out dir"))),
        _ => {
            eprintln!("usage: see src/main.rs");
            return ExitCode::from(64);
        }
    }
    ExitCode::SUCCESS
}

fn handle(name: Option<&str>) -> Handle {
    match name {
        Some(n) if n != "-" => Handle::Name(n.to_string().into()),
        _ => Handle::Default,
    }
}

struct Pet {
    _file: File,
    sm: StateMachine,
    viewport: Viewport,
    r: Renderer,
}

impl Pet {
    fn open(bytes: &[u8], w: u32, h: u32, artboard: Option<&str>, sm: Option<&str>) -> Pet {
        let file = File::new(bytes).expect("File::new");
        let ab = Artboard::instantiate(&file, handle(artboard)).expect("artboard");
        let sm = StateMachine::instantiate(&ab, handle(sm)).expect("state machine");
        let mut viewport = Viewport::default();
        viewport.resize(w, h);
        Pet { _file: file, sm, viewport, r: Renderer::new(w, h) }
    }

    /// Advances and draws if anything changed. Returns whether a new frame was drawn.
    fn step(&mut self, dt: Duration) -> bool {
        self.r.begin_frame();
        self.sm.advance_and_maybe_draw(&mut self.r, dt, &mut self.viewport)
    }
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn input_list(sm: &StateMachine) -> Vec<(String, &'static str, String)> {
    // rive-rs doesn't export its `Input` enum (state_machine::inputs is private), so the kind and name come from
    // its Debug output, e.g. `Number(Number { name: "rating" })`, and the value from the typed getters.
    sm.inputs()
        .map(|i| {
            let dbg = format!("{i:?}");
            let name = dbg.split('"').nth(1).unwrap_or_default().to_string();
            if dbg.starts_with("Bool") {
                let v = sm.get_bool(&name).map(|b| b.get().to_string()).unwrap_or_default();
                (name, "bool", v)
            } else if dbg.starts_with("Number") {
                let v = sm.get_number(&name).map(|n| n.get().to_string()).unwrap_or_default();
                (name, "number", v)
            } else {
                (name, "trigger", String::new())
            }
        })
        .collect()
}

fn info(path: &Path) {
    let bytes = read(path);
    println!("file: {} ({} bytes)", path.display(), bytes.len());
    let report = inspect::inspect(&bytes);
    match report.status {
        0 => {}
        s => println!("inspect: import failed (rive::ImportResult {s})"),
    }
    for (name, sms, anims) in &report.artboards {
        println!("artboard {name:?}: state machines {sms:?}, animations {anims:?}");
    }
    for (name, in_band, cdn) in &report.assets {
        let kind = if *in_band > 0 { format!("embedded, {in_band} bytes") } else { "REFERENCED (not embedded)".into() };
        println!("asset {name}: {kind}{}", if *cdn { ", has a CDN uuid" } else { "" });
    }
    println!("view models: {}", report.view_models);
    let Ok(file) = File::new(&bytes) else { return };
    let Some(ab) = Artboard::instantiate(&file, Handle::Default) else { return };
    let Some(sm) = StateMachine::instantiate(&ab, Handle::Default) else {
        println!("default artboard has no state machine");
        return;
    };
    // Scene::name() isn't called: on a StateMachine it segfaults (rive_rs_component_name casts the scene, which isn't
    // a Component). The names come from the inspection shim above.
    println!("default artboard {}x{}; default state machine's inputs:", sm.width(), sm.height());
    for (name, kind, value) in input_list(&sm) {
        println!("  input {kind} {name:?} = {value}");
    }
}

fn save(r: &Renderer, path: &Path) {
    r.pixmap.save_png(path).unwrap();
}

fn render_steps(path: &Path, out: &Path, steps: u32, dt_ms: f32, artboard: Option<&str>, sm: Option<&str>) {
    std::fs::create_dir_all(out).unwrap();
    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
    let mut pet = Pet::open(&read(path), W, H, artboard, sm);
    let dt = Duration::from_secs_f32(dt_ms / 1000.0);
    // the first step draws the initial pose
    pet.step(Duration::ZERO);
    for i in 0..steps {
        let drew = pet.step(dt);
        let file = out.join(format!("{stem}-{i:02}.png"));
        save(&pet.r, &file);
        println!("{} t={:.0} ms drew={drew} {:?}", file.display(), (i + 1) as f32 * dt_ms, pet.r.stats);
    }
}

/// Like render, but plays the default scene: the state machine, or else the first linear animation (to reach files
/// without a state machine, e.g. the ones with image meshes).
fn render_scene(path: &Path, out: &Path, steps: u32, dt_ms: f32) {
    std::fs::create_dir_all(out).unwrap();
    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
    let file = File::new(&read(path)).expect("File::new");
    let ab = Artboard::instantiate(&file, Handle::Default).expect("artboard");
    let mut scene = <Box<dyn rive_rs::scene::Scene<Renderer>>>::instantiate(&ab, Handle::Default).expect("scene");
    let mut vp = Viewport::default();
    vp.resize(W, H);
    let mut r = Renderer::new(W, H);
    let dt = Duration::from_secs_f32(dt_ms / 1000.0);
    for i in 0..steps {
        r.begin_frame();
        let drew = scene.advance_and_maybe_draw(&mut r, if i == 0 { Duration::ZERO } else { dt }, &mut vp);
        let file = out.join(format!("{stem}-{i:02}.png"));
        save(&r, &file);
        println!("{} drew={drew} {:?}", file.display(), r.stats);
    }
}

fn changed_pixels(a: &Renderer, b: &Renderer) -> usize {
    a.pixmap.data().chunks_exact(4).zip(b.pixmap.data().chunks_exact(4)).filter(|(x, y)| x != y).count()
}

fn inputs(path: &Path, out: &Path) {
    std::fs::create_dir_all(out).unwrap();
    let bytes = read(path);
    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
    let dt = Duration::from_secs_f32(1.0 / 60.0);
    let names = input_list(&Pet::open(&bytes, W, H, None, None).sm);
    for (name, kind, initial) in names {
        let mut control = Pet::open(&bytes, W, H, None, None);
        let mut changed = Pet::open(&bytes, W, H, None, None);
        // let both settle for a second, then change the input on one of them
        for _ in 0..60 {
            control.step(dt);
            changed.step(dt);
        }
        let what = match kind {
            "bool" => {
                let mut b = changed.sm.get_bool(&name).unwrap();
                b.set(!b.get());
                format!("set to {}", b.get())
            }
            "number" => {
                let mut n = changed.sm.get_number(&name).unwrap();
                let v = n.get() + 1.0;
                n.set(v);
                format!("set to {v}")
            }
            _ => {
                changed.sm.get_trigger(&name).unwrap().fire();
                "fired".to_string()
            }
        };
        let (mut max_diff, mut at) = (0, 0);
        for f in 1..=90 {
            control.step(dt);
            changed.step(dt);
            let d = changed_pixels(&control.r, &changed.r);
            if d > max_diff {
                max_diff = d;
                at = f;
                save(&control.r, &out.join(format!("{stem}-{name}-control.png")));
                save(&changed.r, &out.join(format!("{stem}-{name}-changed.png")));
            }
        }
        println!(
            "{kind} {name:?} (was {initial}) {what}: up to {max_diff} of {} pixels differ from the control, \
             most at frame {at} after the change",
            W * H
        );
    }
}

fn mean_max(samples: &[f64]) -> (f64, f64, f64) {
    let mean = samples.iter().sum::<f64>() / samples.len().max(1) as f64;
    let max = samples.iter().cloned().fold(0.0, f64::max);
    let mut s = samples.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p95 = s.get(s.len() * 95 / 100).copied().unwrap_or(0.0);
    (mean, p95, max)
}

fn proc_status(key: &str) -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with(key)).map(|l| l[key.len()..].trim().to_string()))
        .unwrap_or_else(|| "n/a".into())
}

/// CPU time this (single-threaded) process has run, from the scheduler (nanoseconds).
fn cpu_ns() -> u64 {
    std::fs::read_to_string("/proc/self/schedstat")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse().ok()))
        .unwrap_or(0)
}

/// utime + stime of this process, in clock ticks (100 per second on Linux).
fn proc_ticks() -> u64 {
    let s = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let rest = s.rsplit(')').next().unwrap_or("");
    let f: Vec<&str> = rest.split_whitespace().collect();
    // after the command name: state is field 3, utime 14 and stime 15 (1-based in proc(5))
    f.get(11).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) + f.get(12).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0)
}

fn bench(path: &Path, w: u32, h: u32) {
    let bytes = read(path);
    println!("bench {} at {w}x{h}, release={}", path.display(), !cfg!(debug_assertions));
    println!("baseline VmHWM {} VmRSS {}", proc_status("VmHWM:"), proc_status("VmRSS:"));
    let t = Instant::now();
    let mut pet = Pet::open(&bytes, w, h, None, None);
    println!("load+instantiate {:.2} ms", t.elapsed().as_secs_f64() * 1e3);
    pet.step(Duration::ZERO);
    let dt = Duration::from_secs_f32(1.0 / 60.0);

    // advance + draw, 300 frames at 60 fps
    let (mut times, mut drawn, mut runs, mut run_times) = (Vec::new(), 0, Vec::new(), Vec::new());
    let mut last_stats = pet.r.stats;
    for _ in 0..300 {
        let t = Instant::now();
        let drew = pet.step(dt);
        times.push(t.elapsed().as_secs_f64() * 1e3);
        if drew {
            drawn += 1;
            last_stats = pet.r.stats;
            let t = Instant::now();
            render::runs(&pet.r.pixmap, &mut runs);
            run_times.push(t.elapsed().as_secs_f64() * 1e3);
        }
    }
    let (mean, p95, max) = mean_max(&times);
    println!("advance+draw: {drawn}/300 frames drawn; mean {mean:.3} ms, p95 {p95:.3} ms, max {max:.3} ms");
    println!("  last frame drew {last_stats:?}");
    let (rmean, _, rmax) = mean_max(&run_times);
    println!("hit runs from alpha: {} runs; mean {:.4} ms, max {:.4} ms", runs.len(), rmean, rmax);

    // advance only (no draw), on a second instance
    let mut adv = Pet::open(&bytes, w, h, None, None);
    let mut at = Vec::new();
    for _ in 0..300 {
        let t = Instant::now();
        adv.sm.advance_and_apply(dt);
        at.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let (amean, _, amax) = mean_max(&at);
    println!("advance only: mean {amean:.3} ms, max {amax:.3} ms");

    // idle: run until advancing reports no change (up to 30 s of animation), then time idle ticks
    let mut settled = None;
    for f in 0..1800 {
        if !pet.step(dt) {
            settled = Some(f);
            break;
        }
    }
    match settled {
        Some(f) => {
            let mut it = Vec::new();
            let mut redraws = 0;
            for _ in 0..300 {
                let t = Instant::now();
                if pet.step(dt) {
                    redraws += 1;
                }
                it.push(t.elapsed().as_secs_f64() * 1e3);
            }
            let (imean, _, imax) = mean_max(&it);
            println!(
                "idle: settled after {f} more frames; 300 idle ticks: {redraws} redraws, mean {:.2} us, max {:.2} us",
                imean * 1e3,
                imax * 1e3
            );
        }
        None => println!("idle: never settled within 30 s (the state machine keeps animating)"),
    }
    println!("after bench VmHWM {} VmRSS {}", proc_status("VmHWM:"), proc_status("VmRSS:"));

    // paced: 10 s at each rate, sleeping between frames, measuring this process's CPU time
    for fps in [30u32, 60] {
        let mut pet = Pet::open(&bytes, w, h, None, None);
        let frame = Duration::from_secs_f64(1.0 / fps as f64);
        let (cpu0, start) = (cpu_ns(), Instant::now());
        let mut next = start;
        let mut last = start;
        let (mut drawn, mut steps) = (0, Vec::new());
        let ticks0 = proc_ticks();
        while start.elapsed() < Duration::from_secs(10) {
            let now = Instant::now();
            if pet.step(now - last) {
                drawn += 1;
                render::runs(&pet.r.pixmap, &mut runs);
            }
            steps.push(now.elapsed().as_secs_f64() * 1e3);
            last = now;
            next += frame;
            if let Some(d) = next.checked_duration_since(Instant::now()) {
                std::thread::sleep(d);
            }
        }
        let wall = start.elapsed().as_secs_f64();
        let cpu = (cpu_ns() - cpu0) as f64 / 1e9;
        let ticks = (proc_ticks() - ticks0) as f64 / 100.0;
        let (smean, _, smax) = mean_max(&steps);
        println!(
            "paced {fps} fps for {wall:.1} s: {drawn} frames drawn, step mean {smean:.3} ms max {smax:.3} ms; CPU \
             {:.2} % of one core (schedstat), {:.1} % (utime+stime); from the step mean {:.2} %, from the unpaced \
             mean {:.2} %",
            cpu / wall * 100.0,
            ticks / wall * 100.0,
            smean * fps as f64 / 10.0,
            mean * fps as f64 / 10.0
        );
    }
}

/// The child: parse, instantiate the defaults and draw a few frames. Exit codes: 0 loaded, 20 malformed,
/// 21 unsupported version, 22 no artboard or state machine (not 3: abort() exits with 3 on Windows).
fn load(path: &Path) -> ExitCode {
    let bytes = read(path);
    let file = match File::new(&bytes) {
        Ok(f) => f,
        Err(rive_rs::Error::Malformed) => return ExitCode::from(20),
        Err(rive_rs::Error::UnsupportedVersion) => return ExitCode::from(21),
    };
    // SPIKE_PARSE_ONLY=1: stop after File::new, to tell parse crashes from crashes while playing
    if std::env::var_os("SPIKE_PARSE_ONLY").is_some() {
        return ExitCode::SUCCESS;
    }
    let Some(ab) = Artboard::instantiate(&file, Handle::Default) else { return ExitCode::from(22) };
    let Some(mut sm) = StateMachine::instantiate(&ab, Handle::Default) else { return ExitCode::from(22) };
    let mut r = Renderer::new(W, H);
    let mut vp = Viewport::default();
    vp.resize(W, H);
    for _ in 0..30 {
        r.begin_frame();
        sm.advance_and_maybe_draw(&mut r, Duration::from_millis(33), &mut vp);
    }
    ExitCode::SUCCESS
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn fuzz(path: &Path, out: &Path) {
    std::fs::create_dir_all(out).unwrap();
    let bytes = read(path);
    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
    let n = bytes.len();
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    for cut in [0, 1, 4, 5, 8, 12, 16, 32, 64] {
        cases.push((format!("truncated to {cut} bytes"), bytes[..cut.min(n)].to_vec()));
    }
    for pct in [10, 25, 50, 75, 90, 99] {
        cases.push((format!("truncated to {pct} %"), bytes[..n * pct / 100].to_vec()));
    }
    for cut in (1..40).map(|i| n * i / 40) {
        cases.push((format!("truncated to {cut} bytes"), bytes[..cut].to_vec()));
    }
    let mut v = bytes.clone();
    v[4] = v[4].wrapping_add(1);
    cases.push(("major version + 1".into(), v));
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for flips in [1usize, 4, 16, 64, 256] {
        for k in 0..20 {
            let mut v = bytes.clone();
            for _ in 0..flips {
                // keep the header so the body gets parsed
                let at = 16 + (rng.next() as usize) % (n - 16);
                v[at] ^= 1 << (rng.next() % 8);
            }
            cases.push((format!("{flips} bit flips #{k}"), v));
        }
    }
    for k in 0..20 {
        let mut v = bytes.clone();
        for b in v.iter_mut().skip(16) {
            if rng.next() % 4 == 0 {
                *b = rng.next() as u8;
            }
        }
        cases.push((format!("25 % random bytes #{k}"), v));
    }

    let exe = std::env::current_exe().unwrap();
    let (mut ok, mut malformed, mut unsupported, mut no_scene, mut crashed, mut timeout, mut other) =
        (0, 0, 0, 0, 0, 0, 0);
    let mut crashes = Vec::new();
    for (i, (what, data)) in cases.iter().enumerate() {
        let file: PathBuf = out.join(format!("{stem}-{i:03}.riv"));
        std::fs::write(&file, data).unwrap();
        let mut child = Command::new(&exe)
            .arg("load")
            .arg(&file)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let start = Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break Some(s);
            }
            if start.elapsed() > Duration::from_secs(5) {
                child.kill().ok();
                child.wait().ok();
                break None;
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        let ms = start.elapsed().as_secs_f64() * 1e3;
        match status {
            None => {
                timeout += 1;
                crashes.push(format!("{what}: timeout"));
            }
            Some(s) => match (s.code(), crash(&s)) {
                (Some(0), _) => ok += 1,
                (Some(20), _) => malformed += 1,
                (Some(21), _) => unsupported += 1,
                (Some(22), _) => no_scene += 1,
                (_, Some(how)) => {
                    crashed += 1;
                    crashes.push(format!("{what}: {how} after {ms:.0} ms"));
                }
                (code, _) => {
                    other += 1;
                    crashes.push(format!("{what}: exit {code:?} after {ms:.0} ms"));
                }
            },
        }
        if s_keep(&status) {
            std::fs::remove_file(&file).ok();
        }
    }
    println!(
        "{} cases: loaded {ok}, Malformed {malformed}, UnsupportedVersion {unsupported}, no artboard/state machine \
         {no_scene}, crashed {crashed}, timed out {timeout}, other exit {other}",
        cases.len()
    );
    for c in crashes {
        println!("  {c}");
    }
}

/// How a child crashed: a signal on Unix; on Windows an NTSTATUS exception code (0xC...) or abort()'s exit 3.
#[cfg(unix)]
fn crash(s: &std::process::ExitStatus) -> Option<String> {
    use std::os::unix::process::ExitStatusExt;
    s.signal().map(|sig| format!("signal {sig}"))
}

#[cfg(windows)]
fn crash(s: &std::process::ExitStatus) -> Option<String> {
    match s.code() {
        Some(c) if (c as u32) >= 0xC000_0000 => Some(format!("exception {:#x}", c as u32)),
        Some(3) => Some("abort".into()),
        _ => None,
    }
}

/// Keep only the copies that crashed or hung, for the record.
fn s_keep(status: &Option<std::process::ExitStatus>) -> bool {
    matches!(status, Some(s) if s.code().is_some_and(|c| [0, 20, 21, 22].contains(&c)))
}
