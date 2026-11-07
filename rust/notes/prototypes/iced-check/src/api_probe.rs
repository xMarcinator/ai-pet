//! Signatures that the notes quote; compiled but not called.
#![allow(dead_code)]
use iced::time::Instant;
use iced::widget::{column, container, float, text};
use iced::{Color, Element, Event, Length, Point, Size, Subscription, Task, Vector, window};

#[derive(Debug, Clone)]
pub enum Msg {
    PetFrame(Instant),
    Monitor(Option<Size>),
    Scale(f32),
    Pos(Option<Point>),
    Moved(window::Id, Point),
    Unfocused(window::Id),
}

/// `window::frames()` fires for RedrawRequested of *every* window; this variant keeps the id.
pub fn frames_of(pet: window::Id) -> Subscription<Msg> {
    iced::event::listen_raw(|event, _status, id| match event {
        Event::Window(window::Event::RedrawRequested(at)) => Some((id, at)),
        _ => None,
    })
    .with(pet)
    .filter_map(|(pet, (id, at))| (id == pet).then_some(Msg::PetFrame(at)))
}

pub fn window_events() -> Subscription<Msg> {
    window::events().filter_map(|(id, e)| match e {
        window::Event::Moved(p) => Some(Msg::Moved(id, p)),
        window::Event::Unfocused => Some(Msg::Unfocused(id)),
        _ => None,
    })
}

pub fn queries(id: window::Id) -> Task<Msg> {
    Task::batch([
        window::monitor_size(id).map(Msg::Monitor),
        window::scale_factor(id).map(Msg::Scale),
        window::position(id).map(Msg::Pos),
    ])
}

pub fn effects(id: window::Id) -> Task<Msg> {
    Task::batch([
        window::set_mode(id, window::Mode::Windowed),
        window::set_level(id, window::Level::AlwaysOnTop),
        window::enable_mouse_passthrough(id),
        window::disable_mouse_passthrough(id),
        window::move_to(id, Point::new(10.0, 10.0)),
        window::resize(id, Size::new(300.0, 300.0)),
        window::close(id),
    ])
}

/// Bubbles laid out by iced (variable text height), slid in with `float(..).translate(..)`
/// and faded by scaling every colour's alpha. `t` in 0..=1 per bubble.
pub fn bubble_column<'a>(items: &'a [(String, f32)]) -> Element<'a, Msg> {
    column(items.iter().map(|(label, t)| {
        let t = *t;
        let card = container(text(label).size(13))
            .padding([8, 12])
            .width(220)
            .style(move |_theme| container::Style {
                background: Some(Color::from_rgb8(0x24, 0x27, 0x2e).scale_alpha(t).into()),
                text_color: Some(Color::WHITE.scale_alpha(t)),
                ..container::Style::default()
            });
        float(card)
            .translate(move |_bounds, _viewport| Vector::new(0.0, 12.0 * (1.0 - t)))
            .into()
    }))
    .spacing(6)
    .width(Length::Shrink)
    .into()
}
