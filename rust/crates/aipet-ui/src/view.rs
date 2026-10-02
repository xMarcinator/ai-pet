//! The pet's surface: every layer pinned where [`crate::geometry`] puts it, bottom to top: the reviews header, the
//! bubbles (back to front, each with its round buttons and dismiss button while the pointer is on it), the ground
//! shadow, the sprite's body, the glow's halo, the glow and effects, the menu when it is drawn inline, which hides
//! everything under it from the pointer, and a tooltip while one shows.

use aipet_core::board::{self, GITHUB_PURPLE};
use iced::widget::canvas::{Frame, Geometry, Path, Stroke, stroke};
use iced::widget::image::{FilterMethod, Handle};
use iced::widget::{Space, button, canvas, container, image, mouse_area, opaque, pin, row, stack, text};
use iced::{
    Border, Color, ContentFit, Element, Length, Padding, Point, Rectangle, Renderer, Shadow, Theme, Vector, mouse,
};

use crate::cards::{Button, Card};
use crate::geometry::{
    BODY, CARD_GLOW_BLUR, CARD_H, CARD_RADIUS, CARD_SHADOW_BLUR, CARD_SHADOW_Y, CLOSE_SIZE, CardFrame, HALO, HEADER_H,
    HEADER_RADIUS, INLINE_MENU,
};
use crate::style::{self, AMBER, Glyph, Icon, argb};
use crate::{Message, PetUi, TIP_FADE, TIP_TEXT};

/// Where a bubble's parts sit in it, unscaled (MakeCard: a 1 px border, padding 14 on the left, the dot in an
/// 18 px box with 8 px after it, the two lines of text centred in the 50 px).
const DOT: Point = Point::new(1.0 + 14.0 + 9.0, CARD_H / 2.0);
const TEXT_X: f32 = 1.0 + 14.0 + 18.0 + 8.0;
const TITLE_SIZE: f32 = 13.5;
const DETAIL_SIZE: f32 = 12.0;
/// The title's top: the lines' heights (1.362 em each) and the detail's 1 px margin, centred.
const TITLE_Y: f32 = (CARD_H - 1.362 * (TITLE_SIZE + DETAIL_SIZE) - 1.0) / 2.0;
const DETAIL_Y: f32 = TITLE_Y + 1.362 * TITLE_SIZE + 1.0;
/// A round button's icon box (MakeIcon's 16 × 16), centred in the button.
const ICON: f32 = 16.0;

impl PetUi {
    /// The pet's whole surface ([`crate::SURFACE`]), transparent where nothing is drawn. It reports the pointer
    /// ([`Message::PointerMoved`], [`Message::PointerLeft`]) itself.
    pub fn pet_view(&self) -> Element<'_, Message> {
        let mut layers: Vec<Element<'_, Message>> = Vec::new();
        if let (Some((_, label, _)), Some(r)) = (&self.header, self.header_rect()) {
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
        let body = self.motion.rect_to_surface(BODY);
        let halo = self.motion.rect_to_surface(HALO);
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
            // opaque: a press on its padding or separator stays in the menu, and the bubbles under it don't see
            // the pointer
            layers.push(pin(opaque(self.menu_view())).position(INLINE_MENU.position()).into());
        }
        if let Some(tip) = self.tip_view() {
            layers.push(tip);
        }
        mouse_area(stack(layers).width(Length::Fill).height(Length::Fill))
            .on_move(Message::PointerMoved)
            .on_exit(Message::PointerLeft)
            .into()
    }

    /// A bubble's shadow at scale `s`, faded with it: a soft drop shadow (0 4 18 #61000000), or for one that needs
    /// you an amber glow, breathing.
    pub(crate) fn card_shadow(&self, card: &Card, s: f32) -> Shadow {
        let shadow = if card.state == "attention" {
            let alpha = (0x40 as f32 + 0x90 as f32 * (0.5 + 0.5 * (self.t * 3.5).sin() as f32)) / 255.0;
            Shadow {
                color: Color {
                    a: alpha,
                    ..argb(AMBER)
                },
                offset: Vector::ZERO,
                blur_radius: CARD_GLOW_BLUR * s,
            }
        } else {
            Shadow {
                color: argb(0x61000000),
                offset: Vector::new(0.0, CARD_SHADOW_Y * s),
                blur_radius: CARD_SHADOW_BLUR * s,
            }
        };
        Shadow {
            color: shadow.color.scale_alpha(card.o),
            ..shadow
        }
    }

    /// One bubble's layers: its body, then (while its text shows) the pulsing dot and the text, then its round
    /// buttons and dismiss button while the pointer is on it. Everything fades with the bubble and scales with it.
    /// Only a bubble whose text shows takes the pointer: the ones folded behind the front one don't, as in the C#.
    fn card<'a>(&'a self, card: &'a Card, frame: CardFrame, layers: &mut Vec<Element<'a, Message>>) {
        if !card.drawn() {
            return;
        }
        let (o, s) = (card.o, frame.scale);
        let highlight = card.state == "attention";
        // StyleBody: the app's colour, or amber for one that needs you, or a faint edge
        let (edge, edge_width) = match (card.app, highlight) {
            (Some((_, app)), _) => (argb(app & 0x00FFFFFF | 0xB8000000), 1.5),
            (None, true) => (argb(AMBER & 0x00FFFFFF | 0xE0000000), 1.5),
            (None, false) => (argb(0x26FFFFFF), 1.0),
        };
        let background = argb(if highlight { 0xF72E291E } else { 0xF5252528 });
        let shadow = self.card_shadow(card, s);
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
        let plate: Element<'a, Message> = if card.content {
            mouse_area(plate)
                .on_press(Message::CardPressed(card.id.clone()))
                .interaction(mouse::Interaction::Pointer)
                .into()
        } else {
            plate.into()
        };
        layers.push(pin(plate).position(body.position()).into());

        if card.content {
            let colour = argb(dot_colour(card.kind, card.state));
            let dot = frame.map(DOT);
            if card.pulsing() {
                let phase = ((self.t - card.born_at) % 1.3 / 1.3) as f32;
                let grow = 1.0 - (1.0 - phase).powi(3);
                let alpha = 0.55 * (1.0 - phase) * o;
                layers.push(disc(dot, 8.0 * (1.0 + 1.6 * grow) * s, colour.scale_alpha(alpha)));
            }
            layers.push(disc(dot, 8.0 * s, colour.scale_alpha(o)));
            let title_colour = if highlight { argb(0xFFFFE9A8) } else { Color::WHITE };
            let fitted = card.fitted();
            layers.push(line(
                &fitted.title,
                TITLE_SIZE * s,
                style::UI_SEMIBOLD,
                title_colour.scale_alpha(o),
                frame.map(Point::new(TEXT_X, TITLE_Y)),
            ));
            layers.push(line(
                &fitted.detail,
                DETAIL_SIZE * s,
                style::UI,
                argb(0xFFA8A8AE).scale_alpha(o),
                frame.map(Point::new(TEXT_X, DETAIL_Y)),
            ));
        }

        if card.interactive() {
            for (button, r) in card.button_rects(&frame) {
                layers.push(pin(round_button(card, button, r, o, s)).position(r.position()).into());
            }
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
                .on_press(Message::Dismiss(card.id.clone()))
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

    /// The tooltip while one shows (Avalonia 12's ToolTip in the Fluent dark theme): white 12 px text on #2B2B2B,
    /// with a 1 px border of black at 36 %, corners of 5 px and padding 8, 5, 8, 7, fading in.
    fn tip_view(&self) -> Option<Element<'_, Message>> {
        let shown = self.open_tip()?;
        let alpha = ((self.t - shown.at) / TIP_FADE).clamp(0.0, 1.0) as f32;
        let label = text(shown.text.as_str())
            .size(TIP_TEXT)
            .font(style::UI)
            .line_height(style::LINE_HEIGHT)
            .shaping(text::Shaping::Advanced)
            .wrapping(text::Wrapping::Word)
            .color(Color::WHITE.scale_alpha(alpha));
        // Avalonia's padding is inside the border; iced draws the border over its padding, so that counts it too
        let padding = Padding {
            top: 1.0 + 5.0,
            right: 1.0 + 8.0,
            bottom: 1.0 + 7.0,
            left: 1.0 + 8.0,
        };
        let r = shown.rect;
        let tip = container(label)
            .padding(padding)
            .width(r.width)
            .height(r.height)
            .style(move |_| container::Style {
                background: Some(argb(0xFF2B2B2B).scale_alpha(alpha).into()),
                border: Border {
                    color: Color::BLACK.scale_alpha(0.36 * alpha),
                    width: 1.0,
                    radius: 5.0.into(),
                },
                ..container::Style::default()
            });
        Some(pin(tip).position(r.position()).into())
    }
}

/// A bubble's dot colour: a GitHub review's purple, else its state's from the Board's table (the C#'s StatusColor,
/// which has a paused player's green), and for a state that table lacks the pet's grey.
fn dot_colour(kind: &str, state: &str) -> u32 {
    if kind == "github" {
        GITHUB_PURPLE
    } else {
        board::status_color(state).unwrap_or_else(|| style::status_colour(state))
    }
}

/// One of a bubble's round buttons (App.axaml's Button.round): 32 px round, #3A3A3E (#4A4A4F hovered, #58585E
/// pressed) with a faint 1 px edge, its icon in the middle; it fades and scales with the bubble.
fn round_button<'a>(card: &Card, b: Button, r: Rectangle, o: f32, s: f32) -> Element<'a, Message> {
    let icon = ButtonIcon {
        icon: match b {
            Button::Ticket | Button::Open => Symbol::Open,
            Button::PullRequest => Symbol::PullRequest,
            Button::Previous => Symbol::Previous,
            // the player's state: pause while it plays
            Button::PlayPause if card.state == "music" => Symbol::Pause,
            Button::PlayPause => Symbol::Play,
            Button::Next => Symbol::Next,
        },
        colour: style::ICON.scale_alpha(o),
    };
    button(container(canvas(icon).width(ICON * s).height(ICON * s)).center(Length::Fill))
        .width(r.width)
        .height(r.height)
        .padding(0)
        .on_press(Message::Button(card.id.clone(), b))
        .style(move |_, status| button::Style {
            background: Some(
                argb(match status {
                    button::Status::Pressed => 0xFF58585E,
                    button::Status::Hovered => 0xFF4A4A4F,
                    _ => 0xFF3A3A3E,
                })
                .scale_alpha(o)
                .into(),
            ),
            text_color: Color::WHITE,
            border: Border {
                color: argb(0x18FFFFFF).scale_alpha(o),
                width: s,
                radius: (r.width / 2.0).into(),
            },
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// The icons on the round buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Symbol {
    /// An arrow out of a box: Open in Jira, and a chat's Open.
    Open,
    /// Two branches joined: the pull request's.
    PullRequest,
    Previous,
    Pause,
    Play,
    Next,
}

/// A round button's icon: MakeCard's path data in its 16 × 16 box, with the icon pen (1.6 px, 1.5 for the pull
/// request's; round caps and joins), the triangles also filled.
#[derive(Clone, Copy, Debug)]
struct ButtonIcon {
    icon: Symbol,
    colour: Color,
}

impl<Message> canvas::Program<Message> for ButtonIcon {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        // scaled with the widget, so an icon on a scaled bubble shrinks with it
        let k = bounds.width / ICON;
        let at = |x: f32, y: f32| Point::new(x * k, y * k);
        let pen = |width: f32| {
            Stroke::default()
                .with_color(self.colour)
                .with_width(width * k)
                .with_line_cap(stroke::LineCap::Round)
                .with_line_join(stroke::LineJoin::Round)
        };
        let triangle = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
            Path::new(|p| {
                p.move_to(at(a.0, a.1));
                p.line_to(at(b.0, b.1));
                p.line_to(at(c.0, c.1));
                p.close();
            })
        };
        let bar = |x: f32| Path::line(at(x, 3.5), at(x, 12.5));
        match self.icon {
            // M9,3 H13 V7 M13,3 L7.5,8.5 M11,9.5 V12.5 A0.5,0.5 0 0 1 10.5,13 H3.5 A0.5,0.5 0 0 1 3,12.5 V5.5
            // A0.5,0.5 0 0 1 3.5,5 H6.5
            Symbol::Open => {
                let path = Path::new(|p| {
                    p.move_to(at(9.0, 3.0));
                    p.line_to(at(13.0, 3.0));
                    p.line_to(at(13.0, 7.0));
                    p.move_to(at(13.0, 3.0));
                    p.line_to(at(7.5, 8.5));
                    p.move_to(at(11.0, 9.5));
                    p.arc_to(at(11.0, 13.0), at(3.5, 13.0), 0.5 * k);
                    p.arc_to(at(3.0, 13.0), at(3.0, 5.5), 0.5 * k);
                    p.arc_to(at(3.0, 5.0), at(6.5, 5.0), 0.5 * k);
                    p.line_to(at(6.5, 5.0));
                });
                frame.stroke(&path, pen(1.6));
            }
            // M4.5,2.5 A1.5,1.5 0 1 1 4.49,2.5 M4.5,5.5 V13.5 M11.5,10.5 A1.5,1.5 0 1 1 11.49,10.5 M11.5,10.5 V6.5
            // A2,2 0 0 0 9.5,4.5 H7 M8.5,3 L7,4.5 L8.5,6 (the two almost-closed arcs are the branches' circles)
            Symbol::PullRequest => {
                let path = Path::new(|p| {
                    p.circle(at(4.5, 4.0), 1.5 * k);
                    p.move_to(at(4.5, 5.5));
                    p.line_to(at(4.5, 13.5));
                    p.circle(at(11.5, 12.0), 1.5 * k);
                    p.move_to(at(11.5, 10.5));
                    p.arc_to(at(11.5, 4.5), at(7.0, 4.5), 2.0 * k);
                    p.line_to(at(7.0, 4.5));
                    p.move_to(at(8.5, 3.0));
                    p.line_to(at(7.0, 4.5));
                    p.line_to(at(8.5, 6.0));
                });
                frame.stroke(&path, pen(1.5));
            }
            // M4,3.5 V12.5 M12.5,3.5 L6.5,8 L12.5,12.5 Z, filled
            Symbol::Previous => {
                let head = triangle((12.5, 3.5), (6.5, 8.0), (12.5, 12.5));
                frame.fill(&head, self.colour);
                frame.stroke(&head, pen(1.6));
                frame.stroke(&bar(4.0), pen(1.6));
            }
            // M12,3.5 V12.5 M3.5,3.5 L9.5,8 L3.5,12.5 Z, filled
            Symbol::Next => {
                let head = triangle((3.5, 3.5), (9.5, 8.0), (3.5, 12.5));
                frame.fill(&head, self.colour);
                frame.stroke(&head, pen(1.6));
                frame.stroke(&bar(12.0), pen(1.6));
            }
            // M5.5,3.5 V12.5 M10.5,3.5 V12.5
            Symbol::Pause => {
                frame.stroke(&bar(5.5), pen(1.6));
                frame.stroke(&bar(10.5), pen(1.6));
            }
            // M5,3 L13,8 L5,13 Z, filled
            Symbol::Play => {
                let play = triangle((5.0, 3.0), (13.0, 8.0), (5.0, 13.0));
                frame.fill(&play, self.colour);
                frame.stroke(&play, pen(1.6));
            }
        }
        vec![frame.into_geometry()]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_github_reviews_dot_is_purple_and_every_other_is_its_states() {
        assert_eq!(dot_colour("github", "review"), 0xFFA371F7);
        assert_eq!(dot_colour("jira", "review"), 0xFF4C9AFF);
        assert_eq!(dot_colour("github-error", "error"), 0xFFE5484D);
        assert_eq!(dot_colour("music", "paused"), 0xFF5E8F6E);
        let states = [
            "thinking",
            "working",
            "attention",
            "done",
            "idle",
            "sleep",
            "review",
            "music",
            "paused",
            "error",
        ];
        for state in states {
            assert_eq!(Some(dot_colour("chat", state)), board::status_color(state), "{state}");
        }
    }
}
