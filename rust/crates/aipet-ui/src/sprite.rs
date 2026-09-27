//! The sprite's bitmaps: the pet's layers as images, the glow's blurred halo, and the ground shadow.
//!
//! `Handle::from_rgba` takes straight (not premultiplied) alpha and makes a new texture on every call, so layers are
//! converted from the pet's premultiplied pixels and turned into images only when their pixels actually change.

use aipet_sprite::{Avatar, GH, GW, PIXELS, Pet, PetFrame, PetInput, XorShift64};
use iced::widget::image::Handle;

use crate::geometry::HALO_PAD;

/// The halo's bitmap: the sprite's 130 × 120 plus [`HALO_PAD`] on each side, at 1 px per logical px.
pub const HALO_W: usize = GW as usize * 5 + 2 * HALO_PAD as usize;
pub const HALO_H: usize = GH as usize * 5 + 2 * HALO_PAD as usize;

/// The ground shadow's bitmap: 4 px per logical px of its 76 × 13, so it stays smooth when scaled.
const SHADOW_W: usize = 76 * 4;
const SHADOW_H: usize = 13 * 4;

/// The sprite as images, with the body's solid rows for hit testing.
pub struct Sprite {
    body: [u32; PIXELS],
    glow: [u32; PIXELS],
    fx: [u32; PIXELS],
    glow_colour: u32,
    /// The body layer.
    pub body_image: Handle,
    /// The glow and effects layers together: both are drawn over the halo.
    pub top_image: Handle,
    /// The glow's blurred halo, drawn between the body and the glow (a drop shadow sits under what casts it).
    pub halo_image: Handle,
    /// The body's rows of solid pixels: (x, y, length) in grid pixels.
    pub runs: Vec<(i32, i32, i32)>,
}

impl Sprite {
    pub fn new() -> Self {
        let empty = [0; PIXELS];
        Sprite {
            body: empty,
            glow: empty,
            fx: empty,
            glow_colour: 0,
            body_image: layer_image(&empty),
            top_image: layer_image(&empty),
            halo_image: Handle::from_rgba(HALO_W as u32, HALO_H as u32, halo(&empty, 0)),
            runs: Vec::new(),
        }
    }

    /// Takes a frame's pixels (call it when `pixels_changed`) and remakes the images whose pixels differ.
    pub fn update(&mut self, frame: &PetFrame, glow_colour: u32) {
        if self.body != *frame.body {
            self.body = *frame.body;
            self.body_image = layer_image(&self.body);
            self.runs = runs(&self.body);
        }
        let glow_changed = self.glow != *frame.glow || self.glow_colour != glow_colour;
        if glow_changed {
            self.glow = *frame.glow;
            self.glow_colour = glow_colour;
            self.halo_image = Handle::from_rgba(HALO_W as u32, HALO_H as u32, halo(&self.glow, glow_colour));
        }
        if glow_changed || self.fx != *frame.fx {
            self.fx = *frame.fx;
            self.top_image = layer_image(&over(&self.fx, &self.glow));
        }
    }
}

/// One avatar standing idle, all layers in one image (for Settings' avatar picker).
pub fn still(avatar: &Avatar) -> Handle {
    let mut pet = Pet::with_rng(XorShift64::new(1));
    pet.set_avatar(avatar.clone());
    let input = PetInput {
        state: "idle".to_owned(),
        ..PetInput::default()
    };
    let frame = pet.update(&input, 0.0);
    layer_image(&over(frame.fx, &over(frame.glow, frame.body)))
}

fn layer_image(px: &[u32; PIXELS]) -> Handle {
    Handle::from_rgba(GW as u32, GH as u32, straight_rgba(px))
}

/// Straight-alpha RGBA bytes from premultiplied 0xAARRGGBB pixels.
pub fn straight_rgba(px: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(px.len() * 4);
    for &p in px {
        let a = p >> 24;
        let unmul = |c: u32| {
            if a == 0 {
                0
            } else {
                ((c * 255 + a / 2) / a).min(255) as u8
            }
        };
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

/// The glow layer's halo, as Avalonia draws the C#'s `DropShadowDirectionEffect` (BlurRadius 10, Opacity 0.9): the
/// layer's alpha at 5 px per grid pixel, blurred with σ = 0.288675 × 10 + 0.5 ≈ 3.4 px (Skia's radius-to-sigma; here
/// three box blurs of radius 3, σ ≈ 3.46), in `colour` at 90 % of its alpha. Straight RGBA, [`HALO_W`] × [`HALO_H`].
pub fn halo(glow: &[u32; PIXELS], colour: u32) -> Vec<u8> {
    let pad = HALO_PAD as usize;
    let mut alpha = vec![0f32; HALO_W * HALO_H];
    for (i, &p) in glow.iter().enumerate() {
        let a = (p >> 24) as f32 / 255.0;
        if a == 0.0 {
            continue;
        }
        let (gx, gy) = (i % GW as usize, i / GW as usize);
        for y in 0..5 {
            let row = (pad + gy * 5 + y) * HALO_W + pad + gx * 5;
            alpha[row..row + 5].fill(a);
        }
    }
    let mut line = Vec::new();
    for _ in 0..3 {
        box_blur(&mut alpha, HALO_W, 1, HALO_H, HALO_W, &mut line);
        box_blur(&mut alpha, HALO_H, HALO_W, HALO_W, 1, &mut line);
    }
    // the colour everywhere (so linear filtering has no dark fringe), the blur in the alpha; Avalonia's shadow alpha
    // is the colour's alpha × opacity cast to a byte
    let strength = ((colour >> 24) as f32 * 0.9).trunc();
    let [r, g, b] = [(colour >> 16) as u8, (colour >> 8) as u8, colour as u8];
    alpha
        .iter()
        .flat_map(|&a| [r, g, b, (a * strength).round().clamp(0.0, 255.0) as u8])
        .collect()
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

/// The rows of solid (non-transparent) pixels of a layer, as (x, y, length) in grid pixels.
pub fn runs(layer: &[u32; PIXELS]) -> Vec<(i32, i32, i32)> {
    let mut runs = Vec::new();
    for (y, row) in layer.chunks(GW as usize).enumerate() {
        let mut x = 0;
        while x < row.len() {
            if row[x] >> 24 == 0 {
                x += 1;
                continue;
            }
            let start = x;
            while x < row.len() && row[x] >> 24 != 0 {
                x += 1;
            }
            runs.push((start as i32, y as i32, (x - start) as i32));
        }
    }
    runs
}

/// The ground shadow: the C#'s radial gradient, #66000000 in the middle, #33000000 at 55 %, transparent at the rim.
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

/// The shadow gradient's alpha at `r` (0 in the middle, 1 at the rim).
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
        let mut glow = [0u32; PIXELS];
        glow[(10 * GW + 12) as usize] = 0xFF7FEFFF; // one eye pixel at (12, 10)
        let rgba = halo(&glow, 0xFF3D9BFF);
        let alpha = |x: usize, y: usize| rgba[(y * HALO_W + x) * 4 + 3];
        // the pixel's middle, in the halo: pad + 12 × 5 + 2
        let (cx, cy) = (10 + 62, 10 + 52);
        assert!((50..70).contains(&alpha(cx, cy)), "{}", alpha(cx, cy));
        assert!(alpha(cx + 6, cy) > 0 && alpha(cx + 6, cy) < alpha(cx, cy));
        assert_eq!(
            alpha(cx + 12, cy),
            0,
            "three radius-3 passes reach 9 px past the 5 px pixel's edge"
        );
        assert_eq!(alpha(0, 0), 0);
        // tinted: every pixel carries the colour, the alpha does the fading
        assert_eq!(&rgba[..3], &[0x3D, 0x9B, 0xFF]);
        // a fully covered area comes out at 90 % of the colour's alpha
        let full = halo(&[0xFFFFFFFF; PIXELS], 0xFF000000);
        assert_eq!(full[((HALO_H / 2) * HALO_W + HALO_W / 2) * 4 + 3], 229);
    }

    #[test]
    fn runs_are_the_solid_stretches_of_each_row() {
        let mut layer = [0u32; PIXELS];
        for x in [3, 4, 5, 9] {
            layer[(2 * GW + x) as usize] = 0xFF000000;
        }
        layer[(23 * GW + 25) as usize] = 0x40000000;
        assert_eq!(runs(&layer), vec![(3, 2, 3), (9, 2, 1), (25, 23, 1)]);
    }

    #[test]
    fn shadow_gradient_matches_the_stops() {
        assert_eq!(shadow_alpha(0.0), 0x66);
        assert_eq!(shadow_alpha(0.55), 0x33);
        assert_eq!(shadow_alpha(1.0), 0);
        assert_eq!(shadow_alpha(1.4), 0);
    }
}
