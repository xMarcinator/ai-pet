//! The Settings window (`SettingsWindow.axaml`): a sidebar of pages (General, Avatars, Jira, GitHub), the page's title
//! and hint, and the page. This file is the window, the navigation, the rows, cards and fields the pages share, and
//! the [`Action`]s Settings asks of the app; each page is a file of its own, which fills it without touching this one:
//! - [`general`]: the pet's switches, Reset position, the data folder, Import defaults… and the updates section;
//! - [`avatars`]: the avatars, Reload and the avatars folder;
//! - [`jira`] and [`github`]: their fields, Test, Save and Forget;
//! - [`updates`]: the version and where its update stands, shown on the General page.
//!
//! A page is a state struct kept in [`Settings`], a `Message` enum of its own (wrapped in this file's [`Message`]),
//! and three functions this file calls: `view(&PetUi)`, `update(&mut PetUi, Message)` and `tick(&mut PetUi)`. A page
//! reads and changes the pet ([`PetUi`]'s switches, avatars and [`Settings::services`]) through them; work that takes
//! long (a Test, an import) runs on a thread, and the page picks its outcome up in `tick`, which runs every frame.

pub mod avatars;
pub mod general;
pub mod github;
pub mod jira;
pub mod updates;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use aipet_core::github::GitHubWatcher;
use aipet_core::jira::{Http, JiraWatcher};
use aipet_core::secrets::{MemorySecrets, SecretStore};
use aipet_core::update_status::{Kind, UpdateStatus};
use iced::widget::text::IntoFragment;
use iced::widget::{Space, button, column, container, row, scrollable, text, text_input, toggler};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow};

use crate::style::{self, argb};
use crate::{Effect, PetUi};

/// A page of Settings, in the sidebar's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    General,
    Avatars,
    Jira,
    GitHub,
}

impl Page {
    pub const ALL: [Page; 4] = [Page::General, Page::Avatars, Page::Jira, Page::GitHub];

    /// The page's name in the sidebar, and its title.
    pub fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Avatars => "Avatars",
            Page::Jira => "Jira",
            Page::GitHub => "GitHub",
        }
    }

    /// The line under the title (SettingsWindow's `Pages`).
    pub fn hint(self) -> &'static str {
        match self {
            Page::General => "How the pet behaves on your desktop.",
            Page::Avatars => "Pick how the pet looks. Its colour also tints check marks and buttons.",
            Page::Jira => "A bubble for every issue your search finds, and a wave when a new one turns up.",
            Page::GitHub => {
                "Pull requests waiting on your review. A PR that mentions a Jira key (in its title, branch, description \
                 or commits) joins that Jira bubble, which then gets a button to open it."
            }
        }
    }
}

/// What Settings' buttons ask for beyond Settings itself. The General page's Import defaults… is done by that page
/// (it needs the file picker and the watchers); the others go to the app (`Host::settings`), except Reset position,
/// which needs the window and so goes to the shell ([`Effect::ResetPosition`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Put the pet back in its default corner.
    ResetPosition,
    /// Open the data folder, made first if it is missing.
    OpenDataFolder,
    /// Read a team's preset file into the Jira and GitHub settings.
    ImportDefaults,
    /// Look for a newer release now.
    CheckForUpdates,
    /// Install the update that is ready, and start the pet again.
    RestartToUpdate,
}

/// What Settings sends, wrapped in the pet's [`crate::Message::Settings`].
#[derive(Clone, Debug)]
pub enum Message {
    /// A page of the sidebar was chosen.
    Show(Page),
    /// A button whose work is outside Settings.
    Action(Action),
    General(general::Message),
    Avatars(avatars::Message),
    Jira(jira::Message),
    GitHub(github::Message),
    Updates(updates::Message),
}

/// What the pages work with beyond the pet: the data folder, the review watchers and the token store, the HTTP client
/// Test uses, and the app's version and update status. The app gives the real ones; [`Services::detached`] gives
/// ones that touch nothing.
pub struct Services {
    /// The data folder, as Settings shows it (`Paths.DataDir`).
    pub data_dir: PathBuf,
    /// The watchers the core runs, which Save, Forget and Import defaults change; none where nothing watches (the
    /// demo, tests).
    pub jira: Option<Arc<JiraWatcher>>,
    pub github: Option<Arc<GitHubWatcher>>,
    /// Where the tokens are (the platform's store): Jira's Test reads the saved token when none is typed.
    pub secrets: Arc<dyn SecretStore>,
    /// The client Jira's Test searches with.
    pub http: Http,
    /// The app's version, as released.
    pub version: String,
    /// Where the app's update stands now (the app's `updates::status`).
    pub update_status: fn() -> UpdateStatus,
}

impl Services {
    /// Services that touch nothing: no watchers, an empty token store in memory, a client that reaches nothing but
    /// this computer, and updates off. For the demo and tests; `data_dir` is only shown and opened.
    pub fn detached(data_dir: PathBuf) -> Services {
        Services {
            data_dir,
            jira: None,
            github: None,
            secrets: Arc::new(MemorySecrets::default()),
            http: Http::local(Duration::from_secs(5)),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            update_status: || UpdateStatus::new(Kind::Off),
        }
    }
}

/// Settings' state: the page shown, the services, and each page's own.
pub struct Settings {
    pub page: Page,
    pub services: Services,
    pub general: general::General,
    pub avatars: avatars::Avatars,
    pub jira: jira::Jira,
    pub github: github::GitHub,
    pub updates: updates::Updates,
}

impl Settings {
    pub fn new(services: Services) -> Settings {
        Settings {
            page: Page::default(),
            services,
            general: general::General::default(),
            avatars: avatars::Avatars::default(),
            jira: jira::Jira::default(),
            github: github::GitHub::default(),
            updates: updates::Updates::default(),
        }
    }
}

/// Handles a message of Settings, and says what the shell should do, if anything.
pub(crate) fn update(pet: &mut PetUi, message: Message) -> Option<Effect> {
    match message {
        Message::Show(page) => {
            pet.settings.page = page;
            None
        }
        Message::Action(Action::ResetPosition) => Some(Effect::ResetPosition),
        Message::Action(Action::ImportDefaults) => general::import(pet),
        Message::Action(action) => pet.host.settings(action).then_some(Effect::Quit),
        Message::General(message) => general::update(pet, message),
        Message::Avatars(message) => avatars::update(pet, message),
        Message::Jira(message) => jira::update(pet, message),
        Message::GitHub(message) => github::update(pet, message),
        Message::Updates(message) => updates::update(pet, message),
    }
}

/// The Settings window closed: the Jira and GitHub pages drop what was typed and not saved, as the C#'s next window
/// starts afresh. An open window brought forward again keeps it, as the C#'s does.
pub(crate) fn closed(pet: &mut PetUi) {
    jira::closed(pet);
    github::closed(pet);
}

/// The pages' background work, every frame: picks up what their threads finished.
pub(crate) fn tick(pet: &mut PetUi) {
    general::tick(pet);
    avatars::tick(pet);
    jira::tick(pet);
    github::tick(pet);
    updates::tick(pet);
}

impl PetUi {
    /// The Settings window: the sidebar, and the page with its title and hint. It paints its own opaque background,
    /// since the pet's surfaces share a transparent clear colour.
    pub fn settings_view(&self) -> Element<'_, crate::Message> {
        let accent = self.accent();
        let page = self.settings.page;
        let content = match page {
            Page::General => general::view(self),
            Page::Avatars => avatars::view(self),
            Page::Jira => jira::view(self),
            Page::GitHub => github::view(self),
        };
        let header = column![
            text(page.title()).size(19).font(style::UI_SEMIBOLD).color(Color::WHITE),
            text(page.hint()).size(12.5).font(style::UI).color(style::MUTED),
        ]
        .spacing(5)
        .padding([18, 28]);
        let body = column![
            header,
            scrollable(container(content).padding([0, 28])).height(Length::Fill)
        ];
        let window = row![sidebar(page, accent), body.width(Length::Fill)];
        container(window)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(argb(0xFF232326).into()),
                text_color: Some(style::ICON),
                ..container::Style::default()
            })
            .into()
    }
}

/// The sidebar: the accent's dot and "AiPet settings", then a button per page.
fn sidebar<'a>(page: Page, accent: Color) -> Element<'a, crate::Message> {
    let title = row![
        dot(accent, 9.0),
        text("AiPet settings")
            .size(15)
            .font(style::UI_SEMIBOLD)
            .color(Color::WHITE),
    ]
    .spacing(9)
    .align_y(Alignment::Center)
    .padding([0, 10]);
    let pages = Page::ALL.into_iter().map(|p| {
        let selected = p == page;
        button(text(p.title()).size(13.5).font(style::UI))
            .width(Length::Fill)
            .padding([9, 12])
            .on_press(crate::Message::Settings(Message::Show(p)))
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: (selected || hovered)
                        .then(|| argb(if selected { 0x2CFFFFFF } else { 0x1AFFFFFF }).into()),
                    text_color: if selected { Color::WHITE } else { style::ICON },
                    border: Border {
                        radius: 8.0.into(),
                        ..Border::default()
                    },
                    shadow: Shadow::default(),
                    snap: true,
                }
            })
            .into()
    });
    let nav = column![title, Space::new().height(18)].extend(pages).spacing(2);
    container(nav)
        .width(196)
        .height(Length::Fill)
        .padding([16, 12])
        .style(|_| container::Style {
            background: Some(argb(0xFF18181B).into()),
            ..container::Style::default()
        })
        .into()
}

/// A filled circle `size` across.
pub(crate) fn dot<'a>(colour: Color, size: f32) -> Element<'a, crate::Message> {
    container(Space::new().width(size).height(size))
        .style(move |_| container::Style {
            background: Some(colour.into()),
            border: Border {
                radius: (size / 2.0).into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// A section's small capitals heading.
pub(crate) fn section<'a>(label: &'a str) -> Element<'a, crate::Message> {
    container(text(label).size(11).font(style::UI_SEMIBOLD).color(argb(0xFF8A8A92)))
        .padding([18, 2])
        .into()
}

/// A rounded group of rows (Border.card).
pub(crate) fn card<'a>(content: impl Into<Element<'a, crate::Message>>) -> Element<'a, crate::Message> {
    container(content)
        .width(Length::Fill)
        .padding([12, 16])
        .style(|_| container::Style {
            background: Some(argb(0xFF1C1C1F).into()),
            border: Border {
                color: argb(0xFF2C2C31),
                width: 1.0,
                radius: 10.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// A title with a hint under it, filling the row's width (a row's left side).
pub(crate) fn label<'a>(title: impl IntoFragment<'a>, hint: impl IntoFragment<'a>) -> Element<'a, crate::Message> {
    column![
        text(title).size(13).font(style::UI_SEMIBOLD).color(Color::WHITE),
        text(hint).size(11.5).font(style::UI).color(style::MUTED),
    ]
    .spacing(4)
    .width(Length::Fill)
    .into()
}

/// A setting with a title, a hint and a switch.
pub(crate) fn switch_row<'a>(
    title: impl IntoFragment<'a>,
    hint: impl IntoFragment<'a>,
    on: bool,
    accent: Color,
    message: impl Fn(bool) -> crate::Message + 'a,
) -> Element<'a, crate::Message> {
    let switch = toggler(on).on_toggle(message).size(20).style(move |_, status| {
        let on = matches!(
            status,
            toggler::Status::Active { is_toggled: true } | toggler::Status::Hovered { is_toggled: true }
        );
        toggler::Style {
            background: Background::Color(if on { accent } else { Color::TRANSPARENT }),
            background_border_width: if on { 0.0 } else { 1.0 },
            background_border_color: argb(0x99FFFFFF),
            foreground: Background::Color(if on { Color::WHITE } else { argb(0xFFC8C8CC) }),
            foreground_border_width: 0.0,
            foreground_border_color: Color::TRANSPARENT,
            text_color: None,
            border_radius: None,
            padding_ratio: 0.2,
        }
    });
    row![label(title, hint), switch]
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
}

/// A setting with a title, a hint and a button on the right.
pub(crate) fn button_row<'a>(
    title: impl IntoFragment<'a>,
    hint: impl IntoFragment<'a>,
    button: impl Into<Element<'a, crate::Message>>,
) -> Element<'a, crate::Message> {
    row![label(title, hint), button.into()]
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
}

/// A round-ended button (Button.pill); `primary` fills it with the accent.
pub(crate) fn pill<'a>(label: &'a str, primary: bool, accent: Color) -> button::Button<'a, crate::Message> {
    button(text(label).size(13).font(style::UI_SEMIBOLD))
        .padding([7, 16])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let background = match (primary, hovered) {
                (true, false) => accent,
                (true, true) => accent.scale_alpha(0.88),
                (false, false) => argb(0xFF3A3A3E),
                (false, true) => argb(0xFF4A4A4F),
            };
            button::Style {
                background: Some(background.into()),
                text_color: if primary || hovered { Color::WHITE } else { style::ICON },
                border: Border {
                    color: argb(0x18FFFFFF),
                    width: 1.0,
                    radius: 16.0.into(),
                },
                shadow: Shadow::default(),
                snap: true,
            }
        })
}

/// A labelled text box. A `secret` one shows dots, and never a saved value: the pages fill it only with what is
/// typed.
#[allow(dead_code, reason = "the Jira and GitHub pages' boxes, which are still to come")]
pub(crate) fn field<'a>(
    title: &'a str,
    placeholder: &'a str,
    value: &'a str,
    secret: bool,
    on_input: impl Fn(String) -> crate::Message + 'a,
) -> Element<'a, crate::Message> {
    let input = text_input(placeholder, value)
        .on_input(on_input)
        .secure(secret)
        .size(13)
        .font(style::UI)
        .padding([7, 10])
        .style(|_, status| {
            let focused = matches!(status, text_input::Status::Focused { .. });
            text_input::Style {
                background: argb(0xFF141416).into(),
                border: Border {
                    color: argb(if focused { 0x66FFFFFF } else { 0x26FFFFFF }),
                    width: 1.0,
                    radius: 8.0.into(),
                },
                icon: style::MUTED,
                placeholder: argb(0xFF6A6A72),
                value: Color::WHITE,
                selection: argb(0x664C9AFF),
            }
        });
    column![text(title).size(12).font(style::UI_SEMIBOLD).color(style::ICON), input]
        .spacing(6)
        .into()
}

/// How a status line reads: fine, failed, or just news (SettingsWindow's Ok, Bad and Muted).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Bad,
    #[default]
    Muted,
}

/// A status line in its tone's colour.
pub(crate) fn status<'a>(line: impl IntoFragment<'a>, tone: Tone) -> Element<'a, crate::Message> {
    let colour = match tone {
        Tone::Ok => argb(0xFF8FE3A8),
        Tone::Bad => argb(0xFFFF8A8A),
        Tone::Muted => style::MUTED,
    };
    text(line).size(12).font(style::UI).color(colour).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::detached;

    #[test]
    fn the_sidebar_shows_each_page_and_the_actions_go_where_they_belong() {
        let (mut ui, host) = detached();
        assert_eq!(ui.settings.page, Page::General);
        ui.update(crate::Message::Settings(Message::Show(Page::Jira)));
        assert_eq!(ui.settings.page, Page::Jira);
        // Reset position moves the window: the shell's
        let reset = ui.update(crate::Message::Settings(Message::Action(Action::ResetPosition)));
        assert_eq!(reset, Some(Effect::ResetPosition));
        // the app's: the data folder, and updates; Restart to update quits once the app has started the update
        for action in [Action::OpenDataFolder, Action::CheckForUpdates] {
            assert_eq!(ui.update(crate::Message::Settings(Message::Action(action))), None);
        }
        host.quit_on_restart(true);
        let restart = ui.update(crate::Message::Settings(Message::Action(Action::RestartToUpdate)));
        assert_eq!(restart, Some(Effect::Quit));
        assert_eq!(
            host.settings_actions(),
            [Action::OpenDataFolder, Action::CheckForUpdates, Action::RestartToUpdate]
        );
        // Import defaults is the General page's own
        assert_eq!(
            ui.update(crate::Message::Settings(Message::Action(Action::ImportDefaults))),
            None
        );
        assert!(!host.settings_actions().contains(&Action::ImportDefaults));
    }

    #[test]
    fn detached_services_touch_nothing() {
        let services = Services::detached(PathBuf::from("data"));
        assert!(services.jira.is_none() && services.github.is_none());
        assert_eq!(services.secrets.read(aipet_core::secrets::JIRA), None);
        assert_eq!((services.update_status)().kind, Kind::Off);
    }
}
