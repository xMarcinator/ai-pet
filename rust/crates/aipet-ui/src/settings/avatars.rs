//! Settings' Avatars page (`SettingsWindow.axaml.cs:214-261`): every avatar as a still with its accent's dot, to pick
//! one (the pet wears it, and Settings takes its colour), then "Your own avatars": Reload, and Open folder, which
//! makes the avatars folder first if it is missing; and Recolour, which makes a new pet from a vector one in colours
//! you pick, saved as a recolour file in the avatars folder.

use std::collections::BTreeMap;
use std::fs;

use aipet_sprite::Avatar;
use aipet_sprite::load::{self, LoadedPack};
use aipet_sprite::pack::Kind;
use iced::widget::image::FilterMethod;
use iced::widget::{Row, button, column, container, image, row, text, text_input, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Shadow};

use super::{Tone, button_row, card, dot, field, pill, status};
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
    /// The recolour being made, while the editor is open.
    pub recolour: Option<Recolouring>,
}

/// A new pet being made from a vector one: its name, and each named colour as typed.
#[derive(Debug)]
pub struct Recolouring {
    pub base: String,
    pub name: String,
    /// The base's colour names, in its palette's order, each with its text.
    pub colours: Vec<(String, String)>,
    /// The new colours on the base, standing idle.
    preview: Option<image::Handle>,
    pub error: Option<String>,
}

/// The vector pets a recolour can start from.
fn bases(pet: &PetUi) -> impl Iterator<Item = &LoadedPack> {
    shown(pet)
        .map(|(_, a)| a)
        .filter(|a| a.pack.kind == Kind::Vector && !a.pack.palette.colours.is_empty())
}

/// The recolour being made, if the editor is open.
#[doc(hidden)]
pub fn recolouring(pet: &PetUi) -> Option<&Recolouring> {
    pet.settings.avatars.recolour.as_ref()
}

/// The tiles the page draws, in order: each avatar's name, and whether it is the one worn. The worn avatar's tile is
/// missing when its file has gone.
#[doc(hidden)]
pub fn tiles(pet: &PetUi) -> Vec<(&str, bool)> {
    shown(pet).map(|(i, avatar)| (avatar.name(), i == pet.avatar)).collect()
}

/// The pet worn, as drawn: its pack and sheets.
#[doc(hidden)]
pub fn worn(pet: &PetUi) -> LoadedPack {
    pet.pet.loaded()
}

fn shown(pet: &PetUi) -> impl Iterator<Item = (usize, &LoadedPack)> {
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
    /// Start a recolour of the vector pet with this name.
    Recolour(String),
    RecolourName(String),
    /// A colour's text, by its index in the base's palette.
    RecolourColour(usize, String),
    /// Save the recolour as a file in the avatars folder, read the folder again and wear it.
    SaveRecolour,
    CancelRecolour,
}

pub(super) fn view(pet: &PetUi) -> Element<'_, crate::Message> {
    let accent = pet.accent();
    let tiles = shown(pet)
        .filter_map(|(i, avatar)| Some((i, avatar, pet.stills.get(i)?)))
        .map(|(i, avatar, still)| {
            let selected = i == pet.avatar;
            let name = row![
                dot(argb(avatar.pack.accent | 0xFF000000), 9.0),
                text(avatar.name())
                    .size(13)
                    .font(style::UI_SEMIBOLD)
                    .color(Color::WHITE),
            ]
            .spacing(7)
            .align_y(Alignment::Center);
            let tile = column![
                image(still.clone())
                    .width(78)
                    .height(72)
                    // pixel art stays crisp; painted and vector stills are drawn larger and scaled down smoothly
                    .filter_method(if avatar.pack.kind == Kind::Pixel {
                        FilterMethod::Nearest
                    } else {
                        FilterMethod::Linear
                    }),
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
            tooltip(tile, tip(format!("Wear {}", avatar.name())), tooltip::Position::Bottom).into()
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
        "Put a pet in the avatars folder: a pack folder (pack.json and its PNG sheets, or an Aseprite json-array \
         export), or an avatar .json file. It shows up here after Reload.",
        buttons,
    );
    let mut own = column![own].spacing(8);
    own = own.push(recolour_view(pet, accent, folder));
    if let Some(line) = &pet.settings.avatars.folder_error {
        own = own.push(status(line.as_str(), Tone::Bad));
    }
    // what the folder had that didn't load, one line each with why
    for skipped in &pet.skipped {
        own = own.push(status(
            format!("Skipped {}: {}", skipped.file, skipped.reason),
            Tone::Bad,
        ));
    }
    column![grid, card(own)]
        .spacing(16)
        .padding(iced::Padding::ZERO.top(10).bottom(24))
        .into()
}

/// Recolour: the vector pets to start from, then the editor: a preview, the name and each named colour.
fn recolour_view(pet: &PetUi, accent: Color, folder: bool) -> Element<'_, crate::Message> {
    let send = |m| crate::Message::Settings(super::Message::Avatars(m));
    let Some(r) = &pet.settings.avatars.recolour else {
        let choices = Row::with_children(bases(pet).map(|a| {
            let p = pill(a.name(), false, accent);
            if folder {
                p.on_press(send(Message::Recolour(a.name().to_owned())))
            } else {
                p
            }
            .into()
        }))
        .spacing(8)
        .wrap()
        .vertical_spacing(8);
        return column![
            super::label(
                "Recolour a pet",
                "Make a new pet from a vector one in your own colours."
            ),
            choices
        ]
        .spacing(10)
        .into();
    };
    let colours = Row::with_children(r.colours.iter().enumerate().map(|(i, (name, value))| {
        let shown = aipet_sprite::vector::colour(value)
            .ok()
            .flatten()
            .map_or(Color::TRANSPARENT, argb);
        let input = text_input("#RRGGBB", value)
            .on_input(move |s| send(Message::RecolourColour(i, s)))
            .size(13)
            .font(style::UI)
            .padding([6, 8])
            .width(96)
            .style(super::input_style);
        column![
            text(words(name)).size(12).font(style::UI_SEMIBOLD).color(style::ICON),
            row![dot(shown, 14.0), input].spacing(8).align_y(Alignment::Center),
        ]
        .spacing(4)
        .width(132)
        .into()
    }))
    .spacing(12)
    .wrap()
    .vertical_spacing(12);
    let preview: Element<'_, crate::Message> = match &r.preview {
        Some(h) => image(h.clone())
            .width(156)
            .height(144)
            .filter_method(FilterMethod::Linear)
            .into(),
        None => iced::widget::Space::new().width(156).height(144).into(),
    };
    let buttons = row![
        pill("Save", true, accent).on_press(send(Message::SaveRecolour)),
        pill("Cancel", false, accent).on_press(send(Message::CancelRecolour)),
    ]
    .spacing(8);
    let mut editor = column![
        text(format!("Recolour {}", r.base))
            .size(14)
            .font(style::UI_SEMIBOLD)
            .color(Color::WHITE),
        row![
            preview,
            column![
                field("Name", "My pet", &r.name, false, move |s| send(Message::RecolourName(
                    s
                ))),
                colours
            ]
            .spacing(12)
        ]
        .spacing(16),
        buttons,
    ]
    .spacing(12);
    if let Some(e) = &r.error {
        editor = editor.push(status(e.as_str(), Tone::Bad));
    }
    editor.into()
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
        Message::Reload => {
            let found = load::all(&folder);
            pet.skipped = found.skipped;
            reload(pet, found.pets);
        }
        // the platform opens only a folder that is there: one that can't be made isn't opened, and the page says why
        Message::Recolour(base) => {
            let b = bases(pet).find(|a| a.name() == base)?;
            let colours = b
                .pack
                .palette
                .colours
                .iter()
                .map(|(n, &c)| (n.clone(), aipet_sprite::vector::colour_name(Some(c))))
                .collect();
            let name = format!("{} (mine)", b.name().split(" (").next().unwrap_or(b.name()));
            let mut r = Recolouring {
                base,
                name,
                colours,
                preview: None,
                error: None,
            };
            r.preview = preview(pet, &r);
            pet.settings.avatars.recolour = Some(r);
        }
        Message::RecolourName(name) => {
            if let Some(r) = &mut pet.settings.avatars.recolour {
                r.name = name;
                r.error = None;
            }
        }
        Message::RecolourColour(i, value) => {
            if let Some(mut r) = pet.settings.avatars.recolour.take() {
                if let Some(c) = r.colours.get_mut(i) {
                    c.1 = value;
                }
                r.error = None;
                r.preview = preview(pet, &r);
                pet.settings.avatars.recolour = Some(r);
            }
        }
        Message::CancelRecolour => pet.settings.avatars.recolour = None,
        Message::SaveRecolour => {
            let r = pet.settings.avatars.recolour.take()?;
            match save(pet, &r, &folder) {
                Ok(name) => {
                    let found = load::all(&folder);
                    pet.skipped = found.skipped;
                    reload(pet, found.pets);
                    if let Some(i) = pet.avatars.iter().position(|a| a.name() == name) {
                        return pet.update(crate::Message::SelectAvatar(i));
                    }
                }
                Err(e) => {
                    pet.settings.avatars.recolour = Some(Recolouring { error: Some(e), ..r });
                }
            }
        }
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
fn reload(pet: &mut PetUi, mut avatars: Vec<LoadedPack>) {
    let (index, gone) = match avatars.iter().position(|a| a.name() == pet.pet.pack().name) {
        Some(i) => {
            // a pack whose art changed is worn as it is now
            if avatars[i] != pet.pet.loaded() {
                pet.pet.set_pack(avatars[i].clone());
                pet.sprite = crate::sprite::Sprite::new(&avatars[i]);
            }
            (i, None)
        }
        None => {
            avatars.push(pet.pet.loaded());
            (avatars.len() - 1, Some(avatars.len() - 1))
        }
    };
    pet.stills = avatars.iter().map(crate::sprite::still).collect();
    pet.avatars = avatars;
    pet.avatar = index;
    pet.settings.avatars.gone = gone;
}

/// A palette name as words: `eyeHi` is "Eye hi", `rim-mid` "Rim mid".
fn words(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c == '-' || c == '_' {
            out.push(' ');
        } else if i == 0 {
            out.extend(c.to_uppercase());
        } else if c.is_uppercase() {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The base in `r`'s colours (those that read as colours), standing idle.
fn preview(pet: &PetUi, r: &Recolouring) -> Option<image::Handle> {
    let mut base = bases(pet).find(|a| a.name() == r.base)?.clone();
    let colours: BTreeMap<String, u32> = r
        .colours
        .iter()
        .filter_map(|(n, v)| Some((n.clone(), aipet_sprite::vector::colour(v).ok().flatten()?)))
        .collect();
    base.pack.recolour(&colours).ok()?;
    Some(crate::sprite::still(&base))
}

/// Writes `r` as a recolour file in `folder`, named after the pet: the colours that differ from the base's. The pet's
/// name, or why it can't be saved.
fn save(pet: &PetUi, r: &Recolouring, folder: &std::path::Path) -> Result<String, String> {
    let name = r.name.trim();
    if name.is_empty() {
        return Err("Give the new pet a name.".to_owned());
    }
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let path = folder.join(format!("{}.json", if stem.is_empty() { "recolour" } else { &stem }));
    // a name already taken is fine only when it's this file's own pet, saved again
    let file: Option<serde_json::Value> = fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let own = file
        .as_ref()
        .is_some_and(|v| v.get("name").and_then(|n| n.as_str()) == Some(name));
    if !own && pet.avatars.iter().any(|a| a.name() == name) {
        return Err(format!("There's already a pet named {name}."));
    }
    if path.exists() && !own {
        return Err(format!("{} is already in the avatars folder.", path.display()));
    }
    // a recolour saved again under its own name keeps the pet it was made from as its base
    let base_name = match file.as_ref().and_then(|v| v.get("base")?.as_str()) {
        Some(b) if own && r.base == name => b.to_owned(),
        _ => r.base.clone(),
    };
    let base = bases(pet)
        .find(|a| a.name() == base_name)
        .ok_or("The pet it starts from has gone.")?;
    let mut palette = serde_json::Map::new();
    for (n, v) in &r.colours {
        let c = aipet_sprite::vector::colour(v.trim())
            .ok()
            .flatten()
            .ok_or_else(|| format!("{n}: {v:?} isn't a colour (#RRGGBB)."))?;
        if base.pack.palette.colours.get(n) != Some(&c) {
            palette.insert(n.clone(), aipet_sprite::vector::colour_name(Some(c)).into());
        }
    }
    let json = serde_json::json!({"base": base_name, "name": name, "palette": palette});
    fs::create_dir_all(folder).map_err(|e| format!("Couldn't make the avatars folder: {e}"))?;
    let text = serde_json::to_string_pretty(&json).expect("a recolour always serialises") + "\n";
    fs::write(&path, text).map_err(|e| format!("Couldn't save {}: {e}", path.display()))?;
    Ok(name.to_owned())
}

pub(super) fn tick(_pet: &mut PetUi) {}
