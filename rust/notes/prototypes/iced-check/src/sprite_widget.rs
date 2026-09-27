//! Minimal leaf widget: draws one image handle with nearest filtering.
#![allow(dead_code)]
use iced::advanced::image as adv_image;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::Tree;
use iced::advanced::{Widget, mouse};
use iced::widget::image;
use iced::{Element, Length, Rectangle, Size};

pub struct Sprite {
    pub handle: image::Handle,
    pub w: f32,
    pub h: f32,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Sprite
where
    Renderer: adv_image::Renderer<Handle = image::Handle>,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.w), Length::Fixed(self.h))
    }

    fn layout(&mut self, _tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(self.w, self.h, Size::new(self.w, self.h)))
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        renderer.draw_image(
            adv_image::Image::new(self.handle.clone())
                .filter_method(image::FilterMethod::Nearest)
                .snap(true),
            bounds,
            bounds,
        );
    }
}

impl<'a, Message, Theme, Renderer> From<Sprite> for Element<'a, Message, Theme, Renderer>
where
    Renderer: adv_image::Renderer<Handle = image::Handle> + 'a,
{
    fn from(s: Sprite) -> Self {
        Element::new(s)
    }
}
