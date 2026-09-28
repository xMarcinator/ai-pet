//! The bubbles ("cards") above the pet: which are shown, how they stack, and how they come and go. A port of
//! MainWindow's SyncCards, LayoutCards and the card part of OnFrame.

use std::borrow::Cow;

use iced::Point;

use crate::geometry::{CARD_H, CENTER_X, CardFrame, HEADER_GAP, STACKS_BOTTOM};

/// The stacks above the pet, from the pet up: chats, then reviews (the C#'s music stack has no bubbles here).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    Chats,
    Reviews,
}

impl Section {
    const ALL: [Section; 2] = [Section::Chats, Section::Reviews];

    fn index(self) -> usize {
        self as usize
    }

    /// The gap a stack keeps above it while it holds bubbles.
    fn gap(self) -> f32 {
        match self {
            Section::Chats => 14.0,
            Section::Reviews => 0.0,
        }
    }
}

/// A bubble as the pet's data (here the demo script) describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Bubble {
    pub id: &'static str,
    pub section: Section,
    /// The chat's state, for the dot's colour and the style: thinking, working, attention, done, idle, review…
    pub state: &'static str,
    pub title: Cow<'static, str>,
    pub detail: Cow<'static, str>,
    /// The border colour of the app the chat runs in.
    pub app_colour: Option<u32>,
}

/// A bubble's text cut to fit, and the bubble's width.
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
    pub id: &'static str,
    pub section: Section,
    pub state: &'static str,
    pub app_colour: Option<u32>,
    /// The text as shown: fitted, and with the thinking dots.
    pub title: String,
    pub detail: String,
    pub width: f32,
    /// What its text was fitted from (the title, the detail and how many thinking dots), to fit again only when that
    /// changes.
    source: (String, String, usize),
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
}

/// The bubble stacks.
#[derive(Default)]
pub struct Stacks {
    cards: Vec<Card>,
    /// The cards' indices in drawing order, back to front.
    order: Vec<usize>,
    expanded: [bool; 2],
    /// Each stack's height (with its gap), easing towards its target.
    height: [f32; 2],
    target: [f32; 2],
}

impl Stacks {
    /// Brings the bubbles in line with `show` (in stack order): new ones come in from 14 px below, missing ones
    /// leave. A spread stack folds again when it is down to one bubble or the pointer has been away a while.
    /// `fit` cuts a title and detail to size and gives the bubble's width.
    pub fn sync(&mut self, show: &[Bubble], t: f64, pointer_away: bool, fit: impl Fn(&str, &str) -> Fitted) {
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
            let dots = match bubble.state {
                "thinking" => 1 + (t * 2.5) as usize % 3,
                _ => 0,
            };
            let card = match self.cards.iter_mut().find(|c| c.id == bubble.id) {
                Some(card) => card,
                None => {
                    self.cards.push(Card {
                        id: bubble.id,
                        section: bubble.section,
                        state: bubble.state,
                        app_colour: bubble.app_colour,
                        title: String::new(),
                        detail: String::new(),
                        width: 0.0,
                        source: (String::new(), String::new(), 0),
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
                    self.cards.last_mut().expect("just pushed")
                }
            };
            card.state = bubble.state;
            card.app_colour = bubble.app_colour;
            if card.source.0 != bubble.title || card.source.1 != bubble.detail || card.source.2 != dots {
                let detail = match dots {
                    0 => bubble.detail.to_string(),
                    n => format!("{}{}", bubble.detail.trim_end_matches(['.', '…']), ".".repeat(n)),
                };
                let fitted = fit(&bubble.title, &detail);
                card.source = (bubble.title.to_string(), bubble.detail.to_string(), dots);
                (card.title, card.detail, card.width) = (fitted.title, fitted.detail, fitted.width);
            }
        }
        self.layout(show);
    }

    /// Sets where each stack wants its bubbles: spread out one above the other, or folded behind the front one
    /// (smaller, fainter, only their tops showing).
    fn layout(&mut self, show: &[Bubble]) {
        let order = |id: &str| show.iter().position(|b| b.id == id).unwrap_or(usize::MAX);
        for section in Section::ALL {
            let open = self.expanded[section.index()];
            let active = |c: &Card| !c.removing && c.section == section;
            let n = self.cards.iter().filter(|c| active(c)).count();
            for k in 0..self.cards.len() {
                if !active(&self.cards[k]) {
                    continue;
                }
                // its place in the stack, from the front: how many of the stack's bubbles come before it in `show`
                let at = order(self.cards[k].id);
                let i = self.cards.iter().filter(|c| active(c) && order(c.id) < at).count();
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
    pub fn expand(&mut self, section: Section, show: &[Bubble]) -> bool {
        let count = self
            .cards
            .iter()
            .filter(|c| !c.removing && c.section == section)
            .count();
        if self.expanded[section.index()] || count <= 1 {
            return false;
        }
        self.expanded = [false; 2];
        self.expanded[section.index()] = true;
        self.layout(show);
        true
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
            (c.target.0 - c.y).abs() > PX || (c.target.1 - c.s).abs() * c.width > PX || (c.target.2 - c.o).abs() > 0.01
        };
        self.cards.iter().any(card) || self.height.iter().zip(self.target).any(|(h, t)| (t - h).abs() > PX)
    }

    /// The bottom edge of a stack: the chats sit on the stacks' bottom, the reviews on top of the chats.
    fn bottom(&self, section: Section) -> f32 {
        match section {
            Section::Chats => STACKS_BOTTOM,
            Section::Reviews => STACKS_BOTTOM - self.height[Section::Chats.index()],
        }
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
                width: c.width,
                scale: c.s,
            };
            (c, frame)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bubble(id: &'static str, section: Section, state: &'static str) -> Bubble {
        Bubble {
            id,
            section,
            state,
            title: id.into(),
            detail: "Doing things…".into(),
            app_colour: None,
        }
    }

    fn fit(title: &str, detail: &str) -> Fitted {
        Fitted {
            title: title.to_owned(),
            detail: detail.to_owned(),
            width: 240.0,
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
        stacks.cards.iter().find(|c| c.id == id)
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
            (stacks.height[0] - 64.0).abs() < 0.1,
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
        assert!((stacks.height[0] - (14.0 + 50.0 + 9.0)).abs() < 0.1);
        // the front bubble is drawn last
        assert_eq!(stacks.frames().map(|(c, _)| c.id).collect::<Vec<_>>(), ["b", "a"]);

        assert!(stacks.expand(Section::Chats, &show));
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
        ];
        stacks.sync(&show[..3], 0.0, false, fit);
        assert!(!stacks.expand(Section::Reviews, &show[..3]));
        stacks.sync(&show, 0.0, false, fit);
        assert!(stacks.expand(Section::Chats, &show) && stacks.expand(Section::Reviews, &show));
        assert_eq!(stacks.expanded, [false, true]);
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
        let r = stacks.frames().find(|(c, _)| c.id == "r").unwrap().1;
        assert!((r.bottom_center.y - (460.0 - 64.0)).abs() < 0.1, "{r:?}");
        assert!((stacks.header_bottom() - (460.0 - 64.0 - 50.0 - 8.0)).abs() < 0.1);
        assert!(stacks.has_reviews());
    }

    #[test]
    fn thinking_dots_count_up_and_text_is_fitted_only_when_it_changes() {
        let mut stacks = Stacks::default();
        let show = [bubble("a", Section::Chats, "thinking")];
        let fits = std::cell::Cell::new(0);
        let counting = |title: &str, detail: &str| {
            fits.set(fits.get() + 1);
            fit(title, detail)
        };
        stacks.sync(&show, 0.0, false, counting);
        assert_eq!(card(&stacks, "a").unwrap().detail, "Doing things.");
        stacks.sync(&show, 0.1, false, counting);
        assert_eq!(fits.get(), 1);
        stacks.sync(&show, 0.8, false, counting);
        assert_eq!(card(&stacks, "a").unwrap().detail, "Doing things...");
        assert_eq!(fits.get(), 2);
    }
}
