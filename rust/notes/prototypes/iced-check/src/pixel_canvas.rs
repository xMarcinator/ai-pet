//! Canvas alternative for the sprite: one fill_rectangle per opaque pixel, cached.
#![allow(dead_code)]
use iced::mouse;
use iced::widget::canvas::{self, Cache, Frame, Geometry};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};

pub struct PixelSprite<'a> {
    /// Straight-alpha RGBA, `w * h * 4` bytes.
    pub rgba: &'a [u8],
    pub w: u32,
    pub h: u32,
    pub scale: f32,
    /// Cleared by the owner whenever the frame changes (`cache.clear()`).
    pub cache: &'a Cache,
}

impl<Message> canvas::Program<Message> for PixelSprite<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame: &mut Frame| {
            for y in 0..self.h {
                for x in 0..self.w {
                    let i = ((y * self.w + x) * 4) as usize;
                    let [r, g, b, a] = [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]];
                    if a == 0 {
                        continue;
                    }
                    frame.fill_rectangle(
                        Point::new(x as f32 * self.scale, y as f32 * self.scale),
                        Size::new(self.scale, self.scale),
                        Color::from_rgba8(r, g, b, a as f32 / 255.0),
                    );
                }
            }
        });
        vec![geometry]
    }
}

pub fn view<'a, Message: 'a>(sprite: PixelSprite<'a>) -> iced::Element<'a, Message> {
    let (w, h) = (sprite.w as f32 * sprite.scale, sprite.h as f32 * sprite.scale);
    iced::widget::canvas(sprite).width(w).height(h).into()
}
