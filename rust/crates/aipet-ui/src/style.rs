//! Colours, fonts and text measuring shared by the pet, the menu and Settings (`App.axaml`'s resources).

use iced::advanced::graphics::text::Paragraph;
use iced::advanced::text::{self, Paragraph as _, Text};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, stroke};
use iced::{Color, Font, Pixels, Point, Rectangle, Renderer, Size, Theme, alignment, font, mouse};

/// The UI font. The C# asks for Segoe UI Variable Text, Segoe UI, Ubuntu, Cantarell, Noto Sans…; on the Linux
/// desktops the pet runs on that comes down to Noto Sans. Windows has no Noto Sans, so there it is Segoe UI, which
/// every Windows 10 and 11 has with its semibold face. Segoe UI Variable Text is not asked for: it is an instance of
/// Windows 11's variable font, whose family name is Segoe UI Variable, and it has the same metrics as Segoe UI.
pub const UI: Font = Font::with_name(if cfg!(windows) { "Segoe UI" } else { "Noto Sans" });
pub const UI_SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..UI
};
/// Noto Sans' line spacing (ascender 1069 + descender 293 per 1000 em), which Avalonia uses for a line's height.
/// Segoe UI's is 1.330 (2210 + 514 per 2048, the same in Segoe UI Variable); the lines are kept at Noto Sans' on
/// Windows too, because the bubbles' text positions are worked out from it.
pub const LINE_HEIGHT: text::LineHeight = text::LineHeight::Relative(1.362);

/// A colour from 0xAARRGGBB.
pub const fn argb(c: u32) -> Color {
    Color::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as f32 / 255.0)
}

pub const PANEL_BG: Color = argb(0xF5232326);
pub const PANEL_EDGE: Color = argb(0xFF3A3A3F);
pub const ICON: Color = argb(0xFFEDEDF0);
pub const MUTED: Color = argb(0xFF9A9AA2);
pub const AMBER: u32 = 0xFFFFCF3F;
/// The mood colours of Board.StatusColor, by pet state.
pub fn status_colour(state: &str) -> u32 {
    match state {
        "thinking" => 0xFF7AA7FF,
        "working" => 0xFFE27A52,
        "attention" => AMBER,
        "done" => 0xFF5FD38D,
        "review" => 0xFF4C9AFF,
        "music" => 0xFF1DB954,
        "error" => 0xFFE5484D,
        _ => 0xFF8A8A90,
    }
}

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

/// The small line icons drawn with a pen (`Path` data from the C#).
#[derive(Clone, Copy, Debug)]
pub enum Icon {
    /// The dismiss cross: "M0,0 L7,7 M7,0 L0,7", 7 × 7.
    Close,
    /// The menu's check mark: "M1,5 L4.5,8.5 L11,1.5", 12 × 10.
    Check,
}

/// An [`Icon`] in a colour and pen width, as a canvas program.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub icon: Icon,
    pub colour: Color,
    pub width: f32,
}

impl Glyph {
    /// The icon's size, with room for the pen's round ends around it.
    pub fn size(&self) -> Size {
        let natural = match self.icon {
            Icon::Close => Size::new(7.0, 7.0),
            Icon::Check => Size::new(12.0, 10.0),
        };
        Size::new(natural.width + self.width, natural.height + self.width)
    }
}

impl<Message> canvas::Program<Message> for Glyph {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        // scaled with the widget, so a glyph on a scaled bubble shrinks with it
        let k = bounds.width / self.size().width;
        let at = |x: f32, y: f32| Point::new((x + self.width / 2.0) * k, (y + self.width / 2.0) * k);
        let path = Path::new(|b| {
            let mut line = |points: &[(f32, f32)]| {
                b.move_to(at(points[0].0, points[0].1));
                for &(x, y) in &points[1..] {
                    b.line_to(at(x, y));
                }
            };
            match self.icon {
                Icon::Close => {
                    line(&[(0.0, 0.0), (7.0, 7.0)]);
                    line(&[(7.0, 0.0), (0.0, 7.0)]);
                }
                Icon::Check => line(&[(1.0, 5.0), (4.5, 8.5), (11.0, 1.5)]),
            }
        });
        let pen = Stroke::default()
            .with_color(self.colour)
            .with_width(self.width * k)
            .with_line_cap(stroke::LineCap::Round)
            .with_line_join(stroke::LineJoin::Round);
        frame.stroke(&path, pen);
        vec![frame.into_geometry()]
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
