//! What the pet asks of the operating system beyond its windows: the C#'s `IPlatform` and `IMediaPlayer`
//! (`src/AiPet.Core/Platform.cs`), without `SendEscape`: the Rust pet has no Stop button, and never sends another app
//! simulated keys (see the spec's Decision Context).
//!
//! - [`Platform`]: opening links and folders, bringing an agent's desktop app to the front, and the music player.
//!   The desktop crate implements it for each operating system (`aipet_desktop::platform`), and both shells use that
//!   one. [`PetUi`](crate::PetUi) and Settings take it.
//! - [`NoPlatform`]: does nothing, and has no player.
//! - [`Recorder`]: notes every call, for tests.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// The music player as the Board reads it (`IMediaPlayer`'s Name, Song, Artist, Playing and TrackSince).
pub use aipet_core::board::Media;

/// The operating system's services (`IPlatform`). Calls come from the UI thread, and the player's from the core's.
pub trait Platform: Send + Sync {
    /// Opens a web address, or an app's own link (`claude://`, `codex://`), with whatever the desktop opens it with.
    fn open_url(&self, url: &str);

    /// Opens a folder in the file manager. The caller makes a missing one first.
    fn open_folder(&self, path: &Path);

    /// Brings an agent's desktop app (`claude`, `codex`) to the front, on the chat `link` opens when there is one.
    /// It never runs the agent's CLI: when no window of the app is found, at most a link is opened (the chat's, else
    /// the app's own).
    fn focus_agent(&self, agent: &str, link: Option<&str>);

    /// The music player the pet listens along with, or none where this platform has none.
    fn media(&self) -> Option<&dyn MediaPlayer>;
}

/// The music player (`IMediaPlayer`): Spotify through its window title on Windows, MPRIS on Linux.
pub trait MediaPlayer: Send + Sync {
    /// The player's name, for Settings' "Listen along with …" (e.g. "Spotify").
    fn name(&self) -> &str;

    /// Asks the player what it plays, about once a second while the user listens along, and says so.
    fn poll(&self) -> Media;

    fn previous(&self);

    fn play_pause(&self);

    fn next(&self);

    /// Brings the player to the front.
    fn focus(&self);
}

/// A platform that does nothing and has no player: where there is no implementation (the BSDs).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoPlatform;

impl Platform for NoPlatform {
    fn open_url(&self, _url: &str) {}

    fn open_folder(&self, _path: &Path) {}

    fn focus_agent(&self, _agent: &str, _link: Option<&str>) {}

    fn media(&self) -> Option<&dyn MediaPlayer> {
        None
    }
}

/// A call [`Recorder`] noted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    OpenUrl(String),
    OpenFolder(PathBuf),
    FocusAgent { agent: String, link: Option<String> },
    Poll,
    Previous,
    PlayPause,
    Next,
    Focus,
}

/// A platform for tests: it notes every call, in order, and does nothing else. It has a player only when made with
/// one ([`Recorder::with_player`]), which answers every poll with what it was given.
#[derive(Debug, Default)]
pub struct Recorder {
    calls: Mutex<Vec<Call>>,
    player: Option<(String, Media)>,
}

impl Recorder {
    /// A recorder with no player.
    pub fn new() -> Recorder {
        Recorder::default()
    }

    /// A recorder whose player is called `name` and always plays `media`.
    pub fn with_player(name: &str, media: Media) -> Recorder {
        Recorder {
            calls: Mutex::default(),
            player: Some((name.to_owned(), media)),
        }
    }

    /// The calls so far, oldest first.
    pub fn calls(&self) -> Vec<Call> {
        self.lock().clone()
    }

    fn note(&self, call: Call) {
        self.lock().push(call);
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Call>> {
        self.calls.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Platform for Recorder {
    fn open_url(&self, url: &str) {
        self.note(Call::OpenUrl(url.to_owned()));
    }

    fn open_folder(&self, path: &Path) {
        self.note(Call::OpenFolder(path.to_owned()));
    }

    fn focus_agent(&self, agent: &str, link: Option<&str>) {
        self.note(Call::FocusAgent {
            agent: agent.to_owned(),
            link: link.map(str::to_owned),
        });
    }

    fn media(&self) -> Option<&dyn MediaPlayer> {
        self.player.as_ref().map(|_| self as &dyn MediaPlayer)
    }
}

impl MediaPlayer for Recorder {
    fn name(&self) -> &str {
        self.player.as_ref().map_or("", |(name, _)| name)
    }

    fn poll(&self) -> Media {
        self.note(Call::Poll);
        self.player.as_ref().map(|(_, media)| media.clone()).unwrap_or_default()
    }

    fn previous(&self) {
        self.note(Call::Previous);
    }

    fn play_pause(&self) {
        self.note(Call::PlayPause);
    }

    fn next(&self) {
        self.note(Call::Next);
    }

    fn focus(&self) {
        self.note(Call::Focus);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recorder_notes_every_call_in_order_and_has_a_player_only_when_given_one() {
        let platform = Recorder::new();
        platform.open_url("claude://");
        platform.focus_agent("codex", None);
        platform.open_folder(Path::new("data"));
        assert!(platform.media().is_none());
        assert_eq!(
            platform.calls(),
            [
                Call::OpenUrl("claude://".into()),
                Call::FocusAgent {
                    agent: "codex".into(),
                    link: None
                },
                Call::OpenFolder("data".into()),
            ]
        );

        let song = Media {
            name: "Spotify".into(),
            song: Some("Song".into()),
            playing: true,
            ..Media::default()
        };
        let platform = Recorder::with_player("Spotify", song.clone());
        let player = platform.media().expect("a player");
        assert_eq!((player.name(), player.poll()), ("Spotify", song));
        player.play_pause();
        assert_eq!(platform.calls(), [Call::Poll, Call::PlayPause]);
        assert!(NoPlatform.media().is_none());
    }
}
