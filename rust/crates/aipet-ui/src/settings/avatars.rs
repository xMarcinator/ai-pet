//! Settings' Avatars page (`SettingsWindow.axaml.cs:214-261`): every avatar as a still with its accent's dot, to pick
//! one (the pet wears it, and Settings takes its colour), then "Your own avatars": Reload, and Open folder, which
//! makes the avatars folder first if it is missing.

use std::fs;

use aipet_sprite::Avatar;
use iced::widget::image::FilterMethod;
use iced::widget::{Row, button, column, container, image, row, text, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Shadow};

use super::{Tone, button_row, card, dot, pill, status};
use crate::style::{self, argb};
use crate::{Effect, PetUi};

/// The Avatars page's own state.
#[derive(Debug, Default)]
pub struct Avatars {
    /// The pet's list keeps the avatar worn when Reload found its file gone, so that the pet still wears it, but the
    /// page draws no tile for it: `FillAvatars` shows only what `Avatar.All()` reads.
    gone: Option<usize>,
    /// Why Open folder couldn't make the avatars folder, under "Your own avatars".
    pub folder_error: Option<String>,
}

/// The tiles the page draws, in order: each avatar's name, and whether it is the one worn. The worn avatar's tile is
/// missing when its file has gone.
#[doc(hidden)]
pub fn tiles(pet: &PetUi) -> Vec<(&str, bool)> {
    shown(pet)
        .map(|(i, avatar)| (avatar.name.as_str(), i == pet.avatar))
        .collect()
}

/// The avatar the pet wears, as drawn.
#[doc(hidden)]
pub fn worn(pet: &PetUi) -> &Avatar {
    pet.pet.avatar()
}

fn shown(pet: &PetUi) -> impl Iterator<Item = (usize, &Avatar)> {
    let gone = pet.settings.avatars.gone;
    pet.avatars.iter().enumerate().filter(move |&(i, _)| Some(i) != gone)
}

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
    let tiles = shown(pet)
        .filter_map(|(i, avatar)| Some((i, avatar, pet.stills.get(i)?)))
        .map(|(i, avatar, still)| {
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
    let folder_button = |label, message| {
        let pill = pill(label, false, accent);
        if folder {
            pill.on_press(crate::Message::Settings(super::Message::Avatars(message)))
        } else {
            // greyed out as a disabled Fluent button is (ButtonBackgroundDisabled and so on): a pill has no disabled
            // look of its own
            pill.style(|_, _| button::Style {
                background: Some(argb(0x0BFFFFFF).into()),
                text_color: argb(0x5DFFFFFF),
                border: Border {
                    color: argb(0x12FFFFFF),
                    width: 1.0,
                    radius: 16.0.into(),
                },
                shadow: Shadow::default(),
                snap: true,
            })
        }
    };
    let buttons = row![
        folder_button("Reload", Message::Reload),
        folder_button("Open folder", Message::OpenFolder),
    ]
    .spacing(8);
    let own = button_row(
        "Your own avatars",
        "Put an avatar .json file (a copy of a built-in one with your own colours and shape) in the avatars folder; \
         it shows up here.",
        buttons,
    );
    let mut own = column![own].spacing(8);
    if let Some(line) = &pet.settings.avatars.folder_error {
        own = own.push(status(line.as_str(), Tone::Bad));
    }
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
        // the platform opens only a folder that is there: one that can't be made isn't opened, and the page says why
        Message::OpenFolder => match fs::create_dir_all(&folder) {
            Ok(()) => {
                pet.settings.avatars.folder_error = None;
                pet.platform.open_folder(&folder);
            }
            Err(e) => {
                pet.settings.avatars.folder_error =
                    Some(format!("Couldn't make the avatars folder {}: {e}", folder.display()))
            }
        },
    }
    None
}

/// The avatars as read again (`FillAvatars`), one tile each. The worn avatar's tile is the one with its name, and
/// when its file has changed the pet wears it as it is now (the C# reapplies a tile clicked again; here the one worn
/// takes no click). When its file has gone, the pet keeps wearing it as it was, with no tile, until it takes another.
fn reload(pet: &mut PetUi, mut avatars: Vec<Avatar>) {
    let worn = pet.pet.avatar().clone();
    let (index, gone) = match avatars.iter().position(|a| a.name == worn.name) {
        Some(i) => {
            if avatars[i] != worn {
                pet.pet.set_avatar(avatars[i].clone());
            }
            (i, None)
        }
        None => {
            avatars.push(worn);
            (avatars.len() - 1, Some(avatars.len() - 1))
        }
    };
    pet.stills = avatars.iter().map(crate::sprite::still).collect();
    pet.avatars = avatars;
    pet.avatar = index;
    pet.settings.avatars.gone = gone;
}

pub(super) fn tick(_pet: &mut PetUi) {}
