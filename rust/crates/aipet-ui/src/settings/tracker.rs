//! Settings' Tracker page (fnt-81): the flow-next tracker's address and a one-time companion code, Enroll and
//! Forget, whether chats that aren't tracker tasks hide while it's connected, and where the watcher stands, through
//! [`super::Services`]' watcher.
//!
//! A tracker on this computer gives the pet a token; a hosted one a certificate for a key made here, its CA pinned by
//! the code (`<secret>.<fingerprint>`). The code box is write-only and single-use: Enroll empties it. Enroll runs on a thread of its own (the tracker may
//! take its time), and [`tick`] picks its outcome up. Revoking the pet is done in the tracker's client; Forget only
//! deletes the pet's own credential.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use aipet_core::tracker::{SessionsWithoutTask, Status, TrackerSettings, TrackerWatcher};
use iced::Element;
use iced::widget::{column, row};

use super::jira::{hint, status_box, watch_box};
use super::{Tone, field, pill};
use crate::{Effect, PetUi};

/// The address a tracker on this computer listens on by default.
pub const DEFAULT_ADDRESS: &str = "http://127.0.0.1:3210";
const PRIVACY: &str = "The pet reads the tracker's active tasks. Nothing your agents' hooks report is sent to it, \
                       except a chat's id, to ask whether that chat is one of the tracker's runs.";

/// The Tracker page's own state: its fields as typed, and its status line.
#[derive(Debug, Default)]
pub struct Tracker {
    pub address: String,
    /// The one-time code as typed since the last Enroll.
    pub code: String,
    pub hide_other_chats: bool,
    /// The status line the page's buttons left; none shows where the watcher stands.
    pub status: Option<(String, Tone)>,
    /// The saved settings the fields were last filled from.
    loaded: Option<TrackerSettings>,
    /// The Enroll running, which sends its outcome.
    enrolling: Option<Receiver<Result<(), String>>>,
}

/// What the Tracker page sends of its own.
#[derive(Clone, Debug)]
pub enum Message {
    Address(String),
    Code(String),
    /// Hide chats that aren't tracker tasks while the tracker is connected: saved at once.
    HideOtherChats(bool),
    Enroll,
    Forget,
}

impl Tracker {
    /// The status line now: the one the page's buttons left, else where the watcher stands.
    pub fn status(&self, watcher: Option<&TrackerWatcher>) -> (String, Tone) {
        if let Some(status) = &self.status {
            return status.clone();
        }
        let Some(watcher) = watcher else {
            return ("The tracker isn't followed here.".to_owned(), Tone::Muted);
        };
        match watcher.status() {
            Status::Off => (
                "Not connected. Make a companion code on the tracker's Devices & runners page and enter it above."
                    .to_owned(),
                Tone::Muted,
            ),
            Status::Connecting => ("Connecting…".to_owned(), Tone::Muted),
            Status::Connected { actions_allowed } => {
                let tasks = watcher.tasks().len();
                let actions = if actions_allowed {
                    "Answers and pause/stop are on for this pet."
                } else {
                    "Answers and pause/stop are off; switch them on for this pet in the tracker."
                };
                (format!("Connected. {tasks} active task(s). {actions}"), Tone::Ok)
            }
            Status::Unreachable(why) => (format!("{why}. The pet keeps trying."), Tone::Bad),
            Status::Revoked => (
                "The tracker revoked this pet; enroll it again with a new code.".to_owned(),
                Tone::Bad,
            ),
        }
    }

    fn fill(&mut self, saved: TrackerSettings) {
        self.address = saved.base_url.clone().unwrap_or_else(|| DEFAULT_ADDRESS.to_owned());
        self.hide_other_chats = saved.sessions_without_task == SessionsWithoutTask::Hide;
        self.status = None;
        self.loaded = Some(saved);
    }
}

fn saved(watcher: Option<&Arc<TrackerWatcher>>) -> TrackerSettings {
    watcher.map(|tracker| tracker.settings()).unwrap_or_default()
}

fn send(message: Message) -> crate::Message {
    crate::Message::Settings(super::Message::Tracker(message))
}

pub(super) fn view(pet: &PetUi) -> Element<'_, crate::Message> {
    let page = &pet.settings.tracker;
    let watcher = pet.settings.services.tracker.as_ref();
    let accent = pet.accent();
    let enrolled = watcher.is_some_and(|tracker| tracker.is_enrolled());
    let address = column![
        field("Tracker address", "", &page.address, false, |s| send(Message::Address(
            s
        ))),
        hint("http://127.0.0.1:3210 for a tracker on this computer, or a hosted tracker's https:// address"),
    ]
    .spacing(6);
    let code = column![
        field("Companion code", "", &page.code, true, |s| send(Message::Code(s))),
        hint("Made on the tracker's Devices & runners page (a companion code); it works once, for 15 minutes"),
    ]
    .spacing(6);
    let busy = page.enrolling.is_some();
    let mut buttons = row![
        pill(if enrolled { "Enroll again" } else { "Enroll" }, true, accent)
            .on_press_maybe(watcher.filter(|_| !busy).map(|_| send(Message::Enroll))),
    ]
    .spacing(8);
    if enrolled {
        buttons = buttons
            .push(pill("Forget this tracker", false, accent).on_press_maybe(watcher.map(|_| send(Message::Forget))));
    }
    let (line, tone) = page.status(watcher.map(Arc::as_ref));
    column![
        address,
        code,
        buttons.wrap().vertical_spacing(8),
        watch_box(
            "Hide chats that aren't tracker tasks while the tracker is connected",
            page.hide_other_chats,
            accent,
            |on| send(Message::HideOtherChats(on))
        ),
        status_box(line, tone),
        hint(PRIVACY),
    ]
    .spacing(16)
    .padding(iced::Padding::ZERO.top(8).bottom(24))
    .into()
}

pub(super) fn update(pet: &mut PetUi, message: Message) -> Option<Effect> {
    let page = &mut pet.settings.tracker;
    match message {
        Message::Address(address) => page.address = address,
        Message::Code(code) => page.code = code,
        Message::HideOtherChats(on) => {
            page.hide_other_chats = on;
            save_choice(pet);
        }
        Message::Enroll => enroll(pet),
        Message::Forget => forget(pet),
    }
    None
}

fn save_choice(pet: &mut PetUi) {
    let Some(tracker) = &pet.settings.services.tracker else {
        return;
    };
    let page = &mut pet.settings.tracker;
    let settings = TrackerSettings {
        sessions_without_task: if page.hide_other_chats {
            SessionsWithoutTask::Hide
        } else {
            SessionsWithoutTask::Show
        },
        ..tracker.settings()
    };
    page.loaded = Some(settings.clone());
    if let Err(error) = tracker.save(settings) {
        page.status = Some((format!("Couldn't save the setting: {error}"), Tone::Bad));
    }
}

fn enroll(pet: &mut PetUi) {
    let Some(tracker) = &pet.settings.services.tracker else {
        return;
    };
    let page = &mut pet.settings.tracker;
    let (watcher, address, code) = (
        Arc::clone(tracker),
        page.address.clone(),
        std::mem::take(&mut page.code),
    );
    let (sender, receiver) = mpsc::channel();
    let spawned = thread::Builder::new()
        .name("aipet-tracker-enroll".into())
        .spawn(move || {
            // a page that stopped waiting misses nothing it wants
            let _ = sender.send(watcher.enroll(&address, &code));
        });
    match spawned {
        Ok(_) => {
            page.status = Some(("Enrolling…".to_owned(), Tone::Muted));
            page.enrolling = Some(receiver);
        }
        Err(error) => page.status = Some((format!("Couldn't enroll: {error}"), Tone::Bad)),
    }
}

fn forget(pet: &mut PetUi) {
    let Some(tracker) = &pet.settings.services.tracker else {
        return;
    };
    tracker.forget();
    let page = &mut pet.settings.tracker;
    page.enrolling = None;
    page.status = Some((
        "Forgotten. The pet is still listed in the tracker until you revoke it there.".to_owned(),
        Tone::Muted,
    ));
}

/// Fills the fields on the first frame and whenever the saved settings changed outside the page, and picks a
/// finished Enroll's outcome up.
pub(super) fn tick(pet: &mut PetUi) {
    let saved = saved(pet.settings.services.tracker.as_ref());
    let page = &mut pet.settings.tracker;
    if page.loaded.as_ref() != Some(&saved) && page.enrolling.is_none() {
        page.fill(saved);
    }
    if let Some(enrolling) = &page.enrolling {
        match enrolling.try_recv() {
            Ok(outcome) => {
                page.enrolling = None;
                page.status = match outcome {
                    // where the watcher stands shows from now on
                    Ok(()) => None,
                    Err(why) => Some((why, Tone::Bad)),
                };
            }
            Err(TryRecvError::Disconnected) => page.enrolling = None,
            Err(TryRecvError::Empty) => {}
        }
    }
}

/// The Settings window closed: a typed code goes, with the status line.
pub(super) fn closed(pet: &mut PetUi) {
    let page = &mut pet.settings.tracker;
    page.code.clear();
    page.status = None;
    page.loaded = None;
}
