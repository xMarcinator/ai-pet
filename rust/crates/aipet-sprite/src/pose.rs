//! The pet's pose: what its behaviour says each part of it shows, without saying how to draw it.
//!
//! [`Pet::update`](crate::Pet::update) computes one every frame from the [`PetInput`](crate::PetInput), the time and
//! the random source, next to the pixels it still draws itself. A pose is a set of channels (the mood, the eyes, the
//! arms, the feet, the prop, the headphones, the effect and the facing), each with the time its value last changed,
//! plus the whole-body motion. A pack's layers are bound to channels, and pick their animations by the channel's
//! value ([`crate::compose`]).

/// The art box: the sprite's size in logical px (26 × 24 grid cells of 5 px), which every pack's art fits.
pub const ART_W: f64 = 130.0;
pub const ART_H: f64 = 120.0;
/// How far art and part positions may reach outside the art box: the glow's halo.
pub const HALO_PAD: f64 = 10.0;
/// The point on the ground shadow, in art-box px: the bottom centre of the sprite, as the UI scales it about.
pub const ANCHOR: (f64, f64) = (ART_W * 0.5, ART_H * 0.917);

/// A pose channel: what a pack layer is bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Channel {
    State,
    Eyes,
    Arms,
    Feet,
    Prop,
    Headphones,
    Fx,
    Facing,
}

impl Channel {
    pub const ALL: [Channel; 8] = [
        Channel::State,
        Channel::Eyes,
        Channel::Arms,
        Channel::Feet,
        Channel::Prop,
        Channel::Headphones,
        Channel::Fx,
        Channel::Facing,
    ];

    /// The channel a manifest names, by its name.
    pub fn parse(name: &str) -> Option<Channel> {
        Channel::ALL.into_iter().find(|c| c.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Channel::State => "state",
            Channel::Eyes => "eyes",
            Channel::Arms => "arms",
            Channel::Feet => "feet",
            Channel::Prop => "prop",
            Channel::Headphones => "headphones",
            Channel::Fx => "fx",
            Channel::Facing => "facing",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The mood the pet animates: the chats' state, or `Listen` when music replaces idling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Idle,
    Sleep,
    Thinking,
    Working,
    Attention,
    Done,
    Listen,
}

impl Mood {
    /// The mood of a state name; an unknown one is idle's bob without idle acts in the C#, so it maps to idle.
    pub(crate) fn of(state: &str) -> Mood {
        match state {
            "sleep" => Mood::Sleep,
            "thinking" => Mood::Thinking,
            "working" => Mood::Working,
            "attention" => Mood::Attention,
            "done" => Mood::Done,
            "listen" => Mood::Listen,
            _ => Mood::Idle,
        }
    }
}

/// One of the random things the pet does while idle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleAct {
    Look,
    Hop,
    Stretch,
    Wiggle,
    Tap,
}

impl IdleAct {
    pub(crate) fn of(name: &str) -> Option<IdleAct> {
        Some(match name {
            "look" => IdleAct::Look,
            "hop" => IdleAct::Hop,
            "stretch" => IdleAct::Stretch,
            "wiggle" => IdleAct::Wiggle,
            "tap" => IdleAct::Tap,
            _ => return None,
        })
    }
}

/// The `state` channel: the mood, with the idle act while one is playing (and shown: not while hovered or held).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateValue {
    pub mood: Mood,
    pub act: Option<IdleAct>,
}

impl StateValue {
    /// Its key in a pack: `idle`, `idle-hop`, `sleep`, …; a pack without `idle-hop` falls back as its manifest says.
    pub fn key(self) -> &'static str {
        match (self.mood, self.act) {
            (Mood::Idle, Some(IdleAct::Look)) => "idle-look",
            (Mood::Idle, Some(IdleAct::Hop)) => "idle-hop",
            (Mood::Idle, Some(IdleAct::Stretch)) => "idle-stretch",
            (Mood::Idle, Some(IdleAct::Wiggle)) => "idle-wiggle",
            (Mood::Idle, Some(IdleAct::Tap)) => "idle-tap",
            (Mood::Idle, None) => "idle",
            (Mood::Sleep, _) => "sleep",
            (Mood::Thinking, _) => "thinking",
            (Mood::Working, _) => "working",
            (Mood::Attention, _) => "attention",
            (Mood::Done, _) => "done",
            (Mood::Listen, _) => "listen",
        }
    }
}

/// The eyes' expression. Blinking is `Closed`, as sleeping is; `Down` (reading the laptop) is kept apart from `Open`
/// so a pack can draw it, and falls back like any other value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expression {
    Open,
    Closed,
    Happy,
    Squint,
    Wide,
    Up,
    Down,
}

impl Expression {
    pub fn key(self) -> &'static str {
        match self {
            Expression::Open => "open",
            Expression::Closed => "closed",
            Expression::Happy => "happy",
            Expression::Squint => "squint",
            Expression::Wide => "wide",
            Expression::Up => "up",
            Expression::Down => "down",
        }
    }

    /// The renderer's eye names (`eye_cells`).
    pub(crate) fn of(eyes: &str) -> Expression {
        match eyes {
            "happy" => Expression::Happy,
            "sleep" | "blink" => Expression::Closed,
            "squint" => Expression::Squint,
            "wide" => Expression::Wide,
            "up" => Expression::Up,
            "down" => Expression::Down,
            _ => Expression::Open,
        }
    }
}

/// Where the eyes look: one of 9 directions, the centre included. `dx` is −1 (left), 0 or 1 (right); `dy` is −1
/// (up), 0 or 1 (down), in the unmirrored art.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Look {
    pub dx: i8,
    pub dy: i8,
}

impl Look {
    pub const CENTER: Look = Look { dx: 0, dy: 0 };

    /// All nine, the centre first.
    pub const ALL: [Look; 9] = [
        Look { dx: 0, dy: 0 },
        Look { dx: 0, dy: -1 },
        Look { dx: 1, dy: -1 },
        Look { dx: 1, dy: 0 },
        Look { dx: 1, dy: 1 },
        Look { dx: 0, dy: 1 },
        Look { dx: -1, dy: 1 },
        Look { dx: -1, dy: 0 },
        Look { dx: -1, dy: -1 },
    ];

    pub fn key(self) -> &'static str {
        match (self.dx, self.dy) {
            (0, -1) => "up",
            (1, -1) => "up-right",
            (1, 0) => "right",
            (1, 1) => "down-right",
            (0, 1) => "down",
            (-1, 1) => "down-left",
            (-1, 0) => "left",
            (-1, -1) => "up-left",
            _ => "center",
        }
    }

    pub fn parse(key: &str) -> Option<Look> {
        Look::ALL.into_iter().find(|l| l.key() == key)
    }
}

/// The `eyes` channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Eyes {
    pub expression: Expression,
    pub look: Look,
}

/// The `arms` channel: the renderer's arm poses, plus holding a prop while working (drawn as `Rest` by today's
/// renderer; a pack without them falls back).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arms {
    Rest,
    Raised,
    WaveA,
    WaveB,
    RunA,
    RunB,
    Hold,
    Lens,
}

impl Arms {
    pub fn key(self) -> &'static str {
        match self {
            Arms::Rest => "rest",
            Arms::Raised => "raised",
            Arms::WaveA => "wave-a",
            Arms::WaveB => "wave-b",
            Arms::RunA => "run-a",
            Arms::RunB => "run-b",
            Arms::Hold => "hold",
            Arms::Lens => "lens",
        }
    }
}

/// The `feet` channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feet {
    Stand,
    Tap,
    DangleA,
    DangleB,
    RunA,
    RunB,
}

impl Feet {
    pub fn key(self) -> &'static str {
        match self {
            Feet::Stand => "stand",
            Feet::Tap => "tap",
            Feet::DangleA => "dangle-a",
            Feet::DangleB => "dangle-b",
            Feet::RunA => "run-a",
            Feet::RunB => "run-b",
        }
    }
}

/// The `prop` channel: what the pet holds while working.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    None,
    Laptop,
    Lens,
}

impl Prop {
    pub fn key(self) -> &'static str {
        match self {
            Prop::None => "none",
            Prop::Laptop => "laptop",
            Prop::Lens => "lens",
        }
    }
}

/// The `fx` channel: one effect at a time. Today's renderer can draw the sparkles of a poke together with the dots
/// of thinking; a pose says sparkles then, as the poke is the reaction to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fx {
    None,
    Zs,
    Dots,
    Notes,
    Alert,
    Sparkles,
    Dust,
}

impl Fx {
    pub fn key(self) -> &'static str {
        match self {
            Fx::None => "none",
            Fx::Zs => "zs",
            Fx::Dots => "dots",
            Fx::Notes => "notes",
            Fx::Alert => "alert",
            Fx::Sparkles => "sparkles",
            Fx::Dust => "dust",
        }
    }
}

/// The `facing` channel. The art is drawn facing right; facing left mirrors it, if the pack allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    Left,
    Right,
}

impl Facing {
    pub fn key(self) -> &'static str {
        match self {
            Facing::Left => "left",
            Facing::Right => "right",
        }
    }
}

/// The whole-body motion of a frame: offset and squash in art-box px about [`ANCHOR`], and the shadow.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Motion {
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub shadow_scale: f64,
    pub shadow_opacity: f64,
}

/// A frame's pose: each channel's value, when it last changed (pet-clock seconds), and the motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub state: StateValue,
    pub eyes: Eyes,
    pub arms: Arms,
    pub feet: Feet,
    pub prop: Prop,
    pub headphones: bool,
    pub fx: Fx,
    pub facing: Facing,
    pub motion: Motion,
    since: [f64; 8],
}

impl Pose {
    /// A channel's value, as a pack names it.
    pub fn key(&self, channel: Channel) -> &'static str {
        match channel {
            Channel::State => self.state.key(),
            Channel::Eyes => self.eyes.expression.key(),
            Channel::Arms => self.arms.key(),
            Channel::Feet => self.feet.key(),
            Channel::Prop => self.prop.key(),
            Channel::Headphones => {
                if self.headphones {
                    "on"
                } else {
                    "off"
                }
            }
            Channel::Fx => self.fx.key(),
            Channel::Facing => self.facing.key(),
        }
    }

    /// When a channel's value last changed. A channel's animation starts then.
    pub fn since(&self, channel: Channel) -> f64 {
        self.since[channel.index()]
    }
}

/// A pose's values without the times or the motion: what one frame's behaviour says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Values {
    pub state: StateValue,
    pub eyes: Eyes,
    pub arms: Arms,
    pub feet: Feet,
    pub prop: Prop,
    pub headphones: bool,
    pub fx: Fx,
    pub facing: Facing,
}

impl Values {
    fn keys(&self) -> [&'static str; 8] {
        let pose = Pose {
            state: self.state,
            eyes: self.eyes,
            arms: self.arms,
            feet: self.feet,
            prop: self.prop,
            headphones: self.headphones,
            fx: self.fx,
            facing: self.facing,
            motion: Motion::default(),
            since: [0.0; 8],
        };
        Channel::ALL.map(|c| pose.key(c))
    }
}

/// How far (DIPs) the pointer goes past an edge between two look directions before the eyes turn, and back before
/// they return: without it, a pointer resting near an edge would make them flicker between the two.
pub const LOOK_HYSTERESIS: f64 = 4.0;
/// The pointer's distance from the sprite's centre (DIPs) where the eyes turn sideways: the renderer's eye-follow
/// rounds `(x − 65) / 28` to the nearest whole step, so its first step is at 14.
const LOOK_SIDE: f64 = 14.0;
/// The pointer's height (DIPs from the sprite's top) above which the eyes look up, as the renderer's `11 * P - 40`.
const LOOK_UP: f64 = 15.0;

/// Times each channel's value, and turns the eyes towards the pointer with hysteresis. One per pet.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tracker {
    last: Option<([&'static str; 8], [f64; 8])>,
    pointer: Look,
}

impl Tracker {
    /// The eyes' direction from a hovering pointer over the sprite (DIPs), or `None` without one.
    pub fn pointer_look(&mut self, pointer: Option<(f64, f64)>) -> Option<Look> {
        let Some((x, y)) = pointer else {
            self.pointer = Look::CENTER;
            return None;
        };
        let was = self.pointer;
        let d = x - ART_W * 0.5;
        let dx = if d.abs() >= LOOK_SIDE + LOOK_HYSTERESIS {
            d.signum() as i8
        } else if d.abs() < LOOK_SIDE - LOOK_HYSTERESIS || (was.dx != 0 && d.signum() as i8 != was.dx) {
            0
        } else {
            was.dx
        };
        let dy = if y < LOOK_UP - LOOK_HYSTERESIS {
            -1
        } else if y >= LOOK_UP + LOOK_HYSTERESIS {
            0
        } else {
            was.dy
        };
        self.pointer = Look { dx, dy };
        Some(self.pointer)
    }

    /// The pose for `values` at `t`, with each channel's time of change.
    pub fn pose(&mut self, values: Values, motion: Motion, t: f64) -> Pose {
        let keys = values.keys();
        let since = match self.last {
            Some((last, mut since)) => {
                for (i, key) in keys.iter().enumerate() {
                    if last[i] != *key {
                        since[i] = t;
                    }
                }
                since
            }
            None => [t; 8],
        };
        self.last = Some((keys, since));
        Pose {
            state: values.state,
            eyes: values.eyes,
            arms: values.arms,
            feet: values.feet,
            prop: values.prop,
            headphones: values.headphones,
            fx: values.fx,
            facing: values.facing,
            motion,
            since,
        }
    }

    /// Forgets the times, so the next pose starts every channel afresh (a new avatar).
    pub fn reset(&mut self) {
        self.last = None;
    }
}

impl Values {
    /// Idle, eyes open and ahead, arms at rest, standing, nothing held or shown, facing right.
    pub const IDLE: Values = Values {
        state: StateValue {
            mood: Mood::Idle,
            act: None,
        },
        eyes: Eyes {
            expression: Expression::Open,
            look: Look::CENTER,
        },
        arms: Arms::Rest,
        feet: Feet::Stand,
        prop: Prop::None,
        headphones: false,
        fx: Fx::None,
        facing: Facing::Right,
    };
}

impl Pose {
    /// A pose with `values`, every channel changed at `since`, and no motion: for tests and tools that script poses.
    pub fn scripted(values: Values, since: f64) -> Pose {
        let mut pose = Tracker::default().pose(values, Motion::default(), since);
        pose.motion = Motion {
            scale_x: 1.0,
            scale_y: 1.0,
            shadow_scale: 1.0,
            shadow_opacity: 1.0,
            ..Motion::default()
        };
        pose
    }

    /// The same pose with one channel's time of change set to `at`.
    pub fn changed_at(mut self, channel: Channel, at: f64) -> Pose {
        self.since[channel.index()] = at;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_look_turns_past_the_edge_and_back_with_hysteresis() {
        let mut t = Tracker::default();
        let at = |t: &mut Tracker, x: f64| t.pointer_look(Some((x, 60.0))).unwrap().dx;
        assert_eq!(at(&mut t, 65.0), 0);
        // across the renderer's edge at 14 px right of centre, but within the band: still centre
        assert_eq!(at(&mut t, 65.0 + 15.0), 0);
        assert_eq!(at(&mut t, 65.0 + 17.9), 0);
        // past it: right, and it stays right back across the edge, until well inside
        assert_eq!(at(&mut t, 65.0 + 18.0), 1);
        assert_eq!(at(&mut t, 65.0 + 13.0), 1);
        assert_eq!(at(&mut t, 65.0 + 10.1), 1);
        assert_eq!(at(&mut t, 65.0 + 9.9), 0);
        // to the left side at once when the pointer jumps across
        assert_eq!(at(&mut t, 65.0 + 18.0), 1);
        assert_eq!(at(&mut t, 65.0 - 30.0), -1);
        // a pointer wobbling on the edge never flickers
        let mut seen = Vec::new();
        for i in 0..20 {
            seen.push(at(&mut t, 65.0 - 14.0 + if i % 2 == 0 { 2.0 } else { -2.0 }));
        }
        assert!(seen.iter().all(|&d| d == -1), "{seen:?}");
    }

    #[test]
    fn the_look_goes_up_above_the_line_with_hysteresis_and_centres_without_a_pointer() {
        let mut t = Tracker::default();
        let dy = |t: &mut Tracker, y: f64| t.pointer_look(Some((65.0, y))).unwrap().dy;
        assert_eq!(dy(&mut t, 30.0), 0);
        assert_eq!(dy(&mut t, 12.0), 0);
        assert_eq!(dy(&mut t, 10.9), -1);
        assert_eq!(dy(&mut t, 17.0), -1);
        assert_eq!(dy(&mut t, 19.0), 0);
        assert_eq!(t.pointer_look(None), None);
        assert_eq!(dy(&mut t, 17.0), 0, "a new hover starts from the centre");
    }

    #[test]
    fn a_channel_keeps_its_time_until_its_value_changes() {
        let mut tr = Tracker::default();
        let values = Values::IDLE;
        let p = tr.pose(values, Motion::default(), 1.0);
        assert_eq!(p.since(Channel::State), 1.0);
        let p = tr.pose(values, Motion::default(), 2.0);
        assert_eq!(p.since(Channel::Arms), 1.0);
        let waving = Values {
            arms: Arms::WaveA,
            ..values
        };
        let p = tr.pose(waving, Motion::default(), 3.0);
        assert_eq!((p.since(Channel::Arms), p.since(Channel::State)), (3.0, 1.0));
        assert_eq!(p.key(Channel::Arms), "wave-a");
        assert_eq!(p.key(Channel::Headphones), "off");
    }

    #[test]
    fn every_look_has_a_key_that_parses_back() {
        for look in Look::ALL {
            assert_eq!(Look::parse(look.key()), Some(look));
        }
        assert_eq!(Channel::parse("eyes"), Some(Channel::Eyes));
        assert_eq!(Channel::parse("tail"), None);
    }
}
