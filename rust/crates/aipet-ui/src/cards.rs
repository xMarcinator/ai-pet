//! The bubbles ("cards") above the pet: which are shown, how they stack, how they come and go, and what a bubble the
//! pointer is on shows. A port of MainWindow's SyncCards, LayoutCards, UpdateChrome and the card part of OnFrame.

use std::borrow::Cow;
use std::sync::Arc;

use iced::{Point, Rectangle, Size};

use crate::geometry::{CARD_H, CENTER_X, CardFrame, HEADER_GAP, STACKS_BOTTOM};

/// The stacks above the pet, from the pet up: the music player, chats, then reviews.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    Music,
    Chats,
    Reviews,
}

impl Section {
    const ALL: [Section; 3] = [Section::Music, Section::Chats, Section::Reviews];

    fn index(self) -> usize {
        self as usize
    }

    /// The gap a stack keeps above it while it holds bubbles.
    fn gap(self) -> f32 {
        match self {
            Section::Music | Section::Chats => 14.0,
            Section::Reviews => 0.0,
        }
    }
}

/// A bubble's round buttons (MakeCard's), in the order they stand, left to right. There is no Stop: the Rust pet never
/// sends another app keys (the spec's Decision Context).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Button {
    /// "Open in Jira": a Jira issue, or a pull request that names one.
    Ticket,
    /// "Open pull request".
    PullRequest,
    /// The music player's.
    Previous,
    PlayPause,
    Next,
    /// A working or thinking chat in its agent's desktop app: opens the chat, or brings the app forward. It stands
    /// where the C#'s Stop did, which opened the chat this way whenever it couldn't stop it.
    Open,
}

impl Button {
    const ALL: [Button; 6] = [
        Button::Ticket,
        Button::PullRequest,
        Button::Previous,
        Button::PlayPause,
        Button::Next,
        Button::Open,
    ];

    fn bit(self) -> u8 {
        1 << self as u8
    }

    /// Its tooltip; `app` names the desktop app a chat's Open brings forward.
    pub fn tip(self, app: Option<&str>) -> String {
        match self {
            Button::Ticket => "Open in Jira".into(),
            Button::PullRequest => "Open pull request".into(),
            Button::Previous => "Previous".into(),
            Button::PlayPause => "Play / pause".into(),
            Button::Next => "Next".into(),
            Button::Open => format!("Open in the {}", app.unwrap_or("app")),
        }
    }
}

/// A set of [`Button`]s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons(u8);

/// A round button's size, the margin before each, and the margin before the first (Button.round, MakeCard's Actions).
const BUTTON: f32 = 32.0;
const BUTTON_MARGIN: f32 = 4.0;
const ROW_MARGIN: f32 = 12.0;
/// The buttons' row ends at the bubble's right padding (9) and border (1).
const ROW_END: f32 = 10.0;

impl Buttons {
    /// These, and `button` too when `on`.
    pub fn with(self, button: Button, on: bool) -> Buttons {
        if on { Buttons(self.0 | button.bit()) } else { self }
    }

    pub fn contains(self, button: Button) -> bool {
        self.0 & button.bit() != 0
    }

    /// Left to right.
    pub fn iter(self) -> impl Iterator<Item = Button> {
        Button::ALL.into_iter().filter(move |&b| self.contains(b))
    }

    pub fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// How wide the row of buttons is while it shows, its margin included: a bubble without buttons still keeps the
    /// margin (MakeCard's Actions panel shows, empty).
    pub fn row_width(self) -> f32 {
        ROW_MARGIN + (BUTTON_MARGIN + BUTTON) * self.len() as f32
    }
}

/// A bubble as the Board describes it (a row of SyncCards).
#[derive(Clone, Debug, PartialEq)]
pub struct Bubble {
    pub id: Arc<str>,
    pub section: Section,
    /// What it is, as the Board says: `chat`, `jira`, `jira-error`, `github`, `github-error` or `music`.
    pub kind: &'static str,
    /// Its state, for the dot's colour and the style: thinking, working, attention, done, idle, review…
    pub state: &'static str,
    pub title: Cow<'static, str>,
    pub detail: Cow<'static, str>,
    /// How many bubbles its stack leaves out, said after the detail (and its thinking dots) when it is the stack's
    /// last.
    pub more: usize,
    /// The app a chat runs in: its name and its border colour.
    pub app: Option<(&'static str, u32)>,
    /// The round buttons it shows while the pointer is on it.
    pub buttons: Buttons,
}

/// A bubble's title and detail cut to fit, and the bubble's width.
#[derive(Clone, Debug, PartialEq)]
pub struct Fitted {
    pub title: String,
    pub detail: String,
    pub width: f32,
}

/// Spread out, bubbles sit this far apart; folded, each one peeks out this far above the one in front of it.
const SPREAD: f32 = 58.0;
const PEEK: f32 = 9.0;
/// How long a leaving bubble stays before it is dropped.
const LEAVE: f64 = 0.25;

/// A bubble on screen, easing towards where its stack wants it.
#[derive(Clone, Debug)]
pub struct Card {
    pub id: Arc<str>,
    pub section: Section,
    pub kind: &'static str,
    pub state: &'static str,
    pub app: Option<(&'static str, u32)>,
    pub buttons: Buttons,
    /// What its text is fitted from: the title, and the detail with its thinking dots and how many more there are.
    text: (String, String),
    /// Its text fitted at rest, and beside the buttons it was fitted for (made once the pointer is on it).
    rest: Fitted,
    chrome: Option<(Buttons, Fitted)>,
    /// Whether it shows its buttons and dismiss button: the pointer is on it, it shows its text (the front of a folded
    /// stack, or any bubble of a spread one), and it isn't leaving (UpdateChrome's `interactive`).
    interactive: bool,
    pub born_at: f64,
    /// How far it rises above its stack's bottom (negative: up), its scale and its opacity, now and the target.
    pub y: f32,
    pub s: f32,
    pub o: f32,
    target: (f32, f32, f32),
    pub removing: bool,
    removed_at: f64,
    /// Whether its text shows and it takes the pointer: every bubble of a spread stack, the front one of a folded one.
    pub content: bool,
    /// Drawing order: bubbles further back are drawn first.
    z: i32,
}

impl Card {
    /// Whether it is drawn at all (not once it has faded out).
    pub fn drawn(&self) -> bool {
        self.o >= 0.004
    }

    /// Whether the dot pulses: something is going on.
    pub fn pulsing(&self) -> bool {
        matches!(self.state, "working" | "attention" | "thinking" | "music")
    }

    /// Whether it shows its round buttons and dismiss button, its text fitted beside them.
    pub fn interactive(&self) -> bool {
        self.interactive
    }

    /// Its text as shown, and its width: fitted beside its buttons while it is interactive (the C#'s TextBlocks get
    /// MaxWidth 190 then), else at rest (250).
    pub fn fitted(&self) -> &Fitted {
        match &self.chrome {
            Some((_, fitted)) if self.interactive => fitted,
            _ => &self.rest,
        }
    }

    pub fn width(&self) -> f32 {
        self.fitted().width
    }

    /// How wide it takes the pointer: its width, and while it is interactive at least its width at rest. A hovered
    /// bubble its buttons made narrower would otherwise lose the pointer to the strip it gave up, get wide again,
    /// and flicker.
    pub fn reach(&self) -> f32 {
        if self.interactive {
            self.width().max(self.rest.width)
        } else {
            self.width()
        }
    }

    /// Its whole title, as the Board gives it.
    pub fn title(&self) -> &str {
        &self.text.0
    }

    /// Its round buttons while they show, each with where it is: in a row at the bubble's right end, 32 px round with
    /// 4 px before each, centred top to bottom (MakeCard's Actions, right-aligned by the row's star column).
    pub fn button_rects(&self, frame: &CardFrame) -> impl Iterator<Item = (Button, Rectangle)> {
        let shown = if self.interactive {
            self.buttons
        } else {
            Buttons::default()
        };
        let (n, frame) = (shown.len(), *frame);
        shown.iter().enumerate().map(move |(i, button)| {
            let right = frame.width - ROW_END - (BUTTON_MARGIN + BUTTON) * (n - 1 - i) as f32;
            let top_left = frame.map(Point::new(right - BUTTON, (CARD_H - BUTTON) / 2.0));
            (
                button,
                Rectangle::new(top_left, Size::new(BUTTON, BUTTON) * frame.scale),
            )
        })
    }
}

/// The function that cuts a bubble's title and detail to size and gives its width: at rest (`None`), or beside a row
/// of buttons this wide.
pub trait Fit: Fn(&str, &str, Option<f32>) -> Fitted {}

impl<F: Fn(&str, &str, Option<f32>) -> Fitted> Fit for F {}

/// The bubble stacks.
#[derive(Default)]
pub struct Stacks {
    cards: Vec<Card>,
    /// The cards' indices in drawing order, back to front.
    order: Vec<usize>,
    expanded: [bool; 3],
    /// Each stack's height (with its gap), easing towards its target.
    height: [f32; 3],
    target: [f32; 3],
    /// The bubble the pointer is on.
    hovered: Option<Arc<str>>,
}

impl Stacks {
    /// Brings the bubbles in line with `show` (in stack order): new ones come in from 14 px below, missing ones
    /// leave. A spread stack folds again when it is down to one bubble or the pointer has been away a while.
    pub fn sync(&mut self, show: &[Bubble], t: f64, pointer_away: bool, fit: impl Fit) {
        for section in Section::ALL {
            let count = show.iter().filter(|b| b.section == section).count();
            if count <= 1 || pointer_away {
                self.expanded[section.index()] = false;
            }
        }
        for card in self.cards.iter_mut().filter(|c| !c.removing) {
            if !show.iter().any(|b| b.id == card.id) {
                card.removing = true;
                card.removed_at = t;
                card.target = (card.y + 10.0, card.target.1, 0.0);
            }
        }
        for bubble in show {
            // a bubble coming back while its old self is still leaving starts afresh
            if let Some(i) = self.cards.iter().position(|c| c.id == bubble.id && c.removing) {
                self.cards.remove(i);
            }
            let detail = detail(bubble, t);
            match self.cards.iter_mut().find(|c| c.id == bubble.id) {
                Some(card) => {
                    (card.kind, card.state, card.app, card.buttons) =
                        (bubble.kind, bubble.state, bubble.app, bubble.buttons);
                    if card.text.0 != *bubble.title || card.text.1 != detail {
                        card.rest = fit(&bubble.title, &detail, None);
                        card.chrome = None;
                        card.text = (bubble.title.to_string(), detail);
                    }
                }
                None => {
                    let rest = fit(&bubble.title, &detail, None);
                    self.cards.push(Card {
                        id: Arc::clone(&bubble.id),
                        section: bubble.section,
                        kind: bubble.kind,
                        state: bubble.state,
                        app: bubble.app,
                        buttons: bubble.buttons,
                        text: (bubble.title.to_string(), detail),
                        rest,
                        chrome: None,
                        interactive: false,
                        born_at: t,
                        y: 14.0,
                        s: 1.0,
                        o: 0.0,
                        target: (0.0, 1.0, 1.0),
                        removing: false,
                        removed_at: 0.0,
                        content: true,
                        z: 0,
                    });
                }
            }
        }
        self.layout(show);
        self.chrome(&fit);
    }

    /// Sets the bubble the pointer is on (none when it is on none).
    pub fn hover(&mut self, id: Option<Arc<str>>, fit: impl Fit) {
        if self.hovered != id {
            self.hovered = id;
            self.chrome(&fit);
        }
    }

    /// Sets which bubble is interactive, and fits its text beside its buttons where that changed.
    fn chrome(&mut self, fit: &impl Fit) {
        for card in &mut self.cards {
            card.interactive = self.hovered.as_ref() == Some(&card.id) && card.content && !card.removing;
            if card.interactive && card.chrome.as_ref().is_none_or(|(buttons, _)| *buttons != card.buttons) {
                let fitted = fit(&card.text.0, &card.text.1, Some(card.buttons.row_width()));
                card.chrome = Some((card.buttons, fitted));
            }
        }
    }

    /// Sets where each stack wants its bubbles: spread out one above the other, or folded behind the front one
    /// (smaller, fainter, only their tops showing).
    fn layout(&mut self, show: &[Bubble]) {
        let order = |id: &str| show.iter().position(|b| *b.id == *id).unwrap_or(usize::MAX);
        for section in Section::ALL {
            let open = self.expanded[section.index()];
            let active = |c: &Card| !c.removing && c.section == section;
            let n = self.cards.iter().filter(|c| active(c)).count();
            for k in 0..self.cards.len() {
                if !active(&self.cards[k]) {
                    continue;
                }
                // its place in the stack, from the front: how many of the stack's bubbles come before it in `show`
                let at = order(&self.cards[k].id);
                let i = self.cards.iter().filter(|c| active(c) && order(&c.id) < at).count();
                let card = &mut self.cards[k];
                let f = i as f32;
                card.target = if open {
                    (-f * SPREAD, 1.0, 1.0)
                } else {
                    (-f * PEEK, 1.0 - 0.06 * f, if i < 3 { 1.0 - 0.22 * f } else { 0.0 })
                };
                card.content = open || i == 0;
                card.z = 100 - i as i32;
            }
            self.target[section.index()] = match n {
                0 => 0.0,
                _ if open => section.gap() + CARD_H + (n - 1) as f32 * SPREAD,
                _ => section.gap() + CARD_H + PEEK * (n.min(3) - 1) as f32,
            };
        }
        self.restack();
    }

    /// Sorts `order` into drawing order again: by depth, and in the cards' order at the same depth.
    fn restack(&mut self) {
        let cards = &self.cards;
        self.order.clear();
        self.order.extend(0..cards.len());
        self.order.sort_by_key(|&i| cards[i].z);
    }

    /// Spreads out a folded stack of more than one bubble, and folds the others: two spread stacks are taller than
    /// the room above the pet. Returns whether it spread.
    pub fn expand(&mut self, section: Section, show: &[Bubble], fit: impl Fit) -> bool {
        let count = self
            .cards
            .iter()
            .filter(|c| !c.removing && c.section == section)
            .count();
        if self.expanded[section.index()] || count <= 1 {
            return false;
        }
        self.expanded = [false; 3];
        self.expanded[section.index()] = true;
        self.layout(show);
        self.chrome(&fit);
        true
    }

    /// The stack a bubble is in, staying or leaving.
    pub fn section(&self, id: &str) -> Option<Section> {
        self.cards.iter().find(|c| *c.id == *id).map(|c| c.section)
    }

    /// The bubble with this id that is staying.
    pub fn card(&self, id: &str) -> Option<&Card> {
        self.cards.iter().find(|c| *c.id == *id && !c.removing)
    }

    /// Eases every bubble and stack `dt` seconds towards its target, and drops the bubbles that have left.
    pub fn animate(&mut self, dt: f64, t: f64) {
        let k = (1.0 - (-dt * 13.0).exp()) as f32;
        let fade = (1.0 - (-dt * 16.0).exp()) as f32;
        for card in &mut self.cards {
            card.y += (card.target.0 - card.y) * k;
            card.s += (card.target.1 - card.s) * k;
            card.o += (card.target.2 - card.o) * fade;
        }
        let before = self.cards.len();
        self.cards.retain(|c| !(c.removing && t - c.removed_at > LEAVE));
        if self.cards.len() != before {
            self.restack();
        }
        for (height, target) in self.height.iter_mut().zip(self.target) {
            *height += (target - *height) * k;
        }
    }

    /// Whether a bubble or a stack is still visibly on its way: more than half a px from where it is going, or 1 %
    /// of its opacity.
    pub fn easing(&self) -> bool {
        const PX: f32 = 0.5;
        let card = |c: &Card| {
            (c.target.0 - c.y).abs() > PX
                || (c.target.1 - c.s).abs() * c.width() > PX
                || (c.target.2 - c.o).abs() > 0.01
        };
        self.cards.iter().any(card) || self.height.iter().zip(self.target).any(|(h, t)| (t - h).abs() > PX)
    }

    /// The bottom edge of a stack: the stacks below it, each as high as it is now, sit between it and the stacks'
    /// bottom.
    fn bottom(&self, section: Section) -> f32 {
        let below: f32 = Section::ALL[..section.index()]
            .iter()
            .map(|s| self.height[s.index()])
            .sum();
        STACKS_BOTTOM - below
    }

    /// Where the reviews header's bottom edge is: 8 px above the reviews stack.
    pub fn header_bottom(&self) -> f32 {
        self.bottom(Section::Reviews) - self.height[Section::Reviews.index()] - HEADER_GAP
    }

    /// Whether a review bubble is staying (the header shows while one is).
    pub fn has_reviews(&self) -> bool {
        self.cards.iter().any(|c| !c.removing && c.section == Section::Reviews)
    }

    /// Every bubble with where it is drawn, back to front.
    pub fn frames(&self) -> impl DoubleEndedIterator<Item = (&Card, CardFrame)> {
        self.order.iter().map(|&i| {
            let c = &self.cards[i];
            let frame = CardFrame {
                bottom_center: Point::new(CENTER_X, self.bottom(c.section) + c.y),
                width: c.width(),
                scale: c.s,
            };
            (c, frame)
        })
    }
}

/// A bubble's detail as shown at `t`: a thinking chat's with its dots counting up (one to three, 2.5 a second, in place
/// of its own), then how many more bubbles its stack leaves out.
fn detail(bubble: &Bubble, t: f64) -> String {
    let mut detail = match bubble.state {
        "thinking" => {
            let dots = 1 + (t * 2.5) as usize % 3;
            format!("{}{}", bubble.detail.trim_end_matches(['.', '…']), ".".repeat(dots))
        }
        _ => bubble.detail.to_string(),
    };
    if bubble.more > 0 {
        detail.push_str(&format!("  ·  +{} more", bubble.more));
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bubble(id: &'static str, section: Section, state: &'static str) -> Bubble {
        Bubble {
            id: id.into(),
            section,
            kind: "chat",
            state,
            title: id.into(),
            detail: "Doing things…".into(),
            more: 0,
            app: None,
            buttons: Buttons::default(),
        }
    }

    /// 240 wide at rest; beside the buttons as wide as their row makes it, the text cut to 10 characters.
    fn fit(title: &str, detail: &str, row: Option<f32>) -> Fitted {
        let cut = |s: &str| s.chars().take(if row.is_some() { 10 } else { usize::MAX }).collect();
        Fitted {
            title: cut(title),
            detail: cut(detail),
            width: 240.0 + row.unwrap_or(0.0),
        }
    }

    /// Runs the animation for `secs` at 60 fps.
    fn run(stacks: &mut Stacks, t: &mut f64, secs: f64) {
        for _ in 0..(secs * 60.0) as usize {
            *t += 1.0 / 60.0;
            stacks.animate(1.0 / 60.0, *t);
        }
    }

    fn card<'a>(stacks: &'a Stacks, id: &str) -> Option<&'a Card> {
        stacks.cards.iter().find(|c| *c.id == *id)
    }

    fn frame(stacks: &Stacks, id: &str) -> CardFrame {
        stacks.frames().find(|(c, _)| *c.id == *id).unwrap().1
    }

    #[test]
    fn a_bubble_slides_in_from_below_and_fades_out_upwards() {
        let mut stacks = Stacks::default();
        let mut t = 0.0;
        let show = [bubble("a", Section::Chats, "working")];
        stacks.sync(&show, t, false, fit);
        let a = card(&stacks, "a").unwrap();
        assert_eq!((a.y, a.o), (14.0, 0.0));
        assert!(stacks.easing());
        let (_, frame) = stacks.frames().next().unwrap();
        assert_eq!(frame.bottom_center, Point::new(190.0, 474.0));
        run(&mut stacks, &mut t, 1.0);
        let a = card(&stacks, "a").unwrap();
        assert!(a.y.abs() < 0.01 && (a.o - 1.0).abs() < 0.01, "{a:?}");
        assert!(
            (stacks.height[Section::Chats.index()] - 64.0).abs() < 0.1,
            "the stack is its gap and one bubble high"
        );
        assert!(!stacks.easing(), "it has arrived");

        stacks.sync(&[], t, false, fit);
        assert!(stacks.easing());
        run(&mut stacks, &mut t, 0.1);
        let a = card(&stacks, "a").unwrap();
        assert!(a.removing && a.y > 1.0 && a.o < 0.5, "{a:?}");
        run(&mut stacks, &mut t, 0.2);
        assert!(stacks.cards.is_empty());
    }

    #[test]
    fn a_folded_stack_peeks_and_spreads_when_asked() {
        let mut stacks = Stacks::default();
        let mut t = 0.0;
        let show = [
            bubble("a", Section::Chats, "working"),
            bubble("b", Section::Chats, "idle"),
        ];
        stacks.sync(&show, t, false, fit);
        run(&mut stacks, &mut t, 2.0);
        let b = card(&stacks, "b").unwrap();
        assert!((b.y + 9.0).abs() < 0.01 && (b.s - 0.94).abs() < 0.001 && (b.o - 0.78).abs() < 0.001);
        assert!(card(&stacks, "a").unwrap().content && !b.content);
        assert!((stacks.height[Section::Chats.index()] - (14.0 + 50.0 + 9.0)).abs() < 0.1);
        // the front bubble is drawn last
        assert_eq!(stacks.frames().map(|(c, _)| &*c.id).collect::<Vec<_>>(), ["b", "a"]);

        assert!(stacks.expand(Section::Chats, &show, fit));
        run(&mut stacks, &mut t, 2.0);
        let b = card(&stacks, "b").unwrap();
        assert!((b.y + 58.0).abs() < 0.01 && b.content && (b.o - 1.0).abs() < 0.01);
        // it folds again once the pointer has been away
        stacks.sync(&show, t, true, fit);
        assert!(!stacks.expanded[Section::Chats.index()]);
    }

    #[test]
    fn one_bubble_never_spreads_and_only_one_stack_is_spread() {
        let mut stacks = Stacks::default();
        let show = [
            bubble("a", Section::Chats, "working"),
            bubble("b", Section::Chats, "idle"),
            bubble("r1", Section::Reviews, "review"),
            bubble("r2", Section::Reviews, "review"),
            bubble("music", Section::Music, "music"),
        ];
        stacks.sync(&show[..3], 0.0, false, fit);
        assert!(!stacks.expand(Section::Reviews, &show[..3], fit));
        stacks.sync(&show, 0.0, false, fit);
        assert!(stacks.expand(Section::Chats, &show, fit) && stacks.expand(Section::Reviews, &show, fit));
        assert_eq!(stacks.expanded, [false, false, true]);
        // the player's stack only ever has its one bubble
        assert!(!stacks.expand(Section::Music, &show, fit));
    }

    #[test]
    fn the_music_stack_is_right_above_the_pet_the_chats_above_it_and_the_reviews_on_top() {
        let mut stacks = Stacks::default();
        let mut t = 0.0;
        // the Board's order: chats, reviews, music
        let show = [
            bubble("a", Section::Chats, "working"),
            bubble("r", Section::Reviews, "review"),
            bubble("music", Section::Music, "music"),
        ];
        stacks.sync(&show, t, false, fit);
        run(&mut stacks, &mut t, 2.0);
        let bottom = |id| frame(&stacks, id).bottom_center.y;
        assert!((bottom("music") - 460.0).abs() < 0.1, "on the stacks' bottom");
        assert!(
            (bottom("a") - (460.0 - 64.0)).abs() < 0.1,
            "above music and its 14 px gap"
        );
        assert!(
            (bottom("r") - (460.0 - 64.0 - 64.0)).abs() < 0.1,
            "above the chats and theirs"
        );
        assert!((stacks.header_bottom() - (460.0 - 128.0 - 50.0 - 8.0)).abs() < 0.1);
        // the player's bubble is the front of its own stack, whatever the chats do
        assert!(card(&stacks, "music").unwrap().content);
    }

    #[test]
    fn reviews_stack_on_top_of_the_chats_with_the_header_above() {
        let mut stacks = Stacks::default();
        let mut t = 0.0;
        let show = [
            bubble("a", Section::Chats, "working"),
            bubble("r", Section::Reviews, "review"),
        ];
        stacks.sync(&show, t, false, fit);
        run(&mut stacks, &mut t, 2.0);
        let r = frame(&stacks, "r");
        assert!((r.bottom_center.y - (460.0 - 64.0)).abs() < 0.1, "{r:?}");
        assert!((stacks.header_bottom() - (460.0 - 64.0 - 50.0 - 8.0)).abs() < 0.1);
        assert!(stacks.has_reviews());
    }

    #[test]
    fn thinking_dots_count_up_before_the_more_and_text_is_fitted_only_when_it_changes() {
        let mut stacks = Stacks::default();
        let last = Bubble {
            more: 2,
            ..bubble("a", Section::Chats, "thinking")
        };
        let fits = std::cell::Cell::new(0);
        let counting = |title: &str, detail: &str, row: Option<f32>| {
            fits.set(fits.get() + 1);
            fit(title, detail, row)
        };
        stacks.sync(std::slice::from_ref(&last), 0.0, false, counting);
        assert_eq!(card(&stacks, "a").unwrap().fitted().detail, "Doing things.  ·  +2 more");
        stacks.sync(std::slice::from_ref(&last), 0.1, false, counting);
        assert_eq!(fits.get(), 1);
        stacks.sync(std::slice::from_ref(&last), 0.8, false, counting);
        assert_eq!(
            card(&stacks, "a").unwrap().fitted().detail,
            "Doing things...  ·  +2 more"
        );
        assert_eq!(fits.get(), 2);
    }

    #[test]
    fn the_hovered_bubble_fits_its_text_beside_its_buttons_and_keeps_its_reach() {
        let mut stacks = Stacks::default();
        let mut t = 0.0;
        let two = Buttons::default()
            .with(Button::Ticket, true)
            .with(Button::PullRequest, true);
        let show = [
            Bubble {
                title: "A long title of an issue".into(),
                buttons: two,
                ..bubble("r1", Section::Reviews, "review")
            },
            bubble("r2", Section::Reviews, "review"),
        ];
        stacks.sync(&show, t, false, fit);
        run(&mut stacks, &mut t, 2.0);
        let r1 = card(&stacks, "r1").unwrap();
        assert!(!r1.interactive() && r1.button_rects(&frame(&stacks, "r1")).next().is_none());
        assert_eq!(r1.title(), "A long title of an issue");

        stacks.hover(Some("r1".into()), fit);
        let r1 = card(&stacks, "r1").unwrap();
        assert!(r1.interactive());
        assert_eq!(r1.fitted().title, "A long tit", "fitted beside its buttons");
        assert_eq!(r1.width(), 240.0 + 12.0 + 2.0 * 36.0);
        // in a row at the right end, 4 px apart and centred top to bottom
        let f = frame(&stacks, "r1");
        let body = f.body();
        let buttons: Vec<(Button, Rectangle)> = r1.button_rects(&f).collect();
        assert_eq!(
            buttons,
            [
                (
                    Button::Ticket,
                    Rectangle::new(
                        Point::new(body.x + body.width - 10.0 - 68.0, body.y + 9.0),
                        Size::new(32.0, 32.0)
                    )
                ),
                (
                    Button::PullRequest,
                    Rectangle::new(
                        Point::new(body.x + body.width - 10.0 - 32.0, body.y + 9.0),
                        Size::new(32.0, 32.0)
                    )
                ),
            ]
        );

        // the bubble folded behind it isn't interactive, even under the pointer, until the stack spreads
        stacks.hover(Some("r2".into()), fit);
        assert!(!card(&stacks, "r2").unwrap().interactive() && !card(&stacks, "r1").unwrap().interactive());
        assert!(stacks.expand(Section::Reviews, &show, fit));
        assert!(card(&stacks, "r2").unwrap().interactive());

        // a leaving bubble isn't interactive
        stacks.hover(Some("r1".into()), fit);
        stacks.sync(&[], t, false, fit);
        assert!(!card(&stacks, "r1").unwrap().interactive());

        // a hovered bubble its buttons make narrower still reaches as far as it did at rest
        let narrow = |_: &str, _: &str, row: Option<f32>| Fitted {
            title: String::new(),
            detail: String::new(),
            width: if row.is_some() { 250.0 } else { 300.0 },
        };
        let mut stacks = Stacks::default();
        stacks.sync(&show[..1], 0.0, false, narrow);
        stacks.hover(Some("r1".into()), narrow);
        let r1 = card(&stacks, "r1").unwrap();
        assert_eq!((r1.width(), r1.reach()), (250.0, 300.0));
        stacks.hover(None, narrow);
        let r1 = card(&stacks, "r1").unwrap();
        assert_eq!((r1.width(), r1.reach()), (300.0, 300.0));
    }
}
