//! The GitHub watcher: pull requests waiting on the user's review, through GitHub's GraphQL API
//! (`src/AiPet.Core/GitHub.cs`).
//!
//! The settings are `github.json` in the data folder ([`GitHubSettings`]); the token is `AiPet:GitHub` in the secret
//! store, one the user saved in Settings (no other credentials on the computer are used). Without one, GitHub reviews
//! are off. [`GitHubWatcher`] searches for `review-requested:@me` on a thread of its own, 4 s after each start (so the
//! first Jira poll has landed) and then every `PollSeconds`, but never more often than every 60 s.
//!
//! Each pull request is tagged with the Jira keys in its title, branch, description and commit messages, so the pet
//! can put it in the matching Jira issue's bubble. For a Jira issue with no review request, the watcher looks up the
//! pull request that mentions its key: in the settings' organisations, or else among the pull requests that involve
//! the user, never all of GitHub. It keeps each lookup for 10 minutes, a miss included.
//!
//! After each poll it sends [`Event::Changed`], and [`Event::NewReviews`] when review requests turned up that it
//! hadn't seen before (never on the first poll).

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

pub use crate::config::GitHubSettings;
use crate::config::write_dictionary;
pub use crate::http::Http;
use crate::http::{
    Element, Failure, NULL_REFERENCE, Poller, address, lock, parse_json, unix_seconds, upper_char, upper_invariant,
};
use crate::secrets::{self, SecretStore};

/// The first poll comes this long after a start, so the first Jira poll's keys can be looked up.
pub const FIRST_POLL: Duration = Duration::from_secs(4);
/// Polls are at least this far apart, whatever `PollSeconds` says.
pub const MIN_INTERVAL: Duration = Duration::from_secs(60);
/// How long a Jira key's pull request, or the lack of one, is kept.
const KEY_CACHE: Duration = Duration::from_secs(10 * 60);

/// The search for review requests, its variable, and the Settings page's Test (`PollAsync`, `TestAsync`).
const POLL_QUERY: &str = "query($q: String!) {\n  search(query: $q, type: ISSUE, first: 30) {\n    nodes { ... on \
                          PullRequest {\n      number title url body headRefName updatedAt\n      repository { \
                          nameWithOwner }\n      commits(last: 30) { nodes { commit { message } } }\n    } }\n  }\n}";
const REVIEW_REQUESTED: &str = "is:open is:pr review-requested:@me archived:false";
const TEST_QUERY: &str = "{ viewer { login }\n  search(query: \"is:open is:pr review-requested:@me archived:false\", \
                          type: ISSUE, first: 1) { issueCount } }";

/// A pull request waiting on the user's review (`GitHubWatcher.PullRequest`). A field GitHub sends as null reads as
/// empty.
#[derive(Clone, Debug, PartialEq)]
pub struct PullRequest {
    /// `owner/name`.
    pub repo: String,
    pub number: i32,
    pub title: String,
    pub url: String,
    /// When it last changed, in Unix seconds; 0 when GitHub sent no time the C# could read.
    pub updated: f64,
    /// The Jira keys it mentions, upper case, each once, in the order they come.
    pub keys: Vec<String>,
}

/// What the watcher tells its owner: the C#'s `Changed` and `NewReviews` events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A poll or a lookup is done: the review requests, the pull requests for Jira keys, or the error may have
    /// changed.
    Changed,
    /// This many review requests are ones the watcher hadn't seen before.
    NewReviews(usize),
}

/// How long the watcher waits between polls with these settings: `PollSeconds`, at least [`MIN_INTERVAL`].
pub fn interval(settings: &GitHubSettings) -> Duration {
    MIN_INTERVAL.max(Duration::from_secs(settings.poll_seconds.max(0).unsigned_abs().into()))
}

/// Supplies the Jira keys the pet shows, so their pull requests can be looked up.
type JiraKeys = Arc<dyn Fn() -> Vec<String> + Send + Sync>;

/// Watches GitHub for review requests (`GitHubWatcher`). It polls once [`GitHubWatcher::restart`] has started it,
/// and stops when it is dropped.
pub struct GitHubWatcher {
    shared: Arc<Shared>,
    poller: Poller,
}

struct Shared {
    file: PathBuf,
    secrets: Arc<dyn SecretStore>,
    http: Http,
    events: Box<dyn Fn(Event) + Send + Sync>,
    jira_keys: Mutex<JiraKeys>,
    /// Held through a lookup of Jira keys, so two don't ask for the same keys at once.
    lookup: Mutex<()>,
    state: Mutex<State>,
}

struct State {
    config: GitHubSettings,
    review_requests: Vec<PullRequest>,
    pr_for_key: HashMap<String, String>,
    last_error: Option<String>,
    /// The URLs of every review request so far.
    seen: HashSet<String>,
    /// Each Jira key looked up: its pull request, or none, and when.
    key_cache: HashMap<String, (Option<String>, Instant)>,
    first_poll: bool,
}

impl GitHubWatcher {
    /// A watcher with `github.json` from `data_dir` (the defaults when it can't be read), the token from `secrets`,
    /// requests through `http`, and its events sent to `events`. It knows no Jira keys until
    /// [`GitHubWatcher::set_jira_keys`], and doesn't poll until [`GitHubWatcher::restart`].
    pub fn new<E>(data_dir: &Path, secrets: Arc<dyn SecretStore>, http: Http, events: Sender<E>) -> GitHubWatcher
    where
        E: From<Event> + Send + 'static,
    {
        let file = data_dir.join("github.json");
        let (config, _) = GitHubSettings::load_from(&file);
        GitHubWatcher {
            shared: Arc::new(Shared {
                file,
                secrets,
                http,
                events: Box::new(move |event| {
                    // an owner that stopped listening misses nothing it wants
                    let _ = events.send(E::from(event));
                }),
                jira_keys: Mutex::new(Arc::new(Vec::new)),
                lookup: Mutex::new(()),
                state: Mutex::new(State {
                    config,
                    review_requests: Vec::new(),
                    pr_for_key: HashMap::new(),
                    last_error: None,
                    seen: HashSet::new(),
                    key_cache: HashMap::new(),
                    first_poll: true,
                }),
            }),
            poller: Poller::default(),
        }
    }

    pub fn config(&self) -> GitHubSettings {
        self.shared.lock().config.clone()
    }

    /// What the last search that worked found (none while GitHub reviews are off).
    pub fn review_requests(&self) -> Vec<PullRequest> {
        self.shared.lock().review_requests.clone()
    }

    /// Jira key → the URL of the pull request that mentions it: a review request's, else the one a lookup found
    /// (open ones first, then the newest).
    pub fn pr_for_key(&self) -> HashMap<String, String> {
        self.shared.lock().pr_for_key.clone()
    }

    /// What went wrong in the last poll, if it failed.
    pub fn last_error(&self) -> Option<String> {
        self.shared.lock().last_error.clone()
    }

    /// Whether a token is saved.
    pub fn has_saved_token(&self) -> bool {
        self.shared.token().is_some()
    }

    /// Where the Jira keys the pet shows come from (`JiraKeys`), for the pull requests the watcher looks up and the
    /// projects it recognises in pull requests.
    pub fn set_jira_keys(&self, keys: impl Fn() -> Vec<String> + Send + Sync + 'static) {
        *lock(&self.shared.jira_keys) = Arc::new(keys);
    }

    /// Saves the settings, and the token (trimmed) when one is given, then starts again with nothing seen or looked
    /// up. A failed write comes back after the settings are in use, as for the C#; so does a failed secret, except
    /// on Windows, where the C# ignores a failed Credential Manager write.
    pub fn save(&self, settings: GitHubSettings, new_token: &str) -> io::Result<()> {
        self.shared.lock().config = settings.clone();
        settings.save_to(&self.shared.file)?;
        if !new_token.is_empty() {
            let written = self.shared.secrets.write(secrets::GITHUB, "github", new_token.trim());
            if !cfg!(windows) {
                written?;
            }
        }
        self.shared.forget_seen();
        self.restart();
        Ok(())
    }

    /// Removes the saved token; GitHub reviews are off until a new one is saved.
    pub fn forget_token(&self) {
        self.shared.secrets.delete(secrets::GITHUB);
        self.shared.forget_seen();
        self.restart();
    }

    /// Starts polling again: the first poll after [`FIRST_POLL`], then one every [`interval`].
    pub fn restart(&self) {
        let shared = Arc::clone(&self.shared);
        self.poller.restart("aipet-github", FIRST_POLL, move || {
            shared.poll();
            interval(&shared.lock().config)
        });
    }

    /// One poll, now, on this thread, as the loop polls: for tests, which can't wait the loop's 60 s.
    #[doc(hidden)]
    pub fn poll(&self) {
        self.shared.poll();
    }

    /// Looks the Jira keys' pull requests up again on a thread of its own, then sends [`Event::Changed`] (the pet
    /// calls it when the Jira issues change).
    pub fn jira_changed(&self) {
        let shared = Arc::clone(&self.shared);
        let _ = thread::Builder::new().name("aipet-github-keys".into()).spawn(move || {
            shared.lookup_jira_keys();
            (shared.events)(Event::Changed);
        });
    }

    /// Checks a host and token without saving anything (the Settings page's Test): who they sign in as, and how
    /// many pull requests wait on that account's review. The saved token stands in when none is typed.
    pub fn test(&self, host: &str, typed_token: &str) -> Result<String, String> {
        let token = match typed_token.trim() {
            "" => self.shared.token().ok_or("Paste an access token first.")?,
            typed => typed.into(),
        };
        let doc = graphql(&self.shared.http, Some(host), &token, TEST_QUERY, None)?;
        let read = || -> Result<String, String> {
            let data = Element(&doc).get("data")?;
            let login = data.get("viewer")?.get("login")?.string()?.unwrap_or("");
            let waiting = data.get("search")?.get("issueCount")?.int32()?;
            Ok(format!(
                "Connected as {login}. {waiting} pull request(s) waiting on your review."
            ))
        };
        read().map_err(|message| format!("Couldn't read GitHub's answer: {message}"))
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    fn token(&self) -> Option<String> {
        self.secrets.read(secrets::GITHUB).filter(|token| !token.is_empty())
    }

    fn jira_keys(&self) -> Vec<String> {
        let keys = Arc::clone(&lock(&self.jira_keys));
        keys()
    }

    fn forget_seen(&self) {
        let mut state = self.lock();
        state.first_poll = true;
        state.seen.clear();
        state.key_cache.clear();
    }

    /// `PollAsync`: one search for review requests when GitHub reviews are on and a token is saved, then the lookup
    /// of the Jira keys' pull requests.
    fn poll(&self) {
        let config = self.lock().config.clone();
        let Some(token) = self.token().filter(|_| config.enabled) else {
            {
                let mut state = self.lock();
                state.review_requests.clear();
                state.last_error = None;
            }
            (self.events)(Event::Changed);
            return;
        };
        let answer = graphql(
            &self.http,
            config.host.as_deref(),
            &token,
            POLL_QUERY,
            Some(REVIEW_REQUESTED),
        );
        let mut fresh = 0;
        match answer {
            Err(error) => self.lock().last_error = Some(error),
            Ok(doc) => {
                let read = review_requests(&doc, &config, &self.jira_keys());
                let mut state = self.lock();
                match read {
                    Ok(list) => {
                        state.last_error = None;
                        let new = list.iter().filter(|pr| !state.seen.contains(&pr.url)).count();
                        state.seen.extend(list.iter().map(|pr| pr.url.clone()));
                        state.review_requests = list;
                        if !state.first_poll {
                            fresh = new;
                        }
                        state.first_poll = false;
                    }
                    Err(message) => state.last_error = Some(format!("Couldn't read GitHub's answer: {message}")),
                }
            }
        }
        if fresh > 0 {
            (self.events)(Event::NewReviews(fresh));
        }
        self.lookup_jira_keys();
        (self.events)(Event::Changed);
    }

    /// `LookupJiraKeysAsync`: for the Jira keys no review request mentions, the pull request that does. The keys not
    /// looked up in the last 10 minutes go in one GraphQL query, one aliased search each.
    fn lookup_jira_keys(&self) {
        let _one_at_a_time = lock(&self.lookup);
        let config = self.lock().config.clone();
        let Some(token) = self.token().filter(|_| config.enabled) else {
            self.lock().pr_for_key.clear();
            return;
        };
        let jira_keys = self.jira_keys();
        let (mut map, missing, to_fetch) = {
            let state = self.lock();
            let mut map = HashMap::new();
            for pr in &state.review_requests {
                for key in &pr.keys {
                    map.entry(key.clone()).or_insert_with(|| pr.url.clone());
                }
            }
            let mut missing: Vec<String> = Vec::new();
            for key in jira_keys {
                if !map.contains_key(&key) && !missing.contains(&key) {
                    missing.push(key);
                }
            }
            let now = Instant::now();
            let to_fetch: Vec<String> = missing
                .iter()
                .filter(|key| {
                    state
                        .key_cache
                        .get(*key)
                        .is_none_or(|(_, at)| now.duration_since(*at) > KEY_CACHE)
                })
                .cloned()
                .collect();
            (map, missing, to_fetch)
        };
        if !to_fetch.is_empty() {
            let query = lookup_query(&config, &to_fetch);
            // a failed lookup changes nothing: the keys keep what they had, and are asked for again next time
            if let Ok(doc) = graphql(&self.http, config.host.as_deref(), &token, &query, None) {
                let found = pr_urls(&doc, to_fetch.len());
                let now = Instant::now();
                let mut state = self.lock();
                for (key, url) in to_fetch.into_iter().zip(found) {
                    state.key_cache.insert(key, (url, now));
                }
            }
        }
        let mut state = self.lock();
        for key in missing {
            if let Some((Some(url), _)) = state.key_cache.get(&key) {
                map.insert(key, url.clone());
            }
        }
        state.pr_for_key = map;
    }
}

/// POSTs a GraphQL query to the host's API (`GraphQLAsync`): the answer's JSON, or the error text to show.
fn graphql(http: &Http, host: Option<&str>, token: &str, query: &str, search: Option<&str>) -> Result<Value, String> {
    let host = match host {
        Some(host) if !host.trim().is_empty() => address(host),
        _ => "github.com".into(),
    };
    let endpoint = if host == "github.com" {
        format!("{}://api.github.com/graphql", http.scheme())
    } else {
        format!("{}://{host}/api/graphql", http.scheme())
    };
    let authorization = format!("Bearer {token}");
    let headers = [
        ("Authorization", authorization.as_str()),
        ("User-Agent", "AiPet/1.0"),
        ("Content-Type", "application/json; charset=utf-8"),
    ];
    let answer = http
        .post(&endpoint, &headers, &request_body(query, search))
        .map_err(|failure| match failure {
            Failure::Timeout => "GitHub didn't answer in time".into(),
            Failure::Unreachable(message) => format!("Couldn't reach GitHub: {message}"),
            Failure::Other(message) => format!("GitHub search failed: {message}"),
        })?;
    if answer.status == 401 {
        return Err("GitHub didn't accept the token".into());
    }
    if !answer.success() {
        return Err(format!("GitHub returned {}", answer.status_line()));
    }
    let failed = |message: String| format!("GitHub search failed: {message}");
    let doc = parse_json(&answer.body).map_err(failed)?;
    let root = Element(&doc);
    if let Some(errors) = root.try_get("errors").map_err(failed)? {
        let errors = errors.array().map_err(failed)?;
        if !errors.is_empty() && root.try_get("data").map_err(failed)?.is_none() {
            let message = Element(&errors[0])
                .get("message")
                .and_then(|m| m.string())
                .map_err(failed)?;
            return Err(failed(message.unwrap_or("").into()));
        }
    }
    Ok(doc)
}

/// `new JsonObject { ["query"] = query, ["variables"] = new JsonObject { ["q"] = search } }.ToJsonString()`, with
/// System.Text.Json's escaping.
fn request_body(query: &str, search: Option<&str>) -> String {
    let body = write_dictionary(&[("query".into(), Some(query.into()))]);
    match search {
        None => body,
        Some(search) => {
            let variables = write_dictionary(&[("q".into(), Some(search.into()))]);
            format!("{},\"variables\":{variables}}}", &body[..body.len() - 1])
        }
    }
}

/// The review requests in the search's answer, each with the Jira keys it mentions.
fn review_requests(doc: &Value, config: &GitHubSettings, jira_keys: &[String]) -> Result<Vec<PullRequest>, String> {
    let nodes = Element(doc).get("data")?.get("search")?.get("nodes")?.array()?;
    let mut projects: Option<Vec<String>> = None;
    let mut list = Vec::new();
    for node in nodes {
        let node = Element(node);
        let Some(number) = node.try_get("number")? else {
            continue;
        };
        let mut text = String::new();
        for field in ["title", "headRefName", "body"] {
            text.push_str(&node.text(field)?);
            text.push('\n');
        }
        if let Some(commits) = node.try_get("commits")? {
            for commit in commits.get("nodes")?.array()? {
                text.push_str(Element(commit).get("commit")?.get("message")?.string()?.unwrap_or(""));
                text.push('\n');
            }
        }
        let updated = unix_seconds(&node.text("updatedAt")?).unwrap_or(0.0);
        let repo = node
            .get("repository")?
            .get("nameWithOwner")?
            .string()?
            .unwrap_or("")
            .into();
        let number = number.int32()?;
        let (title, url) = (node.text("title")?, node.text("url")?);
        if projects.is_none() {
            projects = Some(key_projects(config, jira_keys)?);
        }
        let keys = find_keys(&text, projects.as_deref().unwrap_or_default());
        list.push(PullRequest {
            repo,
            number,
            title,
            url,
            updated,
            keys,
        });
    }
    Ok(list)
}

/// The Jira projects whose keys are recognised: the settings' `JiraProjects`, then those of the Jira keys the pet
/// shows, trimmed, in upper case, each once. A null in the settings' list fails as it does in the C#.
fn key_projects(config: &GitHubSettings, jira_keys: &[String]) -> Result<Vec<String>, String> {
    let configured = config.jira_projects.iter().flatten();
    let from_settings = configured.map(|p| p.as_deref().ok_or(NULL_REFERENCE));
    let from_keys = jira_keys.iter().map(|k| Ok(k.split('-').next().unwrap_or("")));
    let mut projects: Vec<String> = Vec::new();
    for project in from_settings.chain(from_keys) {
        let project = upper_invariant(project?.trim());
        if !project.is_empty() && !projects.contains(&project) {
            projects.push(project);
        }
    }
    Ok(projects)
}

/// `FindKeys`: the C#'s `(?<![A-Z0-9])(ABC|DEF)-(\d+)`, case-insensitive: each project, not right after a letter or
/// digit, then a dash and digits. The first project that makes a key at a place wins, the next key is looked for
/// after it, and each key is kept once, in upper case. Letters and digits are ASCII here; .NET's regex also takes
/// the Kelvin sign for a K and other scripts' decimal digits, which no Jira key has.
fn find_keys(text: &str, projects: &[String]) -> Vec<String> {
    if projects.is_empty() {
        return Vec::new();
    }
    let text: Vec<char> = text.chars().collect();
    let projects: Vec<Vec<char>> = projects.iter().map(|p| p.chars().collect()).collect();
    let mut keys: Vec<String> = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let after_key_character = i > 0 && text[i - 1].is_ascii_alphanumeric();
        let found = if after_key_character {
            None
        } else {
            projects.iter().find_map(|project| key_at(&text, i, project))
        };
        let Some((dash, end)) = found else {
            i += 1;
            continue;
        };
        let project: String = text[i..dash].iter().collect();
        let number: String = text[dash + 1..end].iter().collect();
        let key = format!("{}-{number}", upper_invariant(&project));
        if !keys.contains(&key) {
            keys.push(key);
        }
        i = end;
    }
    keys
}

/// Where `project` (in upper case) makes a key at `start`: the dash's place and the end of the digits.
fn key_at(text: &[char], start: usize, project: &[char]) -> Option<(usize, usize)> {
    let dash = start + project.len();
    let same = text
        .get(start..dash)?
        .iter()
        .zip(project)
        .all(|(&c, &p)| upper_char(c) == p);
    if !same || text.get(dash) != Some(&'-') {
        return None;
    }
    let digits = text[dash + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
    (digits > 0).then_some((dash, dash + 1 + digits))
}

/// The lookup's query: for each key, an aliased search for pull requests that mention it, in the settings'
/// organisations, or among those that involve the user when there are none.
fn lookup_query(config: &GitHubSettings, keys: &[String]) -> String {
    let orgs: Vec<String> = config
        .orgs
        .iter()
        .flatten()
        .flatten()
        .filter(|org| !org.trim().is_empty())
        .map(|org| format!("org:{}", org.trim()))
        .collect();
    let scope = if orgs.is_empty() {
        "involves:@me".into()
    } else {
        orgs.join(" ")
    };
    let searches: Vec<String> = keys
        .iter()
        .enumerate()
        .map(|(i, key)| {
            format!(
                "k{i}: search(query: \"is:pr {key} in:title,body {scope}\", type: ISSUE, first: 5) {{ nodes {{ ... on \
                 PullRequest {{ url state updatedAt }} }} }}"
            )
        })
        .collect();
    format!("{{ {} }}", searches.join(" "))
}

/// Each looked-up key's pull request, in the order asked, as far as the answer can be read (the C# keeps what it
/// read before an error).
fn pr_urls(doc: &Value, count: usize) -> Vec<Option<String>> {
    let Ok(data) = Element(doc).get("data") else {
        return Vec::new();
    };
    (0..count).map_while(|i| pr_url(data, i).ok()).collect()
}

/// The pull request the search `k{i}` found: an open one first, then the newest.
fn pr_url(data: Element, i: usize) -> Result<Option<String>, String> {
    let Some(found) = data.try_get(&format!("k{i}"))? else {
        return Ok(None);
    };
    let mut candidates = Vec::new();
    for node in found.get("nodes")?.array()? {
        let node = Element(node);
        if node.try_get("url")?.is_some() {
            candidates.push((
                node.text("state")? == "OPEN",
                node.text("updatedAt")?,
                node.text("url")?,
            ));
        }
    }
    // `OrderBy(open first).ThenByDescending(updatedAt).FirstOrDefault()`: the first of the best, as the sort is stable
    let best = candidates
        .into_iter()
        .min_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    Ok(best.map(|(_, _, url)| url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polls_are_at_least_60_seconds_apart() {
        let every = |poll_seconds| {
            interval(&GitHubSettings {
                poll_seconds,
                ..GitHubSettings::default()
            })
        };
        assert_eq!(every(120), Duration::from_secs(120));
        assert_eq!(every(61), Duration::from_secs(61));
        for below in [60, 59, 1, 0, -5, i32::MIN] {
            assert_eq!(every(below), Duration::from_secs(60), "{below}");
        }
    }

    #[test]
    fn keys_are_found_as_the_csharps_regex_finds_them() {
        let projects = |p: &[&str]| p.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for (text, found) in [
            ("ABC-12 fixes abc-7 and ABC-12 again", vec!["ABC-12", "ABC-7"]),
            // not right after a letter or digit, but after anything else
            (
                "XABC-1 1ABC-2 _ABC-3 feature/abc-4-login (ABC-5)",
                vec!["ABC-3", "ABC-4", "ABC-5"],
            ),
            // digits are needed, and all of them are taken
            ("ABC- ABC-x ABC-123456x", vec!["ABC-123456"]),
            // a key right after a key's digits isn't one
            ("ABC-1ABC-2", vec!["ABC-1"]),
            // the first project that makes a key wins, the next one is tried when it doesn't
            ("AB-1 ABC-2", vec!["AB-1", "ABC-2"]),
            ("DEF-9\nabc-10", vec!["DEF-9", "ABC-10"]),
            ("nothing here", vec![]),
        ] {
            assert_eq!(find_keys(text, &projects(&["AB", "ABC", "DEF"])), found, "{text}");
        }
        assert_eq!(find_keys("ABC-1", &[]), Vec::<String>::new());
    }

    #[test]
    fn projects_come_from_the_settings_then_the_jira_keys() {
        let config = GitHubSettings {
            jira_projects: Some(vec![Some(" abc ".into()), Some(String::new()), Some("DEF".into())]),
            ..GitHubSettings::default()
        };
        let keys = ["DEF-1".to_string(), "xyz-2".into(), "NOKEY".into()];
        assert_eq!(key_projects(&config, &keys).unwrap(), ["ABC", "DEF", "XYZ", "NOKEY"]);
        let null = GitHubSettings {
            jira_projects: Some(vec![None]),
            ..GitHubSettings::default()
        };
        assert_eq!(key_projects(&null, &[]).unwrap_err(), NULL_REFERENCE);
    }

    #[test]
    fn the_lookup_prefers_an_open_pull_request_then_the_newest() {
        let doc: Value = serde_json::from_str(
            r#"{"data":{
                "k0":{"nodes":[{"url":"u1","state":"MERGED","updatedAt":"2026-09-03T00:00:00Z"},
                               {"url":"u2","state":"OPEN","updatedAt":"2026-09-01T00:00:00Z"},
                               {"url":"u3","state":"OPEN","updatedAt":"2026-09-02T00:00:00Z"},
                               {"url":"u4","state":"OPEN","updatedAt":"2026-09-02T00:00:00Z"}]},
                "k1":{"nodes":[{}]},
                "k2":{"nodes":[{"url":"u5","state":"CLOSED"}]},
                "k4":{"nodes":[1]},
                "k5":{"nodes":[{"url":"u6"}]}}}"#,
        )
        .unwrap();
        // k3 isn't in the answer; k4 can't be read, so the C# stops there
        assert_eq!(
            pr_urls(&doc, 6),
            [Some("u3".to_string()), None, Some("u5".into()), None]
        );
    }
}
