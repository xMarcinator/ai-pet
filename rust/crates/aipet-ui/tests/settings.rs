//! Settings' General and Avatars pages (SettingsWindow's BuildGeneral, ImportAsync and FillAvatars), through the
//! pet's own messages: Import defaults… with a chosen file (the picker itself is the desktop's, and is never opened
//! here), the switches kept in `config.json`, and the avatars read again and their folder opened. Every test works in
//! a folder of its own, with tokens in memory and a client that reaches nothing but this computer.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aipet_core::config::{Config, GitHubSettings, JiraSettings};
use aipet_core::github::{self, GitHubWatcher};
use aipet_core::jira::{self, Http, JiraWatcher};
use aipet_core::secrets::{self, MemorySecrets, SecretStore};
use aipet_core::update_status::{Kind, UpdateStatus};
use aipet_ui::platform::{Call, Recorder};
use aipet_ui::settings::{self, Services, Tone, general};
use aipet_ui::{Host, Message, PetUi, Place, Prefs, Setup};

const JIRA_SITE: &str = "team.atlassian.net";
const JIRA_TOKEN: &str = "jira-token";
const GITHUB_TOKEN: &str = "github-token";

/// A folder of the test's own, removed with it.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Scratch {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("aipet-settings-{}-{test}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    /// A file in the folder with `text` in it.
    fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A pet whose Settings has real watchers over a data folder of its own, with both tokens saved and Jira and GitHub
/// set up as `saved` says.
struct Pet {
    ui: PetUi,
    secrets: Arc<MemorySecrets>,
    jira: Arc<JiraWatcher>,
    github: Arc<GitHubWatcher>,
    platform: Arc<Recorder>,
    scratch: Scratch,
    _events: (Receiver<jira::Event>, Receiver<github::Event>),
}

impl Pet {
    fn new(test: &str) -> Pet {
        let scratch = Scratch::new(test);
        let jira_saved = JiraSettings {
            enabled: true,
            site: Some(JIRA_SITE.into()),
            email: Some("me@team.example".into()),
            ..JiraSettings::default()
        };
        jira_saved.save_to(&scratch.0.join("jira.json")).unwrap();
        let github_saved = GitHubSettings {
            enabled: true,
            host: Some("github.com".into()),
            ..GitHubSettings::default()
        };
        github_saved.save_to(&scratch.0.join("github.json")).unwrap();
        let secrets = Arc::new(MemorySecrets::default());
        secrets.write(secrets::JIRA, "me@team.example", JIRA_TOKEN).unwrap();
        secrets.write(secrets::GITHUB, "github", GITHUB_TOKEN).unwrap();
        let http = || Http::local(Duration::from_secs(1));
        let (jira_events, jira_heard) = channel::<jira::Event>();
        let (github_events, github_heard) = channel::<github::Event>();
        let jira = Arc::new(JiraWatcher::new(&scratch.0, secrets.clone(), http(), jira_events));
        let github = Arc::new(GitHubWatcher::new(&scratch.0, secrets.clone(), http(), github_events));
        let platform = Arc::new(Recorder::new());
        let ui = PetUi::new(Setup {
            platform: platform.clone(),
            avatars: aipet_sprite::Avatar::custom_dir(&scratch.0),
            services: Services {
                data_dir: scratch.0.clone(),
                jira: Some(jira.clone()),
                github: Some(github.clone()),
                secrets: secrets.clone(),
                http: http(),
                version: "0.3.0".into(),
                update_status: || UpdateStatus::new(Kind::Off),
            },
            ..Setup::detached()
        });
        Pet {
            ui,
            secrets,
            jira,
            github,
            platform,
            scratch,
            _events: (jira_heard, github_heard),
        }
    }

    /// Imports a preset file with `text` in it, as the picker's choice, and says what the page shows then.
    fn import(&mut self, text: &str) -> (String, Tone) {
        let file = self.scratch.file("preset.json", text);
        self.import_file(&file)
    }

    fn import_file(&mut self, file: &Path) -> (String, Tone) {
        general::import_file(&mut self.ui, file);
        self.ui
            .settings()
            .general
            .import_status
            .clone()
            .expect("a line under Team defaults")
    }

    fn tokens(&self) -> (Option<String>, Option<String>) {
        (self.secrets.read(secrets::JIRA), self.secrets.read(secrets::GITHUB))
    }

    /// The settings as saved in the data folder.
    fn saved(&self) -> (JiraSettings, GitHubSettings) {
        (
            JiraSettings::load_from(&self.scratch.0.join("jira.json")).0,
            GitHubSettings::load_from(&self.scratch.0.join("github.json")).0,
        )
    }
}

fn kept() -> (Option<String>, Option<String>) {
    (Some(JIRA_TOKEN.into()), Some(GITHUB_TOKEN.into()))
}

#[test]
fn a_preset_that_moves_jira_or_github_forgets_that_token() {
    let cases = [
        (
            r#"{"version":1,"jira":{"site":"other.atlassian.net"}}"#,
            (None, Some(GITHUB_TOKEN.into())),
            "Imported Jira (site). It points Jira at other.atlassian.net, so the token you saved for the old address \
             was removed: paste it again if you trust the new one. Your email stays as it was.",
        ),
        (
            r#"{"version":1,"github":{"host":" ghe.team.example "}}"#,
            (Some(JIRA_TOKEN.into()), None),
            "Imported GitHub (host). It points GitHub at ghe.team.example, so the token you saved for the old address \
             was removed: paste it again if you trust the new one. Your email stays as it was.",
        ),
        (
            r#"{"version":1,"jira":{"site":"other.atlassian.net"},"github":{"host":"ghe.team.example"}}"#,
            (None, None),
            "Imported Jira (site) and GitHub (host). It points Jira at other.atlassian.net and GitHub at \
             ghe.team.example, so the tokens you saved for the old addresses were removed: paste them again if you \
             trust the new one. Your email stays as it was.",
        ),
    ];
    for (preset, tokens, line) in cases {
        let mut pet = Pet::new("moves");
        assert_eq!(pet.import(preset), (line.to_owned(), Tone::Muted), "{preset}");
        assert_eq!(pet.tokens(), tokens, "{preset}");
        // the new addresses are saved, and the email stays
        let (jira, github) = pet.saved();
        let want_site = if tokens.0.is_none() {
            "other.atlassian.net"
        } else {
            JIRA_SITE
        };
        assert_eq!(jira.site.as_deref(), Some(want_site), "{preset}");
        assert_eq!(jira.email.as_deref(), Some("me@team.example"));
        let want_host = if tokens.1.is_none() {
            "ghe.team.example"
        } else {
            "github.com"
        };
        assert_eq!(github.host.as_deref(), Some(want_host), "{preset}");
        assert_eq!(pet.jira.config(), jira, "the watcher runs with what was saved");
        assert_eq!(pet.github.config(), github);
    }
}

#[test]
fn a_preset_that_keeps_the_sites_keeps_the_tokens() {
    let mut pet = Pet::new("keeps");
    // the same site in other letters, a search, organisations and switches: nothing points elsewhere
    let preset = r#"{
        "version": 1,
        // the team's board
        "jira": {"site": "TEAM.atlassian.net", "jql": "project = AP", "enabled": false},
        "github": {"host": "github.com", "orgs": ["team", " tools ", ""], "enabled": true},
    }"#;
    assert_eq!(
        pet.import(preset),
        (
            "Imported Jira (site, search, off) and GitHub (host, 2 organisations, on). Your tokens and email stay as \
             they were."
                .to_owned(),
            Tone::Ok
        )
    );
    assert_eq!(pet.tokens(), kept());
    let (jira, github) = pet.saved();
    assert_eq!(
        jira,
        JiraSettings {
            enabled: false,
            site: Some("TEAM.atlassian.net".into()),
            email: Some("me@team.example".into()),
            jql: Some("project = AP".into()),
            ..JiraSettings::default()
        }
    );
    assert_eq!(github.orgs, Some(vec![Some("team".into()), Some("tools".into())]));
    // a preset that moves a site with no token saved has nothing to forget
    pet.secrets.delete(secrets::JIRA);
    let (line, tone) = pet.import(r#"{"version":1,"jira":{"site":"other.atlassian.net"}}"#);
    assert_eq!(
        (line.as_str(), tone),
        (
            "Imported Jira (site). Your tokens and email stay as they were.",
            Tone::Ok
        )
    );
    assert_eq!(pet.tokens(), (None, Some(GITHUB_TOKEN.into())));
}

#[test]
fn a_bad_file_shows_the_csharps_message_and_changes_nothing() {
    let cases = [
        (
            "not json",
            "Nothing was imported. The file isn't valid JSON (line 1).",
            Tone::Bad,
        ),
        (
            "[1]",
            "Nothing was imported. The file isn't an AiPet preset.",
            Tone::Bad,
        ),
        (
            r#"{"jira":{"site":"other.atlassian.net"}}"#,
            "Nothing was imported. The file isn't an AiPet preset: it has no \"version\".",
            Tone::Bad,
        ),
        (
            r#"{"version":2,"jira":{"site":"other.atlassian.net"}}"#,
            "Nothing was imported. The preset is version 2; this AiPet reads version 1.",
            Tone::Bad,
        ),
        (
            r#"{"version":1,"jira":{"site":7}}"#,
            "Nothing was imported. \"jira.site\" must be text in quotes.",
            Tone::Bad,
        ),
        (
            r#"{"version":1,"slack":{}}"#,
            "Nothing was imported: the file has no Jira or GitHub settings.",
            Tone::Muted,
        ),
    ];
    for (text, line, tone) in cases {
        let mut pet = Pet::new("bad");
        let before = pet.saved();
        assert_eq!(pet.import(text), (line.to_owned(), tone), "{text}");
        assert_eq!(pet.tokens(), kept(), "{text}");
        assert_eq!(pet.saved(), before, "{text}");
    }
    // a byte order mark picks the encoding, as a StreamReader's does
    let preset = r#"{"version":1,"jira":{"site":"other.atlassian.net"}}"#;
    let utf16 = |be: bool| -> Vec<u8> {
        let units = std::iter::once(0xFEFF).chain(preset.encode_utf16());
        units
            .flat_map(|u| if be { u.to_be_bytes() } else { u.to_le_bytes() })
            .collect()
    };
    let utf32 = |be: bool| -> Vec<u8> {
        let chars = std::iter::once(0xFEFF).chain(preset.chars().map(u32::from));
        chars
            .flat_map(|c| if be { c.to_be_bytes() } else { c.to_le_bytes() })
            .collect()
    };
    let utf8 = [&[0xEF, 0xBB, 0xBF][..], preset.as_bytes()].concat();
    for bytes in [utf8, utf16(false), utf16(true), utf32(false), utf32(true)] {
        let mut pet = Pet::new("encoded");
        let file = pet.scratch.0.join("encoded.json");
        std::fs::write(&file, &bytes).unwrap();
        let (line, tone) = pet.import_file(&file);
        assert!(line.starts_with("Imported Jira (site)."), "{:x?}: {line}", &bytes[..4]);
        assert_eq!(tone, Tone::Muted);
        assert_eq!(pet.saved().0.site.as_deref(), Some("other.atlassian.net"));
    }
    // a file that can't be read
    let mut pet = Pet::new("unreadable");
    let missing = pet.scratch.0.join("gone.json");
    let (line, tone) = pet.import_file(&missing);
    assert!(line.starts_with("Couldn't read the file: "), "{line}");
    assert_eq!(tone, Tone::Bad);
    assert_eq!(pet.tokens(), kept());
}

/// The app's host as far as `config.json` goes: every change of the preferences is written to it at once, as the
/// app's `config::Store::change` does.
struct ConfigFile(PathBuf, Arc<Mutex<usize>>);

impl Host for ConfigFile {
    fn prefs(&mut self, prefs: &Prefs) {
        let mut config = Config::load_from(&self.0).0;
        config.pills = prefs.bubbles;
        config.on_top = prefs.on_top;
        config.music = prefs.music;
        config.avatar = prefs.avatar.clone();
        config.save_to(&self.0).unwrap();
        *self.1.lock().unwrap() += 1;
    }

    fn dismiss(&mut self, _id: &str) {}

    fn settings(&mut self, _action: settings::Action) -> bool {
        false
    }

    fn saved_place(&mut self) -> Option<Place> {
        None
    }

    fn save_place(&mut self, _place: Place) {}

    fn reset_place(&mut self) {}

    fn quit(&mut self) {}
}

#[test]
fn the_switches_and_the_avatar_persist_through_config_json() {
    let scratch = Scratch::new("switches");
    let file = scratch.0.join("config.json");
    let writes = Arc::new(Mutex::new(0));
    let start = |prefs: Prefs| {
        PetUi::new(Setup {
            host: Box::new(ConfigFile(file.clone(), writes.clone())),
            prefs,
            ..Setup::detached()
        })
    };
    let mut ui = start(Prefs::default());
    // the General page's switches, and an avatar picked on the Avatars page
    ui.update(Message::SetBubbles(false));
    ui.update(Message::SetOnTop(false));
    ui.update(Message::SetMusic(false));
    ui.update(Message::SelectAvatar(1));
    assert_eq!(*writes.lock().unwrap(), 4, "each change is written");
    let wanted = ui.prefs();
    assert_eq!(
        wanted,
        Prefs {
            bubbles: false,
            on_top: false,
            music: false,
            avatar: Some("Hood".into())
        }
    );
    // the next start reads them back
    let (config, _) = Config::load_from(&file);
    let ui = start(Prefs::from(&config));
    assert_eq!(ui.prefs(), wanted);
    // a switch set to what it is writes nothing
    let mut ui = ui;
    ui.update(Message::SetBubbles(false));
    assert_eq!(*writes.lock().unwrap(), 4);
}

#[test]
fn reload_shows_new_avatars_and_open_folder_makes_the_folder_first() {
    let pet = Pet::new("avatars");
    let mut ui = pet.ui;
    let folder = aipet_sprite::Avatar::custom_dir(&pet.scratch.0);
    assert!(!folder.exists());
    let avatars = |message| Message::Settings(settings::Message::Avatars(message));
    // Open folder makes the folder, then opens it
    ui.update(avatars(settings::avatars::Message::OpenFolder));
    assert!(folder.is_dir());
    assert_eq!(pet.platform.calls(), [Call::OpenFolder(folder.clone())]);

    // an avatar put in the folder shows after Reload, and picking it puts it on
    let custom = folder.join("green.json");
    std::fs::write(&custom, include_str!("../../../../avatars/hood-green.json")).unwrap();
    ui.update(Message::SelectAvatar(2));
    assert_eq!(ui.prefs().avatar.as_deref(), Some("Sprout"), "not there before Reload");
    ui.update(avatars(settings::avatars::Message::Reload));
    ui.update(Message::SelectAvatar(2));
    assert_eq!(ui.prefs().avatar.as_deref(), Some("Hood (green)"));

    // the avatar worn, edited under the same name, is worn as it is now once read again
    let read = |name: &str| {
        aipet_sprite::Avatar::all(&folder)
            .into_iter()
            .find(|a| a.name == name)
            .unwrap()
    };
    assert_eq!(settings::avatars::worn(&ui), &read("Hood (green)"));
    let edited = include_str!("../../../../avatars/hood-green.json").replace("#0F4422", "#FF0000");
    assert_ne!(edited, include_str!("../../../../avatars/hood-green.json"));
    std::fs::write(&custom, edited).unwrap();
    assert_ne!(settings::avatars::worn(&ui), &read("Hood (green)"), "not before Reload");
    ui.update(avatars(settings::avatars::Message::Reload));
    assert_eq!(settings::avatars::worn(&ui), &read("Hood (green)"));
    assert_eq!(ui.prefs().avatar.as_deref(), Some("Hood (green)"));

    // the avatar worn stays on when its file goes, with no tile, and the others are as the folder has them
    let worn = settings::avatars::worn(&ui).clone();
    std::fs::remove_file(&custom).unwrap();
    ui.update(avatars(settings::avatars::Message::Reload));
    assert_eq!(ui.prefs().avatar.as_deref(), Some("Hood (green)"));
    assert_eq!(settings::avatars::worn(&ui), &worn);
    let tiles = settings::avatars::tiles(&ui);
    assert!(
        tiles.iter().all(|&(name, worn)| name != "Hood (green)" && !worn),
        "{tiles:?}"
    );
    assert_eq!(
        tiles.iter().map(|&(name, _)| name).collect::<Vec<_>>(),
        aipet_sprite::Avatar::all(&folder)
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
    );
    ui.update(Message::SelectAvatar(1));
    assert_eq!(ui.prefs().avatar.as_deref(), Some("Hood"));
    assert!(settings::avatars::tiles(&ui)[1].1, "its tile is the one worn now");
    ui.update(avatars(settings::avatars::Message::Reload));
    ui.update(Message::SelectAvatar(2));
    assert_eq!(ui.prefs().avatar.as_deref(), Some("Hood"), "its tile went with it");
}

#[test]
fn open_folder_opens_nothing_when_the_folder_cannot_be_made() {
    let mut pet = Pet::new("avatars-blocked");
    // a file where the avatars folder goes
    let folder = aipet_sprite::Avatar::custom_dir(&pet.scratch.0);
    pet.scratch.file("avatars", "not a folder");
    pet.ui.update(Message::Settings(settings::Message::Avatars(
        settings::avatars::Message::OpenFolder,
    )));
    assert_eq!(pet.platform.calls(), []);
    let line = pet
        .ui
        .settings()
        .avatars
        .folder_error
        .clone()
        .expect("a line saying why");
    assert!(
        line.starts_with(&format!("Couldn't make the avatars folder {}: ", folder.display())),
        "{line}"
    );
    // once it can be made, it opens, and the line goes
    std::fs::remove_file(&folder).unwrap();
    pet.ui.update(Message::Settings(settings::Message::Avatars(
        settings::avatars::Message::OpenFolder,
    )));
    assert_eq!(pet.platform.calls(), [Call::OpenFolder(folder)]);
    assert_eq!(pet.ui.settings().avatars.folder_error, None);
}
