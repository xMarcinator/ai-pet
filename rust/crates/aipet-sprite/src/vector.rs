//! Vector packs' drawing (R8): path data in SVG `d` syntax, parsed at load into segments; colours; the scene a pose
//! draws (each part's shapes with where they go); and a rasteriser with tiny-skia, for the hit mask, the avatar
//! picker's still and the painted bake.
//!
//! Nothing here reads a file or a URL: a shape is its path data and its colours, and anything else (an SVG file, a
//! `url(…)` paint) is not a shape and skips the pack.

use crate::compose::Placed;
use crate::pack::{COORD_LIMIT, Layer, Pack, Part, Shape};
use crate::pose::{ART_H, ART_W, HALO_PAD, Pose, Values};

/// A path segment, in absolute coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Move(f64, f64),
    Line(f64, f64),
    Quad(f64, f64, f64, f64),
    Cubic(f64, f64, f64, f64, f64, f64),
    Close,
}

/// How a stroke's ends are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cap {
    #[default]
    Butt,
    Round,
    Square,
}

/// How a stroke's corners are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Join {
    #[default]
    Miter,
    Round,
    Bevel,
}

impl Cap {
    pub fn parse(s: &str) -> Result<Cap, String> {
        Ok(match s {
            "butt" => Cap::Butt,
            "round" => Cap::Round,
            "square" => Cap::Square,
            _ => return Err(format!("unknown linecap {s:?} (butt, round or square)")),
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Cap::Butt => "butt",
            Cap::Round => "round",
            Cap::Square => "square",
        }
    }
}

impl Join {
    pub fn parse(s: &str) -> Result<Join, String> {
        Ok(match s {
            "miter" => Join::Miter,
            "round" => Join::Round,
            "bevel" => Join::Bevel,
            _ => return Err(format!("unknown linejoin {s:?} (miter, round or bevel)")),
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Join::Miter => "miter",
            Join::Round => "round",
            Join::Bevel => "bevel",
        }
    }
}

/// A colour: `#RGB`, `#RRGGBB` or `#RRGGBBAA` as straight 0xAARRGGBB, or `none` for no paint. Anything else, a
/// `url(…)` paint among them, is refused: a vector pack loads nothing from outside itself.
pub fn colour(s: &str) -> Result<Option<u32>, String> {
    if s == "none" {
        return Ok(None);
    }
    let hex = s
        .strip_prefix('#')
        .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| format!("colour {s:?} is not #RGB, #RRGGBB, #RRGGBBAA or none"))?;
    let n = |h: &str| u32::from_str_radix(h, 16).unwrap_or(0);
    Ok(Some(match hex.len() {
        3 => {
            let d = |i: usize| n(&hex[i..=i]) * 0x11;
            0xFF00_0000 | d(0) << 16 | d(1) << 8 | d(2)
        }
        6 => 0xFF00_0000 | n(hex),
        8 => n(&hex[6..8]) << 24 | n(&hex[..6]),
        _ => return Err(format!("colour {s:?} is not #RGB, #RRGGBB, #RRGGBBAA or none")),
    }))
}

/// The colour as a manifest writes it.
pub fn colour_name(c: Option<u32>) -> String {
    match c {
        None => "none".into(),
        Some(c) if c >> 24 == 0xFF => format!("#{:06X}", c & 0xFF_FFFF),
        Some(c) => format!("#{:06X}{:02X}", c & 0xFF_FFFF, c >> 24),
    }
}

/// Reads path data: the commands `M L H V C S Q T A Z`, absolute and relative, with implicit repeats. Numbers are
/// finite and within [`COORD_LIMIT`]; anything that isn't path data is an error.
pub fn parse(d: &str) -> Result<Vec<Seg>, String> {
    let mut p = Reader { s: d.as_bytes(), i: 0 };
    let mut out = Vec::new();
    let (mut cur, mut start) = ((0.0, 0.0), (0.0, 0.0));
    // the last control point, for S and T
    let mut last_cubic: Option<(f64, f64)> = None;
    let mut last_quad: Option<(f64, f64)> = None;
    let mut cmd: Option<u8> = None;
    loop {
        p.skip();
        if p.i >= p.s.len() {
            break;
        }
        let c = p.s[p.i];
        if c.is_ascii_alphabetic() {
            if !b"MmLlHhVvCcSsQqTtAaZz".contains(&c) {
                return Err(format!("path data: unknown command {:?}", c as char));
            }
            p.i += 1;
            cmd = Some(c);
        } else if cmd.is_none() {
            return Err("path data must start with a command".into());
        }
        let c = cmd.unwrap_or(b'M');
        let rel = c.is_ascii_lowercase();
        let base = if rel { cur } else { (0.0, 0.0) };
        let pt = |p: &mut Reader| -> Result<(f64, f64), String> { Ok((base.0 + p.num()?, base.1 + p.num()?)) };
        let (mut cubic, mut quad) = (None, None);
        match c.to_ascii_uppercase() {
            b'M' => {
                cur = pt(&mut p)?;
                start = cur;
                out.push(Seg::Move(cur.0, cur.1));
                // later pairs are lines
                cmd = Some(if rel { b'l' } else { b'L' });
            }
            b'L' => {
                cur = pt(&mut p)?;
                out.push(Seg::Line(cur.0, cur.1));
            }
            b'H' => {
                cur.0 = base.0 + p.num()?;
                out.push(Seg::Line(cur.0, cur.1));
            }
            b'V' => {
                cur.1 = base.1 + p.num()?;
                out.push(Seg::Line(cur.0, cur.1));
            }
            b'C' | b'S' => {
                let c1 = if c.eq_ignore_ascii_case(&b'C') {
                    pt(&mut p)?
                } else {
                    last_cubic.map_or(cur, |l| (2.0 * cur.0 - l.0, 2.0 * cur.1 - l.1))
                };
                let c2 = pt(&mut p)?;
                let to = pt(&mut p)?;
                out.push(Seg::Cubic(c1.0, c1.1, c2.0, c2.1, to.0, to.1));
                cubic = Some(c2);
                cur = to;
            }
            b'Q' | b'T' => {
                let q = if c.eq_ignore_ascii_case(&b'Q') {
                    pt(&mut p)?
                } else {
                    last_quad.map_or(cur, |l| (2.0 * cur.0 - l.0, 2.0 * cur.1 - l.1))
                };
                let to = pt(&mut p)?;
                out.push(Seg::Quad(q.0, q.1, to.0, to.1));
                quad = Some(q);
                cur = to;
            }
            b'A' => {
                let (rx, ry, rot) = (p.num()?, p.num()?, p.num()?);
                let (large, sweep) = (p.flag()?, p.flag()?);
                let to = pt(&mut p)?;
                arc(&mut out, cur, (rx, ry), rot, large, sweep, to);
                cur = to;
            }
            _ => {
                out.push(Seg::Close);
                cur = start;
                // a Z takes no numbers: another needs its own command
                cmd = None;
            }
        }
        last_cubic = cubic;
        last_quad = quad;
    }
    if !matches!(out.first(), None | Some(Seg::Move(..))) {
        return Err("path data must start with M".into());
    }
    Ok(out)
}

struct Reader<'a> {
    s: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn skip(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }

    fn num(&mut self) -> Result<f64, String> {
        self.skip();
        let from = self.i;
        let at = |r: &Self, i: usize| r.s.get(i).copied().unwrap_or(0);
        if matches!(at(self, self.i), b'+' | b'-') {
            self.i += 1;
        }
        let mut dot = false;
        while self.i < self.s.len() {
            match self.s[self.i] {
                b'0'..=b'9' => {}
                b'.' if !dot => dot = true,
                b'e' | b'E' if self.i > from && matches!(at(self, self.i + 1), b'0'..=b'9' | b'+' | b'-') => {
                    self.i += 2;
                    while at(self, self.i).is_ascii_digit() {
                        self.i += 1;
                    }
                    break;
                }
                _ => break,
            }
            self.i += 1;
        }
        let text = std::str::from_utf8(&self.s[from..self.i]).unwrap_or("");
        let n: f64 = text
            .parse()
            .map_err(|_| format!("path data: expected a number at byte {from}"))?;
        if !n.is_finite() || n.abs() > COORD_LIMIT {
            return Err(format!("path data: {text} is out of range (±{COORD_LIMIT})"));
        }
        Ok(n)
    }

    fn flag(&mut self) -> Result<bool, String> {
        self.skip();
        match self.s.get(self.i) {
            Some(b'0') => {
                self.i += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.i += 1;
                Ok(true)
            }
            _ => Err(format!("path data: expected an arc flag at byte {}", self.i)),
        }
    }
}

/// An elliptical arc as cubics of at most 90° each (SVG's endpoint parameterisation, F.6.5).
fn arc(out: &mut Vec<Seg>, from: (f64, f64), (rx, ry): (f64, f64), rot: f64, large: bool, sweep: bool, to: (f64, f64)) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 || from == to {
        if from != to {
            out.push(Seg::Line(to.0, to.1));
        }
        return;
    }
    let (sin, cos) = rot.to_radians().sin_cos();
    let (dx, dy) = ((from.0 - to.0) / 2.0, (from.1 - to.1) / 2.0);
    let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    let l = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if l > 1.0 {
        rx *= l.sqrt();
        ry *= l.sqrt();
    }
    let num = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut k = (num / den).sqrt();
    if large == sweep {
        k = -k;
    }
    let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
    let (cx, cy) = (
        cos * cx1 - sin * cy1 + (from.0 + to.0) / 2.0,
        sin * cx1 + cos * cy1 + (from.1 + to.1) / 2.0,
    );
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let (ux, uy) = ((x1 - cx1) / rx, (y1 - cy1) / ry);
    let (vx, vy) = ((-x1 - cx1) / rx, (-y1 - cy1) / ry);
    let start = angle(1.0, 0.0, ux, uy);
    let mut delta = angle(ux, uy, vx, vy);
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    let n = (delta.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = delta / n as f64;
    let t = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |a: f64| {
        let (s, c) = a.sin_cos();
        (cx + cos * rx * c - sin * ry * s, cy + sin * rx * c + cos * ry * s)
    };
    let deriv = |a: f64| {
        let (s, c) = a.sin_cos();
        (-cos * rx * s - sin * ry * c, -sin * rx * s + cos * ry * c)
    };
    for i in 0..n {
        let (a0, a1) = (start + step * i as f64, start + step * (i + 1) as f64);
        let (p0, p1) = (point(a0), if i + 1 == n { to } else { point(a1) });
        let (d0, d1) = (deriv(a0), deriv(a1));
        out.push(Seg::Cubic(
            p0.0 + t * d0.0,
            p0.1 + t * d0.1,
            p1.0 - t * d1.0,
            p1.1 - t * d1.1,
            p1.0,
            p1.1,
        ));
    }
}

/// An affine map: (x, y) to (a x + c y + e, b x + d y + f), as SVG's `matrix(a b c d e f)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn apply(&self, (x, y): (f64, f64)) -> (f64, f64) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }

    /// `self` after `other`: `other` maps first.
    pub fn then(&self, o: &Affine) -> Affine {
        Affine {
            a: o.a * self.a + o.c * self.b,
            b: o.b * self.a + o.d * self.b,
            c: o.a * self.c + o.c * self.d,
            d: o.b * self.c + o.d * self.d,
            e: o.a * self.e + o.c * self.f + o.e,
            f: o.b * self.e + o.d * self.f + o.f,
        }
    }

    /// How much it scales lengths, on average: a stroke's width under it.
    pub fn stretch(&self) -> f64 {
        (self.a * self.d - self.b * self.c).abs().sqrt()
    }
}

/// One part's drawing this frame: its layer's shapes, where they go, and how opaque.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item<'a> {
    pub layer: usize,
    pub shapes: &'a [Shape],
    pub at: Affine,
    pub opacity: f64,
}

/// The part a vector layer without parts draws: its shapes in art-box coordinates.
pub fn whole() -> Part {
    Part {
        id: String::new(),
        origin: (0.0, 0.0),
        shape: None,
    }
}

/// What a vector pack draws for `pose`, bottom to top, from where [`crate::compose::Parts::place`] put its parts:
/// each part's own shape, else its layer's value's.
pub fn scene<'a>(pack: &'a Pack, placed: &[Placed], pose: &Pose) -> Vec<Item<'a>> {
    let squash = (pose.motion.scale_x, pose.motion.scale_y);
    placed
        .iter()
        .filter_map(|p| {
            let layer = &pack.layers[p.layer];
            item(pack, p, layer.resolve_art(pose.key(layer.channel)), squash)
        })
        .collect()
}

/// A placed part's drawing: its own shape, else `value`'s; squashed with the body by `squash` (x, y).
pub fn item<'a>(pack: &'a Pack, p: &Placed, value: Option<&str>, squash: (f64, f64)) -> Option<Item<'a>> {
    let layer = &pack.layers[p.layer];
    let named = layer.parts.get(p.part).and_then(|part| part.shape.as_deref());
    let shapes = layer.shapes.get(named.or(value)?)?;
    let (sin, cos) = p.rotate.to_radians().sin_cos();
    let flip = if p.mirrored { -1.0 } else { 1.0 };
    let (sx, sy) = (p.scale * squash.0 * flip, p.scale * squash.1);
    Some(Item {
        layer: p.layer,
        shapes,
        at: Affine {
            a: cos * sx,
            b: sin * sx,
            c: -sin * sy,
            d: cos * sy,
            e: p.x,
            f: p.y,
        },
        opacity: p.opacity,
    })
}

/// A canvas of `w` × `h` px whose (0, 0) is art-box (`x0`, `y0`), at `scale` px per art-box px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub w: u32,
    pub h: u32,
    pub x0: f64,
    pub y0: f64,
    pub scale: f64,
}

/// Draws `items` (only those `keep` takes) with anti-aliasing: premultiplied RGBA, row-major.
pub fn render(items: &[Item], view: View, keep: impl Fn(&Item) -> bool) -> Vec<u8> {
    let Some(mut pixmap) = tiny_skia::Pixmap::new(view.w.max(1), view.h.max(1)) else {
        return Vec::new();
    };
    let to_view = Affine {
        a: view.scale,
        b: 0.0,
        c: 0.0,
        d: view.scale,
        e: -view.x0 * view.scale,
        f: -view.y0 * view.scale,
    };
    for item in items.iter().filter(|i| keep(i)) {
        let at = item.at.then(&to_view);
        let tf = tiny_skia::Transform::from_row(
            at.a as f32,
            at.b as f32,
            at.c as f32,
            at.d as f32,
            at.e as f32,
            at.f as f32,
        );
        for shape in item.shapes {
            let Some(path) = skia_path(&shape.path) else {
                continue;
            };
            let paint = |c: u32| {
                let mut p = tiny_skia::Paint::default();
                let a = (f64::from(c >> 24) * shape.opacity * item.opacity)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                p.set_color_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, a);
                p.anti_alias = true;
                p
            };
            if let Some(fill) = shape.fill {
                pixmap.fill_path(&path, &paint(fill), tiny_skia::FillRule::Winding, tf, None);
            }
            if let Some(stroke) = shape.stroke {
                let s = tiny_skia::Stroke {
                    width: shape.width as f32,
                    line_cap: match shape.cap {
                        Cap::Butt => tiny_skia::LineCap::Butt,
                        Cap::Round => tiny_skia::LineCap::Round,
                        Cap::Square => tiny_skia::LineCap::Square,
                    },
                    line_join: match shape.join {
                        Join::Miter => tiny_skia::LineJoin::Miter,
                        Join::Round => tiny_skia::LineJoin::Round,
                        Join::Bevel => tiny_skia::LineJoin::Bevel,
                    },
                    ..tiny_skia::Stroke::default()
                };
                pixmap.stroke_path(&path, &paint(stroke), &s, tf, None);
            }
        }
    }
    pixmap.take()
}

/// A vector pack standing idle, the art box (without its halo pad) at `scale` px per art-box px: (width, height,
/// premultiplied RGBA).
pub fn still(pack: &Pack, scale: f64) -> (u32, u32, Vec<u8>) {
    let pose = Pose::scripted(Values::IDLE, 0.0);
    let placed = crate::compose::Parts::default().place(pack, &pose, 0.0);
    let view = View {
        w: (ART_W * scale).round() as u32,
        h: (ART_H * scale).round() as u32,
        x0: 0.0,
        y0: 0.0,
        scale,
    };
    (view.w, view.h, render(&scene(pack, &placed, &pose), view, |_| true))
}

impl View {
    /// The art box with its halo pad, at `scale` px per art-box px.
    pub fn of_box(scale: f64) -> View {
        View {
            w: ((ART_W + 2.0 * HALO_PAD) * scale).round() as u32,
            h: ((ART_H + 2.0 * HALO_PAD) * scale).round() as u32,
            x0: -HALO_PAD,
            y0: -HALO_PAD,
            scale,
        }
    }
}

fn skia_path(segs: &[Seg]) -> Option<tiny_skia::Path> {
    let mut b = tiny_skia::PathBuilder::new();
    for s in segs {
        match *s {
            Seg::Move(x, y) => b.move_to(x as f32, y as f32),
            Seg::Line(x, y) => b.line_to(x as f32, y as f32),
            Seg::Quad(a, c, x, y) => b.quad_to(a as f32, c as f32, x as f32, y as f32),
            Seg::Cubic(a, c, d, e, x, y) => b.cubic_to(a as f32, c as f32, d as f32, e as f32, x as f32, y as f32),
            Seg::Close => b.close(),
        }
    }
    b.finish()
}

/// Checks a layer's shapes: their paths parsed and their numbers in range (the manifest's reader parsed them).
pub(crate) fn validate(layer: &Layer) -> Result<(), String> {
    for (value, shapes) in &layer.shapes {
        for s in shapes {
            if !(s.width.is_finite() && (0.0..=100.0).contains(&s.width)) {
                return Err(format!(
                    "layer {:?}: shape {value:?}'s stroke width is {} (0 to 100)",
                    layer.name, s.width
                ));
            }
            if !(s.opacity.is_finite() && (0.0..=1.0).contains(&s.opacity)) {
                return Err(format!(
                    "layer {:?}: shape {value:?}'s opacity is {} (0 to 1)",
                    layer.name, s.opacity
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_data_parses_every_command_absolute_and_relative() {
        let segs = parse("M 1 2 L3,4 h 2 v-1 C 0 0 1 1 2 2 s 1 1 2 2 Q 0 0 1 1 t 2 0 Z m 1 1 2 2 z").unwrap();
        assert_eq!(segs[0], Seg::Move(1.0, 2.0));
        assert_eq!(segs[1], Seg::Line(3.0, 4.0));
        assert_eq!(segs[2], Seg::Line(5.0, 4.0));
        assert_eq!(segs[3], Seg::Line(5.0, 3.0));
        assert_eq!(segs[4], Seg::Cubic(0.0, 0.0, 1.0, 1.0, 2.0, 2.0));
        // S reflects the last control point (1, 1) about (2, 2)
        assert_eq!(segs[5], Seg::Cubic(3.0, 3.0, 3.0, 3.0, 4.0, 4.0));
        assert_eq!(segs[6], Seg::Quad(0.0, 0.0, 1.0, 1.0));
        // T reflects (0, 0) about (1, 1)
        assert_eq!(segs[7], Seg::Quad(2.0, 2.0, 3.0, 1.0));
        assert_eq!(segs[8], Seg::Close);
        // after Z the pen is back at the start, (1, 2); m's extra pair is a relative line
        assert_eq!(segs[9], Seg::Move(2.0, 3.0));
        assert_eq!(segs[10], Seg::Line(4.0, 5.0));
        // numbers run together: a sign or a second dot starts the next
        assert_eq!(
            parse("M0-1.5.5e1 2"),
            Ok(vec![Seg::Move(0.0, -1.5), Seg::Line(5.0, 2.0)])
        );
    }

    #[test]
    fn an_arc_becomes_cubics_ending_where_it_says() {
        let segs = parse("M 0 0 A 10 10 0 0 1 20 0").unwrap();
        assert_eq!(segs.len(), 3, "a half circle is two quarter cubics");
        let Seg::Cubic(.., x, y) = segs[2] else { panic!() };
        assert_eq!((x, y), (20.0, 0.0));
        // the top of the half circle (sweeping clockwise in y-down) is at (10, -10)
        let Seg::Cubic(.., x, y) = segs[1] else { panic!() };
        assert!((x - 10.0).abs() < 1e-9 && (y + 10.0).abs() < 1e-9, "({x}, {y})");
    }

    #[test]
    fn bad_path_data_and_numbers_are_refused() {
        for d in [
            "",
            "L 1 1",
            "M 1",
            "M 1 1 X 2",
            "M 1e400 0",
            "M 20000 0",
            "M NaN 0",
            "M inf 0",
            "file:///etc/x",
            "url(http://example.com/a.svg)",
            "M 0 0 A 1 1 0 2 0 1 1",
        ] {
            if d.is_empty() {
                assert_eq!(parse(d), Ok(vec![]));
                continue;
            }
            assert!(parse(d).is_err(), "{d:?} parsed");
        }
    }

    #[test]
    fn colours_are_hex_or_none_and_never_a_reference() {
        assert_eq!(colour("#FA8"), Ok(Some(0xFFFFAA88)));
        assert_eq!(colour("#161A26"), Ok(Some(0xFF161A26)));
        assert_eq!(colour("#00000059"), Ok(Some(0x59000000)));
        assert_eq!(colour("none"), Ok(None));
        for bad in ["url(#g)", "url(http://example.com/x)", "red", "#12345", "#GGGGGG", ""] {
            assert!(colour(bad).is_err(), "{bad:?}");
        }
        for c in [None, Some(0xFF161A26), Some(0x59000000)] {
            assert_eq!(colour(&colour_name(c)), Ok(c));
        }
    }

    #[test]
    fn a_filled_square_renders_where_its_transform_puts_it() {
        let shape = Shape::filled("M -1 -1 H 1 V 1 H -1 Z", 0xFFFF0000);
        let shapes = [shape];
        let item = Item {
            layer: 0,
            shapes: &shapes,
            at: Affine {
                e: 5.0,
                f: 3.0,
                ..Affine::IDENTITY
            },
            opacity: 1.0,
        };
        let view = View {
            w: 10,
            h: 10,
            x0: 0.0,
            y0: 0.0,
            scale: 1.0,
        };
        let px = render(&[item], view, |_| true);
        let alpha = |x: usize, y: usize| px[(y * 10 + x) * 4 + 3];
        assert_eq!(alpha(4, 2), 255);
        assert_eq!(alpha(5, 3), 255);
        assert_eq!(alpha(6, 4), 0);
        assert_eq!(alpha(3, 2), 0);
        // at half opacity
        let half = Item { opacity: 0.5, ..item };
        let px = render(&[half], view, |_| true);
        assert_eq!(px[(2 * 10 + 4) * 4 + 3], 128);
    }
}
