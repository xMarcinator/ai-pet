//! Settings' Avatars page (`SettingsWindow.axaml.cs:214-261`): every avatar as a still with its accent's dot and
//! whether it is built in or the user's own, to pick one (the pet wears it, and Settings takes its colour); under them
//! the worn pet's themes, colours or sheets, built in and the user's own apart, to pick one, and (for a pet with named
//! colours) the theme editor, always on the theme worn: a built-in theme is locked and can be copied as a new theme, one
//! of the user's can be changed, saved, deleted and copied, each a theme file in the avatars folder's `themes` folder
//! (a sheet theme can't be edited); then "Your own avatars": Reload, and Open folder, which makes the avatars folder
//! first if it is missing.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use aipet_sprite::Avatar;
use aipet_sprite::load::{self, LoadedPack};
use aipet_sprite::pack::{Kind, Theme};
use iced::widget::image::FilterMethod;
use iced::widget::{Row, button, column, container, image, row, text, text_input, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Shadow};

use super::{Tone, button_row, card, dot, pill, status};
use crate::style::{self, argb};
use crate::{Effect, PetUi};

/// The Avatars page's own state.
#[derive(Debug, Default)]
pub struct Avatars {
    /// Why Open folder couldn't make the avatars folder, under "Your own avatars".
    pub folder_error: Option<String>,
    /// The theme editor: the worn theme, or a new one made from it; `None` for a pet without named colours.
    pub theme: Option<Theming>,
    /// Why a theme couldn't be deleted, under the themes.
    pub theme_error: Option<String>,
}

/// The worn pet's theme in the editor: its name, and each named colour as typed.
#[derive(Debug)]
pub struct Theming {
    pub pet: String,
    /// The theme the pet wore when the editor took this up: when it wears another, the editor follows.
    pub worn: String,
    pub editing: Editing,
    pub name: String,
    /// The pet's colour names, in its palette's order, each with its text.
    pub colours: Vec<(String, String)>,
    /// The colours on the pet, standing idle.
    preview: Option<image::Handle>,
    pub error: Option<String>,
}

/// Which theme the editor has, and so what it lets the user do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Editing {
    /// A theme that comes with the pet: locked, and copied to make one of the user's.
    BuiltIn,
    /// The user's theme by the name it had when the editor took it up: saved to its file, deleted, or copied.
    Own(String),
    /// A new theme, not saved yet.
    New,
}

/// The theme in the editor: the worn one, or a new one made from it; `None` for a pet without named colours.
#[doc(hidden)]
pub fn theming(pet: &PetUi) -> Option<&Theming> {
    pet.settings.avatars.theme.as_ref()
}

/// The worn pet's themes as the page shows them, in order: each one's name, whether it is worn, and whether it is
/// one of the user's (which Edit and Delete take).
#[doc(hidden)]
pub fn themes(pet: &PetUi) -> Vec<(&str, bool, bool)> {
    let palette = &pet.avatars[pet.avatar].pack.palette;
    if !palette.has_colours() && palette.themes.is_empty() {
        return Vec::new();
    }
    std::iter::once((palette.theme.as_str(), false))
        .chain(palette.themes.iter().map(|t| (t.name.as_str(), t.file.is_some())))
        .map(|(name, own)| (name, name == pet.theme, own))
        .collect()
}

/// Whether the page has the theme editor: the worn pet has named colours to make a theme from (a pet whose themes are
/// only sheets has none).
#[doc(hidden)]
pub fn makes_themes(pet: &PetUi) -> bool {
    pet.avatars[pet.avatar].pack.palette.has_colours()
}

/// The tiles the page draws, in order: one per pet (FNT-74), its name, and whether it is the one worn.
#[doc(hidden)]
pub fn tiles(pet: &PetUi) -> Vec<(&str, bool)> {
    shown(pet)
        .map(|(i, avatar)| (avatar.pack.pet(), i == pet.avatar))
        .collect()
}

/// Each tile's pet, and whether the drawing it shows is the user's own (from the avatars folder) rather than built in.
#[doc(hidden)]
pub fn own_tiles(pet: &PetUi) -> Vec<(&str, bool)> {
    shown(pet)
        .map(|(_, avatar)| (avatar.pack.pet(), !avatar.builtin))
        .collect()
}

/// Each tile's pet, and its dot's colour (0xAARRGGBB): the accent of the drawing and theme its still shows.
#[doc(hidden)]
pub fn tile_accents(pet: &PetUi) -> Vec<(&str, u32)> {
    shown(pet)
        .filter_map(|(i, avatar)| Some((avatar.pack.pet(), pet.stills.get(i)?.accent)))
        .collect()
}

/// The worn pet's styles as the page shows them, in order: each one's label, its drawing, and whether it is worn.
/// Empty for a pet with one style.
#[doc(hidden)]
pub fn styles(pet: &PetUi) -> Vec<(&str, usize, bool)> {
    let Some(g) = crate::pets(&pet.avatars)
        .into_iter()
        .find(|g| g.styles.contains(&pet.avatar))
    else {
        return Vec::new();
    };
    if g.styles.len() < 2 {
        return Vec::new();
    }
    g.styles
        .iter()
        .map(|&i| (pet.avatars[i].pack.style(), i, i == pet.avatar))
        .collect()
}

/// Wears the worn pet in its style drawn by `i`, in the theme it wears now where that style has one by that name, else
/// in that style's default (FNT-74 R3).
pub(crate) fn select_style(pet: &mut PetUi, i: usize) {
    if i == pet.avatar || !styles(pet).iter().any(|&(_, s, _)| s == i) {
        return;
    }
    let palette = &pet.avatars[i].pack.palette;
    let theme = if palette.names().any(|n| n == pet.theme) {
        pet.theme.clone()
    } else {
        palette.theme.clone()
    };
    pet.wear(i, &theme);
    pet.save_prefs();
}

/// The drawing of `g` in the style its pet was last worn in, while it still has that style.
fn chosen_style(pet: &PetUi, g: &aipet_sprite::load::Group) -> Option<usize> {
    let (style, _) = pet.chosen.get(&g.pet)?;
    g.styles.iter().copied().find(|&i| pet.avatars[i].pack.style() == style)
}

/// The pet worn, as drawn: its pack and sheets, in its theme.
#[doc(hidden)]
pub fn worn(pet: &PetUi) -> LoadedPack {
    pet.pet.loaded()
}

/// Each pet's tile, in order: the drawing it shows (the worn one for the worn pet, else the style it was last worn
/// in, else its default style) and it.
pub(crate) fn shown(pet: &PetUi) -> impl Iterator<Item = (usize, &LoadedPack)> {
    crate::pets(&pet.avatars).into_iter().map(move |g| {
        let i = if g.styles.contains(&pet.avatar) {
            pet.avatar
        } else {
            chosen_style(pet, &g).unwrap_or(g.default)
        };
        (i, &*pet.avatars[i])
    })
}

/// What the Avatars page sends of its own (picking an avatar sends the pet's [`crate::Message::SelectAvatar`], and
/// picking a theme its [`crate::Message::SelectTheme`]).
#[derive(Clone, Debug)]
pub enum Message {
    /// Read the avatars folder again.
    Reload,
    /// Open the avatars folder, made first if it is missing.
    OpenFolder,
    /// Copy as new theme: a new theme for the worn pet in the editor, from the colours it shows.
    CopyTheme,
    /// Delete the worn pet's theme of the user's by this name: its file goes.
    DeleteTheme(String),
    ThemeName(String),
    /// A colour's text, by its index in the pet's palette.
    ThemeColour(usize, String),
    /// Save the theme as a file in the themes folder, read the avatars folder again and wear the pet in it.
    SaveTheme,
    /// Drop the new theme: the editor shows the theme worn again.
    CancelTheme,
}

pub(super) fn view(pet: &PetUi) -> Element<'_, crate::Message> {
    let accent = pet.accent();
    let tiles = shown(pet)
        .filter_map(|(i, avatar)| Some((i, avatar, pet.stills.get(i)?)))
        .map(|(i, avatar, still)| {
            let selected = i == pet.avatar;
            let name = row![
                // in the theme the still shows
                dot(argb(still.accent | 0xFF000000), 9.0),
                text(avatar.pack.pet())
                    .size(13)
                    .font(style::UI_SEMIBOLD)
                    .color(Color::WHITE),
            ]
            .spacing(7)
            .align_y(Alignment::Center);
            // where the drawing shown came from
            let origin = text(if avatar.builtin { "Built in" } else { "Your own" })
                .size(11.5)
                .font(style::UI)
                .color(style::MUTED);
            let name = column![name, origin].spacing(2).align_x(Alignment::Center);
            let tile = column![
                image(still.image.clone())
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
            tooltip(
                tile,
                tip(format!("Wear {}", avatar.pack.pet())),
                tooltip::Position::Bottom,
            )
            .into()
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
            greyed(pill)
        }
    };
    let buttons = row![
        folder_button("Reload", Message::Reload),
        folder_button("Open folder", Message::OpenFolder),
    ]
    .spacing(8);
    let own = button_row(
        "Your own avatars",
        "Put a pet in the avatars folder: a pack folder (a pack.json with its sheets or shapes, or Aseprite art), \
         or a legacy avatar .json file. Theme files go in its themes folder. They show up here after Reload.",
        buttons,
    );
    let mut own = column![own].spacing(8);
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
    // the worn pet's styles, where it has more than one: how it is drawn, beside its themes (FNT-74)
    let list = styles(pet);
    let style_card = (!list.is_empty()).then(|| {
        let pills = list
            .iter()
            .map(|&(label, i, on)| pill(label, on, accent).on_press(crate::Message::SelectStyle(i)).into());
        card(button_row(
            "Style",
            "How the pet is drawn. Each style has its own themes; switching keeps the theme where the style has one by \
             that name.",
            Row::with_children(pills).spacing(8).wrap().vertical_spacing(8),
        ))
    });
    let mut page = column![grid];
    if let Some(style_card) = style_card {
        page = page.push(style_card);
    }
    page.push(card(themes_view(pet, accent, folder)))
        .push(card(own))
        .spacing(16)
        .padding(iced::Padding::ZERO.top(10).bottom(24))
        .into()
}

/// The worn pet's themes as pills, the one worn marked, built in and the user's own apart; then, for a pet with named
/// colours, the editor on the theme worn (or a new one made from it).
fn themes_view(pet: &PetUi, accent: Color, folder: bool) -> Element<'_, crate::Message> {
    let worn = &pet.avatars[pet.avatar];
    let list = themes(pet);
    if list.is_empty() {
        return super::label(
            "Themes",
            format!(
                "{} has no colours to change: only pixel and vector pets with named colours have themes.",
                worn.name()
            ),
        );
    }
    let about = if makes_themes(pet) {
        format!(
            "{} in other colours. Built-in themes are locked: copy one to make your own, which you can change.",
            worn.name()
        )
    } else {
        format!(
            "{} drawn in other sheets. Its themes come with it and can't be edited.",
            worn.name()
        )
    };
    let mut out = column![super::label("Themes", about)].spacing(10);
    // built in, then the user's own where there are any
    for (title, own) in [("Built in", false), ("Your own", true)] {
        let pills: Vec<Element<'_, crate::Message>> = list
            .iter()
            .filter(|&&(_, _, o)| o == own)
            .map(|&(name, on, _)| {
                pill(name, on, accent)
                    .on_press(crate::Message::SelectTheme(name.to_owned()))
                    .into()
            })
            .collect();
        if !pills.is_empty() {
            out = out.push(
                column![
                    text(title).size(11.5).font(style::UI_SEMIBOLD).color(style::MUTED),
                    Row::with_children(pills).spacing(8).wrap().vertical_spacing(8),
                ]
                .spacing(6),
            );
        }
    }
    if let Some(e) = &pet.settings.avatars.theme_error {
        out = out.push(status(e.as_str(), Tone::Bad));
    }
    if let Some(t) = &pet.settings.avatars.theme {
        out = out.push(editor(pet, t, accent, folder));
    }
    out.into()
}

/// The editor: a preview, the name and each named colour. A built-in theme's are locked, and it offers only Copy as
/// new theme; one of the user's offers Save, Delete and Copy as new theme; a new one Save and Cancel.
fn editor<'a>(pet: &'a PetUi, t: &'a Theming, accent: Color, folder: bool) -> Element<'a, crate::Message> {
    let send = |m| crate::Message::Settings(super::Message::Avatars(m));
    let locked = t.editing == Editing::BuiltIn;
    let colours = Row::with_children(t.colours.iter().enumerate().map(|(i, (name, value))| {
        let shown = aipet_sprite::vector::colour(value)
            .ok()
            .flatten()
            .map_or(Color::TRANSPARENT, argb);
        let mut input = text_input("#RRGGBB", value)
            .size(13)
            .font(style::UI)
            .padding([6, 8])
            .width(96)
            .style(super::input_style);
        // a box without on_input is disabled
        if !locked {
            input = input.on_input(move |s| send(Message::ThemeColour(i, s)));
        }
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
    let kind = pet.avatars[pet.avatar].pack.kind;
    let preview: Element<'_, crate::Message> = match &t.preview {
        Some(h) => image(h.clone())
            .width(156)
            .height(144)
            .filter_method(if kind == Kind::Pixel {
                FilterMethod::Nearest
            } else {
                FilterMethod::Linear
            })
            .into(),
        None => iced::widget::Space::new().width(156).height(144).into(),
    };
    let mut name = text_input("My theme", &t.name)
        .size(13)
        .font(style::UI)
        .padding([7, 10])
        .style(super::input_style);
    if !locked {
        name = name.on_input(move |s| send(Message::ThemeName(s)));
    }
    let name = column![
        text("Theme name").size(12).font(style::UI_SEMIBOLD).color(style::ICON),
        name
    ]
    .spacing(6);
    // what writes to the avatars folder is greyed out where there is none (the demo)
    let writes = |p: button::Button<'a, crate::Message>, m| if folder { p.on_press(send(m)) } else { greyed(p) };
    let copy = || writes(pill("Copy as new theme", false, accent), Message::CopyTheme);
    let (title, buttons) = match &t.editing {
        Editing::BuiltIn => (format!("{}'s theme {}", t.pet, t.name), row![copy()]),
        Editing::Own(own) => (
            format!("Edit {}'s theme {own}", t.pet),
            row![
                writes(pill("Save", true, accent), Message::SaveTheme),
                writes(pill("Delete", false, accent), Message::DeleteTheme(own.clone())),
                copy(),
            ],
        ),
        Editing::New => (
            format!("A new theme for {}", t.pet),
            row![
                writes(pill("Save", true, accent), Message::SaveTheme),
                pill("Cancel", false, accent).on_press(send(Message::CancelTheme)),
            ],
        ),
    };
    let mut head = column![text(title).size(14).font(style::UI_SEMIBOLD).color(Color::WHITE)].spacing(4);
    if locked {
        head = head.push(status("Built-in theme: copy it to make your own.", Tone::Muted));
    }
    let mut editor = column![
        head,
        row![preview, column![name, colours].spacing(12)].spacing(16),
        buttons.spacing(8),
    ]
    .spacing(12)
    .padding(iced::Padding::ZERO.top(6));
    if let Some(e) = &t.error {
        editor = editor.push(status(e.as_str(), Tone::Bad));
    }
    editor.into()
}

/// A pill greyed out as a disabled Fluent button is (ButtonBackgroundDisabled and so on), taking no clicks: a pill has
/// no disabled look of its own.
fn greyed(pill: button::Button<'_, crate::Message>) -> button::Button<'_, crate::Message> {
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
        Message::CopyTheme => {
            let t = pet.settings.avatars.theme.take()?;
            let base = &pet.avatars[pet.avatar];
            let taken = |n: &str| base.pack.palette.names().any(|t| t == n);
            let name = (1..)
                .map(|i| {
                    if i == 1 {
                        "My theme".to_owned()
                    } else {
                        format!("My theme {i}")
                    }
                })
                .find(|n| !taken(n))
                .expect("a name is free");
            // the colours as they are in the editor, and its preview of them
            pet.settings.avatars.theme_error = None;
            pet.settings.avatars.theme = Some(Theming {
                editing: Editing::New,
                name,
                error: None,
                ..t
            });
        }
        Message::DeleteTheme(name) => {
            let file = own_theme(pet, &name)?.file.clone()?;
            match aipet_sprite::themes::delete(&pet.settings.services.data_dir, &file) {
                Ok(_) => {
                    pet.settings.avatars.theme_error = None;
                    let found = load::all(&folder);
                    pet.skipped = found.skipped;
                    reload(pet, found.pets);
                    pet.save_prefs();
                }
                Err(e) => pet.settings.avatars.theme_error = Some(e),
            }
        }
        // a built-in theme is locked
        Message::ThemeName(name) => {
            if let Some(t) = &mut pet.settings.avatars.theme
                && t.editing != Editing::BuiltIn
            {
                t.name = name;
                t.error = None;
            }
        }
        Message::ThemeColour(i, value) => {
            if let Some(mut t) = pet.settings.avatars.theme.take() {
                if t.editing != Editing::BuiltIn {
                    if let Some(c) = t.colours.get_mut(i) {
                        c.1 = value;
                    }
                    t.error = None;
                    t.preview = preview(pet, &t);
                }
                pet.settings.avatars.theme = Some(t);
            }
        }
        Message::CancelTheme => show_worn(pet),
        Message::SaveTheme => {
            let t = pet.settings.avatars.theme.take()?;
            if t.editing == Editing::BuiltIn {
                pet.settings.avatars.theme = Some(t);
                return None;
            }
            match save(pet, &t, &folder) {
                Ok(name) => {
                    let found = load::all(&folder);
                    pet.skipped = found.skipped;
                    reload(pet, found.pets);
                    if let Some(i) = pet.avatars.iter().position(|a| a.name() == t.pet) {
                        pet.wear(i, &name);
                        pet.save_prefs();
                    }
                }
                Err(e) => {
                    pet.settings.avatars.theme = Some(Theming { error: Some(e), ..t });
                }
            }
        }
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

/// The worn pet's theme of the user's by `name`.
fn own_theme<'a>(pet: &'a PetUi, name: &str) -> Option<&'a Theme> {
    pet.avatars[pet.avatar]
        .pack
        .palette
        .themes
        .iter()
        .find(|t| t.name == name && t.file.is_some())
}

/// Puts the editor on the theme the pet wears, unless it has it already (with the user's changes, or a new theme made
/// from it and not saved yet). Whatever puts a pet, style or theme on calls it, Settings or not: the pet owns what it
/// wears, and the editor follows.
pub(crate) fn follow(pet: &mut PetUi) {
    let worn = pet.avatars[pet.avatar].name();
    let on = pet
        .settings
        .avatars
        .theme
        .as_ref()
        .is_some_and(|t| t.pet == worn && t.worn == pet.theme);
    if !on {
        show_worn(pet);
    }
}

/// The editor on the theme the pet wears, as it is: locked for a built-in one; none for a pet without named colours.
fn show_worn(pet: &mut PetUi) {
    pet.settings.avatars.theme = None;
    let base = &pet.avatars[pet.avatar];
    if !base.pack.palette.has_colours() {
        return;
    }
    let Some(colours) = base.pack.palette.in_theme(&pet.theme) else {
        return;
    };
    let editing = match own_theme(pet, &pet.theme) {
        Some(_) => Editing::Own(pet.theme.clone()),
        None => Editing::BuiltIn,
    };
    pet.settings.avatars.theme = Some(Theming {
        pet: base.name().to_owned(),
        worn: pet.theme.clone(),
        editing,
        name: pet.theme.clone(),
        colours: colours
            .into_iter()
            .map(|(n, c)| (n, aipet_sprite::vector::colour_name(Some(c))))
            .collect(),
        // the worn pet's tile is the pet in this theme, standing idle
        preview: pet.stills.get(pet.avatar).map(|s| s.image.clone()),
        error: None,
    });
    pet.settings.avatars.theme_error = None;
}

/// The avatars as read again (`FillAvatars`), one tile each. The worn avatar's tile is the one with its name, and
/// when its file has changed the pet wears it as it is now, in its theme if it still has it, else in its default (the
/// C# reapplies a tile clicked again; here the one worn takes no click). When its file has gone, the pet keeps
/// wearing it as it was, with no tile, until it takes another.
fn reload(pet: &mut PetUi, avatars: Vec<LoadedPack>) {
    // the drawing worn, else the worn pet's default style, else the first pet (FNT-74 R3, R4)
    let worn = &pet.avatars[pet.avatar];
    let groups = aipet_sprite::load::groups(&avatars);
    let index = avatars
        .iter()
        .position(|a| a.name() == worn.name())
        .or_else(|| groups.iter().find(|g| g.pet == worn.pack.pet()).map(|g| g.default))
        .unwrap_or_else(|| groups.first().map_or(0, |g| g.default));
    let palette = &avatars[index].pack.palette;
    let theme = if palette.names().any(|n| n == pet.theme) {
        pet.theme.clone()
    } else {
        palette.theme.clone()
    };
    let worn = crate::themed(&avatars[index], &theme);
    // a pack whose art changed is worn as it is now
    if worn != pet.pet.loaded() {
        pet.pet.set_pack(worn.clone());
        pet.sprite = crate::sprite::Sprite::new(&worn);
    }
    // the others in the theme their pet was last worn in, where they have it
    pet.stills = avatars
        .iter()
        .enumerate()
        .map(|(i, a)| match pet.chosen.get(a.pack.pet()) {
            _ if i == index => crate::Still::of(&worn),
            Some((_, theme)) => crate::Still::of(&crate::themed(a, theme)),
            None => crate::Still::of(a),
        })
        .collect();
    pet.avatars = crate::shelf::shelve(avatars);
    pet.chosen.insert(
        pet.avatars[index].pack.pet().to_owned(),
        (pet.avatars[index].pack.style().to_owned(), theme.clone()),
    );
    pet.avatar = index;
    pet.theme = theme;
    follow(pet);
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

/// The worn pet in `t`'s colours (those that read as colours), standing idle.
fn preview(pet: &PetUi, t: &Theming) -> Option<image::Handle> {
    let base = pet.avatars.iter().find(|a| a.name() == t.pet)?;
    let colours: BTreeMap<String, u32> = t
        .colours
        .iter()
        .filter_map(|(n, v)| Some((n.clone(), aipet_sprite::vector::colour(v).ok().flatten()?)))
        .collect();
    Some(crate::sprite::still(&base.unpacked().recoloured(&colours).ok()?))
}

/// Saves `t` with the shared theme code ([`aipet_sprite::themes::save`]) in the avatars folder `avatars`: the colours
/// that differ from the pet's default. An edited theme keeps its file; a new one is named after the pet and the theme.
/// The theme's name, or why it can't be saved.
fn save(pet: &PetUi, t: &Theming, avatars: &Path) -> Result<String, String> {
    if t.name.trim().is_empty() {
        return Err("Give the theme a name.".to_owned());
    }
    let base = pet
        .avatars
        .iter()
        .find(|a| a.name() == t.pet)
        .ok_or("The pet has gone.")?;
    let mut colours = BTreeMap::new();
    for (n, v) in &t.colours {
        let c = aipet_sprite::vector::colour(v.trim())
            .ok()
            .flatten()
            .ok_or_else(|| format!("{}: {v:?} isn't a colour (#RRGGBB).", words(n)))?;
        colours.insert(n.clone(), c);
    }
    let own = match &t.editing {
        Editing::Own(own) => Some(own.as_str()),
        _ => None,
    };
    aipet_sprite::themes::save(avatars, &base.pack, &t.name, &colours, own).map(|saved| saved.name)
}

pub(super) fn tick(_pet: &mut PetUi) {}
