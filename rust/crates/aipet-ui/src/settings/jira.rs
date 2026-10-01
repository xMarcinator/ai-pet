//! Settings' Jira page (`SettingsWindow.axaml.cs:284-347`): the site, email, search and token, Test, Save and Forget,
//! through [`super::Services`]' watcher, token store and HTTP client. Still to come: the page shows only its title and
//! hint.

use iced::Element;
use iced::widget::Space;

use crate::{Effect, PetUi};

/// The Jira page's own state.
#[derive(Debug, Default)]
pub struct Jira {}

/// What the Jira page sends of its own.
#[derive(Clone, Debug)]
pub enum Message {}

pub(super) fn view(_pet: &PetUi) -> Element<'_, crate::Message> {
    Space::new().into()
}

pub(super) fn update(_pet: &mut PetUi, message: Message) -> Option<Effect> {
    match message {}
}

pub(super) fn tick(_pet: &mut PetUi) {}
