//! The pet's front end, shared by the shells (`aipet-wayland`, `aipet-desktop`): the sprite driven by a clock, the
//! bubbles above it, the menu and Settings, and the geometry both the view and the input region come from.
//!
//! A shell owns a [`PetUi`], wraps its [`Message`]s in its own, calls [`PetUi::tick`] every
//! [`PetUi::frame_interval`], draws [`PetUi::pet_view`] in a 380 × 600 transparent surface ([`SURFACE`]), and makes
//! [`PetUi::hit_rects`] the surface's input region whenever it changes ([`PetUi::drawn_rects`] is where it is drawn
//! on). Dragging is the shell's (it moves the surface); it reports the drag with [`Message::DragStarted`],
//! [`Message::DragMoved`] and [`Message::DragEnded`] (or [`Message::DragCancelled`]) so the pet dangles, runs, lands,
//! or gets poked. [`SPRITE`] and [`INLINE_MENU`] say where the sprite and the menu are.

mod cards;
mod demo;
mod geometry;
mod menu;
mod settings;
mod sprite;
mod style;
mod view;

use std::borrow::Cow;
use std::path::Path;
use std::time::{Duration, Instant};

use aipet_sprite::{Avatar, Pet, PetFrame, PetInput};
use iced::widget::image::Handle;
use iced::{Point, Rectangle, Size, Vector};

use cards::{Bubble, Fitted, Section, Stacks};
use geometry::{CARD_RADIUS, CLOSE_SIZE, HALO, HEADER_H, HEADER_RADIUS, MENU_RADIUS, Motion};
pub use geometry::{INLINE_MENU, MENU, Rect, SPRITE, SURFACE};
use sprite::Sprite;

/// The repository's avatars folder, next to the built-ins (the spike never reads the real pet's data folder).
const AVATARS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../avatars");
/// The bubble an empty pet shows while you point at it.
const GHOST: &str = "_ghost";
/// The widest a bubble's title or detail gets (TextBlock.MaxWidth).
const TEXT_MAX: f32 = 250.0;

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

/// What the pet's views send.
#[derive(Clone, Debug)]
pub enum Message {
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
    /// A bubble was clicked: a folded stack spreads out.
    CardPressed(&'static str),
    /// A bubble's dismiss button was clicked.
    Dismiss(&'static str),
    /// A menu item was chosen. The shell closes the menu.
    Menu(MenuItem),
    SelectAvatar(usize),
    SetBubbles(bool),
    SetMusic(bool),
    /// Forces the pet's mood, for trying the states out; `None` goes back to the demo script's.
    SetMood(Option<&'static str>),
    Quit,
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
    OpenSettings,
    Quit,
}

/// The pet, its bubbles and its settings.
pub struct PetUi {
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
    /// The bubbles shown, in stack order (each tick refills it from the demo script).
    show: Vec<Bubble>,
    /// Bubbles dismissed, each with what it showed then.
    dismissed: Vec<Dismissal>,
    /// The reviews header while it shows: how many reviews wait, its text and its width.
    header: Option<(usize, String, f32)>,
    pointer: Option<Point>,
    /// When the pointer left the surface.
    left_at: Option<f64>,
    hover: bool,
    hovered_card: Option<&'static str>,
    /// Where the pointer was at the last drag move that counted, from the press.
    drag_last: Vector,
    bubbles: bool,
    on_top: bool,
    music: bool,
    mood: Option<&'static str>,
    inline_menu: bool,
    /// Until when (pet clock) frames stay smooth, after the last fast motion.
    smooth_until: f64,
    /// `AIPET_FPS`: a fixed frame interval.
    fixed_frame: Option<Duration>,
}

/// A bubble the user dismissed: it stays away until it does something new.
struct Dismissal {
    id: &'static str,
    state: &'static str,
    detail: Cow<'static, str>,
}

impl Dismissal {
    /// Whether `bubble` is the dismissed one, showing what it showed then.
    fn unchanged(&self, bubble: &Bubble) -> bool {
        bubble.id == self.id && bubble.state == self.state && bubble.detail == self.detail
    }
}

impl Default for PetUi {
    fn default() -> Self {
        PetUi::new()
    }
}

impl PetUi {
    /// The pet wearing Sprout, with bubbles on and music off.
    pub fn new() -> Self {
        let avatars = Avatar::all(Path::new(AVATARS_DIR));
        let stills = avatars.iter().map(sprite::still).collect();
        PetUi {
            pet: Pet::new(),
            input: PetInput::default(),
            start: None,
            t: 0.0,
            motion: Motion::REST,
            sprite: Sprite::new(),
            sprite_rects: Vec::new(),
            shadow: sprite::shadow_image(),
            avatars,
            stills,
            avatar: 0,
            stacks: Stacks::default(),
            show: Vec::new(),
            dismissed: Vec::new(),
            header: None,
            pointer: None,
            left_at: None,
            hover: false,
            hovered_card: None,
            drag_last: Vector::ZERO,
            bubbles: true,
            on_top: true,
            music: false,
            mood: None,
            inline_menu: false,
            smooth_until: 0.0,
            fixed_frame: fixed_frame(),
        }
    }

    /// Moves everything on to `now`: the demo script, the pet's frame, the bubbles' easing. Call it every
    /// [`PetUi::frame_interval`].
    pub fn tick(&mut self, now: Instant) {
        let start = *self.start.get_or_insert(now);
        let t = now.saturating_duration_since(start).as_secs_f64();
        let elapsed = t - self.t;
        let dt = elapsed.clamp(0.001, 0.05);
        // a new review calls for attention for 6 s (the C#'s NewReviews)
        if demo::review_arrived(self.t, t) {
            self.input.alert_until = t + 6.0;
        }
        self.t = t;

        let mood = demo::scene(t, &mut self.show);
        let mood = self.mood.unwrap_or(mood);
        if self.input.state != mood {
            self.input.state = mood.to_owned();
            self.input.state_since = t;
        }
        self.input.hover = self.hover;
        self.input.music = self.music;
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

    /// Brings `show` (the script's bubbles) and the bubbles on screen in line with what should show.
    fn sync_cards(&mut self) {
        // a dismissed bubble comes back once it does something new: it goes, or its state or detail changes (the
        // C#'s Board.Refresh)
        self.dismissed.retain(|d| self.show.iter().any(|b| d.unchanged(b)));
        self.show.retain(|b| !self.dismissed.iter().any(|d| d.id == b.id));
        if !self.bubbles {
            self.show.clear();
        } else if self.show.is_empty() && self.hover && !self.input.dragging {
            // an empty pet you point at says how it is, as the C#'s ghost bubble
            let asleep = self.input.state == "sleep";
            self.show.push(Bubble {
                id: GHOST,
                section: Section::Chats,
                state: if asleep { "sleep" } else { "idle" },
                title: "Claude Code".into(),
                detail: if asleep { "Napping" } else { "All caught up" }.into(),
                app_colour: None,
            });
        }

        let away = self.left_at.is_some_and(|at| self.t - at > 2.5);
        self.stacks.sync(&self.show, self.t, away, fit_card);

        let waiting = self.show.iter().filter(|b| b.section == Section::Reviews).count();
        self.header = match self.header.take() {
            _ if !self.stacks.has_reviews() => None,
            Some(header) if header.0 == waiting => Some(header),
            _ => {
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

    /// Works out what the pointer is over: the sprite (a wave when the pointer arrives, the eyes follow it) and the
    /// bubble whose dismiss button shows.
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
        let hovered = self.hovered_card;
        // the menu drawn inline covers the bubbles under it
        let on_menu = |p: &Point| self.inline_menu && INLINE_MENU.contains(*p);
        self.hovered_card = self.pointer.filter(|p| !on_menu(p)).and_then(|p| {
            let front_to_back = self.stacks.frames().rev();
            front_to_back
                .filter(|(c, _)| c.content && !c.removing)
                .find(|(c, f)| f.body().contains(p) || (hovered == Some(c.id) && f.close().contains(p)))
                .map(|(c, _)| c.id)
        });
    }

    /// Handles a message from the pet's views or the shell's drag, and says what the shell should do, if anything.
    pub fn update(&mut self, message: Message) -> Option<Effect> {
        match message {
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
                // a release lands a drag or pokes a click; a lost drag just lets go
                if matches!(message, Message::DragEnded) {
                    if self.input.moved {
                        self.input.land_until = self.t + 0.45;
                    } else {
                        self.input.poke_until = self.t + 1.2;
                    }
                }
                self.input.dragging = false;
                self.input.moved = false;
                self.track_pointer();
            }
            Message::CardPressed(id) => {
                // a folded stack spreads out; a spread one would open the chat, which the demo doesn't have
                if let Some(bubble) = self.show.iter().find(|b| b.id == id) {
                    self.stacks.expand(bubble.section, &self.show);
                }
            }
            Message::Dismiss(id) => {
                if let Some(b) = self.show.iter().find(|b| b.id == id) {
                    self.dismissed.push(Dismissal {
                        id,
                        state: b.state,
                        detail: b.detail.clone(),
                    });
                }
            }
            Message::Menu(item) => match item {
                MenuItem::Bubbles => self.bubbles = !self.bubbles,
                MenuItem::OnTop => {
                    self.on_top = !self.on_top;
                    return Some(Effect::OnTop(self.on_top));
                }
                MenuItem::Avatars | MenuItem::Settings => return Some(Effect::OpenSettings),
                MenuItem::Quit => return Some(Effect::Quit),
            },
            Message::SelectAvatar(i) => {
                if let Some(avatar) = self.avatars.get(i) {
                    self.avatar = i;
                    self.pet.set_avatar(avatar.clone());
                }
            }
            Message::SetBubbles(on) => self.bubbles = on,
            Message::SetMusic(on) => self.music = on,
            Message::SetMood(mood) => self.mood = mood,
            Message::Quit => return Some(Effect::Quit),
        }
        None
    }

    /// Where the pet's surface takes the mouse, in whole logical px: the sprite (its hit ellipse's box and its body),
    /// every bubble that is visible (with its dismiss button while that shows), the reviews header, and the menu
    /// while it is open inline. Everything else clicks through. It changes as things move; hand it to the surface
    /// whenever it does.
    pub fn hit_rects(&self) -> Vec<Rect> {
        let mut rects = Vec::with_capacity(64);
        rects.extend_from_slice(&self.sprite_rects);
        for (card, frame) in self.stacks.frames() {
            if card.o <= 0.05 {
                continue;
            }
            geometry::rounded(frame.body(), CARD_RADIUS * frame.scale, &mut rects);
            if self.close_visible(card) {
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
    /// drawn with its shadow or glow (and its dismiss button while that shows), the reviews header and the menu
    /// while it is open inline. The rest of the surface is transparent, so a region that clips drawing as well
    /// (X11's bounding shape, a Windows window region) can be exactly this. It changes as things move.
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
            if self.close_visible(card) {
                add(frame.close());
            }
        }
        if let Some(header) = self.header_rect() {
            add(header);
        }
        if self.inline_menu {
            add(INLINE_MENU);
        }
        rects
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

    fn close_visible(&self, card: &cards::Card) -> bool {
        self.hovered_card == Some(card.id) && card.content && !card.removing
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

/// A bubble's title and detail cut to [`TEXT_MAX`], and its width: MakeCard's padding (14 + 9), 1 px borders, the
/// dot's 18 + 8 and the longer text, 240 at least.
fn fit_card(title: &str, detail: &str) -> Fitted {
    let (title, title_w) = style::fit(title, 13.5, style::UI_SEMIBOLD, TEXT_MAX);
    let (detail, detail_w) = style::fit(detail, 12.0, style::UI, TEXT_MAX);
    let width = (14.0 + 9.0 + 2.0 + 26.0 + title_w.max(detail_w).ceil()).clamp(240.0, 350.0);
    Fitted { title, detail, width }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A pet ticked for `secs` at 60 fps from a fixed start.
    fn run(ui: &mut PetUi, start: Instant, from: f64, secs: f64) {
        for i in 0..=(secs * 60.0) as u32 {
            ui.tick(start + Duration::from_secs_f64(from + i as f64 / 60.0));
        }
    }

    #[test]
    fn the_input_region_holds_the_sprite_and_every_visible_bubble() {
        let mut ui = PetUi::new();
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
        let mut ui = PetUi::new();
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
    fn a_click_pokes_and_a_drag_lands() {
        let mut ui = PetUi::new();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 1.0);
        ui.update(Message::DragStarted);
        ui.update(Message::DragMoved(Vector::new(2.0, 1.0)));
        assert!(!ui.drag_moved());
        ui.update(Message::DragEnded);
        assert!(ui.input.poke_until > ui.t && ui.input.land_until < ui.t);

        ui.update(Message::DragStarted);
        ui.update(Message::DragMoved(Vector::new(-10.0, 0.0)));
        assert!(ui.drag_moved() && ui.input.facing == -1);
        ui.update(Message::DragEnded);
        assert!(ui.input.land_until > ui.t && !ui.input.dragging);
    }

    fn shows(ui: &PetUi, id: &str) -> bool {
        ui.show.iter().any(|b| b.id == id)
    }

    #[test]
    fn a_dismissed_bubble_comes_back_when_it_does_something_new() {
        let mut ui = PetUi::new();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 3.0);
        assert!(shows(&ui, "claude"));
        ui.update(Message::Dismiss("claude"));
        // still thinking: it stays away (though its thinking dots count on)
        run(&mut ui, start, 3.0, 2.5);
        assert!(!shows(&ui, "claude"));
        // at 6 s it works: that's new
        run(&mut ui, start, 5.5, 1.0);
        assert!(shows(&ui, "claude"));

        // the review doesn't change: it stays away until it goes, and is back when it comes again
        run(&mut ui, start, 6.5, 8.0);
        ui.update(Message::Dismiss("jira"));
        run(&mut ui, start, 14.5, 16.0);
        assert!(!shows(&ui, "jira"));
        run(&mut ui, start, 30.5, 1.0);
        assert!(ui.dismissed.is_empty(), "it went at 31 s");
        run(&mut ui, start, demo::PERIOD + 13.0, 0.5);
        assert!(shows(&ui, "jira"));
    }

    #[test]
    fn an_empty_pet_shows_the_ghost_even_when_its_bubbles_were_dismissed() {
        let mut ui = PetUi::new();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 3.2);
        ui.update(Message::Dismiss("claude"));
        run(&mut ui, start, 3.2, 1.3);
        ui.update(Message::PointerMoved(Point::new(190.0, 530.0)));
        run(&mut ui, start, 4.5, 1.0);
        assert!(ui.sprite_hit(Point::new(190.0, 530.0)));
        assert_eq!(ui.show.iter().map(|b| b.id).collect::<Vec<_>>(), [GHOST]);
        // dismissing the ghost doesn't keep it away (as in the C#, whose board never has it)
        ui.update(Message::Dismiss(GHOST));
        run(&mut ui, start, 5.5, 0.1);
        assert!(shows(&ui, GHOST));
    }

    #[test]
    fn a_new_review_calls_for_attention_for_six_seconds() {
        let mut ui = PetUi::new();
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
        let mut ui = PetUi::new();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 1.0);
        ui.update(Message::DragStarted);
        ui.update(Message::DragMoved(Vector::new(-10.0, 0.0)));
        ui.update(Message::DragCancelled);
        assert!(!ui.input.dragging && !ui.drag_moved());
        assert!(ui.input.poke_until < ui.t && ui.input.land_until < ui.t);
    }

    #[test]
    fn the_inline_menu_hides_the_bubble_under_it_from_the_pointer() {
        let mut ui = PetUi::new();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 3.5);
        // on the Claude bubble, where the menu covers it
        let p = Point::new(190.0, 440.0);
        assert!(INLINE_MENU.contains(p));
        ui.update(Message::PointerMoved(p));
        assert_eq!(ui.hovered_card, Some("claude"));
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
        let mut ui = PetUi::new();
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
        // at 31–32 s only Claude's done bubble is left, and the pet is idle
        assert_eq!(ui.frame_interval(), CALM_FRAME);
    }

    #[test]
    fn a_poke_and_a_hop_are_smooth() {
        let mut ui = PetUi::new();
        ui.fixed_frame = None;
        let start = Instant::now();
        paced(&mut ui, start, 0.0, 1.5);
        assert_eq!(ui.frame_interval(), CALM_FRAME);
        ui.update(Message::DragStarted);
        ui.update(Message::DragEnded);
        let ticks = paced(&mut ui, start, 1.5, 2.6);
        assert!(all(&ticks, 1.5, 2.6, SMOOTH_FRAME), "the poke's jump, 1.2 s");

        // an idle hop: 16 px up and down in 0.9 s
        let mut ui = PetUi::new();
        ui.fixed_frame = None;
        ui.mood = Some("idle");
        let mut at = 0.0;
        while ui.pet.idle_act() != Some("hop") {
            assert!(at < 600.0, "no hop in ten minutes");
            paced(&mut ui, start, at, at + 1.0);
            at += 1.0;
        }
        assert_eq!(ui.frame_interval(), SMOOTH_FRAME);
    }

    #[test]
    fn aipet_fps_fixes_the_frame_interval() {
        assert_eq!(parse_fps("30"), Some(CALM_FRAME));
        assert_eq!(parse_fps(" 1000 "), Some(Duration::from_millis(1)));
        assert_eq!(parse_fps("0"), None);
        assert_eq!(parse_fps("fast"), None);
        let mut ui = PetUi::new();
        ui.fixed_frame = parse_fps("10");
        ui.update(Message::DragStarted);
        assert_eq!(ui.frame_interval(), Duration::from_millis(100));
    }

    #[test]
    fn the_drawn_extent_holds_the_input_region_and_every_shadow() {
        let mut ui = PetUi::new();
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

    #[test]
    fn menu_items_toggle_or_ask_the_shell() {
        let mut ui = PetUi::new();
        assert_eq!(ui.update(Message::Menu(MenuItem::Bubbles)), None);
        assert!(!ui.bubbles);
        assert_eq!(ui.update(Message::Menu(MenuItem::OnTop)), Some(Effect::OnTop(false)));
        assert_eq!(ui.update(Message::Menu(MenuItem::Avatars)), Some(Effect::OpenSettings));
        assert_eq!(ui.update(Message::Menu(MenuItem::Quit)), Some(Effect::Quit));
        ui.set_inline_menu(true);
        assert!(ui.hit_rects().iter().any(|r| r.y < INLINE_MENU.y as i32 + 10));
    }
}
