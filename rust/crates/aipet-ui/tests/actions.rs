//! The bubbles' buttons and clicks (MainWindow's MakeCard, UpdateChrome and OpenItem), through the pet's own messages:
//! which round buttons each kind of bubble shows while the pointer is on it, where a click on a bubble or a button
//! goes, the tooltips, and that the buttons take the pointer. The platform is the recording fake: nothing opens,
//! plays or comes forward for real, and it has no way to send another app a key.

use std::sync::Arc;
use std::time::{Duration, Instant};

use aipet_core::board::{Board, Media, Sources};
use aipet_core::github::{GitHubSettings, PullRequest};
use aipet_core::jira::{Issue, JiraSettings};
use aipet_core::sessions::Entry;
use aipet_ui::platform::{Call, Recorder};
use aipet_ui::settings::Page;
use aipet_ui::{Button, Effect, Message, PetUi, SPRITE, SURFACE, Setup, Shown};
use iced::Point;

/// The Board's clock.
const NOW: f64 = 1_800_000_000.0;
/// An id a desktop app gives a chat (a GUID), which makes the chat's deep link.
const GUID: &str = "0199c0de-0000-7000-8000-00000000c0de";
const ISSUE: &str = "https://example.atlassian.net/browse/AP-1";
const PR: &str = "https://github.com/me/repo/pull/7";

/// A pet on a Board, with a recording platform whose player is Spotify.
struct Pet {
    ui: PetUi,
    platform: Arc<Recorder>,
    start: Instant,
    t: f64,
}

impl Pet {
    /// The pet once the bubbles of `sources` have slid in.
    fn new(sources: &Sources) -> Pet {
        let platform = Arc::new(Recorder::with_player("Spotify", Media::default()));
        let mut ui = PetUi::new(Setup {
            platform: platform.clone(),
            ..Setup::detached()
        });
        let mut board = Board::new();
        board.refresh(sources, NOW);
        ui.update(Message::Board(Arc::new(board)));
        let mut pet = Pet {
            ui,
            platform,
            start: Instant::now(),
            t: 0.0,
        };
        pet.run(1.5);
        pet
    }

    /// Moves the pet on `secs` at 60 fps.
    fn run(&mut self, secs: f64) {
        let until = self.t + secs;
        while self.t < until {
            self.t += 1.0 / 60.0;
            self.ui.tick(self.start + Duration::from_secs_f64(self.t));
        }
    }

    fn point(&mut self, p: Point) {
        self.ui.update(Message::PointerMoved(p));
    }

    fn shown(&self, id: &str) -> Shown {
        self.ui.shown(id).unwrap_or_else(|| panic!("no bubble {id}"))
    }

    /// Points at the middle of a bubble, and says what it shows then.
    fn hover(&mut self, id: &str) -> Shown {
        let middle = self.shown(id).body.center();
        self.point(middle);
        self.shown(id)
    }

    /// The platform's calls that `act` made.
    fn calls(&mut self, act: impl FnOnce(&mut PetUi) -> Option<Effect>) -> (Option<Effect>, Vec<Call>) {
        let before = self.platform.calls().len();
        let effect = act(&mut self.ui);
        (effect, self.platform.calls()[before..].to_vec())
    }
}

fn chat(id: &str, state: &'static str, place: &str) -> Entry {
    let mut chat = Entry::default();
    chat.id = id.into();
    chat.agent = Some(if id.starts_with("codex") { "codex" } else { "claude" });
    chat.state = Some(state);
    chat.title = Some("Fix the bug".into());
    chat.detail = Some("Running tests".into());
    chat.r#where = Some(place.into());
    chat.has_transcript = true;
    chat.ts = NOW - 5.0;
    chat
}

fn chats(chats: Vec<Entry>) -> Sources {
    Sources {
        chats,
        ..Sources::default()
    }
}

fn issue(key: &str) -> Issue {
    Issue {
        key: key.into(),
        summary: "Port the pet".into(),
        status: "In review".into(),
        updated: NOW - 60.0,
        url: format!("https://example.atlassian.net/browse/{key}"),
    }
}

fn jira() -> JiraSettings {
    JiraSettings {
        enabled: true,
        site: Some("example.atlassian.net".into()),
        ..JiraSettings::default()
    }
}

fn review(keys: &[&str]) -> PullRequest {
    PullRequest {
        repo: "me/repo".into(),
        number: 7,
        title: "Add the thing".into(),
        url: PR.into(),
        updated: NOW - 40.0,
        keys: keys.iter().map(|k| (*k).to_owned()).collect(),
    }
}

fn player(playing: bool) -> Sources {
    Sources {
        media: Some(Media {
            name: "Spotify".into(),
            song: Some("A song".into()),
            artist: "A band".into(),
            playing,
            track_since: NOW - 30.0,
        }),
        music_on: true,
        ..Sources::default()
    }
}

fn open(url: &str) -> Call {
    Call::OpenUrl(url.into())
}

fn focus(agent: &str) -> Call {
    Call::FocusAgent {
        agent: agent.into(),
        link: None,
    }
}

/// Where a click on a bubble goes.
#[derive(Debug, PartialEq)]
enum Opens {
    /// The platform, with these calls.
    Platform(Vec<Call>),
    /// Settings, on this page; the platform isn't called.
    Settings(Page),
}

/// One kind of bubble: the Board's sources that make it, its id, the buttons it shows while the pointer is on it,
/// what each button calls, and where a click on it goes.
struct Kind {
    name: &'static str,
    sources: Sources,
    id: String,
    buttons: Vec<(Button, Call)>,
    click: Opens,
}

fn kinds() -> Vec<Kind> {
    let codex = format!("codex:{GUID}");
    let codex_link = format!("codex://threads/{GUID}");
    vec![
        Kind {
            name: "a working chat in a terminal",
            sources: chats(vec![chat("claude:a", "working", "terminal")]),
            id: "claude:a".into(),
            buttons: vec![],
            click: Opens::Platform(vec![focus("claude")]),
        },
        Kind {
            name: "a finished chat in the Claude app",
            sources: chats(vec![chat("claude:a", "done", "desktop")]),
            id: "claude:a".into(),
            buttons: vec![],
            click: Opens::Platform(vec![focus("claude")]),
        },
        Kind {
            name: "a chat that needs you in the ChatGPT app, with its deep link",
            sources: chats(vec![chat(&codex, "attention", "desktop")]),
            id: codex.clone(),
            buttons: vec![],
            click: Opens::Platform(vec![open(&codex_link)]),
        },
        Kind {
            name: "a Jira issue with its pull request",
            sources: Sources {
                jira: jira(),
                issues: vec![issue("AP-1")],
                pr_for_key: [("AP-1".to_owned(), PR.to_owned())].into(),
                ..Sources::default()
            },
            id: "jira:AP-1".into(),
            buttons: vec![(Button::Ticket, open(ISSUE)), (Button::PullRequest, open(PR))],
            click: Opens::Platform(vec![open(ISSUE)]),
        },
        Kind {
            name: "a Jira issue alone",
            sources: Sources {
                jira: jira(),
                issues: vec![issue("AP-1")],
                ..Sources::default()
            },
            id: "jira:AP-1".into(),
            buttons: vec![(Button::Ticket, open(ISSUE))],
            click: Opens::Platform(vec![open(ISSUE)]),
        },
        Kind {
            name: "a review request that names a Jira issue",
            sources: Sources {
                jira: jira(),
                review_requests: vec![review(&["AP-9"])],
                ..Sources::default()
            },
            id: format!("gh:{PR}"),
            buttons: vec![
                (Button::Ticket, open("https://example.atlassian.net/browse/AP-9")),
                (Button::PullRequest, open(PR)),
            ],
            click: Opens::Platform(vec![open(PR)]),
        },
        Kind {
            name: "a review request alone",
            sources: Sources {
                review_requests: vec![review(&[])],
                ..Sources::default()
            },
            id: format!("gh:{PR}"),
            buttons: vec![(Button::PullRequest, open(PR))],
            click: Opens::Platform(vec![open(PR)]),
        },
        Kind {
            name: "the player",
            sources: player(true),
            id: "music".into(),
            buttons: vec![
                (Button::Previous, Call::Previous),
                (Button::PlayPause, Call::PlayPause),
                (Button::Next, Call::Next),
            ],
            click: Opens::Platform(vec![Call::Focus]),
        },
        Kind {
            name: "Jira's error",
            sources: Sources {
                jira: jira(),
                jira_error: Some("Jira said 401".into()),
                ..Sources::default()
            },
            id: "jira:_error".into(),
            buttons: vec![],
            click: Opens::Settings(Page::Jira),
        },
        Kind {
            name: "GitHub's error",
            sources: Sources {
                github: GitHubSettings {
                    enabled: true,
                    ..GitHubSettings::default()
                },
                github_error: Some("GitHub said 401".into()),
                ..Sources::default()
            },
            id: "gh:_error".into(),
            buttons: vec![],
            click: Opens::Settings(Page::GitHub),
        },
    ]
}

#[test]
fn every_kind_of_bubble_shows_its_buttons_while_the_pointer_is_on_it_and_a_click_opens_its_item() {
    for kind in kinds() {
        let name = kind.name;
        let mut pet = Pet::new(&kind.sources);
        let before = pet.shown(&kind.id);
        assert!(
            before.buttons.is_empty() && before.close.is_none(),
            "{name}: none before the pointer"
        );

        let shown = pet.hover(&kind.id);
        let buttons: Vec<Button> = shown.buttons.iter().map(|(b, _)| *b).collect();
        let expected: Vec<Button> = kind.buttons.iter().map(|(b, _)| *b).collect();
        assert_eq!(buttons, expected, "{name}");
        assert!(shown.close.is_some(), "{name}: the dismiss button");
        for (button, call) in &kind.buttons {
            let id: Arc<str> = kind.id.as_str().into();
            let pressed = pet.calls(|ui| ui.update(Message::Button(id, *button)));
            assert_eq!(pressed, (None, vec![call.clone()]), "{name}: {button:?}");
        }

        pet.ui.update(Message::PointerLeft);
        let after = pet.shown(&kind.id);
        assert!(
            after.buttons.is_empty() && after.close.is_none(),
            "{name}: gone with the pointer"
        );

        let clicked = pet.calls(|ui| ui.update(Message::CardPressed(kind.id.as_str().into())));
        match kind.click {
            Opens::Platform(calls) => assert_eq!(clicked, (None, calls), "{name}"),
            Opens::Settings(page) => {
                assert_eq!(clicked, (Some(Effect::OpenSettings), vec![]), "{name}");
                assert_eq!(pet.ui.settings().page, page, "{name}");
            }
        }
    }
}

#[test]
fn no_bubble_has_a_stop_button_a_busy_chat_in_its_app_shows_open_and_no_key_goes_to_the_app() {
    let codex = format!("codex:{GUID}");
    let mut claude = chat("claude:a", "working", "desktop");
    claude.host_id = Some(format!("local_{GUID}"));
    let busy = [
        (
            claude,
            "claude:a".to_owned(),
            "Open in the Claude app",
            open(&format!("claude://claude.ai/epitaxy/local_{GUID}")),
        ),
        (
            chat(&codex, "thinking", "desktop"),
            codex.clone(),
            "Open in the ChatGPT app",
            open(&format!("codex://threads/{GUID}")),
        ),
        // no deep link: its app comes forward
        (
            chat("claude:b", "working", "desktop"),
            "claude:b".to_owned(),
            "Open in the Claude app",
            focus("claude"),
        ),
    ];
    for (entry, id, tip, opens) in busy {
        let mut pet = Pet::new(&chats(vec![entry]));
        // where the C# had Stop, only Open
        let shown = pet.hover(&id);
        assert_eq!(
            shown.buttons.iter().map(|(b, _)| *b).collect::<Vec<_>>(),
            [Button::Open],
            "{id}"
        );
        let (_, open_button) = shown.buttons[0];
        pet.point(open_button.center());
        pet.run(0.5);
        assert_eq!(pet.ui.tooltip().map(|(text, _)| text), Some(tip), "{id}");
        // Open, and a click on the bubble, open the chat or bring its app forward, and call nothing else
        let pressed = pet.calls(|ui| ui.update(Message::Button(id.as_str().into(), Button::Open)));
        let clicked = pet.calls(|ui| ui.update(Message::CardPressed(id.as_str().into())));
        assert_eq!(
            (pressed, clicked),
            ((None, vec![opens.clone()]), (None, vec![opens])),
            "{id}"
        );
    }
}

#[test]
fn the_buttons_take_the_pointer() {
    let mut claude = chat("claude:a", "working", "desktop");
    claude.title = Some("A chat whose title is far too long to fit in its bubble at all".into());
    let mut sources = player(true);
    sources.chats = vec![claude];
    sources.jira = jira();
    sources.issues = vec![issue("AP-1")];
    sources.pr_for_key = [("AP-1".to_owned(), PR.to_owned())].into();
    let mut pet = Pet::new(&sources);
    for id in ["claude:a", "jira:AP-1", "music"] {
        let shown = pet.hover(id);
        assert!(!shown.buttons.is_empty(), "{id}");
        let rects = pet.ui.hit_rects();
        let taken = |p: Point| rects.iter().any(|r| r.contains(p));
        for (button, r) in &shown.buttons {
            // the middle and the rim of the round button, all round
            let (c, rim) = (r.center(), r.width / 2.0 - 0.5);
            let rim = (0..16).map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 16.0;
                Point::new(c.x + rim * a.cos(), c.y + rim * a.sin())
            });
            assert!(std::iter::once(c).chain(rim).all(taken), "{id}: {button:?} at {r:?}");
            // inside its bubble
            assert!(
                shown.body.contains(r.position()) && shown.body.contains(Point::new(r.x + r.width, r.y + r.height))
            );
        }
    }
}

#[test]
fn a_bubble_the_pointer_is_on_keeps_its_width() {
    // a long title is cut shorter while the pointer is on the bubble (UpdateChrome), but the bubble keeps its width,
    // as the C#'s does, so the pointer stays on it, right to its left edge
    let mut done = chat("claude:a", "done", "desktop");
    done.title = Some("A chat whose title is far too long to fit in its bubble at all".into());
    let mut pet = Pet::new(&chats(vec![done]));
    let wide = pet.shown("claude:a").body;
    let hovered = pet.hover("claude:a").body;
    assert_eq!(hovered, wide);
    let edge = Point::new(wide.x + 3.0, wide.center_y());
    pet.point(edge);
    for _ in 0..6 {
        assert!(pet.shown("claude:a").close.is_some(), "still on it");
        assert_eq!(pet.shown("claude:a").body, wide);
        assert!(pet.ui.hit_rects().iter().any(|r| r.contains(edge)));
        pet.run(1.0 / 60.0);
    }
}

#[test]
fn a_tooltip_opens_below_the_pointer_after_a_moment_and_the_next_one_at_once() {
    let mut sources = chats(vec![chat("claude:a", "done", "desktop")]);
    sources.jira = jira();
    sources.issues = vec![issue("AP-1")];
    sources.pr_for_key = [("AP-1".to_owned(), PR.to_owned())].into();
    let mut pet = Pet::new(&sources);
    let shown = pet.hover("jira:AP-1");
    let [(_, ticket), (_, pr)] = shown.buttons[..] else {
        panic!("{shown:?}");
    };
    let at = pr.center();
    pet.point(at);
    pet.run(0.3);
    assert_eq!(pet.ui.tooltip(), None, "not yet");
    let region = pet.ui.hit_rects();
    pet.run(0.15);
    let (text, r) = pet.ui.tooltip().expect("the pull request's tooltip");
    assert_eq!(text, "Open pull request");
    // 20 px below the pointer, moved back in where it would reach past the surface's side
    assert_eq!((r.x, r.y), (at.x.min(SURFACE.width - r.width), at.y + 20.0));
    assert!(
        r.x + r.width <= SURFACE.width && r.x < at.x,
        "{r:?}: it reached past the side"
    );
    // it is drawn on the surface, and is not in the input region
    let drawn = pet.ui.drawn_rects();
    let covered = |p: Point| drawn.iter().any(|d| d.contains(p));
    assert!(covered(r.position()) && covered(Point::new(r.x + r.width - 1.0, r.y + r.height - 1.0)));
    assert_eq!(pet.ui.hit_rects(), region);

    // the next button's opens at once, where the pointer is
    pet.point(ticket.center());
    assert_eq!(pet.ui.tooltip().map(|(text, _)| text), Some("Open in Jira"));
    // and the dismiss button's
    pet.point(shown.close.unwrap().center());
    assert_eq!(pet.ui.tooltip().map(|(text, _)| text), Some("Dismiss"));

    // a bubble's own says its whole title, and a chat's the app it runs in
    pet.ui.update(Message::PointerLeft);
    pet.run(0.5);
    let middle = pet.shown("claude:a").body.center();
    pet.point(middle);
    pet.run(0.45);
    assert_eq!(
        pet.ui.tooltip().map(|(text, _)| text),
        Some("Fix the bug\nIn the Claude app")
    );
    pet.point(pet.shown("jira:AP-1").body.center());
    assert_eq!(pet.ui.tooltip().map(|(text, _)| text), Some("AP-1 · Port the pet"));

    // the pointer coming onto a tooltip where it reaches past what it belongs to has left that, which closes it and
    // takes it out of what is drawn (and the input region never had it)
    let (_, r) = pet.ui.tooltip().unwrap();
    let region = pet.ui.hit_rects();
    let bodies = [pet.shown("jira:AP-1").body, pet.shown("claude:a").body];
    let off = (0..r.height as i32)
        .rev()
        .flat_map(|y| (0..r.width as i32).map(move |x| Point::new(r.x + x as f32 + 0.5, r.y + y as f32 + 0.5)))
        .find(|p| !bodies.iter().any(|b| b.expand(2.0).contains(*p)) && !region.iter().any(|h| h.contains(*p)))
        .expect("a tooltip that reaches past the bubble");
    pet.point(off);
    assert_eq!(pet.ui.tooltip(), None);
    assert!(!pet.ui.hit_rects().iter().any(|h| h.contains(off)));
    pet.run(0.5);
    assert_eq!(pet.ui.tooltip(), None, "it stays closed");
}

#[test]
fn a_folded_stacks_back_bubbles_show_no_buttons_and_its_first_click_spreads_it() {
    let older = Issue {
        updated: NOW - 120.0,
        ..issue("AP-2")
    };
    let sources = Sources {
        jira: jira(),
        issues: vec![older, issue("AP-1")],
        ..Sources::default()
    };
    let mut pet = Pet::new(&sources);
    // the newer issue is in front; the other peeks out above it, folded
    let (front, back) = ("jira:AP-1", "jira:AP-2");
    let peek = pet.shown(back).body;
    pet.point(Point::new(peek.center_x(), peek.y + 3.0));
    assert!(pet.shown(back).buttons.is_empty() && pet.shown(front).buttons.is_empty());

    let first = pet.calls(|ui| ui.update(Message::CardPressed(front.into())));
    assert_eq!(first, (None, vec![]), "it spreads");
    pet.run(1.0);
    assert_eq!(
        pet.hover(back).buttons.iter().map(|(b, _)| *b).collect::<Vec<_>>(),
        [Button::Ticket]
    );
    let second = pet.calls(|ui| ui.update(Message::CardPressed(front.into())));
    assert_eq!(second, (None, vec![open(ISSUE)]));
}

#[test]
fn the_empty_pets_bubble_brings_claude_forward() {
    let mut pet = Pet::new(&Sources::default());
    // on the sprite: the ghost bubble says how the pet is
    pet.point(Point::new(SPRITE.center_x(), SPRITE.y + 60.0));
    pet.run(0.2);
    assert!(pet.shown("_ghost").buttons.is_empty());
    let clicked = pet.calls(|ui| ui.update(Message::CardPressed("_ghost".into())));
    assert_eq!(clicked, (None, vec![focus("claude")]));
}
