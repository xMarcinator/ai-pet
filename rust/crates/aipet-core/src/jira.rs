//! The Jira watcher: the issues a JQL search finds on the user's Jira Cloud site, by default their own open ones
//! (`src/AiPet.Core/Jira.cs`).
//!
//! The settings are `jira.json` in the data folder ([`JiraSettings`]); the API token is `AiPet:Jira` in the secret
//! store. [`search`] is the C#'s `SearchAsync`: its request, and the error texts the pet's Jira bubble and the
//! Settings page show. [`JiraWatcher`] runs it on a thread of its own, 1.5 s after each start and then every
//! `PollSeconds`, but never more often than every 30 s. After each poll it sends [`Event::Changed`], and
//! [`Event::NewReviews`] when the search found issues it hadn't found before (never on the first poll, so an alert
//! means something new).

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

pub use crate::config::{DEFAULT_JQL, JiraSettings};
pub use crate::http::Http;
use crate::http::{Element, Failure, Poller, address, base64, escape_data_string, lock, parse_json, unix_seconds};
use crate::secrets::{self, SecretStore};

/// The first poll comes this long after a start.
pub const FIRST_POLL: Duration = Duration::from_millis(1500);
/// Polls are at least this far apart, whatever `PollSeconds` says.
pub const MIN_INTERVAL: Duration = Duration::from_secs(30);

/// An issue the search found (`JiraWatcher.Issue`). A field Jira sends as null reads as empty.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub key: String,
    pub summary: String,
    /// The status's name.
    pub status: String,
    /// When it last changed, in Unix seconds; 0 when Jira sent no time the C# could read.
    pub updated: f64,
    /// Its page: `https://{site}/browse/{key}`.
    pub url: String,
}

/// What the watcher tells its owner: the C#'s `Changed` and `NewReviews` events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A poll is done: the issues, or the error, may have changed.
    Changed,
    /// This many issues are ones the search hadn't found before.
    NewReviews(usize),
}

/// How long the watcher waits between polls with these settings: `PollSeconds`, at least [`MIN_INTERVAL`].
pub fn interval(settings: &JiraSettings) -> Duration {
    MIN_INTERVAL.max(Duration::from_secs(settings.poll_seconds.max(0).unsigned_abs().into()))
}

/// Runs the settings' JQL search with this token (`SearchAsync`, which the Settings page's Test uses too): the
/// issues, or the error text to show.
pub fn search(http: &Http, settings: &JiraSettings, token: &str) -> Result<Vec<Issue>, String> {
    let site = settings.site.as_deref().unwrap_or("");
    if site.trim().is_empty() {
        return Err("Enter your Jira site first (e.g. your-team.atlassian.net)".into());
    }
    let site = address(site);
    let jql = match settings.jql.as_deref() {
        Some(jql) if !jql.trim().is_empty() => jql,
        _ => DEFAULT_JQL,
    };
    let url = format!(
        "{}://{site}/rest/api/3/search/jql?jql={}&fields=summary,status,updated&maxResults=25",
        http.scheme(),
        escape_data_string(jql)
    );
    // the poll needs an email and Settings always sends one, so the C#'s null can't come here
    let email = settings.email.as_deref().unwrap_or("").trim();
    let authorization = format!("Basic {}", base64(format!("{email}:{token}").as_bytes()));
    let answer = http
        .get(
            &url,
            &[("Authorization", &authorization), ("Accept", "application/json")],
        )
        .map_err(|failure| match failure {
            Failure::Timeout => "Jira didn't answer in time".into(),
            Failure::Unreachable(message) => format!("Couldn't reach Jira: {message}"),
            Failure::Other(message) => format!("Jira search failed: {message}"),
        })?;
    if answer.status == 401 {
        return Err("Jira didn't accept the email/API token".into());
    }
    if !answer.success() {
        return Err(jira_error(&answer.body).unwrap_or_else(|| format!("Jira returned {}", answer.status_line())));
    }
    issues(&answer.body, &site).map_err(|message| format!("Jira search failed: {message}"))
}

/// The first of the `errorMessages` Jira sends with a failure, if it sent one.
fn jira_error(body: &str) -> Option<String> {
    let doc = parse_json(body).ok()?;
    let first = Element(&doc).try_get("errorMessages").ok()??.array().ok()?.first()?;
    Element(first).string().ok()?.map(str::to_owned)
}

fn issues(body: &str, site: &str) -> Result<Vec<Issue>, String> {
    let doc = parse_json(body)?;
    let Some(found) = Element(&doc).try_get("issues")? else {
        return Ok(Vec::new());
    };
    let mut issues = Vec::new();
    for item in found.array()? {
        let item = Element(item);
        let key = item.get("key")?.string()?.unwrap_or("").to_owned();
        let fields = item.get("fields")?;
        let summary = match fields.try_get("summary")? {
            Some(summary) => summary.string()?.unwrap_or(""),
            None => "",
        };
        let status = match fields.try_get("status")? {
            Some(status) => match status.try_get("name")? {
                Some(name) => name.string()?.unwrap_or(""),
                None => "",
            },
            None => "",
        };
        let updated = match fields.try_get("updated")? {
            Some(updated) => updated.string()?.and_then(unix_seconds).unwrap_or(0.0),
            None => 0.0,
        };
        issues.push(Issue {
            url: format!("https://{site}/browse/{key}"),
            key,
            summary: summary.into(),
            status: status.into(),
            updated,
        });
    }
    Ok(issues)
}

/// Watches the Jira search (`JiraWatcher`). It polls once [`JiraWatcher::restart`] has started it, and stops when it
/// is dropped.
pub struct JiraWatcher {
    shared: Arc<Shared>,
    poller: Poller,
}

struct Shared {
    file: PathBuf,
    secrets: Arc<dyn SecretStore>,
    http: Http,
    events: Box<dyn Fn(Event) + Send + Sync>,
    state: Mutex<State>,
}

struct State {
    config: JiraSettings,
    issues: Vec<Issue>,
    last_error: Option<String>,
    /// The keys every search so far has found.
    seen: HashSet<String>,
    first_poll: bool,
}

impl JiraWatcher {
    /// A watcher with `jira.json` from `data_dir` (the defaults when it can't be read), tokens from `secrets`,
    /// requests through `http`, and its events sent to `events`. It doesn't poll until [`JiraWatcher::restart`].
    pub fn new<E>(data_dir: &Path, secrets: Arc<dyn SecretStore>, http: Http, events: Sender<E>) -> JiraWatcher
    where
        E: From<Event> + Send + 'static,
    {
        let file = data_dir.join("jira.json");
        let (config, _) = JiraSettings::load_from(&file);
        JiraWatcher {
            shared: Arc::new(Shared {
                file,
                secrets,
                http,
                events: Box::new(move |event| {
                    // an owner that stopped listening misses nothing it wants
                    let _ = events.send(E::from(event));
                }),
                state: Mutex::new(State {
                    config,
                    issues: Vec::new(),
                    last_error: None,
                    seen: HashSet::new(),
                    first_poll: true,
                }),
            }),
            poller: Poller::default(),
        }
    }

    pub fn config(&self) -> JiraSettings {
        self.shared.lock().config.clone()
    }

    /// What the last search that worked found (none while the watcher isn't set up).
    pub fn issues(&self) -> Vec<Issue> {
        self.shared.lock().issues.clone()
    }

    /// What went wrong in the last poll, if it failed.
    pub fn last_error(&self) -> Option<String> {
        self.shared.lock().last_error.clone()
    }

    /// Whether an API token is saved.
    pub fn has_token(&self) -> bool {
        self.shared.token().is_some()
    }

    /// Saves the settings, and the token when one is given, then starts again with nothing seen. A failed write
    /// comes back after the settings are in use, as for the C#; so does a failed secret, except on Windows, where the
    /// C# ignores a failed Credential Manager write.
    pub fn save(&self, settings: JiraSettings, new_token: &str) -> io::Result<()> {
        self.shared.lock().config = settings.clone();
        settings.save_to(&self.shared.file)?;
        if !new_token.is_empty() {
            let email = settings.email.as_deref().unwrap_or("");
            let written = self.shared.secrets.write(secrets::JIRA, email, new_token);
            if !cfg!(windows) {
                written?;
            }
        }
        {
            let mut state = self.shared.lock();
            state.first_poll = true;
            state.seen.clear();
        }
        self.restart();
        Ok(())
    }

    /// Removes the saved API token; the watcher is off until a new one is saved.
    pub fn forget_token(&self) {
        self.shared.secrets.delete(secrets::JIRA);
        self.restart();
    }

    /// Starts polling again: the first poll after [`FIRST_POLL`], then one every [`interval`].
    pub fn restart(&self) {
        let shared = Arc::clone(&self.shared);
        self.poller.restart("aipet-jira", FIRST_POLL, move || {
            shared.poll();
            interval(&shared.lock().config)
        });
    }

    /// One poll, now, on this thread, as the loop polls: for tests, which can't wait the loop's 30 s.
    #[doc(hidden)]
    pub fn poll(&self) {
        self.shared.poll();
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    fn token(&self) -> Option<String> {
        self.secrets.read(secrets::JIRA).filter(|token| !token.is_empty())
    }

    /// `PollAsync`: one search, when the watcher is set up (on, with a site, an email and a token).
    fn poll(&self) {
        let config = self.lock().config.clone();
        let blank = |s: &Option<String>| s.as_deref().is_none_or(|s| s.trim().is_empty());
        let token = self
            .token()
            .filter(|_| config.enabled && !blank(&config.site) && !blank(&config.email));
        let Some(token) = token else {
            {
                let mut state = self.lock();
                state.issues.clear();
                state.last_error = None;
            }
            (self.events)(Event::Changed);
            return;
        };
        let found = search(&self.http, &config, &token);
        let fresh = {
            let mut state = self.lock();
            match found {
                Ok(issues) => {
                    state.last_error = None;
                    let fresh = issues.iter().filter(|i| !state.seen.contains(&i.key)).count();
                    state.seen.extend(issues.iter().map(|i| i.key.clone()));
                    state.issues = issues;
                    let alert = !state.first_poll;
                    state.first_poll = false;
                    if alert { fresh } else { 0 }
                }
                Err(error) => {
                    state.last_error = Some(error);
                    0
                }
            }
        };
        if fresh > 0 {
            (self.events)(Event::NewReviews(fresh));
        }
        (self.events)(Event::Changed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polls_are_at_least_30_seconds_apart() {
        let every = |poll_seconds| {
            interval(&JiraSettings {
                poll_seconds,
                ..JiraSettings::default()
            })
        };
        assert_eq!(every(120), Duration::from_secs(120));
        assert_eq!(every(31), Duration::from_secs(31));
        for below in [30, 29, 1, 0, -5, i32::MIN] {
            assert_eq!(every(below), Duration::from_secs(30), "{below}");
        }
        assert_eq!(every(i32::MAX), Duration::from_secs(i32::MAX as u64));
    }
}
