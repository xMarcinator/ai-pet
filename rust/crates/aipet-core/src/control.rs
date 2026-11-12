//! What agents ask of the pet over MCP (fnt-71): its avatars, its settings and its board. The listener sends a
//! [`Request`] to the pet, which owns its preferences and `config.json`, applies it as it applies the matching
//! change from Settings, and answers on the request's own channel. Nothing here goes through Settings: the pet
//! answers with Settings closed or absent.
//!
//! The pet is whatever [`PetLink`] reaches: today the iced pet's state, as news its shell hands it every frame.

use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::time::Duration;

/// How long the pet has to answer: a pet that doesn't is stuck, and the agent hears so.
pub const ANSWER_WITHIN: Duration = Duration::from_secs(5);

/// What an agent asks the pet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// The pets the user can wear, with their styles and themes, the one worn, and what didn't load.
    ListAvatars,
    /// Wear a pet: in this style and theme, else its default style and theme.
    Wear {
        name: String,
        style: Option<String>,
        theme: Option<String>,
    },
    GetSettings,
    /// Change these settings, leaving the rest as they are.
    SetSettings(SettingsChange),
    /// The board's cards, folded or not.
    GetBoard,
    /// Dismiss a card, as its close button does.
    Dismiss(String),
}

/// The settings an agent may read and change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// How big the pet is drawn, in percent.
    pub pet_size: i32,
    /// How big the bubbles are drawn, in percent.
    pub bubble_size: i32,
    pub bubbles: bool,
    pub on_top: bool,
    pub music: bool,
}

/// Some of [`Settings`]: what is none stays as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsChange {
    pub pet_size: Option<i32>,
    pub bubble_size: Option<i32>,
    pub bubbles: Option<bool>,
    pub on_top: Option<bool>,
    pub music: Option<bool>,
}

/// The pet worn: its name, style and theme.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worn {
    pub name: String,
    pub style: String,
    pub theme: String,
}

/// A pet the user can wear, with its styles in the order Settings shows them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Avatar {
    pub name: String,
    pub styles: Vec<Style>,
}

/// One of a pet's drawings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Style {
    pub style: String,
    /// `pixel`, `painted` or `vector`.
    pub kind: String,
    /// Its colour themes, its default first.
    pub themes: Vec<String>,
}

/// What the avatars folder had that didn't load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    /// The folder or file, as named in the avatars folder.
    pub path: String,
    pub reason: String,
}

/// A card on the board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    /// What [`Command::Dismiss`] takes: `claude:<sid>`, `jira:<key>`, `task:<id>`, …
    pub id: String,
    /// `claude`, `codex`, `tracker`, `jira`, `github` or `music`.
    pub source: String,
    pub title: String,
    /// idle, thinking, working, attention, done, review, music, paused or error.
    pub state: String,
}

/// What the pet answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    Avatars {
        worn: Worn,
        avatars: Vec<Avatar>,
        skipped: Vec<Skipped>,
    },
    Worn(Worn),
    Settings(Settings),
    Board(Vec<Card>),
    Dismissed(String),
}

/// A command and where its answer goes: an [`Answer`], or why the pet refused, which changed nothing.
#[derive(Clone, Debug)]
pub struct Request {
    pub command: Command,
    reply: SyncSender<Result<Answer, String>>,
}

impl Request {
    /// A request, and where its answer will come.
    pub fn new(command: Command) -> (Request, mpsc::Receiver<Result<Answer, String>>) {
        let (reply, answer) = mpsc::sync_channel(1);
        (Request { command, reply }, answer)
    }

    /// Answers it. An asker that stopped waiting hears nothing.
    pub fn answer(&self, answer: Result<Answer, String>) {
        let _ = self.reply.try_send(answer);
    }
}

/// How a request reaches the pet: false when it can't (the pet has gone).
pub type PetLink = Arc<dyn Fn(Request) -> bool + Send + Sync>;

/// Asks the pet and waits up to `within` for its answer. Blocks: an async caller runs it on a blocking thread.
pub fn ask(pet: &PetLink, command: Command, within: Duration) -> Result<Answer, String> {
    let (request, answer) = Request::new(command);
    if !pet(request) {
        return Err("the pet isn't running".into());
    }
    match answer.recv_timeout(within) {
        Ok(answer) => answer,
        Err(RecvTimeoutError::Timeout) => Err(format!("the pet didn't answer within {} s", within.as_secs_f64())),
        Err(RecvTimeoutError::Disconnected) => Err("the pet dropped the request without answering".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_comes_back_to_the_asker() {
        let pet: PetLink = Arc::new(|request: Request| {
            request.answer(Ok(Answer::Dismissed("x".into())));
            true
        });
        assert_eq!(
            ask(&pet, Command::Dismiss("x".into()), ANSWER_WITHIN),
            Ok(Answer::Dismissed("x".into()))
        );
    }

    #[test]
    fn a_pet_that_never_answers_is_an_error_after_the_wait() {
        let kept = Arc::new(std::sync::Mutex::new(Vec::new()));
        let held = Arc::clone(&kept);
        let pet: PetLink = Arc::new(move |request| {
            held.lock().unwrap().push(request);
            true
        });
        let started = std::time::Instant::now();
        let e = ask(&pet, Command::GetSettings, Duration::from_millis(50)).unwrap_err();
        assert!(e.contains("didn't answer"), "{e}");
        assert!(started.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    fn a_dropped_request_or_a_gone_pet_is_an_error_at_once() {
        let dropped: PetLink = Arc::new(|_| true);
        assert!(
            ask(&dropped, Command::GetBoard, ANSWER_WITHIN)
                .unwrap_err()
                .contains("dropped")
        );
        let gone: PetLink = Arc::new(|_| false);
        assert!(
            ask(&gone, Command::GetBoard, ANSWER_WITHIN)
                .unwrap_err()
                .contains("isn't running")
        );
    }

    #[test]
    fn a_late_answer_after_the_asker_gave_up_goes_nowhere() {
        let (request, answer) = Request::new(Command::GetBoard);
        drop(answer);
        request.answer(Ok(Answer::Board(Vec::new())));
    }
}
