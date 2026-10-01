//! Settings' GitHub page (`SettingsWindow.axaml.cs:350-412`): the host, organisations and token, Test, Save and
//! Forget, through [`super::Services`]' watcher. Still to come: the page shows only its title and hint.

use iced::Element;
use iced::widget::Space;

use crate::{Effect, PetUi};

/// The GitHub page's own state.
#[derive(Debug, Default)]
pub struct GitHub {}

/// What the GitHub page sends of its own.
#[derive(Clone, Debug)]
pub enum Message {}

pub(super) fn view(_pet: &PetUi) -> Element<'_, crate::Message> {
    Space::new().into()
}

pub(super) fn update(_pet: &mut PetUi, message: Message) -> Option<Effect> {
    match message {}
}

pub(super) fn tick(_pet: &mut PetUi) {}
