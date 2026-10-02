//! The pet's front end, shared by the shells (`aipet-wayland`, `aipet-desktop`): the sprite driven by a clock, the
//! bubbles above it, the menu and Settings, and the geometry both the view and the input region come from.
//!
//! The app makes a [`PetUi`] from a [`Setup`] and hands it to a shell with the core's [`News`] ([`Launch`]). The
//! shell turns each piece of news into a [`Message`], calls [`PetUi::tick`] every [`PetUi::frame_interval`], draws
//! [`PetUi::pet_view`] in a 380 × 600 transparent surface ([`SURFACE`]), and makes [`PetUi::hit_rects`] the surface's
//! input region whenever it changes ([`PetUi::drawn_rects`] is where it is drawn on). Dragging is the shell's (it
//! moves the surface); it reports the drag with [`Message::DragStarted`], [`Message::DragMoved`] and
//! [`Message::DragEnded`] (or [`Message::DragCancelled`]) so the pet dangles, runs, lands, or gets poked. [`SPRITE`]
//! and [`INLINE_MENU`] say where the sprite and the menu are.
//!
//! The bubbles and the mood are the core's [`Board`]'s, as `MainWindow`'s Refresh and SyncCards make them
//! (`src/AiPet.UI/MainWindow.axaml.cs:303-314, 486-564`). A bubble the pointer is on shows its round [`Button`]s and
//! its dismiss button, and a click on it opens its item, as MakeCard, UpdateChrome and OpenItem have it (`:352-485`).
//! What the pet asks of the app (saving its preferences, dismissing a bubble, Settings' actions, its place, quitting)
//! goes to the app's [`Host`]; what it asks of the operating system (opening links, bringing an agent's app forward,
//! the music player) goes to the [`platform::Platform`]. With the `demo` feature, [`PetUi::demo`] plays a scripted day
//! through a Board of its own instead (`demo`).

mod cards;
#[cfg(any(test, feature = "demo"))]
pub mod demo;
mod geometry;
mod menu;
pub mod platform;
pub mod settings;
mod sprite;
mod style;
mod view;

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use aipet_core::board::{self, Board, Session};
use aipet_core::config::Config;
use aipet_sprite::{Avatar, Pet, PetFrame, PetInput};
use iced::widget::image::Handle;
use iced::{Point, Rectangle, Size, Vector};

pub use cards::Button;
use cards::{Bubble, Buttons, Card, Fitted, Section, Stacks};
use geometry::{CARD_RADIUS, CLOSE_SIZE, CardFrame, HALO, HEADER_H, HEADER_RADIUS, MENU_RADIUS, Motion};
pub use geometry::{INLINE_MENU, MENU, Rect, SPRITE, SURFACE};
use platform::Platform;
use settings::{Page, Settings};
use sprite::Sprite;

/// The bubble an empty pet shows while you point at it.
const GHOST: &str = "_ghost";
/// The widest a bubble's title or detail gets (TextBlock.MaxWidth): at rest, and beside the bubble's buttons while the
/// pointer is on it (UpdateChrome).
const TEXT_MAX: f32 = 250.0;
const TEXT_BESIDE: f32 = 190.0;
/// The narrowest and widest a bubble gets (its Border's MinWidth and MaxWidth).
const CARD_MIN: f32 = 240.0;
const CARD_MAX: f32 = 350.0;
/// How long a new review calls for attention (the C#'s NewReviews: `AlertUntil = Now + 6`).
const ALERT: f64 = 6.0;
/// A tooltip (Avalonia 12's ToolTip in the Fluent dark theme) opens 0.4 s after the pointer comes onto what has one, or
/// at once within 0.1 s of the last one closing (ShowDelay, BetweenShowDelay); 20 px below where the pointer then is
/// (Placement Pointer, VerticalOffset 20); and fades in over 0.15 s.
const TIP_DELAY: f64 = 0.4;
const TIP_BETWEEN: f64 = 0.1;
const TIP_BELOW: f32 = 20.0;
const TIP_FADE: f64 = 0.15;
/// A tooltip's text size, and its widest (MaxWidth, its 1 px border and 8, 5, 8, 7 padding included).
const TIP_TEXT: f32 = 12.0;
const TIP_MAX: f32 = 320.0;
/// What a tooltip's border and padding add to its text's size.
const TIP_FRAME: Size = Size::new(2.0 + 8.0 + 8.0, 2.0 + 5.0 + 7.0);

/// [`PetUi::frame_interval`] while something moves fast: the C# draws at the display rate, 60 Hz on most screens.
pub const SMOOTH_FRAME: Duration = Duration::from_nanos(16_666_667);
/// [`PetUi::frame_interval`] while the pet is calm: faster than the sprite's pixels change (24 fps), so none of
/// their frames is skipped even by a timer 8 ms late, and half of 60 Hz, so each frame stays up for two refreshes.
pub const CALM_FRAME: Duration = Duration::from_nanos(33_333_333);
/// The fastest the sprite may move at [`CALM_FRAME`]: 1 px a frame. Its pixels land on whole px, so faster than
/// that it jumps 2 px at times, which shows. At their fastest (Pet.cs' amplitudes and rates), the calm moods stay
/// under it: the bob while asleep (1.5 px/s), idle (3.3), thinking (4.2) and working (19); listening's nod (35), a
/// poke (39), a hop (56), a wiggle (64), done's jump (67), attention's hops (79) and a landing (87) don't.
const CALM_SPEED: f64 = 30.0;
/// How long frames stay smooth after the last fast motion: longer than attention's 0.6 s pause between hops, so a
/// motion that pauses doesn't start again with a coarse step.
const SMOOTH_HOLD: f64 = 1.0;

/// What the pet's views send, and what the shell tells the pet.
#[derive(Clone, Debug)]
pub enum Message {
    /// The core refreshed its Board: the bubbles, the mood and the prop are its.
    Board(Arc<Board>),
    /// A review turned up (Jira's or GitHub's NewReviews): the pet calls for attention for 6 s.
    Alert,
    /// The pointer moved over the pet's surface, to this surface-local point (logical px).
    PointerMoved(Point),
    /// The pointer left the surface.
    PointerLeft,
    /// A left press on the sprite: a drag begins (or a poke, if the pointer doesn't move).
    DragStarted,
    /// The pointer moved during a drag: how far it is from where it was pressed, in screen px.
    DragMoved(Vector),
    /// The button was released.
    DragEnded,
    /// The drag was lost without a release (the surface went away, or the release never came): the pet is let go,
    /// neither poked nor landing. A shell that knows the button went up after all sends [`Message::DragEnded`].
    DragCancelled,
    /// A bubble was clicked: a folded stack of more than one spreads out, else its item opens (an error bubble's
    /// Settings page, a review's page, the player, or a chat).
    CardPressed(Arc<str>),
    /// One of a bubble's round buttons was clicked.
    Button(Arc<str>, Button),
    /// A bubble's dismiss button was clicked.
    Dismiss(Arc<str>),
    /// A menu item was chosen. The shell closes the menu.
    Menu(MenuItem),
    SelectAvatar(usize),
    SetBubbles(bool),
    SetOnTop(bool),
    SetMusic(bool),
    /// Forces the pet's mood, for trying the states out; `None` goes back to the Board's.
    #[cfg(any(test, feature = "demo"))]
    SetMood(Option<&'static str>),
    Settings(settings::Message),
}

/// The menu's items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    Bubbles,
    OnTop,
    Avatars,
    Settings,
    Quit,
}

/// What [`PetUi::update`] asks of the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Keep the pet above other windows, or not.
    OnTop(bool),
    /// Open Settings, or bring it forward, on its page.
    OpenSettings,
    /// Quit: tell the host ([`Host::quit`]), then end.
    Quit,
    /// A drag moved the pet and let it go there: save the window's place ([`Host::save_place`]).
    Dropped,
    /// Settings' Reset position: put the window back in its default corner ([`Host::reset_place`]) and save that
    /// place.
    ResetPosition,
}

/// What the core tells the pet; each shell turns it into a [`Message`].
#[derive(Clone, Debug)]
pub enum News {
    /// The Board after a refresh that changed what the pet shows.
    Board(Arc<Board>),
    /// A review turned up.
    Alert,
}

impl From<News> for Message {
    fn from(news: News) -> Message {
        match news {
            News::Board(board) => Message::Board(board),
            News::Alert => Message::Alert,
        }
    }
}

/// The pet's preferences, as `config.json` keeps them (`Pills`, `OnTop`, `Music`, `Avatar`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub bubbles: bool,
    pub on_top: bool,
    pub music: bool,
    /// The avatar's name; the first avatar when none or an unknown one.
    pub avatar: Option<String>,
}

impl From<&Config> for Prefs {
    fn from(config: &Config) -> Prefs {
        Prefs {
            bubbles: config.pills,
            on_top: config.on_top,
            music: config.music,
            avatar: config.avatar.clone(),
        }
    }
}

impl Default for Prefs {
    /// `config.json`'s defaults.
    fn default() -> Self {
        Prefs::from(&Config::default())
    }
}

/// Where the pet was left, as `config.json` keeps it: the whole window's top-left corner in desktop pixels (`Left`,
/// `Top`), and the height of the window it was saved with (`WindowHeight`, logical px; a file from before the toolbar
/// under the pet went has `Toolbar`, whose row the app takes off it). The shells place the window from it and say it
/// after a drop or a reset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub left: i32,
    pub top: i32,
    pub window_height: f64,
}

/// What the pet needs from the app around it (the C#'s `MainWindow` and `SettingsHost`): its preferences and place
/// kept, bubbles dismissed in the core, Settings' actions done, and the quit. The app's is the `aipet` binary's; a
/// [`Detached`] one keeps nothing.
pub trait Host: Send {
    /// A switch or the avatar changed (the menu, Settings): kept in `config.json`, and the core hears of music.
    fn prefs(&mut self, prefs: &Prefs);

    /// The user dismissed a bubble: the core's Board hides it until it does something new.
    fn dismiss(&mut self, id: &str);

    /// One of Settings' actions for the app: open the data folder, check for updates, or restart to update. True
    /// when the pet should now quit (Restart to update started the update).
    fn settings(&mut self, action: settings::Action) -> bool;

    /// Placement, at start: where the pet was left, or none for its default corner.
    fn saved_place(&mut self) -> Option<Place>;

    /// Placement: the window's place after a drag ended there, or after a reset put it there.
    fn save_place(&mut self, place: Place);

    /// Placement: Reset position was chosen. The shell then puts the window in its default corner and saves that
    /// place.
    fn reset_place(&mut self);

    /// The pet quits: what is still unsaved is written, and an update that is ready installs.
    fn quit(&mut self);
}

/// A host that keeps nothing and does nothing: the demo's, and tests'.
#[derive(Clone, Copy, Debug, Default)]
pub struct Detached;

impl Host for Detached {
    fn prefs(&mut self, _prefs: &Prefs) {}

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

/// What a pet starts with.
pub struct Setup {
    pub host: Box<dyn Host>,
    /// The operating system's services; the desktop crate has each system's.
    pub platform: Arc<dyn Platform>,
    pub prefs: Prefs,
    /// The folder of the user's own avatars (`Avatar::custom_dir` of the data folder), next to the built-in ones.
    pub avatars: PathBuf,
    pub services: settings::Services,
}

impl Setup {
    /// A pet that touches nothing: the [`Detached`] host, no platform, the default preferences with music off, the
    /// built-in avatars only, and [`settings::Services::detached`].
    pub fn detached() -> Setup {
        Setup {
            host: Box::new(Detached),
            platform: Arc::new(platform::NoPlatform),
            prefs: Prefs {
                music: false,
                ..Prefs::default()
            },
            avatars: PathBuf::new(),
            services: settings::Services::detached(PathBuf::from("AiPet")),
        }
    }
}

/// What a shell runs: the pet, and the core's news for it.
pub struct Launch {
    pub ui: PetUi,
    pub news: Receiver<News>,
}

/// The pet left for a shell by [`Launch::hand_over`].
static HANDED_OVER: Mutex<Option<Launch>> = Mutex::new(None);

impl Launch {
    /// Leaves the pet for a shell whose `run` takes no arguments (`aipet_wayland::run`); it takes it with
    /// [`Launch::take`].
    pub fn hand_over(self) {
        *HANDED_OVER.lock().unwrap_or_else(PoisonError::into_inner) = Some(self);
    }

    /// The pet [`Launch::hand_over`] left, once.
    pub fn take() -> Option<Launch> {
        HANDED_OVER.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

/// The pet, its bubbles and its settings.
pub struct PetUi {
    host: Box<dyn Host>,
    platform: Arc<dyn Platform>,
    pet: Pet,
    input: PetInput,
    /// When the pet's clock started (the first tick), and its time now in seconds.
    start: Option<Instant>,
    t: f64,
    motion: Motion,
    sprite: Sprite,
    /// The sprite's part of the input region, remade in place each tick.
    sprite_rects: Vec<Rect>,
    shadow: Handle,
    avatars: Vec<Avatar>,
    /// Each avatar standing still, for the picker.
    stills: Vec<Handle>,
    avatar: usize,
    stacks: Stacks,
    /// The core's last Board, if one came.
    board: Option<Arc<Board>>,
    /// The bubbles shown, in stack order (each tick refills it from the Board).
    show: Vec<Bubble>,
    /// How many reviews wait: the review bubbles shown, and those the stack leaves out.
    waiting: usize,
    /// The reviews header while it shows: how many reviews wait, its text and its width.
    header: Option<(usize, String, f32)>,
    pointer: Option<Point>,
    /// When the pointer left the surface.
    left_at: Option<f64>,
    hover: bool,
    hovered_card: Option<Arc<str>>,
    /// The tooltip of what the pointer is on, and when the last one that showed went.
    tip: Option<Tip>,
    tip_gone_at: Option<f64>,
    /// Where the pointer was at the last drag move that counted, from the press.
    drag_last: Vector,
    bubbles: bool,
    on_top: bool,
    music: bool,
    /// A mood forced from Settings, over the Board's.
    #[cfg(any(test, feature = "demo"))]
    mood: Option<&'static str>,
    /// The demo's script, when it plays in place of the core.
    #[cfg(any(test, feature = "demo"))]
    demo: Option<demo::Demo>,
    inline_menu: bool,
    /// Until when (pet clock) frames stay smooth, after the last fast motion.
    smooth_until: f64,
    /// `AIPET_FPS`: a fixed frame interval.
    fixed_frame: Option<Duration>,
    settings: Settings,
}

impl PetUi {
    /// The pet with its preferences and avatar, waiting for the core's first Board (asleep and without bubbles until
    /// then).
    pub fn new(setup: Setup) -> Self {
        let avatars = Avatar::all(&setup.avatars);
        let avatar = setup
            .prefs
            .avatar
            .as_deref()
            .and_then(|name| avatars.iter().position(|a| a.name == name))
            .unwrap_or(0);
        let stills = avatars.iter().map(sprite::still).collect();
        let mut pet = Pet::new();
        pet.set_avatar(avatars[avatar].clone());
        PetUi {
            host: setup.host,
            platform: setup.platform,
            pet,
            input: PetInput::default(),
            start: None,
            t: 0.0,
            motion: Motion::REST,
            sprite: Sprite::new(),
            sprite_rects: Vec::new(),
            shadow: sprite::shadow_image(),
            avatars,
            stills,
            avatar,
            stacks: Stacks::default(),
            board: None,
            show: Vec::new(),
            waiting: 0,
            header: None,
            pointer: None,
            left_at: None,
            hover: false,
            hovered_card: None,
            tip: None,
            tip_gone_at: None,
            drag_last: Vector::ZERO,
            bubbles: setup.prefs.bubbles,
            on_top: setup.prefs.on_top,
            music: setup.prefs.music,
            #[cfg(any(test, feature = "demo"))]
            mood: None,
            #[cfg(any(test, feature = "demo"))]
            demo: None,
            inline_menu: false,
            smooth_until: 0.0,
            fixed_frame: fixed_frame(),
            settings: Settings::new(setup.services),
        }
    }

    /// The pet playing the demo's scripted day (`demo`), through a Board of its own, in place of the core's: for
    /// development and screenshots.
    #[cfg(any(test, feature = "demo"))]
    pub fn demo(setup: Setup) -> Self {
        PetUi {
            demo: Some(demo::Demo::new()),
            ..PetUi::new(setup)
        }
    }

    /// Moves everything on to `now`: the Board's bubbles and mood, the pet's frame, the bubbles' easing. Call it every
    /// [`PetUi::frame_interval`].
    pub fn tick(&mut self, now: Instant) {
        let start = *self.start.get_or_insert(now);
        let t = now.saturating_duration_since(start).as_secs_f64();
        let elapsed = t - self.t;
        let dt = elapsed.clamp(0.001, 0.05);
        #[cfg(any(test, feature = "demo"))]
        if let Some(demo) = &mut self.demo {
            // a new review calls for attention, as the C#'s NewReviews does
            if demo::review_arrived(self.t, t) {
                self.input.alert_until = t + ALERT;
            }
            self.board = Some(demo.board(t));
        }
        self.t = t;
        settings::tick(self);

        let board = self.board.as_deref();
        let mood = board.map_or("sleep", Board::state);
        #[cfg(any(test, feature = "demo"))]
        let mood = self.mood.unwrap_or(mood);
        if self.input.state != mood {
            self.input.state = mood.to_owned();
            self.input.state_since = t;
        }
        let prop = board.and_then(Board::prop);
        if self.input.prop.as_deref() != prop {
            self.input.prop = prop.map(str::to_owned);
        }
        self.input.hover = self.hover;
        self.input.music = self.music && self.playing();
        let glow = self.pet.avatar().glow;
        let frame = self.pet.update(&self.input, t);
        if frame.pixels_changed {
            self.sprite.update(&frame, glow);
        }
        let motion = motion(&frame);
        let shift = motion.shift(self.motion);
        self.motion = motion;
        geometry::sprite_rects(self.motion, &self.sprite.runs, &mut self.sprite_rects);

        self.sync_cards();
        self.stacks.animate(dt, t);
        if f64::from(shift) > CALM_SPEED * elapsed || self.stacks.easing() {
            self.smooth_until = t + SMOOTH_HOLD;
        }
        // the pet and the bubbles moved: what is under a still pointer may have changed
        self.track_pointer();
    }

    /// Whether the pet bops along: the player plays (the Board's music bubble says so). The demo has no player: there
    /// "Listen along" alone does it.
    fn playing(&self) -> bool {
        #[cfg(any(test, feature = "demo"))]
        if self.demo.is_some() {
            return true;
        }
        self.board
            .as_ref()
            .and_then(|board| board.find("music"))
            .is_some_and(|music| music.eff == "music")
    }

    /// Brings the bubbles on screen in line with the Board's (SyncCards).
    fn sync_cards(&mut self) {
        self.show.clear();
        self.waiting = 0;
        if self.bubbles {
            if let Some(board) = self.board.clone() {
                self.fill(&board);
            }
            if self.show.is_empty() && self.hover && !self.input.dragging {
                // an empty pet you point at says how it is, as the C#'s ghost bubble (a chat card without a chat)
                let asleep = self.input.state == "sleep";
                self.show.push(Bubble {
                    id: GHOST.into(),
                    section: Section::Chats,
                    kind: "chat",
                    state: if asleep { "sleep" } else { "idle" },
                    title: "Claude Code".into(),
                    detail: if asleep { "Napping" } else { "All caught up" }.into(),
                    more: 0,
                    app: None,
                    buttons: Buttons::default(),
                });
            }
        }

        let away = self.left_at.is_some_and(|at| self.t - at > 2.5);
        self.stacks.sync(&self.show, self.t, away, fit_card);

        let waiting = self.waiting;
        self.header = match self.header.take() {
            _ if !self.stacks.has_reviews() => None,
            Some(header) if header.0 == waiting => Some(header),
            _ => {
                // Jira's search may find your own issues rather than reviews, so the count doesn't call them reviews
                let text = match waiting {
                    0 => "Jira and GitHub".to_owned(),
                    n => format!("{n} waiting on you"),
                };
                // padding 10 + dot 6 + spacing 6 + text + padding 10, and the border
                let width = style::text_width(&text, 11.0, style::UI_SEMIBOLD).ceil() + 34.0;
                Some((waiting, text, width))
            }
        };
    }

    /// The Board's bubbles, in its order: each chat with its app, whose name comes before the detail unless every chat
    /// is in the Claude app (or Claude Code); the last of a stack says how many more there are; and each with the
    /// buttons it shows while the pointer is on it.
    fn fill(&mut self, board: &Board) {
        let cards = board.cards();
        let apps: HashSet<&str> = cards
            .iter()
            .filter(|s| s.kind == "chat")
            .map(|s| board::app_of(s).0)
            .collect();
        let several_apps = apps.len() > 1;
        for (i, s) in cards.iter().enumerate() {
            let app = (s.kind == "chat").then(|| board::app_of(s));
            let detail = match app {
                Some((label, _)) if several_apps || !matches!(label, "Claude app" | "Claude") => {
                    format!("{label} · {}", s.detail)
                }
                _ => s.detail.clone(),
            };
            let last = !cards[i + 1..].iter().any(|x| x.section() == s.section());
            self.show.push(Bubble {
                id: s.id.as_str().into(),
                section: section_of(s),
                kind: s.kind,
                state: s.eff,
                title: Cow::Owned(s.name.clone()),
                detail: Cow::Owned(detail),
                more: if last { board.extra(s.section()) } else { 0 },
                app,
                buttons: buttons(s),
            });
        }
        self.waiting = cards.iter().filter(|s| matches!(s.kind, "jira" | "github")).count() + board.extra("reviews");
    }

    /// Works out what the pointer is over: the sprite (a wave when the pointer arrives, the eyes follow it), the bubble
    /// that shows its buttons and dismiss button, and the part of it whose tooltip opens.
    fn track_pointer(&mut self) {
        let over_sprite = self.pointer.is_some_and(|p| self.sprite_hit(p));
        if over_sprite && !self.hover {
            if !self.input.dragging && self.t > self.input.wave_until + 3.0 {
                self.input.wave_until = self.t + 1.4;
            }
            self.hover = true;
        } else if !over_sprite && self.hover && !self.input.dragging {
            self.hover = false;
        }
        self.input.mouse = match self.pointer {
            Some(p) if self.hover || self.input.dragging => {
                let q = self.motion.to_sprite(p);
                Some((q.x as f64, q.y as f64))
            }
            _ => None,
        };
        let hovered = self.hovered_card.take();
        // the menu drawn inline covers the bubbles under it
        let on_menu = |p: &Point| self.inline_menu && INLINE_MENU.contains(*p);
        self.hovered_card = self.pointer.filter(|p| !on_menu(p)).and_then(|p| {
            let front_to_back = self.stacks.frames().rev();
            front_to_back
                .filter(|(c, _)| c.content && !c.removing)
                .find(|(c, f)| {
                    let held = hovered.as_ref() == Some(&c.id);
                    reach(c, f).body().contains(p) || (held && f.close().contains(p))
                })
                .map(|(c, _)| Arc::clone(&c.id))
        });
        self.stacks.hover(self.hovered_card.clone(), fit_card);
        let on = self.hovered_card.clone().zip(self.pointer).and_then(|(id, p)| {
            let (card, frame) = self.stacks.frames().find(|(c, _)| c.id == id && !c.removing)?;
            Some((id, part_at(card, &frame, p)))
        });
        self.follow_tip(on);
    }

    /// Follows what the pointer is on (`on`) with its tooltip, and opens the tooltip once it is due.
    fn follow_tip(&mut self, on: Option<(Arc<str>, Part)>) {
        if self.tip.as_ref().map(|tip| &tip.on) != on.as_ref() {
            if self.tip.take().is_some_and(|tip| tip.shown.is_some()) {
                self.tip_gone_at = Some(self.t);
            }
            self.tip = on.map(|on| Tip {
                on,
                since: self.t,
                shown: None,
            });
        }
        let Some(tip) = &self.tip else {
            return;
        };
        let soon = self.tip_gone_at.is_some_and(|gone| tip.since - gone <= TIP_BETWEEN);
        let due = self.t - tip.since >= if soon { 0.0 } else { TIP_DELAY };
        if tip.shown.is_some() || !due {
            return;
        }
        let (Some(text), Some(p)) = (self.tip_text(&tip.on), self.pointer) else {
            return;
        };
        let rect = tip_rect(&text, p);
        let at = self.t;
        if let Some(tip) = &mut self.tip {
            tip.shown = Some(OpenTip { text, rect, at });
        }
    }

    /// The tooltip of a bubble's part: a round button's, the dismiss button's, or the bubble's own (its whole title,
    /// and for a chat the app it runs in).
    fn tip_text(&self, (id, part): &(Arc<str>, Part)) -> Option<String> {
        let card = self.stacks.card(id)?;
        let app = card.app.map(|(name, _)| name);
        Some(match part {
            Part::Button(button) => button.tip(app),
            Part::Close => "Dismiss".to_owned(),
            Part::Body => match app {
                Some(app) => format!("{}\nIn the {app}", card.title()),
                None => card.title().to_owned(),
            },
        })
    }

    /// Handles a message from the pet's views, the shell or the core, and says what the shell should do, if anything.
    pub fn update(&mut self, message: Message) -> Option<Effect> {
        match message {
            Message::Board(board) => self.board = Some(board),
            Message::Alert => self.input.alert_until = self.t + ALERT,
            Message::PointerMoved(p) => {
                self.pointer = Some(p);
                self.left_at = None;
                self.track_pointer();
            }
            Message::PointerLeft => {
                self.pointer = None;
                self.left_at = Some(self.t);
                self.track_pointer();
            }
            Message::DragStarted => {
                self.input.dragging = true;
                self.input.moved = false;
                self.drag_last = Vector::ZERO;
            }
            Message::DragMoved(from_press) => {
                if !self.input.dragging {
                    return None;
                }
                // a few px of wobble is still a click
                if !self.input.moved && from_press.x.abs() + from_press.y.abs() > 4.0 {
                    self.input.moved = true;
                }
                if self.input.moved {
                    let step = from_press - self.drag_last;
                    if step.x.abs() >= 2.0 {
                        self.input.facing = if step.x > 0.0 { 1 } else { -1 };
                        self.input.last_move = self.t;
                    } else if step.y.abs() > 2.0 {
                        self.input.last_move = self.t;
                    }
                    self.drag_last = from_press;
                }
            }
            Message::DragEnded | Message::DragCancelled => {
                if !self.input.dragging {
                    return None;
                }
                // a release lands a drag (and its place is saved) or pokes a click; a lost drag just lets go
                let released = matches!(message, Message::DragEnded);
                let dropped = released && self.input.moved;
                if dropped {
                    self.input.land_until = self.t + 0.45;
                } else if released {
                    self.input.poke_until = self.t + 1.2;
                }
                self.input.dragging = false;
                self.input.moved = false;
                self.track_pointer();
                return dropped.then_some(Effect::Dropped);
            }
            Message::CardPressed(id) => {
                // a folded stack of more than one spreads out on the first click; after that a click opens the item
                let section = self.stacks.section(&id)?;
                if !self.stacks.expand(section, &self.show, fit_card) {
                    return self.open_item(&id);
                }
            }
            Message::Button(id, button) => return self.press(&id, button),
            Message::Dismiss(id) => self.dismiss(&id),
            Message::Menu(item) => match item {
                MenuItem::Bubbles => {
                    self.bubbles = !self.bubbles;
                    self.save_prefs();
                }
                MenuItem::OnTop => return self.set_on_top(!self.on_top),
                MenuItem::Avatars => return Some(self.open_settings(Page::Avatars)),
                MenuItem::Settings => return Some(self.open_settings(Page::General)),
                MenuItem::Quit => return Some(Effect::Quit),
            },
            Message::SelectAvatar(i) => {
                if let Some(avatar) = self.avatars.get(i).filter(|_| i != self.avatar) {
                    self.avatar = i;
                    self.pet.set_avatar(avatar.clone());
                    self.save_prefs();
                }
            }
            Message::SetBubbles(on) => {
                if self.bubbles != on {
                    self.bubbles = on;
                    self.save_prefs();
                }
            }
            Message::SetOnTop(on) if on != self.on_top => return self.set_on_top(on),
            Message::SetOnTop(_) => {}
            Message::SetMusic(on) => {
                if self.music != on {
                    self.music = on;
                    self.save_prefs();
                }
            }
            #[cfg(any(test, feature = "demo"))]
            Message::SetMood(mood) => self.mood = mood,
            Message::Settings(message) => return settings::update(self, message),
        }
        None
    }

    /// Hides a bubble until it does something new: the core's Board does (the demo's own, in the demo), and the next
    /// Board leaves it out. The ghost isn't the Board's: it stays.
    fn dismiss(&mut self, id: &str) {
        if id == GHOST {
            return;
        }
        #[cfg(any(test, feature = "demo"))]
        if let Some(demo) = &mut self.demo {
            demo.dismiss(id, self.t);
            return;
        }
        self.host.dismiss(id);
    }

    /// Opens a bubble's item (OpenItem): an error bubble's Settings page, the player, a review's page, or a chat: its
    /// deep link opens it in its app, else its agent's app is brought forward. A bubble the Board doesn't have (the
    /// ghost) brings Claude forward.
    fn open_item(&mut self, id: &str) -> Option<Effect> {
        let s = self.board.as_deref().and_then(|board| board.find(id));
        match s.map(|s| (s.kind, s.url.as_deref())) {
            Some(("jira-error", _)) => return Some(self.open_settings(Page::Jira)),
            Some(("github-error", _)) => return Some(self.open_settings(Page::GitHub)),
            Some(("music", _)) => {
                if let Some(player) = self.platform.media() {
                    player.focus();
                }
            }
            Some(("jira" | "github", Some(url))) => self.platform.open_url(url),
            _ => match s.and_then(|s| s.link.as_deref()) {
                Some(link) => self.platform.open_url(link),
                None => self.platform.focus_agent(s.map_or("claude", |s| s.agent), None),
            },
        }
        None
    }

    /// Does what a bubble's round button is for (MakeCard's clicks): opens its ticket or its pull request, or works the
    /// player; a chat's Open opens the chat as a click on the bubble does.
    fn press(&mut self, id: &str, button: Button) -> Option<Effect> {
        let s = self.board.as_deref().and_then(|board| board.find(id));
        let player = self.platform.media();
        match button {
            Button::Ticket => {
                if let Some(url) = s.and_then(|s| s.ticket_url.as_deref()) {
                    self.platform.open_url(url);
                }
            }
            Button::PullRequest => {
                if let Some(url) = s.and_then(|s| s.pr_url.as_deref()) {
                    self.platform.open_url(url);
                }
            }
            Button::Previous => {
                if let Some(player) = player {
                    player.previous();
                }
            }
            Button::PlayPause => {
                if let Some(player) = player {
                    player.play_pause();
                }
            }
            Button::Next => {
                if let Some(player) = player {
                    player.next();
                }
            }
            Button::Open => return self.open_item(id),
        }
        None
    }

    fn set_on_top(&mut self, on: bool) -> Option<Effect> {
        self.on_top = on;
        self.save_prefs();
        Some(Effect::OnTop(on))
    }

    fn open_settings(&mut self, page: Page) -> Effect {
        self.settings.page = page;
        Effect::OpenSettings
    }

    /// The preferences now.
    pub fn prefs(&self) -> Prefs {
        Prefs {
            bubbles: self.bubbles,
            on_top: self.on_top,
            music: self.music,
            avatar: Some(self.avatars[self.avatar].name.clone()),
        }
    }

    fn save_prefs(&mut self) {
        let prefs = self.prefs();
        self.host.prefs(&prefs);
    }

    /// The app's hooks, for what only the shell can do: place the window at start, save its place after a drop or a
    /// reset, and quit.
    pub fn host(&mut self) -> &mut dyn Host {
        &mut *self.host
    }

    /// Settings' state: its page, its services and each page's own.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The operating system's services.
    pub fn platform(&self) -> &dyn Platform {
        &*self.platform
    }

    /// The music player's name, where the platform has one: the one its last poll heard, read anew each time.
    pub fn player(&self) -> Option<String> {
        self.platform.media().map(|player| player.name())
    }

    /// The pet window's whole size (logical px), which its saved place is the top-left corner of.
    pub fn window_size(&self) -> Size {
        SURFACE
    }

    /// Where the pet's surface takes the mouse, in whole logical px: the sprite (its hit ellipse's box and its body),
    /// every bubble that is visible (its round buttons are inside it; with its dismiss button while that shows), the
    /// reviews header, and the menu while it is open inline. Everything else clicks through, shadows, glows and tooltips
    /// too (on Windows, whose window region takes the mouse wherever it draws, the shell lets the mouse through while
    /// the pointer is outside this). It changes as things move; hand it to the surface whenever it does.
    pub fn hit_rects(&self) -> Vec<Rect> {
        let mut rects = Vec::with_capacity(64);
        rects.extend_from_slice(&self.sprite_rects);
        for (card, frame) in self.stacks.frames() {
            if card.o <= 0.05 {
                continue;
            }
            geometry::rounded(reach(card, &frame).body(), CARD_RADIUS * frame.scale, &mut rects);
            if card.interactive() {
                geometry::rounded(frame.close(), CLOSE_SIZE / 2.0 * frame.scale, &mut rects);
            }
        }
        if let Some(header) = self.header_rect() {
            geometry::rounded(header, HEADER_RADIUS, &mut rects);
        }
        if self.inline_menu {
            geometry::rounded(INLINE_MENU, MENU_RADIUS, &mut rects);
        }
        rects
    }

    /// Where the pet's surface is drawn on, in whole logical px like [`PetUi::hit_rects`] (every one of which lies
    /// in one of these): the sprite with its glow's halo, its ground shadow where its lift puts it, every bubble
    /// drawn with its shadow or glow (and its dismiss button while that shows, and as far as it takes the pointer),
    /// the reviews header, the menu while it is open inline, and a tooltip while one shows. The rest of the surface
    /// is transparent, so a region that clips drawing as well (X11's bounding shape, a Windows window region) can be
    /// exactly this. It changes as things move.
    pub fn drawn_rects(&self) -> Vec<Rect> {
        let mut rects = Vec::with_capacity(16);
        let mut add = |r: Rectangle| rects.extend(Rect::covering(r));
        add(self.motion.rect_to_surface(HALO));
        add(self.motion.shadow());
        for (card, frame) in self.stacks.frames() {
            if !card.drawn() {
                continue;
            }
            add(geometry::shadowed(frame.body(), &self.card_shadow(card, frame.scale)));
            if card.reach() > card.width() {
                add(reach(card, &frame).body());
            }
            if card.interactive() {
                add(frame.close());
            }
        }
        if let Some(header) = self.header_rect() {
            add(header);
        }
        if self.inline_menu {
            add(INLINE_MENU);
        }
        if let Some(tip) = self.open_tip() {
            add(tip.rect);
        }
        rects
    }

    /// For tests: a bubble that is staying, as it is now: where its body is, and the dismiss button and round buttons
    /// it shows (none unless the pointer is on it).
    #[doc(hidden)]
    pub fn shown(&self, id: &str) -> Option<Shown> {
        let (card, frame) = self.stacks.frames().find(|(c, _)| *c.id == *id && !c.removing)?;
        Some(Shown {
            body: frame.body(),
            close: card.interactive().then(|| frame.close()),
            buttons: card.button_rects(&frame).collect(),
        })
    }

    /// For tests: the tooltip that shows, its text and where it is.
    #[doc(hidden)]
    pub fn tooltip(&self) -> Option<(&str, Rectangle)> {
        self.open_tip().map(|tip| (tip.text.as_str(), tip.rect))
    }

    /// The tooltip, once it has opened.
    fn open_tip(&self) -> Option<&OpenTip> {
        self.tip.as_ref()?.shown.as_ref()
    }

    /// How long the shell may wait before the next [`PetUi::tick`], and so before drawing again: [`SMOOTH_FRAME`]
    /// while the pointer is on the pet's surface (the pet, a bubble, the menu), during a drag, and for a second after
    /// anything moved faster than a calm frame shows smoothly (a hop, a poke, a landing, attention's hops, a bubble
    /// coming, going, spreading or folding); [`CALM_FRAME`] otherwise (asleep, idle, thinking, working). The pointer
    /// changes it at once, so read it after every update. `AIPET_FPS=<n>` (1 to 1000) fixes it at 1/n s, for
    /// experiments.
    pub fn frame_interval(&self) -> Duration {
        if let Some(fixed) = self.fixed_frame {
            return fixed;
        }
        let lively = self.pointer.is_some() || self.input.dragging || self.t < self.smooth_until;
        if lively { SMOOTH_FRAME } else { CALM_FRAME }
    }

    /// Whether a surface point is on the sprite (as the input region has it).
    pub fn sprite_hit(&self, p: Point) -> bool {
        self.sprite_rects.iter().any(|r| r.contains(p))
    }

    /// The pointer's last position on the surface, while it is on it.
    pub fn pointer(&self) -> Option<Point> {
        self.pointer
    }

    /// Whether the current drag has moved far enough to be a drag rather than a click.
    pub fn drag_moved(&self) -> bool {
        self.input.dragging && self.input.moved
    }

    /// Whether the pet should stay above other windows.
    pub fn on_top(&self) -> bool {
        self.on_top
    }

    /// Draws the menu inside the pet's surface, above the sprite (for shells without popup surfaces), or stops.
    pub fn set_inline_menu(&mut self, open: bool) {
        self.inline_menu = open;
        // a bubble's dismiss button under it goes at once
        self.track_pointer();
    }

    fn header_rect(&self) -> Option<Rectangle> {
        let (_, _, width) = self.header.as_ref().filter(|_| self.bubbles)?;
        let bottom = self.stacks.header_bottom();
        Some(Rectangle::new(
            Point::new(geometry::CENTER_X - width / 2.0, bottom - HEADER_H),
            Size::new(*width, HEADER_H),
        ))
    }

    /// The avatar's accent colour, which the menu's checks and Settings' highlights use.
    fn accent(&self) -> iced::Color {
        style::argb(self.pet.avatar().accent | 0xFF000000)
    }
}

/// What a test sees of a bubble ([`PetUi::shown`]).
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub body: Rectangle,
    pub close: Option<Rectangle>,
    pub buttons: Vec<(Button, Rectangle)>,
}

/// What of a bubble the pointer is on, for the tooltip: its body, its dismiss button, or one of its round buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Body,
    Close,
    Button(Button),
}

/// The tooltip of what the pointer is on: since when it has been on it, and the tooltip once it opened.
struct Tip {
    on: (Arc<str>, Part),
    since: f64,
    shown: Option<OpenTip>,
}

/// A tooltip that shows: its text, where it is, and when it opened (it fades in from then).
struct OpenTip {
    text: String,
    rect: Rectangle,
    at: f64,
}

/// The stack a Board bubble goes in: the music player's, the chats' or the reviews'.
fn section_of(s: &Session) -> Section {
    match s.section() {
        "reviews" => Section::Reviews,
        "music" => Section::Music,
        _ => Section::Chats,
    }
}

/// The round buttons a bubble shows while the pointer is on it (UpdateChrome): Open in Jira where it has a ticket,
/// the pull request's where it has one, the player's three on the player's, and Open on a working or thinking chat in
/// its agent's desktop app, where the C#'s Stop stood.
fn buttons(s: &Session) -> Buttons {
    let music = s.kind == "music";
    let busy_in_app =
        s.kind == "chat" && matches!(s.eff, "working" | "thinking") && s.r#where.as_deref() == Some("desktop");
    Buttons::default()
        .with(Button::Ticket, s.ticket_url.is_some())
        .with(Button::PullRequest, s.pr_url.is_some())
        .with(Button::Previous, music)
        .with(Button::PlayPause, music)
        .with(Button::Next, music)
        .with(Button::Open, busy_in_app)
}

/// Where a bubble takes the pointer: its frame as wide as its reach ([`Card::reach`]).
fn reach(card: &Card, frame: &CardFrame) -> CardFrame {
    CardFrame {
        width: card.reach(),
        ..*frame
    }
}

/// What of a bubble the pointer at `p` is on: its dismiss button or a round button while they show, else the bubble.
fn part_at(card: &Card, frame: &CardFrame, p: Point) -> Part {
    if card.interactive() && frame.close().contains(p) {
        return Part::Close;
    }
    card.button_rects(frame)
        .find(|(_, r)| r.contains(p))
        .map_or(Part::Body, |(button, _)| Part::Button(button))
}

/// Where a tooltip with `text` opens for the pointer at `p`: 20 px below it, moved back in where it would reach past
/// the surface's side, and above the pointer where there is no room below.
fn tip_rect(text: &str, p: Point) -> Rectangle {
    let size = tip_size(text);
    let x = p.x.min(SURFACE.width - size.width).max(0.0);
    let below = p.y + TIP_BELOW;
    let y = if below + size.height <= SURFACE.height {
        below
    } else {
        (p.y - size.height).max(0.0)
    };
    Rectangle::new(Point::new(x, y), size)
}

/// A tooltip's size: its text wrapped to its widest, and its border and padding around it.
fn tip_size(text: &str) -> Size {
    use iced::advanced::graphics::text::Paragraph;
    use iced::advanced::text::{Alignment, Paragraph as _, Shaping, Text, Wrapping};

    let bounds = Paragraph::with_text(Text {
        content: text,
        bounds: Size::new(TIP_MAX - TIP_FRAME.width, f32::INFINITY),
        size: iced::Pixels(TIP_TEXT),
        line_height: style::LINE_HEIGHT,
        font: style::UI,
        align_x: Alignment::Left,
        align_y: iced::alignment::Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::Word,
    })
    .min_bounds();
    Size::new(
        bounds.width.ceil() + TIP_FRAME.width,
        bounds.height.ceil() + TIP_FRAME.height,
    )
}

/// A frame's motion, in the view's f32.
fn motion(frame: &PetFrame) -> Motion {
    Motion {
        offset: Vector::new(frame.x as f32, frame.y as f32),
        scale: Vector::new(frame.scale_x as f32, frame.scale_y as f32),
        shadow_scale: frame.shadow_scale as f32,
        shadow_opacity: frame.shadow_opacity as f32,
    }
}

/// `AIPET_FPS`, when it is set: a fixed frame interval of 1/n s.
fn fixed_frame() -> Option<Duration> {
    let value = std::env::var("AIPET_FPS").ok()?;
    let frame = parse_fps(&value);
    if frame.is_none() {
        eprintln!("aipet: AIPET_FPS must be a whole number from 1 to 1000, not {value:?}; it is ignored");
    }
    frame
}

/// The frame interval for `n` frames a second, from 1 to 1000.
fn parse_fps(n: &str) -> Option<Duration> {
    let n: u32 = n.trim().parse().ok().filter(|n| (1..=1000).contains(n))?;
    Some(Duration::from_secs(1) / n)
}

/// A bubble's title and detail cut to fit, and its width (MakeCard's layout): its padding (14 + 9), 1 px borders, the
/// dot's 18 + 8 and the longer text, from 240 to 350. At rest (`row` none) the text gets up to [`TEXT_MAX`]. Beside a
/// row of buttons `row` px wide it gets up to [`TEXT_BESIDE`], or less where the bubble would be wider than 350.
fn fit_card(title: &str, detail: &str, row: Option<f32>) -> Fitted {
    const FRAME: f32 = 14.0 + 9.0 + 2.0 + 18.0 + 8.0;
    let (max, row) = match row {
        None => (TEXT_MAX, 0.0),
        Some(row) => (TEXT_BESIDE.min(CARD_MAX - FRAME - row), row),
    };
    let (title, title_w) = style::fit(title, 13.5, style::UI_SEMIBOLD, max);
    let (detail, detail_w) = style::fit(detail, 12.0, style::UI, max);
    let width = (FRAME + title_w.max(detail_w).ceil() + row).clamp(CARD_MIN, CARD_MAX);
    Fitted { title, detail, width }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::demo::{CLAUDE, PERIOD, REVIEW};
    use aipet_core::board::{Media, Sources};
    use aipet_core::github::PullRequest;
    use aipet_core::jira::{Issue, JiraSettings};
    use aipet_core::sessions::Entry;
    use aipet_sprite::XorShift64;
    use std::time::Duration;

    /// A host that notes what the pet asks of it.
    #[derive(Clone, Default)]
    pub(crate) struct Host(Arc<Mutex<Asked>>);

    #[derive(Default)]
    pub(crate) struct Asked {
        prefs: Vec<Prefs>,
        dismissed: Vec<String>,
        actions: Vec<settings::Action>,
        quit_on_restart: bool,
    }

    impl Host {
        fn asked(&self) -> std::sync::MutexGuard<'_, Asked> {
            self.0.lock().unwrap()
        }

        pub(crate) fn settings_actions(&self) -> Vec<settings::Action> {
            self.asked().actions.clone()
        }

        /// Whether Restart to update starts an update (and so the pet quits).
        pub(crate) fn quit_on_restart(&self, quit: bool) {
            self.asked().quit_on_restart = quit;
        }

        fn prefs(&self) -> Vec<Prefs> {
            self.asked().prefs.clone()
        }

        fn dismissed(&self) -> Vec<String> {
            self.asked().dismissed.clone()
        }
    }

    impl super::Host for Host {
        fn prefs(&mut self, prefs: &Prefs) {
            self.asked().prefs.push(prefs.clone());
        }

        fn dismiss(&mut self, id: &str) {
            self.asked().dismissed.push(id.to_owned());
        }

        fn settings(&mut self, action: settings::Action) -> bool {
            let mut asked = self.asked();
            asked.actions.push(action);
            action == settings::Action::RestartToUpdate && asked.quit_on_restart
        }

        fn saved_place(&mut self) -> Option<Place> {
            None
        }

        fn save_place(&mut self, _place: Place) {}

        fn reset_place(&mut self) {}

        fn quit(&mut self) {}
    }

    /// A pet fed by tests (no demo), with a host that notes what it was asked.
    pub(crate) fn detached() -> (PetUi, Host) {
        let host = Host::default();
        let ui = PetUi::new(Setup {
            host: Box::new(host.clone()),
            ..Setup::detached()
        });
        (ui, host)
    }

    /// A pet playing the demo's script.
    fn demo() -> PetUi {
        PetUi::demo(Setup::detached())
    }

    /// A pet ticked for `secs` at 60 fps from a fixed start.
    fn run(ui: &mut PetUi, start: Instant, from: f64, secs: f64) {
        for i in 0..=(secs * 60.0) as u32 {
            ui.tick(start + Duration::from_secs_f64(from + i as f64 / 60.0));
        }
    }

    fn shows(ui: &PetUi, id: &str) -> bool {
        ui.show.iter().any(|b| *b.id == *id)
    }

    // ------------------------------------------------------------------ the Board's bubbles and mood

    /// A hook chat.
    fn chat(id: &str, state: &'static str, title: &str, detail: &str, place: &str, ts: f64) -> Entry {
        let mut chat = Entry::default();
        chat.id = id.into();
        chat.agent = Some(if id.starts_with("codex") { "codex" } else { "claude" });
        chat.state = Some(state);
        chat.title = Some(title.into());
        chat.detail = Some(detail.into());
        chat.r#where = Some(place.into());
        chat.has_transcript = true;
        chat.ts = ts;
        chat
    }

    const NOW: f64 = 1_800_000_000.0;

    fn board(sources: &Sources) -> Arc<Board> {
        let mut board = Board::new();
        board.refresh(sources, NOW);
        Arc::new(board)
    }

    /// The pet after it got `board` and ticked once.
    fn shown(board: Arc<Board>) -> (PetUi, Host) {
        let (mut ui, host) = detached();
        ui.update(Message::Board(board));
        ui.tick(Instant::now());
        (ui, host)
    }

    fn bubble<'a>(ui: &'a PetUi, id: &str) -> &'a Bubble {
        ui.show
            .iter()
            .find(|b| *b.id == *id)
            .unwrap_or_else(|| panic!("no {id}: {:?}", ui.show))
    }

    #[test]
    fn board_chats_become_bubbles_with_their_apps() {
        // one app: the Claude app needs no label, Claude Code in a terminal does
        let alone = Sources {
            chats: vec![chat(
                "claude:a",
                "working",
                "Fix the bug",
                "Running tests",
                "desktop",
                NOW - 5.0,
            )],
            ..Sources::default()
        };
        let (ui, _) = shown(board(&alone));
        let a = bubble(&ui, "claude:a");
        assert_eq!(
            (a.section, a.state, a.app),
            (Section::Chats, "working", Some(("Claude app", 0xFFD97757)))
        );
        assert_eq!((a.title.as_ref(), a.detail.as_ref()), ("Fix the bug", "Running tests"));

        let two = Sources {
            chats: vec![
                chat(
                    "claude:a",
                    "attention",
                    "Fix the bug",
                    "Needs your permission",
                    "desktop",
                    NOW - 5.0,
                ),
                chat(
                    "codex:b",
                    "thinking",
                    "Port the hook",
                    "Thinking",
                    "terminal",
                    NOW - 3.0,
                ),
            ],
            ..Sources::default()
        };
        let (ui, _) = shown(board(&two));
        // the Board's order: the one that needs you first
        assert_eq!(
            ui.show.iter().map(|b| &*b.id).collect::<Vec<_>>(),
            ["claude:a", "codex:b"]
        );
        assert_eq!(bubble(&ui, "claude:a").detail, "Claude app · Needs your permission");
        let b = bubble(&ui, "codex:b");
        assert_eq!((b.state, b.app), ("thinking", Some(("Codex CLI", 0xFFA3A3AD))));
        assert_eq!(b.detail, "Codex CLI · Thinking");
    }

    #[test]
    fn reviews_errors_music_and_what_the_caps_leave_out_become_bubbles() {
        let issue = |key: &str, updated: f64| Issue {
            key: key.into(),
            summary: format!("Summary of {key}"),
            status: "In review".into(),
            updated,
            url: format!("https://example.atlassian.net/browse/{key}"),
        };
        let sources = Sources {
            chats: (0..5)
                .map(|i| {
                    chat(
                        &format!("claude:{i}"),
                        "working",
                        "A chat",
                        "Working",
                        "desktop",
                        NOW - f64::from(i),
                    )
                })
                .collect(),
            jira: JiraSettings {
                enabled: true,
                site: Some("example.atlassian.net".into()),
                ..JiraSettings::default()
            },
            issues: vec![issue("AP-1", NOW - 60.0), issue("AP-2", NOW - 50.0)],
            jira_error: Some("Jira said 401".into()),
            review_requests: vec![PullRequest {
                repo: "me/repo".into(),
                number: 7,
                title: "Add the thing".into(),
                url: "https://github.com/me/repo/pull/7".into(),
                updated: NOW - 40.0,
                keys: Vec::new(),
            }],
            media: Some(Media {
                name: "Spotify".into(),
                song: Some("A song".into()),
                artist: "A band".into(),
                playing: true,
                track_since: NOW - 30.0,
            }),
            music_on: true,
            ..Sources::default()
        };
        let (ui, _) = shown(board(&sources));
        let sections: Vec<(&str, Section)> = ui.show.iter().map(|b| (&*b.id, b.section)).collect();
        // four chats, the last saying there's one more; the error first in the reviews, then the newest reviews
        assert_eq!(
            sections,
            [
                ("claude:0", Section::Chats),
                ("claude:1", Section::Chats),
                ("claude:2", Section::Chats),
                ("claude:3", Section::Chats),
                ("jira:_error", Section::Reviews),
                ("gh:https://github.com/me/repo/pull/7", Section::Reviews),
                ("jira:AP-2", Section::Reviews),
                ("jira:AP-1", Section::Reviews),
                ("music", Section::Music),
            ]
        );
        // the stack's last says how many more there are (after its thinking dots, if it has them)
        let last = bubble(&ui, "claude:3");
        assert_eq!((last.detail.as_ref(), last.more), ("Working", 1));
        assert_eq!(bubble(&ui, "claude:2").more, 0);
        let shown = |id| ui.stacks.card(id).unwrap().fitted().detail.clone();
        assert_eq!(shown("claude:3"), "Working  ·  +1 more");
        assert_eq!(shown("claude:2"), "Working");
        let error = bubble(&ui, "jira:_error");
        assert_eq!(
            (error.state, error.detail.as_ref()),
            ("error", "Jira said 401. Click to fix")
        );
        let review = bubble(&ui, "jira:AP-2");
        assert_eq!(
            (review.state, review.title.as_ref()),
            ("review", "AP-2 · Summary of AP-2")
        );
        assert_eq!(review.app, None);
        let music = bubble(&ui, "music");
        assert_eq!(
            (music.state, music.title.as_ref(), music.detail.as_ref()),
            ("music", "A song", "♫ A band")
        );
        // the header counts the reviews, not the error
        assert_eq!(ui.waiting, 3);
        assert_eq!(ui.header.as_ref().map(|h| h.1.as_str()), Some("3 waiting on you"));
    }

    #[test]
    fn the_mood_and_the_prop_are_the_boards() {
        let (mut ui, _) = detached();
        let start = Instant::now();
        ui.tick(start);
        assert_eq!(
            (ui.input.state.as_str(), ui.input.prop.as_deref()),
            ("sleep", None),
            "before any Board"
        );
        let mut working = chat("claude:a", "working", "A chat", "Editing", "desktop", NOW - 5.0);
        working.prop = Some("laptop");
        ui.update(Message::Board(board(&Sources {
            chats: vec![working],
            ..Sources::default()
        })));
        run(&mut ui, start, 0.5, 0.1);
        assert_eq!(
            (ui.input.state.as_str(), ui.input.prop.as_deref()),
            ("working", Some("laptop"))
        );
        assert!((ui.input.state_since - 0.5).abs() < 0.02, "{}", ui.input.state_since);
        // a Board without chats: asleep, empty-handed
        ui.update(Message::Board(board(&Sources::default())));
        run(&mut ui, start, 1.0, 0.1);
        assert_eq!((ui.input.state.as_str(), ui.input.prop.as_deref()), ("sleep", None));
        assert!(ui.show.is_empty());
    }

    #[test]
    fn an_alert_calls_for_attention_for_six_seconds() {
        let (mut ui, _) = detached();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 2.0);
        ui.update(Message::Alert);
        assert!((ui.input.alert_until - 8.0).abs() < 0.02, "{}", ui.input.alert_until);
        run(&mut ui, start, 2.0, 0.5);
        assert_eq!(ui.pet.anim_state(), "attention");
        // news from the core becomes the same messages
        let alert = Message::from(News::Alert);
        assert!(matches!(alert, Message::Alert));
    }

    #[test]
    fn the_pet_bops_along_only_while_the_player_plays_and_music_is_on() {
        let playing = |on: bool| Sources {
            media: Some(Media {
                name: "Spotify".into(),
                song: Some("A song".into()),
                playing: on,
                track_since: NOW,
                ..Media::default()
            }),
            music_on: true,
            ..Sources::default()
        };
        let (mut ui, _) = shown(board(&playing(true)));
        assert!(!ui.input.music, "Listen along is off");
        ui.update(Message::SetMusic(true));
        ui.tick(Instant::now());
        assert!(ui.input.music);
        ui.update(Message::Board(board(&playing(false))));
        ui.tick(Instant::now());
        assert!(!ui.input.music, "paused");
    }

    #[test]
    fn settings_names_the_player_the_last_poll_heard() {
        let vlc = Media {
            name: "Vlc".into(),
            song: Some("A song".into()),
            playing: true,
            track_since: NOW,
            ..Media::default()
        };
        let platform = Arc::new(platform::Recorder::with_player("Spotify", vlc));
        let ui = PetUi::new(Setup {
            platform: platform.clone(),
            ..Setup::detached()
        });
        let listen = |ui: &PetUi| ui.player().map(|player| format!("Listen along with {player}"));
        assert_eq!(listen(&ui).as_deref(), Some("Listen along with Spotify"));
        // the core polls the player; Settings, drawn after, names the one heard
        platform.media().expect("a player").poll();
        assert_eq!(listen(&ui).as_deref(), Some("Listen along with Vlc"));
    }

    #[test]
    fn hidden_bubbles_show_nothing_and_a_dismissal_goes_to_the_core() {
        let sources = Sources {
            chats: vec![chat("claude:a", "working", "A chat", "Editing", "desktop", NOW - 5.0)],
            ..Sources::default()
        };
        let (mut ui, host) = shown(board(&sources));
        ui.update(Message::Dismiss(bubble(&ui, "claude:a").id.clone()));
        assert_eq!(host.dismissed(), ["claude:a"]);
        // the bubble goes with the core's next Board, not before
        ui.tick(Instant::now());
        assert!(shows(&ui, "claude:a"));
        ui.update(Message::Dismiss(GHOST.into()));
        assert_eq!(host.dismissed(), ["claude:a"], "the ghost isn't the core's");

        ui.update(Message::SetBubbles(false));
        ui.tick(Instant::now());
        assert!(ui.show.is_empty() && ui.header.is_none());
    }

    // ------------------------------------------------------------------ preferences and the shell's part

    #[test]
    fn the_preferences_come_from_the_setup_and_every_change_goes_to_the_host() {
        let host = Host::default();
        let mut ui = PetUi::new(Setup {
            host: Box::new(host.clone()),
            prefs: Prefs {
                bubbles: false,
                on_top: false,
                music: true,
                avatar: Some("Hood".into()),
            },
            ..Setup::detached()
        });
        assert_eq!(ui.avatars[ui.avatar].name, "Hood");
        assert_eq!(
            ui.prefs(),
            Prefs {
                bubbles: false,
                on_top: false,
                music: true,
                avatar: Some("Hood".into())
            }
        );
        assert_eq!(ui.update(Message::SetOnTop(true)), Some(Effect::OnTop(true)));
        assert_eq!(ui.update(Message::SetOnTop(true)), None, "no change");
        ui.update(Message::Menu(MenuItem::Bubbles));
        ui.update(Message::SelectAvatar(0));
        let saved = host.prefs();
        assert_eq!(saved.len(), 3);
        assert_eq!(
            saved.last(),
            Some(&Prefs {
                bubbles: true,
                on_top: true,
                music: true,
                avatar: Some("Sprout".into())
            })
        );
        // an avatar it doesn't know: the first
        let ui = PetUi::new(Setup {
            prefs: Prefs {
                avatar: Some("Nobody".into()),
                ..Prefs::default()
            },
            ..Setup::detached()
        });
        assert_eq!(ui.avatar, 0);
    }

    #[test]
    fn menu_items_toggle_or_ask_the_shell() {
        let (mut ui, _) = detached();
        assert_eq!(ui.update(Message::Menu(MenuItem::Bubbles)), None);
        assert!(!ui.bubbles);
        assert_eq!(ui.update(Message::Menu(MenuItem::OnTop)), Some(Effect::OnTop(false)));
        assert_eq!(ui.update(Message::Menu(MenuItem::Avatars)), Some(Effect::OpenSettings));
        assert_eq!(ui.settings.page, Page::Avatars);
        assert_eq!(ui.update(Message::Menu(MenuItem::Settings)), Some(Effect::OpenSettings));
        assert_eq!(ui.settings.page, Page::General);
        assert_eq!(ui.update(Message::Menu(MenuItem::Quit)), Some(Effect::Quit));
        ui.set_inline_menu(true);
        assert!(ui.hit_rects().iter().any(|r| r.y < INLINE_MENU.y as i32 + 10));
        assert_eq!(ui.window_size(), SURFACE);
    }

    #[test]
    fn launch_hands_the_pet_over_once() {
        let (ui, _) = detached();
        let (_sender, news) = std::sync::mpsc::channel();
        Launch { ui, news }.hand_over();
        assert!(Launch::take().is_some());
        assert!(Launch::take().is_none());
    }

    // ------------------------------------------------------------------ the pet itself, on the demo's script

    #[test]
    fn the_input_region_holds_the_sprite_and_every_visible_bubble() {
        let mut ui = demo();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 1.0);
        let rects = ui.hit_rects();
        assert!(!rects.is_empty());
        // at 1 s there are no bubbles: everything is around the sprite
        assert!(
            rects.iter().all(|r| r.y >= 440 && r.x >= 100 && r.x + r.width <= 280),
            "{rects:?}"
        );
        assert!(ui.sprite_hit(Point::new(190.0, 530.0)), "the middle of the pet");
        assert!(!ui.sprite_hit(Point::new(20.0, 20.0)));
        // at 15 s Claude, Codex and a review show (and the header), all above the pet
        run(&mut ui, start, 1.0, 14.0);
        let rects = ui.hit_rects();
        assert!(rects.iter().any(|r| r.y < 440), "bubbles are in the region");
        assert!(
            rects
                .iter()
                .all(|r| r.x >= 0 && r.y >= 0 && r.x + r.width <= 380 && r.y + r.height <= 600)
        );
    }

    #[test]
    fn pointing_at_the_pet_hovers_it_waves_and_follows_the_eyes() {
        let mut ui = demo();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 4.0);
        ui.update(Message::PointerMoved(Point::new(190.0, 530.0)));
        assert!(ui.hover && ui.input.wave_until > ui.t);
        assert!(ui.input.mouse.is_some());
        ui.update(Message::PointerMoved(Point::new(10.0, 10.0)));
        assert!(!ui.hover && ui.input.mouse.is_none());
        // back within 3 s of the wave's end: no second wave
        let waved = ui.input.wave_until;
        ui.update(Message::PointerMoved(Point::new(190.0, 530.0)));
        assert_eq!(ui.input.wave_until, waved);
    }

    #[test]
    fn a_click_pokes_and_a_drag_lands_and_asks_for_its_place_to_be_saved() {
        let mut ui = demo();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 1.0);
        ui.update(Message::DragStarted);
        ui.update(Message::DragMoved(Vector::new(2.0, 1.0)));
        assert!(!ui.drag_moved());
        assert_eq!(ui.update(Message::DragEnded), None);
        assert!(ui.input.poke_until > ui.t && ui.input.land_until < ui.t);

        ui.update(Message::DragStarted);
        ui.update(Message::DragMoved(Vector::new(-10.0, 0.0)));
        assert!(ui.drag_moved() && ui.input.facing == -1);
        assert_eq!(ui.update(Message::DragEnded), Some(Effect::Dropped));
        assert!(ui.input.land_until > ui.t && !ui.input.dragging);
    }

    #[test]
    fn a_dismissed_bubble_comes_back_when_it_does_something_new() {
        let mut ui = demo();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 3.0);
        assert!(shows(&ui, CLAUDE));
        ui.update(Message::Dismiss(bubble(&ui, CLAUDE).id.clone()));
        // still thinking: it stays away (though its thinking dots count on)
        run(&mut ui, start, 3.0, 2.5);
        assert!(!shows(&ui, CLAUDE));
        // at 6 s it works: that's new
        run(&mut ui, start, 5.5, 1.0);
        assert!(shows(&ui, CLAUDE));

        // the review doesn't change: it stays away until it goes, and is back when it comes again
        run(&mut ui, start, 6.5, 8.0);
        ui.update(Message::Dismiss(bubble(&ui, REVIEW).id.clone()));
        run(&mut ui, start, 14.5, 16.0);
        assert!(!shows(&ui, REVIEW));
        run(&mut ui, start, 30.5, 1.0);
        assert!(ui.board.as_ref().unwrap().dismissed().is_empty(), "it went at 31 s");
        run(&mut ui, start, PERIOD + 13.0, 0.5);
        assert!(shows(&ui, REVIEW));
    }

    #[test]
    fn an_empty_pet_shows_the_ghost_even_when_its_bubbles_were_dismissed() {
        let mut ui = demo();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 3.2);
        ui.update(Message::Dismiss(bubble(&ui, CLAUDE).id.clone()));
        run(&mut ui, start, 3.2, 1.3);
        ui.update(Message::PointerMoved(Point::new(190.0, 530.0)));
        run(&mut ui, start, 4.5, 1.0);
        assert!(ui.sprite_hit(Point::new(190.0, 530.0)));
        assert_eq!(ui.show.iter().map(|b| &*b.id).collect::<Vec<_>>(), [GHOST]);
        // dismissing the ghost doesn't keep it away (as in the C#, whose board never has it)
        ui.update(Message::Dismiss(GHOST.into()));
        run(&mut ui, start, 5.5, 0.1);
        assert!(shows(&ui, GHOST));
    }

    #[test]
    fn the_demos_new_review_calls_for_attention_for_six_seconds() {
        let mut ui = demo();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 12.9);
        assert_eq!(ui.input.alert_until, 0.0);
        run(&mut ui, start, 12.9, 1.0);
        assert!((ui.input.alert_until - 19.0).abs() < 0.02, "{}", ui.input.alert_until);
        // working, but the pet shows the alert
        assert_eq!((ui.input.state.as_str(), ui.pet.anim_state()), ("working", "attention"));
    }

    #[test]
    fn a_lost_drag_lets_go_without_a_poke_or_a_landing() {
        let mut ui = demo();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 1.0);
        ui.update(Message::DragStarted);
        ui.update(Message::DragMoved(Vector::new(-10.0, 0.0)));
        assert_eq!(ui.update(Message::DragCancelled), None);
        assert!(!ui.input.dragging && !ui.drag_moved());
        assert!(ui.input.poke_until < ui.t && ui.input.land_until < ui.t);
    }

    #[test]
    fn the_inline_menu_hides_the_bubble_under_it_from_the_pointer() {
        let mut ui = demo();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 3.5);
        // on the Claude bubble, where the menu covers it
        let p = Point::new(190.0, 440.0);
        assert!(INLINE_MENU.contains(p));
        ui.update(Message::PointerMoved(p));
        assert_eq!(ui.hovered_card.as_deref(), Some(CLAUDE));
        let with_badge = ui.hit_rects().len();
        ui.set_inline_menu(true);
        assert_eq!(ui.hovered_card, None, "no dismiss button beside the menu");
        ui.set_inline_menu(false);
        assert_eq!(ui.hit_rects().len(), with_badge);
    }

    /// Ticks the pet from `from` to `until` s the way a shell following [`PetUi::frame_interval`] does, and gives
    /// the time of each tick and the interval it asked for next.
    fn paced(ui: &mut PetUi, start: Instant, from: f64, until: f64) -> Vec<(f64, Duration)> {
        let mut at = from;
        let mut ticks = Vec::new();
        while at < until {
            ui.tick(start + Duration::from_secs_f64(at));
            let next = ui.frame_interval();
            ticks.push((at, next));
            at += next.as_secs_f64();
        }
        ticks
    }

    fn all(ticks: &[(f64, Duration)], from: f64, until: f64, frame: Duration) -> bool {
        ticks
            .iter()
            .filter(|(t, _)| (from..until).contains(t))
            .all(|&(_, next)| next == frame)
    }

    #[test]
    fn frames_are_smooth_while_things_move_fast_and_calm_otherwise() {
        let mut ui = demo();
        ui.fixed_frame = None;
        let start = Instant::now();
        // a calm frame comes before the sprite's pixels (24 fps) could change twice
        assert!(CALM_FRAME < Duration::from_secs(1) / 24 && SMOOTH_FRAME < CALM_FRAME);
        let ticks = paced(&mut ui, start, 0.0, 30.0);
        // asleep, thinking and working are calm
        assert!(all(&ticks, 1.1, 2.0, CALM_FRAME), "asleep");
        assert!(all(&ticks, 4.0, 5.9, CALM_FRAME), "thinking");
        assert!(all(&ticks, 11.0, 12.9, CALM_FRAME), "working");
        // Claude's bubble slides in at 2 s (and the pet wakes): smooth for a while
        assert!(all(&ticks, 2.0, 2.5, SMOOTH_FRAME), "a bubble coming");
        // the review's alert, then Claude needing you: the pet hops all along
        assert!(all(&ticks, 13.1, 22.0, SMOOTH_FRAME), "attention");
        // the script's busy first 30 s: about 13 s of it calm
        let calm: Duration = ticks
            .iter()
            .map(|&(_, next)| next)
            .filter(|&next| next == CALM_FRAME)
            .sum();
        assert!(calm > Duration::from_secs(10), "{calm:?} calm");

        // pointing at the pet, or dragging it, is smooth at once, and calm again when it's over
        let t = ui.t;
        let (point, leave) = (Message::PointerMoved(Point::new(190.0, 530.0)), Message::PointerLeft);
        ui.update(point);
        assert_eq!(ui.frame_interval(), SMOOTH_FRAME);
        ui.update(leave);
        ui.update(Message::DragStarted);
        assert_eq!(ui.frame_interval(), SMOOTH_FRAME);
        ui.update(Message::DragCancelled);
        paced(&mut ui, start, t, t + SMOOTH_HOLD + 1.5);
        // at 31–32 s only Claude's done bubble is left, and the pet has landed from done's jump
        assert_eq!(ui.frame_interval(), CALM_FRAME);
    }

    #[test]
    fn a_poke_and_a_hop_are_smooth() {
        let mut ui = demo();
        ui.fixed_frame = None;
        let start = Instant::now();
        paced(&mut ui, start, 0.0, 1.5);
        assert_eq!(ui.frame_interval(), CALM_FRAME);
        ui.update(Message::DragStarted);
        ui.update(Message::DragEnded);
        let ticks = paced(&mut ui, start, 1.5, 2.6);
        assert!(all(&ticks, 1.5, 2.6, SMOOTH_FRAME), "the poke's jump, 1.2 s");

        // an idle hop: 16 px up and down in 0.9 s. The pet's whims are seeded, so the hop comes at the same time on
        // every run, and it is looked at a calm frame after it starts, by when it has moved: on its first frame it
        // hasn't yet, which a check right then would take for a calm pet
        let mut ui = demo();
        ui.pet = Pet::with_rng(XorShift64::new(15));
        ui.fixed_frame = None;
        ui.mood = Some("idle");
        let mut at = 0.0;
        while ui.pet.idle_act() != Some("hop") {
            assert!(at < 600.0, "no hop in ten minutes");
            let ticks = paced(&mut ui, start, at, at + CALM_FRAME.as_secs_f64());
            at = ticks.last().map_or(at, |&(t, next)| t + next.as_secs_f64());
        }
        paced(&mut ui, start, at, at + 2.0 * CALM_FRAME.as_secs_f64());
        assert_eq!(ui.pet.idle_act(), Some("hop"));
        assert_eq!(ui.frame_interval(), SMOOTH_FRAME);
    }

    #[test]
    fn aipet_fps_fixes_the_frame_interval() {
        assert_eq!(parse_fps("30"), Some(CALM_FRAME));
        assert_eq!(parse_fps(" 1000 "), Some(Duration::from_millis(1)));
        assert_eq!(parse_fps("0"), None);
        assert_eq!(parse_fps("fast"), None);
        let (mut ui, _) = detached();
        ui.fixed_frame = parse_fps("10");
        ui.update(Message::DragStarted);
        assert_eq!(ui.frame_interval(), Duration::from_millis(100));
    }

    #[test]
    fn the_drawn_extent_holds_the_input_region_and_every_shadow() {
        let mut ui = demo();
        let start = Instant::now();
        let within = |ui: &PetUi| {
            let drawn = ui.drawn_rects();
            ui.hit_rects().iter().all(|h| {
                drawn.iter().any(|d| {
                    d.x <= h.x && d.y <= h.y && d.x + d.width >= h.x + h.width && d.y + d.height >= h.y + h.height
                })
            })
        };
        run(&mut ui, start, 0.0, 1.0);
        // no bubbles yet: the halo and the ground shadow
        let drawn = ui.drawn_rects();
        assert_eq!(drawn.len(), 2);
        assert!(drawn[0].x <= 115 && drawn[0].x + drawn[0].width >= 265, "{drawn:?}");
        assert!(drawn[1].y + drawn[1].height >= 584, "the shadow's bottom");
        assert!(within(&ui));

        // at 15 s Claude, Codex and the review show, and the header; the bubbles' shadows reach past them
        run(&mut ui, start, 1.0, 14.0);
        assert!(within(&ui));
        let (card, frame) = ui.stacks.frames().last().unwrap();
        let (body, s) = (frame.body(), frame.scale);
        let reach = geometry::shadowed(body, &ui.card_shadow(card, s));
        assert!(reach.x <= body.x - 14.0 * s && reach.y + reach.height >= body.y + body.height + 18.0 * s);
        assert!(ui.drawn_rects().contains(&Rect::covering(reach).unwrap()));
        ui.set_inline_menu(true);
        assert!(within(&ui) && ui.drawn_rects().contains(&Rect::covering(INLINE_MENU).unwrap()));
    }
}
