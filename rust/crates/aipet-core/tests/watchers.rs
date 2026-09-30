//! The Jira and GitHub watchers against a stub server on this computer: the requests (URL, method, headers, body),
//! what is read from the answers, the error texts, the minimum intervals and the new-review alerts.
//!
//! The expected requests and texts are what the C# (`src/AiPet.Core/Jira.cs`, `GitHub.cs`) sent and said against
//! the same kind of stub, recorded with .NET 10: only the headers it sets (no User-Agent, Accept or Accept-Encoding
//! of HttpClient's own), `Uri.EscapeDataString`'s escaping, System.Text.Json's for the GraphQL body. The C# always
//! uses `https://`; the stub speaks plain HTTP through `Http::local`, which reaches this computer only, so no test can
//! reach the real Jira or GitHub. Tokens are kept in memory and the settings in a folder of the test's own, never the
//! user's.

use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use aipet_core::config::{GitHubSettings, JiraSettings};
use aipet_core::github::{self, GitHubWatcher, PullRequest};
use aipet_core::jira::{self, Http, Issue, JiraWatcher};
use aipet_core::secrets::{GITHUB, JIRA, MemorySecrets, SecretStore};

// ---------------------------------------------------------------------------------------------------------------
// The stub

/// A request as the stub read it; header names in lower case, sorted.
#[derive(Clone, Debug)]
struct Request {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: String,
}

/// How the stub answers.
enum Reply {
    /// A status, its reason phrase and a body.
    Answer(u16, &'static str, String),
    /// Nothing, for longer than the client waits.
    Silence,
}

fn ok(body: &str) -> Reply {
    Reply::Answer(200, "OK", body.into())
}

/// A server on this computer that answers each request with the next of its replies (the last one again when
/// they run out), and records the requests with when they came.
struct Stub {
    port: u16,
    requests: Receiver<(Instant, Request)>,
}

impl Stub {
    fn new(replies: Vec<Reply>) -> Stub {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (sender, requests) = mpsc::channel();
        let replies = Arc::new(Mutex::new(replies));
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (sender, replies) = (sender.clone(), Arc::clone(&replies));
                thread::spawn(move || serve(stream, &sender, &replies));
            }
        });
        Stub { port, requests }
    }

    /// The address a watcher is given: `127.0.0.1:port`.
    fn site(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    /// The next request, if one comes within `wait`.
    fn next(&self, wait: Duration) -> Option<(Instant, Request)> {
        self.requests.recv_timeout(wait).ok()
    }

    fn request(&self) -> Request {
        self.next(Duration::from_secs(10)).expect("a request").1
    }

    fn no_more_requests(&self) {
        assert!(self.requests.try_recv().is_err(), "no other request");
    }

    /// Forgets the requests so far.
    fn drain(&self) {
        while self.requests.try_recv().is_ok() {}
    }
}

fn serve(stream: TcpStream, sender: &Sender<(Instant, Request)>, replies: &Mutex<Vec<Reply>>) {
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);
    // requests keep coming on a connection until the client closes it
    while let Some(request) = read_request(&mut reader) {
        let reply = {
            let mut replies = replies.lock().unwrap();
            if replies.len() > 1 {
                replies.remove(0)
            } else {
                clone(&replies[0])
            }
        };
        let _ = sender.send((Instant::now(), request));
        match reply {
            Reply::Answer(status, reason, body) => {
                let head = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\n\
                     Content-Length: {}\r\n\r\n",
                    body.len()
                );
                if writer
                    .write_all(head.as_bytes())
                    .and_then(|_| writer.write_all(body.as_bytes()))
                    .is_err()
                {
                    return;
                }
            }
            Reply::Silence => {
                thread::sleep(Duration::from_secs(5));
                return;
            }
        }
    }
}

fn clone(reply: &Reply) -> Reply {
    match reply {
        Reply::Answer(status, reason, body) => Reply::Answer(*status, reason, body.clone()),
        Reply::Silence => Reply::Silence,
    }
}

fn read_request(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut parts = line.trim_end().splitn(3, ' ');
    let (method, target) = (parts.next()?.to_string(), parts.next()?.to_string());
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':')?;
        headers.push((name.to_lowercase(), value.trim().to_string()));
    }
    headers.sort();
    let length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .map_or(0, |(_, value)| value.parse().unwrap());
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        target,
        headers,
        body: String::from_utf8(body).unwrap(),
    })
}

/// The headers expected, sorted as the stub sorts them.
fn headers(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = expected.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect();
    headers.sort();
    headers
}

/// A client for the stub that gives up quickly.
fn quick() -> Http {
    Http::local(Duration::from_secs(10))
}

/// A port nobody listens on, and the OS's words for a connection to it being refused, as .NET words them.
fn closed_port() -> (u16, String) {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    #[cfg(windows)]
    let refused = 10061;
    #[cfg(target_os = "macos")]
    let refused = 61;
    #[cfg(not(any(windows, target_os = "macos")))]
    let refused = 111;
    let text = io::Error::from_raw_os_error(refused).to_string();
    let text = text
        .strip_suffix(&format!(" (os error {refused})"))
        .unwrap()
        .to_string();
    (port, text)
}

/// A folder of the test's own under the temp folder, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("aipet-core-watchers-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn secrets(saved: &[(&str, &str)]) -> Arc<MemorySecrets> {
    let store = Arc::new(MemorySecrets::default());
    for (key, token) in saved {
        store.write(key, "user", token).unwrap();
    }
    store
}

// ---------------------------------------------------------------------------------------------------------------
// Jira

fn jira_settings(site: &str) -> JiraSettings {
    JiraSettings {
        enabled: true,
        site: Some(site.into()),
        email: Some("me@example.com".into()),
        ..JiraSettings::default()
    }
}

#[test]
fn jira_sends_the_csharps_request_and_reads_the_issues() {
    let stub = Stub::new(vec![ok(r#"{"issues":[
        {"key":"ABC-1","fields":{"summary":"Fix the login","status":{"name":"In Review"},
                                 "updated":"2026-09-29T14:03:12.345+0200"}},
        {"key":"ABC-2","fields":{"summary":null,"updated":null}},
        {"key":"ABC-3","fields":{"status":{},"updated":"not a time"}}]}"#)]);
    let settings = JiraSettings {
        site: Some(format!(" https://{}/ ", stub.site())),
        email: Some(" me@example.com ".into()),
        ..jira_settings("")
    };
    let issues = jira::search(&quick(), &settings, "t0k3n").unwrap();

    let request = stub.request();
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.target,
        "/rest/api/3/search/jql?jql=assignee%20%3D%20currentUser%28%29%20AND%20statusCategory%20%21%3D%20Done%20\
         ORDER%20BY%20updated%20DESC&fields=summary,status,updated&maxResults=25"
    );
    let host = stub.site();
    let expected = [
        ("host", host.as_str()),
        ("authorization", "Basic bWVAZXhhbXBsZS5jb206dDBrM24="),
        ("accept", "application/json"),
    ];
    assert_eq!(request.headers, headers(&expected));
    assert_eq!(request.body, "");

    let browse = |key: &str| format!("https://{}/browse/{key}", stub.site());
    let issue = |key: &str, summary: &str, status: &str, updated| Issue {
        key: key.into(),
        summary: summary.into(),
        status: status.into(),
        updated,
        url: browse(key),
    };
    assert_eq!(
        issues,
        [
            issue("ABC-1", "Fix the login", "In Review", 1790683392.345),
            issue("ABC-2", "", "", 0.0),
            issue("ABC-3", "", "", 0.0),
        ]
    );
}

#[test]
fn jira_escapes_the_search_as_the_csharp_does() {
    let stub = Stub::new(vec![ok(r#"{"issues":[]}"#)]);
    let settings = JiraSettings {
        jql: Some("project = \"A+B\" <x> & 'y' ~ é".into()),
        ..jira_settings(&stub.site())
    };
    assert_eq!(jira::search(&quick(), &settings, "t"), Ok(Vec::new()));
    assert_eq!(
        stub.request().target,
        "/rest/api/3/search/jql?jql=project%20%3D%20%22A%2BB%22%20%3Cx%3E%20%26%20%27y%27%20~%20%C3%A9\
         &fields=summary,status,updated&maxResults=25"
    );
}

#[test]
fn jira_errors_are_the_csharps() {
    let wrong_type = "The requested operation requires an element of type 'Object', but the target element has type \
                      'Null'.";
    for (reply, error) in [
        (
            Reply::Answer(401, "Unauthorized", "{}".into()),
            "Jira didn't accept the email/API token".to_string(),
        ),
        (
            Reply::Answer(
                403,
                "Forbidden",
                r#"{"errorMessages":["You can't search here."],"errors":{}}"#.into(),
            ),
            "You can't search here.".into(),
        ),
        (
            Reply::Answer(403, "Forbidden", "not json".into()),
            "Jira returned 403 Forbidden".into(),
        ),
        // a rate limit
        (
            Reply::Answer(429, "Too Many Requests", String::new()),
            "Jira returned 429 Too Many Requests".into(),
        ),
        (
            Reply::Answer(500, "Internal Server Error", r#"{"errorMessages":[]}"#.into()),
            "Jira returned 500 Internal Server Error".into(),
        ),
        (
            ok("<html>"),
            "Jira search failed: '<' is an invalid start of a value. LineNumber: 0 | BytePositionInLine: 0.".into(),
        ),
        (
            ok(""),
            "Jira search failed: The input does not contain any JSON tokens. Expected the input to start with a \
             valid JSON token, when isFinalBlock is true. LineNumber: 0 | BytePositionInLine: 0."
                .into(),
        ),
        (
            ok(r#"{"issues":[{"key":"ABC-1"}]}"#),
            "Jira search failed: The given key was not present in the dictionary.".into(),
        ),
        (
            ok(r#"{"issues":[{"key":"ABC-1","fields":{"status":null}}]}"#),
            format!("Jira search failed: {wrong_type}"),
        ),
        (Reply::Silence, "Jira didn't answer in time".into()),
    ] {
        let stub = Stub::new(vec![reply]);
        let http = Http::local(Duration::from_millis(500));
        assert_eq!(jira::search(&http, &jira_settings(&stub.site()), "t"), Err(error));
    }
}

#[test]
fn jira_says_what_is_wrong_with_the_address_or_the_network() {
    let search = |site: &str| jira::search(&quick(), &jira_settings(site), "t").unwrap_err();
    assert_eq!(
        search("  "),
        "Enter your Jira site first (e.g. your-team.atlassian.net)"
    );
    assert_eq!(
        search("my site"),
        "Jira search failed: Invalid URI: The hostname could not be parsed."
    );
    assert_eq!(
        search("localhost:99999"),
        "Jira search failed: Invalid URI: Invalid port specified."
    );
    let (port, refused) = closed_port();
    assert_eq!(
        search(&format!("127.0.0.1:{port}")),
        format!("Couldn't reach Jira: {refused} (127.0.0.1:{port})")
    );
}

/// A Jira watcher on the stub, with a token saved, and its events.
fn jira_watcher(stub: &Stub, dir: &Scratch) -> (JiraWatcher, Receiver<jira::Event>) {
    jira_settings(&stub.site()).save_to(&dir.0.join("jira.json")).unwrap();
    let (sender, events) = mpsc::channel();
    let watcher = JiraWatcher::new(&dir.0, secrets(&[(JIRA, "t")]), quick(), sender);
    (watcher, events)
}

fn issues(keys: &[&str]) -> Reply {
    let issues: Vec<String> = keys
        .iter()
        .map(|k| format!(r#"{{"key":"{k}","fields":{{}}}}"#))
        .collect();
    ok(&format!(r#"{{"issues":[{}]}}"#, issues.join(",")))
}

fn events<T>(events: &Receiver<T>) -> Vec<T> {
    events.try_iter().collect()
}

#[test]
fn a_new_jira_issue_alerts_once() {
    use jira::Event::{Changed, NewReviews};
    let stub = Stub::new(vec![
        issues(&["ABC-1"]),
        issues(&["ABC-1", "ABC-2"]),
        Reply::Answer(503, "Service Unavailable", String::new()),
        issues(&["ABC-2"]),
        issues(&["ABC-1", "ABC-2", "ABC-3", "ABC-4"]),
    ]);
    let dir = Scratch::new("jira-alerts");
    let (watcher, received) = jira_watcher(&stub, &dir);
    // the first poll finds them all new, but only says it changed
    watcher.poll();
    assert_eq!(events(&received), [Changed]);
    watcher.poll();
    assert_eq!(events(&received), [NewReviews(1), Changed]);
    // a failed poll keeps the issues and says why
    watcher.poll();
    assert_eq!(events(&received), [Changed]);
    assert_eq!(
        watcher.last_error().as_deref(),
        Some("Jira returned 503 Service Unavailable")
    );
    assert_eq!(watcher.issues().len(), 2);
    // one seen before isn't new when it comes back
    watcher.poll();
    watcher.poll();
    assert_eq!(events(&received), [Changed, NewReviews(2), Changed]);
    assert_eq!(watcher.last_error(), None);
    assert_eq!(watcher.issues().len(), 4);
}

#[test]
fn jira_is_off_without_its_settings_or_a_token() {
    let stub = Stub::new(vec![issues(&["ABC-1"])]);
    let dir = Scratch::new("jira-off");
    let (watcher, received) = jira_watcher(&stub, &dir);
    watcher.poll();
    assert_eq!(watcher.issues().len(), 1);
    stub.request();
    // no token: no request, no issues, no error
    watcher.forget_token();
    assert!(!watcher.has_token());
    watcher.poll();
    assert_eq!((watcher.issues(), watcher.last_error()), (Vec::new(), None));
    stub.no_more_requests();
    for settings in [
        JiraSettings {
            enabled: false,
            ..jira_settings(&stub.site())
        },
        JiraSettings {
            email: Some(" ".into()),
            ..jira_settings(&stub.site())
        },
        JiraSettings {
            site: None,
            ..jira_settings(&stub.site())
        },
    ] {
        watcher.save(settings, "t").unwrap();
        watcher.poll();
        assert_eq!(watcher.issues(), Vec::new());
    }
    stub.no_more_requests();
    assert!(received.try_iter().all(|e| e == jira::Event::Changed));
}

#[test]
fn jira_save_writes_the_settings_and_the_token() {
    let stub = Stub::new(vec![issues(&[])]);
    let dir = Scratch::new("jira-save");
    let store = secrets(&[]);
    let (sender, _events) = mpsc::channel::<jira::Event>();
    let watcher = JiraWatcher::new(&dir.0, Arc::clone(&store) as Arc<dyn SecretStore>, quick(), sender);
    assert_eq!(watcher.config(), JiraSettings::default());
    assert!(!watcher.has_token());

    let settings = jira_settings(&stub.site());
    watcher.save(settings.clone(), "").unwrap();
    assert!(!watcher.has_token());
    watcher.save(settings.clone(), "t0k3n").unwrap();
    assert_eq!(store.read(JIRA).as_deref(), Some("t0k3n"));
    assert!(watcher.has_token());
    assert_eq!(watcher.config(), settings);
    assert_eq!(JiraSettings::load_from(&dir.0.join("jira.json")).0, settings);
    // a new watcher reads what was saved
    let (sender, _events) = mpsc::channel::<jira::Event>();
    let again = JiraWatcher::new(&dir.0, store, quick(), sender);
    assert_eq!(again.config(), settings);
}

#[test]
fn jira_polls_no_more_often_than_every_30_seconds() {
    let stub = Stub::new(vec![issues(&["ABC-1"])]);
    let dir = Scratch::new("jira-loop");
    // it asks for a poll every second
    let settings = JiraSettings {
        poll_seconds: 1,
        ..jira_settings(&stub.site())
    };
    settings.save_to(&dir.0.join("jira.json")).unwrap();
    let (sender, received) = mpsc::channel();
    let watcher = JiraWatcher::new(&dir.0, secrets(&[(JIRA, "t")]), quick(), sender);
    let started = Instant::now();
    watcher.restart();
    let (first, _) = stub.next(Duration::from_secs(20)).expect("the first poll");
    assert!(first - started >= Duration::from_millis(1450), "{:?}", first - started);
    assert_eq!(received.recv_timeout(Duration::from_secs(10)), Ok(jira::Event::Changed));
    assert!(
        stub.next(Duration::from_millis(2500)).is_none(),
        "a second poll within 30 s"
    );
    assert_eq!(jira::interval(&settings), Duration::from_secs(30));
}

// ---------------------------------------------------------------------------------------------------------------
// GitHub

/// The body the C# posts to search for review requests, as .NET 10 sent it.
const POLL_BODY: &str = r#"{"query":"query($q: String!) {\n  search(query: $q, type: ISSUE, first: 30) {\n    nodes { ... on PullRequest {\n      number title url body headRefName updatedAt\n      repository { nameWithOwner }\n      commits(last: 30) { nodes { commit { message } } }\n    } }\n  }\n}","variables":{"q":"is:open is:pr review-requested:@me archived:false"}}"#;

fn github_settings(host: &str) -> GitHubSettings {
    GitHubSettings {
        enabled: true,
        host: Some(host.into()),
        ..GitHubSettings::default()
    }
}

fn github_headers(stub: &Stub, token: &str, body: &str) -> Vec<(String, String)> {
    let (host, authorization, length) = (stub.site(), format!("Bearer {token}"), body.len().to_string());
    headers(&[
        ("host", &host),
        ("authorization", &authorization),
        ("user-agent", "AiPet/1.0"),
        ("content-type", "application/json; charset=utf-8"),
        ("content-length", &length),
    ])
}

/// A GitHub watcher with these settings (on the stub when they name no host), a token saved, and its events.
fn github_watcher(stub: &Stub, dir: &Scratch, settings: GitHubSettings) -> (GitHubWatcher, Receiver<github::Event>) {
    let settings = GitHubSettings {
        host: settings
            .host
            .filter(|host| !host.is_empty())
            .or_else(|| Some(stub.site())),
        ..settings
    };
    settings.save_to(&dir.0.join("github.json")).unwrap();
    let (sender, events) = mpsc::channel();
    let watcher = GitHubWatcher::new(&dir.0, secrets(&[(GITHUB, "ghp_saved")]), quick(), sender);
    (watcher, events)
}

fn pull_requests(nodes: &[&str]) -> Reply {
    ok(&format!(r#"{{"data":{{"search":{{"nodes":[{}]}}}}}}"#, nodes.join(",")))
}

fn pull_request(number: u32) -> String {
    format!(
        r#"{{"number":{number},"title":"Change {number}","url":"https://ghe.example.com/acme/app/pull/{number}",
            "body":"","headRefName":"b{number}","updatedAt":"2026-09-29T12:03:12Z",
            "repository":{{"nameWithOwner":"acme/app"}},"commits":{{"nodes":[]}}}}"#
    )
}

#[test]
fn github_sends_the_csharps_request_and_reads_the_review_requests() {
    let stub = Stub::new(vec![pull_requests(&[
        r#"{"number":12,"title":"ABC-1: fix the login","url":"https://ghe.example.com/acme/app/pull/12",
            "body":"Closes def-7","headRefName":"feature/abc-2","updatedAt":"2026-09-29T12:03:12Z",
            "repository":{"nameWithOwner":"acme/app"},
            "commits":{"nodes":[{"commit":{"message":"ABC-1 again, not XABC-3"}}]}}"#,
        // an issue, not a pull request
        "{}",
        r#"{"number":13,"title":null,"url":"https://ghe.example.com/acme/app/pull/13","updatedAt":"yesterday",
            "repository":{"nameWithOwner":null}}"#,
    ])]);
    let dir = Scratch::new("github-poll");
    // the host as typed, with https:// and a slash, which the C# drops
    let settings = GitHubSettings {
        jira_projects: Some(vec![Some("abc".into())]),
        ..github_settings(&format!(" https://{}/ ", stub.site()))
    };
    let (watcher, received) = github_watcher(&stub, &dir, settings);
    watcher.set_jira_keys(|| vec!["DEF-7".into()]);
    watcher.poll();

    let request = stub.request();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("POST", "/api/graphql")
    );
    assert_eq!(request.headers, github_headers(&stub, "ghp_saved", POLL_BODY));
    assert_eq!(request.body, POLL_BODY);
    // every key is in a review request, so there is nothing to look up
    stub.no_more_requests();

    let url = |n: u32| format!("https://ghe.example.com/acme/app/pull/{n}");
    assert_eq!(
        watcher.review_requests(),
        [
            PullRequest {
                repo: "acme/app".into(),
                number: 12,
                title: "ABC-1: fix the login".into(),
                url: url(12),
                updated: 1790683392.0,
                keys: vec!["ABC-1".into(), "ABC-2".into(), "DEF-7".into()],
            },
            PullRequest {
                repo: String::new(),
                number: 13,
                title: String::new(),
                url: url(13),
                updated: 0.0,
                keys: Vec::new(),
            },
        ]
    );
    let expected: HashMap<String, String> = ["ABC-1", "ABC-2", "DEF-7"]
        .into_iter()
        .map(|key| (key.to_string(), url(12)))
        .collect();
    assert_eq!(watcher.pr_for_key(), expected);
    assert_eq!(watcher.last_error(), None);
    assert_eq!(events(&received), [github::Event::Changed]);
}

#[test]
fn github_looks_up_the_pull_request_for_each_jira_key() {
    let found = r#"{"data":{"k0":{"nodes":[{"url":"https://ghe.example.com/acme/app/pull/3","state":"OPEN",
                                           "updatedAt":"2026-09-29T12:03:12Z"}]},
                            "k1":{"nodes":[]}}}"#;
    let stub = Stub::new(vec![pull_requests(&[]), ok(found)]);
    let dir = Scratch::new("github-lookup");
    let orgs = Some(vec![
        Some("acme".into()),
        Some(" tools ".into()),
        Some(" ".into()),
        None,
    ]);
    let (watcher, received) = github_watcher(
        &stub,
        &dir,
        GitHubSettings {
            orgs,
            ..github_settings("")
        },
    );
    watcher.set_jira_keys(|| vec!["ABC-1".into(), "DEF-2".into(), "ABC-1".into()]);
    watcher.poll();
    stub.request();
    let lookup = stub.request();
    let body = "{\"query\":\"{ k0: search(query: \\u0022is:pr ABC-1 in:title,body org:acme org:tools\\u0022, type: \
                ISSUE, first: 5) { nodes { ... on PullRequest { url state updatedAt } } } k1: search(query: \
                \\u0022is:pr DEF-2 in:title,body org:acme org:tools\\u0022, type: ISSUE, first: 5) { nodes { ... on \
                PullRequest { url state updatedAt } } } }\"}";
    assert_eq!(lookup.body, body);
    assert_eq!(lookup.headers, github_headers(&stub, "ghp_saved", body));
    let pr = "https://ghe.example.com/acme/app/pull/3".to_string();
    assert_eq!(watcher.pr_for_key(), HashMap::from([("ABC-1".to_string(), pr.clone())]));
    assert_eq!(events(&received), [github::Event::Changed]);

    // looked up in the last 10 minutes, a miss too: nothing is asked again
    watcher.jira_changed();
    assert_eq!(
        received.recv_timeout(Duration::from_secs(10)),
        Ok(github::Event::Changed)
    );
    stub.no_more_requests();
    assert_eq!(watcher.pr_for_key(), HashMap::from([("ABC-1".to_string(), pr)]));
}

#[test]
fn without_organisations_the_lookup_searches_only_what_involves_the_user() {
    let stub = Stub::new(vec![pull_requests(&[]), ok(r#"{"data":{}}"#)]);
    let dir = Scratch::new("github-involves");
    let (watcher, _events) = github_watcher(&stub, &dir, github_settings(""));
    watcher.set_jira_keys(|| vec!["ABC-1".into()]);
    watcher.poll();
    stub.request();
    // .NET 10 sent this body for the organisation acme with `org:acme` in place of `involves:@me`
    assert_eq!(
        stub.request().body,
        "{\"query\":\"{ k0: search(query: \\u0022is:pr ABC-1 in:title,body involves:@me\\u0022, type: ISSUE, \
         first: 5) { nodes { ... on PullRequest { url state updatedAt } } } }\"}"
    );
    assert_eq!(watcher.pr_for_key(), HashMap::new());
}

#[test]
fn github_errors_are_the_csharps() {
    let wrong = |wanted: &str, actual: &str| {
        format!(
            "The requested operation requires an element of type '{wanted}', but the target element has type '{actual}'."
        )
    };
    for (reply, error) in [
        (
            Reply::Answer(401, "Unauthorized", "{}".into()),
            "GitHub didn't accept the token".to_string(),
        ),
        // rate limits
        (
            Reply::Answer(403, "Forbidden", r#"{"message":"API rate limit exceeded"}"#.into()),
            "GitHub returned 403 Forbidden".into(),
        ),
        (
            Reply::Answer(429, "Too Many Requests", "{}".into()),
            "GitHub returned 429 Too Many Requests".into(),
        ),
        (
            ok(r#"{"errors":[{"message":"Bad query"}]}"#),
            "GitHub search failed: Bad query".into(),
        ),
        (ok(r#"{"errors":[{"message":null}]}"#), "GitHub search failed: ".into()),
        (ok("[1]"), format!("GitHub search failed: {}", wrong("Object", "Array"))),
        (
            ok(r#"{"errors":[{}]}"#),
            "GitHub search failed: The given key was not present in the dictionary.".into(),
        ),
        (
            ok(r#"{"errors":{}}"#),
            format!("GitHub search failed: {}", wrong("Array", "Object")),
        ),
        (
            ok("<html>"),
            "GitHub search failed: '<' is an invalid start of a value. LineNumber: 0 | BytePositionInLine: 0.".into(),
        ),
        // errors with data: the answer is read, and what can't be read is said so
        (
            ok(r#"{"data":null,"errors":[{"message":"x"}]}"#),
            format!("Couldn't read GitHub's answer: {}", wrong("Object", "Null")),
        ),
        (
            ok(r#"{"data":{"search":{"nodes":[{"number":1.5}]}}}"#),
            "Couldn't read GitHub's answer: The given key was not present in the dictionary.".into(),
        ),
        (Reply::Silence, "GitHub didn't answer in time".into()),
    ] {
        let stub = Stub::new(vec![reply]);
        let dir = Scratch::new("github-errors");
        let (sender, _events) = mpsc::channel::<github::Event>();
        let settings = github_settings(&stub.site());
        settings.save_to(&dir.0.join("github.json")).unwrap();
        let watcher = GitHubWatcher::new(
            &dir.0,
            secrets(&[(GITHUB, "t")]),
            Http::local(Duration::from_millis(500)),
            sender,
        );
        watcher.poll();
        assert_eq!(watcher.last_error(), Some(error));
    }
}

#[test]
fn github_says_what_is_wrong_with_the_network() {
    let (port, refused) = closed_port();
    let dir = Scratch::new("github-refused");
    let (sender, _events) = mpsc::channel::<github::Event>();
    let watcher = GitHubWatcher::new(&dir.0, secrets(&[(GITHUB, "t")]), quick(), sender);
    assert_eq!(
        watcher.test(&format!("127.0.0.1:{port}"), ""),
        Err(format!("Couldn't reach GitHub: {refused} (127.0.0.1:{port})"))
    );
    // github.com's API is at api.github.com, which a stub's client doesn't reach
    for host in ["github.com", " https://github.com/ ", ""] {
        assert_eq!(
            watcher.test(host, ""),
            Err("GitHub search failed: api.github.com isn't this computer, and plain HTTP goes nowhere else".into())
        );
    }
}

#[test]
fn github_test_says_who_the_token_signs_in_as() {
    let stub = Stub::new(vec![
        ok(r#"{"data":{"viewer":{"login":"octo"},"search":{"issueCount":3}}}"#),
        ok(r#"{"data":{"viewer":{"login":"octo"},"search":{"issueCount":3}}}"#),
        ok(r#"{"data":{"viewer":{"login":"octo"}}}"#),
    ]);
    let dir = Scratch::new("github-test");
    let (sender, _events) = mpsc::channel::<github::Event>();
    let store = secrets(&[]);
    let watcher = GitHubWatcher::new(&dir.0, Arc::clone(&store) as Arc<dyn SecretStore>, quick(), sender);
    let host = format!("http://{}", stub.site());
    assert_eq!(watcher.test(&host, " "), Err("Paste an access token first.".into()));
    stub.no_more_requests();

    let connected = "Connected as octo. 3 pull request(s) waiting on your review.".to_string();
    assert_eq!(watcher.test(&host, " ghp_typed "), Ok(connected.clone()));
    let request = stub.request();
    let body = "{\"query\":\"{ viewer { login }\\n  search(query: \\u0022is:open is:pr review-requested:@me \
                archived:false\\u0022, type: ISSUE, first: 1) { issueCount } }\"}";
    assert_eq!(request.body, body);
    assert_eq!(request.headers, github_headers(&stub, "ghp_typed", body));
    // the saved token when none is typed
    store.write(GITHUB, "github", "ghp_saved").unwrap();
    assert_eq!(watcher.test(&host, ""), Ok(connected));
    assert!(
        stub.request()
            .headers
            .contains(&("authorization".into(), "Bearer ghp_saved".into()))
    );
    assert_eq!(
        watcher.test(&host, ""),
        Err("Couldn't read GitHub's answer: The given key was not present in the dictionary.".into())
    );
}

#[test]
fn a_new_review_request_alerts_once() {
    use github::Event::{Changed, NewReviews};
    let (one, two, three) = (pull_request(1), pull_request(2), pull_request(3));
    let stub = Stub::new(vec![
        pull_requests(&[&one]),
        pull_requests(&[&one, &two]),
        Reply::Answer(502, "Bad Gateway", String::new()),
        pull_requests(&[&two]),
        pull_requests(&[&one, &two, &three]),
    ]);
    let dir = Scratch::new("github-alerts");
    let (watcher, received) = github_watcher(&stub, &dir, github_settings(""));
    watcher.poll();
    assert_eq!(events(&received), [Changed]);
    watcher.poll();
    assert_eq!(events(&received), [NewReviews(1), Changed]);
    watcher.poll();
    assert_eq!(watcher.last_error().as_deref(), Some("GitHub returned 502 Bad Gateway"));
    assert_eq!(watcher.review_requests().len(), 2);
    watcher.poll();
    watcher.poll();
    assert_eq!(events(&received), [Changed, Changed, NewReviews(1), Changed]);
    assert_eq!(watcher.review_requests().len(), 3);
    stub.drain();
    // forgetting the token turns reviews off, and a new one starts from nothing seen
    watcher.forget_token();
    watcher.poll();
    assert_eq!((watcher.review_requests(), watcher.last_error()), (Vec::new(), None));
    watcher.save(watcher.config(), " ghp_new ").unwrap();
    watcher.poll();
    assert_eq!(events(&received), [Changed, Changed]);
    assert!(
        stub.next(Duration::from_secs(10))
            .is_some_and(|(_, r)| r.headers.contains(&("authorization".into(), "Bearer ghp_new".into())))
    );
}

#[test]
fn github_is_off_until_it_is_on_with_a_token() {
    let stub = Stub::new(vec![pull_requests(&[])]);
    let dir = Scratch::new("github-off");
    let settings = GitHubSettings {
        enabled: false,
        ..github_settings("")
    };
    let (watcher, received) = github_watcher(&stub, &dir, settings);
    assert!(watcher.has_saved_token());
    watcher.poll();
    watcher.jira_changed();
    assert_eq!(
        received.recv_timeout(Duration::from_secs(10)),
        Ok(github::Event::Changed)
    );
    assert_eq!(
        received.recv_timeout(Duration::from_secs(10)),
        Ok(github::Event::Changed)
    );
    assert_eq!((watcher.review_requests(), watcher.last_error()), (Vec::new(), None));
    stub.no_more_requests();
}

#[test]
fn github_polls_no_more_often_than_every_60_seconds() {
    let stub = Stub::new(vec![pull_requests(&[])]);
    let dir = Scratch::new("github-loop");
    // it asks for a poll every second
    let settings = GitHubSettings {
        poll_seconds: 1,
        ..github_settings("")
    };
    let (watcher, received) = github_watcher(&stub, &dir, settings.clone());
    let started = Instant::now();
    watcher.restart();
    let (first, _) = stub.next(Duration::from_secs(20)).expect("the first poll");
    assert!(first - started >= Duration::from_millis(3950), "{:?}", first - started);
    assert_eq!(
        received.recv_timeout(Duration::from_secs(10)),
        Ok(github::Event::Changed)
    );
    assert!(
        stub.next(Duration::from_millis(2500)).is_none(),
        "a second poll within 60 s"
    );
    assert_eq!(github::interval(&settings), Duration::from_secs(60));
}
