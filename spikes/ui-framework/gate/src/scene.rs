//! The gate's scene: the real pet window's worst case (R1), drawn on the CPU at device scale.
//!
//! - **The window:** 380 × 600 logical px (`aipet-ui` `geometry::SURFACE`).
//! - **Three cards:** a spread stack of three bubbles above the sprite, with long text cut to fit as `fit_card` does,
//!   drawn as `view.rs` draws a card: the drop shadow (0 4 14 #61000000), the plate, the dot (pulsing for a working
//!   or thinking card), the title and the detail.
//! - **The pet:** Sprout (pixel, the built-in pack) or Dewdrop (vector, `avatars/samples/skill-example`), idle or
//!   working with the laptop, its glow's halo, the ground shadow, and the body's breathing, squash and hop.
//!
//! The cards and their text are drawn once per scale into a static layer. Each frame only the dynamic items (the
//! sprite with its halo, the ground shadow, the pulsing dots) are redrawn, over the static layer restored in the rects
//! that changed; `frame` returns those rects in device px, and none when nothing changed.

use std::collections::HashMap;
use std::path::Path;

use aipet_sprite::compose::{self, Parts};
use aipet_sprite::load::LoadedPack;
use aipet_sprite::pack::{Art, Kind, Pack};
use aipet_sprite::pose::{ART_H, ART_W, Channel, HALO_PAD, Pose};
use aipet_sprite::vector::{self, View};
use aipet_sprite::{Pet, PetFrame, PetInput};
use tiny_skia::{
    Color, FillRule, FilterQuality, IntRect, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Rect, Transform,
};

use crate::text::{LINE_SPACING, Text};

pub const W: f32 = 380.0;
pub const H: f32 = 600.0;
const CENTER_X: f32 = W / 2.0;
/// The sprite's box at rest, and the point it squashes about (`geometry::SPRITE`, `SPRITE_ORIGIN`).
const SPRITE: (f32, f32, f32, f32) = (CENTER_X - 65.0, H - 8.0 - 124.0, 130.0, 120.0);
const ORIGIN: (f32, f32) = (65.0, 120.0 * 0.917);
const SHADOW: (f32, f32) = (76.0, 13.0);
const STACKS_BOTTOM: f32 = SPRITE.1 - 8.0;
const CARD_H: f32 = 50.0;
const CARD_RADIUS: f32 = 25.0;
const SPREAD: f32 = 58.0;
const DOT: (f32, f32) = (24.0, CARD_H / 2.0);
const TEXT_X: f32 = 41.0;
const TITLE_SIZE: f32 = 13.5;
const DETAIL_SIZE: f32 = 12.0;
const TITLE_Y: f32 = (CARD_H - LINE_SPACING * (TITLE_SIZE + DETAIL_SIZE) - 1.0) / 2.0;
const DETAIL_Y: f32 = TITLE_Y + LINE_SPACING * TITLE_SIZE + 1.0;
const TEXT_MAX: f32 = 250.0;
const CARD_MIN: f32 = 240.0;
const CARD_MAX: f32 = 350.0;
const SHADOW_BLUR: f32 = 14.0;
const SHADOW_Y: f32 = 4.0;

/// The three cards, bottom first: title, detail, state (its dot's colour and whether it pulses). The texts are the
/// iced pet's fixed gate board (`core_thread::gate_sources` on the spike branch): working, two chats and a review;
/// idle, three reviews. The gate draws them as one spread stack, every card's text showing, which is more than the
/// iced pet's folded stacks show.
const WORKING: [(&str, &str, &str); 3] = [
    (
        "Refactor the payment reconciliation service so the nightly batch stops timing out",
        "Claude Code CLI · Editing crates/reconcile/src/batch/scheduler.rs and its tests",
        "working",
    ),
    (
        "Why does the settlement export double-count refunds issued after midnight UTC",
        "Claude app · Reading the export pipeline and the refund ledger's history",
        "thinking",
    ),
    (
        "ABC-1423 · Migrate the customer notification templates to the new rendering engine",
        "In Review · PR notifications#882",
        "review",
    ),
];
const IDLE: [(&str, &str, &str); 3] = [
    (
        "ABC-1423 · Migrate the customer notification templates to the new rendering engine",
        "In Review · PR notifications#882",
        "review",
    ),
    (
        "ABC-1431 · Retry failed webhook deliveries with backoff instead of dropping them on the floor",
        "In Review",
        "review",
    ),
    (
        "ABC-1440 · Move the invoice PDF renderer off the request path and into the worker queue",
        "In Review",
        "review",
    ),
];

fn dot_colour(state: &str) -> u32 {
    match state {
        "thinking" => 0xFF7AA7FF,
        "working" => 0xFFE27A52,
        _ => 0xFF4C9AFF,
    }
}

/// A rect in device px.
pub type Damage = IntRect;

struct Card {
    /// Its body's top-left and width, in logical px.
    x: f32,
    y: f32,
    w: f32,
    title: String,
    detail: String,
    state: &'static str,
}

/// A raster frame (the picks of a pose) as images at device scale: the art and its halo.
struct Raster {
    art: Pixmap,
    halo: Option<Pixmap>,
}

pub struct Scene {
    pack: LoadedPack,
    pet: Pet,
    pub input: PetInput,
    frame: PetFrame,
    parts: Parts,
    cards: Vec<Card>,
    text: Text,
    /// Device px per logical px, and the static layer and the composed window at that scale.
    scale: f32,
    static_layer: Option<Pixmap>,
    master: Option<Pixmap>,
    ground: Pixmap,
    /// Raster frames by their picks; the vector art by its values and parts, with the scale it was drawn at.
    rasters: HashMap<String, Raster>,
    vector: Option<(String, Pixmap)>,
    /// What the dynamic items covered last frame, and what they drew (to tell an unchanged frame).
    last_rects: Vec<Damage>,
    last_key: String,
    /// The time the cards were born (the dots pulse from it).
    pub frames: u64,
}

/// The pet the case wears: `sprout` (the built-in pixel Sprout) or `dewdrop` (the vector sample).
pub fn pack(name: &str, repo: &Path) -> LoadedPack {
    match name {
        "dewdrop" => aipet_sprite::load::load_folder(&repo.join("avatars/samples/skill-example")).expect("Dewdrop"),
        _ => aipet_sprite::load::all(&repo.join("no-such-avatars-folder"))
            .pets
            .into_iter()
            .find(|p| p.name() == "Sprout" && p.pack.kind == Kind::Pixel)
            .expect("the built-in Sprout"),
    }
}

impl Scene {
    pub fn new(pack: LoadedPack, state: &str) -> Scene {
        let mut pet = Pet::new();
        let input = PetInput {
            state: state.to_owned(),
            prop: (state == "working").then(|| "laptop".to_owned()),
            moods: true,
            ..PetInput::default()
        };
        let frame = pet.update(&input, 0.0);
        let mut text = Text::new();
        // the spread stack, bottom first, each card as wide as its fitted text (`fit_card`)
        let frame_w = 14.0 + 9.0 + 2.0 + 18.0 + 8.0;
        let mut cards = Vec::new();
        let texts = if state == "working" { WORKING } else { IDLE };
        for (i, (title, detail, state)) in texts.iter().enumerate() {
            let (title, tw) = text.fit(title, TITLE_SIZE, true, TEXT_MAX);
            let (detail, dw) = text.fit(detail, DETAIL_SIZE, false, TEXT_MAX);
            let w = (frame_w + tw.max(dw).ceil()).clamp(CARD_MIN, CARD_MAX);
            let y = STACKS_BOTTOM - CARD_H - i as f32 * SPREAD;
            cards.push(Card {
                x: CENTER_X - w / 2.0,
                y,
                w,
                title,
                detail,
                state,
            });
        }
        Scene {
            pack,
            pet,
            input,
            frame,
            parts: Parts::default(),
            cards,
            text,
            scale: 0.0,
            static_layer: None,
            master: None,
            ground: ground_shadow(),
            rasters: HashMap::new(),
            vector: None,
            last_rects: Vec::new(),
            last_key: String::new(),
            frames: 0,
        }
    }

    /// The window in device px at `scale`.
    pub fn size(scale: f32) -> (u32, u32) {
        ((W * scale).round() as u32, (H * scale).round() as u32)
    }

    /// The composed window, premultiplied RGBA.
    pub fn master(&self) -> &Pixmap {
        self.master.as_ref().expect("a frame was drawn")
    }

    /// The clickable rects in logical px: the cards, and the sprite's box.
    pub fn input_rects(&self) -> Vec<(i32, i32, i32, i32)> {
        let mut out: Vec<_> = self
            .cards
            .iter()
            .map(|c| (c.x as i32, c.y as i32, c.w as i32, CARD_H as i32))
            .collect();
        let f = &self.frame;
        out.push((
            (SPRITE.0 + f.x as f32) as i32,
            (SPRITE.1 + f.y as f32) as i32,
            SPRITE.2 as i32,
            SPRITE.3 as i32,
        ));
        out
    }

    /// Moves the pet to `t` and redraws what changed at `scale`: the changed rects in device px (all of it after a
    /// scale change), none when nothing changed.
    pub fn frame(&mut self, t: f64, scale: f32) -> Vec<Damage> {
        self.frame = self.pet.update(&self.input, t);
        self.frames += 1;
        let (bw, bh) = Scene::size(scale);
        let rescaled = self.scale != scale || self.master.is_none();
        if rescaled {
            self.scale = scale;
            self.rasters.clear();
            self.vector = None;
            self.static_layer = Some(self.draw_static(bw, bh));
            self.master = self.static_layer.clone();
            self.last_rects.clear();
            self.last_key.clear();
        }
        let pose = *self.pet.pose();
        let (items, rects, key) = self.dynamic(&pose, t);
        if !rescaled && key == self.last_key {
            return Vec::new();
        }
        let mut damage: Vec<Damage> = if rescaled {
            vec![IntRect::from_xywh(0, 0, bw, bh).unwrap()]
        } else {
            self.last_rects.iter().chain(rects.iter()).copied().collect()
        };
        merge(&mut damage);
        // restore the static layer where anything changed, then draw the dynamic items over it
        let mut master = self.master.take().unwrap();
        let base = self.static_layer.as_ref().unwrap();
        for r in &damage {
            copy_rect(base, &mut master, *r);
        }
        let mut pm = master.as_mut();
        for item in items {
            item.draw(&mut pm, self);
        }
        self.master = Some(master);
        self.last_rects = rects;
        self.last_key = key;
        damage
    }

    /// The cards and their text at the current scale.
    fn draw_static(&mut self, bw: u32, bh: u32) -> Pixmap {
        let s = self.scale;
        let mut pm = Pixmap::new(bw, bh).unwrap();
        let cards: Vec<(f32, f32, f32, String, String, &str)> = self
            .cards
            .iter()
            .map(|c| (c.x, c.y, c.w, c.title.clone(), c.detail.clone(), c.state))
            .collect();
        for (x, y, w, title, detail, state) in cards {
            // the drop shadow: the plate's shape blurred, offset down
            let shadow = card_shadow(w, s);
            let pad = SHADOW_BLUR * 1.5;
            pm.draw_pixmap(
                ((x - pad) * s).round() as i32,
                ((y + SHADOW_Y - pad) * s).round() as i32,
                shadow.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
            let t = Transform::from_scale(s, s);
            let plate = rounded(x, y, w, CARD_H, CARD_RADIUS);
            let mut paint = Paint {
                anti_alias: true,
                ..Paint::default()
            };
            paint.set_color(argb(0xF5252528));
            pm.fill_path(&plate, &paint, FillRule::Winding, t, None);
            paint.set_color(argb(0x26FFFFFF));
            pm.stroke_path(&plate, &paint, &tiny_skia::Stroke { width: 1.0, ..Default::default() }, t, None);
            if !pulses(state) {
                disc(&mut pm.as_mut(), (x + DOT.0) * s, (y + DOT.1) * s, 8.0 * s, argb(dot_colour(state)));
            }
            let mut m = pm.as_mut();
            self.text
                .draw(&mut m, &title, TITLE_SIZE, true, s, (x + TEXT_X) * s, (y + TITLE_Y) * s, [255, 255, 255, 255]);
            self.text
                .draw(&mut m, &detail, DETAIL_SIZE, false, s, (x + TEXT_X) * s, (y + DETAIL_Y) * s, [0xA8, 0xA8, 0xAE, 255]);
        }
        pm
    }

    /// This frame's dynamic items, the device rects they cover, and a key that is equal when they draw the same.
    fn dynamic(&mut self, pose: &Pose, t: f64) -> (Vec<Item>, Vec<Damage>, String) {
        let s = self.scale;
        let f = self.frame.clone();
        let mut items = Vec::new();
        let mut key = String::new();
        // the pulsing dots
        for c in &self.cards {
            if pulses(c.state) {
                let phase = (t % 1.3 / 1.3) as f32;
                let grow = 1.0 - (1.0 - phase).powi(3);
                let (cx, cy) = ((c.x + DOT.0) * s, (c.y + DOT.1) * s);
                items.push(Item::Dot {
                    cx,
                    cy,
                    ring: 8.0 * (1.0 + 1.6 * grow) * s,
                    ring_alpha: 0.55 * (1.0 - phase),
                    colour: dot_colour(c.state),
                });
                key.push_str(&format!("dot{:.0}{:.2};", 8.0 * (1.0 + 1.6 * grow) * s, phase));
            }
        }
        // the ground shadow: its size and opacity follow the hop
        let (sw, sh) = (SHADOW.0 * f.shadow_scale as f32, SHADOW.1 * f.shadow_scale as f32);
        let (scx, scy) = (CENTER_X, SPRITE.1 + 103.0 + SHADOW.1 / 2.0);
        items.push(Item::Ground {
            rect: Rect::from_xywh((scx - sw / 2.0) * s, (scy - sh / 2.0) * s, sw * s, sh * s).unwrap(),
            opacity: f.shadow_opacity as f32,
        });
        key.push_str(&format!("g{:.3}{:.3};", f.shadow_scale, f.shadow_opacity));
        // the sprite: squash and stretch about its origin, then the hop
        let pack = &self.pack.pack;
        let ps = pixel_size(pack, s);
        let motion = Transform::from_translate(
            (SPRITE.0 + ORIGIN.0 + f.x as f32) * s,
            (SPRITE.1 + ORIGIN.1 + f.y as f32) * s,
        )
        .pre_scale(f.scale_x as f32, f.scale_y as f32);
        let art_x = -(pack.anchor.0 as f32) * ps * s;
        let art_y = -(pack.anchor.1 as f32) * ps * s;
        key.push_str(&format!("m{:.2}{:.2}{:.4}{:.4};", f.x, f.y, f.scale_x, f.scale_y));
        match pack.kind {
            Kind::Vector => {
                let mut drawn = *pose;
                drawn.motion = Pose::scripted(aipet_sprite::pose::Values::IDLE, 0.0).motion;
                let placed = self.parts.place(pack, &drawn, t);
                let vkey = format!("{:?}{:?}", Channel::ALL.map(|c| drawn.key(c)), placed);
                if self.vector.as_ref().is_none_or(|(k, _)| *k != vkey) {
                    let items_v = vector::scene(pack, &placed, &drawn);
                    let view = View::of_box(f64::from(ps * s));
                    let rgba = vector::render(&items_v, view, |_| true);
                    let pm = Pixmap::from_vec(rgba, tiny_skia::IntSize::from_wh(view.w, view.h).unwrap()).unwrap();
                    self.vector = Some((vkey.clone(), pm));
                }
                key.push_str(&vkey);
                let pad = HALO_PAD as f32 * ps * s;
                items.push(Item::Image {
                    transform: motion.pre_translate(art_x - pad, art_y - pad),
                    which: Which::Vector,
                    nearest: false,
                });
            }
            _ => {
                let picks = compose::pick(pack, pose, t);
                let rkey: String = picks
                    .iter()
                    .map(|p| format!("{}:{}:{:?}:{};", p.layer, p.value, p.frame, p.mirrored))
                    .collect();
                if !self.rasters.contains_key(&rkey) {
                    let raster = self.raster(pose, t, ps * s);
                    self.rasters.insert(rkey.clone(), raster);
                }
                key.push_str(&rkey);
                let nearest = pack.kind == Kind::Pixel;
                let pad = HALO_PAD as f32 * s;
                if self.rasters[&rkey].halo.is_some() {
                    items.push(Item::Image {
                        transform: motion.pre_translate(art_x - pad, art_y - pad),
                        which: Which::Halo(rkey.clone()),
                        nearest: false,
                    });
                }
                items.push(Item::Image {
                    transform: motion.pre_translate(art_x, art_y),
                    which: Which::Raster(rkey),
                    nearest,
                });
            }
        }
        let (bw, bh) = Scene::size(s);
        let rects = items
            .iter()
            .filter_map(|i| i.bounds(self))
            .filter_map(|r| r.intersect(&IntRect::from_xywh(0, 0, bw, bh).unwrap()))
            .collect();
        (items, rects, key)
    }

    /// A raster pack's frame for `pose`, composed at the pack's resolution and scaled to `px` device px per art px,
    /// with the halo of its glowing layers.
    fn raster(&self, pose: &Pose, t: f64, px: f32) -> Raster {
        let pack = &self.pack.pack;
        let (w, h, all) = flatten_where(pack, &self.pack.sheets, pose, t, |_| true);
        let art = scaled(&all, w, h, px);
        let glows = pack.layers.iter().any(|l| l.glow);
        let halo = glows.then(|| {
            let (_, _, glow) = flatten_where(pack, &self.pack.sheets, pose, t, |l| l.glow);
            halo(&glow, w as usize, h as usize, px.round().max(1.0) as usize, pack.glow, self.scale)
        });
        Raster { art, halo }
    }
}

enum Which {
    Raster(String),
    Halo(String),
    Vector,
}

enum Item {
    Dot {
        cx: f32,
        cy: f32,
        ring: f32,
        ring_alpha: f32,
        colour: u32,
    },
    Ground {
        rect: Rect,
        opacity: f32,
    },
    Image {
        transform: Transform,
        which: Which,
        nearest: bool,
    },
}

impl Item {
    fn image<'a>(&self, scene: &'a Scene) -> Option<&'a Pixmap> {
        match self {
            Item::Image { which, .. } => match which {
                Which::Raster(k) => Some(&scene.rasters[k].art),
                Which::Halo(k) => scene.rasters[k].halo.as_ref(),
                Which::Vector => scene.vector.as_ref().map(|(_, p)| p),
            },
            _ => None,
        }
    }

    fn bounds(&self, scene: &Scene) -> Option<Damage> {
        let r = match self {
            Item::Dot { cx, cy, ring, .. } => Rect::from_xywh(cx - ring / 2.0, cy - ring / 2.0, *ring, *ring)?,
            Item::Ground { rect, .. } => *rect,
            Item::Image { transform, .. } => {
                let p = self.image(scene)?;
                Rect::from_xywh(0.0, 0.0, p.width() as f32, p.height() as f32)?.transform(*transform)?
            }
        };
        IntRect::from_ltrb(
            r.left().floor() as i32 - 1,
            r.top().floor() as i32 - 1,
            r.right().ceil() as i32 + 1,
            r.bottom().ceil() as i32 + 1,
        )
    }

    fn draw(&self, pm: &mut PixmapMut, scene: &Scene) {
        match self {
            Item::Dot {
                cx,
                cy,
                ring,
                ring_alpha,
                colour,
            } => {
                let c = argb(*colour);
                let mut ringc = c;
                ringc.set_alpha(*ring_alpha);
                disc(pm, *cx, *cy, *ring, ringc);
                disc(pm, *cx, *cy, 8.0 * scene.scale, c);
            }
            Item::Ground { rect, opacity } => {
                let t = Transform::from_row(
                    rect.width() / scene.ground.width() as f32,
                    0.0,
                    0.0,
                    rect.height() / scene.ground.height() as f32,
                    rect.x(),
                    rect.y(),
                );
                let paint = PixmapPaint {
                    opacity: *opacity,
                    quality: FilterQuality::Bilinear,
                    ..PixmapPaint::default()
                };
                pm.draw_pixmap(0, 0, scene.ground.as_ref(), &paint, t, None);
            }
            Item::Image { transform, nearest, .. } => {
                if let Some(p) = self.image(scene) {
                    let paint = PixmapPaint {
                        quality: if *nearest { FilterQuality::Nearest } else { FilterQuality::Bilinear },
                        ..PixmapPaint::default()
                    };
                    pm.draw_pixmap(0, 0, p.as_ref(), &paint, *transform, None);
                }
            }
        }
    }
}

fn pulses(state: &str) -> bool {
    matches!(state, "working" | "thinking")
}

/// A pixel pack's art px is a whole number of device px; painted and vector art keep their size (`sprite::pixel_size`).
fn pixel_size(pack: &Pack, scale: f32) -> f32 {
    let ps = pack.pixel_size as f32;
    match pack.kind {
        Kind::Pixel => (ps * scale).round().max(1.0) / scale,
        _ => ps,
    }
}

/// `compose::flatten` for the layers `keep` takes: the pose's frames over each other at the pack's resolution,
/// premultiplied 0xAARRGGBB.
fn flatten_where(
    pack: &Pack,
    sheets: &[aipet_sprite::bake::Sheet],
    pose: &Pose,
    t: f64,
    keep: impl Fn(&aipet_sprite::pack::Layer) -> bool,
) -> (u32, u32, Vec<u32>) {
    let (w, h) = ((ART_W / pack.pixel_size).round() as i64, (ART_H / pack.pixel_size).round() as i64);
    let mut out = vec![0u32; (w * h) as usize];
    let offsets = Parts::default().raster_offsets(pack, pose, t);
    for p in compose::pick(pack, pose, t) {
        let layer = &pack.layers[p.layer];
        if !keep(layer) {
            continue;
        }
        let Some(Art::Raster {
            sheet,
            x,
            y,
            w: fw,
            h: fh,
            pivot,
        }) = p.frame.map(|f| layer.animations[p.value].frames[f].art.clone())
        else {
            continue;
        };
        let (dx, dy) = offsets[p.layer];
        let ox = (dx / pack.pixel_size).round() as i64 - pivot.0 as i64;
        let oy = (dy / pack.pixel_size).round() as i64 - pivot.1 as i64;
        let s = &sheets[sheet];
        for fy in 0..i64::from(fh) {
            for fx in 0..i64::from(fw) {
                let src = s.pixels[((i64::from(y) + fy) * i64::from(s.width) + i64::from(x) + fx) as usize];
                let cx = if p.mirrored { w - 1 - (fx + ox) } else { fx + ox };
                let cy = fy + oy;
                if src >> 24 == 0 || !(0..w).contains(&cx) || !(0..h).contains(&cy) {
                    continue;
                }
                let dst = &mut out[(cy * w + cx) as usize];
                let top = aipet_sprite::pm(src);
                let a = top >> 24;
                let mut px = 0;
                for shift in [0, 8, 16, 24] {
                    let c = ((top >> shift) & 0xFF) + ((*dst >> shift) & 0xFF) * (255 - a) / 255;
                    px |= c.min(255) << shift;
                }
                *dst = px;
            }
        }
    }
    (w as u32, h as u32, out)
}

/// Premultiplied 0xAARRGGBB art as a pixmap `px` device px per art px, nearest: drawn once per frame image.
fn scaled(argb: &[u32], w: u32, h: u32, px: f32) -> Pixmap {
    let (sw, sh) = ((w as f32 * px).round() as u32, (h as f32 * px).round() as u32);
    let mut src = Pixmap::new(w, h).unwrap();
    for (i, c) in argb.iter().enumerate() {
        let d = &mut src.data_mut()[i * 4..i * 4 + 4];
        d.copy_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, *c as u8, (c >> 24) as u8]);
    }
    let mut out = Pixmap::new(sw.max(1), sh.max(1)).unwrap();
    let paint = PixmapPaint {
        quality: FilterQuality::Nearest,
        ..PixmapPaint::default()
    };
    out.draw_pixmap(0, 0, src.as_ref(), &paint, Transform::from_scale(sw as f32 / w as f32, sh as f32 / h as f32), None);
    out
}

/// The glow's halo (`sprite::halo`): the glowing pixels blurred and tinted, `HALO_PAD` past the art on each side, at
/// device scale.
fn halo(frame: &[u32], w: usize, h: usize, px: usize, colour: u32, scale: f32) -> Pixmap {
    let pad = (HALO_PAD as f32 * scale).round() as usize;
    let (hw, hh) = (w * px + 2 * pad, h * px + 2 * pad);
    let mut alpha = vec![0f32; hw * hh];
    for (i, &p) in frame.iter().enumerate() {
        let a = (p >> 24) as f32 / 255.0;
        if a == 0.0 {
            continue;
        }
        let (fx, fy) = (i % w, i / w);
        for y in 0..px {
            let row = (pad + fy * px + y) * hw + pad + fx * px;
            alpha[row..row + px].fill(a);
        }
    }
    let radius = (3.0 * scale).round().max(1.0) as usize;
    for _ in 0..3 {
        box_blur(&mut alpha, hw, hh, radius);
    }
    let strength = ((colour >> 24) as f32 * 0.9).trunc() / 255.0;
    let mut pm = Pixmap::new(hw as u32, hh as u32).unwrap();
    let [r, g, b] = [(colour >> 16) as f32, (colour >> 8 & 0xFF) as f32, (colour & 0xFF) as f32];
    for (i, a) in alpha.iter().enumerate() {
        let a = (a * strength).clamp(0.0, 1.0);
        pm.data_mut()[i * 4..i * 4 + 4].copy_from_slice(&[
            (r * a) as u8,
            (g * a) as u8,
            (b * a) as u8,
            (a * 255.0).round() as u8,
        ]);
    }
    pm
}

/// A box blur of radius `r` along the rows, then the columns.
fn box_blur(buf: &mut [f32], w: usize, h: usize, r: usize) {
    let norm = 1.0 / (2 * r + 1) as f32;
    let mut line = Vec::new();
    for (len, step, lines, line_step) in [(w, 1, h, w), (h, w, w, 1)] {
        for l in 0..lines {
            let at = |i: usize| l * line_step + i * step;
            line.clear();
            line.extend((0..len).map(|i| buf[at(i)]));
            let mut sum: f32 = line[..r.min(len)].iter().sum();
            for i in 0..len {
                if i + r < len {
                    sum += line[i + r];
                }
                buf[at(i)] = sum * norm;
                if i >= r {
                    sum -= line[i - r];
                }
            }
        }
    }
}

/// A card's drop shadow at device scale: its plate's shape blurred, #61000000.
fn card_shadow(w: f32, s: f32) -> Pixmap {
    let pad = SHADOW_BLUR * 1.5;
    let (pw, ph) = (((w + 2.0 * pad) * s).ceil() as usize, ((CARD_H + 2.0 * pad) * s).ceil() as usize);
    let mut mask = Pixmap::new(pw as u32, ph as u32).unwrap();
    let mut paint = Paint::default();
    paint.set_color(Color::BLACK);
    mask.fill_path(&rounded(pad, pad, w, CARD_H, CARD_RADIUS), &paint, FillRule::Winding, Transform::from_scale(s, s), None);
    let mut alpha: Vec<f32> = mask.data().chunks(4).map(|p| f32::from(p[3]) / 255.0).collect();
    let r = ((SHADOW_BLUR / 2.0 / 1.7) * s).round().max(1.0) as usize;
    for _ in 0..3 {
        box_blur(&mut alpha, pw, ph, r);
    }
    let mut out = Pixmap::new(pw as u32, ph as u32).unwrap();
    for (i, a) in alpha.iter().enumerate() {
        out.data_mut()[i * 4 + 3] = (a * f32::from(0x61u8)).round() as u8;
    }
    out
}

/// The ground shadow's soft ellipse (`sprite::shadow_image`), 4 × its logical size.
fn ground_shadow() -> Pixmap {
    let (w, h) = ((SHADOW.0 * 4.0) as u32, (SHADOW.1 * 4.0) as u32);
    let mut pm = Pixmap::new(w, h).unwrap();
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / h as f32 * 2.0 - 1.0;
            let r = (dx * dx + dy * dy).sqrt();
            let a = if r < 0.55 { 102.0 - 51.0 * r / 0.55 } else { 51.0 * (1.0 - r).max(0.0) / 0.45 };
            let i = ((y * w + x) * 4 + 3) as usize;
            pm.data_mut()[i] = a.round() as u8;
        }
    }
    pm
}

fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let r = r.min(w / 2.0).min(h / 2.0);
    let k = 0.552_284_8 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}

fn disc(pm: &mut PixmapMut, cx: f32, cy: f32, d: f32, c: Color) {
    let Some(r) = Rect::from_xywh(cx - d / 2.0, cy - d / 2.0, d, d) else {
        return;
    };
    let Some(path) = PathBuilder::from_oval(r) else {
        return;
    };
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    paint.set_color(c);
    pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
}

fn argb(c: u32) -> Color {
    Color::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8)
}

/// Copies `r` of `from` into `to` (same size).
fn copy_rect(from: &Pixmap, to: &mut Pixmap, r: IntRect) {
    let w = from.width() as usize;
    let (x0, x1) = (r.x() as usize * 4, (r.x() as usize + r.width() as usize) * 4);
    for y in r.y() as usize..r.y() as usize + r.height() as usize {
        let at = y * w * 4;
        to.data_mut()[at + x0..at + x1].copy_from_slice(&from.data()[at + x0..at + x1]);
    }
}

/// Merges rects that overlap or touch, so a region is copied once.
pub fn merge(rects: &mut Vec<Damage>) {
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                let (a, b) = (rects[i], rects[j]);
                let touch = a.left() <= b.right() && b.left() <= a.right() && a.top() <= b.bottom() && b.top() <= a.bottom();
                if touch {
                    rects[i] = IntRect::from_ltrb(
                        a.left().min(b.left()),
                        a.top().min(b.top()),
                        a.right().max(b.right()),
                        a.bottom().max(b.bottom()),
                    )
                    .unwrap();
                    rects.swap_remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
    }
}
