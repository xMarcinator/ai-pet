//! Baking an avatar into a layered pixel pack: today's renderer, run once for every value of every channel.
//!
//! The renderer drew each frame from the avatar's ellipses, its palette and hard-coded cell tables. Here the same
//! drawing makes each layer's frames, once, at load (or ahead of time for the built-in pets, whose packs are
//! committed): the body, the headphones, the arms and feet, the prop, the eyes, the laptop's screen and the effects.
//! Composited ([`crate::compose`]) they give back the renderer's frames, which the golden data pins.
//!
//! - **Limbs** are what they change on the bare body: their cells, their outline, and the body cells they shade
//!   differently, so the body's edge next to an arm stays as the renderer drew it.
//! - **Animations on the pet's clock** (the effects, the lens's scan, the laptop's cursor) are sampled every
//!   millisecond over their period and kept as frames with durations ([`Clock::Pet`]).
//! - **Colours are straight** (not premultiplied), as a PNG keeps them; [`crate::pm`] premultiplies them as the
//!   renderer's `put` did.

use std::collections::{BTreeMap, BTreeSet};

use crate::avatar::Avatar;
use crate::pack::{Animation, Art, Clock, Frame, Kind, Layer, LoopMode, Mirror, Pack, Palette, Theme};
use crate::pet::{Cell, GH, GW, PIXELS};
use crate::pose::{Channel, Look};

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
const FEET: &[Cell] = &[(8, 21), (9, 21), (10, 21), (15, 21), (16, 21), (17, 21)];
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

/// A layer's pixels on the grid, straight 0xAARRGGBB, row-major.
pub type Buf = [u32; PIXELS];

/// A sheet: frames side by side, rows of [`ROW`] frames, straight 0xAARRGGBB.
#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

/// Frames per sheet row.
const ROW: usize = 32;

/// A pack and its sheets (by index, as its frames name them).
#[derive(Clone, Debug, PartialEq)]
pub struct Baked {
    pub pack: Pack,
    pub sheets: Vec<Sheet>,
}

/// An avatar's cell sets, from its ellipses.
struct Geometry {
    body: BTreeSet<Cell>,
    face: BTreeSet<Cell>,
    spec: Vec<Cell>,
    hp_band: Vec<Cell>,
    hp_cups: Vec<Cell>,
    hp_shine: Vec<Cell>,
}

impl Geometry {
    fn of(a: &Avatar) -> Geometry {
        let body = build_body(&a.shapes);
        let (hp_band, hp_cups, hp_shine) = build_headphones(&body);
        Geometry {
            face: build_face(a.face_top, a.face_bottom),
            spec: find_spec(&body),
            body,
            hp_band,
            hp_cups,
            hp_shine,
        }
    }
}

/// A pixel drawn in a fully transparent colour: it still covers what's under it, as the renderer's `put` did. Its
/// alpha is 0, so premultiplied it is transparent; only a pixel of 0 is one a layer doesn't draw.
pub const COVERED: u32 = 0x0000_0001;

fn put(buf: &mut Buf, c: Cell, color: u32) {
    if c.0 >= 0 && c.0 < GW && c.1 >= 0 && c.1 < GH {
        buf[(c.1 * GW + c.0) as usize] = if color >> 24 == 0 { COVERED } else { color };
    }
}

fn put_all(buf: &mut Buf, cells: impl IntoIterator<Item = Cell>, color: u32) {
    for c in cells {
        put(buf, c, color);
    }
}

/// The cells in `color` with an outline (the avatar's outline colour) around them.
fn outlined(buf: &mut Buf, cells: impl IntoIterator<Item = Cell>, color: u32, outline_color: u32) {
    let set: BTreeSet<Cell> = cells.into_iter().collect();
    put_all(buf, outline(&set), outline_color);
    put_all(buf, set, color);
}

/// How much of the headphones [`draw_body`] draws: their outline goes under the limbs (it skips their cells), and the
/// band and cups over them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Headphones {
    Off,
    Outline,
    On,
}

/// The body with `limbs` (shaded and outlined with it), its highlights and blush, the headphones if asked, and the
/// face panel: the renderer's body layer before the eyes and the prop.
fn draw_body(a: &Avatar, g: &Geometry, limbs: &[Cell], headphones: Headphones) -> Buf {
    let mut buf = [0; PIXELS];
    let mut shape = g.body.clone();
    shape.extend(limbs.iter().copied());
    put_all(&mut buf, outline(&shape), a.outline);
    let rim = a.shading.as_deref() == Some("rim");
    for &(x, y) in &shape {
        let below = shape.contains(&(x, y + 1));
        let right = shape.contains(&(x + 1, y));
        let above = shape.contains(&(x, y - 1));
        let left = shape.contains(&(x - 1, y));
        let col = if rim {
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
        put(&mut buf, (x, y), col);
    }
    if rim {
        for &c in shape.iter().filter(|c| !shape.contains(&(c.0, c.1 - 1)) && c.1 < 8) {
            put(&mut buf, c, a.spec);
        }
    } else {
        for &c in &g.spec {
            if shape.contains(&c) {
                put(&mut buf, c, a.spec);
            }
        }
    }
    if a.blush {
        for &c in BLUSH_CELLS {
            if shape.contains(&c) {
                put(&mut buf, c, a.blush_c);
            }
        }
    }
    if headphones != Headphones::Off {
        let hp: BTreeSet<Cell> = g.hp_band.iter().chain(&g.hp_cups).copied().collect();
        for c in outline(&hp) {
            if !shape.contains(&c) {
                put(&mut buf, c, a.outline);
            }
        }
    }
    if headphones == Headphones::On {
        put_all(&mut buf, g.hp_band.iter().copied(), HP_DARK);
        put_all(&mut buf, g.hp_cups.iter().copied(), HP_CUP);
        put_all(&mut buf, g.hp_shine.iter().copied(), a.accent | 0xFF000000);
    }
    for &c in &g.face {
        let face = &g.face;
        let edge = !face.contains(&(c.0 + 1, c.1))
            || !face.contains(&(c.0 - 1, c.1))
            || !face.contains(&(c.0, c.1 + 1))
            || !face.contains(&(c.0, c.1 - 1));
        put(&mut buf, c, if edge { a.bezel } else { a.screen });
    }
    put(&mut buf, (9, a.face_top.wrapping_add(1)), a.glint);
    put(&mut buf, (10, a.face_top.wrapping_add(1)), a.glint);
    buf
}

/// The eyes in the glow layer, looking ahead (a look moves the whole layer).
fn draw_eyes(a: &Avatar, kind: &str) -> Buf {
    let mut buf = [0; PIXELS];
    let eyes = eye_cells(kind, 0);
    let set: BTreeSet<Cell> = eyes.iter().copied().collect();
    for &(x, y) in &eyes {
        let up = set.contains(&(x, y - 1));
        let dn = set.contains(&(x, y + 1));
        let colour = if !up && dn {
            a.eye_hi
        } else if up && !dn {
            a.eye_lo
        } else {
            a.eye
        };
        put(&mut buf, (x, y), colour);
    }
    buf
}

/// The magnifier, scanned `sx` columns.
fn draw_lens(a: &Avatar, sx: i32) -> Buf {
    let mut buf = [0; PIXELS];
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
    put_all(&mut buf, outline(&ring.union(&glass).copied().collect()), a.outline);
    put_all(&mut buf, glass.iter().copied(), GLASS);
    put(&mut buf, (19 + sx, 14), 0xCCFFFFFF);
    put_all(&mut buf, ring.iter().copied(), RING);
    outlined(&mut buf, [(23 + sx, 18), (24 + sx, 19)], HANDLE, a.outline);
    buf
}

fn draw_laptop(a: &Avatar) -> Buf {
    let mut buf = [0; PIXELS];
    let lid: BTreeSet<Cell> = (14..22).flat_map(|x| (15..21).map(move |y| (x, y))).collect();
    let base_row: BTreeSet<Cell> = (12..23).map(|x| (x, 21)).collect();
    put_all(&mut buf, outline(&lid.union(&base_row).copied().collect()), a.outline);
    put_all(&mut buf, lid.iter().copied(), LID);
    put_all(&mut buf, (14..22).map(|x| (x, 15)), LID_HI);
    put_all(&mut buf, (15..21).flat_map(|x| (16..20).map(move |y| (x, y))), LID_IN);
    put_all(&mut buf, base_row.iter().copied(), BASE);
    put_all(&mut buf, [(21, 21), (22, 21)], BASE_SH);
    buf
}

/// The prompt on the laptop's screen, in the glow layer, with its cursor if on.
fn draw_screen(a: &Avatar, cursor: bool) -> Buf {
    let mut buf = [0; PIXELS];
    put_all(&mut buf, [(16, 16), (17, 17), (16, 18)], a.eye);
    if cursor {
        put_all(&mut buf, [(18, 18), (19, 18)], a.eye);
    }
    buf
}

fn draw_dust(t: f64) -> Buf {
    let mut buf = [0; PIXELS];
    for (i, &d) in DUST.iter().enumerate() {
        if ((t * 8.0) as i32).wrapping_add(i as i32) % 3 != 0 {
            put(&mut buf, d, DUSTC);
        }
    }
    buf
}

fn draw_dots(a: &Avatar, t: f64) -> Buf {
    let mut buf = [0; PIXELS];
    let n = (t * 3.0) as i32 % 4;
    let dots = [(20, 5), (22, 3), (24, 1)];
    for i in 0..n {
        outlined(&mut buf, [dots[i as usize]], DOT, a.outline);
    }
    buf
}

fn draw_notes(a: &Avatar, t: f64) -> Buf {
    let mut buf = [0; PIXELS];
    let acc = a.accent | 0xFF000000;
    for i in 0..2 {
        let ph = (t * 0.45 + i as f64 * 0.5) % 1.0;
        let bx = (if i == 0 { 19 } else { 2 }) + (ph * if i == 0 { 3.0 } else { -2.0 }).round_ties_even() as i32;
        let by = 7 - (ph * 7.0).round_ties_even() as i32;
        let color = if ph > 0.7 { (acc & 0x00FFFFFF) | 0x88000000 } else { acc };
        put_all(&mut buf, NOTE.iter().map(|n| (bx + n.0, by + n.1)), color);
    }
    buf
}

fn draw_zs(t: f64) -> Buf {
    let mut buf = [0; PIXELS];
    for i in 0..3 {
        let ph = (t * 0.35 + i as f64 / 3.0) % 1.0;
        let bx = 19 + (ph * 4.0).round_ties_even() as i32;
        let by = 6 - (ph * 6.0).round_ties_even() as i32;
        put_all(
            &mut buf,
            ZGLYPH.iter().map(|g| (bx + g.0, by + g.1)),
            if ph > 0.75 { 0x99E3E8FF } else { ZZ },
        );
    }
    buf
}

fn draw_alert(a: &Avatar) -> Buf {
    let mut buf = [0; PIXELS];
    outlined(&mut buf, [(24, 1), (24, 2), (24, 3), (24, 5)], BANG, a.outline);
    buf
}

fn draw_sparkles(t: f64) -> Buf {
    let mut buf = [0; PIXELS];
    let on = (t * 4.0) as i32 % 2 == 1;
    for (x, y) in if on { [(3, 6), (22, 3)] } else { [(2, 9), (23, 6)] } {
        put_all(&mut buf, [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)], SPARK);
        put(&mut buf, (x, y), SPARK_HI);
    }
    buf
}

/// What `with` adds to or changes on `without`: its pixels where they differ, transparent elsewhere.
fn diff(with: &Buf, without: &Buf) -> Buf {
    let mut buf = [0; PIXELS];
    for i in 0..PIXELS {
        if with[i] != without[i] {
            buf[i] = with[i];
        }
    }
    buf
}

/// `draw` sampled every millisecond over `ms` milliseconds, as frames with their durations: a new frame wherever
/// the pixels change. A sample is taken in the middle of its millisecond. `ms` is a whole number of the animation's
/// periods, so a loop of it stays in step with the pet clock however long the pet runs.
fn sampled(ms: u32, draw: impl Fn(f64) -> Buf) -> Vec<(Buf, u32)> {
    let mut frames: Vec<(Buf, u32)> = Vec::new();
    for m in 0..ms {
        let buf = draw((f64::from(m) + 0.5) / 1000.0);
        match frames.last_mut() {
            Some((last, d)) if *last == buf => *d += 1,
            _ => frames.push((buf, 1)),
        }
    }
    frames
}

/// One value's frames (each with its duration), how they loop, and by which clock.
type Value = (String, Vec<(Buf, u32)>, LoopMode, Clock);

/// A layer being made: its frames per value, before they go on its sheet.
struct Making {
    layer: Layer,
    values: Vec<Value>,
}

impl Making {
    fn new(name: &str, channel: Channel, default: &str) -> Making {
        let mut layer = Layer::new(name, channel, default);
        layer.hit = matches!(
            channel,
            Channel::State | Channel::Headphones | Channel::Arms | Channel::Feet
        );
        Making {
            layer,
            values: Vec::new(),
        }
    }

    /// A theme changes this layer's colours, though it is an effects layer.
    fn themed(mut self) -> Making {
        self.layer.themed = true;
        self
    }

    fn still(mut self, value: &str, buf: Buf) -> Making {
        self.values
            .push((value.to_owned(), vec![(buf, 1000)], LoopMode::Once, Clock::Change));
        self
    }

    fn timed(mut self, value: &str, frames: Vec<(Buf, u32)>) -> Making {
        self.values.push((value.to_owned(), frames, LoopMode::Loop, Clock::Pet));
        self
    }

    /// Values that go to another of the layer's values.
    fn falls(mut self, pairs: &[(&str, &str)]) -> Making {
        for (from, to) in pairs {
            self.layer.fallback.insert((*from).to_owned(), Some((*to).to_owned()));
        }
        self
    }

    /// Values that draw nothing on this layer.
    fn empty_for(mut self, values: &[&str]) -> Making {
        for v in values {
            self.layer.fallback.insert((*v).to_owned(), None);
        }
        self
    }

    /// The layer, with its frames laid out on a sheet of their own (index `sheet`).
    fn finish(mut self, sheet: usize) -> (Layer, Sheet) {
        let count: usize = self.values.iter().map(|v| v.1.len()).sum();
        let cols = count.clamp(1, ROW);
        let rows = count.div_ceil(ROW).max(1);
        let (w, h) = (cols * GW as usize, rows * GH as usize);
        let mut pixels = vec![0; w * h];
        let mut n = 0;
        for (value, frames, mode, clock) in self.values {
            let mut list = Vec::new();
            for (buf, ms) in frames {
                let (fx, fy) = ((n % ROW) * GW as usize, (n / ROW) * GH as usize);
                for y in 0..GH as usize {
                    let row = &buf[y * GW as usize..(y + 1) * GW as usize];
                    pixels[(fy + y) * w + fx..(fy + y) * w + fx + GW as usize].copy_from_slice(row);
                }
                list.push(Frame {
                    art: Art::Raster {
                        sheet,
                        x: fx as u32,
                        y: fy as u32,
                        w: GW as u32,
                        h: GH as u32,
                        pivot: (0.0, 0.0),
                    },
                    ms,
                });
                n += 1;
            }
            self.layer.animations.insert(
                value,
                Animation {
                    frames: list,
                    mode,
                    clock,
                },
            );
        }
        let sheet = Sheet {
            width: w as u32,
            height: h as u32,
            pixels,
        };
        (self.layer, sheet)
    }
}

/// The arm values a pose names, besides the running ones.
const ARM_VALUES: [&str; 6] = ["rest", "raised", "wave-a", "wave-b", "hold", "lens"];
const FEET_VALUES: [&str; 4] = ["stand", "tap", "dangle-a", "dangle-b"];
const FX_VALUES: [&str; 6] = ["none", "zs", "dots", "notes", "alert", "dust"];

/// `avatar` as a layered pixel pack: the renderer's every layer, once.
pub fn bake(a: &Avatar) -> Baked {
    let g = Geometry::of(a);
    let base = draw_body(a, &g, &[], Headphones::Off);
    let limb = |cells: &[Cell]| diff(&draw_body(a, &g, cells, Headphones::Off), &base);
    let hp_outline = draw_body(a, &g, &[], Headphones::Outline);
    let mut wave_a = ARM_L.to_vec();
    wave_a.extend_from_slice(WAVE_A);
    let mut wave_b = ARM_L.to_vec();
    wave_b.extend_from_slice(WAVE_B);
    let sparkles_of = |fx: &str| format!("{fx}+sparkles");

    let layers = vec![
        Making::new("body", Channel::State, "body").still("body", base),
        Making::new("headphones-outline", Channel::Headphones, "on")
            .still("on", diff(&hp_outline, &base))
            .empty_for(&["off"]),
        Making::new("arms", Channel::Arms, "rest")
            .still("rest", limb(ARMS_DOWN))
            .still("raised", limb(ARMS_UP))
            .still("wave-a", limb(&wave_a))
            .still("wave-b", limb(&wave_b))
            .falls(&[("hold", "rest"), ("lens", "rest")])
            .empty_for(&["run-a", "run-b"]),
        Making::new("arms-run", Channel::Arms, "run-a")
            .still("run-a", limb(RUN_ARMS[0]))
            .still("run-b", limb(RUN_ARMS[1]))
            .still("run-a@left", limb(&left(RUN_ARMS[0])))
            .still("run-b@left", limb(&left(RUN_ARMS[1])))
            .empty_for(&ARM_VALUES),
        Making::new("feet", Channel::Feet, "stand")
            .still("stand", limb(FEET))
            .still("tap", limb(FOOT_TAP))
            .still("dangle-a", limb(DANGLE_A))
            .still("dangle-b", limb(DANGLE_B))
            .empty_for(&["run-a", "run-b"]),
        Making::new("feet-run", Channel::Feet, "run-a")
            .still("run-a", limb(RUN_FEET[0]))
            .still("run-b", limb(RUN_FEET[1]))
            .still("run-a@left", limb(&left(RUN_FEET[0])))
            .still("run-b@left", limb(&left(RUN_FEET[1])))
            .empty_for(&FEET_VALUES),
        Making::new("headphones", Channel::Headphones, "on")
            .still("on", diff(&draw_body(a, &g, &[], Headphones::On), &hp_outline))
            .empty_for(&["off"]),
        Making::new("prop", Channel::Prop, "laptop")
            .still("laptop", draw_laptop(a))
            .timed(
                "lens",
                sampled(8000, |t| draw_lens(a, [0, 1, 0, -1][((t * 1.5) as i32 % 4) as usize])),
            )
            .empty_for(&["none"]),
        {
            let mut eyes = Making::new("eyes", Channel::Eyes, "open")
                .still("open", draw_eyes(a, "normal"))
                .still("closed", draw_eyes(a, "sleep"))
                .still("blink", draw_eyes(a, "blink"))
                .still("happy", draw_eyes(a, "happy"))
                .still("squint", draw_eyes(a, "squint"))
                .still("wide", draw_eyes(a, "wide"))
                .still("up", draw_eyes(a, "up"))
                .still("down", draw_eyes(a, "down"));
            eyes.layer.glow = true;
            eyes.layer.look = Look::ALL
                .into_iter()
                .map(|l| (l, (f64::from(l.dx) * 5.0, 0.0)))
                .collect();
            eyes
        },
        {
            let mut screen = Making::new("screen", Channel::Prop, "laptop")
                .timed("laptop", sampled(800, |t| draw_screen(a, (t * 2.5) as i32 % 2 == 1)))
                .empty_for(&["none", "lens"]);
            screen.layer.glow = true;
            screen
        },
        {
            let mut dust = Making::new("dust", Channel::Fx, "dust")
                .timed("dust", sampled(375, draw_dust))
                .falls(&[("dust+sparkles", "dust")])
                .empty_for(&["none", "zs", "dots", "notes", "alert", "sparkles"])
                .empty_for(
                    &["zs", "dots", "notes", "alert"]
                        .map(sparkles_of)
                        .each_ref()
                        .map(String::as_str),
                );
            dust.layer.mirror = Some(Mirror::Flip);
            dust
        },
        // the dots, notes and alert are drawn in the avatar's outline and accent: a theme changes them too
        Making::new("fx", Channel::Fx, "zs")
            .themed()
            .timed("zs", sampled(20_000, draw_zs))
            .timed("dots", sampled(4000, |t| draw_dots(a, t)))
            .timed("notes", sampled(20_000, |t| draw_notes(a, t)))
            .still("alert", draw_alert(a))
            .falls(&[
                ("zs+sparkles", "zs"),
                ("dots+sparkles", "dots"),
                ("notes+sparkles", "notes"),
                ("alert+sparkles", "alert"),
            ])
            .empty_for(&["none", "dust", "sparkles", "dust+sparkles"]),
        Making::new("sparkles", Channel::Fx, "sparkles")
            .timed("sparkles", sampled(500, draw_sparkles))
            .empty_for(&FX_VALUES),
    ];

    let mut sheets = Vec::new();
    let mut done = Vec::new();
    for (i, m) in layers.into_iter().enumerate() {
        let (layer, sheet) = m.finish(i);
        done.push(layer);
        sheets.push(sheet);
    }
    Baked {
        pack: Pack {
            name: a.name.clone(),
            kind: Kind::Pixel,
            pixel_size: 5.0,
            anchor: (13.0, 22.0),
            mirror: Mirror::None,
            faces: crate::pose::Facing::Right,
            accent: a.accent,
            glow: a.glow,
            layers: done,
            palette: palette(a),
        },
        sheets,
    }
}

/// An avatar's colours by their palette names, in the palette's order.
fn named(a: &Avatar) -> [(&'static str, u32); 15] {
    [
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
    ]
}

/// The baked pack's palette: each of the avatar's colours once, under the first name that has it, so that a theme
/// changing one name never changes another's pixels. The accent and the glow name theirs.
fn palette(a: &Avatar) -> Palette {
    let mut p = Palette::default();
    let mut by_colour: BTreeMap<u32, &str> = BTreeMap::new();
    for (name, c) in named(a) {
        if let std::collections::btree_map::Entry::Vacant(e) = by_colour.entry(c & 0x00FF_FFFF) {
            e.insert(name);
            p.colours.insert(name.to_owned(), c | 0xFF00_0000);
        }
    }
    let name_of = |c: u32| {
        let name = by_colour.get(&(c & 0x00FF_FFFF))?;
        (p.colours[*name] == c).then(|| (*name).to_owned())
    };
    let (accent, glow) = (name_of(a.accent), name_of(a.glow));
    Palette { accent, glow, ..p }
}

/// A theme named `name` for a pack baked from one avatar, from another avatar of the same drawing: its colours under
/// the names the palette keeps.
pub fn theme(palette: &Palette, name: &str, other: &Avatar) -> Theme {
    let colours = named(other)
        .into_iter()
        .filter(|(n, _)| palette.colours.contains_key(*n))
        .map(|(n, c)| (n.to_owned(), c | 0xFF00_0000))
        .collect();
    Theme::of_colours(name, colours)
}

/// The green Hood's avatar, as the repo's `avatars/hood-green.json` gives it.
pub fn hood_green() -> Avatar {
    Avatar::parse(include_str!("../../../../avatars/hood-green.json"), "hood-green")
        .expect("avatars/hood-green.json is an avatar")
}

/// The built-in pixel pets as the bake makes them, by pack folder: Sprout, and Hood in Blue with its Green theme
/// (`avatars/hood-green.json`).
pub fn built_in() -> Vec<(&'static str, Baked)> {
    let mut hood = bake(&Avatar::hood());
    hood.pack.palette.theme = "Blue".to_owned();
    let green = theme(&hood.pack.palette, "Green", &hood_green());
    hood.pack.palette.themes.push(green);
    vec![("sprout", bake(&Avatar::sprout())), ("hood", hood)]
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

/// Top-left-most body cells on each bump: little shine pixels for the "soft" style. The C# takes the first 3 after a
/// stable sort by row over cells added row-major, so ties on the row go to the smaller column.
fn find_spec(body: &BTreeSet<Cell>) -> Vec<Cell> {
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

/// The eye cells for `kind`, shifted `lx` columns.
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

/// Cells mirrored about the grid's centre, as a running limb facing left.
fn left(cells: &[Cell]) -> Vec<Cell> {
    cells.iter().map(|&(x, y)| (GW - 1 - x, y)).collect()
}

/// The layer names a baked pack has, in drawing order, and the buffer each goes to (0 body, 1 glow, 2 fx).
pub fn targets(pack: &Pack) -> BTreeMap<usize, usize> {
    pack.layers
        .iter()
        .enumerate()
        .map(|(i, l)| {
            (
                i,
                if l.channel == Channel::Fx {
                    2
                } else if l.glow {
                    1
                } else {
                    0
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xs(y: i32, xs: &[i32]) -> Vec<Cell> {
        xs.iter().map(|&x| (x, y)).collect()
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
        assert_eq!(left(DUST), vec![(21, 21), (23, 20), (25, 19)]);
        assert_eq!(left(&left(DUST)), DUST);
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

    #[test]
    fn eye_columns_wrap_without_panicking() {
        for lx in [i32::MAX, i32::MIN, i32::MAX - 12, i32::MIN + 8] {
            for kind in ["wide", "squint", "blink", "happy", "sleep", "up", "down", "normal"] {
                assert!(!eye_cells(kind, lx).is_empty());
            }
        }
    }

    #[test]
    fn a_baked_pack_validates_and_keeps_to_the_frame_limit() {
        for a in [Avatar::sprout(), Avatar::hood()] {
            let b = bake(&a);
            assert_eq!(b.pack.validate(), Ok(()), "{}", a.name);
            assert!(
                b.pack.frame_count() <= crate::pack::MAX_FRAMES,
                "{}: {}",
                a.name,
                b.pack.frame_count()
            );
            assert!(b.sheets.iter().all(|s| s.width <= 2048 && s.height <= 2048));
        }
    }

    #[test]
    fn a_sampled_animation_keeps_each_change_and_its_duration() {
        let frames = sampled(500, draw_sparkles);
        assert_eq!(frames.iter().map(|f| f.1).collect::<Vec<_>>(), [250, 250]);
        assert_ne!(frames[0].0, frames[1].0);
    }
}
