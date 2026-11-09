//! What every prototype shares: Sprout from `aipet_sprite`, one bubble's box, the hit mask and the input rects.
//! Only the text and getting pixels on screen differ between the candidates.
//!
//! The scene is `W` x `H` logical pixels: the bubble at the top, the sprite (26 x 24 cells at x5) standing at the
//! bottom, with its shadow.

use std::time::Instant;

pub use tiny_skia;

use aipet_sprite::{GH, GW, P, Pet, PetFrame, PetInput};
use tiny_skia::{
    Color, FillRule, FilterQuality, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Rect, Stroke, Transform,
};

pub const W: u32 = 260;
pub const H: u32 = 230;
/// The bubble's box: x, y, width, height.
pub const BUBBLE: (f32, f32, f32, f32) = (20.0, 8.0, 220.0, 52.0);
pub const BUBBLE_TEXT: &str = "Claude is thinking about the plan";
pub const BUBBLE_TEXT_SIZE: f32 = 13.0;
/// Where the sprite's box (130 x 120) sits, unmoved.
pub const SPRITE: (f32, f32) = ((W as f32 - (GW * P) as f32) / 2.0, H as f32 - (GH * P) as f32 - 10.0);

pub struct Scene {
    pub pet: Pet,
    pub input: PetInput,
    start: Instant,
    pub frame: PetFrame,
    body: Pixmap,
    /// Frames drawn, for the logs.
    pub frames: u64,
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

impl Scene {
    /// Sprout, asleep (what a calm pet does), or in `SPIKE_STATE` (idle, thinking, working, ...).
    pub fn new() -> Self {
        let mut pet = Pet::new();
        let input = PetInput {
            state: std::env::var("SPIKE_STATE").unwrap_or_else(|_| "sleep".into()),
            ..PetInput::default()
        };
        let frame = pet.update(&input, 0.0);
        let mut s = Scene { pet, input, start: Instant::now(), frame, body: Pixmap::new(GW as u32, GH as u32).unwrap(), frames: 0 };
        s.copy_body();
        s
    }

    pub fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    /// Advance the pet to now. True when its art changed (24 fps); its motion changes every tick.
    pub fn tick(&mut self) -> bool {
        let t = self.now();
        self.frame = self.pet.update(&self.input, t);
        if self.frame.pixels_changed {
            self.copy_body();
        }
        self.frames += 1;
        self.frame.pixels_changed
    }

    fn copy_body(&mut self) {
        let [body, _, fx] = self.pet.layers();
        let out = self.body.data_mut();
        for (i, (&b, &f)) in body.iter().zip(fx.iter()).enumerate() {
            // fx over body, both premultiplied 0xAARRGGBB
            let fa = f >> 24;
            let mut px = 0u32;
            for shift in [0, 8, 16, 24] {
                let c = ((f >> shift) & 0xFF) + ((b >> shift) & 0xFF) * (255 - fa) / 255;
                px |= c.min(255) << shift;
            }
            out[i * 4] = (px >> 16) as u8;
            out[i * 4 + 1] = (px >> 8) as u8;
            out[i * 4 + 2] = px as u8;
            out[i * 4 + 3] = (px >> 24) as u8;
        }
    }

    /// The body as premultiplied RGBA, 26 x 24.
    pub fn body_rgba(&self) -> &[u8] {
        self.body.data()
    }

    /// The pointer, in scene coordinates, or None when it left.
    pub fn set_pointer(&mut self, at: Option<(f64, f64)>) {
        let over = at.filter(|&(x, y)| self.hit(x, y));
        self.input.hover = over.is_some();
        self.input.mouse = at.map(|(x, y)| (x - f64::from(SPRITE.0), y - f64::from(SPRITE.1)));
    }

    pub fn set_dragging(&mut self, on: bool) {
        let t = self.now();
        if self.input.dragging && !on {
            self.input.land_until = t + 0.35;
        }
        self.input.dragging = on;
        self.input.moved = on;
        self.input.last_move = t;
    }

    /// The sprite's transform from its 26 x 24 cells to the scene: squash and stretch about its feet, and the hop.
    pub fn sprite_transform(&self) -> Transform {
        let f = &self.frame;
        let feet_x = SPRITE.0 + (GW * P) as f32 / 2.0 + f.x as f32;
        let feet_y = SPRITE.1 + (GH * P) as f32 + f.y as f32;
        Transform::from_translate(feet_x, feet_y)
            .pre_scale(P as f32 * f.scale_x as f32, P as f32 * f.scale_y as f32)
            .pre_translate(-(GW as f32) / 2.0, -(GH as f32))
    }

    /// The shadow's ellipse: centre x, centre y, half width, half height, opacity.
    pub fn shadow(&self) -> (f32, f32, f32, f32, f32) {
        let f = &self.frame;
        (
            SPRITE.0 + (GW * P) as f32 / 2.0,
            SPRITE.1 + (GH * P) as f32 + 2.0,
            34.0 * f.shadow_scale as f32,
            5.0,
            0.22 * f.shadow_opacity as f32,
        )
    }

    /// Everything but the bubble's text, into a cleared `W*scale` x `H*scale` pixmap.
    pub fn draw(&self, pm: &mut PixmapMut, scale: f32) {
        pm.fill(Color::TRANSPARENT);
        let base = Transform::from_scale(scale, scale);
        let mut paint = Paint { anti_alias: true, ..Paint::default() };
        // bubble
        let (x, y, w, h) = BUBBLE;
        let path = rounded(x, y, w, h, 10.0);
        paint.set_color_rgba8(255, 255, 255, 245);
        pm.fill_path(&path, &paint, FillRule::Winding, base, None);
        paint.set_color_rgba8(120, 110, 200, 255);
        pm.stroke_path(&path, &paint, &Stroke { width: 1.5, ..Stroke::default() }, base, None);
        // shadow
        let (cx, cy, rx, ry, a) = self.shadow();
        if let Some(r) = Rect::from_ltrb(cx - rx, cy - ry, cx + rx, cy + ry) {
            if let Some(p) = PathBuilder::from_oval(r) {
                paint.set_color(Color::from_rgba(0.0, 0.0, 0.0, a).unwrap());
                pm.fill_path(&p, &paint, FillRule::Winding, base, None);
            }
        }
        // sprite, nearest neighbour: crisp cells at any scale
        let pp = PixmapPaint { quality: FilterQuality::Nearest, ..PixmapPaint::default() };
        pm.draw_pixmap(0, 0, self.body.as_ref(), &pp, base.pre_concat(self.sprite_transform()), None);
    }

    /// Whether a point of the scene takes clicks: the bubble's box, or a painted cell of the sprite.
    pub fn hit(&self, x: f64, y: f64) -> bool {
        let (bx, by, bw, bh) = BUBBLE;
        if x >= f64::from(bx) && x < f64::from(bx + bw) && y >= f64::from(by) && y < f64::from(by + bh) {
            return true;
        }
        let Some(inv) = self.sprite_transform().invert() else { return false };
        let mut p = [tiny_skia::Point::from_xy(x as f32, y as f32)];
        inv.map_points(&mut p);
        let (cx, cy) = (p[0].x.floor() as i32, p[0].y.floor() as i32);
        (0..GW).contains(&cx) && (0..GH).contains(&cy) && self.body.data()[((cy * GW + cx) * 4 + 3) as usize] != 0
    }

    /// The clickable area as rectangles in scene coordinates (x, y, w, h): the bubble, and one per row run of the
    /// sprite's painted cells (unsquashed: the hop moves them, squash is ignored), padded by 2.
    pub fn input_rects(&self) -> Vec<(i32, i32, i32, i32)> {
        let (bx, by, bw, bh) = BUBBLE;
        let mut out = vec![(bx as i32, by as i32, bw as i32, bh as i32)];
        let ox = SPRITE.0 as i32 + self.frame.x.round() as i32;
        let oy = SPRITE.1 as i32 + self.frame.y.round() as i32;
        let alpha: Vec<u32> = self.body.data().chunks(4).map(|c| u32::from(c[3]) << 24).collect();
        for (x, y, len) in aipet_sprite::compose::runs_argb(&alpha, GW as usize) {
            out.push((ox + x * P - 2, oy + y * P - 2, len * P + 4, P + 4));
        }
        out
    }
}

fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}
