//! Settings' GitHub page (`SettingsWindow.axaml.cs:350-412`): the host, organisations and token, Save, Test
//! connection, Create a token and Remove saved token, through [`super::Services`]' watcher.
//!
//! The token box is write-only, as Jira's: it holds only what is typed, Save empties it, and it never shows the saved
//! token. The other fields are filled from the watcher's saved settings, and again whenever those change outside the
//! page (Import defaults…). Test runs the watcher's test on a thread of its own, and [`tick`] picks its outcome up.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use aipet_core::github::{GitHubSettings, GitHubWatcher};
use aipet_core::secrets;
use iced::Element;
use iced::widget::{column, row};

use super::jira::{hint, status_box, watch_box};
use super::{Services, Tone, field, pill};
use crate::{Effect, PetUi};

const HINT_SAVED: &str = "A token is saved. Leave this empty to keep it.";
const HINT_NONE: &str = "A fine-grained token with read access to pull requests and metadata.";

/// The GitHub page's own state: its fields as typed, and its status line.
#[derive(Debug, Default)]
pub struct GitHub {
    /// Watch GitHub for pull requests waiting on my review.
    pub enabled: bool,
    pub host: String,
    /// The organisations, comma-separated.
    pub orgs: String,
    /// The access token as typed since the last Save: never the saved one.
    pub token: String,
    /// Whether a token is saved, as of the page's last fill, Save or Forget: the token box's hint, Remove saved
    /// token and the status line follow it.
    pub has_token: bool,
    /// The status line the page's buttons left; none shows where the watcher stands ([`GitHub::status`]).
    pub status: Option<(String, Tone)>,
    /// The saved settings the fields were last filled from.
    loaded: Option<GitHubSettings>,
    /// The Test running, which sends its status line.
    testing: Option<Receiver<(String, Tone)>>,
}

/// What the GitHub page sends of its own.
#[derive(Clone, Debug)]
pub enum Message {
    Enabled(bool),
    Host(String),
    Orgs(String),
    /// A token typed: one that isn't blank ticks the watch box.
    Token(String),
    /// Save the settings and the typed token, and start the watcher again.
    Save,
    /// Check the host and the typed token, else the saved one.
    Test,
    /// Open the host's page for a new fine-grained token.
    CreateToken,
    /// Remove the saved token.
    Forget,
}

impl GitHub {
    /// The status line now (`UpdateGitHubStatus`): the one the page's buttons left, else where the watcher stands.
    pub fn status(&self, watcher: Option<&GitHubWatcher>) -> (String, Tone) {
        if let Some(status) = &self.status {
            return status.clone();
        }
        let Some(watcher) = watcher.filter(|watcher| watcher.config().enabled) else {
            return ("GitHub reviews are off.".to_owned(), Tone::Muted);
        };
        if let Some(error) = watcher.last_error() {
            return (error, Tone::Bad);
        }
        let line = if self.has_token {
            format!(
                "Connected with your token. {} pull request(s) waiting on your review.",
                watcher.review_requests().len()
            )
        } else {
            "No token yet, so GitHub reviews are off. Paste a token above.".to_owned()
        };
        (line, Tone::Muted)
    }

    /// The host typed, or `github.com` for none.
    fn host(&self) -> &str {
        match self.host.trim() {
            "" => "github.com",
            host => host,
        }
    }

    /// The settings the fields make, with what the page doesn't show kept from `saved`.
    fn settings(&self, saved: GitHubSettings) -> GitHubSettings {
        let orgs = self.orgs.split(',').map(str::trim).filter(|org| !org.is_empty());
        GitHubSettings {
            enabled: self.enabled,
            host: Some(self.host().to_owned()),
            orgs: Some(orgs.map(|org| Some(org.to_owned())).collect()),
            ..saved
        }
    }

    /// Fills the fields from the saved settings (`FillGitHub`); the token box is left alone, and the status line
    /// shows where the watcher stands again.
    fn fill(&mut self, saved: GitHubSettings, has_token: bool) {
        self.enabled = saved.enabled;
        self.host = saved.host.clone().unwrap_or_default();
        let orgs = saved.orgs.iter().flatten().map(|org| org.as_deref().unwrap_or(""));
        self.orgs = orgs.collect::<Vec<_>>().join(", ");
        self.has_token = has_token;
        self.status = None;
        self.loaded = Some(saved);
    }
}

/// The saved settings: the watcher's, or the defaults where nothing watches.
fn saved(services: &Services) -> GitHubSettings {
    services
        .github
        .as_ref()
        .map(|github| github.config())
        .unwrap_or_default()
}

fn send(message: Message) -> crate::Message {
    crate::Message::Settings(super::Message::GitHub(message))
}

pub(super) fn view(pet: &PetUi) -> Element<'_, crate::Message> {
    let page = &pet.settings.github;
    let watcher = pet.settings.services.github.as_ref();
    let accent = pet.accent();
    let host = column![
        field("GitHub host", "", &page.host, false, |s| send(Message::Host(s))),
        hint("github.com, or your GitHub Enterprise Server host"),
    ]
    .spacing(6);
    let token = column![
        field("Access token", "", &page.token, true, |s| send(Message::Token(s))),
        hint(if page.has_token { HINT_SAVED } else { HINT_NONE }),
    ]
    .spacing(6);
    let orgs = column![
        field("Organisations", "", &page.orgs, false, |s| send(Message::Orgs(s))),
        hint("Where to look for the pull request of a Jira task, comma-separated"),
    ]
    .spacing(6);
    // everything but Create a token needs the watcher, which the demo doesn't have
    let mut buttons = row![
        pill("Save", true, accent).on_press_maybe(watcher.map(|_| send(Message::Save))),
        pill("Test connection", false, accent).on_press_maybe(watcher.map(|_| send(Message::Test))),
        pill("Create a token", false, accent).on_press(send(Message::CreateToken)),
    ]
    .spacing(8);
    if page.has_token {
        buttons = buttons
            .push(pill("Remove saved token", false, accent).on_press_maybe(watcher.map(|_| send(Message::Forget))));
    }
    let (line, tone) = page.status(watcher.map(Arc::as_ref));
    column![
        watch_box(
            "Watch GitHub for pull requests waiting on my review",
            page.enabled,
            accent,
            |on| send(Message::Enabled(on))
        ),
        host,
        token,
        orgs,
        buttons.wrap().vertical_spacing(8),
        status_box(line, tone),
    ]
    .spacing(16)
    .padding(iced::Padding::ZERO.top(8).bottom(24))
    .into()
}

pub(super) fn update(pet: &mut PetUi, message: Message) -> Option<Effect> {
    let page = &mut pet.settings.github;
    match message {
        Message::Enabled(on) => page.enabled = on,
        Message::Host(host) => page.host = host,
        Message::Orgs(orgs) => page.orgs = orgs,
        Message::Token(token) => {
            if !token.trim().is_empty() {
                page.enabled = true;
            }
            page.token = token;
        }
        Message::CreateToken => {
            let url = format!("https://{}/settings/personal-access-tokens/new", page.host());
            pet.platform.open_url(&url);
        }
        Message::Save => save(pet),
        Message::Test => test(pet),
        Message::Forget => forget(pet),
    }
    None
}

fn save(pet: &mut PetUi) {
    let Some(github) = &pet.settings.services.github else {
        return;
    };
    let page = &mut pet.settings.github;
    // a Test still running speaks for what was there before: its outcome would hide Save's
    page.testing = None;
    let settings = page.settings(github.config());
    // the page's own Save doesn't fill the fields again
    page.loaded = Some(settings.clone());
    match github.save(settings, &page.token) {
        Ok(()) => {
            page.token.clear();
            page.has_token = github.has_saved_token();
            page.status = Some((
                "Saved. The pet checks GitHub every couple of minutes.".to_owned(),
                Tone::Muted,
            ));
        }
        Err(error) => page.status = Some((format!("Couldn't save the settings: {error}"), Tone::Bad)),
    }
}

fn test(pet: &mut PetUi) {
    let Some(github) = &pet.settings.services.github else {
        return;
    };
    let page = &mut pet.settings.github;
    let (watcher, host, token) = (Arc::clone(github), page.host.clone(), page.token.clone());
    let (sender, receiver) = mpsc::channel();
    let spawned = thread::Builder::new().name("aipet-github-test".into()).spawn(move || {
        let line = match watcher.test(&host, &token) {
            Ok(message) => (message, Tone::Ok),
            Err(message) => (message, Tone::Bad),
        };
        // a page that stopped waiting for this Test misses nothing it wants
        let _ = sender.send(line);
    });
    match spawned {
        Ok(_) => {
            page.status = Some(("Connecting…".to_owned(), Tone::Muted));
            page.testing = Some(receiver);
        }
        Err(error) => {
            page.status = Some((format!("Couldn't reach GitHub: {error}"), Tone::Bad));
            page.testing = None;
        }
    }
}

fn forget(pet: &mut PetUi) {
    let Some(github) = &pet.settings.services.github else {
        return;
    };
    github.forget_token();
    let page = &mut pet.settings.github;
    // as for Save: a Test still running would hide that the token went
    page.testing = None;
    page.has_token = false;
    page.status = Some((
        "The saved token was removed. GitHub reviews are off until you save a new one.".to_owned(),
        Tone::Muted,
    ));
}

/// Fills the fields on the first frame and whenever the saved settings changed outside the page, and picks a
/// finished Test's status line up.
pub(super) fn tick(pet: &mut PetUi) {
    let services = &pet.settings.services;
    let page = &mut pet.settings.github;
    let saved = saved(services);
    if page.loaded.as_ref() != Some(&saved) {
        let has_token = match &services.github {
            Some(github) => github.has_saved_token(),
            None => services
                .secrets
                .read(secrets::GITHUB)
                .is_some_and(|token| !token.is_empty()),
        };
        page.fill(saved, has_token);
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
