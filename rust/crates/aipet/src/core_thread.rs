//! The core, on a thread of its own: what MainWindow's constructor starts and its Refresh does
//! (`src/AiPet.UI/MainWindow.axaml.cs:83-141, 303-314`).
//!
//! - the hook server, over the chats' store (AgentSessions);
//! - Codex's log watcher, and the Jira and GitHub watchers: GitHub reads the keys of Jira's issues, and looks again
//!   when Jira's change;
//! - the music player, polled every second while the user listens along (on a thread of its own, since a poll may
//!   run a program);
//! - the Board, refreshed every 250 ms and whenever a hook changed a chat or a watcher polled. It goes to the pet as
//!   [`News::Board`] when what it shows changed, and new reviews go as [`News::Alert`].
//!
//! Nothing here waits for the pet: the hook server's `changed`, the watchers' events and the pet's requests only post
//! to this thread. Dropping the [`Core`] stops it, and with it the server and the watchers.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use aipet_core::board::{Board, Media, Session, Sources};
use aipet_core::codex_watcher::CodexWatcher;
use aipet_core::github::{self, GitHubWatcher};
use aipet_core::jira::{self, Http, JiraWatcher};
use aipet_core::secrets::SecretStore;
use aipet_core::server::{self, HookServer};
use aipet_core::sessions::AgentSessions;
use aipet_ipc::protocol::unix_time;
use aipet_ui::News;
use aipet_ui::platform::Platform;

/// The Board is refreshed at least this often (MainWindow's 250 ms timer).
const REFRESH: Duration = Duration::from_millis(250);
/// The music player is asked what it plays this often, while the user listens along.
const MEDIA: Duration = Duration::from_secs(1);

/// What the core works with: the pet's own ([`Options::app`]), or a test's, which keeps off the user's pet and data.
pub struct Options {
    /// Where `jira.json` and `github.json` are: the data folder.
    pub data_dir: PathBuf,
    /// Where the hooks connect, and what the server writes.
    pub server: server::Options,
    /// Codex's folder; none for `$CODEX_HOME`, else `~/.codex`, read each time.
    pub codex_home: Option<PathBuf>,
    pub secrets: Arc<dyn SecretStore>,
    pub http: Http,
    pub platform: Arc<dyn Platform>,
    /// Whether the user listens along (`config.json`'s Music).
    pub music: bool,
}

impl Options {
    /// The pet's own: its data folder, endpoint and logs, Codex's folder, the platform's token store, and HTTPS.
    pub fn app(platform: Arc<dyn Platform>, music: bool) -> Options {
        Options {
            data_dir: aipet_ipc::paths::data_dir().to_owned(),
            server: server::Options::default(),
            codex_home: None,
            secrets: aipet_core::secrets::platform().into(),
            http: Http::new(),
            platform,
            music,
        }
    }
}

/// The running core. Dropping it stops it.
pub struct Core {
    events: Sender<Event>,
    thread: Option<JoinHandle<()>>,
    jira: Arc<JiraWatcher>,
    github: Arc<GitHubWatcher>,
}

/// What the pet asks of the core.
#[derive(Clone, Debug)]
pub struct Handle {
    events: Sender<Event>,
}

impl Handle {
    /// Hides a bubble until it does something new (`Board::dismiss`).
    pub fn dismiss(&self, id: &str) {
        let _ = self.events.send(Event::Dismiss(id.to_owned()));
    }

    /// Whether the user listens along: the player is asked while they do, and the Board has its bubble.
    pub fn music(&self, on: bool) {
        let _ = self.events.send(Event::Music(on));
    }
}

/// What the core's thread hears.
#[derive(Debug)]
enum Event {
    /// A hook event changed a chat.
    Changed,
    Jira(jira::Event),
    GitHub(github::Event),
    /// What the player plays now.
    Media(Media),
    Dismiss(String),
    Music(bool),
    Stop,
}

impl From<jira::Event> for Event {
    fn from(event: jira::Event) -> Event {
        Event::Jira(event)
    }
}

impl From<github::Event> for Event {
    fn from(event: github::Event) -> Event {
        Event::GitHub(event)
    }
}

/// Starts the core with `options`; its news goes to `news`.
pub fn start(options: Options, news: Sender<News>) -> Core {
    let (events, heard) = mpsc::channel();
    let Options {
        data_dir,
        server: server_options,
        codex_home,
        secrets,
        http,
        platform,
        music,
    } = options;

    // the watchers, as MainWindow wires them: GitHub knows Jira's keys, and Jira's news is passed on to it
    let jira = Arc::new(JiraWatcher::new(
        &data_dir,
        Arc::clone(&secrets),
        http.clone(),
        events.clone(),
    ));
    let github = Arc::new(GitHubWatcher::new(&data_dir, secrets, http, events.clone()));
    // weak: the GitHub watcher doesn't keep the Jira watcher alive
    let keys: Weak<JiraWatcher> = Arc::downgrade(&jira);
    github.set_jira_keys(move || {
        keys.upgrade()
            .map(|jira| jira.issues().into_iter().map(|issue| issue.key).collect())
            .unwrap_or_default()
    });
    jira.restart();
    github.restart();
    let (codex, sessions) = match &codex_home {
        Some(home) => (
            CodexWatcher::with_codex_home(home),
            AgentSessions::with_codex_home(home),
        ),
        None => (CodexWatcher::new(), AgentSessions::new()),
    };
    codex.start();
    let sessions = Arc::new(sessions);
    let changed = events.clone();
    let server = HookServer::with_options(
        Arc::clone(&sessions),
        move || {
            let _ = changed.send(Event::Changed);
        },
        server_options,
    );
    // the single-instance check has passed, so this is the only pet taking the endpoint. Whether it could goes to
    // the log; the pet runs on without hooks when it couldn't
    let _ = server.start();

    let listening = Arc::new(AtomicBool::new(music));
    let player = Player::start(platform, Arc::clone(&listening), events.clone());
    let core = CoreThread {
        news,
        board: Board::new(),
        shown: None,
        _server: server,
        sessions,
        codex,
        jira: Arc::clone(&jira),
        github: Arc::clone(&github),
        media: None,
        music,
        listening,
        _player: player,
    };
    let thread = thread::Builder::new()
        .name("aipet-core".into())
        .spawn(move || core.run(&heard))
        .expect("a thread for the core");
    Core {
        events,
        thread: Some(thread),
        jira,
        github,
    }
}

impl Core {
    /// What the pet asks of the core goes through this.
    pub fn handle(&self) -> Handle {
        Handle {
            events: self.events.clone(),
        }
    }

    /// The watchers, which Settings' pages save, test and forget with.
    pub fn jira(&self) -> Arc<JiraWatcher> {
        Arc::clone(&self.jira)
    }

    pub fn github(&self) -> Arc<GitHubWatcher> {
        Arc::clone(&self.github)
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.events.send(Event::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// What the Board shows the pet: news goes out when it changes.
#[derive(PartialEq)]
struct Shown {
    all: Vec<Session>,
    cards: Vec<Session>,
    state: &'static str,
    prop: Option<&'static str>,
}

impl Shown {
    fn of(board: &Board) -> Shown {
        Shown {
            all: board.all().to_vec(),
            cards: board.cards().to_vec(),
            state: board.state(),
            prop: board.prop(),
        }
    }
}

/// The core thread's state.
struct CoreThread {
    news: Sender<News>,
    board: Board,
    /// What the pet was last sent.
    shown: Option<Shown>,
    /// Kept for the thread's life: dropping it stops the server.
    _server: HookServer,
    sessions: Arc<AgentSessions>,
    codex: CodexWatcher,
    jira: Arc<JiraWatcher>,
    github: Arc<GitHubWatcher>,
    /// What the player played when last asked.
    media: Option<Media>,
    music: bool,
    listening: Arc<AtomicBool>,
    _player: Option<Player>,
}

impl CoreThread {
    fn run(mut self, heard: &Receiver<Event>) {
        let mut next = Instant::now();
        loop {
            let refresh = match heard.recv_timeout(next.saturating_duration_since(Instant::now())) {
                Ok(Event::Changed | Event::GitHub(github::Event::Changed)) | Err(RecvTimeoutError::Timeout) => true,
                Ok(Event::Jira(jira::Event::Changed)) => {
                    self.github.jira_changed();
                    true
                }
                Ok(Event::Jira(jira::Event::NewReviews(_)) | Event::GitHub(github::Event::NewReviews(_))) => {
                    let _ = self.news.send(News::Alert);
                    false
                }
                Ok(Event::Media(media)) => {
                    self.media = Some(media);
                    true
                }
                Ok(Event::Dismiss(id)) => {
                    self.board.dismiss(&id, unix_time(SystemTime::now()));
                    true
                }
                Ok(Event::Music(on)) => {
                    self.music = on;
                    self.listening.store(on, Ordering::SeqCst);
                    true
                }
                Ok(Event::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            };
            if refresh {
                self.refresh();
                next = Instant::now() + REFRESH;
            }
        }
    }

    /// Rebuilds the Board from the sources as they are now, and sends it to the pet if what it shows changed.
    fn refresh(&mut self) {
        let sources = Sources::read(
            &self.sessions,
            &self.jira,
            &self.github,
            Some(&self.codex),
            self.media.clone(),
            self.music,
        );
        self.board.refresh(&sources, unix_time(SystemTime::now()));
        let shown = Shown::of(&self.board);
        if self.shown.as_ref() != Some(&shown) {
            self.shown = Some(shown);
            let _ = self.news.send(News::Board(Arc::new(self.board.clone())));
        }
    }
}

/// The thread that asks the player what it plays, every [`MEDIA`] while the user listens along. It ends when it is
/// dropped.
struct Player {
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Player {
    /// The player's thread, where the platform has a player.
    fn start(platform: Arc<dyn Platform>, listening: Arc<AtomicBool>, events: Sender<Event>) -> Option<Player> {
        platform.media()?;
        let (stop, stopped) = mpsc::channel::<()>();
        let thread = thread::Builder::new()
            .name("aipet-player".into())
            .spawn(move || {
                loop {
                    if listening.load(Ordering::SeqCst)
                        && let Some(player) = platform.media()
                        && events.send(Event::Media(player.poll())).is_err()
                    {
                        return;
                    }
                    match stopped.recv_timeout(MEDIA) {
                        Err(RecvTimeoutError::Timeout) => {}
                        _ => return,
                    }
                }
            })
            .expect("a thread for the player");
        Some(Player {
            stop: Some(stop),
            thread: Some(thread),
        })
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // the thread wakes at once when its channel goes
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    use aipet_core::secrets::MemorySecrets;
    use aipet_ipc::connect::{NoPet, connect_to};
    use aipet_ipc::protocol::CONNECT;
    use aipet_ui::platform::{Call, Recorder};

    use super::*;

    /// A test's own folders and endpoint, removed with it; and what the server logged.
    struct Env {
        root: PathBuf,
        endpoint: OsString,
        log: Arc<Mutex<Vec<String>>>,
    }

    impl Env {
        fn new(test: &str) -> Env {
            static COUNT: AtomicUsize = AtomicUsize::new(0);
            let n = COUNT.fetch_add(1, Ordering::SeqCst);
            let pid = std::process::id();
            let root = std::env::temp_dir().join(format!("aipet-core-thread-{pid}-{test}"));
            let _ = fs::remove_dir_all(&root);
            for sub in ["data", "codex"] {
                fs::create_dir_all(root.join(sub)).unwrap();
            }
            // a socket's path must stay under 104 bytes, so it isn't in the temp folder
            let endpoint = if cfg!(windows) {
                format!("AiPet-test-core-{pid}-{n}")
            } else {
                format!("/tmp/aipet-c-{pid:x}-{n}.sock")
            };
            Env {
                root,
                endpoint: endpoint.into(),
                log: Arc::default(),
            }
        }

        /// Options that keep off the user's pet and data: this test's endpoint, folders and log, tokens in memory, a
        /// client that reaches only this computer.
        fn options(&self, platform: Arc<dyn Platform>, music: bool) -> Options {
            let log = Arc::clone(&self.log);
            Options {
                data_dir: self.root.join("data"),
                server: server::Options {
                    endpoint: self.endpoint.clone(),
                    events_log: self.root.join("data").join("hook-events.log"),
                    log: Box::new(move |line| log.lock().unwrap().push(line.to_owned())),
                },
                codex_home: Some(self.root.join("codex")),
                secrets: Arc::new(MemorySecrets::default()),
                http: Http::local(Duration::from_secs(5)),
                platform,
                music,
            }
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
            if cfg!(unix) {
                let _ = fs::remove_file(&self.endpoint);
            }
        }
    }

    /// The first Board the core sends that `matches`, within 20 s.
    fn board_where(news: &Receiver<News>, matches: impl Fn(&Board) -> bool) -> Arc<Board> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match news.recv_timeout(left) {
                Ok(News::Board(board)) if matches(&board) => return board,
                Ok(_) => {}
                Err(e) => panic!("no such Board: {e}"),
            }
        }
    }

    /// Sends a Claude hook event to the core's server, as the hook does; its reply.
    fn hook(endpoint: &OsString, sid: &str, event: &str) -> String {
        let at = unix_time(SystemTime::now());
        let line = format!(
            r#"{{"v":1,"type":"event","agent":"claude","at":{at},"sent":{at},"pid":4242,"env":{{}},"payload":{{"hook_event_name":"{event}","session_id":"{sid}","prompt":"Port the pet"}}}}"#
        );
        let mut pet = connect_to(endpoint, CONNECT).unwrap_or_else(|no| panic!("no pet: {no:?}"));
        pet.ask(&line).unwrap().expect("a reply")
    }

    #[test]
    fn a_hooks_chat_reaches_the_pet_and_a_dismissal_takes_its_bubble_away() {
        let env = Env::new("chat");
        let (news, heard) = mpsc::channel();
        let core = start(env.options(Arc::new(Recorder::new()), false), news);
        // at once: a Board with nothing on it
        let first = board_where(&heard, |_| true);
        assert!(first.cards().is_empty() && first.state() == "sleep");
        let listening = env.log.lock().unwrap().clone();
        assert!(
            listening.iter().any(|l| l.starts_with("hooks: listening on")),
            "{listening:?}"
        );

        let sid = "6f2ac1c4-0d55-4bd5-9a2b-3c4d5e6f7a8b";
        let id = format!("claude:{sid}");
        let reply = hook(&env.endpoint, sid, "UserPromptSubmit");
        assert!(reply.contains(r#""ok":true"#), "{reply}");
        let shown = board_where(&heard, |b| b.cards().iter().any(|s| s.id == id));
        assert_ne!(shown.state(), "sleep");

        core.handle().dismiss(&id);
        board_where(&heard, |b| {
            b.dismissed().contains_key(&id) && b.cards().iter().all(|s| s.id != id)
        });

        // stopping it stops the server
        drop(core);
        assert_eq!(connect_to(&env.endpoint, Duration::ZERO).err(), Some(NoPet::Closed));
    }

    #[test]
    fn new_reviews_become_alerts_and_the_jira_watchers_news_reaches_github() {
        let env = Env::new("alerts");
        let (news, heard) = mpsc::channel();
        let core = start(env.options(Arc::new(Recorder::new()), false), news);
        board_where(&heard, |_| true);
        for event in [
            Event::Jira(jira::Event::NewReviews(2)),
            Event::GitHub(github::Event::NewReviews(1)),
        ] {
            core.events.send(event).unwrap();
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                match heard.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                    Ok(News::Alert) => break,
                    Ok(News::Board(_)) => {}
                    Err(e) => panic!("no alert: {e}"),
                }
            }
        }
    }

    #[test]
    fn the_player_is_asked_only_while_the_user_listens_along() {
        let env = Env::new("music");
        let playing = Media {
            name: "Spotify".into(),
            song: Some("A song".into()),
            artist: "A band".into(),
            playing: true,
            track_since: unix_time(SystemTime::now()),
        };
        let platform = Arc::new(Recorder::with_player("Spotify", playing));
        let (news, heard) = mpsc::channel();
        let core = start(env.options(platform.clone(), true), news);
        let board = board_where(&heard, |b| b.find("music").is_some());
        assert_eq!(board.find("music").map(|s| s.eff), Some("music"));

        core.handle().music(false);
        board_where(&heard, |b| b.find("music").is_none());
        let polls = || platform.calls().iter().filter(|c| **c == Call::Poll).count();
        let asked = polls();
        thread::sleep(MEDIA * 3);
        assert!(
            polls() <= asked + 1,
            "{} polls after listening stopped",
            polls() - asked
        );
        drop(core);
    }
}
