//! The compositor: from a [`Pack`] and a [`Pose`], what each layer shows and where each part is.
//!
//! [`pick`] chooses each layer's value (through its fallbacks), the animation's frame from the time since that
//! channel changed, and whether it is mirrored. [`Parts`] places the movable parts: the pose's target, eased from
//! wherever the part was when the value changed, plus the value's keyframes and (for the eyes) the look offset; then
//! the whole-body motion, then the mirroring. [`runs_argb`] and [`runs_rgba`] turn a frame's alpha into the row runs
//! that become the pet's hit rectangles.

use std::collections::HashMap;

use crate::pack::{Clock, Kind, Layer, LoopMode, Mirror, Pack};
use crate::pose::{ANCHOR, ART_H, ART_W, Channel, Facing, Look, Pose};

/// Which item of a sequence plays `elapsed` ms in, by `mode`: each lasts `ms(i)`, there are `n` (at least one).
pub(crate) fn step(n: usize, ms: impl Fn(usize) -> u32, mode: LoopMode, elapsed: f64) -> usize {
    // before the start, or a time that isn't one: the first
    if n <= 1 || elapsed.is_nan() || elapsed <= 0.0 {
        return 0;
    }
    // the order of play once round: forwards, or forwards then back without repeating the ends
    let order: Vec<usize> = match mode {
        LoopMode::PingPong => (0..n).chain((1..n - 1).rev()).collect(),
        LoopMode::Loop | LoopMode::Once => (0..n).collect(),
    };
    let total: f64 = order.iter().map(|&i| f64::from(ms(i))).sum();
    let mut at = match mode {
        LoopMode::Once if elapsed >= total => return n - 1,
        LoopMode::Once => elapsed,
        LoopMode::Loop | LoopMode::PingPong => elapsed % total,
    };
    for &i in &order {
        let d = f64::from(ms(i));
        if at < d {
            return i;
        }
        at -= d;
    }
    *order.last().unwrap_or(&0)
}

/// What a layer shows this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pick<'a> {
    /// The layer's index in the pack.
    pub layer: usize,
    /// The value it shows, after its fallbacks.
    pub value: &'a str,
    /// The frame of that value's animation, if it has frames (vector layers draw shapes).
    pub frame: Option<usize>,
    pub mirrored: bool,
}

/// Whether the pose's facing mirrors a layer: by its own policy, else the pack's.
pub fn mirrored(pack: &Pack, layer: &Layer, pose: &Pose) -> bool {
    pose.facing == Facing::Left && layer.mirror.unwrap_or(pack.mirror) == Mirror::Flip
}

/// How far a raster layer without parts is moved, in art-box px: the eyes channel's look offset, when it has a table.
pub fn raster_offset(pack: &Pack, layer: &Layer, pose: &Pose) -> (f64, f64) {
    if layer.channel == Channel::Eyes && layer.parts.is_empty() && !layer.look.is_empty() {
        look_offset(pack, layer, pose.eyes.look)
    } else {
        (0.0, 0.0)
    }
}

/// Each layer's value and frame at `t` (pet-clock seconds), in drawing order. A layer whose fallbacks say to draw
/// nothing for the pose's value is left out. Facing left, a layer's `<value>@left` frames, if it has them, are used
/// unmirrored.
pub fn pick<'a>(pack: &'a Pack, pose: &Pose, t: f64) -> Vec<Pick<'a>> {
    pack.layers
        .iter()
        .enumerate()
        .filter_map(|(i, layer)| {
            let mut value = layer.resolve(pose.key(layer.channel))?;
            let mut mirrored = mirrored(pack, layer, pose);
            // a value's own frames for facing left, where the layer has them, instead of mirroring
            if pose.facing == Facing::Left
                && let Some((key, _)) = layer.animations.get_key_value(&format!("{value}@left") as &str)
            {
                value = key;
                mirrored = false;
            }
            let frame = layer.animations.get(value).map(|a| {
                let from = match a.clock {
                    Clock::Change => pose.since(layer.channel),
                    Clock::Pet => 0.0,
                };
                // reduced motion holds the first frame
                if pose.still {
                    0
                } else {
                    step(a.frames.len(), |f| a.frames[f].ms, a.mode, (t - from) * 1000.0)
                }
            });
            Some(Pick {
                layer: i,
                value,
                frame,
                mirrored,
            })
        })
        .collect()
}

/// Where a part is this frame, in art-box px, after the body's motion and the mirroring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub layer: usize,
    /// The part's index in its layer.
    pub part: usize,
    pub x: f64,
    pub y: f64,
    /// Degrees about the part's origin.
    pub rotate: f64,
    pub scale: f64,
    pub opacity: f64,
    pub mirrored: bool,
}

/// A part on its way from one pose's position to another's.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Move {
    from: (f64, f64),
    to: (f64, f64),
    start: f64,
    ms: u32,
}

impl Move {
    /// Where it is at `t`, eased in and out.
    fn at(&self, t: f64) -> (f64, f64) {
        let u = if self.ms == 0 {
            1.0
        } else {
            ((t - self.start) * 1000.0 / f64::from(self.ms)).clamp(0.0, 1.0)
        };
        let e = if u < 0.5 {
            4.0 * u * u * u
        } else {
            1.0 - (-2.0 * u + 2.0).powi(3) / 2.0
        };
        (
            self.from.0 + (self.to.0 - self.from.0) * e,
            self.from.1 + (self.to.1 - self.from.1) * e,
        )
    }
}

/// The eyes' offset for `look` from a layer's table: the direction itself, else the nearest given within 45°, else
/// the centre's, else none. Raster packs move by whole art pixels.
fn look_offset(pack: &Pack, layer: &Layer, look: Look) -> (f64, f64) {
    let table = &layer.look;
    let found = table.get(&look).copied().or_else(|| {
        if look == Look::CENTER {
            return None;
        }
        let dir = |l: Look| f64::from(l.dy).atan2(f64::from(l.dx));
        Look::ALL
            .into_iter()
            .filter(|l| *l != Look::CENTER && table.contains_key(l))
            .map(|l| {
                let d = (dir(l) - dir(look)).abs();
                (d.min(2.0 * std::f64::consts::PI - d), l)
            })
            .filter(|(d, _)| *d <= std::f64::consts::FRAC_PI_4 + 1e-9)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, l)| table[&l])
    });
    let (dx, dy) = found
        .or_else(|| table.get(&Look::CENTER).copied())
        .unwrap_or((0.0, 0.0));
    match pack.kind {
        Kind::Pixel | Kind::Painted => {
            let px = pack.pixel_size;
            ((dx / px).round() * px, (dy / px).round() * px)
        }
        Kind::Vector => (dx, dy),
    }
}

/// The parts' motion between frames: where each part was going when its layer's value last changed. One per pet
/// and pack.
#[derive(Clone, Debug, Default)]
pub struct Parts {
    moves: HashMap<(usize, usize), Move>,
}

impl Parts {
    /// Whether a part is still on its way to a pose's position at `t`.
    pub fn moving(&self, t: f64) -> bool {
        self.moves
            .values()
            .any(|m| m.from != m.to && (t - m.start) * 1000.0 < f64::from(m.ms))
    }

    /// Every part's place at `t`, layer by layer. A layer that draws nothing for the pose's value places none.
    pub fn place(&mut self, pack: &Pack, pose: &Pose, t: f64) -> Vec<Placed> {
        let m = pose.motion;
        let mut placed = Vec::new();
        for (li, layer) in pack.layers.iter().enumerate() {
            let mirrored = mirrored(pack, layer, pose);
            let Some(value) = layer.resolve(pose.key(layer.channel)) else {
                continue;
            };
            let targets = layer.poses.get(value);
            let anim = layer.part_animations.get(value);
            let look = if layer.channel == Channel::Eyes && !layer.look.is_empty() {
                look_offset(pack, layer, pose.eyes.look)
            } else {
                (0.0, 0.0)
            };
            // a vector layer without parts draws its shapes in art-box coordinates, as one part at (0, 0)
            let whole = [crate::vector::whole()];
            let parts = if layer.parts.is_empty() && pack.kind == Kind::Vector {
                &whole[..]
            } else {
                &layer.parts[..]
            };
            for (pi, part) in parts.iter().enumerate() {
                let target = targets.and_then(|t| t.get(&part.id)).copied().unwrap_or(part.origin);
                let mv = self.moves.entry((li, pi)).or_insert(Move {
                    from: target,
                    to: target,
                    start: t,
                    ms: 0,
                });
                if mv.to != target {
                    *mv = Move {
                        from: mv.at(t),
                        to: target,
                        start: t,
                        ms: layer.transition_ms,
                    };
                }
                let (mut x, mut y) = mv.at(t);
                let (mut rotate, mut scale, mut opacity) = (0.0, 1.0, 1.0);
                if let Some(a) = anim
                    && let Some(&(_, phase)) = a.parts.iter().find(|(id, _)| *id == part.id)
                {
                    let total: f64 = a.keys.iter().map(|k| f64::from(k.ms)).sum();
                    let elapsed = (t - pose.since(layer.channel)) * 1000.0 + phase * total;
                    let k = a.keys[if pose.still {
                        0
                    } else {
                        step(a.keys.len(), |i| a.keys[i].ms, a.mode, elapsed)
                    }];
                    x += k.dx;
                    y += k.dy;
                    (rotate, scale, opacity) = (k.rotate, k.scale, k.opacity);
                }
                x += look.0;
                y += look.1;
                // the body's motion: squash about the anchor, then the offset
                x = ANCHOR.0 + (x - ANCHOR.0) * m.scale_x + m.x;
                y = ANCHOR.1 + (y - ANCHOR.1) * m.scale_y + m.y;
                if mirrored {
                    x = ART_W - x;
                    rotate = -rotate;
                }
                placed.push(Placed {
                    layer: li,
                    part: pi,
                    x,
                    y,
                    rotate,
                    scale,
                    opacity,
                    mirrored,
                });
            }
        }
        placed
    }
}

/// A raster pack's frames for `pose` at `t`, laid over each other in its art box at the pack's own resolution:
/// (width, height, premultiplied 0xAARRGGBB pixels). For a still of a painted pack.
pub fn flatten(pack: &Pack, sheets: &[crate::bake::Sheet], pose: &Pose, t: f64) -> (u32, u32, Vec<u32>) {
    let (w, h) = (
        (ART_W / pack.pixel_size).round() as i64,
        (ART_H / pack.pixel_size).round() as i64,
    );
    let mut out = vec![0u32; (w * h) as usize];
    for p in pick(pack, pose, t) {
        let layer = &pack.layers[p.layer];
        let Some(crate::pack::Art::Raster {
            sheet,
            x,
            y,
            w: fw,
            h: fh,
            pivot,
        }) = p.frame.map(|f| layer.animations[p.value].frames[f].art.clone())
        else {
            continue;
        };
        let (dx, dy) = raster_offset(pack, layer, pose);
        let ox = (dx / pack.pixel_size).round() as i64 - pivot.0 as i64;
        let oy = (dy / pack.pixel_size).round() as i64 - pivot.1 as i64;
        let s = &sheets[sheet];
        for fy in 0..i64::from(fh) {
            for fx in 0..i64::from(fw) {
                let src = s.pixels[((i64::from(y) + fy) * i64::from(s.width) + i64::from(x) + fx) as usize];
                let cx = if p.mirrored { w - 1 - (fx + ox) } else { fx + ox };
                let cy = fy + oy;
                if src >> 24 == 0 || !(0..w).contains(&cx) || !(0..h).contains(&cy) {
                    continue;
                }
                let dst = &mut out[(cy * w + cx) as usize];
                let top = crate::pm(src);
                let a = top >> 24;
                // premultiplied source-over, channel by channel
                let mut px = 0;
                for shift in [0, 8, 16, 24] {
                    let c = ((top >> shift) & 0xFF) + ((*dst >> shift) & 0xFF) * (255 - a) / 255;
                    px |= c.min(255) << shift;
                }
                *dst = px;
            }
        }
    }
    (w as u32, h as u32, out)
}

/// The rows of a frame's pixels with any alpha, as (x, y, length), from premultiplied `0xAARRGGBB` pixels in rows of
/// `width`.
pub fn runs_argb(pixels: &[u32], width: usize) -> Vec<(i32, i32, i32)> {
    runs(pixels.len(), width, |i| pixels[i] >> 24 != 0)
}

/// The same from RGBA bytes (a decoded sheet's frame), four per pixel, in rows of `width` pixels.
pub fn runs_rgba(bytes: &[u8], width: usize) -> Vec<(i32, i32, i32)> {
    runs(bytes.len() / 4, width, |i| bytes[i * 4 + 3] != 0)
}

fn runs(pixels: usize, width: usize, solid: impl Fn(usize) -> bool) -> Vec<(i32, i32, i32)> {
    let mut runs = Vec::new();
    if width == 0 {
        return runs;
    }
    for y in 0..pixels / width {
        let row = y * width;
        let mut x = 0;
        while x < width {
            if !solid(row + x) {
                x += 1;
                continue;
            }
            let start = x;
            while x < width && solid(row + x) {
                x += 1;
            }
            runs.push((start as i32, y as i32, (x - start) as i32));
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::pack::tests::{frames, pack};
    use crate::pack::{Key, Layer, PartAnimation};
    use crate::pose::{Arms, Expression, Eyes, Mood, StateValue, Values};

    fn at(mode: LoopMode, ms: &[u32], elapsed: f64) -> usize {
        step(ms.len(), |i| ms[i], mode, elapsed)
    }

    #[test]
    fn a_loop_plays_its_frames_by_their_durations_and_starts_over() {
        let ms = [100, 50, 200];
        let got: Vec<usize> = [0.0, 99.0, 100.0, 149.0, 150.0, 349.0, 350.0, 450.0]
            .into_iter()
            .map(|e| at(LoopMode::Loop, &ms, e))
            .collect();
        assert_eq!(got, [0, 0, 1, 1, 2, 2, 0, 1]);
    }

    #[test]
    fn once_holds_its_last_frame_and_pingpong_comes_back_without_repeating_the_ends() {
        let ms = [100, 100, 100];
        assert_eq!(at(LoopMode::Once, &ms, 250.0), 2);
        assert_eq!(at(LoopMode::Once, &ms, 10_000.0), 2);
        let got: Vec<usize> = (0..9)
            .map(|i| at(LoopMode::PingPong, &ms, i as f64 * 100.0 + 1.0))
            .collect();
        assert_eq!(got, [0, 1, 2, 1, 0, 1, 2, 1, 0]);
        assert_eq!(at(LoopMode::PingPong, &[100], 5_000.0), 0);
    }

    fn idle_pack() -> Pack {
        let mut body = Layer::new("body", Channel::State, "idle");
        body.animations.insert("idle".into(), frames(2, 160, LoopMode::Loop));
        body.animations.insert("sleep".into(), frames(1, 1000, LoopMode::Once));
        body.fallback.insert("listen".into(), Some("sleep".into()));
        let mut hp = Layer::new("headphones", Channel::Headphones, "on");
        hp.animations.insert("on".into(), frames(1, 1000, LoopMode::Once));
        hp.fallback.insert("off".into(), None);
        pack(vec![body, hp])
    }

    #[test]
    fn a_layer_picks_its_value_through_the_fallbacks_and_its_frame_from_the_channels_time() {
        let p = idle_pack();
        let pose = Pose::scripted(Values::IDLE, 10.0);
        let picks = pick(&p, &pose, 10.2);
        // the headphones layer draws nothing when they're off
        assert_eq!(
            picks,
            [Pick {
                layer: 0,
                value: "idle",
                frame: Some(1),
                mirrored: false
            }]
        );
        let listening = Values {
            state: StateValue {
                mood: Mood::Listen,
                act: None,
            },
            headphones: true,
            facing: Facing::Left,
            ..Values::IDLE
        };
        let picks = pick(&p, &Pose::scripted(listening, 10.0), 10.2);
        assert_eq!(picks.len(), 2);
        assert_eq!(
            (picks[0].value, picks[0].frame, picks[0].mirrored),
            ("sleep", Some(0), true)
        );
        assert_eq!(picks[1].value, "on");
        // an unknown value goes to the default; an idle act idle-hop, which this pack lacks, too
        let hop = Values {
            state: StateValue {
                mood: Mood::Idle,
                act: Some(crate::pose::IdleAct::Hop),
            },
            ..Values::IDLE
        };
        assert_eq!(pick(&p, &Pose::scripted(hop, 0.0), 0.0)[0].value, "idle");
    }

    #[test]
    fn a_pack_that_never_mirrors_stays_unmirrored_facing_left() {
        let mut p = idle_pack();
        p.mirror = Mirror::None;
        let left = Values {
            facing: Facing::Left,
            ..Values::IDLE
        };
        assert!(!pick(&p, &Pose::scripted(left, 0.0), 0.0)[0].mirrored);
    }

    fn arms_pack() -> Pack {
        let mut arms = Layer::new("arms", Channel::Arms, "rest");
        arms.parts = vec![
            crate::pack::Part {
                id: "hand-left".into(),
                origin: (35.0, 101.0),
                shape: None,
            },
            crate::pack::Part {
                id: "hand-right".into(),
                origin: (95.0, 101.0),
                shape: None,
            },
        ];
        arms.poses.insert("rest".into(), BTreeMap::new());
        arms.poses.insert(
            "wave-a".into(),
            BTreeMap::from([("hand-right".to_owned(), (116.0, 71.0))]),
        );
        arms.transition_ms = 200;
        arms.part_animations.insert(
            "hold".into(),
            PartAnimation {
                parts: vec![("hand-left".into(), 0.0), ("hand-right".into(), 0.5)],
                mode: LoopMode::Loop,
                keys: vec![Key::offset(0.0, 0.0, 100), Key::offset(0.0, 2.0, 100)],
            },
        );
        pack(vec![arms])
    }

    fn arms(a: Arms, since: f64) -> Pose {
        Pose::scripted(
            Values {
                arms: a,
                ..Values::IDLE
            },
            since,
        )
    }

    #[test]
    fn a_pose_change_eases_the_part_over_the_transition_and_a_second_change_starts_where_it_is() {
        let p = arms_pack();
        let mut parts = Parts::default();
        let right = |placed: Vec<Placed>| (placed[1].x, placed[1].y);
        let near = |(x, y): (f64, f64), (ex, ey): (f64, f64)| (x - ex).abs() < 1e-9 && (y - ey).abs() < 1e-9;
        assert_eq!(right(parts.place(&p, &arms(Arms::Rest, 0.0), 0.0)), (95.0, 101.0));
        // waving: half-way through the 200 ms transition it is half-way (the easing is symmetric)
        let wave = arms(Arms::WaveA, 1.0);
        assert_eq!(right(parts.place(&p, &wave, 1.0)), (95.0, 101.0));
        assert!(near(right(parts.place(&p, &wave, 1.1)), (105.5, 86.0)));
        // back to rest mid-way: from where it is, not from the wave's target
        let rest = arms(Arms::Rest, 1.1);
        let mid = right(parts.place(&p, &rest, 1.1));
        assert!(near(mid, (105.5, 86.0)), "{mid:?}");
        let (x, y) = right(parts.place(&p, &rest, 1.15));
        assert!(x < 105.5 && x > 95.0 && y > 86.0 && y < 101.0, "({x}, {y})");
        assert_eq!(right(parts.place(&p, &rest, 1.3)), (95.0, 101.0));
        // the other hand never moved
        assert_eq!(parts.place(&p, &rest, 1.3)[0].x, 35.0);
    }

    #[test]
    fn a_looping_keyframe_animation_plays_each_part_at_its_phase() {
        let p = arms_pack();
        let mut parts = Parts::default();
        let pose = arms(Arms::Hold, 5.0);
        let ys = |parts: &mut Parts, t: f64| {
            let placed = parts.place(&p, &pose, t);
            (placed[0].y, placed[1].y)
        };
        // the left hand starts on the first key, the right half a loop on, on the second
        assert_eq!(ys(&mut parts, 5.05), (101.0, 103.0));
        assert_eq!(ys(&mut parts, 5.15), (103.0, 101.0));
        assert_eq!(ys(&mut parts, 5.25), (101.0, 103.0));
    }

    fn eyes_pack(kind: Kind) -> Pack {
        let mut eyes = Layer::new("eyes", Channel::Eyes, "open");
        eyes.parts = vec![crate::pack::Part {
            id: "eye-left".into(),
            origin: (55.5, 63.5),
            shape: None,
        }];
        eyes.poses.insert("open".into(), BTreeMap::new());
        eyes.look = BTreeMap::from([
            (Look::CENTER, (0.0, 0.0)),
            (Look { dx: 0, dy: -1 }, (0.0, -2.5)),
            (Look { dx: 1, dy: 0 }, (2.5, 0.0)),
        ]);
        let mut p = pack(vec![eyes]);
        p.kind = kind;
        p
    }

    fn looking(look: Look) -> Pose {
        Pose::scripted(
            Values {
                eyes: Eyes {
                    expression: Expression::Open,
                    look,
                },
                ..Values::IDLE
            },
            0.0,
        )
    }

    #[test]
    fn the_look_offset_takes_the_direction_or_the_nearest_given_then_the_centre() {
        let p = eyes_pack(Kind::Vector);
        let mut parts = Parts::default();
        let pos = |parts: &mut Parts, look| {
            let placed = parts.place(&p, &looking(look), 0.0);
            (placed[0].x, placed[0].y)
        };
        assert_eq!(pos(&mut parts, Look { dx: 0, dy: -1 }), (55.5, 61.0));
        assert_eq!(pos(&mut parts, Look { dx: 1, dy: 0 }), (58.0, 63.5));
        // up-right isn't given: up and right are both 45° away, and up comes first
        assert_eq!(pos(&mut parts, Look { dx: 1, dy: -1 }), (55.5, 61.0));
        // down isn't given, and nothing is within 45°: the centre
        assert_eq!(pos(&mut parts, Look { dx: 0, dy: 1 }), (55.5, 63.5));
        // a pixel pack moves by whole art pixels: 2.5 px is half a 5 px pixel, which rounds away from zero
        let px = eyes_pack(Kind::Pixel);
        let placed = Parts::default().place(&px, &looking(Look { dx: 0, dy: -1 }), 0.0);
        assert_eq!(placed[0].y, 63.5 - 5.0);
    }

    #[test]
    fn parts_ride_with_the_body_and_mirror_last() {
        let p = eyes_pack(Kind::Vector);
        let mut pose = looking(Look { dx: 1, dy: 0 });
        pose.motion.x = 3.0;
        pose.motion.y = -4.0;
        pose.motion.scale_x = 1.1;
        let placed = Parts::default().place(&p, &pose, 0.0);
        let x = ANCHOR.0 + (58.0 - ANCHOR.0) * 1.1 + 3.0;
        assert_eq!((placed[0].x, placed[0].y), (x, 63.5 - 4.0));
        let mut left = pose;
        left.facing = Facing::Left;
        let placed = Parts::default().place(&p, &left, 0.0);
        assert_eq!(placed[0].x, ART_W - x);
        assert!(placed[0].mirrored);
    }

    #[test]
    fn row_runs_come_from_the_alpha() {
        let t = 0xFF00_0000u32;
        let px = [0, t, t, 0, t, 0, 0, 0, 0x0100_0000, t, 0, 0];
        assert_eq!(runs_argb(&px, 4), [(1, 0, 2), (0, 1, 1), (0, 2, 2)]);
        let rgba: Vec<u8> = px.iter().flat_map(|&p| [0, 0, 0, (p >> 24) as u8]).collect();
        assert_eq!(runs_rgba(&rgba, 4), runs_argb(&px, 4));
        assert!(runs_argb(&[0; 8], 4).is_empty());
    }
}
