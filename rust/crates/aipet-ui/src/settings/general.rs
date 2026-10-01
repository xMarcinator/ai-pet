//! Settings' General page (`SettingsWindow.axaml.cs:96-125`): the pet's switches, its position and the data folder,
//! and the updates section ([`super::updates`]). Team defaults (Import defaults…) is still to come.

use iced::Element;
use iced::widget::column;

use super::{Action, button_row, card, pill, section, switch_row};
use crate::{Effect, PetUi};

/// The General page's own state.
#[derive(Debug, Default)]
pub struct General {}

/// What the General page sends of its own (its switches and buttons send the pet's messages and Settings' actions).
#[derive(Clone, Debug)]
pub enum Message {}

pub(super) fn view(pet: &PetUi) -> Element<'_, crate::Message> {
    let accent = pet.accent();
    let mut switches = column![
        switch_row(
            "Show chat bubbles",
            "The stacked bubbles above the pet: your chats, reviews and what's playing.",
            pet.bubbles,
            accent,
            crate::Message::SetBubbles,
        ),
        switch_row(
            "Always on top",
            "Keep the pet above other windows.",
            pet.on_top,
            accent,
            crate::Message::SetOnTop,
        ),
    ]
    .spacing(14);
    let listen = match pet.player() {
        Some(player) => Some(format!("Listen along with {player}")),
        // the demo has no player: there the switch makes the pet bop along all the same
        #[cfg(any(test, feature = "demo"))]
        None if pet.demo.is_some() => Some("Listen along".to_owned()),
        None => None,
    };
    if let Some(listen) = listen {
        switches = switches.push(switch_row(
            listen,
            "Show what's playing in a bubble, and the pet bops along.",
            pet.music,
            accent,
            crate::Message::SetMusic,
        ));
    }
    let action =
        |label, action| pill(label, false, accent).on_press(crate::Message::Settings(super::Message::Action(action)));
    let more = column![
        button_row(
            "Position",
            "Put the pet back in the bottom-right corner of the screen.",
            action("Reset", Action::ResetPosition),
        ),
        button_row(
            "Data folder",
            pet.settings.services.data_dir.display().to_string(),
            action("Open", Action::OpenDataFolder),
        ),
    ]
    .spacing(12);
    let page = column![section("THE PET"), card(switches)];
    #[cfg(any(test, feature = "demo"))]
    let page = page.push(section("PET MOOD")).push(moods(pet));
    page.push(section("MORE"))
        .push(card(more))
        .push(section("ABOUT"))
        .push(super::updates::section(pet))
        .padding(iced::Padding::ZERO.bottom(24))
        .into()
}

/// The moods the demo's Settings can force, for trying the states out; "Demo" follows the script again.
#[cfg(any(test, feature = "demo"))]
fn moods(pet: &PetUi) -> Element<'_, crate::Message> {
    const MOODS: [(Option<&str>, &str); 7] = [
        (None, "Demo"),
        (Some("sleep"), "Sleep"),
        (Some("idle"), "Idle"),
        (Some("thinking"), "Thinking"),
        (Some("working"), "Working"),
        (Some("attention"), "Needs you"),
        (Some("done"), "Done"),
    ];
    let accent = pet.accent();
    iced::widget::Row::with_children(MOODS.iter().map(|&(mood, label)| {
        pill(label, pet.mood == mood, accent)
            .on_press(crate::Message::SetMood(mood))
            .into()
    }))
    .spacing(8)
    .wrap()
    .vertical_spacing(8)
    .into()
}

pub(super) fn update(_pet: &mut PetUi, message: Message) -> Option<Effect> {
    match message {}
}

/// Import defaults…: a team's preset file into the Jira and GitHub settings, which this page does itself (it needs a
/// file picker and the watchers). Not yet.
pub(super) fn import(_pet: &mut PetUi) -> Option<Effect> {
    None
}

pub(super) fn tick(_pet: &mut PetUi) {}
