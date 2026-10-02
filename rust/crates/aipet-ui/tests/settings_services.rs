//! Settings' Jira and GitHub pages against real watchers and a stub server on this computer: Test's status lines
//! are the C#'s (`SettingsWindow.axaml.cs:284-412`), Save writes the settings and the token and starts the watcher
//! again, Forget removes the token, and a saved token never reaches the page.
//!
//! Requests go through `Http::local`, which reaches this computer only, so no test can reach the real Jira or GitHub.
//! Tokens are kept in memory and the settings in a folder of the test's own, never the user's. The platform is the
//! recording fake: Create a token opens nothing for real.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use aipet_core::config::{GitHubSettings, JiraSettings};
use aipet_core::github::{self, GitHubWatcher};
use aipet_core::jira::{self, Http, JiraWatcher};
use aipet_core::secrets::{GITHUB, JIRA, MemorySecrets, SecretStore};
use aipet_ui::platform::{Call, Recorder};
use aipet_ui::settings::{Message as SettingsMessage, Services, Tone, github as github_page, jira as jira_page};
use aipet_ui::{Message, PetUi, Setup};

// ---------------------------------------------------------------------------------------------------------------
// The stub

/// A request as the stub read it: the method and target, and the Authorization header.
#[derive(Debug)]
struct Request {
    line: String,
    authorization: String,
}

/// A server on this computer that answers each request with the next of its JSON bodies (status and body; the last
/// one again when they run out), and records the requests.
struct Stub {
    port: u16,
    requests: Receiver<Request>,
}

impl Stub {
    fn new(replies: Vec<(u16, &'static str)>) -> Stub {
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

    /// The address the pages are given: `127.0.0.1:port`.
    fn site(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    /// The next request, waiting up to 10 s for it.
    fn request(&self) -> Request {
        self.requests.recv_timeout(Duration::from_secs(10)).expect("a request")
    }

    fn no_more_requests(&self) {
        thread::sleep(Duration::from_millis(100));
        assert!(self.requests.try_recv().is_err(), "no other request");
    }
}

fn serve(stream: TcpStream, sender: &Sender<Request>, replies: &Mutex<Vec<(u16, &'static str)>>) {
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let (mut authorization, mut length) = (String::new(), 0);
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).unwrap_or(0) == 0 {
                return;
            }
            let header = header.trim_end();
            if header.is_empty() {
                break;
            }
            let (name, value) = header.split_once(':').unwrap();
            match name.to_ascii_lowercase().as_str() {
                "authorization" => authorization = value.trim().to_owned(),
                "content-length" => length = value.trim().parse().unwrap(),
                _ => {}
            }
        }
        let mut body = vec![0; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        let (status, body) = {
            let mut replies = replies.lock().unwrap();
            if replies.len() > 1 {
                replies.remove(0)
            } else {
                replies[0]
            }
        };
        let _ = sender.send(Request {
            line: line.trim_end().to_owned(),
            authorization,
        });
        let reply = format!(
            "HTTP/1.1 {status} {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\n\r\n{body}",
            if status == 200 { "OK" } else { "Unauthorized" },
            body.len()
        );
        if writer.write_all(reply.as_bytes()).is_err() {
            return;
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The pet

/// A folder of the test's own under the temp folder, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("aipet-ui-settings-{name}-{}", std::process::id()));
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

/// A pet whose Settings has real watchers over a scratch folder, tokens in memory and a client for the stub.
struct Pet {
    ui: PetUi,
    jira: Arc<JiraWatcher>,
    github: Arc<GitHubWatcher>,
    secrets: Arc<MemorySecrets>,
    platform: Arc<Recorder>,
    dir: Scratch,
    start: Instant,
    // the watchers' events, kept so they have someone to send to
    _events: (Receiver<jira::Event>, Receiver<github::Event>),
}

impl Pet {
    /// The pet with these tokens saved, on its first frame.
    fn new(name: &str, saved: &[(&str, &str)]) -> Pet {
        let dir = Scratch::new(name);
        let secrets = Arc::new(MemorySecrets::default());
        for (key, token) in saved {
            secrets.write(key, "user", token).unwrap();
        }
        let http = Http::local(Duration::from_secs(10));
        let store = || Arc::clone(&secrets) as Arc<dyn SecretStore>;
        let (jira_events, jira_receiver) = mpsc::channel::<jira::Event>();
        let (github_events, github_receiver) = mpsc::channel::<github::Event>();
        let jira = Arc::new(JiraWatcher::new(&dir.0, store(), http.clone(), jira_events));
        let github = Arc::new(GitHubWatcher::new(&dir.0, store(), http.clone(), github_events));
        let platform = Arc::new(Recorder::new());
        let ui = PetUi::new(Setup {
            platform: platform.clone(),
            services: Services {
                jira: Some(Arc::clone(&jira)),
                github: Some(Arc::clone(&github)),
                secrets: store(),
                http,
                ..Services::detached(dir.0.clone())
            },
            ..Setup::detached()
        });
        let mut pet = Pet {
            ui,
            jira,
            github,
            secrets,
            platform,
            dir,
            start: Instant::now(),
            _events: (jira_receiver, github_receiver),
        };
        pet.frame();
        pet
    }

    fn frame(&mut self) {
        self.ui.tick(self.start + self.start.elapsed());
    }

    fn jira(&mut self, message: jira_page::Message) {
        assert_eq!(self.ui.update(Message::Settings(SettingsMessage::Jira(message))), None);
    }

    fn github(&mut self, message: github_page::Message) {
        assert_eq!(
            self.ui.update(Message::Settings(SettingsMessage::GitHub(message))),
            None
        );
    }

    fn jira_page(&self) -> &jira_page::Jira {
        &self.ui.settings().jira
    }

    fn github_page(&self) -> &github_page::GitHub {
        &self.ui.settings().github
    }

    fn jira_status(&self) -> Option<(String, Tone)> {
        self.jira_page().status.clone()
    }

    fn github_status(&self) -> (String, Tone) {
        self.github_page().status(Some(&self.github))
    }

    /// Runs frames until the Test running has its outcome.
    fn tested(&mut self, waiting: &str, status: impl Fn(&Pet) -> String) {
        let until = Instant::now() + Duration::from_secs(15);
        while status(self) == waiting {
            assert!(Instant::now() < until, "the Test took too long");
            thread::sleep(Duration::from_millis(5));
            self.frame();
        }
    }

    /// Jira's Test, run to its outcome.
    fn test_jira(&mut self) -> (String, Tone) {
        self.jira(jira_page::Message::Test);
        let searching = ("Searching…".to_owned(), Tone::Muted);
        if self.jira_status() == Some(searching.clone()) {
            self.tested(&searching.0, |pet| pet.jira_status().unwrap().0);
        }
        self.jira_status().unwrap()
    }

    /// GitHub's Test, run to its outcome.
    fn test_github(&mut self) -> (String, Tone) {
        self.github(github_page::Message::Test);
        assert_eq!(self.github_status(), ("Connecting…".to_owned(), Tone::Muted));
        self.tested("Connecting…", |pet| pet.github_status().0);
        self.github_status()
    }
}

fn ok(line: &str) -> (String, Tone) {
    (line.to_owned(), Tone::Ok)
}

fn bad(line: &str) -> (String, Tone) {
    (line.to_owned(), Tone::Bad)
}

fn muted(line: &str) -> (String, Tone) {
    (line.to_owned(), Tone::Muted)
}

const TWO_ISSUES: &str = r#"{"issues":[{"key":"ABC-1","fields":{"summary":"Fix the login"}},
                                       {"key":"ABC-2","fields":{"summary":"Tidy up"}}]}"#;

// ---------------------------------------------------------------------------------------------------------------
// Jira

#[test]
fn jira_tests_saves_and_forgets_with_the_csharps_texts() {
    let stub = Stub::new(vec![(200, TWO_ISSUES), (401, "{}"), (200, r#"{"issues":[]}"#)]);
    let mut pet = Pet::new("jira", &[]);
    // the saved settings fill the fields; without a token, Jira is ticked so that Save turns it on
    let page = pet.jira_page();
    assert_eq!((page.enabled, page.site.as_str(), page.has_token), (true, "", false));
    assert_eq!(page.jql, aipet_core::jira::DEFAULT_JQL);
    assert_eq!(page.status, None);

    // no token typed and none saved: nothing is asked
    assert_eq!(pet.test_jira(), bad("Paste an API token first."));
    stub.no_more_requests();
    pet.jira(jira_page::Message::Site(format!(" {} ", stub.site())));
    pet.jira(jira_page::Message::Email("me@example.com".into()));
    pet.jira(jira_page::Message::Token("t0k3n".into()));
    assert_eq!(
        pet.test_jira(),
        ok("Connected. The search finds 2 issue(s), e.g. ABC-1: Fix the login")
    );
    let request = stub.request();
    assert!(
        request.line.starts_with("GET /rest/api/3/search/jql?jql="),
        "{request:?}"
    );
    assert_eq!(request.authorization, "Basic bWVAZXhhbXBsZS5jb206dDBrM24=");
    assert_eq!(pet.test_jira(), bad("Jira didn't accept the email/API token"));
    stub.request();
    assert_eq!(pet.test_jira(), ok("Connected. The search finds no issues right now."));
    stub.request();
    // a Test saves nothing
    assert_eq!(pet.secrets.read(JIRA), None);
    assert!(!pet.dir.0.join("jira.json").exists());

    // Save writes jira.json and the token, empties the token box, and starts the watcher again
    pet.jira(jira_page::Message::Jql("  ".into()));
    pet.jira(jira_page::Message::Save);
    assert_eq!(
        pet.jira_status(),
        Some(ok("Saved. The pet checks Jira every couple of minutes."))
    );
    let saved = JiraSettings {
        enabled: true,
        site: Some(stub.site()),
        email: Some("me@example.com".into()),
        ..JiraSettings::default()
    };
    assert_eq!(JiraSettings::load_from(&pet.dir.0.join("jira.json")).0, saved);
    assert_eq!(pet.jira.config(), saved);
    assert_eq!(pet.secrets.read(JIRA).as_deref(), Some("t0k3n"));
    assert_eq!((pet.jira_page().token.as_str(), pet.jira_page().has_token), ("", true));
    // the watcher's first poll, 1.5 s after the restart, with the saved token
    let poll = stub.request();
    assert!(poll.line.starts_with("GET /rest/api/3/search/jql?"), "{poll:?}");
    assert_eq!(poll.authorization, "Basic bWVAZXhhbXBsZS5jb206dDBrM24=");
    // the page's own Save leaves its fields as typed
    pet.frame();
    assert_eq!(pet.jira_page().jql, "  ");

    // Forget removes the token, and Remove saved token goes
    pet.jira(jira_page::Message::Forget);
    assert_eq!(pet.secrets.read(JIRA), None);
    assert!(!pet.jira_page().has_token);
    assert_eq!(
        pet.jira_status(),
        Some(muted(
            "The saved token was removed. Jira stays off until you save a new one."
        ))
    );

    // Create a token opens Atlassian's page
    pet.jira(jira_page::Message::CreateToken);
    assert_eq!(pet.platform.calls(), [Call::OpenUrl(jira_page::TOKENS_PAGE.into())]);
}

// ---------------------------------------------------------------------------------------------------------------
// GitHub

#[test]
fn github_tests_saves_and_forgets_with_the_csharps_texts() {
    let stub = Stub::new(vec![
        (200, r#"{"data":{"viewer":{"login":"octo"},"search":{"issueCount":3}}}"#),
        (401, r#"{"message":"Bad credentials"}"#),
        (200, r#"{"data":{"search":{"nodes":[]}}}"#),
    ]);
    let mut pet = Pet::new("github", &[]);
    let page = pet.github_page();
    assert_eq!(
        (page.enabled, page.host.as_str(), page.orgs.as_str()),
        (false, "github.com", "")
    );
    assert_eq!(pet.github_status(), muted("GitHub reviews are off."));

    pet.github(github_page::Message::Host(stub.site()));
    assert_eq!(pet.test_github(), bad("Paste an access token first."));
    stub.no_more_requests();
    // a token typed ticks the watch box
    pet.github(github_page::Message::Token(" ".into()));
    assert!(!pet.github_page().enabled);
    pet.github(github_page::Message::Token(" ghp_typed ".into()));
    assert!(pet.github_page().enabled);
    assert_eq!(
        pet.test_github(),
        ok("Connected as octo. 3 pull request(s) waiting on your review.")
    );
    let request = stub.request();
    assert_eq!(request.line, "POST /api/graphql HTTP/1.1");
    assert_eq!(request.authorization, "Bearer ghp_typed");
    assert_eq!(pet.test_github(), bad("GitHub didn't accept the token"));
    stub.request();
    assert_eq!(pet.secrets.read(GITHUB), None);

    // Save writes github.json and the token (trimmed), and starts the watcher again
    pet.github(github_page::Message::Orgs(" acme, ,widgets ".into()));
    pet.github(github_page::Message::Save);
    assert_eq!(
        pet.github_status(),
        muted("Saved. The pet checks GitHub every couple of minutes.")
    );
    let saved = GitHubSettings {
        enabled: true,
        host: Some(stub.site()),
        orgs: Some(vec![Some("acme".into()), Some("widgets".into())]),
        ..GitHubSettings::default()
    };
    assert_eq!(GitHubSettings::load_from(&pet.dir.0.join("github.json")).0, saved);
    assert_eq!(pet.github.config(), saved);
    assert_eq!(pet.secrets.read(GITHUB).as_deref(), Some("ghp_typed"));
    assert_eq!(
        (pet.github_page().token.as_str(), pet.github_page().has_token),
        ("", true)
    );
    // the watcher's first poll, 4 s after the restart, with the saved token
    let poll = stub.request();
    assert_eq!(poll.line, "POST /api/graphql HTTP/1.1");
    assert_eq!(poll.authorization, "Bearer ghp_typed");

    pet.github(github_page::Message::Forget);
    assert_eq!(pet.secrets.read(GITHUB), None);
    assert!(!pet.github_page().has_token);
    assert_eq!(
        pet.github_status(),
        muted("The saved token was removed. GitHub reviews are off until you save a new one.")
    );

    // Create a token opens the host's page, github.com's for no host
    pet.github(github_page::Message::CreateToken);
    pet.github(github_page::Message::Host("  ".into()));
    pet.github(github_page::Message::CreateToken);
    assert_eq!(
        pet.platform.calls(),
        [
            Call::OpenUrl(format!("https://{}/settings/personal-access-tokens/new", stub.site())),
            Call::OpenUrl("https://github.com/settings/personal-access-tokens/new".into()),
        ]
    );
}

#[test]
fn github_says_where_the_watcher_stands_when_no_button_has_spoken() {
    let stub = Stub::new(vec![(401, r#"{"message":"Bad credentials"}"#)]);
    let mut pet = Pet::new("github-status", &[]);
    let on = GitHubSettings {
        enabled: true,
        host: Some(stub.site()),
        ..GitHubSettings::default()
    };
    // saved elsewhere (Import defaults): the page fills again
    pet.github.save(on.clone(), "").unwrap();
    pet.frame();
    assert_eq!(
        pet.github_status(),
        muted("No token yet, so GitHub reviews are off. Paste a token above.")
    );
    pet.github(github_page::Message::Forget);
    pet.secrets.write(GITHUB, "github", "ghp_saved").unwrap();
    pet.github
        .save(
            GitHubSettings {
                poll_seconds: 300,
                ..on
            },
            "",
        )
        .unwrap();
    pet.frame();
    assert!(pet.github_page().has_token);
    assert_eq!(
        pet.github_status(),
        muted("Connected with your token. 0 pull request(s) waiting on your review.")
    );
    // the watcher's last error, as it stands now
    pet.github.poll();
    assert_eq!(pet.github_status(), bad("GitHub didn't accept the token"));
}

// ---------------------------------------------------------------------------------------------------------------
// Both

#[test]
fn a_saved_token_is_never_shown() {
    let stub = Stub::new(vec![
        (200, TWO_ISSUES),
        (200, r#"{"data":{"viewer":{"login":"octo"},"search":{"issueCount":0}}}"#),
    ]);
    let (jira_token, github_token) = ("jira-saved-secret", "ghp_saved_secret");
    let mut pet = Pet::new("never-shown", &[(JIRA, jira_token), (GITHUB, github_token)]);
    let shown = |pet: &Pet| {
        let pages = format!(
            "{:?} {:?} {:?}",
            pet.jira_page(),
            pet.github_page(),
            pet.github_status()
        );
        assert!(!pages.contains(jira_token) && !pages.contains(github_token), "{pages}");
    };
    // the token boxes are empty, and say a token is saved
    assert_eq!((pet.jira_page().token.as_str(), pet.jira_page().has_token), ("", true));
    assert_eq!(
        (pet.github_page().token.as_str(), pet.github_page().has_token),
        ("", true)
    );
    // with a token saved, Jira is ticked only if the settings say so
    assert!(!pet.jira_page().enabled);
    shown(&pet);

    // Test uses the saved token, which still doesn't show
    pet.jira(jira_page::Message::Site(stub.site()));
    pet.jira(jira_page::Message::Email("me@example.com".into()));
    assert_eq!(
        pet.test_jira(),
        ok("Connected. The search finds 2 issue(s), e.g. ABC-1: Fix the login")
    );
    assert!(stub.request().authorization.starts_with("Basic "));
    pet.github(github_page::Message::Host(stub.site()));
    assert_eq!(
        pet.test_github(),
        ok("Connected as octo. 0 pull request(s) waiting on your review.")
    );
    assert_eq!(stub.request().authorization, format!("Bearer {github_token}"));
    shown(&pet);

    // Save with an empty box keeps the saved token, and the box stays empty
    pet.jira(jira_page::Message::Save);
    pet.github(github_page::Message::Save);
    pet.frame();
    assert_eq!(pet.secrets.read(JIRA).as_deref(), Some(jira_token));
    assert_eq!(pet.secrets.read(GITHUB).as_deref(), Some(github_token));
    assert_eq!(pet.jira_page().token, "");
    assert_eq!(pet.github_page().token, "");
    shown(&pet);
}

#[test]
fn settings_saved_elsewhere_fill_the_fields_and_leave_the_token_box_alone() {
    let mut pet = Pet::new("saved-elsewhere", &[]);
    pet.jira(jira_page::Message::Token("typed".into()));
    pet.jira(jira_page::Message::Site("typed.example.net".into()));
    // Import defaults saves through the watchers
    let jira = JiraSettings {
        site: Some("team.example.net".into()),
        jql: Some("project = ABC".into()),
        ..JiraSettings::default()
    };
    pet.jira.save(jira, "").unwrap();
    let github = GitHubSettings {
        host: Some("ghe.example.com".into()),
        orgs: Some(vec![Some("acme".into()), None, Some("widgets".into())]),
        ..GitHubSettings::default()
    };
    pet.github.save(github, "").unwrap();
    pet.frame();
    let page = pet.jira_page();
    assert_eq!(
        (page.site.as_str(), page.jql.as_str(), page.token.as_str()),
        ("team.example.net", "project = ABC", "typed")
    );
    let page = pet.github_page();
    assert_eq!(
        (page.host.as_str(), page.orgs.as_str()),
        ("ghe.example.com", "acme, , widgets")
    );
    // a frame without a change keeps what is typed
    pet.jira(jira_page::Message::Site("typed again".into()));
    pet.frame();
    assert_eq!(pet.jira_page().site, "typed again");
}
