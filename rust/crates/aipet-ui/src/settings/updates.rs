//! The updates section of Settings' General page (`SettingsWindow.axaml.cs:128-150`): the version, where its update
//! stands ([`super::Services::update_status`]), and "Check for updates" or "Restart to update", which go to the app as
//! [`Action::CheckForUpdates`] and [`Action::RestartToUpdate`].

use aipet_core::update_status::{Kind, UpdateStatus};
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
    let shown = Shown::of(&now);
    let about = column![
        iced::widget::text(format!("AiPet {}", services.version))
            .size(13)
            .font(crate::style::UI_SEMIBOLD)
            .color(iced::Color::WHITE),
        status(now.text(), shown.tone),
    ]
    .spacing(4)
    .width(iced::Length::Fill);
    let mut line = row![about].spacing(12).align_y(iced::Alignment::Center);
    if let Some((label, action)) = shown.button {
        let button = pill(label, false, pet.accent()).on_press_maybe(
            shown
                .enabled
                .then_some(crate::Message::Settings(super::Message::Action(action))),
        );
        line = line.push(button);
    }
    card(line)
}

/// How the section shows an update status (`ShowUpdate`): the line's tone, the button and what it does, and
/// whether it can be pressed.
#[derive(Debug, PartialEq)]
struct Shown {
    tone: Tone,
    button: Option<(&'static str, Action)>,
    enabled: bool,
}

impl Shown {
    fn of(now: &UpdateStatus) -> Shown {
        Shown {
            tone: match now.kind {
                Kind::Failed => Tone::Bad,
                Kind::Ready => Tone::Ok,
                _ => Tone::Muted,
            },
            button: match now.kind {
                Kind::Off => None,
                Kind::Ready => Some(("Restart to update", Action::RestartToUpdate)),
                _ => Some(("Check for updates", Action::CheckForUpdates)),
            },
            enabled: !now.busy(),
        }
    }
}

pub(super) fn update(_pet: &mut PetUi, message: Message) -> Option<Effect> {
    match message {}
}

pub(super) fn tick(_pet: &mut PetUi) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// SettingsWindow.ShowUpdate: no button in a copy that doesn't update itself, Restart to update once an update
    /// is ready, Check for updates otherwise, greyed while a check or a download runs; failures in red, a ready
    /// update in green.
    #[test]
    fn the_section_shows_each_status_as_the_csharp_does() {
        let check = Some(("Check for updates", Action::CheckForUpdates));
        let cases = [
            (Kind::Off, Tone::Muted, None, true),
            (Kind::Idle, Tone::Muted, check, true),
            (Kind::Checking, Tone::Muted, check, false),
            (Kind::UpToDate, Tone::Muted, check, true),
            (Kind::Downloading, Tone::Muted, check, false),
            (
                Kind::Ready,
                Tone::Ok,
                Some(("Restart to update", Action::RestartToUpdate)),
                true,
            ),
            (Kind::Failed, Tone::Bad, check, true),
        ];
        for (kind, tone, button, enabled) in cases {
            assert_eq!(
                Shown::of(&UpdateStatus::new(kind)),
                Shown { tone, button, enabled },
                "{kind:?}"
            );
        }
    }
}
