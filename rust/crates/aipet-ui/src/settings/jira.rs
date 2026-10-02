//! Settings' Jira page (`SettingsWindow.axaml.cs:284-347`): the site, email, search and token, Test search, Save,
//! Create a token and Remove saved token, through [`super::Services`]' watcher, token store and HTTP client.
//!
//! The token box is write-only: it holds only what is typed, Save empties it, and it never shows the saved token.
//! The other fields are filled from the watcher's saved settings, and again whenever those change outside the page
//! (Import defaults…). Test searches on a thread of its own, and [`tick`] picks its outcome up. This file also has
//! the bits the GitHub page shares: the watch box, a field's hint and the status box.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use aipet_core::jira::{self, DEFAULT_JQL, Issue, JiraSettings};
use aipet_core::secrets;
use iced::widget::{Space, checkbox, column, container, row, text};
use iced::{Alignment, Border, Color, Element, Length};

use super::{Services, Tone, field, pill, status};
use crate::style::{self, argb};
use crate::{Effect, PetUi};

/// Where Create a token goes: Atlassian's API tokens page.
pub const TOKENS_PAGE: &str = "https://id.atlassian.com/manage-profile/security/api-tokens";

const HINT_SAVED: &str = "A token is saved. Leave this empty to keep it.";
const HINT_NONE: &str = "Create one at id.atlassian.com, then paste it here";

/// The Jira page's own state: its fields as typed, and its status line.
#[derive(Debug, Default)]
pub struct Jira {
    /// Watch Jira for the issues my search finds.
    pub enabled: bool,
    pub site: String,
    pub email: String,
    /// The search (JQL).
    pub jql: String,
    /// The API token as typed since the last Save: never the saved one.
    pub token: String,
    /// Whether a token is saved, as of the page's last fill, Save or Forget: the token box's hint and Remove saved
    /// token follow it.
    pub has_token: bool,
    /// The status line the page's buttons left; none shows the watcher's last error, if it has one.
    pub status: Option<(String, Tone)>,
    /// The saved settings the fields were last filled from.
    loaded: Option<JiraSettings>,
    /// The Test running, which sends its status line.
    testing: Option<Receiver<(String, Tone)>>,
}

/// What the Jira page sends of its own.
#[derive(Clone, Debug)]
pub enum Message {
    Enabled(bool),
    Site(String),
    Email(String),
    Jql(String),
    Token(String),
    /// Save the settings and the typed token, and start the watcher again.
    Save,
    /// Run the search with the typed token, else the saved one.
    Test,
    /// Open Atlassian's API tokens page.
    CreateToken,
    /// Remove the saved token.
    Forget,
}

impl Jira {
    /// The settings the fields make (`JiraSettings()`): trimmed, with the default search for an empty one.
    fn settings(&self, poll_seconds: i32) -> JiraSettings {
        let jql = self.jql.trim();
        JiraSettings {
            enabled: self.enabled,
            site: Some(self.site.trim().to_owned()),
            email: Some(self.email.trim().to_owned()),
            jql: Some(if jql.is_empty() { DEFAULT_JQL } else { jql }.to_owned()),
            poll_seconds,
        }
    }

    /// Fills the fields from the saved settings (`FillJira`); the token box is left alone.
    fn fill(&mut self, saved: JiraSettings, has_token: bool) {
        self.enabled = saved.enabled || !has_token;
        self.site = saved.site.clone().unwrap_or_default();
        self.email = saved.email.clone().unwrap_or_default();
        self.jql = saved.jql.clone().unwrap_or_default();
        self.has_token = has_token;
        self.loaded = Some(saved);
    }
}

/// The saved settings: the watcher's, or the defaults where nothing watches.
fn saved(services: &Services) -> JiraSettings {
    services.jira.as_ref().map(|jira| jira.config()).unwrap_or_default()
}

/// Whether an API token is saved.
fn has_token(services: &Services) -> bool {
    match &services.jira {
        Some(jira) => jira.has_token(),
        None => services
            .secrets
            .read(secrets::JIRA)
            .is_some_and(|token| !token.is_empty()),
    }
}

/// A Test's status line for what the search found.
fn outcome(found: Result<Vec<Issue>, String>) -> (String, Tone) {
    match found {
        Err(error) => (error, Tone::Bad),
        Ok(issues) => match issues.first() {
            None => ("Connected. The search finds no issues right now.".to_owned(), Tone::Ok),
            Some(first) => (
                format!(
                    "Connected. The search finds {} issue(s), e.g. {}: {}",
                    issues.len(),
                    first.key,
                    first.summary
                ),
                Tone::Ok,
            ),
        },
    }
}

fn send(message: Message) -> crate::Message {
    crate::Message::Settings(super::Message::Jira(message))
}

pub(super) fn view(pet: &PetUi) -> Element<'_, crate::Message> {
    let page = &pet.settings.jira;
    let watcher = pet.settings.services.jira.as_ref();
    let accent = pet.accent();
    let site = column![
        field("Site", "", &page.site, false, |s| send(Message::Site(s))),
        hint("Your Jira Cloud address, e.g. your-company.atlassian.net"),
    ]
    .spacing(6)
    .width(Length::Fill);
    let email = column![
        field("Email", "", &page.email, false, |s| send(Message::Email(s))),
        hint("The email you sign in to Atlassian with"),
    ]
    .spacing(6)
    .width(Length::Fill);
    let token = column![
        field("API token", "", &page.token, true, |s| send(Message::Token(s))),
        hint(if page.has_token { HINT_SAVED } else { HINT_NONE }),
    ]
    .spacing(6);
    let jql = column![
        field("Search (JQL)", "", &page.jql, false, |s| send(Message::Jql(s))),
        hint(
            "The default finds your own open issues (assigned to you and not done); empty the field to go back to it. \
             For reviews, use something like Reviewer = currentUser() AND status = Review. Field and status names \
             differ between sites."
        ),
    ]
    .spacing(6);
    // Save and Remove saved token need the watcher, which the demo doesn't have
    let mut buttons = row![
        pill("Save", true, accent).on_press_maybe(watcher.map(|_| send(Message::Save))),
        pill("Test search", false, accent).on_press(send(Message::Test)),
        pill("Create a token", false, accent).on_press(send(Message::CreateToken)),
    ]
    .spacing(8);
    if page.has_token {
        buttons = buttons
            .push(pill("Remove saved token", false, accent).on_press_maybe(watcher.map(|_| send(Message::Forget))));
    }
    let mut content = column![
        watch_box(
            "Watch Jira for the issues my search finds",
            page.enabled,
            accent,
            |on| { send(Message::Enabled(on)) }
        ),
        row![site, Space::new().width(16), email].align_y(Alignment::Start),
        token,
        jql,
        buttons.wrap().vertical_spacing(8),
    ]
    .spacing(16)
    .padding(iced::Padding::ZERO.top(8).bottom(24));
    let line = page.status.clone().or_else(|| {
        watcher
            .and_then(|jira| jira.last_error())
            .map(|error| (error, Tone::Bad))
    });
    if let Some((line, tone)) = line {
        content = content.push(status_box(line, tone));
    }
    content.into()
}

/// A page's Watch … check box, ticked in the accent.
pub(super) fn watch_box<'a>(
    label: &'a str,
    on: bool,
    accent: Color,
    message: impl Fn(bool) -> crate::Message + 'a,
) -> Element<'a, crate::Message> {
    checkbox(on)
        .label(label)
        .on_toggle(message)
        .size(16)
        .spacing(9)
        .text_size(13)
        .font(style::UI)
        .style(move |_, status| {
            let (on, hovered) = match status {
                checkbox::Status::Active { is_checked } => (is_checked, false),
                checkbox::Status::Hovered { is_checked } => (is_checked, true),
                checkbox::Status::Disabled { is_checked } => (is_checked, false),
            };
            checkbox::Style {
                background: if on { accent } else { argb(0xFF141416) }.into(),
                icon_color: Color::WHITE,
                border: Border {
                    color: if on {
                        accent
                    } else {
                        argb(if hovered { 0x99FFFFFF } else { 0x66FFFFFF })
                    },
                    width: 1.0,
                    radius: 4.0.into(),
                },
                text_color: Some(Color::WHITE),
            }
        })
        .into()
}

/// A field's hint, under it.
pub(super) fn hint<'a>(line: &'a str) -> Element<'a, crate::Message> {
    text(line).size(11.5).font(style::UI).color(style::MUTED).into()
}

/// The status line in its box.
pub(super) fn status_box<'a>(line: String, tone: Tone) -> Element<'a, crate::Message> {
    container(status(line, tone))
        .width(Length::Fill)
        .padding([8, 10])
        .style(|_| container::Style {
            background: Some(argb(0xFF1A1A1D).into()),
            border: Border {
                radius: 8.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

pub(super) fn update(pet: &mut PetUi, message: Message) -> Option<Effect> {
    let page = &mut pet.settings.jira;
    match message {
        Message::Enabled(on) => page.enabled = on,
        Message::Site(site) => page.site = site,
        Message::Email(email) => page.email = email,
        Message::Jql(jql) => page.jql = jql,
        Message::Token(token) => page.token = token,
        Message::CreateToken => pet.platform.open_url(TOKENS_PAGE),
        Message::Save => save(pet),
        Message::Test => test(pet),
        Message::Forget => forget(pet),
    }
    None
}

fn save(pet: &mut PetUi) {
    let Some(jira) = &pet.settings.services.jira else {
        return;
    };
    let page = &mut pet.settings.jira;
    // a Test still running speaks for what was there before: its outcome would hide Save's
    page.testing = None;
    let settings = page.settings(jira.config().poll_seconds);
    // the page's own Save doesn't fill the fields again
    page.loaded = Some(settings.clone());
    match jira.save(settings, &page.token) {
        Ok(()) => {
            page.token.clear();
            page.has_token = jira.has_token();
            page.status = Some((
                "Saved. The pet checks Jira every couple of minutes.".to_owned(),
                Tone::Ok,
            ));
        }
        Err(error) => page.status = Some((format!("Couldn't save the settings: {error}"), Tone::Bad)),
    }
}

fn test(pet: &mut PetUi) {
    let services = &pet.settings.services;
    let page = &mut pet.settings.jira;
    let token = match page.token.as_str() {
        "" => services.secrets.read(secrets::JIRA).unwrap_or_default(),
        typed => typed.to_owned(),
    };
    page.testing = None;
    if token.is_empty() {
        page.status = Some(("Paste an API token first.".to_owned(), Tone::Bad));
        return;
    }
    let settings = page.settings(saved(services).poll_seconds);
    let http = services.http.clone();
    let (sender, receiver) = mpsc::channel();
    let spawned = thread::Builder::new().name("aipet-jira-test".into()).spawn(move || {
        // a page that stopped waiting for this Test misses nothing it wants
        let _ = sender.send(outcome(jira::search(&http, &settings, &token)));
    });
    match spawned {
        Ok(_) => {
            page.status = Some(("Searching…".to_owned(), Tone::Muted));
            page.testing = Some(receiver);
        }
        Err(error) => page.status = Some((format!("Jira search failed: {error}"), Tone::Bad)),
    }
}

fn forget(pet: &mut PetUi) {
    let Some(jira) = &pet.settings.services.jira else {
        return;
    };
    jira.forget_token();
    let page = &mut pet.settings.jira;
    // as for Save: a Test still running would hide that the token went
    page.testing = None;
    page.has_token = false;
    page.status = Some((
        "The saved token was removed. Jira stays off until you save a new one.".to_owned(),
        Tone::Muted,
    ));
}

/// Fills the fields on the first frame and whenever the saved settings changed outside the page, and picks a
/// finished Test's status line up.
pub(super) fn tick(pet: &mut PetUi) {
    let services = &pet.settings.services;
    let page = &mut pet.settings.jira;
    let saved = saved(services);
    if page.loaded.as_ref() != Some(&saved) {
        page.fill(saved, has_token(services));
    }
    if let Some(testing) = &page.testing {
        match testing.try_recv() {
            Ok(line) => {
                page.status = Some(line);
                page.testing = None;
            }
            Err(TryRecvError::Disconnected) => page.testing = None,
            Err(TryRecvError::Empty) => {}
        }
    }
}
