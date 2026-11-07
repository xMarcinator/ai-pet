//! A pet as a pack, in memory: layers bound to pose channels, each with its animations, parts and part motion.
//!
//! Nothing here reads files (task 3 loads manifests into this model). [`Pack::validate`] checks what a manifest's
//! loader can't see from the JSON alone: every layer's default, the fallback chains, loop modes, durations, the part
//! ids and the numbers' ranges. A pack that fails is skipped with the reason it gives.

use std::collections::{BTreeMap, BTreeSet};

use crate::pose::{ART_H, ART_W, Channel, HALO_PAD, Look};
use crate::vector::{Cap, Join, Seg};

/// At most this many layers in a pack.
pub const MAX_LAYERS: usize = 16;
/// At most this many frames in a pack, over all its layers and animations.
pub const MAX_FRAMES: usize = 512;
/// At most this many keys in a part animation.
pub const MAX_KEYS: usize = 64;
/// At most this many steps from a value to the layer's default through its fallbacks.
pub const MAX_FALLBACK: usize = 8;
/// A frame's duration: 1 ms to 60 s.
pub const FRAME_MS: std::ops::RangeInclusive<u32> = 1..=60_000;
/// A part animation's key: 1 ms to 10 s; a pose transition: 0 (jump) to 10 s.
pub const KEY_MS: std::ops::RangeInclusive<u32> = 1..=10_000;
pub const TRANSITION_MS: std::ops::RangeInclusive<u32> = 0..=10_000;
/// How far any coordinate may go, in px.
pub const COORD_LIMIT: f64 = 10_000.0;

/// How a pack is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Pixel-art sheets, crisp at any scale.
    Pixel,
    /// Smooth high-resolution sheets.
    Painted,
    /// Parts drawn from path data.
    Vector,
}

impl Kind {
    pub fn parse(name: &str) -> Option<Kind> {
        Some(match name {
            "pixel" => Kind::Pixel,
            "painted" => Kind::Painted,
            "vector" => Kind::Vector,
            _ => return None,
        })
    }
}

/// Whether facing left mirrors the art.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mirror {
    Flip,
    None,
}

/// How an animation plays: over and over, once (holding its last frame), or there and back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopMode {
    Loop,
    Once,
    PingPong,
}

impl LoopMode {
    pub fn parse(name: &str) -> Result<LoopMode, String> {
        match name {
            "loop" => Ok(LoopMode::Loop),
            "once" => Ok(LoopMode::Once),
            "pingpong" => Ok(LoopMode::PingPong),
            other => Err(format!("unknown loop mode {other:?} (loop, once or pingpong)")),
        }
    }
}

/// What a frame draws.
#[derive(Clone, Debug, PartialEq)]
pub enum Art {
    /// A rectangle of a sheet (by its index in the pack's sheets), with the pixel that sits on its part's origin
    /// (for a whole-layer frame, on the art box's top-left).
    Raster {
        sheet: usize,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        pivot: (f64, f64),
    },
    /// Vector parts draw their shapes instead (task 5); the frame only times them.
    Vector,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub art: Art,
    pub ms: u32,
}

/// Which clock an animation plays by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Clock {
    /// From when its channel's value last changed (the default).
    #[default]
    Change,
    /// From the pet clock's start, so it stays in step however often its value comes and goes: today's effects and
    /// the lens's scan.
    Pet,
}

/// A layer's frames for one value of its channel.
#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub frames: Vec<Frame>,
    pub mode: LoopMode,
    pub clock: Clock,
}

/// A vector shape: path data in SVG `d` syntax (parsed at load), drawn around its part's origin, with its paint.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub d: String,
    pub path: Vec<Seg>,
    /// Straight 0xAARRGGBB; `None` paints nothing.
    pub fill: Option<u32>,
    pub stroke: Option<u32>,
    /// The stroke's width, in the part's px.
    pub width: f64,
    pub cap: Cap,
    pub join: Join,
    pub opacity: f64,
    /// The palette colours the fill and stroke name (`$robe`), which a theme changes.
    pub fill_name: Option<String>,
    pub stroke_name: Option<String>,
}

impl Shape {
    /// A shape filled with `colour` and no stroke. Panics on bad path data (for the code's own shapes).
    pub fn filled(d: &str, colour: u32) -> Shape {
        Shape {
            d: d.to_owned(),
            path: crate::vector::parse(d).expect("the shape's path data"),
            fill: Some(colour),
            stroke: None,
            width: 1.0,
            cap: Cap::Butt,
            join: Join::Miter,
            opacity: 1.0,
            fill_name: None,
            stroke_name: None,
        }
    }
}

/// A movable piece of a layer: a unique id, and its origin in art-box px.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub id: String,
    pub origin: (f64, f64),
    /// The shape it draws whatever the value, if any (else the value's shape, by the layer's fallbacks).
    pub shape: Option<String>,
}

/// One key of a part animation: offsets on top of the pose position, held for `ms`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub dx: f64,
    pub dy: f64,
    /// Degrees about the part's origin (vector parts).
    pub rotate: f64,
    pub scale: f64,
    pub opacity: f64,
    pub ms: u32,
}

impl Key {
    /// An offset key: `[dx, dy, ms]`.
    pub fn offset(dx: f64, dy: f64, ms: u32) -> Key {
        Key {
            dx,
            dy,
            rotate: 0.0,
            scale: 1.0,
            opacity: 1.0,
            ms,
        }
    }
}

/// Keyframes for some of a layer's parts, for one value of its channel. Each part may start at its own `phase`
/// (0–1 of the loop), so two hands can bob out of step.
#[derive(Clone, Debug, PartialEq)]
pub struct PartAnimation {
    pub parts: Vec<(String, f64)>,
    pub mode: LoopMode,
    pub keys: Vec<Key>,
}

/// A layer: drawn in the pack's order, bound to one channel.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub channel: Channel,
    /// Whether its pixels take clicks.
    pub hit: bool,
    /// Whether it glows: the UI blurs it in the pack's glow colour. An effects layer (the fx channel) never does.
    pub glow: bool,
    /// Its own mirror policy, where it differs from the pack's (the baked pets mirror only running limbs and dust).
    pub mirror: Option<Mirror>,
    /// Whether a theme changes its sheets' colours (a pixel pack): every layer but the effects, unless it says.
    pub themed: bool,
    pub default: String,
    /// Where a value the layer lacks goes next; `None` draws nothing for it (no headphones layer when off).
    pub fallback: BTreeMap<String, Option<String>>,
    /// Frames per value.
    pub animations: BTreeMap<String, Animation>,
    pub parts: Vec<Part>,
    /// The drawing per value or by name (vector packs): its shapes, bottom to top.
    pub shapes: BTreeMap<String, Vec<Shape>>,
    /// The eyes' offset per direction (the eyes channel only); missing directions take the nearest given.
    pub look: BTreeMap<Look, (f64, f64)>,
    /// Target positions per value, for some or all parts.
    pub poses: BTreeMap<String, BTreeMap<String, (f64, f64)>>,
    pub transition_ms: u32,
    pub part_animations: BTreeMap<String, PartAnimation>,
}

impl Layer {
    /// A layer bound to `channel` with nothing in it yet.
    pub fn new(name: &str, channel: Channel, default: &str) -> Layer {
        Layer {
            name: name.to_owned(),
            channel,
            hit: false,
            glow: false,
            mirror: None,
            themed: channel != Channel::Fx,
            default: default.to_owned(),
            fallback: BTreeMap::new(),
            animations: BTreeMap::new(),
            parts: Vec::new(),
            shapes: BTreeMap::new(),
            look: BTreeMap::new(),
            poses: BTreeMap::new(),
            transition_ms: 0,
            part_animations: BTreeMap::new(),
        }
    }

    /// Whether the layer has anything for `value`: frames, a shape, a pose or a part animation.
    pub fn has(&self, value: &str) -> bool {
        self.animations.contains_key(value)
            || self.shapes.contains_key(value)
            || self.poses.contains_key(value)
            || self.part_animations.contains_key(value)
    }

    /// The value the layer shows for `value`: itself if it has it, else its fallbacks'; then, for a combined value
    /// (`zs+sparkles`), its last part; else the default. `None` when a fallback says to draw nothing.
    /// [`Pack::validate`] has checked the chains end.
    pub fn resolve<'a>(&'a self, value: &'a str) -> Option<&'a str> {
        let mut v = value;
        for _ in 0..=MAX_FALLBACK {
            if self.has(v) {
                return Some(v);
            }
            match self.fallback.get(v) {
                Some(Some(next)) => v = next,
                Some(None) => return None,
                None => break,
            }
        }
        match value.rsplit_once('+') {
            Some((_, last)) if last != value => self.resolve(last),
            _ => Some(&self.default),
        }
    }

    fn validate(&self) -> Result<(), String> {
        let name = &self.name;
        if !self.has(&self.default) {
            return Err(format!(
                "layer {name:?}: its default {:?} has nothing to show",
                self.default
            ));
        }
        // every chain ends within MAX_FALLBACK steps, at a value the layer has, at nothing, or at the default
        for start in self.fallback.keys() {
            let mut seen = BTreeSet::from([start.as_str()]);
            let mut v = start.as_str();
            let mut steps = 0;
            while let Some(Some(next)) = self.fallback.get(v) {
                steps += 1;
                if !seen.insert(next) {
                    return Err(format!(
                        "layer {name:?}: the fallbacks from {start:?} go round in a cycle"
                    ));
                }
                if steps > MAX_FALLBACK {
                    return Err(format!(
                        "layer {name:?}: the fallbacks from {start:?} take more than {MAX_FALLBACK} steps"
                    ));
                }
                v = next;
                if self.has(v) {
                    break;
                }
            }
        }
        for (value, a) in &self.animations {
            if a.frames.is_empty() {
                return Err(format!("layer {name:?}: animation {value:?} has no frames"));
            }
            for f in &a.frames {
                if !FRAME_MS.contains(&f.ms) {
                    return Err(format!(
                        "layer {name:?}: a frame of {value:?} lasts {} ms (1 to 60 000)",
                        f.ms
                    ));
                }
                if let Art::Raster { pivot, .. } = f.art {
                    coord(name, "a frame's pivot", pivot)?;
                }
            }
        }
        for p in &self.parts {
            placed(name, &p.id, p.origin)?;
            if let Some(shape) = &p.shape
                && !self.shapes.contains_key(shape)
            {
                return Err(format!("layer {name:?}: part {:?} draws unknown shape {shape:?}", p.id));
            }
        }
        let part = |id: &str| self.parts.iter().any(|p| p.id == id);
        for (value, targets) in &self.poses {
            for (id, &at) in targets {
                if !part(id) {
                    return Err(format!("layer {name:?}: pose {value:?} moves unknown part {id:?}"));
                }
                placed(name, id, at)?;
            }
        }
        if !TRANSITION_MS.contains(&self.transition_ms) {
            return Err(format!(
                "layer {name:?}: transition_ms is {} (0 to 10 000)",
                self.transition_ms
            ));
        }
        for (value, a) in &self.part_animations {
            if a.keys.is_empty() || a.keys.len() > MAX_KEYS {
                return Err(format!(
                    "layer {name:?}: animation {value:?} has {} keys (1 to {MAX_KEYS})",
                    a.keys.len()
                ));
            }
            for (id, phase) in &a.parts {
                if !part(id) {
                    return Err(format!("layer {name:?}: animation {value:?} moves unknown part {id:?}"));
                }
                if !(0.0..=1.0).contains(phase) {
                    return Err(format!("layer {name:?}: part {id:?}'s phase is {phase} (0 to 1)"));
                }
            }
            for k in &a.keys {
                if !KEY_MS.contains(&k.ms) {
                    return Err(format!(
                        "layer {name:?}: a key of {value:?} lasts {} ms (1 to 10 000)",
                        k.ms
                    ));
                }
                for n in [k.dx, k.dy, k.rotate, k.scale, k.opacity] {
                    if !n.is_finite() || n.abs() > COORD_LIMIT {
                        return Err(format!("layer {name:?}: a key of {value:?} has {n}"));
                    }
                }
                offset(name, value, (k.dx, k.dy))?;
            }
        }
        crate::vector::validate(self)?;
        if self.glow && self.channel == Channel::Fx {
            return Err(format!("layer {name:?}: an effects layer can't glow"));
        }
        if !self.look.is_empty() && self.channel != Channel::Eyes {
            return Err(format!("layer {name:?}: only the eyes channel looks around"));
        }
        for (look, &d) in &self.look {
            offset(name, look.key(), d)?;
        }
        Ok(())
    }
}

/// A finite number within [`COORD_LIMIT`].
fn coord(layer: &str, what: &str, (x, y): (f64, f64)) -> Result<(), String> {
    if x.is_finite() && y.is_finite() && x.abs() <= COORD_LIMIT && y.abs() <= COORD_LIMIT {
        Ok(())
    } else {
        Err(format!("layer {layer:?}: {what} is ({x}, {y})"))
    }
}

/// A position within the art box and its halo.
fn placed(layer: &str, id: &str, at: (f64, f64)) -> Result<(), String> {
    coord(layer, &format!("part {id:?}'s position"), at)?;
    let inside = |v: f64, max: f64| (-HALO_PAD..=max + HALO_PAD).contains(&v);
    if inside(at.0, ART_W) && inside(at.1, ART_H) {
        Ok(())
    } else {
        Err(format!(
            "layer {layer:?}: part {id:?} at ({}, {}) is outside the art box and its halo",
            at.0, at.1
        ))
    }
}

/// An offset that keeps a part within the art box and its halo from anywhere in it: at most the box's size.
fn offset(layer: &str, what: &str, d: (f64, f64)) -> Result<(), String> {
    coord(layer, &format!("{what}'s offset"), d)?;
    if d.0.abs() <= ART_W + 2.0 * HALO_PAD && d.1.abs() <= ART_H + 2.0 * HALO_PAD {
        Ok(())
    } else {
        Err(format!(
            "layer {layer:?}: {what}'s offset ({}, {}) is larger than the art box",
            d.0, d.1
        ))
    }
}

/// At most this many themes on a pet, its own and the user's.
pub const MAX_THEMES: usize = 64;

/// The name of a pack's default theme when it doesn't give one.
pub const DEFAULT_THEME: &str = "Default";

/// A pack's named colours (straight 0xAARRGGBB), which are its default theme, and its other themes. A vector pack's
/// shapes, accent and glow name them (`$robe`); a pixel pack's name exact colours in its sheets.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub colours: BTreeMap<String, u32>,
    /// The colour the accent and the glow name, if they name one.
    pub accent: Option<String>,
    pub glow: Option<String>,
    /// The default theme's name.
    pub theme: String,
    /// The other themes, in order: the pack's own, then the user's.
    pub themes: Vec<Theme>,
}

impl Default for Palette {
    fn default() -> Palette {
        Palette {
            colours: BTreeMap::new(),
            accent: None,
            glow: None,
            theme: DEFAULT_THEME.to_owned(),
            themes: Vec::new(),
        }
    }
}

/// A named colour theme: some of a palette's colours, changed.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: String,
    pub colours: BTreeMap<String, u32>,
    /// For one of the user's themes, its file's name in the themes folder.
    pub file: Option<String>,
}

impl Palette {
    /// Every theme's name, the default first.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.theme.as_str()).chain(self.themes.iter().map(|t| t.name.as_str()))
    }

    /// The palette's colours in the theme `name`: the default's, with the theme's changes. `None` for a theme it
    /// doesn't have.
    pub fn in_theme(&self, name: &str) -> Option<BTreeMap<String, u32>> {
        if name == self.theme {
            return Some(self.colours.clone());
        }
        let theme = self.themes.iter().find(|t| t.name == name)?;
        let mut colours = self.colours.clone();
        colours.extend(theme.colours.iter().map(|(n, &c)| (n.clone(), c)));
        Some(colours)
    }

    /// Whether `theme` can join the palette's themes: a name of its own, and only colours the palette names.
    pub fn check(&self, theme: &Theme) -> Result<(), String> {
        if !theme_name(&theme.name) {
            return Err("its name is empty, or starts or ends with a space".to_owned());
        }
        if self.colours.is_empty() {
            return Err("there are no named colours to change".to_owned());
        }
        if self.names().any(|n| n == theme.name) {
            return Err("there is already a theme by that name".to_owned());
        }
        if self.themes.len() + 1 >= MAX_THEMES {
            return Err(format!("there are too many themes (at most {MAX_THEMES})"));
        }
        for colour in theme.colours.keys() {
            if !self.colours.contains_key(colour) {
                let names: Vec<&str> = self.colours.keys().map(String::as_str).collect();
                return Err(format!(
                    "there is no colour named {colour:?} (the colours are {})",
                    names.join(", ")
                ));
            }
        }
        Ok(())
    }
}

/// Whether `name` will do as a theme's: not empty, and no space at either end.
pub fn theme_name(name: &str) -> bool {
    !name.trim().is_empty() && name.trim() == name
}

/// A pet.
#[derive(Clone, Debug, PartialEq)]
pub struct Pack {
    pub name: String,
    pub kind: Kind,
    /// The art's pixel size in logical px (a pixel pack's grid cell; 1 for painted and vector art).
    pub pixel_size: f64,
    /// The art pixel that sits on the ground shadow.
    pub anchor: (f64, f64),
    pub mirror: Mirror,
    /// 0xAARRGGBB.
    pub accent: u32,
    pub glow: u32,
    /// Drawn in this order.
    pub layers: Vec<Layer>,
    pub palette: Palette,
}

impl Pack {
    /// The same pet in other colours: each named colour in `colours` replaces the palette's, wherever the pack's
    /// shapes, accent and glow name it (a pixel pack's sheets change with [`crate::load::LoadedPack::recoloured`]).
    /// A name the palette lacks is the reason it can't.
    pub fn recolour(&mut self, colours: &BTreeMap<String, u32>) -> Result<(), String> {
        if self.palette.colours.is_empty() {
            return Err(format!("{} has no named colours to change", self.name));
        }
        for (name, &c) in colours {
            let Some(slot) = self.palette.colours.get_mut(name) else {
                let names: Vec<&str> = self.palette.colours.keys().map(String::as_str).collect();
                return Err(format!(
                    "{} has no colour named {name:?} (it has {})",
                    self.name,
                    names.join(", ")
                ));
            };
            *slot = c;
        }
        let pick = |name: &Option<String>| name.as_ref().and_then(|n| colours.get(n)).copied();
        if let Some(c) = pick(&self.palette.accent) {
            self.accent = c;
        }
        if let Some(c) = pick(&self.palette.glow) {
            self.glow = c;
        }
        for shape in self.layers.iter_mut().flat_map(|l| l.shapes.values_mut()).flatten() {
            if let Some(c) = pick(&shape.fill_name) {
                shape.fill = Some(c);
            }
            if let Some(c) = pick(&shape.stroke_name) {
                shape.stroke = Some(c);
            }
        }
        Ok(())
    }

    /// Checks the pack as the spec's limits say; the error is the reason it is skipped.
    pub fn validate(&self) -> Result<(), String> {
        if self.layers.is_empty() || self.layers.len() > MAX_LAYERS {
            return Err(format!("{} layers (1 to {MAX_LAYERS})", self.layers.len()));
        }
        if !(self.pixel_size.is_finite() && self.pixel_size > 0.0 && self.pixel_size <= ART_W) {
            return Err(format!("pixel_size is {}", self.pixel_size));
        }
        coord("-", "the anchor", self.anchor)?;
        let frames: usize = self
            .layers
            .iter()
            .flat_map(|l| l.animations.values())
            .map(|a| a.frames.len())
            .sum();
        if frames > MAX_FRAMES {
            return Err(format!("{frames} frames (at most {MAX_FRAMES})"));
        }
        let mut ids = BTreeSet::new();
        for layer in &self.layers {
            layer.validate()?;
            for p in &layer.parts {
                if !ids.insert(p.id.as_str()) {
                    return Err(format!("part id {:?} is used twice", p.id));
                }
            }
        }
        Ok(())
    }

    /// Every frame in the pack.
    pub fn frame_count(&self) -> usize {
        self.layers
            .iter()
            .flat_map(|l| l.animations.values())
            .map(|a| a.frames.len())
            .sum()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn frames(n: usize, ms: u32, mode: LoopMode) -> Animation {
        Animation {
            frames: (0..n)
                .map(|i| Frame {
                    art: Art::Raster {
                        sheet: 0,
                        x: i as u32 * 26,
                        y: 0,
                        w: 26,
                        h: 24,
                        pivot: (0.0, 0.0),
                    },
                    ms,
                })
                .collect(),
            mode,
            clock: crate::pack::Clock::Change,
        }
    }

    pub(crate) fn pack(layers: Vec<Layer>) -> Pack {
        Pack {
            name: "Test".to_owned(),
            kind: Kind::Pixel,
            pixel_size: 5.0,
            anchor: (13.0, 22.0),
            mirror: Mirror::Flip,
            accent: 0xFF5CFF9C,
            glow: 0xFF5CFF9C,
            layers,
            palette: Default::default(),
        }
    }

    fn body() -> Layer {
        let mut l = Layer::new("body", Channel::State, "idle");
        l.animations.insert("idle".into(), frames(2, 160, LoopMode::Loop));
        l
    }

    #[test]
    fn a_pack_with_a_default_to_show_validates() {
        assert_eq!(pack(vec![body()]).validate(), Ok(()));
    }

    #[test]
    fn a_missing_default_is_an_error() {
        let mut l = body();
        l.default = "sleep".into();
        assert!(pack(vec![l]).validate().unwrap_err().contains("default \"sleep\""));
    }

    #[test]
    fn an_unknown_loop_mode_is_an_error() {
        assert!(LoopMode::parse("bounce").unwrap_err().contains("bounce"));
        assert_eq!(LoopMode::parse("pingpong"), Ok(LoopMode::PingPong));
    }

    #[test]
    fn a_fallback_cycle_is_an_error() {
        let mut l = body();
        l.fallback.insert("done".into(), Some("listen".into()));
        l.fallback.insert("listen".into(), Some("done".into()));
        assert!(pack(vec![l]).validate().unwrap_err().contains("cycle"));
    }

    #[test]
    fn a_fallback_chain_longer_than_eight_steps_is_an_error() {
        let mut l = body();
        for i in 0..9 {
            l.fallback.insert(format!("v{i}"), Some(format!("v{}", i + 1)));
        }
        l.fallback.insert("v9".into(), Some("idle".into()));
        assert!(pack(vec![l.clone()]).validate().unwrap_err().contains("more than 8"));
        l.fallback.retain(|k, _| k.as_str() >= "v2");
        assert_eq!(pack(vec![l]).validate(), Ok(()));
    }

    #[test]
    fn durations_parts_and_numbers_out_of_range_are_errors() {
        let mut l = body();
        l.animations.get_mut("idle").unwrap().frames[0].ms = 0;
        assert!(pack(vec![l]).validate().unwrap_err().contains("0 ms"));

        let mut eyes = Layer::new("eyes", Channel::Eyes, "open");
        eyes.parts.push(Part {
            id: "eye-left".into(),
            origin: (55.5, 63.5),
            shape: None,
        });
        eyes.poses.insert("open".into(), BTreeMap::new());
        assert_eq!(pack(vec![body(), eyes.clone()]).validate(), Ok(()));

        let mut far = eyes.clone();
        far.parts[0].origin = (500.0, 63.5);
        assert!(
            pack(vec![body(), far])
                .validate()
                .unwrap_err()
                .contains("outside the art box")
        );

        let mut nan = eyes.clone();
        nan.parts[0].origin = (f64::NAN, 0.0);
        assert!(pack(vec![body(), nan]).validate().is_err());

        let mut stranger = eyes.clone();
        stranger
            .poses
            .insert("wide".into(), BTreeMap::from([("nose".to_owned(), (60.0, 60.0))]));
        assert!(
            pack(vec![body(), stranger])
                .validate()
                .unwrap_err()
                .contains("unknown part \"nose\"")
        );

        let mut twice = eyes.clone();
        twice.parts.push(twice.parts[0].clone());
        assert!(pack(vec![body(), twice]).validate().unwrap_err().contains("used twice"));

        let mut many = eyes.clone();
        many.part_animations.insert(
            "open".into(),
            PartAnimation {
                parts: vec![("eye-left".into(), 0.0)],
                mode: LoopMode::Loop,
                keys: vec![Key::offset(0.0, 0.0, 10); MAX_KEYS + 1],
            },
        );
        assert!(pack(vec![body(), many]).validate().unwrap_err().contains("65 keys"));

        let mut slow = eyes;
        slow.transition_ms = 10_001;
        assert!(
            pack(vec![body(), slow])
                .validate()
                .unwrap_err()
                .contains("transition_ms")
        );
    }

    #[test]
    fn a_value_resolves_through_its_fallbacks_to_the_default_or_to_nothing() {
        let mut l = body();
        l.animations.insert("sleep".into(), frames(1, 500, LoopMode::Once));
        l.fallback.insert("listen".into(), Some("sleep".into()));
        l.fallback.insert("off".into(), None);
        l.animations.insert("sparkles".into(), frames(2, 250, LoopMode::Loop));
        assert_eq!(
            l.resolve("zs+sparkles"),
            Some("sparkles"),
            "a combined value falls back to its last part"
        );
        assert_eq!(l.resolve("sleep"), Some("sleep"));
        assert_eq!(l.resolve("listen"), Some("sleep"));
        assert_eq!(l.resolve("thinking"), Some("idle"));
        assert_eq!(l.resolve("off"), None);
    }
}
