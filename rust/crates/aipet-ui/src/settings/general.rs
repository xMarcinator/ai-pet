//! Settings' General page (`SettingsWindow.axaml.cs:96-125`): the pet's switches, its position, the data folder, Team
//! defaults (Import defaults…, `:154-204`), and the updates section ([`super::updates`]).
//!
//! Import defaults… asks for a preset file in the desktop's file picker, on a thread of its own so the pet keeps
//! moving while it is open; [`tick`] picks the chosen file up and [`import_file`] reads it into the Jira and GitHub
//! settings.

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::{fmt, thread};

use aipet_core::presets::Preset;
use iced::Element;
use iced::widget::{column, row};

use super::{Action, Tone, button_row, card, label, pill, section, status, switch_row};
use crate::{Effect, PetUi};

/// What asks the user for a preset file: `None` when they chose none.
pub(crate) type Pick = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;

/// The General page's own state.
pub struct General {
    /// The line under Team defaults that the last import left (`ImportStatus`), in its tone; none before one.
    pub import_status: Option<(String, Tone)>,
    /// The file picker; tests put one of their own in.
    pick: Pick,
    /// The picker while it is open: what it answers, or why it couldn't open.
    picking: Option<Receiver<Result<Option<PathBuf>, String>>>,
}

impl Default for General {
    fn default() -> General {
        General {
            import_status: None,
            pick: Arc::new(pick_preset),
            picking: None,
        }
    }
}

impl fmt::Debug for General {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("General")
            .field("import_status", &self.import_status)
            .field("picking", &self.picking.is_some())
            .finish_non_exhaustive()
    }
}

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
    let action = |label, action| pill(label, false, accent).on_press(send(action));
    let general = &pet.settings.general;
    let mut team = column![label(
        "Team defaults",
        "Set up Jira and GitHub from a preset file your team shares. A preset holds no tokens and no email; you add \
         your own.",
    )]
    .spacing(4);
    if let Some((line, tone)) = &general.import_status {
        team = team.push(status(line.as_str(), *tone));
    }
    // one picker at a time
    let import = pill("Import defaults…", false, accent)
        .on_press_maybe(general.picking.is_none().then(|| send(Action::ImportDefaults)));
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
        row![team, import].spacing(12).align_y(iced::Alignment::Center),
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

fn send(action: Action) -> crate::Message {
    crate::Message::Settings(super::Message::Action(action))
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

/// Import defaults…: opens the file picker on a thread of its own; [`tick`] picks its answer up. While it is open the
/// button waits.
pub(super) fn import(pet: &mut PetUi) -> Option<Effect> {
    let page = &mut pet.settings.general;
    if page.picking.is_some() {
        return None;
    }
    let (answer, picking) = mpsc::channel();
    let pick = Arc::clone(&page.pick);
    let opened = thread::Builder::new().name("aipet-import".into()).spawn(move || {
        // the picker is the desktop's (a portal over D-Bus on Linux), and an error from it mustn't take the pet down
        let picked = panic::catch_unwind(AssertUnwindSafe(|| pick())).map_err(|e| panic_text(&*e));
        let _ = answer.send(picked);
    });
    match opened {
        Ok(_) => page.picking = Some(picking),
        Err(e) => page.import_status = Some((format!("Couldn't open a file picker: {e}"), Tone::Bad)),
    }
    None
}

/// Every frame: once the picker has answered, imports the file chosen, if any.
pub(super) fn tick(pet: &mut PetUi) {
    let Some(picking) = &pet.settings.general.picking else {
        return;
    };
    let answer = match picking.try_recv() {
        Ok(answer) => answer,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err("it closed without an answer".to_owned()),
    };
    pet.settings.general.picking = None;
    match answer {
        Err(e) => pet.settings.general.import_status = Some((format!("Couldn't open a file picker: {e}"), Tone::Bad)),
        // none chosen: the line stays as it was
        Ok(None) => {}
        Ok(Some(file)) => import_file(pet, &file),
    }
}

/// The desktop's file picker, for a preset.
fn pick_preset() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import defaults")
        .add_filter("AiPet preset", &["json"])
        .pick_file()
}

fn panic_text(payload: &(dyn Any + Send)) -> String {
    match (payload.downcast_ref::<&str>(), payload.downcast_ref::<String>()) {
        (Some(text), _) => (*text).to_owned(),
        (_, Some(text)) => text.clone(),
        _ => "it failed".to_owned(),
    }
}

/// Reads a preset file into the Jira and GitHub settings, saved as their pages' Save does (`ImportAsync` once a file
/// is chosen), and says how it went under Team defaults. Tokens and the email are never part of a preset; but a saved
/// token goes wherever the settings point, so when the preset points Jira or GitHub at another address, that token is
/// removed before the watcher starts again, and the user pastes it again if it belongs there.
#[doc(hidden)]
pub fn import_file(pet: &mut PetUi, file: &Path) {
    let outcome = import_preset(pet, file);
    pet.settings.general.import_status = Some(outcome);
}

fn import_preset(pet: &PetUi, file: &Path) -> (String, Tone) {
    // as a StreamReader reads it: a byte order mark picks UTF-8, UTF-16 or UTF-32, and bytes that don't decode become
    // U+FFFD
    let text = match aipet_sprite::read_all_text(file) {
        Ok(text) => text,
        Err(e) => return (format!("Couldn't read the file: {e}"), Tone::Bad),
    };
    let preset = match Preset::parse(&text) {
        Ok(preset) => preset,
        Err(e) => return (format!("Nothing was imported. {e}"), Tone::Bad),
    };
    if !preset.has_jira() && !preset.has_github() {
        return (
            "Nothing was imported: the file has no Jira or GitHub settings.".to_owned(),
            Tone::Muted,
        );
    }
    let services = &pet.settings.services;
    let mut moved = Vec::new();
    if preset.has_jira() {
        // only where nothing watches: the demo
        let Some(watcher) = &services.jira else {
            return (
                "Couldn't save the settings: nothing watches Jira here.".to_owned(),
                Tone::Bad,
            );
        };
        let saved = watcher.config();
        let jira = preset.apply_to_jira(&saved);
        if watcher.has_token() && preset.moves_jira(&saved) {
            watcher.forget_token();
            moved.push(format!("Jira at {}", jira.site.as_deref().unwrap_or("")));
        }
        if let Err(e) = watcher.save(jira, "") {
            return (format!("Couldn't save the settings: {e}"), Tone::Bad);
        }
    }
    if preset.has_github() {
        let Some(watcher) = &services.github else {
            return (
                "Couldn't save the settings: nothing watches GitHub here.".to_owned(),
                Tone::Bad,
            );
        };
        let saved = watcher.config();
        let github = preset.apply_to_github(&saved);
        if watcher.has_saved_token() && preset.moves_github(&saved) {
            watcher.forget_token();
            moved.push(format!("GitHub at {}", github.host.as_deref().unwrap_or("")));
        }
        if let Err(e) = watcher.save(github, "") {
            return (format!("Couldn't save the settings: {e}"), Tone::Bad);
        }
    }
    if moved.is_empty() {
        return (
            format!(
                "Imported {}. Your tokens and email stay as they were.",
                preset.describe()
            ),
            Tone::Ok,
        );
    }
    let removed = if moved.len() == 1 {
        "the token you saved for the old address was removed: paste it"
    } else {
        "the tokens you saved for the old addresses were removed: paste them"
    };
    (
        format!(
            "Imported {}. It points {}, so {removed} again if you trust the new one. Your email stays as it was.",
            preset.describe(),
            moved.join(" and ")
        ),
        Tone::Muted,
    )
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::tests::detached;

    /// Presses Import defaults… with `pick` for the picker, and ticks until the picker has answered.
    fn import_with(pick: impl Fn() -> Option<PathBuf> + Send + Sync + 'static) -> PetUi {
        let (mut ui, _host) = detached();
        ui.settings.general.pick = Arc::new(pick);
        let pressed = crate::Message::Settings(super::super::Message::Action(Action::ImportDefaults));
        assert_eq!(ui.update(pressed.clone()), None);
        let first: *const _ = ui.settings.general.picking.as_ref().expect("the picker is open");
        // a second press while it is open opens no second picker
        ui.update(pressed);
        assert!(
            ui.settings
                .general
                .picking
                .as_ref()
                .is_some_and(|open| std::ptr::eq(open, first))
        );
        let until = Instant::now() + Duration::from_secs(10);
        while ui.settings.general.picking.is_some() {
            assert!(Instant::now() < until, "the picker never answered");
            thread::sleep(Duration::from_millis(5));
            super::super::tick(&mut ui);
        }
        ui
    }

    #[test]
    fn the_picker_runs_on_a_thread_and_tick_imports_what_it_chose() {
        // no file chosen: nothing is said
        let ui = import_with(|| None);
        assert_eq!(ui.settings.general.import_status, None);
        // a file: it is read (this one doesn't exist)
        let missing = std::env::temp_dir().join(format!("aipet-no-preset-{}.json", std::process::id()));
        let ui = import_with(move || Some(missing.clone()));
        let (line, tone) = ui.settings.general.import_status.clone().unwrap();
        assert!(line.starts_with("Couldn't read the file: "), "{line}");
        assert_eq!(tone, Tone::Bad);
        // a picker that fails takes nothing down
        let ui = import_with(|| panic!("no portal"));
        assert_eq!(
            ui.settings.general.import_status,
            Some(("Couldn't open a file picker: no portal".to_owned(), Tone::Bad))
        );
    }
}
