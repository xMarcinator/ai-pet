//! Text with cosmic-text, drawn by blitting its glyph masks into a tiny-skia pixmap (not `Buffer::draw`, which calls
//! back per pixel). One font system and glyph cache for the scene.

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent, Weight};
use tiny_skia::PixmapMut;

/// The UI font, as the iced pet uses it on Linux, and its line spacing in em (Noto Sans' ascender + descender).
pub const FAMILY: &str = "Noto Sans";
pub const LINE_SPACING: f32 = (1069.0 + 293.0) / 1000.0;

pub struct Text {
    fonts: FontSystem,
    cache: SwashCache,
}

impl Text {
    pub fn new() -> Text {
        Text {
            fonts: FontSystem::new(),
            cache: SwashCache::new(),
        }
    }

    fn buffer(&mut self, s: &str, size: f32, bold: bool, scale: f32) -> Buffer {
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(size * scale, size * scale * LINE_SPACING));
        let weight = if bold { Weight::SEMIBOLD } else { Weight::NORMAL };
        buffer.set_size(None, None);
        buffer.set_text(
            s,
            &Attrs::new().family(Family::Name(FAMILY)).weight(weight),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.fonts, false);
        buffer
    }

    /// How wide `s` is at `size` logical px.
    pub fn width(&mut self, s: &str, size: f32, bold: bool) -> f32 {
        let buffer = self.buffer(s, size, bold, 1.0);
        buffer.layout_runs().map(|run| run.line_w).fold(0.0, f32::max)
    }

    /// `s`, cut with an ellipsis to fit `max` logical px (the iced pet's `style::fit`), and its width.
    pub fn fit(&mut self, s: &str, size: f32, bold: bool, max: f32) -> (String, f32) {
        let whole = self.width(s, size, bold);
        if whole <= max {
            return (s.to_owned(), whole);
        }
        let chars: Vec<char> = s.chars().collect();
        let (mut lo, mut hi) = (0, chars.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let cut: String = chars[..mid].iter().collect::<String>().trim_end().to_owned() + "…";
            if self.width(&cut, size, bold) <= max {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        let cut = chars[..lo].iter().collect::<String>().trim_end().to_owned() + "…";
        let w = self.width(&cut, size, bold);
        (cut, w)
    }

    /// Draws one line of `s` with its top-left at (`x`, `y`) device px, at `size` logical px × `scale`, in `rgba`
    /// (straight), blitting each glyph's mask source-over into `pm` (premultiplied RGBA).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, pm: &mut PixmapMut, s: &str, size: f32, bold: bool, scale: f32, x: f32, y: f32, rgba: [u8; 4]) {
        let buffer = self.buffer(s, size, bold, scale);
        let (pw, ph) = (pm.width() as i32, pm.height() as i32);
        let data = pm.data_mut();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let physical = glyph.physical((x, y + run.line_y), 1.0);
                let Some(image) = self.cache.get_image(&mut self.fonts, physical.cache_key) else {
                    continue;
                };
                let (gx, gy) = (physical.x + image.placement.left, physical.y - image.placement.top);
                let (gw, gh) = (image.placement.width as i32, image.placement.height as i32);
                for row in 0..gh {
                    let dy = gy + row;
                    if !(0..ph).contains(&dy) {
                        continue;
                    }
                    for col in 0..gw {
                        let dx = gx + col;
                        if !(0..pw).contains(&dx) {
                            continue;
                        }
                        let k = (row * gw + col) as usize;
                        let (src, cover) = match image.content {
                            SwashContent::Mask => (rgba, image.data[k]),
                            SwashContent::Color => {
                                let p = &image.data[k * 4..k * 4 + 4];
                                ([p[0], p[1], p[2], 255], p[3])
                            }
                            SwashContent::SubpixelMask => (rgba, image.data[k * 4]),
                        };
                        let a = u32::from(cover) * u32::from(src[3]) / 255;
                        if a == 0 {
                            continue;
                        }
                        let i = ((dy * pw + dx) * 4) as usize;
                        for c in 0..3 {
                            let s = u32::from(src[c]) * a / 255;
                            let d = u32::from(data[i + c]);
                            data[i + c] = (s + d * (255 - a) / 255).min(255) as u8;
                        }
                        let d = u32::from(data[i + 3]);
                        data[i + 3] = (a + d * (255 - a) / 255).min(255) as u8;
                    }
                }
            }
        }
    }
}
