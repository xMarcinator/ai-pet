//! Vector packs on screen (R8): the parts' shapes drawn through iced's canvas, with their geometry cached until
//! what is drawn changes (a value, a part's place), and the hit mask rasterised with tiny-skia at art resolution
//! when it does.
//!
//! The body's breathing and squash are not in the cached geometry: the canvas is laid out in the sprite's moved and
//! snapped rectangle, and the drawing stretches to it, so the geometry is rebuilt only when that rectangle's size
//! changes by a device pixel (the motion's bucket).

use aipet_sprite::compose::{self, Parts, Placed};
use aipet_sprite::pack::Pack;
use aipet_sprite::pose::{ART_H, ART_W, Channel, HALO_PAD, Pose, Values};
use aipet_sprite::vector::{self, Affine, Cap, Join, Seg, View};
use iced::widget::canvas::{self, Cache, Frame, Geometry, LineCap, LineJoin, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme, mouse};

/// The art box with its halo pad, in art-box px.
const BOX_W: f64 = ART_W + 2.0 * HALO_PAD;
const BOX_H: f64 = ART_H + 2.0 * HALO_PAD;

/// The glow's halo: strokes along a glowing layer's shapes, widest and faintest first (width in art-box px, alpha).
const HALO: [(f64, f32); 3] = [(6.0, 0.10), (3.5, 0.18), (1.5, 0.30)];

/// A vector pack's drawing between frames: where its parts are, what it drew last, and the cached geometry.
#[derive(Default)]
pub struct VectorArt {
    parts: Parts,
    /// The pose drawn, without the body's motion, and each channel's value in it.
    pose: Option<Pose>,
    values: [&'static str; 8],
    placed: Vec<Placed>,
    cache: Cache,
    /// How many times the geometry was cleared to be built again.
    pub rebuilds: usize,
    /// The hit layers' solid rows, (x, y, length) in art-box px from the halo pad's corner.
    pub runs: Vec<(i32, i32, i32)>,
    /// Whether a part is moving between poses, or an animation steps, faster than the calm rate shows: its typical
    /// key's ms.
    pub fast_ms: Option<u32>,
}

impl VectorArt {
    /// Places the parts for `pose` at `t`; when anything drawn changed, the geometry is cleared and the hit mask
    /// redrawn.
    pub fn update(&mut self, pack: &Pack, pose: &Pose, t: f64) {
        let mut drawn = *pose;
        drawn.motion = Pose::scripted(Values::IDLE, 0.0).motion;
        let placed = self.parts.place(pack, &drawn, t);
        let values = Channel::ALL.map(|c| drawn.key(c));
        self.fast_ms = if self.parts.moving(t) {
            Some(16)
        } else {
            fast_ms(pack, &drawn)
        };
        if self.pose.is_some() && values == self.values && placed == self.placed {
            return;
        }
        let items = vector::scene(pack, &placed, &drawn);
        let view = View::of_box(1.0);
        let mask = vector::render(&items, view, |i| pack.layers[i.layer].hit);
        self.runs = compose::runs_rgba(&mask, view.w as usize);
        self.pose = Some(drawn);
        self.values = values;
        self.placed = placed;
        self.cache.clear();
        self.rebuilds += 1;
    }

    /// The canvas that draws it, in the pack's colours.
    pub fn drawing<'a>(&'a self, pack: &'a Pack) -> Drawing<'a> {
        Drawing { art: self, pack }
    }
}

/// The typical key of the part animations on screen, when there are any and they move.
fn fast_ms(pack: &Pack, pose: &Pose) -> Option<u32> {
    if pose.still {
        return None;
    }
    pack.layers
        .iter()
        .filter_map(|l| l.part_animations.get(l.resolve(pose.key(l.channel))?))
        .filter(|a| a.keys.len() > 1)
        .map(|a| {
            let mut ms: Vec<u32> = a.keys.iter().map(|k| k.ms).collect();
            ms.sort_unstable();
            ms[ms.len() / 2]
        })
        .min()
}

/// A vector pack's canvas: the art box with its halo pad, stretched to the bounds it is laid out in.
pub struct Drawing<'a> {
    art: &'a VectorArt,
    pack: &'a Pack,
}

impl<Message> canvas::Program<Message> for Drawing<'_> {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let geometry = self.art.cache.draw(renderer, bounds.size(), |frame| {
            let Some(pose) = &self.art.pose else {
                return;
            };
            let (sx, sy) = (f64::from(bounds.width) / BOX_W, f64::from(bounds.height) / BOX_H);
            let to_bounds = Affine {
                a: sx,
                b: 0.0,
                c: 0.0,
                d: sy,
                e: HALO_PAD * sx,
                f: HALO_PAD * sy,
            };
            let glow = self.pack.glow;
            for item in vector::scene(self.pack, &self.art.placed, pose) {
                let at = item.at.then(&to_bounds);
                let stretch = at.stretch();
                if self.pack.layers[item.layer].glow {
                    for (width, alpha) in HALO {
                        for shape in item.shapes {
                            let stroke = Stroke::default()
                                .with_color(colour(glow, f64::from(alpha) * item.opacity))
                                .with_width((width * stretch) as f32)
                                .with_line_cap(LineCap::Round)
                                .with_line_join(LineJoin::Round);
                            frame.stroke(&path(&shape.path, &at), stroke);
                        }
                    }
                }
                for shape in item.shapes {
                    draw_shape(frame, shape, &at, item.opacity * shape.opacity);
                }
            }
        });
        vec![geometry]
    }
}

fn draw_shape(frame: &mut Frame, shape: &aipet_sprite::pack::Shape, at: &Affine, opacity: f64) {
    let p = path(&shape.path, at);
    if let Some(fill) = shape.fill {
        frame.fill(&p, colour(fill, opacity));
    }
    if let Some(stroke) = shape.stroke {
        let s = Stroke::default()
            .with_color(colour(stroke, opacity))
            .with_width((shape.width * at.stretch()) as f32)
            .with_line_cap(match shape.cap {
                Cap::Butt => LineCap::Butt,
                Cap::Round => LineCap::Round,
                Cap::Square => LineCap::Square,
            })
            .with_line_join(match shape.join {
                Join::Miter => LineJoin::Miter,
                Join::Round => LineJoin::Round,
                Join::Bevel => LineJoin::Bevel,
            });
        frame.stroke(&p, s);
    }
}

/// A straight 0xAARRGGBB colour at `opacity`.
fn colour(argb: u32, opacity: f64) -> Color {
    let [a, r, g, b] = argb.to_be_bytes();
    Color::from_rgba8(r, g, b, (f64::from(a) / 255.0 * opacity) as f32)
}

/// The segments mapped through `at`, as a canvas path.
fn path(segs: &[Seg], at: &Affine) -> Path {
    let p = |x: f64, y: f64| {
        let (x, y) = at.apply((x, y));
        Point::new(x as f32, y as f32)
    };
    Path::new(|b| {
        for s in segs {
            match *s {
                Seg::Move(x, y) => b.move_to(p(x, y)),
                Seg::Line(x, y) => b.line_to(p(x, y)),
                Seg::Quad(cx, cy, x, y) => b.quadratic_curve_to(p(cx, cy), p(x, y)),
                Seg::Cubic(ax, ay, bx, by, x, y) => b.bezier_curve_to(p(ax, ay), p(bx, by), p(x, y)),
                Seg::Close => b.close(),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use aipet_sprite::pose::{Arms, Values};

    use super::*;

    fn pip() -> Pack {
        let found = aipet_sprite::load::all(std::path::Path::new("/nonexistent"));
        found.pets.into_iter().find(|p| p.pack.name == "Pip").unwrap().pack
    }

    #[test]
    fn the_geometry_is_cleared_only_when_what_is_drawn_changes() {
        let pack = pip();
        let mut art = VectorArt::default();
        let mut pose = Pose::scripted(Values::IDLE, 0.0);
        pose.still = true;
        art.update(&pack, &pose, 0.0);
        let first = art.runs.clone();
        assert!(!first.is_empty(), "the hit mask has the body");
        // the same pose later, and the body breathing: nothing drawn changed
        assert_eq!(art.rebuilds, 1);
        // the same pose later, with the body breathing: nothing drawn changed
        pose.motion.scale_y = 1.03;
        art.update(&pack, &pose, 1.0);
        assert_eq!(art.rebuilds, 1);
        // the hands go up: they move over 140 ms, rebuilt as they go, then rest
        let mut raised = Pose::scripted(
            Values {
                arms: Arms::Raised,
                ..Values::IDLE
            },
            2.0,
        );
        // held still, so the sprout's sway doesn't step
        raised.still = true;
        art.update(&pack, &raised, 2.0);
        assert_eq!(art.fast_ms, Some(16), "moving between poses");
        art.update(&pack, &raised, 2.07);
        art.update(&pack, &raised, 3.0);
        let built = art.rebuilds;
        art.update(&pack, &raised, 4.0);
        assert_eq!(art.rebuilds, built);
        assert_eq!(art.fast_ms, None);
        assert_ne!(art.runs, first, "the hit mask follows the hands");
    }
}
