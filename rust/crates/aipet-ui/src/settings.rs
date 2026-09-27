//! The Settings window's content, in the look of `SettingsWindow.axaml`: sections of rounded cards on a dark panel.

use iced::widget::image::FilterMethod;
use iced::widget::{Space, button, column, container, image, row, scrollable, text, toggler};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow};

use crate::style::{self, argb};
use crate::{Message, PetUi};

/// The moods Settings can force, with their labels; `None` follows the demo script.
const MOODS: [(Option<&'static str>, &str); 7] = [
    (None, "Demo"),
    (Some("sleep"), "Sleep"),
    (Some("idle"), "Idle"),
    (Some("thinking"), "Thinking"),
    (Some("working"), "Working"),
    (Some("attention"), "Needs you"),
    (Some("done"), "Done"),
];

impl PetUi {
    /// Settings: the pet's switches (bubbles, music), a mood to force for trying the states out, the avatars, and
    /// Quit. It paints its own opaque background, since the pet's surfaces share a transparent clear colour.
    pub fn settings_view(&self) -> Element<'_, Message> {
        let accent = self.accent();
        let title = row![
            container(Space::new().width(9).height(9)).style(move |_| container::Style {
                background: Some(accent.into()),
                border: Border {
                    radius: 4.5.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
            text("AiPet settings").size(19).font(style::UI_SEMIBOLD),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let hint = text("The Rust spike of the pet's front end. The bubbles are a demo script; nothing is saved.")
            .size(12.5)
            .font(style::UI)
            .color(style::MUTED);

        let switches = card(
            column![
                switch_row(
                    "Show chat bubbles",
                    "The stacked bubbles above the pet.",
                    self.bubbles,
                    accent,
                    Message::SetBubbles,
                ),
                switch_row(
                    "Listen along",
                    "The pet puts its headphones on and bops along when it has nothing else to do.",
                    self.music,
                    accent,
                    Message::SetMusic,
                ),
            ]
            .spacing(14),
        );

        let moods = row(MOODS.iter().map(|&(mood, label)| {
            pill(label, self.mood == mood, accent)
                .on_press(Message::SetMood(mood))
                .into()
        }))
        .spacing(8)
        .wrap()
        .vertical_spacing(8);

        let avatars = row(self
            .avatars
            .iter()
            .zip(&self.stills)
            .enumerate()
            .map(|(i, (avatar, still))| {
                let selected = i == self.avatar;
                let tile = column![
                    image(still.clone())
                        .width(78)
                        .height(72)
                        .filter_method(FilterMethod::Nearest),
                    text(&avatar.name).size(13).font(style::UI_SEMIBOLD).color(Color::WHITE),
                ]
                .spacing(8)
                .align_x(Alignment::Center);
                button(container(tile).center_x(Length::Fill))
                    .width(138)
                    .padding([12, 10])
                    .on_press(Message::SelectAvatar(i))
                    .style(move |_, status| button::Style {
                        background: Some(
                            argb(match (selected, status) {
                                (true, _) => 0xFF26262A,
                                (false, button::Status::Hovered | button::Status::Pressed) => 0xFF242428,
                                _ => 0xFF1C1C1F,
                            })
                            .into(),
                        ),
                        text_color: Color::WHITE,
                        border: Border {
                            color: if selected { accent } else { argb(0xFF2C2C31) },
                            width: 1.5,
                            radius: 12.0.into(),
                        },
                        shadow: Shadow::default(),
                        snap: true,
                    })
                    .into()
            }))
        .spacing(12)
        .wrap()
        .vertical_spacing(12);

        let quit = card(
            row![
                column![
                    text("Quit").size(13).font(style::UI_SEMIBOLD).color(Color::WHITE),
                    text("Close the spike.").size(11.5).font(style::UI).color(style::MUTED),
                ]
                .spacing(4)
                .width(Length::Fill),
                pill("Quit", false, accent).on_press(Message::Quit),
            ]
            .align_y(Alignment::Center),
        );

        let page = column![
            column![title, hint].spacing(6),
            section("THE PET"),
            switches,
            section("PET MOOD"),
            moods,
            section("AVATARS"),
            avatars,
            section("MORE"),
            quit,
        ]
        .padding([24, 28]);
        container(scrollable(page).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(argb(0xFF232326).into()),
                text_color: Some(style::ICON),
                ..container::Style::default()
            })
            .into()
    }
}

/// A section's small capitals heading.
fn section<'a>(label: &'a str) -> Element<'a, Message> {
    container(text(label).size(11).font(style::UI_SEMIBOLD).color(argb(0xFF8A8A92)))
        .padding([18, 2])
        .into()
}

/// A rounded group of rows (Border.card).
fn card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .padding([12, 16])
        .style(|_| container::Style {
            background: Some(argb(0xFF1C1C1F).into()),
            border: Border {
                color: argb(0xFF2C2C31),
                width: 1.0,
                radius: 10.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// A setting with a title, a hint and a switch.
fn switch_row<'a>(
    title: &'a str,
    hint: &'a str,
    on: bool,
    accent: Color,
    message: fn(bool) -> Message,
) -> Element<'a, Message> {
    let texts = column![
        text(title).size(13).font(style::UI_SEMIBOLD).color(Color::WHITE),
        text(hint).size(11.5).font(style::UI).color(style::MUTED),
    ]
    .spacing(4)
    .width(Length::Fill);
    let switch = toggler(on).on_toggle(message).size(20).style(move |_, status| {
        let on = matches!(
            status,
            toggler::Status::Active { is_toggled: true } | toggler::Status::Hovered { is_toggled: true }
        );
        toggler::Style {
            background: Background::Color(if on { accent } else { Color::TRANSPARENT }),
            background_border_width: if on { 0.0 } else { 1.0 },
            background_border_color: argb(0x99FFFFFF),
            foreground: Background::Color(if on { Color::WHITE } else { argb(0xFFC8C8CC) }),
            foreground_border_width: 0.0,
            foreground_border_color: Color::TRANSPARENT,
            text_color: None,
            border_radius: None,
            padding_ratio: 0.2,
        }
    });
    row![texts, switch].spacing(12).align_y(Alignment::Center).into()
}

/// A round-ended button (Button.pill); `primary` fills it with the accent.
fn pill<'a>(label: &'a str, primary: bool, accent: Color) -> button::Button<'a, Message> {
    button(text(label).size(13).font(style::UI_SEMIBOLD))
        .padding([7, 16])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let background = match (primary, hovered) {
                (true, false) => accent,
                (true, true) => accent.scale_alpha(0.88),
                (false, false) => argb(0xFF3A3A3E),
                (false, true) => argb(0xFF4A4A4F),
            };
            button::Style {
                background: Some(background.into()),
                text_color: if primary || hovered { Color::WHITE } else { style::ICON },
                border: Border {
                    color: argb(0x18FFFFFF),
                    width: 1.0,
                    radius: 16.0.into(),
                },
                shadow: Shadow::default(),
                snap: true,
            }
        })
}
