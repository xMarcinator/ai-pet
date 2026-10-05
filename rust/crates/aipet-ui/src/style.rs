//! Colours, fonts and text measuring shared by the pet, the menu and Settings (`App.axaml`'s resources).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use iced::advanced::graphics::text::Paragraph;
use iced::advanced::text::{self, Paragraph as _, Text};
use iced::widget::image;
use iced::widget::image::{FilterMethod, Handle};
use iced::{Color, ContentFit, Element, Font, Pixels, Point, Size, alignment, font};

/// The UI font. The C# asks for Segoe UI Variable Text, Segoe UI, Ubuntu, Cantarell, Noto Sans…; on the Linux
/// desktops the pet runs on that comes down to Noto Sans. Windows has no Noto Sans, so there it is Segoe UI, which
/// every Windows 10 and 11 has with its semibold face. Segoe UI Variable Text is not asked for: it is an instance of
/// Windows 11's variable font, whose family name is Segoe UI Variable, and it has the same metrics as Segoe UI.
pub const UI: Font = Font::with_name(if cfg!(windows) { "Segoe UI" } else { "Noto Sans" });
pub const UI_SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..UI
};
/// The UI font's line spacing in em, which Avalonia uses for a line's height: Segoe UI's (ascender 2210 + descender
/// 514 per 2048, about 1.330, the same in Segoe UI Variable) on Windows, Noto Sans' (1069 + 293 per 1000) elsewhere.
/// The bubbles' text positions are worked out from it.
pub const LINE_SPACING: f32 = if cfg!(windows) {
    (2210.0 + 514.0) / 2048.0
} else {
    (1069.0 + 293.0) / 1000.0
};
pub const LINE_HEIGHT: text::LineHeight = text::LineHeight::Relative(LINE_SPACING);

/// A colour from 0xAARRGGBB.
pub const fn argb(c: u32) -> Color {
    Color::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as f32 / 255.0)
}

pub const PANEL_BG: Color = argb(0xF5232326);
pub const PANEL_EDGE: Color = argb(0xFF3A3A3F);
pub const ICON: Color = argb(0xFFEDEDF0);
pub const MUTED: Color = argb(0xFF9A9AA2);
pub const AMBER: u32 = 0xFFFFCF3F;

/// How wide `content` is on one line.
pub fn text_width(content: &str, size: f32, font: Font) -> f32 {
    Paragraph::with_text(Text {
        content,
        bounds: Size::INFINITE,
        size: Pixels(size),
        line_height: LINE_HEIGHT,
        font,
        align_x: text::Alignment::Left,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::Advanced,
        wrapping: text::Wrapping::None,
    })
    .min_bounds()
    .width
}

/// `content` cut to `max` px with an ellipsis, as the C#'s TextTrimming="CharacterEllipsis"; returns the text and
/// its width.
pub fn fit(content: &str, size: f32, font: Font, max: f32) -> (String, f32) {
    let width = text_width(content, size, font);
    if width <= max {
        return (content.to_owned(), width);
    }
    let chars: Vec<char> = content.chars().collect();
    // the longest prefix that fits with the ellipsis: binary search on the number of characters
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate = format!("{}…", chars[..mid].iter().collect::<String>().trim_end());
        if text_width(&candidate, size, font) <= max {
            lo = mid
        } else {
            hi = mid - 1
        }
    }
    let cut = format!("{}…", chars[..lo].iter().collect::<String>().trim_end());
    let width = text_width(&cut, size, font);
    (cut, width)
}

/// The pet's line icons (MakeCard's and the menu's `Path` data), each in its own box. They are drawn with a pen of
/// round caps and joins, the triangles also filled, as the C# draws them.
///
/// They are pictures, not canvas meshes: in the Windows pet window (GL) the meshes came out doubled and stale (a
/// second copy beside each icon, a check mark or dismiss cross where none belongs; task 27's hands-on pass), though
/// offscreen renders of the same view are right. The sprite's images were never affected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    /// The dismiss cross: "M0,0 L7,7 M7,0 L0,7", 7 × 7, with a 1.6 px pen.
    Close,
    /// The menu's check mark: "M1,5 L4.5,8.5 L11,1.5", 12 × 10, with a 1.9 px pen.
    Check,
    /// The round buttons' icons, in MakeIcon's 16 × 16 box. An arrow out of a box: Open in Jira, and a chat's Open.
    Open,
    /// Two branches joined: the pull request's.
    PullRequest,
    Previous,
    Pause,
    Play,
    Next,
}

/// Icons are rasterised at this many pixels per logical pixel (enough for a 300 % screen) and drawn scaled down.
const ICON_RES: f32 = 3.0;

impl Icon {
    /// Its pen width, for the cross and the check mark (the round buttons' icons have theirs in [`Icon::drawing`]).
    fn pen(self) -> f32 {
        match self {
            Icon::Check => 1.9,
            _ => 1.6,
        }
    }

    /// Its size in logical pixels: the cross and the check mark with room for the pen's round ends around them, the
    /// round buttons' icons their 16 × 16 box.
    pub fn size(self) -> Size {
        match self {
            Icon::Close => Size::new(7.0 + self.pen(), 7.0 + self.pen()),
            Icon::Check => Size::new(12.0 + self.pen(), 10.0 + self.pen()),
            _ => Size::new(16.0, 16.0),
        }
    }

    /// What is drawn, in logical pixels within [`Icon::size`].
    fn drawing(self) -> Drawing {
        let mut d = Drawing::default();
        let p = |x: f32, y: f32| Point::new(x, y);
        match self {
            Icon::Close | Icon::Check => {
                let o = self.pen() / 2.0;
                let at = |points: &[(f32, f32)]| points.iter().map(|&(x, y)| p(x + o, y + o)).collect::<Vec<_>>();
                if self == Icon::Close {
                    d.line(self.pen(), at(&[(0.0, 0.0), (7.0, 7.0)]));
                    d.line(self.pen(), at(&[(7.0, 0.0), (0.0, 7.0)]));
                } else {
                    d.line(self.pen(), at(&[(1.0, 5.0), (4.5, 8.5), (11.0, 1.5)]));
                }
            }
            // M9,3 H13 V7 M13,3 L7.5,8.5 M11,9.5 V12.5 A0.5,0.5 0 0 1 10.5,13 H3.5 A0.5,0.5 0 0 1 3,12.5 V5.5
            // A0.5,0.5 0 0 1 3.5,5 H6.5
            Icon::Open => {
                d.line(1.6, vec![p(9.0, 3.0), p(13.0, 3.0), p(13.0, 7.0)]);
                d.line(1.6, vec![p(13.0, 3.0), p(7.5, 8.5)]);
                let mut frame = vec![p(11.0, 9.5)];
                corner(&mut frame, p(11.0, 13.0), p(3.0, 13.0), 0.5);
                corner(&mut frame, p(3.0, 13.0), p(3.0, 5.0), 0.5);
                corner(&mut frame, p(3.0, 5.0), p(6.5, 5.0), 0.5);
                frame.push(p(6.5, 5.0));
                d.line(1.6, frame);
            }
            // M4.5,2.5 A1.5,1.5 0 1 1 4.49,2.5 M4.5,5.5 V13.5 M11.5,10.5 A1.5,1.5 0 1 1 11.49,10.5 M11.5,10.5 V6.5
            // A2,2 0 0 0 9.5,4.5 H7 M8.5,3 L7,4.5 L8.5,6 (the two almost-closed arcs are the branches' circles)
            Icon::PullRequest => {
                d.line(1.5, circle(p(4.5, 4.0), 1.5));
                d.line(1.5, vec![p(4.5, 5.5), p(4.5, 13.5)]);
                d.line(1.5, circle(p(11.5, 12.0), 1.5));
                let mut branch = vec![p(11.5, 10.5)];
                corner(&mut branch, p(11.5, 4.5), p(7.0, 4.5), 2.0);
                branch.push(p(7.0, 4.5));
                d.line(1.5, branch);
                d.line(1.5, vec![p(8.5, 3.0), p(7.0, 4.5), p(8.5, 6.0)]);
            }
            // M4,3.5 V12.5 M12.5,3.5 L6.5,8 L12.5,12.5 Z, filled
            Icon::Previous => {
                d.triangle(1.6, [p(12.5, 3.5), p(6.5, 8.0), p(12.5, 12.5)]);
                d.line(1.6, vec![p(4.0, 3.5), p(4.0, 12.5)]);
            }
            // M12,3.5 V12.5 M3.5,3.5 L9.5,8 L3.5,12.5 Z, filled
            Icon::Next => {
                d.triangle(1.6, [p(3.5, 3.5), p(9.5, 8.0), p(3.5, 12.5)]);
                d.line(1.6, vec![p(12.0, 3.5), p(12.0, 12.5)]);
            }
            // M5.5,3.5 V12.5 M10.5,3.5 V12.5
            Icon::Pause => {
                d.line(1.6, vec![p(5.5, 3.5), p(5.5, 12.5)]);
                d.line(1.6, vec![p(10.5, 3.5), p(10.5, 12.5)]);
            }
            // M5,3 L13,8 L5,13 Z, filled
            Icon::Play => d.triangle(1.6, [p(5.0, 3.0), p(13.0, 8.0), p(5.0, 13.0)]),
        }
        d
    }

    /// The icon in `colour`, `scale` times its size. Its opacity is the colour's alpha.
    pub fn view<'a, Message: 'a>(self, colour: Color, scale: f32) -> Element<'a, Message> {
        let size = self.size() * scale;
        image(self.picture(colour))
            .width(size.width)
            .height(size.height)
            .content_fit(ContentFit::Fill)
            .filter_method(FilterMethod::Linear)
            .opacity(colour.a)
            .into()
    }

    /// The icon rasterised in `colour` (its alpha left out), made once per icon and colour: a new picture is a new
    /// texture.
    fn picture(self, colour: Color) -> Handle {
        type Pictures = HashMap<(Icon, [u8; 3]), Handle>;
        static PICTURES: LazyLock<Mutex<Pictures>> = LazyLock::new(Mutex::default);
        let [r, g, b, _] = colour.into_rgba8();
        let mut pictures = PICTURES.lock().unwrap_or_else(|e| e.into_inner());
        pictures
            .entry((self, [r, g, b]))
            .or_insert_with(|| {
                let (w, h, coverage) = self.drawing().coverage(self.size(), ICON_RES);
                let rgba: Vec<u8> = coverage
                    .iter()
                    .flat_map(|&c| [r, g, b, (c * 255.0).round() as u8])
                    .collect();
                Handle::from_rgba(w, h, rgba)
            })
            .clone()
    }
}

/// An icon's strokes (polylines, each with its pen width) and filled triangles, in logical pixels.
#[derive(Debug, Default)]
struct Drawing {
    strokes: Vec<(f32, Vec<Point>)>,
    fills: Vec<[Point; 3]>,
}

impl Drawing {
    fn line(&mut self, width: f32, points: Vec<Point>) {
        self.strokes.push((width, points));
    }

    /// A filled triangle, its outline stroked too.
    fn triangle(&mut self, width: f32, [a, b, c]: [Point; 3]) {
        self.fills.push([a, b, c]);
        self.line(width, vec![a, b, c, a]);
    }

    fn covers(&self, at: Point) -> bool {
        self.strokes.iter().any(|(width, points)| {
            points
                .windows(2)
                .any(|s| distance_to_segment(at, s[0], s[1]) <= width / 2.0)
        }) || self.fills.iter().any(|&t| in_triangle(at, t))
    }

    /// How much of each pixel the drawing covers (0 to 1, rows top to bottom), at `res` pixels per logical pixel
    /// over `size`, from 4 × 4 samples a pixel.
    fn coverage(&self, size: Size, res: f32) -> (u32, u32, Vec<f32>) {
        const N: usize = 4;
        let (w, h) = ((size.width * res).ceil() as u32, (size.height * res).ceil() as u32);
        let mut out = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                let mut hits = 0;
                for i in 0..N {
                    for j in 0..N {
                        let sx = (x as f32 + (i as f32 + 0.5) / N as f32) / res;
                        let sy = (y as f32 + (j as f32 + 0.5) / N as f32) / res;
                        hits += usize::from(self.covers(Point::new(sx, sy)));
                    }
                }
                out.push(hits as f32 / (N * N) as f32);
            }
        }
        (w, h, out)
    }
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f32 {
    let (ab, ap) = (b - a, p - a);
    let length2 = ab.x * ab.x + ab.y * ab.y;
    let t = if length2 == 0.0 {
        0.0
    } else {
        ((ap.x * ab.x + ap.y * ab.y) / length2).clamp(0.0, 1.0)
    };
    p.distance(Point::new(a.x + ab.x * t, a.y + ab.y * t))
}

fn in_triangle(p: Point, [a, b, c]: [Point; 3]) -> bool {
    let side = |a: Point, b: Point| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let (d1, d2, d3) = (side(a, b), side(b, c), side(c, a));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

/// Continues `points` round the right-angled corner at `at` towards `next`, rounded with radius `r` (an SVG arc
/// between the two straight runs).
fn corner(points: &mut Vec<Point>, at: Point, next: Point, r: f32) {
    let unit = |v: iced::Vector| v * (1.0 / (v.x * v.x + v.y * v.y).sqrt());
    let from = *points.last().expect("a corner after a point");
    let (u0, u2) = (unit(from - at), unit(next - at));
    let center = at + (u0 + u2) * r;
    let (start, end) = (at + u0 * r, at + u2 * r);
    let angle = |q: Point| (q.y - center.y).atan2(q.x - center.x);
    let (a0, mut a1) = (angle(start), angle(end));
    // the short way round: a quarter turn
    if a1 - a0 > std::f32::consts::PI {
        a1 -= 2.0 * std::f32::consts::PI;
    } else if a0 - a1 > std::f32::consts::PI {
        a1 += 2.0 * std::f32::consts::PI;
    }
    for k in 0..=8 {
        let a = a0 + (a1 - a0) * k as f32 / 8.0;
        points.push(Point::new(center.x + r * a.cos(), center.y + r * a.sin()));
    }
}

/// A circle as a closed polyline.
fn circle(center: Point, r: f32) -> Vec<Point> {
    (0..=32)
        .map(|k| {
            let a = k as f32 / 32.0 * 2.0 * std::f32::consts::PI;
            Point::new(center.x + r * a.cos(), center.y + r * a.sin())
        })
        .collect()
}

#[cfg(test)]
mod icon_tests {
    use super::*;

    /// The coverage of the pixel holding `at` (logical pixels).
    fn at(icon: Icon, x: f32, y: f32) -> f32 {
        let (w, _, coverage) = icon.drawing().coverage(icon.size(), ICON_RES);
        coverage[(y * ICON_RES) as usize * w as usize + (x * ICON_RES) as usize]
    }

    #[test]
    fn icons_are_drawn_from_their_path_data_with_a_round_pen() {
        // the cross: its middle, its four ends, and nothing midway along its box's edges
        assert_eq!(at(Icon::Close, 4.3, 4.3), 1.0);
        for (x, y) in [(0.8, 0.8), (7.8, 0.8), (0.8, 7.8), (7.8, 7.8)] {
            assert_eq!(at(Icon::Close, x, y), 1.0, "({x}, {y})");
        }
        assert_eq!(at(Icon::Close, 4.3, 0.2), 0.0);
        assert_eq!(at(Icon::Close, 0.2, 4.3), 0.0);
        // a filled triangle is filled, and the box's corners round the frame of Open's box
        assert_eq!(at(Icon::Play, 8.0, 8.0), 1.0);
        assert_eq!(at(Icon::Open, 11.0, 13.0), 1.0, "the rounded corner is on the frame");
        assert_eq!(at(Icon::Open, 7.0, 10.0), 0.0, "inside the box is empty");
        // every icon draws something within its box
        for icon in [
            Icon::Close,
            Icon::Check,
            Icon::Open,
            Icon::PullRequest,
            Icon::Previous,
            Icon::Pause,
            Icon::Play,
            Icon::Next,
        ] {
            let (w, h, coverage) = icon.drawing().coverage(icon.size(), ICON_RES);
            assert_eq!(coverage.len(), (w * h) as usize);
            assert!(coverage.contains(&1.0), "{icon:?}");
        }
    }

    #[test]
    fn an_icon_is_rasterised_once_per_colour() {
        let white = Icon::Close.picture(Color::WHITE);
        assert_eq!(
            Icon::Close.picture(Color::WHITE.scale_alpha(0.3)).id(),
            white.id(),
            "the alpha is the view's"
        );
        assert_ne!(Icon::Close.picture(ICON).id(), white.id());
        assert_ne!(Icon::Check.picture(Color::WHITE).id(), white.id());
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// Each UI font names a face Windows has, at its own weight: for a family that isn't there, or a weight its
    /// family lacks, the text stack would substitute another face.
    #[test]
    fn the_ui_fonts_are_faces_windows_has() {
        use iced::advanced::graphics::text::cosmic_text::fontdb::{Family, Query, Weight};
        use iced::advanced::graphics::text::font_system;

        let mut fonts = font_system().write().expect("the font system");
        let db = fonts.raw().db();
        for (font, weight) in [(UI, 400), (UI_SEMIBOLD, 600)] {
            let font::Family::Name(name) = font.family else {
                panic!("{font:?} names no family");
            };
            let query = Query {
                families: &[Family::Name(name)],
                weight: Weight(weight),
                ..Query::default()
            };
            let face = db.query(&query).and_then(|id| db.face(id));
            let face = face.unwrap_or_else(|| panic!("Windows has no {name:?}"));
            assert_eq!(face.weight, Weight(weight), "{name:?} at {weight}");
            assert!(
                face.families.iter().any(|(family, _)| family == name),
                "{name:?}: {:?}",
                face.families
            );
        }
    }
}
