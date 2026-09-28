//! Dragging the pet: a state machine fed with the pet surface's pointer, frame and size events (and when the changes
//! it asked for were made), which says how to move the surface and what the pet should hear. It knows nothing of
//! Wayland, so the tests replay event sequences.
//!
//! At home the surface is [`SURFACE`]-sized and anchored to its output's bottom-right corner, placed by its right
//! and bottom margins. It may hang over the output's edges, as long as the sprite stays on it. A left press on the
//! sprite is a poke, unless the pointer then gets more than 4 px away: then it is a drag, which follows the pointer
//! one of two ways ([`Strategy`]).
//!
//! What the compositor does in between matters, and Hyprland 0.56 was measured: pointer positions follow a new
//! place of the surface at once, but with its `layers` animation on, every change of the surface's position (by its
//! margins, or by a resize that moves its top-left corner) slides there over the animation's time. A resize that
//! keeps the top-left corner shows the old frame, unscaled, until the new one comes.

use std::time::{Duration, Instant};

use aipet_ui::{Message as Pet, SPRITE, SURFACE};
use iced::{Point, Size, Vector};

/// How far (|dx| + |dy|, px) the pointer gets from the press before it is a drag, as in the C#'s Sprite_Moved
/// (aipet-ui tells a poke from a drag by the same rule).
const THRESHOLD: f32 = 4.0;
/// Frames drawn after a move was made (committed) that prove the compositor took it: each frame is drawn after the
/// compositor answered the one before, so by the second it had read the move.
const CONFIRM: u8 = 2;
/// How long a step waits for the compositor to resize the surface before the drag gives up on it.
const PATIENCE: Duration = Duration::from_secs(1);

/// How a drag follows the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    /// Moves the surface by its margins. A pointer position is relative to the surface, and nothing says when the
    /// compositor has moved it, so one move is in flight at a time: from the moment it is asked for the pointer is
    /// ignored, until [`CONFIRM`] frames drawn after it was made came round, and the next move goes from there. The pet
    /// follows at most every other frame and never drifts. Hyprland's animation makes it glide behind the pointer.
    Margin,
    /// Grows the surface over the whole output for the drag, so pointer positions are the output's and the pet is
    /// simply drawn under the pointer, then shrinks it back around the pet on release. Growing and shrinking keep
    /// the surface's top-left corner. Moving that corner (to the output's, and back to the pet's) is sent right
    /// after the first frame drawn for the new corner, so the compositor takes both at once. Hyprland's animation
    /// makes those two moves slide.
    Overlay,
}

impl Strategy {
    /// From `AIPET_DRAG`: `margin` (the default) or `overlay`.
    pub fn from_env() -> Result<Strategy, String> {
        match std::env::var("AIPET_DRAG").ok().as_deref() {
            None | Some("margin") => Ok(Strategy::Margin),
            Some("overlay") => Ok(Strategy::Overlay),
            Some(other) => Err(format!("AIPET_DRAG must be margin or overlay, not {other:?}")),
        }
    }
}

/// Where the surface sits at home: its margins from the output's right and bottom edges (logical px).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Home {
    pub right: i32,
    pub bottom: i32,
}

/// What to ask of the pet's surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Its margins: top, right, bottom, left.
    Margins(i32, i32, i32, i32),
    /// Its home layout: [`SURFACE`]-sized, anchored to the output's bottom-right corner.
    Home,
    /// This big, anchored to the output's top-left corner.
    Cover(u32, u32),
}

/// What happened on the pet's surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    /// A left press on the sprite, at this surface point.
    Press(Point),
    /// The pointer moved to this surface point.
    Motion(Point),
    Release,
    /// The surface drew a frame, at this time (its `RedrawRequested`'s).
    Redraw(Instant),
    /// The compositor gave the surface this size.
    Resized(Size),
    /// The changes of the last step that asked for it ([`Step::confirm`]) have been made: sent, and the surface
    /// committed.
    Committed,
}

/// What to do about an input: the surface's changes, in order, and news for the pet (a drag starting, moving by
/// so much from the press, in screen px, or ending).
#[derive(Debug, Default)]
pub struct Step {
    pub actions: Vec<Action>,
    /// The drag waits to hear when these changes have been made: hand it [`Input::Committed`] then (a change is made
    /// some time after it is asked for, and frames drawn before that prove nothing).
    pub confirm: bool,
    pub pet: Option<Pet>,
}

/// A change asked for: when it was made, once it is, and how many frames drawn after that have come round.
#[derive(Clone, Copy, Debug)]
struct Sent {
    at: Option<Instant>,
    frames: u8,
}

impl Sent {
    fn asked() -> Sent {
        Sent { at: None, frames: 0 }
    }

    /// Counts a frame drawn at `drawn` (one drawn before the change was made, reported late, doesn't count); true
    /// once the compositor has surely taken the change.
    fn landed(&mut self, drawn: Instant) -> bool {
        if self.at.is_some_and(|at| drawn > at) {
            self.frames += 1;
        }
        self.frames >= CONFIRM
    }
}

/// An output's logical size.
type Output = (i32, i32);

#[derive(Clone, Copy, Debug)]
enum Phase {
    Idle,
    /// Pressed on the sprite at `grab` (a surface point), not a drag yet.
    Pressed {
        grab: Point,
    },
    /// The margin strategy following the pointer: `start` is where the drag began, `moving` the move in flight.
    Margins {
        grab: Point,
        start: Home,
        moving: Option<(Home, Sent)>,
    },
    /// Overlay: the output-sized surface is on its way, its top-left still the pet's `origin` on the output, so
    /// pointer positions are still right; `pointer` is the latest, on the output.
    Growing {
        grab: Point,
        output: Output,
        origin: Point,
        pointer: Point,
        released: bool,
        since: Instant,
    },
    /// Overlay: the pet is drawn at `origin` on the output-sized surface, whose top-left goes to the output's right
    /// after the first frame drawn so (`since`), then lands.
    Shifting {
        grab: Point,
        output: Output,
        origin: Point,
        pointer: Point,
        released: bool,
        since: Instant,
        sent: Option<Sent>,
    },
    /// Overlay: the surface covers the output, and the pet's box is drawn at `at` on it.
    Overlay {
        grab: Point,
        output: Output,
        origin: Point,
        at: Point,
    },
    /// Overlay, released: the pet is drawn at the surface's top-left, which goes to `home`, with the home layout,
    /// right after the first frame drawn so (`since`).
    Returning {
        home: Home,
        output: Output,
        since: Instant,
    },
    /// Waiting for the drag's last change to land before another press counts: a move in flight, or (with no
    /// `sent`) the home size.
    Settling {
        sent: Option<Sent>,
        since: Instant,
    },
}

/// The pet surface's drags.
pub struct Drag {
    strategy: Strategy,
    /// Where the surface is, or is going to with the move in flight.
    home: Home,
    output: Option<Output>,
    phase: Phase,
}

impl Drag {
    pub fn new(strategy: Strategy, home: Home) -> Drag {
        Drag {
            strategy,
            home,
            output: None,
            phase: Phase::Idle,
        }
    }

    /// All four margins for home (top, right, bottom, left), so the surface is in the same place whatever it is
    /// anchored to.
    pub fn margins(&self) -> (i32, i32, i32, i32) {
        margins(self.home, self.output)
    }

    /// Whether a drag is on: it needs every pointer move and frame of the surface, in the order they came.
    pub fn active(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }

    /// Where the pet's box is drawn on the surface.
    pub fn offset(&self) -> Vector {
        match self.phase {
            Phase::Shifting { origin, .. } => origin - Point::ORIGIN,
            Phase::Overlay { at, .. } => at - Point::ORIGIN,
            _ => Vector::ZERO,
        }
    }

    /// What the drag is doing, for the log.
    pub fn describe(&self) -> &'static str {
        match self.phase {
            Phase::Idle => "idle",
            Phase::Pressed { .. } => "pressed",
            Phase::Margins { .. } => "moving by margins",
            Phase::Growing { .. } => "growing over the output",
            Phase::Shifting { .. } => "shifting to the output's corner",
            Phase::Overlay { .. } => "over the output",
            Phase::Returning { .. } => "returning home",
            Phase::Settling { .. } => "settling",
        }
    }

    /// The output the surface is on (its logical size), if known. At rest, a home the output no longer holds is
    /// moved onto it.
    pub fn set_output(&mut self, output: Option<Output>) -> Vec<Action> {
        self.output = output.filter(|&(w, h)| w > 0 && h > 0);
        let home = clamp(self.home, self.output);
        if !matches!(self.phase, Phase::Idle) || home == self.home {
            return Vec::new();
        }
        self.home = home;
        vec![move_to(home, self.output)]
    }

    /// The surface's top-left on its output, and the output's size, when that is known and no drag is on.
    pub fn placed(&self) -> Option<(Point, Output)> {
        let output = self.output.filter(|_| !self.active())?;
        Some((origin(self.home, output), output))
    }

    /// The surface is gone, or made anew at home: whatever drag was on is over. A pet still held is let go, with
    /// [`Pet::DragCancelled`] to hear.
    pub fn reset(&mut self) -> Option<Pet> {
        let held = match self.phase {
            Phase::Idle | Phase::Returning { .. } | Phase::Settling { .. } => false,
            Phase::Growing { released, .. } | Phase::Shifting { released, .. } => !released,
            Phase::Pressed { .. } | Phase::Margins { .. } | Phase::Overlay { .. } => true,
        };
        self.phase = Phase::Idle;
        held.then_some(Pet::DragCancelled)
    }

    /// Takes in what happened at `now`.
    pub fn handle(&mut self, input: Input, now: Instant) -> Step {
        let mut step = Step::default();
        self.phase = match (self.phase, input) {
            (Phase::Idle, Input::Press(grab)) => {
                step.pet = Some(Pet::DragStarted);
                Phase::Pressed { grab }
            }
            (Phase::Pressed { grab }, Input::Motion(p)) => {
                let travel = p - grab;
                if travel.x.abs() + travel.y.abs() <= THRESHOLD {
                    step.pet = Some(Pet::DragMoved(travel));
                    Phase::Pressed { grab }
                } else if let (Strategy::Overlay, Some(output)) = (self.strategy, self.output) {
                    step.pet = Some(Pet::DragMoved(travel));
                    step.actions = vec![move_to(self.home, Some(output)), cover(output)];
                    let origin = origin(self.home, output);
                    Phase::Growing {
                        grab,
                        output,
                        origin,
                        pointer: origin + (p - Point::ORIGIN),
                        released: false,
                        since: now,
                    }
                } else {
                    self.follow(grab, self.home, p, &mut step)
                }
            }
            (Phase::Pressed { .. }, Input::Release) => {
                step.pet = Some(Pet::DragEnded);
                Phase::Idle
            }

            (
                Phase::Margins {
                    grab,
                    start,
                    moving: None,
                },
                Input::Motion(p),
            ) => self.follow(grab, start, p, &mut step),
            (
                Phase::Margins {
                    grab,
                    start,
                    moving: Some((target, mut sent)),
                },
                Input::Redraw(drawn),
            ) => {
                if sent.landed(drawn) {
                    self.home = target;
                    Phase::Margins {
                        grab,
                        start,
                        moving: None,
                    }
                } else {
                    Phase::Margins {
                        grab,
                        start,
                        moving: Some((target, sent)),
                    }
                }
            }
            (Phase::Margins { moving, .. }, Input::Release) => {
                step.pet = Some(Pet::DragEnded);
                match moving {
                    Some((target, sent)) => {
                        self.home = target;
                        Phase::Settling {
                            sent: Some(sent),
                            since: now,
                        }
                    }
                    None => Phase::Idle,
                }
            }

            (
                Phase::Growing {
                    grab,
                    output,
                    origin,
                    released: false,
                    since,
                    ..
                },
                Input::Motion(p),
            ) => {
                let pointer = origin + (p - Point::ORIGIN);
                step.pet = Some(Pet::DragMoved(pointer - (origin + (grab - Point::ORIGIN))));
                Phase::Growing {
                    grab,
                    output,
                    origin,
                    pointer,
                    released: false,
                    since,
                }
            }
            (
                Phase::Growing {
                    grab,
                    output,
                    origin,
                    pointer,
                    released,
                    ..
                },
                Input::Resized(size),
            ) if !is_size(size, SURFACE) => {
                if released {
                    // the corner never moved: straight back home
                    step.actions = vec![Action::Home];
                    Phase::Settling { sent: None, since: now }
                } else {
                    Phase::Shifting {
                        grab,
                        output,
                        origin,
                        pointer,
                        released: false,
                        since: now,
                        sent: None,
                    }
                }
            }
            (Phase::Growing { released, since, .. }, Input::Redraw(drawn)) if drawn - since > PATIENCE => {
                // the compositor never gave the surface the output's size
                if !released {
                    step.pet = Some(Pet::DragEnded);
                }
                step.actions = vec![Action::Home];
                Phase::Idle
            }

            (Phase::Shifting { since, sent: None, .. }, Input::Redraw(drawn)) if drawn > since => {
                // the first frame with the pet drawn for the output's corner is out: the corner follows it
                step.actions = vec![Action::Margins(0, 0, 0, 0)];
                step.confirm = true;
                self.phase_with_sent(Sent::asked())
            }
            (
                Phase::Shifting {
                    grab,
                    output,
                    origin,
                    pointer,
                    released,
                    sent: Some(mut sent),
                    since,
                },
                Input::Redraw(drawn),
            ) => {
                if !sent.landed(drawn) {
                    Phase::Shifting {
                        grab,
                        output,
                        origin,
                        pointer,
                        released,
                        since,
                        sent: Some(sent),
                    }
                } else if released {
                    Phase::Returning {
                        home: home_at(origin, output),
                        output,
                        since: now,
                    }
                } else {
                    Phase::Overlay {
                        grab,
                        output,
                        origin,
                        at: place(pointer - (grab - Point::ORIGIN), output),
                    }
                }
            }

            (
                Phase::Overlay {
                    grab, output, origin, ..
                },
                Input::Motion(p),
            ) => {
                step.pet = Some(Pet::DragMoved(p - (origin + (grab - Point::ORIGIN))));
                Phase::Overlay {
                    grab,
                    output,
                    origin,
                    at: place(p - (grab - Point::ORIGIN), output),
                }
            }
            (Phase::Overlay { output, at, .. }, Input::Release) => {
                step.pet = Some(Pet::DragEnded);
                Phase::Returning {
                    home: home_at(at, output),
                    output,
                    since: now,
                }
            }
            (Phase::Returning { home, output, since }, Input::Redraw(drawn)) if drawn > since => {
                // the first frame with the pet drawn at the surface's corner is out: the corner follows it, and
                // then the surface shrinks around the pet, keeping that corner
                step.actions = vec![move_to(home, Some(output)), Action::Home];
                self.home = home;
                Phase::Settling { sent: None, since: now }
            }

            (Phase::Growing { released: false, .. } | Phase::Shifting { released: false, .. }, Input::Release) => {
                step.pet = Some(Pet::DragEnded);
                self.phase_released()
            }
            (Phase::Settling { sent: None, .. }, Input::Resized(size)) if is_size(size, SURFACE) => Phase::Idle,
            (Phase::Settling { since, .. }, Input::Redraw(drawn)) if drawn - since > PATIENCE => Phase::Idle,
            (
                Phase::Settling {
                    sent: Some(mut sent),
                    since,
                },
                Input::Redraw(drawn),
            ) => {
                if sent.landed(drawn) {
                    Phase::Idle
                } else {
                    Phase::Settling {
                        sent: Some(sent),
                        since,
                    }
                }
            }
            (mut phase, Input::Committed) => {
                if let Phase::Margins {
                    moving: Some((_, sent)),
                    ..
                }
                | Phase::Shifting { sent: Some(sent), .. }
                | Phase::Settling { sent: Some(sent), .. } = &mut phase
                {
                    sent.at.get_or_insert(now);
                }
                phase
            }
            (phase, _) => phase,
        };
        step
    }

    /// The margin strategy on a pointer move to `p` with no move in flight: tells the pet how far the pointer is
    /// from the press, and sends the surface where it keeps the pointer on the grab (as far as the output allows).
    fn follow(&mut self, grab: Point, start: Home, p: Point, step: &mut Step) -> Phase {
        let d = p - grab;
        let moved = Vector::new(
            (start.right - self.home.right) as f32,
            (start.bottom - self.home.bottom) as f32,
        );
        step.pet = Some(Pet::DragMoved(moved + d));
        let target = clamp(
            Home {
                right: self.home.right - d.x.round() as i32,
                bottom: self.home.bottom - d.y.round() as i32,
            },
            self.output,
        );
        let moving = (target != self.home).then(|| {
            step.actions.push(move_to(target, self.output));
            step.confirm = true;
            (target, Sent::asked())
        });
        Phase::Margins { grab, start, moving }
    }

    /// The shifting phase with its corner move sent.
    fn phase_with_sent(&self, sent: Sent) -> Phase {
        match self.phase {
            Phase::Shifting {
                grab,
                output,
                origin,
                pointer,
                released,
                since,
                ..
            } => Phase::Shifting {
                grab,
                output,
                origin,
                pointer,
                released,
                since,
                sent: Some(sent),
            },
            phase => phase,
        }
    }

    /// The growing or shifting phase, released: it goes on until the pet can go home.
    fn phase_released(&self) -> Phase {
        match self.phase {
            Phase::Growing {
                grab,
                output,
                origin,
                pointer,
                since,
                ..
            } => Phase::Growing {
                grab,
                output,
                origin,
                pointer,
                released: true,
                since,
            },
            Phase::Shifting {
                grab,
                output,
                origin,
                pointer,
                since,
                sent,
                ..
            } => Phase::Shifting {
                grab,
                output,
                origin,
                pointer,
                released: true,
                since,
                sent,
            },
            phase => phase,
        }
    }
}

/// Whether the compositor gave the surface `size`. At a fractional scale iced reports the size in whole physical px,
/// back in logical px (at 4/3, ceil(380 × 4/3) / (4/3) is 380.25), so it may be off by less than a px.
fn is_size(got: Size, size: Size) -> bool {
    (got.width - size.width).abs() < 1.0 && (got.height - size.height).abs() < 1.0
}

/// The surface's top-left on the output at `home`.
fn origin(home: Home, (w, h): Output) -> Point {
    Point::new(
        (w - SURFACE.width as i32 - home.right) as f32,
        (h - SURFACE.height as i32 - home.bottom) as f32,
    )
}

/// The home that puts the surface's top-left at `origin` (whole px) on the output.
fn home_at(origin: Point, (w, h): Output) -> Home {
    Home {
        right: w - SURFACE.width as i32 - origin.x as i32,
        bottom: h - SURFACE.height as i32 - origin.y as i32,
    }
}

/// All four margins for `home`, so the surface is in the same place whatever it is anchored to (top and left need
/// the output's size; without it they are 0, which only an overlay drag, which knows the output, would read).
fn margins(home: Home, output: Option<Output>) -> (i32, i32, i32, i32) {
    let (top, left) = output.map_or((0, 0), |output| {
        let o = origin(home, output);
        (o.y as i32, o.x as i32)
    });
    (top, home.right, home.bottom, left)
}

/// The margin change that takes the surface to `home`.
fn move_to(home: Home, output: Option<Output>) -> Action {
    let (top, right, bottom, left) = margins(home, output);
    Action::Margins(top, right, bottom, left)
}

fn cover((w, h): Output) -> Action {
    Action::Cover(w as u32, h as u32)
}

/// `home`, moved so the sprite stays on the output (the surface's transparent parts may hang over the edges).
/// Without the output's size, only the right and bottom edges are known.
fn clamp(home: Home, output: Option<Output>) -> Home {
    let mut right = home.right.max((SPRITE.x + SPRITE.width - SURFACE.width) as i32);
    let mut bottom = home.bottom.max((SPRITE.y + SPRITE.height - SURFACE.height) as i32);
    if let Some((w, h)) = output {
        right = right.min(w - SURFACE.width as i32 + SPRITE.x as i32);
        bottom = bottom.min(h - SURFACE.height as i32 + SPRITE.y as i32);
    }
    Home { right, bottom }
}

/// Where the pet's box goes on the output for a top-left the pointer asks for: whole px, the sprite on the output.
fn place(top_left: Point, output: Output) -> Point {
    let rounded = Point::new(top_left.x.round(), top_left.y.round());
    origin(clamp(home_at(rounded, output), Some(output)), output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: Output = (1920, 1200);
    const HOME: Home = Home { right: 420, bottom: 0 };
    /// Where the sprite is pressed: its middle.
    const GRAB: Point = Point::new(190.0, 530.0);
    const FRAME: Duration = Duration::from_millis(16);

    fn drag(strategy: Strategy) -> Drag {
        let mut drag = Drag::new(strategy, HOME);
        assert!(drag.set_output(Some(OUTPUT)).is_empty());
        drag
    }

    /// The surface's top-left on the output for margins (the home anchor reads the right and bottom ones).
    fn placed(action: Action) -> Point {
        let Action::Margins(_, right, bottom, _) = action else {
            panic!("{action:?} is not a margin change")
        };
        origin(Home { right, bottom }, OUTPUT)
    }

    fn moved(step: &Step) -> Vector {
        match step.pet {
            Some(Pet::DragMoved(v)) => v,
            ref other => panic!("not a move: {other:?}"),
        }
    }

    #[test]
    fn a_click_that_never_moves_pokes_and_asks_nothing_of_the_surface() {
        let mut d = drag(Strategy::Margin);
        let t = Instant::now();
        let step = d.handle(Input::Press(GRAB), t);
        assert!(matches!(step.pet, Some(Pet::DragStarted)) && step.actions.is_empty());
        assert!(d.active());
        let step = d.handle(Input::Motion(GRAB + Vector::new(2.0, -1.0)), t);
        assert_eq!(moved(&step), Vector::new(2.0, -1.0));
        assert!(step.actions.is_empty());
        assert!(d.handle(Input::Redraw(t + FRAME), t + FRAME).actions.is_empty());
        let step = d.handle(Input::Release, t + FRAME);
        assert!(matches!(step.pet, Some(Pet::DragEnded)) && step.actions.is_empty());
        assert!(!d.active());
        assert_eq!(d.home, HOME);
    }

    #[test]
    fn a_margin_drag_moves_once_per_landed_change() {
        let mut d = drag(Strategy::Margin);
        let t0 = Instant::now();
        d.handle(Input::Press(GRAB), t0);
        // 10 px right, 20 px up: past the threshold, so the surface moves that far
        let t1 = t0 + FRAME;
        let step = d.handle(Input::Motion(GRAB + Vector::new(10.0, -20.0)), t1);
        assert_eq!(moved(&step), Vector::new(10.0, -20.0));
        assert_eq!(step.actions, vec![Action::Margins(580, 410, 20, 1130)]);
        assert!(step.confirm);
        d.handle(Input::Committed, t1);
        // while it is in flight the pointer is ignored, and a frame drawn before it was made doesn't count
        assert!(d.handle(Input::Motion(GRAB + Vector::new(3.0, 0.0)), t1).pet.is_none());
        d.handle(Input::Redraw(t1 - Duration::from_millis(1)), t1);
        d.handle(Input::Redraw(t1 + FRAME), t1 + FRAME);
        assert!(
            d.handle(Input::Motion(GRAB + Vector::new(3.0, 0.0)), t1 + FRAME)
                .pet
                .is_none()
        );
        d.handle(Input::Redraw(t1 + 2 * FRAME), t1 + 2 * FRAME);
        assert_eq!(d.home, Home { right: 410, bottom: 20 });
        // landed: pointer positions are the moved surface's; 3 px further right is 13 px from the press in all
        let step = d.handle(Input::Motion(GRAB + Vector::new(3.0, 0.0)), t1 + 2 * FRAME);
        assert_eq!(moved(&step), Vector::new(13.0, -20.0));
        assert_eq!(step.actions, vec![Action::Margins(580, 407, 20, 1133)]);
        // released with that move in flight: it is home, and the next press waits until it lands
        let t2 = t1 + 3 * FRAME;
        assert!(matches!(d.handle(Input::Release, t2).pet, Some(Pet::DragEnded)));
        assert_eq!(d.home, Home { right: 407, bottom: 20 });
        d.handle(Input::Committed, t2);
        assert!(d.handle(Input::Press(GRAB), t2).pet.is_none());
        d.handle(Input::Redraw(t2 + FRAME), t2 + FRAME);
        d.handle(Input::Redraw(t2 + 2 * FRAME), t2 + 2 * FRAME);
        assert!(!d.active());
    }

    /// A compositor that takes each margin change as late as the proof allows: just before the second frame drawn
    /// after it was made. Pointer positions it reports are relative to where it has the surface.
    #[test]
    fn a_margin_drag_never_drifts_even_when_the_compositor_is_late() {
        let mut d = drag(Strategy::Margin);
        let mut now = Instant::now();
        let mut applied = origin(HOME, OUTPUT);
        let press = applied + (GRAB - Point::ORIGIN);
        // in flight: where the surface goes, and frames drawn since it was sent
        let mut pending: Option<(Point, u8)> = None;
        d.handle(Input::Press(GRAB), now);
        // the pointer sweeps left and up, two moves a frame, then stops
        let path: Vec<Point> = (1..=60)
            .map(|i| press + Vector::new(-7.0 * i as f32, -3.0 * i as f32))
            .collect();
        let mut frame = 0;
        for (i, pointer) in path
            .iter()
            .chain(std::iter::repeat_n(path.last().unwrap(), 8))
            .enumerate()
        {
            let step = d.handle(Input::Motion(*pointer - (applied - Point::ORIGIN)), now);
            if let Some(&action) = step.actions.first() {
                assert!(pending.is_none(), "one move in flight at a time");
                pending = Some((placed(action), 0));
                d.handle(Input::Committed, now);
                // a frame drawn just before the move was made comes round after it
                d.handle(Input::Redraw(now - Duration::from_millis(1)), now);
            }
            if i % 2 == 1 {
                now += FRAME;
                frame += 1;
                if let Some((to, drawn)) = pending.as_mut() {
                    *drawn += 1;
                    if *drawn == 2 {
                        applied = *to;
                        pending = None;
                    }
                }
                d.handle(Input::Redraw(now), now);
            }
        }
        assert!(frame > 30);
        let last = *path.last().unwrap();
        assert_eq!(
            applied,
            last - (GRAB - Point::ORIGIN),
            "the grab point ends under the pointer"
        );
        assert_eq!(origin(d.home, OUTPUT), applied);
    }

    #[test]
    fn a_margin_drag_keeps_the_sprite_on_the_output() {
        let mut d = drag(Strategy::Margin);
        let t = Instant::now();
        d.handle(Input::Press(GRAB), t);
        // far past the right and bottom edges
        let step = d.handle(Input::Motion(GRAB + Vector::new(5000.0, 5000.0)), t);
        let to = placed(step.actions[0]);
        assert_eq!(to.x + SPRITE.x + SPRITE.width, OUTPUT.0 as f32);
        assert_eq!(to.y + SPRITE.y + SPRITE.height, OUTPUT.1 as f32);
        d.handle(Input::Committed, t);
        d.handle(Input::Redraw(t + FRAME), t + FRAME);
        d.handle(Input::Redraw(t + 2 * FRAME), t + 2 * FRAME);
        // and past the top-left corner: the sprite's top-left stops at the output's
        let step = d.handle(Input::Motion(GRAB + Vector::new(-9000.0, -9000.0)), t + 2 * FRAME);
        let to = placed(step.actions[0]);
        assert_eq!((to.x + SPRITE.x, to.y + SPRITE.y), (0.0, 0.0));
        // pushing on at the edge asks nothing more
        d.handle(Input::Committed, t + 2 * FRAME);
        d.handle(Input::Redraw(t + 3 * FRAME), t + 3 * FRAME);
        d.handle(Input::Redraw(t + 4 * FRAME), t + 4 * FRAME);
        let step = d.handle(Input::Motion(GRAB + Vector::new(-50.0, 0.0)), t + 4 * FRAME);
        assert!(step.actions.is_empty() && step.pet.is_some());
    }

    #[test]
    fn a_home_the_output_no_longer_holds_is_moved_onto_it() {
        let mut d = Drag::new(
            Strategy::Margin,
            Home {
                right: 1800,
                bottom: 1000,
            },
        );
        let to = d.set_output(Some((1280, 800)));
        assert_eq!(
            d.home,
            Home {
                right: 1280 - 255,
                bottom: 800 - 132
            }
        );
        assert_eq!(to, vec![Action::Margins(-468, 1025, 668, -125)]);
    }

    #[test]
    fn an_overlay_drag_grows_shifts_follows_and_goes_home_keeping_the_pet_in_place() {
        let mut d = drag(Strategy::Overlay);
        let t0 = Instant::now();
        let origin = origin(HOME, OUTPUT);
        d.handle(Input::Press(GRAB), t0);
        // past the threshold: all four margins (so the top-left stays put), then the output's size from there
        let step = d.handle(Input::Motion(GRAB + Vector::new(8.0, 0.0)), t0);
        assert_eq!(
            step.actions,
            vec![Action::Margins(600, 420, 0, 1120), Action::Cover(1920, 1200)]
        );
        assert_eq!(d.offset(), Vector::ZERO);
        // growing: positions are still the box's
        let step = d.handle(Input::Motion(GRAB + Vector::new(20.0, 5.0)), t0 + FRAME);
        assert_eq!(moved(&step), Vector::new(20.0, 5.0));
        // grown: the pet is drawn where it is on the output from the next frame, and the corner follows that
        // frame, not one drawn before
        let t1 = t0 + 3 * FRAME;
        assert!(
            d.handle(Input::Resized(Size::new(1920.0, 1200.0)), t1)
                .actions
                .is_empty()
        );
        assert_eq!(d.offset(), origin - Point::ORIGIN);
        assert!(d.handle(Input::Redraw(t1 - FRAME), t1).actions.is_empty());
        let step = d.handle(Input::Redraw(t1 + FRAME), t1 + FRAME);
        assert_eq!(step.actions, vec![Action::Margins(0, 0, 0, 0)]);
        assert!(step.confirm);
        d.handle(Input::Committed, t1 + FRAME);
        // positions are ambiguous until the shift lands
        assert!(d.handle(Input::Motion(Point::new(5.0, 5.0)), t1 + FRAME).pet.is_none());
        d.handle(Input::Redraw(t1 + 2 * FRAME), t1 + 2 * FRAME);
        d.handle(Input::Redraw(t1 + 3 * FRAME), t1 + 3 * FRAME);
        // landed: the pet goes under the last pointer position seen, then follows the output's positions
        assert_eq!(d.offset(), Vector::new(1140.0, 605.0));
        let step = d.handle(Input::Motion(Point::new(700.5, 300.2)), t1 + 3 * FRAME);
        assert_eq!(moved(&step), Vector::new(700.5 - 1310.0, 300.2 - 1130.0));
        assert_eq!(d.offset(), Vector::new(511.0, -230.0));
        // released: drawn at the surface's corner again, which follows the first frame drawn so, with home
        let t2 = t1 + 4 * FRAME;
        assert!(matches!(d.handle(Input::Release, t2).pet, Some(Pet::DragEnded)));
        assert_eq!(d.offset(), Vector::ZERO);
        assert!(d.handle(Input::Redraw(t2 - FRAME), t2).actions.is_empty());
        let step = d.handle(Input::Redraw(t2 + FRAME), t2 + FRAME);
        assert_eq!(step.actions, vec![Action::Margins(-230, 1029, 830, 511), Action::Home]);
        assert_eq!(
            d.home,
            Home {
                right: 1029,
                bottom: 830
            }
        );
        // settles once the surface is its home size again
        assert!(d.active());
        d.handle(Input::Resized(SURFACE), t2 + 2 * FRAME);
        assert!(!d.active());
    }

    #[test]
    fn an_overlay_drag_released_while_growing_goes_home_without_moving_the_corner() {
        let mut d = drag(Strategy::Overlay);
        let t = Instant::now();
        d.handle(Input::Press(GRAB), t);
        d.handle(Input::Motion(GRAB + Vector::new(0.0, 6.0)), t);
        assert!(matches!(d.handle(Input::Release, t + FRAME).pet, Some(Pet::DragEnded)));
        let step = d.handle(Input::Resized(Size::new(1920.0, 1200.0)), t + 2 * FRAME);
        assert_eq!(step.actions, vec![Action::Home]);
        assert_eq!(d.offset(), Vector::ZERO);
        d.handle(Input::Resized(SURFACE), t + 3 * FRAME);
        assert!(!d.active());
        assert_eq!(d.home, HOME);
    }

    #[test]
    fn an_overlay_drag_released_while_shifting_finishes_the_shift_then_goes_home() {
        let mut d = drag(Strategy::Overlay);
        let t = Instant::now();
        d.handle(Input::Press(GRAB), t);
        d.handle(Input::Motion(GRAB + Vector::new(30.0, 0.0)), t);
        d.handle(Input::Resized(Size::new(1920.0, 1200.0)), t);
        d.handle(Input::Release, t);
        assert_eq!(
            d.handle(Input::Redraw(t + FRAME), t + FRAME).actions,
            vec![Action::Margins(0, 0, 0, 0)]
        );
        d.handle(Input::Committed, t + FRAME);
        d.handle(Input::Redraw(t + 2 * FRAME), t + 2 * FRAME);
        d.handle(Input::Redraw(t + 3 * FRAME), t + 3 * FRAME);
        // the pet never followed: it goes home where it started
        assert_eq!(d.offset(), Vector::ZERO);
        let step = d.handle(Input::Redraw(t + 4 * FRAME), t + 4 * FRAME);
        assert_eq!(step.actions, vec![Action::Margins(600, 420, 0, 1120), Action::Home]);
    }

    #[test]
    fn an_overlay_drag_gives_up_on_a_surface_that_never_grows() {
        let mut d = drag(Strategy::Overlay);
        let t = Instant::now();
        d.handle(Input::Press(GRAB), t);
        d.handle(Input::Motion(GRAB + Vector::new(30.0, 0.0)), t);
        let step = d.handle(Input::Redraw(t + PATIENCE + FRAME), t + PATIENCE + FRAME);
        assert_eq!(step.actions, vec![Action::Home]);
        assert!(matches!(step.pet, Some(Pet::DragEnded)));
        assert!(!d.active());
    }

    #[test]
    fn a_margin_move_counts_only_frames_drawn_after_it_was_made() {
        let mut d = drag(Strategy::Margin);
        let t = Instant::now();
        d.handle(Input::Press(GRAB), t);
        let step = d.handle(Input::Motion(GRAB + Vector::new(10.0, 0.0)), t);
        assert!(step.confirm && step.actions.len() == 1);
        // the move is still on its way to the compositor: frames drawn meanwhile prove nothing
        d.handle(Input::Redraw(t + FRAME), t + FRAME);
        d.handle(Input::Redraw(t + 2 * FRAME), t + 2 * FRAME);
        assert!(d.handle(Input::Motion(GRAB), t + 2 * FRAME).pet.is_none());
        let made = t + 2 * FRAME + Duration::from_millis(1);
        d.handle(Input::Committed, made);
        d.handle(Input::Redraw(t + 3 * FRAME), t + 3 * FRAME);
        assert!(d.handle(Input::Motion(GRAB), t + 3 * FRAME).pet.is_none());
        d.handle(Input::Redraw(t + 4 * FRAME), t + 4 * FRAME);
        assert_eq!(d.home, Home { right: 410, bottom: 0 });
        assert_eq!(
            moved(&d.handle(Input::Motion(GRAB), t + 4 * FRAME)),
            Vector::new(10.0, 0.0)
        );
    }

    #[test]
    fn a_drag_lost_with_the_surface_lets_a_held_pet_go() {
        let mut d = drag(Strategy::Margin);
        let t = Instant::now();
        assert!(d.reset().is_none());
        d.handle(Input::Press(GRAB), t);
        assert!(matches!(d.reset(), Some(Pet::DragCancelled)));
        assert!(!d.active() && d.reset().is_none());
        // an overlay drag that was already released has told the pet so
        let mut d = drag(Strategy::Overlay);
        d.handle(Input::Press(GRAB), t);
        d.handle(Input::Motion(GRAB + Vector::new(30.0, 0.0)), t);
        assert!(matches!(d.reset(), Some(Pet::DragCancelled)));
        d.handle(Input::Press(GRAB), t);
        d.handle(Input::Motion(GRAB + Vector::new(30.0, 0.0)), t);
        d.handle(Input::Release, t);
        assert!(d.active() && d.reset().is_none());
    }

    #[test]
    fn an_overlay_drag_at_a_fractional_scale_knows_the_home_size() {
        let mut d = drag(Strategy::Overlay);
        let t = Instant::now();
        d.handle(Input::Press(GRAB), t);
        d.handle(Input::Motion(GRAB + Vector::new(0.0, 6.0)), t);
        // at 4/3 the home size comes back as 380.25 × 600: not grown yet
        let home = Size::new(380.25, 600.0);
        d.handle(Input::Resized(home), t);
        assert_eq!(d.describe(), "growing over the output");
        d.handle(Input::Release, t + FRAME);
        assert_eq!(
            d.handle(Input::Resized(Size::new(1920.0, 1200.0)), t + 2 * FRAME)
                .actions,
            vec![Action::Home]
        );
        // back home: the next press counts at once
        d.handle(Input::Resized(home), t + 3 * FRAME);
        assert!(!d.active());
        assert!(matches!(
            d.handle(Input::Press(GRAB), t + 3 * FRAME).pet,
            Some(Pet::DragStarted)
        ));
    }

    #[test]
    fn a_drag_places_the_surface_only_at_rest_on_a_known_output() {
        assert!(Drag::new(Strategy::Margin, HOME).placed().is_none());
        let mut d = drag(Strategy::Margin);
        assert_eq!(d.placed(), Some((Point::new(1120.0, 600.0), OUTPUT)));
        d.handle(Input::Press(GRAB), Instant::now());
        assert!(d.placed().is_none());
    }

    #[test]
    fn an_overlay_drag_without_a_known_output_moves_by_margins() {
        let mut d = Drag::new(Strategy::Overlay, HOME);
        let t = Instant::now();
        d.handle(Input::Press(GRAB), t);
        let step = d.handle(Input::Motion(GRAB + Vector::new(-10.0, 0.0)), t);
        assert_eq!(step.actions, vec![Action::Margins(0, 430, 0, 0)]);
    }
}
