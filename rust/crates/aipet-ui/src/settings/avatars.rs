//! Settings' Avatars page (`SettingsWindow.axaml.cs:214-261`): every avatar as a still with its accent's dot, to pick
//! one (the pet wears it, and Settings takes its colour), then "Your own avatars": Reload, and Open folder, which
//! makes the avatars folder first if it is missing.

use std::fs;

use aipet_sprite::Avatar;
use iced::widget::image::FilterMethod;
use iced::widget::{Row, button, column, container, image, row, text, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Shadow};

use super::{button_row, card, dot, pill};
use crate::style::{self, argb};
use crate::{Effect, PetUi};

/// The Avatars page's own state.
#[derive(Debug, Default)]
pub struct Avatars {}

/// What the Avatars page sends of its own (picking an avatar sends the pet's [`crate::Message::SelectAvatar`]).
#[derive(Clone, Debug)]
pub enum Message {
    /// Read the avatars folder again.
    Reload,
    /// Open the avatars folder, made first if it is missing.
    OpenFolder,
}

pub(super) fn view(pet: &PetUi) -> Element<'_, crate::Message> {
    let accent = pet.accent();
    let tiles = pet
        .avatars
        .iter()
        .zip(&pet.stills)
        .enumerate()
        .map(|(i, (avatar, still))| {
            let selected = i == pet.avatar;
            let name = row![
                dot(argb(avatar.accent | 0xFF000000), 9.0),
                text(&avatar.name).size(13).font(style::UI_SEMIBOLD).color(Color::WHITE),
            ]
            .spacing(7)
            .align_y(Alignment::Center);
            let tile = column![
                image(still.clone())
                    .width(78)
                    .height(72)
                    .filter_method(FilterMethod::Nearest),
                name,
            ]
            .spacing(8)
            .align_x(Alignment::Center);
            let tile = button(container(tile).center_x(Length::Fill))
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
                });
            tooltip(tile, tip(format!("Wear {}", avatar.name)), tooltip::Position::Bottom).into()
        });
    let grid = Row::with_children(tiles).spacing(12).wrap().vertical_spacing(12);
    // the demo shows the repository's avatars, which aren't in a data folder: there is nothing to reload or open
    #[cfg(any(test, feature = "demo"))]
    let folder = pet.demo.is_none();
    #[cfg(not(any(test, feature = "demo")))]
    let folder = true;
    let send = |message| folder.then_some(crate::Message::Settings(super::Message::Avatars(message)));
    let buttons = row![
        pill("Reload", false, accent).on_press_maybe(send(Message::Reload)),
        pill("Open folder", false, accent).on_press_maybe(send(Message::OpenFolder)),
    ]
    .spacing(8);
    let own = button_row(
        "Your own avatars",
        "Put an avatar .json file (a copy of a built-in one with your own colours and shape) in the avatars folder; \
         it shows up here.",
        buttons,
    );
    column![grid, card(own)]
        .spacing(16)
        .padding(iced::Padding::ZERO.top(10).bottom(24))
        .into()
}

/// A tile's tooltip, in the menu's panel colours.
fn tip<'a>(line: String) -> Element<'a, crate::Message> {
    container(text(line).size(12).font(style::UI).color(style::ICON))
        .padding([5, 8])
        .style(|_| container::Style {
            background: Some(style::PANEL_BG.into()),
            border: Border {
                color: style::PANEL_EDGE,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub(super) fn update(pet: &mut PetUi, message: Message) -> Option<Effect> {
    let folder = Avatar::custom_dir(&pet.settings.services.data_dir);
    match message {
        Message::Reload => reload(pet, Avatar::all(&folder)),
        Message::OpenFolder => {
            // a folder that can't be made can't be opened either: the platform says so, if anything
            let _ = fs::create_dir_all(&folder);
            pet.platform.open_folder(&folder);
        }
    }
    None
}

/// The avatars as read again (`FillAvatars`). The pet keeps wearing its avatar as it was: its tile is the one with
/// its name, or, when its file has gone, a tile of its own after the others, until it takes another.
fn reload(pet: &mut PetUi, mut avatars: Vec<Avatar>) {
    let worn = pet.pet.avatar();
    let index = match avatars.iter().position(|a| a.name == worn.name) {
        Some(i) => i,
        None => {
            avatars.push(worn.clone());
            avatars.len() - 1
        }
    };
    pet.stills = avatars.iter().map(crate::sprite::still).collect();
    pet.avatars = avatars;
    pet.avatar = index;
}

pub(super) fn tick(_pet: &mut PetUi) {}
