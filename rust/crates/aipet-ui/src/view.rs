//! The pet's surface: every layer pinned where [`crate::geometry`] puts it, bottom to top: the reviews header, the
//! bubbles (back to front), the ground shadow, the sprite's body, the glow's halo, the glow and effects, and the
//! menu when it is drawn inline.

use iced::widget::image::{FilterMethod, Handle};
use iced::widget::{Space, button, canvas, container, image, mouse_area, pin, row, stack, text};
use iced::{Border, Color, ContentFit, Element, Length, Point, Rectangle, Shadow, Vector, mouse};

use crate::cards::Card;
use crate::geometry::{
    CARD_H, CARD_RADIUS, CLOSE_SIZE, CardFrame, HALO_PAD, HEADER_H, HEADER_RADIUS, INLINE_MENU, SPRITE,
};
use crate::style::{self, AMBER, Glyph, Icon, argb};
use crate::{Message, PetUi};

/// Where a bubble's parts sit in it, unscaled (MakeCard: a 1 px border, padding 14 on the left, the dot in an
/// 18 px box with 8 px after it, the two lines of text centred in the 50 px).
const DOT: Point = Point::new(1.0 + 14.0 + 9.0, CARD_H / 2.0);
const TEXT_X: f32 = 1.0 + 14.0 + 18.0 + 8.0;
const TITLE_SIZE: f32 = 13.5;
const DETAIL_SIZE: f32 = 12.0;
/// The title's top: the lines' heights (1.362 em each) and the detail's 1 px margin, centred.
const TITLE_Y: f32 = (CARD_H - 1.362 * (TITLE_SIZE + DETAIL_SIZE) - 1.0) / 2.0;
const DETAIL_Y: f32 = TITLE_Y + 1.362 * TITLE_SIZE + 1.0;

impl PetUi {
    /// The pet's whole surface ([`crate::SURFACE`]), transparent where nothing is drawn. It reports the pointer
    /// ([`Message::PointerMoved`], [`Message::PointerLeft`]) itself.
    pub fn pet_view(&self) -> Element<'_, Message> {
        let mut layers: Vec<Element<'_, Message>> = Vec::new();
        if let (Some((label, _)), Some(r)) = (&self.header, self.header_rect()) {
            layers.push(pin(header(label, r)).position(r.position()).into());
        }
        for (card, frame) in self.stacks.frames() {
            self.card(card, frame, &mut layers);
        }

        let shadow = self.motion.shadow();
        layers.push(
            pin(image(self.shadow.clone())
                .width(shadow.width)
                .height(shadow.height)
                .content_fit(ContentFit::Fill)
                .opacity(self.motion.shadow_opacity))
            .position(shadow.position())
            .into(),
        );
        let body = self.motion.rect_to_surface(Rectangle::new(Point::ORIGIN, SPRITE));
        let halo = self.motion.rect_to_surface(Rectangle {
            x: -HALO_PAD,
            y: -HALO_PAD,
            width: SPRITE.width + 2.0 * HALO_PAD,
            height: SPRITE.height + 2.0 * HALO_PAD,
        });
        layers.push(pixels(&self.sprite.body_image, body, FilterMethod::Nearest));
        layers.push(pixels(&self.sprite.halo_image, halo, FilterMethod::Linear));
        layers.push(pixels(&self.sprite.top_image, body, FilterMethod::Nearest));
        // the hand pointer over the sprite, as the C#'s Cursor="Hand"
        let hand = self.motion.rect_to_surface(crate::geometry::HIT_ELLIPSE);
        layers.push(
            pin(mouse_area(Space::new().width(hand.width).height(hand.height)).interaction(mouse::Interaction::Pointer))
                .position(hand.position())
                .into(),
        );

        if self.inline_menu {
            layers.push(pin(self.menu_view()).position(INLINE_MENU.position()).into());
        }
        mouse_area(stack(layers).width(Length::Fill).height(Length::Fill))
            .on_move(Message::PointerMoved)
            .on_exit(Message::PointerLeft)
            .into()
    }

    /// One bubble's layers: its body, then (while its text shows) the pulsing dot and the text, then the dismiss
    /// button while it is hovered. Everything fades with the bubble and scales with it.
    fn card<'a>(&'a self, card: &'a Card, frame: CardFrame, layers: &mut Vec<Element<'a, Message>>) {
        let o = card.o;
        if o < 0.004 {
            return;
        }
        let s = frame.scale;
        let highlight = card.state == "attention";
        // StyleBody: the app's colour, or amber for one that needs you, or a faint edge
        let (edge, edge_width) = match (card.app_colour, highlight) {
            (Some(app), _) => (argb(app & 0x00FFFFFF | 0xB8000000), 1.5),
            (None, true) => (argb(AMBER & 0x00FFFFFF | 0xE0000000), 1.5),
            (None, false) => (argb(0x26FFFFFF), 1.0),
        };
        let background = argb(if highlight { 0xF72E291E } else { 0xF5252528 });
        // a bubble that needs you glows amber, breathing; the others cast a soft shadow (0 4 18 #61000000)
        let shadow = if highlight {
            let alpha = (0x40 as f32 + 0x90 as f32 * (0.5 + 0.5 * (self.t * 3.5).sin() as f32)) / 255.0;
            Shadow {
                color: Color {
                    a: alpha,
                    ..argb(AMBER)
                },
                offset: Vector::ZERO,
                blur_radius: 16.0 * s,
            }
        } else {
            Shadow {
                color: argb(0x61000000),
                offset: Vector::new(0.0, 4.0 * s),
                blur_radius: 14.0 * s,
            }
        };
        let shadow = Shadow {
            color: shadow.color.scale_alpha(o),
            ..shadow
        };
        let body = frame.body();
        let plate = container(Space::new().width(body.width).height(body.height)).style(move |_| container::Style {
            background: Some(background.scale_alpha(o).into()),
            border: Border {
                color: edge.scale_alpha(o),
                width: edge_width * s,
                radius: (CARD_RADIUS * s).into(),
            },
            shadow,
            text_color: None,
            snap: true,
        });
        layers.push(
            pin(mouse_area(plate)
                .on_press(Message::CardPressed(card.id))
                .interaction(mouse::Interaction::Pointer))
            .position(body.position())
            .into(),
        );

        if card.content {
            let colour = argb(style::status_colour(card.state));
            let dot = frame.map(DOT);
            if card.pulsing() {
                let phase = ((self.t - card.born_at) % 1.3 / 1.3) as f32;
                let grow = 1.0 - (1.0 - phase).powi(3);
                let alpha = 0.55 * (1.0 - phase) * o;
                layers.push(disc(dot, 8.0 * (1.0 + 1.6 * grow) * s, colour.scale_alpha(alpha)));
            }
            layers.push(disc(dot, 8.0 * s, colour.scale_alpha(o)));
            let title_colour = if highlight { argb(0xFFFFE9A8) } else { Color::WHITE };
            layers.push(line(
                &card.title,
                TITLE_SIZE * s,
                style::UI_SEMIBOLD,
                title_colour.scale_alpha(o),
                frame.map(Point::new(TEXT_X, TITLE_Y)),
            ));
            layers.push(line(
                &card.detail,
                DETAIL_SIZE * s,
                style::UI,
                argb(0xFFA8A8AE).scale_alpha(o),
                frame.map(Point::new(TEXT_X, DETAIL_Y)),
            ));
        }

        if self.close_visible(card) {
            let r = frame.close();
            let glyph = Glyph {
                icon: Icon::Close,
                colour: Color::WHITE.scale_alpha(o),
                width: 1.6,
            };
            let size = glyph.size() * s;
            let close = button(container(canvas(glyph).width(size.width).height(size.height)).center(Length::Fill))
                .width(r.width)
                .height(r.height)
                .padding(0)
                .on_press(Message::Dismiss(card.id))
                .style(move |_, status| button::Style {
                    background: Some(
                        argb(match status {
                            button::Status::Hovered | button::Status::Pressed => 0xFF4A4A4F,
                            _ => 0xFF2E2E31,
                        })
                        .scale_alpha(o)
                        .into(),
                    ),
                    text_color: Color::WHITE,
                    border: Border {
                        color: argb(0x66FFFFFF).scale_alpha(o),
                        width: s,
                        radius: (CLOSE_SIZE / 2.0 * s).into(),
                    },
                    shadow: Shadow::default(),
                    snap: true,
                });
            layers.push(pin(close).position(r.position()).into());
        }
    }
}

/// An image stretched over `r`.
fn pixels<'a>(handle: &Handle, r: Rectangle, filter: FilterMethod) -> Element<'a, Message> {
    pin(image(handle.clone())
        .width(r.width)
        .height(r.height)
        .content_fit(ContentFit::Fill)
        .filter_method(filter))
    .position(r.position())
    .into()
}

/// A filled circle `diameter` across, centred on `center`.
fn disc<'a>(center: Point, diameter: f32, colour: Color) -> Element<'a, Message> {
    let circle = container(Space::new().width(diameter).height(diameter)).style(move |_| container::Style {
        background: Some(colour.into()),
        border: Border {
            radius: (diameter / 2.0).into(),
            ..Border::default()
        },
        ..container::Style::default()
    });
    pin(circle)
        .position(Point::new(center.x - diameter / 2.0, center.y - diameter / 2.0))
        .into()
}

/// One line of text with its top-left at `at`.
fn line<'a>(content: &'a str, size: f32, font: iced::Font, colour: Color, at: Point) -> Element<'a, Message> {
    pin(text(content)
        .size(size)
        .font(font)
        .line_height(style::LINE_HEIGHT)
        .shaping(text::Shaping::Advanced)
        .wrapping(text::Wrapping::None)
        .color(colour))
    .position(at)
    .into()
}

/// The reviews header: a blue pill with a dot and how many reviews wait.
fn header<'a>(label: &'a str, r: Rectangle) -> Element<'a, Message> {
    let dot = container(Space::new().width(6).height(6)).style(|_| container::Style {
        background: Some(argb(0xFF4C9AFF).into()),
        border: Border {
            radius: 3.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    });
    let label = text(label)
        .size(11)
        .font(style::UI_SEMIBOLD)
        .line_height(style::LINE_HEIGHT)
        .shaping(text::Shaping::Advanced)
        .color(argb(0xFFBFD6FF));
    container(row![dot, label].spacing(6).align_y(iced::Alignment::Center))
        .width(r.width)
        .height(HEADER_H)
        .padding([0, 11])
        .align_y(iced::Alignment::Center)
        .style(|_| container::Style {
            background: Some(argb(0xE61C2B45).into()),
            border: Border {
                color: argb(0x554C9AFF),
                width: 1.0,
                radius: HEADER_RADIUS.into(),
            },
            ..container::Style::default()
        })
        .into()
}
