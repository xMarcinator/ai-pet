//! The pet's front end, shared by the shells (`aipet-wayland`, `aipet-desktop`): the sprite driven by a clock, the
//! bubbles above it, the menu and Settings, and the geometry both the view and the input region come from.
//!
//! A shell owns a [`PetUi`], wraps its [`Message`]s in its own, calls [`PetUi::tick`] every frame, draws
//! [`PetUi::pet_view`] in a 380 × 600 transparent surface ([`SURFACE`]), and makes [`PetUi::hit_rects`] the surface's
//! input region whenever it changes. Dragging is the shell's (it moves the surface); it reports the drag with
//! [`Message::DragStarted`], [`Message::DragMoved`] and [`Message::DragEnded`] so the pet dangles, runs, lands, or
//! gets poked.

mod cards;
mod demo;
mod geometry;
mod menu;
mod settings;
mod sprite;
mod style;
mod view;

use std::path::Path;
use std::time::Instant;

use aipet_sprite::{Avatar, Pet, PetFrame, PetInput};
use iced::widget::image::Handle;
use iced::{Point, Rectangle, Size, Vector};

use cards::{Bubble, Fitted, Section, Stacks};
use geometry::{CARD_RADIUS, CLOSE_SIZE, HEADER_H, HEADER_RADIUS, INLINE_MENU, MENU_RADIUS, Motion};
pub use geometry::{MENU, Rect, SURFACE};
use sprite::Sprite;

/// The repository's avatars folder, next to the built-ins (the spike never reads the real pet's data folder).
const AVATARS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../avatars");
/// The bubble an empty pet shows while you point at it.
const GHOST: &str = "_ghost";
/// The widest a bubble's title or detail gets (TextBlock.MaxWidth).
const TEXT_MAX: f32 = 250.0;

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
    sprite_rects: Vec<Rect>,
    shadow: Handle,
    avatars: Vec<Avatar>,
    /// Each avatar standing still, for the picker.
    stills: Vec<Handle>,
    avatar: usize,
    stacks: Stacks,
    /// The bubbles shown, in stack order.
    show: Vec<Bubble>,
    /// Bubbles dismissed in a pass of the demo script: (pass, id).
    dismissed: Vec<(u64, &'static str)>,
    /// The reviews header's text and width, while it shows.
    header: Option<(String, f32)>,
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
        }
    }

    /// Moves everything on to `now`: the demo script, the pet's frame, the bubbles' easing. Call it every frame.
    pub fn tick(&mut self, now: Instant) {
        let start = *self.start.get_or_insert(now);
        let t = now.saturating_duration_since(start).as_secs_f64();
        let dt = (t - self.t).clamp(0.001, 0.05);
        self.t = t;

        let scene = demo::scene(t);
        let mood = self.mood.unwrap_or(scene.mood);
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
        self.motion = motion(&frame);
        self.sprite_rects = geometry::sprite_rects(self.motion, &self.sprite.runs);

        self.sync_cards(scene.bubbles);
        self.stacks.animate(dt, t);
        // the pet and the bubbles moved: what is under a still pointer may have changed
        self.track_pointer();
    }

    fn sync_cards(&mut self, mut show: Vec<Bubble>) {
        if !self.bubbles {
            show.clear();
        } else if show.is_empty() && self.hover && !self.input.dragging {
            // an empty pet you point at says how it is, as the C#'s ghost bubble
            let asleep = self.input.state == "sleep";
            show.push(Bubble {
                id: GHOST,
                section: Section::Chats,
                state: if asleep { "sleep" } else { "idle" },
                title: "Claude Code".to_owned(),
                detail: if asleep { "Napping" } else { "All caught up" }.to_owned(),
                app_colour: None,
            });
        }
        let pass = demo::cycle(self.t);
        self.dismissed.retain(|&(p, _)| p == pass);
        show.retain(|b| !self.dismissed.iter().any(|&(_, id)| id == b.id));

        let away = self.left_at.is_some_and(|at| self.t - at > 2.5);
        self.stacks.sync(&show, self.t, away, fit_card);

        let waiting = show.iter().filter(|b| b.section == Section::Reviews).count();
        let text = match waiting {
            0 => "Jira and GitHub".to_owned(),
            n => format!("{n} waiting on you"),
        };
        self.header = match self.header.take() {
            _ if !self.stacks.has_reviews() => None,
            Some(header) if header.0 == text => Some(header),
            _ => {
                // padding 10 + dot 6 + spacing 6 + text + padding 10, and the border
                let width = style::text_width(&text, 11.0, style::UI_SEMIBOLD).ceil() + 34.0;
                Some((text, width))
            }
        };
        self.show = show;
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
        self.hovered_card = self.pointer.and_then(|p| {
            let frames = self.stacks.frames();
            let front_to_back = frames.iter().rev();
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
            Message::DragEnded => {
                if !self.input.dragging {
                    return None;
                }
                if self.input.moved {
                    self.input.land_until = self.t + 0.45;
                } else {
                    self.input.poke_until = self.t + 1.2;
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
            Message::Dismiss(id) => self.dismissed.push((demo::cycle(self.t), id)),
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
        let mut rects = self.sprite_rects.clone();
        for (card, frame) in self.stacks.frames() {
            if card.o <= 0.05 {
                continue;
            }
            rects.extend(geometry::rounded(frame.body(), CARD_RADIUS * frame.scale));
            if self.close_visible(card) {
                rects.extend(geometry::rounded(frame.close(), CLOSE_SIZE / 2.0 * frame.scale));
            }
        }
        if let Some(header) = self.header_rect() {
            rects.extend(geometry::rounded(header, HEADER_RADIUS));
        }
        if self.inline_menu {
            rects.extend(geometry::rounded(INLINE_MENU, MENU_RADIUS));
        }
        rects
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
    }

    fn close_visible(&self, card: &cards::Card) -> bool {
        self.hovered_card == Some(card.id) && card.content && !card.removing
    }

    fn header_rect(&self) -> Option<Rectangle> {
        let (_, width) = self.header.as_ref().filter(|_| self.bubbles)?;
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

    #[test]
    fn dismissed_bubbles_stay_away_until_the_script_starts_over() {
        let mut ui = PetUi::new();
        let start = Instant::now();
        run(&mut ui, start, 0.0, 3.0);
        assert!(ui.show.iter().any(|b| b.id == "claude"));
        ui.update(Message::Dismiss("claude"));
        run(&mut ui, start, 3.0, 1.0);
        assert!(!ui.show.iter().any(|b| b.id == "claude"));
        run(&mut ui, start, demo::PERIOD + 3.0, 0.1);
        assert!(ui.show.iter().any(|b| b.id == "claude"));
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
