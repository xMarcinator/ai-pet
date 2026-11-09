//! A painted pack from a vector one (R11's painted Pip): each layer's every value drawn with tiny-skia at a high
//! resolution into PNG sheets, drawn smoothly on screen.
//!
//! Like the pixel bake ([`crate::bake`]), it runs ahead of time: `tests/bake.rs` checks the committed pack is what it
//! makes. A layer's parts become its frames: each value's pose, and its part animation as a frame per stretch of
//! time in which no part's key changes (a phase shifts a part's keys). The eyes' look table stays a table, moving
//! the whole frame. A pose's eased transition becomes a jump, the one thing a painted pack can't do. The vector
//! pack's colour themes become sheet themes: each layer's sheet that a theme changes, painted in its colours.

use std::collections::BTreeSet;

use crate::bake::{Baked, Sheet};
use crate::compose::{Placed, step};
use crate::pack::{Animation, Art, Clock, Frame, Kind, Layer, LoopMode, Pack, Theme, ThemeSheet};
use crate::pose::HALO_PAD;
use crate::vector::{self, View, whole};

/// A sheet is at most this wide: frames go on in rows.
const SHEET_W: u32 = 2048;

/// `vector` painted at `scale` art px per logical px, named `name`, with each of its colour themes as a sheet theme of
/// the same name: the sheets it changes, painted in its colours, as `<layer>-<theme>.png`.
pub fn paint(vector: &Pack, name: &str, scale: f64) -> Result<Baked, String> {
    if vector.kind != Kind::Vector {
        return Err(format!("{} isn't a vector pack", vector.name));
    }
    let mut baked = layers(vector, name, scale)?;
    baked.pack.palette.theme = vector.palette.theme.clone();
    for theme in &vector.palette.themes {
        let mut coloured = vector.clone();
        coloured.recolour(&vector.palette.in_theme(&theme.name).expect("one of its themes"))?;
        let other = layers(&coloured, name, scale)?;
        if other.pack.layers != baked.pack.layers {
            return Err(format!(
                "{}'s theme {:?} moves its frames, which a sheet theme can't",
                vector.name, theme.name
            ));
        }
        let file = theme.name.to_lowercase().replace(' ', "-");
        let mut sheets = Vec::new();
        for (index, sheet) in other.sheets.into_iter().enumerate() {
            if sheet != baked.sheets[index] {
                let layer = &baked.pack.layers[index].name;
                sheets.push(ThemeSheet {
                    sheet: format!("{layer}.png"),
                    index,
                    file: format!("{layer}-{file}.png"),
                    with: baked.sheets.len(),
                });
                baked.sheets.push(sheet);
            }
        }
        if !sheets.is_empty() {
            baked.pack.palette.themes.push(Theme {
                sheets,
                ..Theme::of_colours(&theme.name, Default::default())
            });
        }
    }
    baked.pack.validate()?;
    Ok(baked)
}

/// `vector`'s layers painted, a sheet each, in the colours it has.
fn layers(vector: &Pack, name: &str, scale: f64) -> Result<Baked, String> {
    let mut pack = Pack {
        name: name.to_owned(),
        kind: Kind::Painted,
        pixel_size: vector.pixel_size / scale,
        anchor: (vector.anchor.0 * scale, vector.anchor.1 * scale),
        layers: Vec::new(),
        // painted colours are in its sheets: nothing to recolour
        palette: Default::default(),
        // a style of the same pet, the painted one
        pet: Some(vector.pet().to_owned()),
        style: None,
        ..vector.clone()
    };
    let mut sheets = Vec::new();
    for (li, layer) in vector.layers.iter().enumerate() {
        let mut painted = Layer::new(&layer.name, layer.channel, &layer.default);
        painted.hit = layer.hit;
        painted.glow = layer.glow;
        painted.mirror = layer.mirror;
        painted.fallback = layer.fallback.clone();
        painted.look = layer.look.clone();
        let mut shelf = Shelf::default();
        for value in values(layer) {
            let mut frames = Vec::new();
            let (times, mode) = timeline(layer, &value)?;
            for (at, ms) in times {
                let (pixels, w, h, pivot) = frame(vector, li, &value, at, scale);
                let (x, y) = shelf.put(w, h, pixels);
                frames.push(Frame {
                    art: Art::Raster {
                        sheet: li,
                        x,
                        y,
                        w,
                        h,
                        pivot,
                    },
                    ms,
                });
            }
            painted.animations.insert(
                value,
                Animation {
                    frames,
                    mode,
                    clock: Clock::Change,
                },
            );
        }
        sheets.push(shelf.sheet());
        pack.layers.push(painted);
    }
    pack.validate()?;
    Ok(Baked { pack, sheets })
}

/// The values a layer shows for itself (not the names its parts' fixed shapes go by).
fn values(layer: &Layer) -> Vec<String> {
    let fixed: BTreeSet<&str> = layer.parts.iter().filter_map(|p| p.shape.as_deref()).collect();
    let mut out: BTreeSet<&str> = layer
        .shapes
        .keys()
        .map(String::as_str)
        .filter(|k| !fixed.contains(k))
        .chain(layer.poses.keys().map(String::as_str))
        .chain(layer.part_animations.keys().map(String::as_str))
        .collect();
    out.insert(&layer.default);
    out.into_iter()
        .filter(|v| layer.resolve(v) == Some(v))
        .map(str::to_owned)
        .collect()
}

/// When a value's frames start (ms into its animation) and how long each lasts: a frame wherever a part's key
/// changes. A value without a part animation is one frame.
fn timeline(layer: &Layer, value: &str) -> Result<(Vec<(f64, u32)>, LoopMode), String> {
    let Some(a) = layer.part_animations.get(value).filter(|a| a.keys.len() > 1) else {
        return Ok((vec![(0.0, 1000)], LoopMode::Loop));
    };
    if a.mode == LoopMode::PingPong {
        return Err(format!(
            "layer {:?}: {value:?} plays there and back, which isn't painted",
            layer.name
        ));
    }
    let total: u32 = a.keys.iter().map(|k| k.ms).sum();
    let mut starts: BTreeSet<u32> = BTreeSet::from([0]);
    for &(_, phase) in &a.parts {
        let shift = (phase * f64::from(total)).round() as u32;
        let mut at = 0;
        for k in &a.keys {
            starts.insert(match a.mode {
                LoopMode::Loop => (at + total - shift % total) % total,
                _ => at.saturating_sub(shift),
            });
            at += k.ms;
        }
    }
    let starts: Vec<u32> = starts.into_iter().collect();
    let times = starts
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let end = starts.get(i + 1).copied().unwrap_or(total);
            (f64::from(s), (end - s).max(1))
        })
        .collect();
    Ok((times, a.mode))
}

/// One frame of layer `li` showing `value`, `at` ms into its animation: its pixels (straight colours), size, and
/// the pixel that sits on the art box's top-left.
fn frame(pack: &Pack, li: usize, value: &str, at: f64, scale: f64) -> (Vec<u32>, u32, u32, (f64, f64)) {
    let layer = &pack.layers[li];
    let anim = layer.part_animations.get(value);
    let targets = layer.poses.get(value);
    let only = [whole()];
    let parts = if layer.parts.is_empty() {
        &only[..]
    } else {
        &layer.parts[..]
    };
    let mut items = Vec::new();
    for (pi, part) in parts.iter().enumerate() {
        let (mut x, mut y) = targets.and_then(|t| t.get(&part.id)).copied().unwrap_or(part.origin);
        let (mut rotate, mut scale_k, mut opacity) = (0.0, 1.0, 1.0);
        if let Some(a) = anim
            && let Some(&(_, phase)) = a.parts.iter().find(|(id, _)| *id == part.id)
        {
            let total: f64 = a.keys.iter().map(|k| f64::from(k.ms)).sum();
            // the middle of the frame's first ms, clear of rounding at its edges
            let elapsed = at + 0.5 + (phase * total).round();
            let k = a.keys[step(a.keys.len(), |i| a.keys[i].ms, a.mode, elapsed)];
            x += k.dx;
            y += k.dy;
            (rotate, scale_k, opacity) = (k.rotate, k.scale, k.opacity);
        }
        let placed = Placed {
            layer: li,
            part: pi,
            x,
            y,
            rotate,
            scale: scale_k,
            opacity,
            mirrored: false,
        };
        // a value that only moves parts draws its fallback's shapes at its pose
        let layer = &pack.layers[li];
        items.extend(vector::item(pack, &placed, layer.resolve_art(value), (1.0, 1.0)));
    }
    let view = View::of_box(scale);
    let rgba = vector::render(&items, view, |_| true);
    // cropped to what is drawn
    let (w, h) = (view.w as usize, view.h as usize);
    let alpha = |x: usize, y: usize| rgba[(y * w + x) * 4 + 3];
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if alpha(x, y) > 0 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    if x0 >= x1 {
        (x0, y0, x1, y1) = (0, 0, 1, 1);
    }
    let mut pixels = Vec::with_capacity((x1 - x0) * (y1 - y0));
    for y in y0..y1 {
        for x in x0..x1 {
            let p = &rgba[(y * w + x) * 4..][..4];
            let a = u32::from(p[3]);
            let un = |c: u8| (u32::from(c) * 255 + a / 2).checked_div(a).map_or(0, |v| v.min(255));
            pixels.push(if a == 0 {
                0
            } else {
                a << 24 | un(p[0]) << 16 | un(p[1]) << 8 | un(p[2])
            });
        }
    }
    let pad = (HALO_PAD * scale).round();
    let pivot = (pad - x0 as f64, pad - y0 as f64);
    (pixels, (x1 - x0) as u32, (y1 - y0) as u32, pivot)
}

/// Frames put on a sheet in rows, each row as tall as its tallest.
#[derive(Default)]
struct Shelf {
    frames: Vec<(u32, u32, u32, u32, Vec<u32>)>,
    x: u32,
    y: u32,
    row: u32,
}

impl Shelf {
    fn put(&mut self, w: u32, h: u32, pixels: Vec<u32>) -> (u32, u32) {
        if self.x + w > SHEET_W {
            self.y += self.row;
            (self.x, self.row) = (0, 0);
        }
        let at = (self.x, self.y);
        self.frames.push((at.0, at.1, w, h, pixels));
        self.x += w;
        self.row = self.row.max(h);
        at
    }

    fn sheet(self) -> Sheet {
        let width = self.frames.iter().map(|f| f.0 + f.2).max().unwrap_or(1);
        let height = self.frames.iter().map(|f| f.1 + f.3).max().unwrap_or(1);
        let mut pixels = vec![0; (width * height) as usize];
        for (x, y, w, h, px) in self.frames {
            for row in 0..h {
                let from = (row * w) as usize;
                let to = ((y + row) * width + x) as usize;
                pixels[to..to + w as usize].copy_from_slice(&px[from..from + w as usize]);
            }
        }
        Sheet { width, height, pixels }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::{Key, PartAnimation};
    use crate::pose::Channel;

    #[test]
    fn a_part_animation_is_a_frame_wherever_a_part_changes_key() {
        let mut layer = Layer::new("hands", Channel::Arms, "rest");
        layer.part_animations.insert(
            "rest".into(),
            PartAnimation {
                parts: vec![("a".into(), 0.0), ("b".into(), 0.25)],
                mode: LoopMode::Loop,
                keys: vec![Key::offset(0.0, 0.0, 100), Key::offset(0.0, 1.0, 300)],
            },
        );
        let (times, mode) = timeline(&layer, "rest").unwrap();
        assert_eq!(mode, LoopMode::Loop);
        // a changes at 0 and 100; b, a quarter (100 ms) on, at 300 and 0
        assert_eq!(times, [(0.0, 100), (100.0, 200), (300.0, 100)]);
        assert_eq!(timeline(&layer, "other").unwrap().0, [(0.0, 1000)]);
    }
}
