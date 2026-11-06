//! A CPU renderer for rive-rs on tiny-skia 0.11: `rive_rs::renderer::Renderer` plus its Path, Paint, Gradient,
//! Image and Buffer types. It draws into one premultiplied RGBA `Pixmap`.

use std::cell::OnceCell;

use rive_rs::path::{Commands, FillRule, Verb};
use rive_rs::renderer::{self, BlendMode, BufferFlags, BufferType, Color, PaintStyle, StrokeCap, StrokeJoin};
use tiny_skia as sk;

fn ts(t: &[f32; 6]) -> sk::Transform {
    // rive: x' = t0 x + t2 y + t4, y' = t1 x + t3 y + t5
    sk::Transform::from_row(t[0], t[1], t[2], t[3], t[4], t[5])
}

fn color(c: Color) -> sk::Color {
    sk::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn blend(b: BlendMode) -> sk::BlendMode {
    match b {
        BlendMode::SrcOver => sk::BlendMode::SourceOver,
        BlendMode::Screen => sk::BlendMode::Screen,
        BlendMode::Overlay => sk::BlendMode::Overlay,
        BlendMode::Darken => sk::BlendMode::Darken,
        BlendMode::Lighten => sk::BlendMode::Lighten,
        BlendMode::ColorDodge => sk::BlendMode::ColorDodge,
        BlendMode::ColorBurn => sk::BlendMode::ColorBurn,
        BlendMode::HardLight => sk::BlendMode::HardLight,
        BlendMode::SoftLight => sk::BlendMode::SoftLight,
        BlendMode::Difference => sk::BlendMode::Difference,
        BlendMode::Exclusion => sk::BlendMode::Exclusion,
        BlendMode::Multiply => sk::BlendMode::Multiply,
        BlendMode::Hue => sk::BlendMode::Hue,
        BlendMode::Saturation => sk::BlendMode::Saturation,
        BlendMode::Color => sk::BlendMode::Color,
        BlendMode::Luminosity => sk::BlendMode::Luminosity,
    }
}

/// SPIKE_TRACE=1 prints every draw.
fn trace() -> bool {
    static T: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *T.get_or_init(|| std::env::var_os("SPIKE_TRACE").is_some())
}

// ---- Buffer (vertex, uv and index data for image meshes) ----

pub struct Buffer(Vec<u8>);

impl renderer::Buffer for Buffer {
    fn new(_: BufferType, _: BufferFlags, len_in_bytes: usize) -> Self {
        Buffer(vec![0; len_in_bytes])
    }
    fn map(&mut self) -> &mut [u8] {
        &mut self.0
    }
    fn unmap(&mut self) {}
}

impl Buffer {
    fn f32_pairs(&self) -> Vec<[f32; 2]> {
        self.0
            .chunks_exact(8)
            .map(|c| {
                [
                    f32::from_ne_bytes([c[0], c[1], c[2], c[3]]),
                    f32::from_ne_bytes([c[4], c[5], c[6], c[7]]),
                ]
            })
            .collect()
    }
    fn u16s(&self) -> Vec<u16> {
        self.0.chunks_exact(2).map(|c| u16::from_ne_bytes([c[0], c[1]])).collect()
    }
}

// ---- Path ----

/// A mutable path; the built tiny-skia path is cached until the next change.
#[derive(Default)]
pub struct Path {
    builder: sk::PathBuilder,
    fill: Option<FillRule>,
    built: OnceCell<Option<sk::Path>>,
}

impl Path {
    fn changed(&mut self) {
        self.built = OnceCell::new();
    }
    fn get(&self) -> Option<&sk::Path> {
        self.built.get_or_init(|| self.builder.clone().finish()).as_ref()
    }
    fn fill_rule(&self) -> sk::FillRule {
        match self.fill {
            Some(FillRule::EvenOdd) => sk::FillRule::EvenOdd,
            _ => sk::FillRule::Winding,
        }
    }
}

impl renderer::Path for Path {
    fn new(commands: &mut Commands, fill_rule: FillRule) -> Self {
        let mut path = Path::default();
        for (verb, p) in commands {
            match verb {
                Verb::Move => path.builder.move_to(p[0].x, p[0].y),
                Verb::Line => path.builder.line_to(p[0].x, p[0].y),
                Verb::Cubic => path.builder.cubic_to(p[0].x, p[0].y, p[1].x, p[1].y, p[2].x, p[2].y),
                Verb::Close => path.builder.close(),
            }
        }
        path.fill = Some(fill_rule);
        path
    }
    fn reset(&mut self) {
        self.builder.clear();
        self.changed();
    }
    fn extend(&mut self, from: &Self, transform: &[f32; 6]) {
        if let Some(p) = from.get().and_then(|p| p.clone().transform(ts(transform))) {
            self.builder.push_path(&p);
            self.changed();
        }
    }
    fn set_fill_rule(&mut self, fill_rule: FillRule) {
        self.fill = Some(fill_rule);
    }
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(x, y);
        self.changed();
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(x, y);
        self.changed();
    }
    fn cubic_to(&mut self, ox: f32, oy: f32, ix: f32, iy: f32, x: f32, y: f32) {
        self.builder.cubic_to(ox, oy, ix, iy, x, y);
        self.changed();
    }
    fn close(&mut self) {
        self.builder.close();
        self.changed();
    }
}

// ---- Gradient and Paint ----

#[derive(Clone)]
pub enum Gradient {
    Linear { start: sk::Point, end: sk::Point, stops: Vec<sk::GradientStop>, last: sk::Color },
    Radial { center: sk::Point, radius: f32, stops: Vec<sk::GradientStop>, last: sk::Color },
}

fn stops(colors: &[Color], stops: &[f32]) -> (Vec<sk::GradientStop>, sk::Color) {
    let s = colors.iter().zip(stops).map(|(c, &p)| sk::GradientStop::new(p, color(*c))).collect();
    (s, colors.last().map(|c| color(*c)).unwrap_or(sk::Color::TRANSPARENT))
}

impl renderer::Gradient for Gradient {
    fn new_linear(sx: f32, sy: f32, ex: f32, ey: f32, colors: &[Color], s: &[f32]) -> Self {
        let (stops, last) = stops(colors, s);
        Gradient::Linear { start: sk::Point::from_xy(sx, sy), end: sk::Point::from_xy(ex, ey), stops, last }
    }
    fn new_radial(cx: f32, cy: f32, radius: f32, colors: &[Color], s: &[f32]) -> Self {
        let (stops, last) = stops(colors, s);
        Gradient::Radial { center: sk::Point::from_xy(cx, cy), radius, stops, last }
    }
}

impl Gradient {
    fn shader(&self) -> sk::Shader<'static> {
        let pad = sk::SpreadMode::Pad;
        let id = sk::Transform::identity();
        match self {
            Gradient::Linear { start, end, stops, last } => {
                sk::LinearGradient::new(*start, *end, stops.clone(), pad, id).unwrap_or(sk::Shader::SolidColor(*last))
            }
            Gradient::Radial { center, radius, stops, last } => {
                sk::RadialGradient::new(*center, *center, *radius, stops.clone(), pad, id)
                    .unwrap_or(sk::Shader::SolidColor(*last))
            }
        }
    }
}

pub struct Paint {
    stroke: bool,
    color: sk::Color,
    gradient: Option<Gradient>,
    thickness: f32,
    join: sk::LineJoin,
    cap: sk::LineCap,
    blend: sk::BlendMode,
}

impl Default for Paint {
    fn default() -> Self {
        Paint {
            stroke: false,
            color: sk::Color::BLACK,
            gradient: None,
            thickness: 1.0,
            join: sk::LineJoin::Miter,
            cap: sk::LineCap::Butt,
            blend: sk::BlendMode::SourceOver,
        }
    }
}

impl renderer::Paint for Paint {
    type Gradient = Gradient;
    fn set_style(&mut self, style: PaintStyle) {
        self.stroke = style == PaintStyle::Stroke;
    }
    fn set_color(&mut self, c: Color) {
        self.color = color(c);
        self.gradient = None;
    }
    fn set_thickness(&mut self, thickness: f32) {
        self.thickness = thickness;
    }
    fn set_join(&mut self, join: StrokeJoin) {
        self.join = match join {
            StrokeJoin::Miter => sk::LineJoin::Miter,
            StrokeJoin::Round => sk::LineJoin::Round,
            StrokeJoin::Bevel => sk::LineJoin::Bevel,
        };
    }
    fn set_cap(&mut self, cap: StrokeCap) {
        self.cap = match cap {
            StrokeCap::Butt => sk::LineCap::Butt,
            StrokeCap::Round => sk::LineCap::Round,
            StrokeCap::Square => sk::LineCap::Square,
        };
    }
    fn set_blend_mode(&mut self, b: BlendMode) {
        self.blend = blend(b);
    }
    fn set_gradient(&mut self, gradient: &Gradient) {
        self.gradient = Some(gradient.clone());
    }
    fn invalidate_stroke(&mut self) {}
}

// ---- Image ----

pub struct Image(sk::Pixmap);

impl renderer::Image for Image {
    /// PNG only (tiny-skia's decoder). JPEG and WebP images in a .riv, and broken PNGs, become a 1x1 transparent
    /// image. Never None: rive-rs turns None into a null image pointer that rive-cpp still draws, and
    /// `renderer_draw_image` then dereferences it (segfault, found by the fuzz step).
    fn decode(data: &[u8]) -> Option<Self> {
        Some(Image(sk::Pixmap::decode_png(data).unwrap_or_else(|_| sk::Pixmap::new(1, 1).unwrap())))
    }
}

// ---- Renderer ----

#[derive(Clone)]
struct State {
    transform: sk::Transform,
    clip: Option<sk::Mask>,
}

/// Counts what a frame drew, to note which features a file uses.
#[derive(Default, Debug, Clone, Copy)]
pub struct Stats {
    pub fills: u32,
    pub strokes: u32,
    pub gradients: u32,
    pub clips: u32,
    pub blends: u32,
    pub images: u32,
    pub mesh_triangles: u32,
}

pub struct Renderer {
    pub pixmap: sk::Pixmap,
    stack: Vec<State>,
    pending_clear: bool,
    pub stats: Stats,
}

impl Renderer {
    pub fn new(width: u32, height: u32) -> Self {
        Renderer {
            pixmap: sk::Pixmap::new(width, height).expect("pixmap size"),
            stack: vec![State { transform: sk::Transform::identity(), clip: None }],
            pending_clear: false,
            stats: Stats::default(),
        }
    }

    /// The next draw starts by clearing the frame. Nothing is cleared if the scene doesn't draw (idle).
    pub fn begin_frame(&mut self) {
        self.pending_clear = true;
        self.stats = Stats::default();
    }

    fn top(&mut self) -> &mut State {
        self.stack.last_mut().unwrap()
    }

    fn sk_paint(&self, paint: &Paint) -> sk::Paint<'static> {
        let mut p = sk::Paint::default();
        p.anti_alias = true;
        p.blend_mode = paint.blend;
        match &paint.gradient {
            Some(g) => p.shader = g.shader(),
            None => p.set_color(paint.color),
        }
        p
    }
}

impl renderer::Renderer for Renderer {
    type Buffer = Buffer;
    type Path = Path;
    type Paint = Paint;
    type Gradient = Gradient;
    type Image = Image;

    fn state_push(&mut self) {
        if std::mem::take(&mut self.pending_clear) {
            self.pixmap.fill(sk::Color::TRANSPARENT);
        }
        let top = self.top().clone();
        self.stack.push(top);
    }

    fn state_pop(&mut self) {
        self.stack.pop();
        if self.stack.is_empty() {
            self.stack.push(State { transform: sk::Transform::identity(), clip: None });
        }
    }

    fn transform(&mut self, t: &[f32; 6]) {
        let top = self.top();
        top.transform = top.transform.pre_concat(ts(t));
    }

    /// Clips intersect: with the clip of the enclosing state, and with earlier clips in the same state (a drawable
    /// with several clipping shapes calls this once per shape).
    fn set_clip(&mut self, path: &Path) {
        self.stats.clips += 1;
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let fill_rule = path.fill_rule();
        let state = self.stack.last_mut().unwrap();
        let transform = state.transform;
        let Some(p) = path.get() else {
            // an empty clip path hides everything after it
            state.clip = Some(sk::Mask::new(w, h).unwrap());
            return;
        };
        match &mut state.clip {
            Some(mask) => mask.intersect_path(p, fill_rule, true, transform),
            None => {
                let mut mask = sk::Mask::new(w, h).unwrap();
                mask.fill_path(p, fill_rule, true, transform);
                state.clip = Some(mask);
            }
        }
    }

    fn draw_path(&mut self, path: &Path, paint: &Paint) {
        let Some(p) = path.get() else { return };
        let skp = self.sk_paint(paint);
        if paint.gradient.is_some() {
            self.stats.gradients += 1;
        }
        if paint.blend != sk::BlendMode::SourceOver {
            self.stats.blends += 1;
        }
        let state = self.stack.last().unwrap();
        let (transform, clip) = (state.transform, state.clip.as_ref());
        if trace() {
            let b = p.clone().transform(transform).map(|p| p.bounds());
            eprintln!(
                "draw {} blend {:?} grad {} color {:?} clip {} bounds {:?}",
                if paint.stroke { "stroke" } else { "fill" },
                paint.blend,
                paint.gradient.is_some(),
                paint.color,
                clip.is_some(),
                b
            );
        }
        if paint.stroke {
            if paint.thickness <= 0.0 {
                return;
            }
            self.stats.strokes += 1;
            let stroke = sk::Stroke {
                width: paint.thickness,
                line_cap: paint.cap,
                line_join: paint.join,
                ..Default::default()
            };
            self.pixmap.stroke_path(p, &skp, &stroke, transform, clip);
        } else {
            self.stats.fills += 1;
            self.pixmap.fill_path(p, &skp, path.fill_rule(), transform, clip);
        }
    }

    fn draw_image(&mut self, image: &Image, blend_mode: BlendMode, opacity: f32) {
        self.stats.images += 1;
        let state = self.stack.last().unwrap();
        let img = &image.0;
        let transform = state.transform.pre_translate(img.width() as f32 * -0.5, img.height() as f32 * -0.5);
        let paint = sk::PixmapPaint { opacity, blend_mode: blend(blend_mode), quality: sk::FilterQuality::Bilinear };
        self.pixmap.draw_pixmap(0, 0, img.as_ref(), &paint, transform, state.clip.as_ref());
    }

    /// Each triangle is filled with the image as a pattern, mapped from its UVs. Triangle edges are anti-aliased
    /// separately, so faint seams can show between triangles (as in rive-rs's vello renderer).
    fn draw_image_mesh(
        &mut self,
        image: &Image,
        vertices: &Buffer,
        uvs: &Buffer,
        indices: &Buffer,
        blend_mode: BlendMode,
        opacity: f32,
    ) {
        let (verts, uvs, idx) = (vertices.f32_pairs(), uvs.f32_pairs(), indices.u16s());
        let img = &image.0;
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        let state = self.stack.last().unwrap().clone();
        for tri in idx.chunks_exact(3) {
            let get = |v: &Vec<[f32; 2]>, i: u16| v.get(i as usize).copied();
            let (Some(a), Some(b), Some(c)) = (get(&verts, tri[0]), get(&verts, tri[1]), get(&verts, tri[2])) else {
                continue;
            };
            let (Some(ua), Some(ub), Some(uc)) = (get(&uvs, tri[0]), get(&uvs, tri[1]), get(&uvs, tri[2])) else {
                continue;
            };
            let from = [[ua[0] * iw, ua[1] * ih], [ub[0] * iw, ub[1] * ih], [uc[0] * iw, uc[1] * ih]];
            let Some(map) = affine_between(from, [a, b, c]) else { continue };
            let mut pb = sk::PathBuilder::new();
            pb.move_to(a[0], a[1]);
            pb.line_to(b[0], b[1]);
            pb.line_to(c[0], c[1]);
            pb.close();
            let Some(path) = pb.finish() else { continue };
            self.stats.mesh_triangles += 1;
            let mut paint = sk::Paint::default();
            paint.anti_alias = true;
            paint.blend_mode = blend(blend_mode);
            paint.shader =
                sk::Pattern::new(img.as_ref(), sk::SpreadMode::Pad, sk::FilterQuality::Bilinear, opacity, map);
            self.pixmap.fill_path(&path, &paint, sk::FillRule::Winding, state.transform, state.clip.as_ref());
        }
    }
}

/// The affine transform that maps triangle `from` onto triangle `to`.
fn affine_between(from: [[f32; 2]; 3], to: [[f32; 2]; 3]) -> Option<sk::Transform> {
    // M * [x y 1]^T = [x' y']^T for the three points: solve via the inverse of the `from` basis.
    let f = sk::Transform::from_row(
        from[1][0] - from[0][0],
        from[1][1] - from[0][1],
        from[2][0] - from[0][0],
        from[2][1] - from[0][1],
        from[0][0],
        from[0][1],
    );
    let t = sk::Transform::from_row(
        to[1][0] - to[0][0],
        to[1][1] - to[0][1],
        to[2][0] - to[0][0],
        to[2][1] - to[0][1],
        to[0][0],
        to[0][1],
    );
    Some(t.pre_concat(f.invert()?))
}

/// Rows of non-transparent pixels as (x, y, length), as aipet-ui's `sprite::runs` takes them from a layer.
pub fn runs(pixmap: &sk::Pixmap, out: &mut Vec<(i32, i32, i32)>) {
    out.clear();
    let w = pixmap.width() as usize;
    for (y, row) in pixmap.data().chunks_exact(w * 4).enumerate() {
        let mut x = 0;
        while x < w {
            if row[x * 4 + 3] == 0 {
                x += 1;
                continue;
            }
            let start = x;
            while x < w && row[x * 4 + 3] != 0 {
                x += 1;
            }
            out.push((start as i32, y as i32, (x - start) as i32));
        }
    }
}
