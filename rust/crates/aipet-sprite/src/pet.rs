//! The pixel pet: a port of `src/AiPet.Core/Pet.cs`.
//!
//! Numbers follow the C# exactly: `Math.Round` is banker's rounding (`round_ties_even`), `(int)x` truncates toward
//! zero and saturates (`as i32`), `%` keeps the dividend's sign (as Rust's does), and every expression keeps the C#
//! operand order, so the doubles come out bit for bit the same.

use std::collections::hash_map::RandomState;
use std::f64::consts::PI;
use std::hash::{BuildHasher, Hasher};

use crate::avatar::Avatar;
use crate::bake::{self, Sheet};
use crate::compose;
use crate::pack::{Art, Pack};
use crate::pose::{self, Expression, Facing, Fx, IdleAct, Look, Mood, Motion, Pose, Prop, StateValue, Values};

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

/// The pet: its behaviour (the animation state machine), the pose it computes, and its pack, composited into three
/// layers. Platform-neutral; the UI just blits the three layers (Glow gets a coloured blur on top).
pub struct Pet<R: PetRng = XorShift64> {
    // ------------------------------------------------------------------ the avatar
    avatar: Avatar,

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

    // ------------------------------------------------------------------ the pose and the pack
    tracker: pose::Tracker,
    pose: Pose,
    pack: Pack,
    /// The pack's sheets, straight colours (premultiplied as they're drawn).
    sheets: Vec<Sheet>,
    /// When the layers were last drawn.
    drawn_at: f64,
}

/// What the behaviour chose for one frame, in the old renderer's terms (eye names and columns), with the named poses
/// ([`pose`]) the pose takes.
struct Figure<'a> {
    eyes: &'static str,
    lx: i32,
    arms_pose: pose::Arms,
    feet_pose: pose::Feet,
    /// The idle act, while one plays and is shown (not while hovered or held).
    act: Option<&'static str>,
    /// The pointer the eyes follow, if they do (DIPs over the sprite).
    pointer: Option<(f64, f64)>,
    prop: Option<&'a str>,
    x: f64,
    y: f64,
    running: bool,
    held: bool,
    poked: bool,
    headphones: bool,
}

impl Default for Figure<'_> {
    fn default() -> Self {
        Figure {
            eyes: "normal",
            lx: 0,
            arms_pose: pose::Arms::Rest,
            feet_pose: pose::Feet::Stand,
            act: None,
            pointer: None,
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
            tracker: pose::Tracker::default(),
            pose: Pose::scripted(Values::IDLE, 0.0),
            pack: Pack {
                name: String::new(),
                kind: crate::pack::Kind::Pixel,
                pixel_size: PF,
                anchor: (13.0, 22.0),
                mirror: crate::pack::Mirror::None,
                accent: 0,
                glow: 0,
                layers: Vec::new(),
            },
            sheets: Vec::new(),
            drawn_at: 0.0,
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
        // a built-in pet's embedded pack, else the avatar baked now: once, not per frame
        (self.pack, self.sheets) = crate::builtin::pack_for(&a).unwrap_or_else(|| {
            let baked = bake::bake(&a);
            (baked.pack, baked.sheets)
        });
        self.avatar = a;
        self.last_draw_frame = -1;
        self.tracker.reset();
    }

    /// The last frame's pose ([`Pet::update`] computes it): what each pack layer shows, and the motion.
    pub fn pose(&self) -> &Pose {
        &self.pose
    }

    fn r(&mut self, a: f64, b: f64) -> f64 {
        a + self.rng.next_double() * (b - a)
    }

    fn compute_pose<'a>(&mut self, inp: &'a PetInput, t: f64) -> Figure<'a> {
        let mut p = Figure::default();
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
        let wave = |rate: f64| {
            if (t * rate) as i32 % 2 == 1 {
                pose::Arms::WaveA
            } else {
                pose::Arms::WaveB
            }
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
                    p.act = self.idle_act;
                    match self.idle_act {
                        Some("look") => p.lx = [-2, -1, 0, 1, 2, 1, 0, -1][((t * 4.0) as i32 % 8) as usize],
                        Some("hop") => {
                            p.y = -(PI * (t - self.idle_start) / (self.idle_until - self.idle_start)).sin() * 3.2 * PF
                        }
                        Some("stretch") => {
                            p.arms_pose = pose::Arms::Raised;
                            p.eyes = "happy";
                            p.y = -PF;
                        }
                        Some("wiggle") => p.x = (t * 16.0).sin() * 0.8 * PF,
                        Some("tap") => {
                            let tap = (t * 5.0) as i32 % 2 == 1;
                            p.feet_pose = if tap { pose::Feet::Tap } else { pose::Feet::Stand };
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
                p.arms_pose = wave(3.0);
            }
            "done" => {
                p.eyes = "happy";
                p.y = if since < 0.7 {
                    -(PI * since / 0.7).sin() * 3.0 * PF
                } else {
                    0.0
                };
                if since < 4.0 {
                    p.arms_pose = wave(3.0);
                }
            }
            _ => {}
        }

        if let Some(m) = inp.mouse.filter(|_| inp.hover && !drg && st != "working") {
            p.pointer = Some(m);
            p.lx = (((m.0 - (13 * P) as f64) / 28.0).round_ties_even() as i32).clamp(-2, 2);
            p.eyes = if m.1 < (11 * P - 40) as f64 { "up" } else { "normal" };
            if st == "sleep" {
                p.y = 0.0;
            }
        }
        if waving && matches!(st, "idle" | "sleep" | "done") {
            p.arms_pose = wave(5.0);
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
            (p.feet_pose, p.arms_pose) = if f == 0 {
                (pose::Feet::RunA, pose::Arms::RunA)
            } else {
                (pose::Feet::RunB, pose::Arms::RunB)
            };
            p.y = -(t * 10.0 * PI).sin().abs() * PF;
            p.x = inp.facing.wrapping_mul(P) as f64;
            p.eyes = "normal";
            p.lx = 2i32.wrapping_mul(inp.facing);
            p.prop = None;
            p.running = true;
        } else if drg {
            let a = (t * 3.0) as i32 % 2 == 1;
            p.feet_pose = if a { pose::Feet::DangleA } else { pose::Feet::DangleB };
            p.arms_pose = pose::Arms::Raised;
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
        let redraw = n != self.last_draw_frame;
        if redraw {
            self.last_draw_frame = n;
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
        let motion = Motion {
            x: pose.x,
            y: pose.y,
            scale_x: 1.0 + self.squash - stretch * 0.5 - breathe * 0.5,
            scale_y: 1.0 - self.squash + stretch + breathe,
            shadow_scale: if pose.held { 0.55 } else { lift },
            shadow_opacity: if pose.held { 0.35 } else { lift },
        };
        let values = self.values(&pose, inp, t);
        self.pose = self.tracker.pose(values, motion, t);
        if redraw {
            self.drawn_at = t;
            self.composite(t);
        }
        PetFrame {
            body: &self.body_px,
            glow: &self.glow_px,
            fx: &self.fx_px,
            pixels_changed,
            x: motion.x,
            y: motion.y,
            scale_x: motion.scale_x,
            scale_y: motion.scale_y,
            shadow_scale: motion.shadow_scale,
            shadow_opacity: motion.shadow_opacity,
        }
    }

    /// The body, glow and effects layers as they'd be drawn `dt` seconds from when they last were, with the same pose;
    /// the layers themselves are left as drawn. For the golden test: an effect on the pet clock may change frame up to
    /// a millisecond from where the C# did.
    #[doc(hidden)]
    pub fn layers_shifted(&mut self, dt: f64) -> [[u32; PIXELS]; 3] {
        let at = self.drawn_at;
        self.composite(at + dt);
        let shifted = [self.body_px, self.glow_px, self.fx_px];
        self.composite(at);
        shifted
    }

    /// Draws the pose's layers from the pack into the body, glow and effects layers: each layer's frame, moved by its
    /// look offset and mirrored as it says, copied where it isn't transparent.
    fn composite(&mut self, t: f64) {
        self.body_px.fill(0);
        self.glow_px.fill(0);
        self.fx_px.fill(0);
        for pick in compose::pick(&self.pack, &self.pose, t) {
            let layer = &self.pack.layers[pick.layer];
            let Some(Art::Raster { sheet, x, y, w, h, .. }) =
                pick.frame.map(|f| &layer.animations[pick.value].frames[f].art).cloned()
            else {
                continue;
            };
            let (dx, dy) = compose::raster_offset(&self.pack, layer, &self.pose);
            let (ox, oy) = ((dx / self.pack.pixel_size) as i32, (dy / self.pack.pixel_size) as i32);
            let target = if layer.channel == pose::Channel::Fx {
                &mut self.fx_px
            } else if layer.glow {
                &mut self.glow_px
            } else {
                &mut self.body_px
            };
            let sheet = &self.sheets[sheet];
            for fy in 0..h as i32 {
                for fx in 0..w as i32 {
                    let src = sheet.pixels[((y as i32 + fy) * sheet.width as i32 + x as i32 + fx) as usize];
                    // 0 is not drawn; a transparent colour that was drawn still covers (bake::COVERED)
                    if src == 0 {
                        continue;
                    }
                    let cx = fx.wrapping_add(ox);
                    let cx = if pick.mirrored { GW - 1 - cx } else { cx };
                    let cy = fy.wrapping_add(oy);
                    if (0..GW).contains(&cx) && (0..GH).contains(&cy) {
                        target[(cy * GW + cx) as usize] = pm(src);
                    }
                }
            }
        }
    }

    /// The pose's channels for what `draw` draws from `p`: the same conditions, in the pose's terms.
    fn values(&mut self, p: &Figure, inp: &PetInput, t: f64) -> Values {
        let mood = Mood::of(&self.anim_state);
        let expression = Expression::of(p.eyes);
        let look = match self.tracker.pointer_look(p.pointer) {
            Some(look) => look,
            None => Look {
                dx: p.lx.clamp(-2, 2) as i8,
                dy: match expression {
                    Expression::Up => -1,
                    Expression::Down => 1,
                    _ => 0,
                },
            },
        };
        let prop = match p.prop.filter(|_| !p.poked && t >= inp.land_until) {
            Some("lens") => Prop::Lens,
            Some(_) => Prop::Laptop,
            None => Prop::None,
        };
        let arms = match (p.arms_pose, prop) {
            (pose::Arms::Rest, Prop::Laptop) => pose::Arms::Hold,
            (pose::Arms::Rest, Prop::Lens) => pose::Arms::Lens,
            (arms, _) => arms,
        };
        let fx = if p.running {
            Fx::Dust
        } else if p.held {
            Fx::None
        } else {
            match mood {
                Mood::Thinking => Fx::Dots,
                Mood::Listen => Fx::Notes,
                Mood::Sleep if !inp.hover => Fx::Zs,
                Mood::Attention => Fx::Alert,
                _ => Fx::None,
            }
        };
        Values {
            state: StateValue {
                mood,
                act: if mood == Mood::Idle {
                    p.act.and_then(IdleAct::of)
                } else {
                    None
                },
            },
            eyes: pose::Eyes { expression, look },
            arms,
            feet: p.feet_pose,
            prop,
            headphones: p.headphones,
            fx,
            sparkles: (self.anim_state == "done" || p.poked) && !p.held,
            facing: if inp.facing > 0 { Facing::Right } else { Facing::Left },
        }
    }
}

// ------------------------------------------------------------------ pixels

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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_pose_names_what_the_renderer_draws() {
        use crate::pose::{Arms, Channel, Expression, Facing, Feet, Fx, Mood, Prop};
        let mut pet = Pet::with_rng(Fixed(0.5));
        // asleep, not hovered: eyes closed, Zs
        pet.update(&PetInput::default(), 1.0);
        let p = *pet.pose();
        assert_eq!(
            (p.state.mood, p.eyes.expression, p.fx),
            (Mood::Sleep, Expression::Closed, Fx::Zs)
        );
        assert_eq!(p.since(Channel::State), 1.0);
        // working with the laptop: the arms hold it, the eyes look down at it
        let working = PetInput {
            state: "working".to_owned(),
            ..PetInput::default()
        };
        pet.update(&working, 2.0);
        let p = *pet.pose();
        assert_eq!(
            (p.prop, p.arms, p.eyes.expression),
            (Prop::Laptop, Arms::Hold, Expression::Down)
        );
        assert_eq!((p.eyes.look.dx, p.eyes.look.dy), (1, 1));
        assert_eq!(p.since(Channel::State), 2.0);
        // held without moving for a while: dangling, arms up, wide eyes, nothing held, no effect
        let held = PetInput {
            dragging: true,
            moved: true,
            last_move: 0.0,
            ..idle()
        };
        pet.update(&held, 3.0);
        let p = *pet.pose();
        assert!(matches!(p.feet, Feet::DangleA | Feet::DangleB));
        assert_eq!(
            (p.arms, p.eyes.expression, p.prop, p.fx),
            (Arms::Raised, Expression::Wide, Prop::None, Fx::None)
        );
        // running to the left: the run poses, dust, facing left
        let running = PetInput {
            last_move: 4.0,
            facing: -1,
            ..held
        };
        pet.update(&running, 4.1);
        let p = *pet.pose();
        assert!(matches!(p.arms, Arms::RunA | Arms::RunB));
        assert_eq!((p.fx, p.facing), (Fx::Dust, Facing::Left));
        assert_eq!(p.motion.x, -5.0);
        // the music replaces idling: listen, with headphones and notes
        let music = PetInput { music: true, ..idle() };
        pet.update(&music, 5.0);
        let p = *pet.pose();
        assert_eq!((p.state.key(), p.headphones, p.fx), ("listen", true, Fx::Notes));
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
        // running: lx = 2 * facing and x = facing * P wrap as the C#'s unchecked ints do, so a debug build doesn't
        // panic. The C#'s eyes then fell off the grid; the pose clamps the look, so they look far to the side
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
        assert!(f.pixels_changed && f.glow.iter().any(|&px| px != 0));
        assert_eq!(pet.pose().eyes.look.dx.abs(), 2);
        for (i, facing) in [i32::MAX, i32::MIN, 1 << 30, -(1 << 30), i32::MIN + 1]
            .into_iter()
            .enumerate()
        {
            let f = pet.update(&PetInput { facing, ..run.clone() }, 1.15 + i as f64 * 0.05);
            assert!(f.pixels_changed);
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
