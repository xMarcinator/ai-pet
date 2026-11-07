//! The sprite's bitmaps: the pack's frames as images, the glow's blurred halos, and the ground shadow.
//!
//! `Handle::from_rgba` makes a new texture on every call, so every frame of the pack (and its mirror image, and for
//! a glowing layer its halo) becomes an image once, when the pet puts the pack on ([`Art::new`]). Each redraw then
//! only picks which images show where ([`Sprite::update`]): a frame that doesn't change makes no new image.

use std::collections::BTreeMap;

use aipet_sprite::compose;
use aipet_sprite::load::LoadedPack;
use aipet_sprite::pack::{Art as Raster, Kind, Mirror, Pack};
use aipet_sprite::pose::{Channel, Pose};
use aipet_sprite::{GH, GW, PIXELS, Pet, PetInput, XorShift64};
use iced::widget::image::{FilterMethod, Handle};
use iced::{Point, Rectangle, Size};

use crate::geometry::{HALO_PAD, SPRITE_ORIGIN};
use crate::vector_view::VectorArt;

/// The ground shadow's bitmap: 4 px per logical px of its 76 × 13, so it stays smooth when scaled.
const SHADOW_W: usize = 76 * 4;
const SHADOW_H: usize = 13 * 4;

/// Where a layer is drawn: with the body, or in the glow (over its halo), or with the effects on top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Body,
    Glow,
    Fx,
}

/// One frame as images, with its rows of solid pixels for hit testing.
struct Frame {
    image: Handle,
    /// Its mirror image, for a layer that mirrors.
    flipped: Option<Handle>,
    /// Its glow's halo, and the halo's mirror image, for a layer that glows.
    halo: Option<(Handle, Option<Handle>)>,
    /// Rows of solid pixels: (x, y, length) in the frame's pixels.
    runs: Vec<(i32, i32, i32)>,
    w: u32,
    h: u32,
    pivot: (f64, f64),
    ms: u32,
}

/// A pack's every frame as images, made once when the pet puts it on.
pub struct Art {
    /// Per layer, per value, the frames.
    layers: Vec<BTreeMap<String, Vec<Frame>>>,
    /// How many images it made: a redraw makes none.
    pub made: usize,
}

/// Whether a layer of `pack` may be drawn mirrored.
fn mirrors(pack: &Pack, layer: usize) -> bool {
    pack.layers[layer].mirror.unwrap_or(pack.mirror) == Mirror::Flip
}

/// The frame's pixels from its sheet (straight 0xAARRGGBB), row-major `w` × `h`.
fn cut(p: &LoadedPack, sheet: usize, x: u32, y: u32, w: u32, h: u32) -> Vec<u32> {
    let s = &p.sheets[sheet];
    let mut px = Vec::with_capacity((w * h) as usize);
    for row in y..y + h {
        let at = (row * s.width + x) as usize;
        px.extend_from_slice(&s.pixels[at..at + w as usize]);
    }
    px
}

fn flip(px: &[u32], w: u32) -> Vec<u32> {
    px.chunks(w as usize)
        .flat_map(|row| row.iter().rev().copied())
        .collect()
}

/// Straight 0xAARRGGBB pixels as RGBA bytes.
fn rgba(px: &[u32]) -> Vec<u8> {
    px.iter()
        .flat_map(|&c| [(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8])
        .collect()
}

impl Art {
    pub fn new(p: &LoadedPack) -> Art {
        let pack = &p.pack;
        let mut made = 0;
        // a glow's halo is blurred at 1 px per logical px: an art pixel is this many
        let px = pack.pixel_size.round().max(1.0) as usize;
        let layers = pack
            .layers
            .iter()
            .enumerate()
            .map(|(i, layer)| {
                let flips = mirrors(pack, i);
                layer
                    .animations
                    .iter()
                    .map(|(value, a)| {
                        let frames = a
                            .frames
                            .iter()
                            .filter_map(|f| match f.art {
                                Raster::Raster {
                                    sheet,
                                    x,
                                    y,
                                    w,
                                    h,
                                    pivot,
                                } => Some((cut(p, sheet, x, y, w, h), w, h, pivot, f.ms)),
                                Raster::Vector => None,
                            })
                            .map(|(pixels, w, h, pivot, ms)| {
                                let image = Handle::from_rgba(w, h, rgba(&pixels));
                                let flipped = flips.then(|| Handle::from_rgba(w, h, rgba(&flip(&pixels, w))));
                                let halo = layer.glow.then(|| {
                                    let (hw, hh, bytes) = halo(&pixels, w as usize, h as usize, px, pack.glow);
                                    let flipped = flips.then(|| {
                                        let (_, _, b) = halo(&flip(&pixels, w), w as usize, h as usize, px, pack.glow);
                                        Handle::from_rgba(hw as u32, hh as u32, b)
                                    });
                                    (Handle::from_rgba(hw as u32, hh as u32, bytes), flipped)
                                });
                                made += 1
                                    + usize::from(flipped.is_some())
                                    + halo.as_ref().map_or(0, |h| 1 + usize::from(h.1.is_some()));
                                Frame {
                                    runs: runs(&pixels, w as usize),
                                    image,
                                    flipped,
                                    halo,
                                    w,
                                    h,
                                    pivot,
                                    ms,
                                }
                            })
                            .collect();
                        (value.clone(), frames)
                    })
                    .collect()
            })
            .collect();
        Art { layers, made }
    }
}

/// An image to draw this frame: where (in the sprite, before the body's motion), how, and with which halo.
#[derive(Clone, Debug)]
pub struct Shown {
    pub target: Target,
    pub image: Handle,
    pub rect: Rectangle,
    pub filter: FilterMethod,
    /// The halo, drawn [`HALO_PAD`] beyond the frame on each side.
    pub halo: Option<Handle>,
}

/// The pack's art size for a logical px at `scale` device px: a pixel pack's pixels are a whole number of device
/// px, so they stay crisp; painted and vector art keep their size.
pub fn pixel_size(pack: &Pack, scale: f32) -> f32 {
    let ps = pack.pixel_size as f32;
    match pack.kind {
        Kind::Pixel if scale > 0.0 => (ps * scale).round().max(1.0) / scale,
        _ => ps,
    }
}

/// What the sprite shows: the images of this redraw, and the art's hit rectangles.
pub struct Sprite {
    pub art: Art,
    /// The images to draw, in the pack's order.
    pub shown: Vec<Shown>,
    /// The hit layers' solid pixels, as rectangles in the sprite (before the body's motion).
    pub hits: Vec<Rectangle>,
    /// Whether an animation on screen moves faster than the calm frame rate shows (its typical frame is shorter).
    pub fast_ms: Option<u32>,
    /// A vector pack's drawing, and where its canvas goes in the sprite (before the body's motion).
    pub vector: Option<(VectorArt, Rectangle)>,
}

impl Sprite {
    pub fn new(p: &LoadedPack) -> Self {
        Sprite {
            art: Art::new(p),
            shown: Vec::new(),
            hits: Vec::new(),
            fast_ms: None,
            vector: (p.pack.kind == Kind::Vector).then(|| (VectorArt::default(), Rectangle::default())),
        }
    }

    /// Picks the frames `pose` shows at `t` and lays them out at `scale` device px per logical px.
    pub fn update(&mut self, pack: &Pack, pose: &Pose, t: f64, scale: f32) {
        self.shown.clear();
        self.hits.clear();
        self.fast_ms = None;
        let ps = pixel_size(pack, scale);
        let filter = match pack.kind {
            Kind::Pixel => FilterMethod::Nearest,
            Kind::Painted | Kind::Vector => FilterMethod::Linear,
        };
        // the art sits with its anchor where the sprite squashes about
        let origin = Point::new(
            SPRITE_ORIGIN.x - pack.anchor.0 as f32 * ps,
            SPRITE_ORIGIN.y - pack.anchor.1 as f32 * ps,
        );
        if let Some((art, rect)) = &mut self.vector {
            art.update(pack, pose, t);
            self.fast_ms = art.fast_ms;
            let pad = aipet_sprite::pose::HALO_PAD as f32;
            *rect = Rectangle::new(
                Point::new(origin.x - pad * ps, origin.y - pad * ps),
                Size::new(
                    (aipet_sprite::pose::ART_W as f32 + 2.0 * pad) * ps,
                    (aipet_sprite::pose::ART_H as f32 + 2.0 * pad) * ps,
                ),
            );
            for &(x, y, len) in &art.runs {
                self.hits.push(Rectangle::new(
                    Point::new(rect.x + x as f32 * ps, rect.y + y as f32 * ps),
                    Size::new(len as f32 * ps, ps),
                ));
            }
            return;
        }
        for pick in compose::pick(pack, pose, t) {
            let layer = &pack.layers[pick.layer];
            let Some(frames) = self.art.layers[pick.layer].get(pick.value) else {
                continue;
            };
            let Some(f) = pick.frame.and_then(|i| frames.get(i)) else {
                continue;
            };
            // a held animation (reduced motion) doesn't move
            if frames.len() > 1 && !pose.still {
                let mut ms: Vec<u32> = frames.iter().map(|f| f.ms).collect();
                ms.sort_unstable();
                let typical = ms[ms.len() / 2];
                self.fast_ms = Some(self.fast_ms.map_or(typical, |m| m.min(typical)));
            }
            let (dx, dy) = compose::raster_offset(pack, layer, pose);
            let at_x = (dx / pack.pixel_size).round() as f32 - f.pivot.0 as f32;
            let at_y = (dy / pack.pixel_size).round() as f32 - f.pivot.1 as f32;
            let mut rect = Rectangle::new(
                Point::new(origin.x + at_x * ps, origin.y + at_y * ps),
                Size::new(f.w as f32 * ps, f.h as f32 * ps),
            );
            let mirrored = pick.mirrored && f.flipped.is_some();
            if mirrored {
                // about the sprite's middle, as the art box is
                rect.x = 2.0 * SPRITE_ORIGIN.x - rect.x - rect.width;
            }
            let target = if layer.channel == Channel::Fx {
                Target::Fx
            } else if layer.glow {
                Target::Glow
            } else {
                Target::Body
            };
            let image = if mirrored { f.flipped.clone() } else { None }.unwrap_or_else(|| f.image.clone());
            let halo = f
                .halo
                .as_ref()
                .map(|(h, flipped)| if mirrored { flipped.clone() } else { None }.unwrap_or_else(|| h.clone()));
            if layer.hit {
                for &(x, y, len) in &f.runs {
                    let x = if mirrored { f.w as i32 - x - len } else { x };
                    self.hits.push(Rectangle::new(
                        Point::new(rect.x + x as f32 * ps, rect.y + y as f32 * ps),
                        Size::new(len as f32 * ps, ps),
                    ));
                }
            }
            self.shown.push(Shown {
                target,
                image,
                rect,
                filter,
                halo,
            });
        }
    }
}

/// One pet standing idle, all layers in one image (for Settings' avatar picker).
pub fn still(avatar: &LoadedPack) -> Handle {
    // a painted pack, or a pixel one on a grid other than the legacy 26 × 24, at its own resolution
    let grid = (aipet_sprite::pose::ART_W / avatar.pack.pixel_size).round() as i32;
    if avatar.pack.kind == Kind::Painted || (avatar.pack.kind == Kind::Pixel && grid != GW) {
        let pose = Pose::scripted(aipet_sprite::pose::Values::IDLE, 0.0);
        let (w, h, px) = compose::flatten(&avatar.pack, &avatar.sheets, &pose, 0.0);
        return Handle::from_rgba(w, h, straight_rgba(&px));
    }
    if avatar.pack.kind == Kind::Vector {
        // the picker shows 78 × 72: drawn at twice that, then scaled down
        let (w, h, px) = aipet_sprite::vector::still(&avatar.pack, 78.0 * 2.0 / aipet_sprite::pose::ART_W);
        let px: Vec<u32> = px
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&[r, g, b, a]| u32::from_be_bytes([a, r, g, b]))
            .collect();
        return Handle::from_rgba(w, h, straight_rgba(&px));
    }
    let mut pet = Pet::with_rng(XorShift64::new(1));
    pet.set_pack(avatar.clone());
    let input = PetInput {
        state: "idle".to_owned(),
        ..PetInput::default()
    };
    pet.update(&input, 0.0);
    let [body, glow, fx] = pet.layers();
    Handle::from_rgba(GW as u32, GH as u32, straight_rgba(&over(fx, &over(glow, body))))
}

/// Straight-alpha RGBA bytes from premultiplied 0xAARRGGBB pixels.
pub fn straight_rgba(px: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(px.len() * 4);
    for &p in px {
        let a = p >> 24;
        // a transparent pixel has no colour to recover
        let unmul = |c: u32| (c * 255 + a / 2).checked_div(a).map_or(0, |v| v.min(255) as u8);
        out.extend_from_slice(&[unmul((p >> 16) & 255), unmul((p >> 8) & 255), unmul(p & 255), a as u8]);
    }
    out
}

/// `top` over `bottom`, pixel by pixel (premultiplied source-over).
fn over(top: &[u32; PIXELS], bottom: &[u32; PIXELS]) -> [u32; PIXELS] {
    let mut out = [0; PIXELS];
    for ((o, &t), &b) in out.iter_mut().zip(top).zip(bottom) {
        let keep = 255 - (t >> 24);
        let channel = |shift: u32| {
            let c = ((t >> shift) & 255) + (((b >> shift) & 255) * keep + 127) / 255;
            c.min(255) << shift
        };
        *o = channel(24) | channel(16) | channel(8) | channel(0);
    }
    out
}

/// A glowing frame's halo, as Avalonia draws the C#'s `DropShadowDirectionEffect` (BlurRadius 10, Opacity 0.9): the
/// frame's alpha at `px` logical px per art pixel, [`HALO_PAD`] wider each side, blurred with σ = 0.288675 × 10 + 0.5
/// ≈ 3.4 px (Skia's radius-to-sigma; here three box blurs of radius 3, σ ≈ 3.46), in `colour` at 90 % of its alpha.
/// Straight RGBA; returns its width, height and bytes.
pub fn halo(frame: &[u32], w: usize, h: usize, px: usize, colour: u32) -> (usize, usize, Vec<u8>) {
    let pad = HALO_PAD as usize;
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
    let mut line = Vec::new();
    for _ in 0..3 {
        box_blur(&mut alpha, hw, 1, hh, hw, &mut line);
        box_blur(&mut alpha, hh, hw, hw, 1, &mut line);
    }
    // the colour everywhere (so linear filtering has no dark fringe), the blur in the alpha; Avalonia's shadow alpha
    // is the colour's alpha × opacity cast to a byte
    let strength = ((colour >> 24) as f32 * 0.9).trunc();
    let [r, g, b] = [(colour >> 16) as u8, (colour >> 8) as u8, colour as u8];
    let bytes = alpha
        .iter()
        .flat_map(|&a| [r, g, b, (a * strength).round().clamp(0.0, 255.0) as u8])
        .collect();
    (hw, hh, bytes)
}

/// Box-blurs `lines` lines of `len` values in place, radius 3; values past the ends count as 0. A line starts at
/// `i × line_step` and walks by `step`.
fn box_blur(buf: &mut [f32], len: usize, step: usize, lines: usize, line_step: usize, tmp: &mut Vec<f32>) {
    const R: usize = 3;
    let norm = 1.0 / (2 * R + 1) as f32;
    for l in 0..lines {
        let at = |i: usize| l * line_step + i * step;
        tmp.clear();
        tmp.extend((0..len).map(|i| buf[at(i)]));
        let mut sum: f32 = tmp[..R.min(len)].iter().sum();
        for i in 0..len {
            if i + R < len {
                sum += tmp[i + R];
            }
            buf[at(i)] = sum * norm;
            if i >= R {
                sum -= tmp[i - R];
            }
        }
    }
}

/// The rows of solid (non-transparent) pixels of a frame `w` pixels wide, as (x, y, length).
pub fn runs(frame: &[u32], w: usize) -> Vec<(i32, i32, i32)> {
    aipet_sprite::compose::runs_argb(frame, w)
}

/// The ground shadow's soft ellipse: straight RGBA, [`SHADOW_W`] × [`SHADOW_H`].
pub fn shadow_image() -> Handle {
    let mut px = Vec::with_capacity(SHADOW_W * SHADOW_H * 4);
    for y in 0..SHADOW_H {
        for x in 0..SHADOW_W {
            let dx = (x as f32 + 0.5) / SHADOW_W as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / SHADOW_H as f32 * 2.0 - 1.0;
            px.extend_from_slice(&[0, 0, 0, shadow_alpha((dx * dx + dy * dy).sqrt())]);
        }
    }
    Handle::from_rgba(SHADOW_W as u32, SHADOW_H as u32, px)
}

/// The shadow's gradient: #66 at the centre, #33 at 55 %, clear at the edge.
fn shadow_alpha(r: f32) -> u8 {
    let a = if r < 0.55 {
        0x66 as f32 + (0x33 - 0x66) as f32 * r / 0.55
    } else {
        0x33 as f32 * (1.0 - r).max(0.0) / 0.45
    };
    a.round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use aipet_sprite::Avatar;
    use aipet_sprite::pose::{Facing, Values};

    #[test]
    fn straight_alpha_undoes_premultiplying() {
        let px = [0xFF102030, 0x00000000, 0x80402010, aipet_sprite::pm(0xCCFFFFFF)];
        assert_eq!(
            straight_rgba(&px),
            vec![
                0x10, 0x20, 0x30, 0xFF, 0, 0, 0, 0, 0x80, 0x40, 0x20, 0x80, 0xFF, 0xFF, 0xFF, 0xCC
            ]
        );
    }

    #[test]
    fn over_keeps_opaque_tops_and_blends_see_through_ones() {
        let mut top = [0u32; PIXELS];
        let mut bottom = [0u32; PIXELS];
        top[0] = 0xFF00FF00;
        bottom[0] = 0xFFFF0000;
        top[1] = 0x80800000; // half-transparent red, premultiplied
        bottom[1] = 0xFF0000FF;
        bottom[2] = 0xFF123456;
        let out = over(&top, &bottom);
        assert_eq!(out[0], 0xFF00FF00);
        assert_eq!(out[1], 0xFF80007F);
        assert_eq!(out[2], 0xFF123456);
        assert_eq!(out[3], 0);
    }

    #[test]
    fn halo_is_a_soft_tinted_blur_of_the_glow() {
        let (w, h) = (GW as usize, GH as usize);
        let mut glow = vec![0u32; w * h];
        glow[10 * w + 12] = 0xFF7FEFFF; // one eye pixel at (12, 10)
        let (hw, _, rgba) = halo(&glow, w, h, 5, 0xFF3D9BFF);
        let alpha = |x: usize, y: usize| rgba[(y * hw + x) * 4 + 3];
        let (cx, cy) = (10 + 62, 10 + 52);
        assert!((50..70).contains(&alpha(cx, cy)), "{}", alpha(cx, cy));
        assert!(alpha(cx + 6, cy) > 0 && alpha(cx + 6, cy) < alpha(cx, cy));
        assert_eq!(alpha(cx + 12, cy), 0, "the blur reaches 9 px");
        let (hw, hh, full) = halo(&vec![0xFFFFFFFF; w * h], w, h, 5, 0xFF000000);
        assert_eq!(full[((hh / 2) * hw + hw / 2) * 4 + 3], 229);
    }

    #[test]
    fn runs_are_the_solid_stretches_of_each_row() {
        let mut layer = vec![0u32; PIXELS];
        for x in [3, 4, 5, 9] {
            layer[(2 * GW + x) as usize] = 0xFF000000;
        }
        layer[(23 * GW + 25) as usize] = 0x40000000;
        assert_eq!(runs(&layer, GW as usize), vec![(3, 2, 3), (9, 2, 1), (25, 23, 1)]);
    }

    #[test]
    fn shadow_gradient_matches_the_stops() {
        assert_eq!(shadow_alpha(0.0), 0x66);
        assert_eq!(shadow_alpha(0.55), 0x33);
        assert_eq!(shadow_alpha(1.0), 0);
        assert_eq!(shadow_alpha(1.4), 0);
    }

    fn sprout() -> LoadedPack {
        LoadedPack::of_avatar(&Avatar::sprout())
    }

    #[test]
    fn every_frame_is_an_image_once_and_a_redraw_makes_none() {
        let p = sprout();
        let mut sprite = Sprite::new(&p);
        let made = sprite.art.made;
        assert!(made >= p.pack.frame_count(), "{made}");
        let pose = Pose::scripted(Values::IDLE, 0.0);
        sprite.update(&p.pack, &pose, 0.0, 1.0);
        let shown = sprite.shown.len();
        sprite.update(&p.pack, &pose, 0.5, 1.0);
        assert_eq!((sprite.art.made, sprite.shown.len()), (made, shown));
        // the baked Sprout's body is drawn in the sprite's 130 × 120 box at scale 1
        let body = &sprite.shown[0];
        assert_eq!(
            (body.target, body.rect),
            (
                Target::Body,
                Rectangle::new(Point::new(0.0, SPRITE_ORIGIN.y - 110.0), Size::new(130.0, 120.0))
            )
        );
        assert!(!sprite.hits.is_empty());
    }

    #[test]
    fn a_pixel_pack_snaps_its_pixels_to_device_pixels() {
        let p = sprout();
        for (scale, device) in [(1.0, 5.0), (1.25, 6.0), (1.5, 8.0), (2.0, 10.0)] {
            let ps = pixel_size(&p.pack, scale);
            assert_eq!(ps * scale, device, "at {scale}");
        }
    }

    #[test]
    fn a_mirrored_layer_uses_its_mirror_image_about_the_middle() {
        let p = sprout();
        let mut sprite = Sprite::new(&p);
        let left = Values {
            facing: Facing::Left,
            fx: aipet_sprite::pose::Fx::Dust,
            ..Values::IDLE
        };
        sprite.update(&p.pack, &Pose::scripted(left, 0.0), 0.0, 1.0);
        let dust = sprite
            .shown
            .iter()
            .find(|s| s.target == Target::Fx)
            .expect("the dust shows");
        assert_eq!(dust.rect.x, 130.0 - dust.rect.width - 0.0);
    }
}
