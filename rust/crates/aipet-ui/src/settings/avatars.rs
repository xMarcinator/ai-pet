//! Settings' Avatars page (`SettingsWindow.axaml.cs:214-261`): every avatar as a still, to pick one. Reload and the
//! avatars folder are still to come.

use iced::widget::image::FilterMethod;
use iced::widget::{Row, button, column, container, image, text};
use iced::{Alignment, Border, Color, Element, Length, Shadow};

use crate::style::{self, argb};
use crate::{Effect, PetUi};

/// The Avatars page's own state.
#[derive(Debug, Default)]
pub struct Avatars {}

/// What the Avatars page sends of its own (picking an avatar sends the pet's [`crate::Message::SelectAvatar`]).
#[derive(Clone, Debug)]
pub enum Message {}

pub(super) fn view(pet: &PetUi) -> Element<'_, crate::Message> {
    let accent = pet.accent();
    let tiles = pet
        .avatars
        .iter()
        .zip(&pet.stills)
        .enumerate()
        .map(|(i, (avatar, still))| {
            let selected = i == pet.avatar;
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
                .on_press(crate::Message::SelectAvatar(i))
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
        });
    Row::with_children(tiles).spacing(12).wrap().vertical_spacing(12).into()
}

pub(super) fn update(_pet: &mut PetUi, message: Message) -> Option<Effect> {
    match message {}
}

pub(super) fn tick(_pet: &mut PetUi) {}
