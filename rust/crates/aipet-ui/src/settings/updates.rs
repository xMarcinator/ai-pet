//! The updates section of Settings' General page (`SettingsWindow.axaml.cs:128-150`): the version, where its update
//! stands ([`super::Services::update_status`]), and "Check for updates" or "Restart to update", which go to the app as
//! [`Action::CheckForUpdates`] and [`Action::RestartToUpdate`].

use aipet_core::update_status::Kind;
use iced::Element;
use iced::widget::{column, row};

use super::{Action, Tone, card, pill, status};
use crate::{Effect, PetUi};

/// The updates section's own state.
#[derive(Debug, Default)]
pub struct Updates {}

/// What the updates section sends of its own.
#[derive(Clone, Debug)]
pub enum Message {}

/// The section: "AiPet <version>", the update's status line, and its button (none for a copy that doesn't update
/// itself, and greyed while a check or download runs).
pub(super) fn section(pet: &PetUi) -> Element<'_, crate::Message> {
    let services = &pet.settings.services;
    let now = (services.update_status)();
    let tone = match now.kind {
        Kind::Failed => Tone::Bad,
        Kind::Ready => Tone::Ok,
        _ => Tone::Muted,
    };
    let about = column![
        iced::widget::text(format!("AiPet {}", services.version))
            .size(13)
            .font(crate::style::UI_SEMIBOLD)
            .color(iced::Color::WHITE),
        status(now.text(), tone),
    ]
    .spacing(4)
    .width(iced::Length::Fill);
    let mut line = row![about].spacing(12).align_y(iced::Alignment::Center);
    if now.kind != Kind::Off {
        let (label, action) = match now.kind {
            Kind::Ready => ("Restart to update", Action::RestartToUpdate),
            _ => ("Check for updates", Action::CheckForUpdates),
        };
        let button = pill(label, false, pet.accent())
            .on_press_maybe((!now.busy()).then_some(crate::Message::Settings(super::Message::Action(action))));
        line = line.push(button);
    }
    card(line)
}

pub(super) fn update(_pet: &mut PetUi, message: Message) -> Option<Effect> {
    match message {}
}

pub(super) fn tick(_pet: &mut PetUi) {}
