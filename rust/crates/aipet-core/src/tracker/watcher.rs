//! The tracker watcher: the pet as the flow-next tracker's companion device, following its active tasks.
//!
//! - **Settings:** `tracker.json` in the data folder ([`TrackerSettings`]). The credential is `AiPet:Tracker` in the
//!   secret store: `{base_url, device_id, token}` as JSON.
//! - **Enrolling:** [`TrackerWatcher::enroll`] redeems the one-time code the tracker's client made
//!   (`POST {base}/enroll {"code", "role": "companion"}`) for a bearer token.
//! - **Following:** a thread of its own reads `hello` (whether the user allows answers and pause/stop), then follows
//!   the change feed (`/api/v1/companion/feed`): a snapshot, then the changes, resumed by `(feed, seq)` after a
//!   reconnect. A connection that fails is tried again after 1 s, doubling to a minute.
//! - **Revocation:** the tracker closes the feed with "Device credential no longer valid", or refuses the token
//!   (401 or 403). The tasks are dropped and the watcher stops until the pet is enrolled again.
//! - **Collapsing chats:** while connected, each chat the pet hears is looked up once
//!   (`/api/v1/runs/by-session/{id}`): a chat that is a tracker run collapses into its task's bubble. A miss is asked
//!   again a minute later, as a run's session can start after its chat.
//!
//! Two kinds of tracker: one on this computer (`http://127.0.0.1:<port>`, loopback only), which the pet calls with
//! its bearer token, and a hosted one (`https://<host>[:<port>]`), its mutual-TLS listener, which the pet calls with
//! the certificate it enrolled for ([`super::tls`]). Nothing the pet's hooks hear is sent to the tracker except a
//! chat's session id in that lookup.

use std::collections::HashMap;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{IpAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::HeaderValue;
use tungstenite::protocol::CloseFrame;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::{Message, WebSocket};

use super::api::{self, FeedMessage};
use super::tls;
use super::view::{Applied, TaskView};
use super::wire::{Tone, TrackerTask};
use crate::http::{Failure, Http, escape_data_string, lock};
use crate::secrets::{self, SecretStore};

/// How long the tasks stay shown after the connection is lost; then they're dropped.
pub const STALE_AFTER: Duration = Duration::from_secs(120);
/// The first wait before connecting again, doubling up to [`BACKOFF_MAX`].
pub const BACKOFF_FIRST: Duration = Duration::from_secs(1);
pub const BACKOFF_MAX: Duration = Duration::from_secs(60);
/// How long a request or the feed's handshake may take.
pub const TIMEOUT: Duration = Duration::from_secs(20);
/// How often the feed's loop looks up new chats.
const LOOKUP_EVERY: Duration = Duration::from_secs(10);
/// How soon a chat that is no run is asked about again.
const MISS_AGAIN: Duration = Duration::from_secs(60);
/// The most chats looked up at once.
const LOOKUPS_AT_ONCE: usize = 20;
/// How often the feed's loop wakes to see whether it was stopped.
const TICK: Duration = Duration::from_millis(250);

/// The close reason of a companion whose credential was revoked (the tracker's `CREDENTIAL_REVOKED`).
pub const CREDENTIAL_REVOKED: &str = "Device credential no longer valid";
/// What the error bubble and Settings say once the tracker revoked the pet.
pub const REVOKED_TEXT: &str = "this pet was revoked; enroll it again";

/// What the user chose for the tracker: `tracker.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackerSettings {
    /// The tracker's address, e.g. `http://127.0.0.1:3210`; none while it isn't set up.
    pub base_url: Option<String>,
    pub sessions_without_task: SessionsWithoutTask,
}

/// Whether chats that are no tracker task show while the tracker is connected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionsWithoutTask {
    #[default]
    Show,
    Hide,
}

impl TrackerSettings {
    /// The settings in `path`; the defaults when it's missing or can't be read.
    pub fn load_from(path: &Path) -> TrackerSettings {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, text)
    }
}

/// The secret: what enrolling gave, and for which tracker. A hosted tracker's adds the pet's certificate (then the
/// tracker's CA) and its key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Credential {
    base_url: String,
    device_id: String,
    token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key: Option<String>,
}

impl Credential {
    /// The client for the tracker's routes: loopback HTTP, or the certificate over HTTPS.
    fn http(&self) -> Result<Http, String> {
        match (&self.cert, &self.key) {
            (Some(chain), Some(key)) => Http::tracker(&tls::ca_of(chain)?, Some((chain, key)), TIMEOUT),
            _ => Ok(Http::local(TIMEOUT)),
        }
    }

    /// The `Authorization` header's value: the bearer token on loopback; none on a hosted tracker, where the
    /// certificate says who the pet is.
    fn bearer(&self) -> Option<String> {
        self.cert.is_none().then(|| format!("Bearer {}", self.token))
    }
}

/// The headers of a call with `bearer`, and `more`.
fn headers<'a>(bearer: Option<&'a str>, more: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    bearer
        .map(|b| ("Authorization", b))
        .into_iter()
        .chain(more.iter().copied())
        .collect()
}

/// Where the watcher is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// No tracker set up, or no credential for it.
    Off,
    Connecting,
    Connected {
        /// Whether the user allows this pet to answer prompts and pause or stop runs.
        actions_allowed: bool,
    },
    /// The last attempt failed with this text; another follows.
    Unreachable(String),
    /// The tracker revoked the pet; it stays off until it is enrolled again.
    Revoked,
}

/// What the pet may do to a tracker run, when the user allows it for this pet (the tracker's switch).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Answer a permission prompt, once.
    Allow {
        run: String,
        request: String,
    },
    Deny {
        run: String,
        request: String,
    },
    Pause {
        run: String,
    },
    Stop {
        run: String,
    },
}

impl Action {
    /// The route and body it posts.
    fn request(&self) -> (String, &'static str) {
        let path = |run: &str, rest: &str| format!("/api/runs/{}/{rest}", escape_data_string(run));
        match self {
            Action::Allow { run, request } => (
                path(run, &format!("permissions/{}", escape_data_string(request))),
                r#"{"decision":"allow"}"#,
            ),
            Action::Deny { run, request } => (
                path(run, &format!("permissions/{}", escape_data_string(request))),
                r#"{"decision":"deny"}"#,
            ),
            Action::Pause { run } => (path(run, "pause"), "{}"),
            Action::Stop { run } => (path(run, "cancel"), "{}"),
        }
    }

    /// What the bubble says once it's done.
    pub fn done_text(&self) -> &'static str {
        match self {
            Action::Allow { .. } => "Allowed",
            Action::Deny { .. } => "Denied",
            Action::Pause { .. } => "Paused",
            Action::Stop { .. } => "Stopped",
        }
    }
}

/// What the watcher tells its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The tasks, the status or the chats' runs may have changed.
    Changed,
}

/// The tracker watcher (module docs).
pub struct TrackerWatcher {
    shared: Arc<Shared>,
    running: Mutex<Option<Arc<Stop>>>,
}

struct Shared {
    file: PathBuf,
    secrets: Arc<dyn SecretStore>,
    events: Box<dyn Fn(Event) + Send + Sync>,
    /// The ids of the chats the pet shows (`claude:<sid>`, `codex:<sid>`), for the run lookups.
    chats: Mutex<Box<dyn Fn() -> Vec<String> + Send + Sync>>,
    state: Mutex<State>,
}

struct State {
    settings: TrackerSettings,
    status: Status,
    view: TaskView,
    /// When the connection was lost, while it is; the tasks are dropped [`STALE_AFTER`] it.
    lost: Option<Instant>,
    /// Each chat's session id: the task its run works on (none: no run), and when that was asked.
    runs: HashMap<String, (Option<String>, Instant)>,
}

impl TrackerWatcher {
    /// A watcher with `tracker.json` from `data_dir`, the credential in `secrets`, and its events sent to `events`.
    /// It doesn't connect until [`TrackerWatcher::restart`].
    pub fn new<E>(data_dir: &Path, secrets: Arc<dyn SecretStore>, events: Sender<E>) -> TrackerWatcher
    where
        E: From<Event> + Send + 'static,
    {
        let file = data_dir.join("tracker.json");
        let settings = TrackerSettings::load_from(&file);
        TrackerWatcher {
            shared: Arc::new(Shared {
                file,
                secrets,
                events: Box::new(move |event| {
                    let _ = events.send(E::from(event));
                }),
                chats: Mutex::new(Box::new(Vec::new)),
                state: Mutex::new(State {
                    settings,
                    status: Status::Off,
                    view: TaskView::default(),
                    lost: None,
                    runs: HashMap::new(),
                }),
            }),
            running: Mutex::new(None),
        }
    }

    pub fn settings(&self) -> TrackerSettings {
        self.shared.lock().settings.clone()
    }

    pub fn status(&self) -> Status {
        self.shared.lock().status.clone()
    }

    /// Whether a credential is saved.
    pub fn is_enrolled(&self) -> bool {
        self.shared.credential().is_some()
    }

    /// What the error bubble says, while the connection fails or after a revocation.
    pub fn last_error(&self) -> Option<String> {
        match &self.shared.lock().status {
            Status::Unreachable(why) => Some(why.clone()),
            Status::Revoked => Some(REVOKED_TEXT.into()),
            _ => None,
        }
    }

    /// The active tasks: red first, then amber, then the rest, newest change first; each with the session ids of
    /// the chats that are its runs. None once the connection has been lost for [`STALE_AFTER`].
    pub fn tasks(&self) -> Vec<TrackerTask> {
        let state = self.shared.lock();
        if state.lost.is_some_and(|lost| lost.elapsed() >= STALE_AFTER) {
            return Vec::new();
        }
        let mut tasks: Vec<TrackerTask> = state.view.tasks().cloned().collect();
        for task in &mut tasks {
            task.sessions = state
                .runs
                .iter()
                .filter(|(_, (run_task, _))| run_task.as_deref() == Some(task.id.as_str()))
                .map(|(sid, _)| sid.clone())
                .collect();
            task.sessions.sort();
        }
        let rank = |tone: Tone| match tone {
            Tone::Red => 0,
            Tone::Amber => 1,
            Tone::Normal => 2,
        };
        tasks.sort_by(|a, b| rank(a.tone).cmp(&rank(b.tone)).then_with(|| b.at.cmp(&a.at)));
        tasks
    }

    /// Where the watcher finds the ids of the chats the pet shows, for the run lookups.
    pub fn set_chats(&self, chats: impl Fn() -> Vec<String> + Send + Sync + 'static) {
        *lock(&self.shared.chats) = Box::new(chats);
    }

    /// Saves the settings. A new address forgets the credential (it was the old tracker's) and starts again.
    pub fn save(&self, settings: TrackerSettings) -> io::Result<()> {
        let moved = {
            let mut state = self.shared.lock();
            let moved = state.settings.base_url != settings.base_url;
            state.settings = settings.clone();
            moved
        };
        settings.save_to(&self.shared.file)?;
        if moved {
            self.forget();
        }
        (self.shared.events)(Event::Changed);
        Ok(())
    }

    /// Redeems a one-time code from the tracker's client at `base_url`, saves the credential and the address, and
    /// starts following. Blocks for the request (at most [`TIMEOUT`]): call it off the UI thread. The error is what
    /// to show: the tracker's refusal, or why it couldn't be reached.
    pub fn enroll(&self, base_url: &str, code: &str) -> Result<(), String> {
        let base = base(base_url)?;
        let code = code.trim();
        if code.is_empty() {
            return Err("Enter the code from the tracker's Devices & runners page".into());
        }
        // a hosted tracker: its CA pinned by the code, and a certificate for a key made here
        let hosted = match base.strip_prefix("https://") {
            Some(_) => {
                let (_, fingerprint) = tls::split_code(code)?;
                let (host, port) = host_port(&base);
                let ca = tls::pin_ca(&host, port, &fingerprint, TIMEOUT)?;
                let (key, csr) = tls::key_and_csr()?;
                Some((fingerprint, Http::tracker(&ca, None, TIMEOUT)?, key, csr))
            }
            None => None,
        };
        let mut body = serde_json::json!({"code": code, "role": "companion"});
        if let Some((_, _, _, csr)) = &hosted {
            body["csr"] = csr.as_str().into();
        }
        let http = match &hosted {
            Some((_, http, _, _)) => http.clone(),
            None => Http::local(TIMEOUT),
        };
        let answer = http
            .post(
                &format!("{base}/enroll"),
                &[("Content-Type", "application/json")],
                &body.to_string(),
            )
            .map_err(|failure| unreachable(&base, &failure))?;
        if !answer.success() {
            return Err(api::refusal(&answer.status_line(), &answer.body));
        }
        let (device_id, token) = api::enrolled(&answer.body)?;
        let (cert, key) = match hosted {
            Some((fingerprint, _, key, _)) => {
                let chain = api::certificate(&answer.body).ok_or("The tracker sent no certificate")?;
                let ca = tls::certificates(&tls::ca_of(&chain)?)?;
                if tls::fingerprint(&ca[0]) != fingerprint {
                    return Err("The tracker's certificate comes from another CA than the code names".into());
                }
                (Some(chain), Some(key))
            }
            None => (None, None),
        };
        let credential = Credential {
            base_url: base.clone(),
            device_id,
            token,
            cert,
            key,
        };
        let secret = serde_json::to_string(&credential).map_err(|e| e.to_string())?;
        self.shared
            .secrets
            .write(secrets::TRACKER, &credential.device_id, &secret)
            .map_err(|e| format!("Couldn't save the tracker's token: {e}"))?;
        let settings = TrackerSettings {
            base_url: Some(base),
            ..self.settings()
        };
        self.shared.lock().settings = settings.clone();
        settings
            .save_to(&self.shared.file)
            .map_err(|e| format!("Couldn't save tracker.json: {e}"))?;
        self.restart();
        Ok(())
    }

    /// Does `action` in the tracker, as this pet. Blocks for the request (at most [`TIMEOUT`]): call it off the UI
    /// thread. Refused here unless the tracker said this pet may; the error is what the bubble shows: the tracker's
    /// refusal (a prompt someone else answered first, say), or why it couldn't be reached.
    pub fn act(&self, action: &Action) -> Result<(), String> {
        if !matches!(self.status(), Status::Connected { actions_allowed: true }) {
            return Err("Answers and pause/stop are off for this pet".into());
        }
        let credential = self.shared.credential().ok_or("The pet isn't enrolled")?;
        let base = &credential.base_url;
        let bearer = credential.bearer();
        let (path, body) = action.request();
        let answer = credential
            .http()?
            .post(
                &format!("{base}{path}"),
                &headers(bearer.as_deref(), &[("Content-Type", "application/json")]),
                body,
            )
            .map_err(|failure| unreachable(base, &failure))?;
        if answer.success() {
            Ok(())
        } else {
            Err(api::refusal(&answer.status_line(), &answer.body))
        }
    }

    /// Forgets the credential and the tasks; the watcher is off until the pet is enrolled again. The device stays
    /// listed in the tracker until the user revokes it there.
    pub fn forget(&self) {
        self.shared.secrets.delete(secrets::TRACKER);
        self.restart();
    }

    /// Stops following and starts again: off without a credential for the tracker set up.
    pub fn restart(&self) {
        if let Some(old) = lock(&self.running).take() {
            old.stop();
        }
        {
            let mut state = self.shared.lock();
            state.view.clear();
            state.lost = None;
            state.runs.clear();
            state.status = Status::Off;
        }
        let credential = self.shared.credential();
        if let Some(credential) = credential {
            let stop = Arc::new(Stop::default());
            *lock(&self.running) = Some(Arc::clone(&stop));
            self.shared.lock().status = Status::Connecting;
            let shared = Arc::clone(&self.shared);
            // without a thread the watcher stays off, as it does before its first start
            let _ = thread::Builder::new()
                .name("aipet-tracker".into())
                .spawn(move || shared.follow(&credential, &stop));
        }
        (self.shared.events)(Event::Changed);
    }
}

impl Drop for TrackerWatcher {
    fn drop(&mut self) {
        if let Some(running) = lock(&self.running).take() {
            running.stop();
        }
    }
}

/// How one connection ended.
enum End {
    Stopped,
    Revoked,
    /// It failed with this text; `connected` says whether it got as far as the feed.
    Failed {
        why: String,
        connected: bool,
    },
    /// The feed moved on (another process or pod): connect again at once, for a snapshot.
    Resync,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    /// The saved credential, if it is for the tracker set up.
    fn credential(&self) -> Option<Credential> {
        let credential: Credential = serde_json::from_str(&self.secrets.read(secrets::TRACKER)?).ok()?;
        let base = self.lock().settings.base_url.clone()?;
        (base == credential.base_url).then_some(credential)
    }

    fn changed(&self) {
        (self.events)(Event::Changed);
    }

    /// The state, for the watcher thread `stop` belongs to to change: none once that thread was stopped. The flag is
    /// read under the lock, so a thread that a restart stopped can't write over what the new one found (a restart
    /// stops the old thread before it clears the state).
    fn state(&self, stop: &Stop) -> Option<MutexGuard<'_, State>> {
        let state = self.lock();
        (!stop.stopped()).then_some(state)
    }

    fn set_status(&self, stop: &Stop, status: Status) {
        let Some(mut state) = self.state(stop) else {
            return;
        };
        if state.status != status {
            if matches!(status, Status::Connected { .. }) {
                state.lost = None;
            } else if matches!(state.status, Status::Connected { .. }) {
                state.lost = Some(Instant::now());
            }
            state.status = status;
            drop(state);
            self.changed();
        }
    }

    /// The watcher's thread: connections one after another until it's stopped or revoked.
    fn follow(&self, credential: &Credential, stop: &Stop) {
        let mut wait = BACKOFF_FIRST;
        loop {
            match self.connection(credential, stop) {
                End::Stopped => return,
                End::Revoked => {
                    if let Some(mut state) = self.state(stop) {
                        state.view.clear();
                        state.runs.clear();
                    }
                    self.set_status(stop, Status::Revoked);
                    self.changed();
                    return;
                }
                End::Resync => {
                    if let Some(mut state) = self.state(stop) {
                        state.view.forget_point();
                    }
                    wait = BACKOFF_FIRST;
                    continue;
                }
                End::Failed { why, connected } => {
                    if connected {
                        wait = BACKOFF_FIRST;
                    }
                    self.set_status(stop, Status::Unreachable(why));
                    if stop.wait(wait) {
                        return;
                    }
                    wait = (wait * 2).min(BACKOFF_MAX);
                }
            }
        }
    }

    /// One connection: hello, then the feed until it ends.
    fn connection(&self, credential: &Credential, stop: &Stop) -> End {
        let base = &credential.base_url;
        let http = match credential.http() {
            Ok(http) => http,
            Err(why) => return End::Failed { why, connected: false },
        };
        let bearer = credential.bearer();
        let headers = headers(bearer.as_deref(), &[]);
        let hello = match http.get(&format!("{base}/api/attach/hello"), &headers) {
            Ok(answer) if matches!(answer.status, 401 | 403) => return End::Revoked,
            Ok(answer) if !answer.success() => {
                return End::Failed {
                    why: api::refusal(&answer.status_line(), &answer.body),
                    connected: false,
                };
            }
            Ok(answer) => answer.body,
            Err(failure) => {
                return End::Failed {
                    why: unreachable(base, &failure),
                    connected: false,
                };
            }
        };
        let actions_allowed = api::actions_allowed(&hello);
        let point = self.lock().view.point().cloned();
        let mut socket = match open_feed(credential, point.as_ref()) {
            Ok(socket) => socket,
            Err(Opened::Refused) => return End::Revoked,
            Err(Opened::Failed(why)) => return End::Failed { why, connected: false },
        };
        self.set_status(stop, Status::Connected { actions_allowed });
        let mut next_lookup = Instant::now();
        loop {
            if stop.stopped() {
                let _ = socket.close(None);
                return End::Stopped;
            }
            if Instant::now() >= next_lookup {
                if self.look_up_runs(stop, &http, base, &headers) {
                    return End::Revoked;
                }
                next_lookup = Instant::now() + LOOKUP_EVERY;
            }
            match socket.read() {
                Ok(Message::Text(text)) => {
                    // a message this pet can't read is skipped; the next snapshot puts the view right
                    if let Ok(message) = FeedMessage::parse(text.as_str()) {
                        let Some(mut state) = self.state(stop) else {
                            return End::Stopped;
                        };
                        let applied = state.view.apply(message);
                        drop(state);
                        match applied {
                            Applied::Changed => self.changed(),
                            Applied::Unchanged => {}
                            Applied::Resync => return End::Resync,
                        }
                    }
                }
                Ok(Message::Close(frame)) => return closed(frame),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                    return End::Failed {
                        why: format!("the tracker at {base} closed the feed"),
                        connected: true,
                    };
                }
                Err(e) => {
                    return End::Failed {
                        why: format!("the tracker's feed at {base} failed: {e}"),
                        connected: true,
                    };
                }
            }
        }
    }

    /// Asks the tracker which of the pet's chats are its runs: new chats, and misses a minute old. Forgets chats
    /// the pet no longer shows. True when the tracker refused the token.
    fn look_up_runs(&self, stop: &Stop, http: &Http, base: &str, headers: &[(&str, &str)]) -> bool {
        let shown: Vec<String> = (lock(&self.chats))()
            .iter()
            .filter_map(|id| id.split_once(':').map(|(_, sid)| sid.to_owned()))
            .filter(|sid| !sid.is_empty())
            .collect();
        let due: Vec<String> = {
            let Some(mut state) = self.state(stop) else {
                return false;
            };
            state.runs.retain(|sid, _| shown.contains(sid));
            shown
                .iter()
                .filter(|sid| match state.runs.get(*sid) {
                    None => true,
                    Some((task, asked)) => task.is_none() && asked.elapsed() >= MISS_AGAIN,
                })
                .take(LOOKUPS_AT_ONCE)
                .cloned()
                .collect()
        };
        let mut changed = false;
        for sid in due {
            let url = format!("{base}/api/v1/runs/by-session/{}", escape_data_string(&sid));
            let task = match http.get(&url, headers) {
                Ok(answer) if answer.status == 200 => api::run_task(&answer.body),
                Ok(answer) if answer.status == 404 => None,
                Ok(answer) if matches!(answer.status, 401 | 403) => return true,
                // asked again at the next lookup
                _ => continue,
            };
            let Some(mut state) = self.state(stop) else {
                return false;
            };
            let before = state.runs.insert(sid, (task.clone(), Instant::now()));
            changed |= before.map(|(t, _)| t) != Some(task);
        }
        if changed {
            self.changed();
        }
        false
    }
}

/// Why opening the feed failed.
enum Opened {
    /// The tracker refused the token.
    Refused,
    Failed(String),
}

/// The feed's connection: plain on loopback, TLS with the pet's certificate to a hosted tracker.
enum FeedStream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl FeedStream {
    fn tcp(&self) -> &TcpStream {
        match self {
            FeedStream::Plain(tcp) => tcp,
            FeedStream::Tls(tls) => tls.get_ref(),
        }
    }
}

impl Read for FeedStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            FeedStream::Plain(tcp) => tcp.read(buf),
            FeedStream::Tls(tls) => tls.read(buf),
        }
    }
}

impl Write for FeedStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            FeedStream::Plain(tcp) => tcp.write(buf),
            FeedStream::Tls(tls) => tls.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            FeedStream::Plain(tcp) => tcp.flush(),
            FeedStream::Tls(tls) => tls.flush(),
        }
    }
}

type Feed = WebSocket<FeedStream>;

/// The change feed's WebSocket, resuming at `point` when there is one, reading with a short timeout so its loop sees
/// a stop.
fn open_feed(credential: &Credential, point: Option<&api::Point>) -> Result<Feed, Opened> {
    let base = &credential.base_url;
    let (host, port) = host_port(base);
    let failed = |e: &dyn std::fmt::Display| Opened::Failed(format!("can't reach the tracker at {base}: {e}"));
    let (scheme, authority) = match base.split_once("://") {
        Some(("https", authority)) => ("wss", authority),
        Some((_, authority)) => ("ws", authority),
        None => return Err(failed(&"no scheme")),
    };
    let mut url = format!("{scheme}://{authority}/api/v1/companion/feed");
    if let Some(point) = point {
        url.push_str(&format!(
            "?feed={}&since={}",
            escape_data_string(&point.feed),
            point.seq
        ));
    }
    let mut request = url.into_client_request().map_err(|e| failed(&e))?;
    if let Some(bearer) = credential.bearer() {
        request
            .headers_mut()
            .insert("authorization", HeaderValue::from_str(&bearer).map_err(|e| failed(&e))?);
    }
    let stream = match (&credential.cert, &credential.key) {
        (Some(chain), Some(key)) => {
            let config =
                tls::client_config(&tls::ca_of(chain).map_err(|e| failed(&e))?, chain, key).map_err(|e| failed(&e))?;
            FeedStream::Tls(Box::new(
                tls::connect(&host, port, config, TIMEOUT).map_err(|e| failed(&e))?,
            ))
        }
        _ => {
            let tcp = tls::connect_tcp(&host, port, TIMEOUT).map_err(|e| failed(&e))?;
            tcp.set_read_timeout(Some(TIMEOUT)).map_err(|e| failed(&e))?;
            tcp.set_write_timeout(Some(TIMEOUT)).map_err(|e| failed(&e))?;
            FeedStream::Plain(tcp)
        }
    };
    let (socket, _) = match tungstenite::client(request, stream) {
        Ok(opened) => opened,
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(answer)))
            if matches!(answer.status().as_u16(), 401 | 403) =>
        {
            return Err(Opened::Refused);
        }
        Err(tungstenite::HandshakeError::Failure(e)) => return Err(failed(&e)),
        Err(tungstenite::HandshakeError::Interrupted(_)) => return Err(failed(&"the handshake timed out")),
    };
    socket
        .get_ref()
        .tcp()
        .set_read_timeout(Some(TICK))
        .map_err(|e| failed(&e))?;
    Ok(socket)
}

/// How a close frame ends the connection: revoked with the tracker's reason, else failed.
fn closed(frame: Option<CloseFrame>) -> End {
    match frame {
        Some(frame) if frame.code == CloseCode::Policy && frame.reason.as_str() == CREDENTIAL_REVOKED => End::Revoked,
        Some(frame) => End::Failed {
            why: format!(
                "the tracker closed the feed ({}): {}",
                u16::from(frame.code),
                frame.reason
            ),
            connected: true,
        },
        None => End::Failed {
            why: "the tracker closed the feed".into(),
            connected: true,
        },
    }
}

/// The address as the watcher keeps it, without a trailing slash: a tracker on this computer,
/// `http://<loopback host>:<port>`, or a hosted one's mutual-TLS listener, `https://<host>[:<port>]`.
fn base(address: &str) -> Result<String, String> {
    let address = address.trim().trim_end_matches('/');
    let (hosted, rest) = match (address.strip_prefix("https://"), address.strip_prefix("http://")) {
        (Some(rest), _) => (true, rest),
        (None, Some(rest)) => (false, rest),
        _ => {
            return Err(
                "Enter the tracker's address: http://127.0.0.1:3210 on this computer, or its https:// address".into(),
            );
        }
    };
    if rest.is_empty() || rest.contains(['/', '?', '#', '@']) {
        return Err("Enter only the tracker's address, without a path".into());
    }
    let host = match rest.rsplit_once(':').filter(|(_, port)| !port.ends_with(']')) {
        Some((host, port)) if port.parse::<u16>().is_ok_and(|p| p > 0) => host,
        Some(_) => return Err("The tracker's port isn't a port".into()),
        None if hosted => rest,
        None => return Err("The tracker's address needs its port, e.g. http://127.0.0.1:3210".into()),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let loopback = host.eq_ignore_ascii_case("localhost") || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback());
    if !hosted && !loopback {
        return Err("A tracker on another computer is reached over https:// (its mutual-TLS listener)".into());
    }
    Ok(address.to_owned())
}

/// A kept address's host and port (443 when an https:// address names none).
fn host_port(base: &str) -> (String, u16) {
    let (scheme, rest) = base.split_once("://").unwrap_or(("http", base));
    match rest.rsplit_once(':').filter(|(_, port)| !port.ends_with(']')) {
        Some((host, port)) => (host.to_owned(), port.parse().unwrap_or(0)),
        None => (rest.to_owned(), if scheme == "https" { 443 } else { 80 }),
    }
}

/// What a failed request says.
fn unreachable(base: &str, failure: &Failure) -> String {
    match failure {
        Failure::Timeout => format!("the tracker at {base} didn't answer"),
        Failure::Unreachable(why) | Failure::Other(why) => format!("can't reach the tracker at {base}: {why}"),
    }
}

/// The watcher thread's stop flag.
#[derive(Debug, Default)]
struct Stop {
    stopped: Mutex<bool>,
    wake: Condvar,
}

impl Stop {
    fn stop(&self) {
        *lock(&self.stopped) = true;
        self.wake.notify_all();
    }

    fn stopped(&self) -> bool {
        *lock(&self.stopped)
    }

    /// Waits `time`, less if stopped; whether it is.
    fn wait(&self, time: Duration) -> bool {
        let stopped = lock(&self.stopped);
        let (stopped, _) = self
            .wake
            .wait_timeout_while(stopped, time, |stopped| !*stopped)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *stopped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_by_http_with_its_port_and_hosted_by_https() {
        for (address, kept) in [
            ("http://127.0.0.1:3210", Some("http://127.0.0.1:3210")),
            (" http://localhost:3210/ ", Some("http://localhost:3210")),
            ("http://[::1]:3210", Some("http://[::1]:3210")),
            ("https://tracker.example.com", Some("https://tracker.example.com")),
            (
                "https://tracker.example.com:8443/",
                Some("https://tracker.example.com:8443"),
            ),
            ("https://[2001:db8::1]:8443", Some("https://[2001:db8::1]:8443")),
            ("http://127.0.0.1", None),
            ("http://127.0.0.1:99999", None),
            ("http://192.168.1.4:3210", None),
            ("http://127.0.0.1:3210/api", None),
            ("https://tracker.example.com/api", None),
            ("https://user@tracker.example.com", None),
            ("127.0.0.1:3210", None),
        ] {
            assert_eq!(base(address).ok().as_deref(), kept, "{address}");
        }
        assert_eq!(
            host_port("https://tracker.example.com"),
            ("tracker.example.com".into(), 443)
        );
        assert_eq!(host_port("https://[2001:db8::1]:8443"), ("[2001:db8::1]".into(), 8443));
        assert_eq!(host_port("http://127.0.0.1:3210"), ("127.0.0.1".into(), 3210));
    }

    #[test]
    fn settings_read_leniently_and_round_trip() {
        let dir = std::env::temp_dir().join(format!("aipet-tracker-settings-{}", std::process::id()));
        let file = dir.join("tracker.json");
        assert_eq!(TrackerSettings::load_from(&file), TrackerSettings::default());
        let settings = TrackerSettings {
            base_url: Some("http://127.0.0.1:3210".into()),
            sessions_without_task: SessionsWithoutTask::Hide,
        };
        settings.save_to(&file).unwrap();
        assert_eq!(TrackerSettings::load_from(&file), settings);
        std::fs::write(&file, r#"{"base_url":"http://127.0.0.1:1","colour":"teal"}"#).unwrap();
        assert_eq!(
            TrackerSettings::load_from(&file).sessions_without_task,
            SessionsWithoutTask::Show
        );
        std::fs::write(&file, "not json").unwrap();
        assert_eq!(TrackerSettings::load_from(&file), TrackerSettings::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_policy_close_with_the_trackers_reason_is_a_revocation() {
        let frame = |code, reason: &str| {
            Some(CloseFrame {
                code,
                reason: reason.into(),
            })
        };
        assert!(matches!(
            closed(frame(CloseCode::Policy, CREDENTIAL_REVOKED)),
            End::Revoked
        ));
        assert!(matches!(
            closed(frame(CloseCode::Policy, "something else")),
            End::Failed { connected: true, .. }
        ));
        assert!(matches!(closed(frame(CloseCode::Away, "")), End::Failed { .. }));
        assert!(matches!(closed(None), End::Failed { .. }));
    }
}
