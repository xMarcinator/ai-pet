//! The pixel pet: a port of `src/AiPet.Core/Pet.cs`.
//!
//! Numbers follow the C# exactly: `Math.Round` is banker's rounding (`round_ties_even`), `(int)x` truncates toward
//! zero and saturates (`as i32`), `%` keeps the dividend's sign (as Rust's does), and every expression keeps the C#
//! operand order, so the doubles come out bit for bit the same.

use std::collections::BTreeSet;
use std::collections::hash_map::RandomState;
use std::f64::consts::PI;
use std::hash::{BuildHasher, Hasher};

use crate::avatar::Avatar;

/// A cell on the pixel grid: (x, y).
pub type Cell = (i32, i32);

/// Grid width, height, and the size of a grid pixel in DIPs.
pub const GW: i32 = 26;
pub const GH: i32 = 24;
pub const P: i32 = 5;
/// Pixels in a layer: GW × GH, row-major.
pub const PIXELS: usize = (GW * GH) as usize;

const PF: f64 = P as f64;

/// What the UI tells the pet each frame.
#[derive(Clone, Debug, PartialEq)]
pub struct PetInput {
    /// Mood from the chats: sleep, idle, thinking, working, attention, done.
    pub state: String,
    /// Seconds (on the pet clock) the state has held: the pet clock's time when the state began (the pet uses
    /// `t - state_since`).
    pub state_since: f64,
    /// "laptop" or "lens" while working.
    pub prop: Option<String>,
    pub hover: bool,
    /// Mouse position over the sprite in DIPs (0..GW*P, 0..GH*P), or None.
    pub mouse: Option<(f64, f64)>,
    pub dragging: bool,
    pub moved: bool,
    pub last_move: f64,
    pub facing: i32,
    pub poke_until: f64,
    pub land_until: f64,
    pub wave_until: f64,
    pub alert_until: f64,
    pub music: bool,
}

impl Default for PetInput {
    fn default() -> Self {
        PetInput {
            state: "sleep".to_owned(),
            state_since: 0.0,
            prop: None,
            hover: false,
            mouse: None,
            dragging: false,
            moved: false,
            last_move: 0.0,
            facing: 1,
            poke_until: 0.0,
            land_until: 0.0,
            wave_until: 0.0,
            alert_until: 0.0,
            music: false,
        }
    }
}

/// Motion + the three pixel layers for one frame.
#[derive(Clone, Copy, Debug)]
pub struct PetFrame<'a> {
    /// Premultiplied 0xAARRGGBB, row-major GW × GH (BGRA in memory on little-endian).
    pub body: &'a [u32; PIXELS],
    pub glow: &'a [u32; PIXELS],
    pub fx: &'a [u32; PIXELS],
    pub pixels_changed: bool,
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub shadow_scale: f64,
    pub shadow_opacity: f64,
}

/// The pet's randomness: idle acts, glances and blink times (`System.Random` in the C#). Injectable, so a test can
/// pin it down or forbid it.
pub trait PetRng {
    /// A double in [0, 1) (`Random.NextDouble`).
    fn next_double(&mut self) -> f64;
    /// An integer in [0, max) for max > 0 (`Random.Next(max)`).
    fn next_int(&mut self, max: i32) -> i32;
}

/// xorshift64*: small and fast; plenty for a pet's whims.
#[derive(Clone, Debug)]
pub struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    pub fn new(seed: u64) -> Self {
        XorShift64 {
            state: if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed },
        }
    }

    /// Seeded from the OS (through std's randomly keyed hasher).
    pub fn from_entropy() -> Self {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(0x5EED);
        XorShift64::new(h.finish())
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

impl PetRng for XorShift64 {
    fn next_double(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    fn next_int(&mut self, max: i32) -> i32 {
        (((self.next_u64() >> 32) * max.max(0) as u64) >> 32) as i32
    }
}

// ------------------------------------------------------------------ fixed colours
const LID: u32 = 0xFF2E3354;
const LID_HI: u32 = 0xFF3F477A;
const LID_IN: u32 = 0xFF14172E;
const BASE: u32 = 0xFFC9CFE2;
const BASE_SH: u32 = 0xFF8E95AE;
const RING: u32 = 0xFFD5DCEB;
const GLASS: u32 = 0x88A9E4FF;
const HANDLE: u32 = 0xFF7A5236;
const SPARK: u32 = 0xFFFFE27A;
const SPARK_HI: u32 = 0xFFFFFFFF;
const ZZ: u32 = 0xFFE3E8FF;
const DOT: u32 = 0xFFFFFFFF;
const BANG: u32 = 0xFFFFD24A;
const DUSTC: u32 = 0xB0E6DED3;
const HP_DARK: u32 = 0xFF26262C;
const HP_CUP: u32 = 0xFF3A3A43;

// ------------------------------------------------------------------ limb and effect cells
/// Xs(21, 8, 9, 10, 15, 16, 17)
const FEET: &[Cell] = &[(8, 21), (9, 21), (10, 21), (15, 21), (16, 21), (17, 21)];
/// Xs(21, 8, 9, 10) + Xs(20, 16, 17, 18)
const FOOT_TAP: &[Cell] = &[(8, 21), (9, 21), (10, 21), (16, 20), (17, 20), (18, 20)];
const DANGLE_A: &[Cell] = &[(9, 22), (10, 22), (15, 21), (16, 21)];
const DANGLE_B: &[Cell] = &[(9, 21), (10, 21), (15, 22), (16, 22)];
const ARMS_DOWN: &[Cell] = &[
    (4, 15),
    (5, 15),
    (4, 16),
    (5, 16),
    (20, 15),
    (21, 15),
    (20, 16),
    (21, 16),
];
const ARM_L: &[Cell] = ARMS_DOWN.split_at(4).0;
const WAVE_A: &[Cell] = &[(21, 10), (22, 10), (21, 11), (22, 11)];
const WAVE_B: &[Cell] = &[(22, 9), (23, 9), (22, 10), (23, 10)];
/// { (3, 10), (4, 10), (3, 11), (4, 11) } + WAVE_A
const ARMS_UP: &[Cell] = &[
    (3, 10),
    (4, 10),
    (3, 11),
    (4, 11),
    (21, 10),
    (22, 10),
    (21, 11),
    (22, 11),
];
const RUN_FEET: [&[Cell]; 2] = [
    &[(17, 21), (18, 21), (19, 21), (7, 20), (8, 20), (9, 20)],
    &[(16, 20), (17, 20), (18, 20), (6, 21), (7, 21), (8, 21)],
];
const RUN_ARMS: [&[Cell]; 2] = [
    &[
        (21, 12),
        (22, 12),
        (21, 13),
        (22, 13),
        (4, 16),
        (5, 16),
        (4, 17),
        (5, 17),
    ],
    &[
        (20, 16),
        (21, 16),
        (20, 17),
        (21, 17),
        (3, 13),
        (4, 13),
        (3, 14),
        (4, 14),
    ],
];
const DUST: &[Cell] = &[(4, 21), (2, 20), (0, 19)];
const ZGLYPH: &[Cell] = &[(0, 0), (1, 0), (2, 0), (1, 1), (0, 2), (1, 2), (2, 2)];
const BLUSH_CELLS: &[Cell] = &[(6, 13), (7, 13), (18, 13), (19, 13)];
const NOTE: &[Cell] = &[
    (2, 0),
    (3, 0),
    (3, 1),
    (2, 1),
    (2, 2),
    (2, 3),
    (0, 3),
    (1, 3),
    (0, 4),
    (1, 4),
];

/// The pixel pet: sprite geometry per avatar, the animation state machine, and the renderer.
/// Platform-neutral; the UI just blits the three layers (Glow gets a coloured blur on top).
pub struct Pet<R: PetRng = XorShift64> {
    // ------------------------------------------------------------------ avatar geometry
    avatar: Avatar,
    body: BTreeSet<Cell>,
    face: BTreeSet<Cell>,
    spec: Vec<Cell>,
    hp_band: Vec<Cell>,
    hp_cups: Vec<Cell>,
    hp_shine: Vec<Cell>,

    // ------------------------------------------------------------------ pixel buffers
    body_px: [u32; PIXELS],
    glow_px: [u32; PIXELS],
    fx_px: [u32; PIXELS],

    // ------------------------------------------------------------------ animation state
    rng: R,
    idle_act: Option<&'static str>,
    anim_state: String,
    idle_start: f64,
    idle_until: f64,
    next_idle: f64,
    blink_at: f64,
    look_at: f64,
    last_frame: f64,
    prev_y: f64,
    squash: f64,
    look: i32,
    last_draw_frame: i32,
}

struct Pose<'a> {
    eyes: &'static str,
    lx: i32,
    arms: Vec<Cell>,
    feet: Vec<Cell>,
    prop: Option<&'a str>,
    x: f64,
    y: f64,
    running: bool,
    held: bool,
    poked: bool,
    headphones: bool,
}

impl Default for Pose<'_> {
    fn default() -> Self {
        Pose {
            eyes: "normal",
            lx: 0,
            arms: ARMS_DOWN.to_vec(),
            feet: FEET.to_vec(),
            prop: None,
            x: 0.0,
            y: 0.0,
            running: false,
            held: false,
            poked: false,
            headphones: false,
        }
    }
}

impl Pet {
    /// A pet wearing Sprout, with an OS-seeded random source.
    pub fn new() -> Self {
        Pet::with_rng(XorShift64::from_entropy())
    }
}

impl Default for Pet {
    fn default() -> Self {
        Pet::new()
    }
}

impl<R: PetRng> Pet<R> {
    /// A pet wearing Sprout, with the given random source.
    pub fn with_rng(rng: R) -> Self {
        let mut pet = Pet {
            avatar: Avatar::sprout(),
            body: BTreeSet::new(),
            face: BTreeSet::new(),
            spec: Vec::new(),
            hp_band: Vec::new(),
            hp_cups: Vec::new(),
            hp_shine: Vec::new(),
            body_px: [0; PIXELS],
            glow_px: [0; PIXELS],
            fx_px: [0; PIXELS],
            rng,
            idle_act: None,
            anim_state: "sleep".to_owned(),
            idle_start: 0.0,
            idle_until: 0.0,
            next_idle: 6.0,
            blink_at: 3.0,
            look_at: 4.0,
            last_frame: 0.0,
            prev_y: 0.0,
            squash: 0.0,
            look: 0,
            last_draw_frame: -1,
        };
        pet.set_avatar(Avatar::sprout());
        pet
    }

    pub fn avatar(&self) -> &Avatar {
        &self.avatar
    }

    /// The state the pet is animating (e.g. "listen" when music replaces idling).
    pub fn anim_state(&self) -> &str {
        &self.anim_state
    }

    /// The idle act in progress ("look", "hop", "stretch", "wiggle" or "tap"), if any. It is kept, but not shown,
    /// while the pet is hovered or held. For tests: the golden data records the C#'s `idleAct` at every sample.
    pub fn idle_act(&self) -> Option<&'static str> {
        self.idle_act
    }

    pub fn set_avatar(&mut self, a: Avatar) {
        self.body = build_body(&a.shapes);
        self.face = build_face(a.face_top, a.face_bottom);
        self.spec = find_spec(&self.body);
        (self.hp_band, self.hp_cups, self.hp_shine) = build_headphones(&self.body);
        self.avatar = a;
        self.last_draw_frame = -1;
    }

    fn r(&mut self, a: f64, b: f64) -> f64 {
        a + self.rng.next_double() * (b - a)
    }

    fn compute_pose<'a>(&mut self, inp: &'a PetInput, t: f64) -> Pose<'a> {
        let mut p = Pose::default();
        let mut st: &'a str = if t < inp.alert_until { "attention" } else { &inp.state };
        p.headphones = inp.music;
        // with nothing to do and music on, the pet listens along instead of idling or napping
        if inp.music && (matches!(st, "idle" | "sleep") || (st == "done" && t - inp.state_since > 4.0)) {
            st = "listen";
        }
        if st != self.anim_state {
            self.idle_act = None;
            self.anim_state = st.to_owned();
        }
        let since = t - inp.state_since;
        let drg = inp.dragging && inp.moved;
        let running = drg && t - inp.last_move < 0.25;
        let landing = t < inp.land_until;
        let waving = t < inp.wave_until && !drg;
        p.poked = t < inp.poke_until;
        p.prop = if st == "working" {
            Some(inp.prop.as_deref().unwrap_or("laptop"))
        } else {
            None
        };

        if t > self.look_at {
            self.look = [-1, 0, 0, 1][self.rng.next_int(4) as usize];
            self.look_at = t + self.r(2.5, 6.0);
        }
        let wave = |rate: f64| -> Vec<Cell> {
            let mut arms = ARM_L.to_vec();
            arms.extend_from_slice(if (t * rate) as i32 % 2 == 1 { WAVE_A } else { WAVE_B });
            arms
        };

        match st {
            "idle" => {
                p.y = (0.5 + 0.5 * (t * 2.2).sin()) * 0.6 * PF;
                p.lx = self.look;
                if !inp.hover && !drg {
                    if self.idle_act.is_none() && t > self.next_idle {
                        let act = ["look", "hop", "stretch", "wiggle", "tap"][self.rng.next_int(5) as usize];
                        self.idle_act = Some(act);
                        self.idle_start = t;
                        self.idle_until = t + if act == "hop" { 0.9 } else { self.r(1.6, 2.6) };
                    }
                    if self.idle_act.is_some() && t > self.idle_until {
                        self.idle_act = None;
                        self.next_idle = t + self.r(5.0, 11.0);
                    }
                    match self.idle_act {
                        Some("look") => p.lx = [-2, -1, 0, 1, 2, 1, 0, -1][((t * 4.0) as i32 % 8) as usize],
                        Some("hop") => {
                            p.y = -(PI * (t - self.idle_start) / (self.idle_until - self.idle_start)).sin() * 3.2 * PF
                        }
                        Some("stretch") => {
                            p.arms = ARMS_UP.to_vec();
                            p.eyes = "happy";
                            p.y = -PF;
                        }
                        Some("wiggle") => p.x = (t * 16.0).sin() * 0.8 * PF,
                        Some("tap") => {
                            p.feet = if (t * 5.0) as i32 % 2 == 1 { FOOT_TAP } else { FEET }.to_vec();
                            p.lx = 1;
                        }
                        _ => {}
                    }
                }
            }
            "listen" => {
                // nod to a ~112 bpm beat, sway every other bar, eyes mostly closed in bliss
                const BEAT: f64 = 60.0 / 112.0;
                p.y = -(PI * t / BEAT).sin().abs() * 1.2 * PF;
                p.x = (PI * t / (BEAT * 2.0)).sin() * 0.6 * PF;
                p.eyes = if (t / BEAT) as i32 % 8 < 6 { "happy" } else { "normal" };
            }
            "sleep" => {
                p.y = (0.5 + 0.5 * (t * 1.2).sin()) * 0.5 * PF;
                p.eyes = "sleep";
            }
            "thinking" => {
                p.y = -(0.5 + 0.5 * (t * 2.4).sin()) * 0.7 * PF;
                p.eyes = "up";
                p.lx = 1;
            }
            "working" => {
                p.y = -(t * 6.3).sin().abs() * 0.6 * PF;
                if p.prop == Some("lens") {
                    p.lx = [-1, 0, 1, 0][((t * 1.5) as i32 % 4) as usize];
                } else {
                    p.eyes = "down";
                    p.lx = 1;
                }
            }
            "attention" => {
                let ph = t % 1.2;
                p.y = if ph < 0.6 {
                    -(PI * ph / 0.6).sin() * 3.0 * PF
                } else {
                    0.0
                };
                p.arms = wave(3.0);
            }
            "done" => {
                p.eyes = "happy";
                p.y = if since < 0.7 {
                    -(PI * since / 0.7).sin() * 3.0 * PF
                } else {
                    0.0
                };
                if since < 4.0 {
                    p.arms = wave(3.0);
                }
            }
            _ => {}
        }

        if let Some(m) = inp.mouse.filter(|_| inp.hover && !drg && st != "working") {
            p.lx = (((m.0 - (13 * P) as f64) / 28.0).round_ties_even() as i32).clamp(-2, 2);
            p.eyes = if m.1 < (11 * P - 40) as f64 { "up" } else { "normal" };
            if st == "sleep" {
                p.y = 0.0;
            }
        }
        if waving && matches!(st, "idle" | "sleep" | "done") {
            p.arms = wave(5.0);
            p.eyes = "happy";
        }
        if p.poked {
            let pp = (inp.poke_until - t) / 1.2;
            p.eyes = if pp > 0.6 { "squint" } else { "happy" };
            p.y = -(pp * PI).sin() * 3.0 * PF;
            p.x = 0.0;
        }
        if landing {
            let pp = 1.0 - (inp.land_until - t) / 0.45;
            p.y = -(PI * pp).sin() * 2.5 * PF;
            p.eyes = "happy";
        }
        if running {
            let f = ((t * 10.0) as i32 % 2) as usize;
            p.feet = mirror(RUN_FEET[f], inp.facing);
            p.arms = mirror(RUN_ARMS[f], inp.facing);
            p.y = -(t * 10.0 * PI).sin().abs() * PF;
            p.x = inp.facing.wrapping_mul(P) as f64;
            p.eyes = "normal";
            p.lx = 2i32.wrapping_mul(inp.facing);
            p.prop = None;
            p.running = true;
        } else if drg {
            p.feet = if (t * 3.0) as i32 % 2 == 1 { DANGLE_A } else { DANGLE_B }.to_vec();
            p.arms = ARMS_UP.to_vec();
            p.eyes = "wide";
            p.lx = 0;
            p.y = 0.0;
            p.x = 0.0;
            p.prop = None;
            p.held = true;
        }

        if matches!(p.eyes, "normal" | "up" | "down") && t > self.blink_at {
            p.eyes = "blink";
            if t > self.blink_at + 0.13 {
                self.blink_at = t + self.r(2.5, 5.5);
            }
        }
        p
    }

    /// Advance to time t (seconds) and produce the frame. Pixels are redrawn at 24 fps; motion every call.
    pub fn update(&mut self, inp: &PetInput, t: f64) -> PetFrame<'_> {
        let dt = (t - self.last_frame).clamp(0.001, 0.05);
        self.last_frame = t;
        let pose = self.compute_pose(inp, t);

        let mut pixels_changed = false;
        let n = (t * 24.0) as i32;
        if n != self.last_draw_frame {
            self.last_draw_frame = n;
            self.draw(&pose, inp, t);
            pixels_changed = true;
        }

        // squash & stretch
        let v = (pose.y - self.prev_y) / dt;
        if self.prev_y < -2.0 && pose.y >= -0.4 && v > 0.0 {
            self.squash = 0.13;
        }
        self.squash *= (-11.0 * dt).exp();
        let stretch = (-v / 2600.0).clamp(0.0, 0.06);
        let breathe = if matches!(inp.state.as_str(), "idle" | "sleep") && !inp.dragging {
            0.012 * (t * if inp.state == "sleep" { 1.2 } else { 2.2 }).sin()
        } else {
            0.0
        };
        self.prev_y = pose.y;
        let lift = (1.0 + pose.y / 45.0).clamp(0.5, 1.0);
        PetFrame {
            body: &self.body_px,
            glow: &self.glow_px,
            fx: &self.fx_px,
            pixels_changed,
            x: pose.x,
            y: pose.y,
            scale_x: 1.0 + self.squash - stretch * 0.5 - breathe * 0.5,
            scale_y: 1.0 - self.squash + stretch + breathe,
            shadow_scale: if pose.held { 0.55 } else { lift },
            shadow_opacity: if pose.held { 0.35 } else { lift },
        }
    }

    fn draw(&mut self, p: &Pose, inp: &PetInput, t: f64) {
        let a = &self.avatar;
        let body_px = &mut self.body_px;
        let glow_px = &mut self.glow_px;
        let fx_px = &mut self.fx_px;
        body_px.fill(0);
        glow_px.fill(0);
        fx_px.fill(0);

        let mut shape = self.body.clone();
        shape.extend(p.arms.iter().copied());
        shape.extend(p.feet.iter().copied());
        put_all(body_px, outline(&shape), a.outline);
        let rim = a.shading.as_deref() == Some("rim");
        for &(x, y) in &shape {
            let below = shape.contains(&(x, y + 1));
            let right = shape.contains(&(x + 1, y));
            let above = shape.contains(&(x, y - 1));
            let left = shape.contains(&(x - 1, y));
            let col = if rim {
                // dark figure, lit from behind: bright edge along the top and sides, darker towards the bottom
                let edge = !below || !right || !above || !left;
                if edge {
                    if y < 17 { a.hi } else { a.shade }
                } else if y >= 18 {
                    a.deep
                } else if y >= 14 {
                    a.shade
                } else {
                    a.body
                }
            } else if !below || !right {
                if y >= 19 { a.deep } else { a.shade }
            } else if (!above || !left) && y < 15 {
                a.hi
            } else if y >= 19 {
                a.shade
            } else {
                a.body
            };
            put(body_px, (x, y), col);
        }
        if rim {
            for &c in shape.iter().filter(|c| !shape.contains(&(c.0, c.1 - 1)) && c.1 < 8) {
                put(body_px, c, a.spec);
            }
        } else {
            for &c in &self.spec {
                if shape.contains(&c) {
                    put(body_px, c, a.spec);
                }
            }
        }
        if a.blush {
            for &c in BLUSH_CELLS {
                if shape.contains(&c) {
                    put(body_px, c, a.blush_c);
                }
            }
        }

        if p.headphones {
            let hp: BTreeSet<Cell> = self.hp_band.iter().chain(&self.hp_cups).copied().collect();
            for c in outline(&hp) {
                if !shape.contains(&c) {
                    put(body_px, c, a.outline);
                }
            }
            put_all(body_px, self.hp_band.iter().copied(), HP_DARK);
            put_all(body_px, self.hp_cups.iter().copied(), HP_CUP);
            put_all(body_px, self.hp_shine.iter().copied(), a.accent | 0xFF000000); // cups carry the avatar's theme colour
        }
        for &c in &self.face {
            let face = &self.face;
            let edge = !face.contains(&(c.0 + 1, c.1))
                || !face.contains(&(c.0 - 1, c.1))
                || !face.contains(&(c.0, c.1 + 1))
                || !face.contains(&(c.0, c.1 - 1));
            put(body_px, c, if edge { a.bezel } else { a.screen });
        }
        put(body_px, (9, a.face_top.wrapping_add(1)), a.glint);
        put(body_px, (10, a.face_top.wrapping_add(1)), a.glint);

        let eyes = eye_cells(p.eyes, p.lx);
        let eye_set: BTreeSet<Cell> = eyes.iter().copied().collect();
        for &(x, y) in &eyes {
            let up = eye_set.contains(&(x, y - 1));
            let dn = eye_set.contains(&(x, y + 1));
            put(
                glow_px,
                (x, y),
                if !up && dn {
                    a.eye_hi
                } else if up && !dn {
                    a.eye_lo
                } else {
                    a.eye
                },
            );
        }

        // props
        if let Some(prop) = p.prop.filter(|_| !p.poked && t >= inp.land_until) {
            if prop == "lens" {
                let sx = [0, 1, 0, -1][((t * 1.5) as i32 % 4) as usize];
                let ring: BTreeSet<Cell> = [
                    (19, 13),
                    (20, 13),
                    (21, 13),
                    (18, 14),
                    (22, 14),
                    (18, 15),
                    (22, 15),
                    (18, 16),
                    (22, 16),
                    (19, 17),
                    (20, 17),
                    (21, 17),
                ]
                .into_iter()
                .map(|(x, y)| (x + sx, y))
                .collect();
                let glass: BTreeSet<Cell> = [19, 20, 21]
                    .into_iter()
                    .flat_map(|x| [14, 15, 16].into_iter().map(move |y| (x + sx, y)))
                    .collect();
                put_all(body_px, outline(&ring.union(&glass).copied().collect()), a.outline);
                put_all(body_px, glass.iter().copied(), GLASS);
                put(body_px, (19 + sx, 14), 0xCCFFFFFF);
                put_all(body_px, ring.iter().copied(), RING);
                outlined(body_px, [(23 + sx, 18), (24 + sx, 19)], HANDLE, a.outline);
            } else {
                let lid: BTreeSet<Cell> = (14..22).flat_map(|x| (15..21).map(move |y| (x, y))).collect();
                let base_row: BTreeSet<Cell> = (12..23).map(|x| (x, 21)).collect();
                put_all(body_px, outline(&lid.union(&base_row).copied().collect()), a.outline);
                put_all(body_px, lid.iter().copied(), LID);
                put_all(body_px, (14..22).map(|x| (x, 15)), LID_HI);
                put_all(body_px, (15..21).flat_map(|x| (16..20).map(move |y| (x, y))), LID_IN);
                put_all(body_px, base_row.iter().copied(), BASE);
                put_all(body_px, [(21, 21), (22, 21)], BASE_SH);
                put_all(glow_px, [(16, 16), (17, 17), (16, 18)], a.eye);
                if (t * 2.5) as i32 % 2 == 1 {
                    put_all(glow_px, [(18, 18), (19, 18)], a.eye);
                }
            }
        }

        // effects
        let acc = a.accent | 0xFF000000;
        if p.running {
            let dust = mirror(DUST, inp.facing);
            for (i, &d) in dust.iter().enumerate() {
                if ((t * 8.0) as i32).wrapping_add(i as i32) % 3 != 0 {
                    put(fx_px, d, DUSTC);
                }
            }
        } else if self.anim_state == "thinking" && !p.held {
            let n = (t * 3.0) as i32 % 4;
            let dots = [(20, 5), (22, 3), (24, 1)];
            for i in 0..n {
                outlined(fx_px, [dots[i as usize]], DOT, a.outline);
            }
        } else if self.anim_state == "listen" && !p.held {
            for i in 0..2 {
                let ph = (t * 0.45 + i as f64 * 0.5) % 1.0;
                let bx =
                    (if i == 0 { 19 } else { 2 }) + (ph * if i == 0 { 3.0 } else { -2.0 }).round_ties_even() as i32;
                let by = 7 - (ph * 7.0).round_ties_even() as i32;
                let color = if ph > 0.7 { (acc & 0x00FFFFFF) | 0x88000000 } else { acc };
                put_all(fx_px, NOTE.iter().map(|n| (bx + n.0, by + n.1)), color);
            }
        } else if self.anim_state == "sleep" && !(inp.hover || p.held) {
            for i in 0..3 {
                let ph = (t * 0.35 + i as f64 / 3.0) % 1.0;
                let bx = 19 + (ph * 4.0).round_ties_even() as i32;
                let by = 6 - (ph * 6.0).round_ties_even() as i32;
                put_all(
                    fx_px,
                    ZGLYPH.iter().map(|g| (bx + g.0, by + g.1)),
                    if ph > 0.75 { 0x99E3E8FF } else { ZZ },
                );
            }
        } else if self.anim_state == "attention" && !p.held {
            outlined(fx_px, [(24, 1), (24, 2), (24, 3), (24, 5)], BANG, a.outline);
        }

        if (self.anim_state == "done" || p.poked) && !p.held {
            let on = (t * 4.0) as i32 % 2 == 1;
            for (x, y) in if on { [(3, 6), (22, 3)] } else { [(2, 9), (23, 6)] } {
                put_all(fx_px, [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)], SPARK);
                put(fx_px, (x, y), SPARK_HI);
            }
        }
    }
}

// ------------------------------------------------------------------ avatar geometry

fn build_body(shapes: &[[f64; 4]]) -> BTreeSet<Cell> {
    let mut s = BTreeSet::new();
    for y in 0..GH {
        for x in 0..GW {
            let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
            for e in shapes {
                let (dx, dy) = ((cx - e[0]) / e[2], (cy - e[1]) / e[3]);
                if dx * dx + dy * dy <= 1.0 {
                    s.insert((x, y));
                    break;
                }
            }
        }
    }
    s
}

fn build_face(top: i32, bottom: i32) -> BTreeSet<Cell> {
    let mut s = BTreeSet::new();
    for x in 8..18 {
        for y in top..=bottom {
            s.insert((x, y));
        }
    }
    for c in [(8, top), (17, top), (8, bottom), (17, bottom)] {
        s.remove(&c);
    }
    s
}

/// Top-left-most body cells on each bump: little shine pixels for the "soft" style.
fn find_spec(body: &BTreeSet<Cell>) -> Vec<Cell> {
    // The C# takes the first 3 after a stable OrderBy(Y) over the HashSet, which enumerates in insertion order:
    // row-major, as BuildBody adds the cells. So ties on Y go to the smaller X.
    let mut cells: Vec<Cell> = body
        .iter()
        .copied()
        .filter(|&(x, y)| !body.contains(&(x, y - 1)) && body.contains(&(x + 1, y + 1)) && x < 13 && y < 9)
        .collect();
    cells.sort_by_key(|&(x, y)| (y, x));
    cells.into_iter().take(3).map(|(x, y)| (x + 1, y + 1)).collect()
}

/// Headphones that hug the head: ear cups at the sides of rows 9–12, a band just above the silhouette.
/// Returns (band, cups, shine).
fn build_headphones(body: &BTreeSet<Cell>) -> (Vec<Cell>, Vec<Cell>, Vec<Cell>) {
    let left = |y: i32| body.iter().filter(|c| c.1 == y).map(|c| c.0).min().unwrap_or(5);
    let right = |y: i32| body.iter().filter(|c| c.1 == y).map(|c| c.0).max().unwrap_or(20);
    let (lx, rx) = (left(10), right(10));
    let mut cups = Vec::new();
    for y in 9..=12 {
        cups.extend([(lx - 1, y), (lx, y), (rx, y), (rx + 1, y)]);
    }
    let mut b = BTreeSet::new();
    let mut prev: Option<i32> = None;
    for x in lx..=rx {
        let col = body.iter().filter(|c| c.0 == x).map(|c| c.1).min().unwrap_or(9);
        let y = (col - 1).min(8);
        b.insert((x, y));
        if let Some(py) = prev {
            for yy in py.min(y) + 1..py.max(y) {
                b.insert((if py < y { x - 1 } else { x }, yy));
            }
        }
        prev = Some(y);
    }
    let top = |b: &BTreeSet<Cell>, x: i32| b.iter().filter(|c| c.0 == x).map(|c| c.1).min().unwrap_or(8);
    for y in top(&b, lx) + 1..9 {
        b.insert((lx, y));
    }
    for y in top(&b, rx) + 1..9 {
        b.insert((rx, y));
    }
    let shine = vec![(lx - 1, 10), (lx - 1, 11), (rx + 1, 10), (rx + 1, 11)];
    (b.into_iter().collect(), cups, shine)
}

fn mirror(cells: &[Cell], facing: i32) -> Vec<Cell> {
    if facing > 0 {
        cells.to_vec()
    } else {
        cells.iter().map(|&(x, y)| (GW - 1 - x, y)).collect()
    }
}

fn outline(cells: &BTreeSet<Cell>) -> BTreeSet<Cell> {
    let mut ring = BTreeSet::new();
    for &(x, y) in cells {
        for n in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
            if !cells.contains(&n) {
                ring.insert(n);
            }
        }
    }
    ring
}

/// The eye cells for `kind`, shifted `lx` columns. The column sums wrap as the C#'s unchecked int maths does: while
/// the pet runs, lx is `2 * facing` (wrapped), and a huge facing must not panic a debug build. Cells that end up off
/// the grid are skipped by `put`.
fn eye_cells(kind: &str, lx: i32) -> Vec<Cell> {
    let x = |dx: i32| dx.wrapping_add(lx);
    let (l, l1, l_1) = (x(10), x(11), x(9));
    let (r, r1, r2) = (x(14), x(15), x(16));
    let block = |y0: i32, y1: i32| -> Vec<Cell> {
        [l, l1, r, r1]
            .into_iter()
            .flat_map(|x| (y0..=y1).map(move |y| (x, y)))
            .collect()
    };
    match kind {
        "wide" => block(10, 13),
        "squint" => vec![(l, 10), (l1, 11), (l, 12), (r1, 10), (r, 11), (r1, 12)],
        "blink" => vec![(l, 12), (l1, 12), (r, 12), (r1, 12)],
        "happy" => vec![(l_1, 11), (l, 10), (l1, 11), (r, 11), (r1, 10), (r2, 11)],
        "sleep" => vec![(l_1, 12), (l, 12), (l1, 12), (r, 12), (r1, 12), (r2, 12)],
        "up" => block(10, 11),
        "down" => block(12, 13),
        _ => block(10, 12),
    }
}

// ------------------------------------------------------------------ pixel buffers

/// Premultiplies an 0xAARRGGBB colour by its alpha, with integer math that rounds down (as the C# `Pm`).
pub fn pm(argb: u32) -> u32 {
    let a = argb >> 24;
    if a == 255 {
        return argb;
    }
    let r = ((argb >> 16) & 255) * a / 255;
    let g = ((argb >> 8) & 255) * a / 255;
    let b = (argb & 255) * a / 255;
    (a << 24) | (r << 16) | (g << 8) | b
}

fn put(buf: &mut [u32; PIXELS], c: Cell, color: u32) {
    if c.0 >= 0 && c.0 < GW && c.1 >= 0 && c.1 < GH {
        buf[(c.1 * GW + c.0) as usize] = pm(color);
    }
}

fn put_all(buf: &mut [u32; PIXELS], cells: impl IntoIterator<Item = Cell>, color: u32) {
    for c in cells {
        put(buf, c, color);
    }
}

/// The cells in `color` with an outline (the avatar's outline colour) around them.
fn outlined(buf: &mut [u32; PIXELS], cells: impl IntoIterator<Item = Cell>, color: u32, outline_color: u32) {
    let set: BTreeSet<Cell> = cells.into_iter().collect();
    put_all(buf, outline(&set), outline_color);
    put_all(buf, set, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xs(y: i32, xs: &[i32]) -> Vec<Cell> {
        xs.iter().map(|&x| (x, y)).collect()
    }

    #[test]
    fn pm_premultiplies_with_integer_math() {
        assert_eq!(pm(0xFF123456), 0xFF123456);
        assert_eq!(pm(0x00FFFFFF), 0x00000000);
        assert_eq!(pm(0x80FF8040), 0x80804020);
        assert_eq!(pm(0x88A9E4FF), 0x885A7988);
        assert_eq!(pm(0x01FFFFFF), 0x01010101);
        assert_eq!(pm(0xFEFFFFFF), 0xFEFEFEFE);
        assert_eq!(pm(0xFE010101), 0xFE000000);
    }

    #[test]
    fn cell_tables_match_their_csharp_definitions() {
        assert_eq!(FEET, xs(21, &[8, 9, 10, 15, 16, 17]));
        assert_eq!(FOOT_TAP, [xs(21, &[8, 9, 10]), xs(20, &[16, 17, 18])].concat());
        assert_eq!(ARM_L, &ARMS_DOWN[..4]);
        assert_eq!(ARMS_UP, [&[(3, 10), (4, 10), (3, 11), (4, 11)][..], WAVE_A].concat());
    }

    #[test]
    fn mirror_flips_around_the_grid_centre() {
        assert_eq!(mirror(DUST, 1), DUST);
        assert_eq!(mirror(DUST, -1), vec![(21, 21), (23, 20), (25, 19)]);
        assert_eq!(mirror(DUST, 0), mirror(DUST, -1));
    }

    #[test]
    fn eye_blocks() {
        assert_eq!(eye_cells("normal", 0).len(), 12);
        assert_eq!(
            eye_cells("wide", 1),
            [11, 12, 15, 16]
                .iter()
                .flat_map(|&x| (10..=13).map(move |y| (x, y)))
                .collect::<Vec<_>>()
        );
        assert_eq!(eye_cells("anything", 0), eye_cells("normal", 0));
    }

    struct Fixed(f64);

    impl PetRng for Fixed {
        fn next_double(&mut self) -> f64 {
            self.0
        }
        fn next_int(&mut self, max: i32) -> i32 {
            (self.0 * max as f64) as i32
        }
    }

    #[test]
    fn a_new_pet_sleeps_as_sprout_and_draws_on_the_first_update() {
        let mut pet = Pet::with_rng(Fixed(0.5));
        assert_eq!(pet.anim_state(), "sleep");
        assert_eq!(pet.avatar().name, "Sprout");
        let f = pet.update(&PetInput::default(), 0.0);
        assert!(f.pixels_changed);
        assert!(f.body.iter().any(|&px| px != 0));
        assert!(f.glow.iter().any(|&px| px != 0));
        // same 1/24 s frame: no redraw
        assert!(!pet.update(&PetInput::default(), 0.01).pixels_changed);
        // a new avatar redraws at once
        pet.set_avatar(Avatar::hood());
        assert!(pet.update(&PetInput::default(), 0.02).pixels_changed);
    }

    #[test]
    fn the_random_source_drives_idle_acts_after_six_seconds() {
        // Fixed(0.5): Next(5) = 2 -> "stretch": arms up, happy eyes, one grid pixel up
        let mut pet = Pet::with_rng(Fixed(0.5));
        let idle = PetInput {
            state: "idle".to_owned(),
            ..PetInput::default()
        };
        pet.update(&idle, 6.5);
        assert_eq!(pet.idle_act, Some("stretch"));
        assert_eq!(pet.update(&idle, 6.6).y, -PF);
    }

    fn idle() -> PetInput {
        PetInput {
            state: "idle".to_owned(),
            ..PetInput::default()
        }
    }

    #[test]
    fn hovering_or_holding_the_pet_holds_the_idle_act() {
        for held in [false, true] {
            // Fixed(0.5): a stretch from 6.5 s to 6.5 + 1.6 + 0.5 * 1.0 = 8.6 s
            let mut pet = Pet::with_rng(Fixed(0.5));
            pet.update(&idle(), 6.5);
            assert_eq!((pet.idle_act(), pet.idle_until), (Some("stretch"), 8.6));
            let busy = if held {
                PetInput {
                    dragging: true,
                    moved: true,
                    last_move: -10.0,
                    ..idle()
                }
            } else {
                PetInput {
                    hover: true,
                    mouse: Some((65.0, 60.0)),
                    ..idle()
                }
            };
            // kept, but not shown (no lift)
            assert_ne!(pet.update(&busy, 7.0).y, -PF, "held: {held}");
            assert_eq!(pet.idle_act(), Some("stretch"));
            // its time is up, but it doesn't end while the pet is busy
            pet.update(&busy, 9.0);
            assert_eq!(pet.idle_act(), Some("stretch"));
            // it ends at the first free update, and the next act comes 5 + 0.5 * 6 = 8 s later
            pet.update(&idle(), 9.5);
            assert_eq!((pet.idle_act(), pet.next_idle), (None, 17.5));
            pet.update(&idle(), 17.4);
            assert_eq!(pet.idle_act(), None);
            assert_eq!(pet.update(&idle(), 17.6).y, -PF);
            assert_eq!((pet.idle_act(), pet.idle_start), (Some("stretch"), 17.6));
        }
    }

    #[test]
    fn a_new_state_drops_the_idle_act() {
        let mut pet = Pet::with_rng(Fixed(0.5));
        pet.update(&idle(), 6.5);
        assert_eq!(pet.idle_act(), Some("stretch"));
        let thinking = PetInput {
            state: "thinking".to_owned(),
            state_since: 7.0,
            ..idle()
        };
        pet.update(&thinking, 7.0);
        assert_eq!(pet.idle_act(), None);
        // back to idle: the next act was due at 6 s, so a new one starts at once
        let f = pet.update(
            &PetInput {
                state_since: 7.5,
                ..idle()
            },
            7.5,
        );
        assert_eq!(f.y, -PF);
        assert_eq!(
            (pet.idle_act(), pet.idle_start, pet.idle_until),
            (Some("stretch"), 7.5, 7.5 + 2.1)
        );
    }

    #[test]
    fn a_huge_facing_wraps_as_in_the_csharp() {
        // running: lx = 2 * facing and x = facing * P wrap as the C#'s unchecked ints do, and so do the eye
        // columns, so a debug build doesn't panic; the eyes fall off the grid and aren't drawn
        let mut pet = Pet::with_rng(Fixed(0.5));
        let run = PetInput {
            dragging: true,
            moved: true,
            last_move: 1.0,
            facing: 1_073_741_823,
            ..idle()
        };
        let f = pet.update(&run, 1.1);
        assert_eq!(f.x.to_bits(), 0x41CF_FFFF_FD80_0000);
        assert_eq!(f.x, 1_073_741_819.0);
        assert!(f.pixels_changed && f.glow.iter().all(|&px| px == 0));
        for (i, facing) in [i32::MAX, i32::MIN, 1 << 30, -(1 << 30), i32::MIN + 1]
            .into_iter()
            .enumerate()
        {
            let f = pet.update(&PetInput { facing, ..run.clone() }, 1.15 + i as f64 * 0.05);
            assert!(f.pixels_changed);
        }
        for lx in [i32::MAX, i32::MIN, i32::MAX - 12, i32::MIN + 8] {
            for kind in ["wide", "squint", "blink", "happy", "sleep", "up", "down", "normal"] {
                assert!(!eye_cells(kind, lx).is_empty());
            }
        }
    }

    #[test]
    fn xorshift_stays_in_range() {
        let mut r = XorShift64::new(1);
        for _ in 0..10_000 {
            let d = r.next_double();
            assert!((0.0..1.0).contains(&d));
            assert!((0..5).contains(&r.next_int(5)));
        }
        assert_eq!(XorShift64::new(0).next_u64(), XorShift64::new(0).next_u64());
    }
}
