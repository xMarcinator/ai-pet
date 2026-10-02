//! Root wrapper that reports the bounds of every descendant `container` that has an `.id(..)`.
//! On every `RedrawRequested` it runs an `Operation` over its content and publishes the list
//! of rectangles when it changed. The shell turns that list into the window's input region.
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, mouse, overlay};
use iced::{Element, Event, Length, Rectangle, Size, Vector, window};

pub struct HitRegions<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    on_change: Box<dyn Fn(Vec<Rectangle>) -> Message + 'a>,
}

impl<'a, Message, Theme, Renderer> HitRegions<'a, Message, Theme, Renderer> {
    pub fn new(
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
        on_change: impl Fn(Vec<Rectangle>) -> Message + 'a,
    ) -> Self {
        Self { content: content.into(), on_change: Box::new(on_change) }
    }
}

#[derive(Default)]
struct State {
    last: Vec<Rectangle>,
}

/// Collects `container(Some(id), bounds)` calls.
struct Collect(Vec<Rectangle>);

impl Operation for Collect {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<()>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
        if id.is_some() {
            // Snap outwards to whole logical pixels so tiny float jitter does not republish.
            let x = bounds.x.floor();
            let y = bounds.y.floor();
            self.0.push(Rectangle {
                x,
                y,
                width: (bounds.x + bounds.width).ceil() - x,
                height: (bounds.y + bounds.height).ceil() - y,
            });
        }
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for HitRegions<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        self.content.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Event::Window(window::Event::RedrawRequested(_)) = event {
            let mut collect = Collect(Vec::new());
            self.content
                .as_widget_mut()
                .operate(&mut tree.children[0], layout, renderer, &mut collect);

            let state = tree.state.downcast_mut::<State>();
            if state.last != collect.0 {
                state.last = collect.0.clone();
                shell.publish((self.on_change)(collect.0));
            }
        }

        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content
            .as_widget()
            .draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(&mut tree.children[0], layout, renderer, viewport, translation)
    }
}

impl<'a, Message, Theme, Renderer> From<HitRegions<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(w: HitRegions<'a, Message, Theme, Renderer>) -> Self {
        Element::new(w)
    }
}
