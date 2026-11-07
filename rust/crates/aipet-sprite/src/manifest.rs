//! A pack's manifest (`pack.json`) and its PNG sheets: read into the in-memory [`Pack`], and written by the bake.
//!
//! The reader takes the whole contract: kinds; layers bound to channels, with hit, glow and mirror; raster frames
//! `[x, y, w, h, duration_ms]` with an optional `pivot`; loop modes and the pet clock; fallbacks (a `null` one draws
//! nothing); look tables; and part motion (R18): parts with origins, shapes, poses with `transition_ms`, and keyframe
//! part animations with `phase`. It asks for each sheet by name ([`read`]), so a loader from disk can check the path
//! and the size first. Unknown keys are ignored.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::bake::Sheet;
use crate::pack::{Animation, Art, Clock, Frame, Key, Kind, Layer, LoopMode, Mirror, Pack, Part, PartAnimation, Shape};
use crate::pose::{Channel, Look};
use crate::vector::{self, Cap, Join};

/// The newest `format_version` this build reads.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct PackJson {
    format_version: u32,
    name: String,
    kind: String,
    pixel_size: f64,
    anchor: [f64; 2],
    #[serde(default = "flip")]
    mirror: String,
    accent: String,
    glow: String,
    layers: Vec<LayerJson>,
}

fn flip() -> String {
    "flip".to_owned()
}

#[derive(Debug, Serialize, Deserialize)]
struct LayerJson {
    name: String,
    channel: String,
    #[serde(default)]
    hit: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    glow: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mirror: Option<String>,
    default: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    fallback: BTreeMap<String, Option<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    look: BTreeMap<String, [f64; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    parts: Vec<PartJson>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    shapes: BTreeMap<String, Drawing>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    poses: BTreeMap<String, BTreeMap<String, [f64; 2]>>,
    #[serde(default, skip_serializing_if = "is_zero")]
    transition_ms: u32,
    /// Frame animations ([`AnimationJson`]) and part animations (with `keys`), by value.
    #[serde(default)]
    animations: BTreeMap<String, Value>,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

#[derive(Debug, Serialize, Deserialize)]
struct PartJson {
    id: String,
    origin: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shape: Option<String>,
}

/// A value's drawing: one shape, or several bottom to top.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Drawing {
    One(ShapeJson),
    Many(Vec<ShapeJson>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShapeJson {
    d: String,
    #[serde(default = "no_paint")]
    fill: String,
    #[serde(default = "no_paint")]
    stroke: String,
    #[serde(default = "one")]
    stroke_width: f64,
    #[serde(default = "butt")]
    linecap: String,
    #[serde(default = "miter")]
    linejoin: String,
    #[serde(default = "one")]
    opacity: f64,
}

fn no_paint() -> String {
    "none".into()
}
fn one() -> f64 {
    1.0
}
fn butt() -> String {
    "butt".into()
}
fn miter() -> String {
    "miter".into()
}

#[derive(Debug, Serialize, Deserialize)]
struct AnimationJson {
    sheet: String,
    /// `[x, y, w, h, ms]` each, or with a `grid`, cell indices.
    frames: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    grid: Option<GridJson>,
    /// With a grid: one duration for every frame, or one each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ms: Option<Value>,
    #[serde(rename = "loop", default = "loop_mode")]
    mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clock: Option<String>,
    /// The frame pixel that sits on the part's origin (or the art box's top-left); one for all the frames, or one each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pivot: Option<Value>,
}

/// A sheet laid out as a grid of equal cells.
#[derive(Debug, Serialize, Deserialize)]
struct GridJson {
    frame: [i64; 2],
    #[serde(default)]
    origin: [i64; 2],
    #[serde(default)]
    gap: [i64; 2],
    columns: i64,
}

fn loop_mode() -> String {
    "loop".to_owned()
}

#[derive(Debug, Deserialize)]
struct PartAnimationJson {
    #[serde(default)]
    part: Option<String>,
    #[serde(default)]
    parts: Vec<String>,
    /// One for all the parts, or one each.
    #[serde(default)]
    phase: Option<Value>,
    #[serde(rename = "loop", default = "loop_mode")]
    mode: String,
    keys: Vec<Value>,
}

fn colour(c: u32) -> String {
    if c >> 24 == 0xFF {
        format!("#{:06X}", c & 0x00FF_FFFF)
    } else {
        format!("#{c:08X}")
    }
}

pub(crate) fn parse_colour(s: &str) -> Result<u32, String> {
    let hex = s
        .strip_prefix('#')
        .ok_or_else(|| format!("colour {s:?} doesn't start with #"))?;
    let v = u32::from_str_radix(hex, 16).map_err(|_| format!("colour {s:?} isn't hex"))?;
    match hex.len() {
        6 => Ok(0xFF00_0000 | v),
        8 => Ok(v),
        _ => Err(format!("colour {s:?} isn't #RRGGBB or #AARRGGBB")),
    }
}

/// The sheet file a layer's frames are on.
fn sheet_name(layer: &str) -> String {
    format!("{layer}.png")
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Pixel => "pixel",
        Kind::Painted => "painted",
        Kind::Vector => "vector",
    }
}

fn mode_name(m: LoopMode) -> &'static str {
    match m {
        LoopMode::Loop => "loop",
        LoopMode::Once => "once",
        LoopMode::PingPong => "pingpong",
    }
}

/// The manifest for a baked `pack`, whose frames are by index the layers' own sheets (`<layer>.png`).
pub fn write(pack: &Pack) -> String {
    let json = PackJson {
        format_version: FORMAT_VERSION,
        name: pack.name.clone(),
        kind: kind_name(pack.kind).to_owned(),
        pixel_size: pack.pixel_size,
        anchor: [pack.anchor.0, pack.anchor.1],
        mirror: mirror_name(pack.mirror).to_owned(),
        accent: colour(pack.accent),
        glow: colour(pack.glow),
        layers: pack
            .layers
            .iter()
            .map(|l| LayerJson {
                name: l.name.clone(),
                channel: l.channel.name().to_owned(),
                hit: l.hit,
                glow: l.glow,
                mirror: l.mirror.map(|m| mirror_name(m).to_owned()),
                default: l.default.clone(),
                fallback: l.fallback.clone(),
                look: l.look.iter().map(|(k, v)| (k.key().to_owned(), [v.0, v.1])).collect(),
                parts: Vec::new(),
                shapes: BTreeMap::new(),
                poses: BTreeMap::new(),
                transition_ms: 0,
                animations: l
                    .animations
                    .iter()
                    .map(|(value, a)| {
                        let frames: Vec<[u32; 5]> = a
                            .frames
                            .iter()
                            .map(|f| match f.art {
                                Art::Raster { x, y, w, h, .. } => [x, y, w, h, f.ms],
                                Art::Vector => [0, 0, 0, 0, f.ms],
                            })
                            .collect();
                        let json = AnimationJson {
                            sheet: sheet_name(&l.name),
                            frames: serde_json::to_value(frames).expect("numbers always serialise"),
                            grid: None,
                            ms: None,
                            mode: mode_name(a.mode).to_owned(),
                            clock: (a.clock == Clock::Pet).then(|| "pet".to_owned()),
                            pivot: pivots(&a.frames),
                        };
                        (
                            value.clone(),
                            serde_json::to_value(json).expect("an animation always serialises"),
                        )
                    })
                    .collect(),
            })
            .collect(),
    };
    let mut text = serde_json::to_string_pretty(&json).expect("a pack's manifest always serialises");
    text.push('\n');
    text
}

/// The frames' pivots as a manifest writes them: none when every frame is placed by its top-left corner.
fn pivots(frames: &[Frame]) -> Option<Value> {
    let each: Vec<[f64; 2]> = frames
        .iter()
        .map(|f| match f.art {
            Art::Raster { pivot, .. } => [pivot.0, pivot.1],
            Art::Vector => [0.0, 0.0],
        })
        .collect();
    each.iter()
        .any(|p| *p != [0.0, 0.0])
        .then(|| serde_json::to_value(each).expect("numbers always serialise"))
}

fn mirror_name(m: Mirror) -> &'static str {
    match m {
        Mirror::Flip => "flip",
        Mirror::None => "none",
    }
}

fn parse_mirror(s: &str) -> Result<Mirror, String> {
    match s {
        "flip" => Ok(Mirror::Flip),
        "none" => Ok(Mirror::None),
        other => Err(format!("unknown mirror {other:?} (flip or none)")),
    }
}

/// A number from JSON, or a reason.
fn number(v: &Value, what: &str) -> Result<f64, String> {
    v.as_f64().ok_or_else(|| format!("{what} isn't a number"))
}

/// A whole number of ms from JSON, saturating.
fn whole_ms(v: &Value, what: &str) -> Result<u32, String> {
    v.as_u64()
        .map(|ms| ms.min(u64::from(u32::MAX)) as u32)
        .ok_or_else(|| format!("{what} isn't a whole number of ms"))
}

/// One key of a part animation: `[dx, dy, ms]`, or an object with optional `dx`, `dy`, `rotate`, `scale`,
/// `opacity` and `ms`.
fn key(v: &Value) -> Result<Key, String> {
    if let Some(a) = v.as_array() {
        let [dx, dy, ms] = a.as_slice() else {
            return Err("a key isn't [dx, dy, ms]".to_owned());
        };
        return Ok(Key::offset(
            number(dx, "a key's dx")?,
            number(dy, "a key's dy")?,
            whole_ms(ms, "a key's ms")?,
        ));
    }
    let o = v.as_object().ok_or("a key isn't [dx, dy, ms] or an object")?;
    let get = |k: &str, default: f64| o.get(k).map_or(Ok(default), |v| number(v, &format!("a key's {k}")));
    Ok(Key {
        dx: get("dx", 0.0)?,
        dy: get("dy", 0.0)?,
        rotate: get("rotate", 0.0)?,
        scale: get("scale", 1.0)?,
        opacity: get("opacity", 1.0)?,
        ms: whole_ms(o.get("ms").ok_or("a key has no ms")?, "a key's ms")?,
    })
}

/// A pair `[x, y]` from JSON.
fn pair(v: &Value, what: &str) -> Result<(f64, f64), String> {
    match v.as_array().map(Vec::as_slice) {
        Some([x, y]) => Ok((number(x, what)?, number(y, what)?)),
        _ => Err(format!("{what} isn't [x, y]")),
    }
}

/// The pack a manifest describes, validated, and its sheets by index as its frames name them. `sheet` gives a file's
/// decoded sheet, or the reason it can't.
pub fn read(text: &str, mut sheet: impl FnMut(&str) -> Result<Sheet, String>) -> Result<(Pack, Vec<Sheet>), String> {
    let json: PackJson = serde_json::from_str(text).map_err(|e| format!("the manifest isn't valid: {e}"))?;
    if json.format_version > FORMAT_VERSION {
        return Err(format!(
            "format_version {} is newer than this AiPet reads ({FORMAT_VERSION})",
            json.format_version
        ));
    }
    let kind = Kind::parse(&json.kind).ok_or_else(|| format!("unknown kind {:?}", json.kind))?;
    let mut names: Vec<String> = Vec::new();
    let mut sheets: Vec<Sheet> = Vec::new();
    let mut layers = Vec::new();
    for l in json.layers {
        let name = l.name.clone();
        let channel =
            Channel::parse(&l.channel).ok_or_else(|| format!("layer {name:?}: unknown channel {:?}", l.channel))?;
        let mut layer = Layer::new(&name, channel, &l.default);
        layer.hit = l.hit;
        layer.glow = l.glow;
        layer.mirror = l.mirror.as_deref().map(parse_mirror).transpose()?;
        layer.fallback = l.fallback;
        layer.transition_ms = l.transition_ms;
        for (k, [dx, dy]) in l.look {
            let look = Look::parse(&k).ok_or_else(|| format!("layer {name:?}: unknown look {k:?}"))?;
            layer.look.insert(look, (dx, dy));
        }
        layer.parts = l
            .parts
            .into_iter()
            .map(|p| Part {
                id: p.id,
                origin: (p.origin[0], p.origin[1]),
                shape: p.shape,
            })
            .collect();
        for (value, drawing) in l.shapes {
            let list = match drawing {
                Drawing::One(s) => vec![s],
                Drawing::Many(v) => v,
            };
            let mut shapes = Vec::with_capacity(list.len());
            for s in list {
                let bad = |e: String| format!("layer {:?}: shape {value:?}: {e}", layer.name);
                shapes.push(Shape {
                    path: vector::parse(&s.d).map_err(bad)?,
                    d: s.d,
                    fill: vector::colour(&s.fill).map_err(bad)?,
                    stroke: vector::colour(&s.stroke).map_err(bad)?,
                    width: s.stroke_width,
                    cap: Cap::parse(&s.linecap).map_err(bad)?,
                    join: Join::parse(&s.linejoin).map_err(bad)?,
                    opacity: s.opacity,
                });
            }
            layer.shapes.insert(value, shapes);
        }
        for (value, targets) in l.poses {
            layer
                .poses
                .insert(value, targets.into_iter().map(|(id, [x, y])| (id, (x, y))).collect());
        }
        for (value, a) in l.animations {
            if a.get("keys").is_some() {
                let a: PartAnimationJson = serde_json::from_value(a)
                    .map_err(|e| format!("layer {name:?}: animation {value:?} isn't valid: {e}"))?;
                let ids: Vec<String> = a.part.into_iter().chain(a.parts).collect();
                let phases: Vec<f64> = match &a.phase {
                    None => vec![0.0; ids.len()],
                    Some(Value::Array(each)) if each.len() == ids.len() => {
                        each.iter().map(|p| number(p, "a phase")).collect::<Result<_, _>>()?
                    }
                    Some(one) => vec![number(one, "a phase")?; ids.len()],
                };
                let keys: Vec<Key> = a
                    .keys
                    .iter()
                    .map(key)
                    .collect::<Result<_, _>>()
                    .map_err(|e| format!("layer {name:?}: {e}"))?;
                let anim = PartAnimation {
                    parts: ids.into_iter().zip(phases).collect(),
                    mode: LoopMode::parse(&a.mode).map_err(|e| format!("layer {name:?}: {e}"))?,
                    keys,
                };
                layer.part_animations.insert(value, anim);
                continue;
            }
            let a: AnimationJson = serde_json::from_value(a)
                .map_err(|e| format!("layer {name:?}: animation {value:?} isn't valid: {e}"))?;
            let index = match names.iter().position(|s| *s == a.sheet) {
                Some(i) => i,
                None => {
                    sheets.push(sheet(&a.sheet).map_err(|e| format!("layer {name:?}: {}: {e}", a.sheet))?);
                    names.push(a.sheet.clone());
                    names.len() - 1
                }
            };
            let s = &sheets[index];
            let count = a.frames.as_array().map_or(0, Vec::len);
            let pivots: Vec<(f64, f64)> = match &a.pivot {
                None => vec![(0.0, 0.0); count],
                Some(Value::Array(v)) if v.first().is_some_and(Value::is_array) => {
                    if v.len() != count {
                        return Err(format!(
                            "layer {name:?}: {value:?} has {} pivots for {count} frames",
                            v.len()
                        ));
                    }
                    v.iter().map(|p| pair(p, "a pivot")).collect::<Result<_, _>>()?
                }
                Some(p) => vec![pair(p, "a pivot")?; count],
            };
            let rects = rects(&name, &value, &a, s)?;
            let mut frames = Vec::new();
            for ([x, y, w, h, ms], pivot) in rects.into_iter().zip(pivots) {
                frames.push(Frame {
                    art: Art::Raster {
                        sheet: index,
                        x,
                        y,
                        w,
                        h,
                        pivot,
                    },
                    ms,
                });
            }
            let clock = match a.clock.as_deref() {
                None | Some("change") => Clock::Change,
                Some("pet") => Clock::Pet,
                Some(other) => return Err(format!("layer {name:?}: unknown clock {other:?}")),
            };
            let anim = Animation {
                frames,
                mode: LoopMode::parse(&a.mode).map_err(|e| format!("layer {name:?}: {e}"))?,
                clock,
            };
            layer.animations.insert(value, anim);
        }
        layers.push(layer);
    }
    let pack = Pack {
        name: json.name,
        kind,
        pixel_size: json.pixel_size,
        anchor: (json.anchor[0], json.anchor[1]),
        mirror: parse_mirror(&json.mirror)?,
        accent: parse_colour(&json.accent)?,
        glow: parse_colour(&json.glow)?,
        layers,
    };
    pack.validate()?;
    Ok((pack, sheets))
}

/// An animation's frames as `[x, y, w, h, ms]` on its sheet `s`: given so, or worked out from its grid.
fn rects(layer: &str, value: &str, a: &AnimationJson, s: &Sheet) -> Result<Vec<[u32; 5]>, String> {
    let bad = |why: String| format!("layer {layer:?}: {value:?}: {why}");
    let list = a.frames.as_array().ok_or_else(|| bad("frames isn't a list".into()))?;
    let Some(g) = &a.grid else {
        return list
            .iter()
            .map(|f| {
                let f: Option<Vec<u32>> = f.as_array().and_then(|v| {
                    v.iter()
                        .map(|n| n.as_u64().and_then(|n| u32::try_from(n).ok()))
                        .collect()
                });
                match f.as_deref() {
                    Some(&[x, y, w, h, ms]) => {
                        if x.checked_add(w).is_none_or(|r| r > s.width) || y.checked_add(h).is_none_or(|b| b > s.height)
                        {
                            return Err(bad(format!("a frame is outside {}", a.sheet)));
                        }
                        Ok([x, y, w, h, ms])
                    }
                    _ => Err(bad("a frame isn't [x, y, w, h, ms]".into())),
                }
            })
            .collect();
    };
    let [w, h] = g.frame;
    if w <= 0 || h <= 0 {
        return Err(bad(format!("the grid's frame is {w} × {h}")));
    }
    if g.columns <= 0 {
        return Err(bad(format!("the grid has {} columns", g.columns)));
    }
    if g.origin.iter().chain(&g.gap).any(|&v| v < 0) {
        return Err(bad("the grid's origin or gap is negative".into()));
    }
    let ms: Vec<u32> = match &a.ms {
        Some(Value::Array(each)) => {
            if each.len() != list.len() {
                return Err(bad(format!("{} durations for {} frames", each.len(), list.len())));
            }
            each.iter()
                .map(|m| whole_ms(m, "a duration"))
                .collect::<Result<_, _>>()?
        }
        Some(one) => vec![whole_ms(one, "the duration")?; list.len()],
        None => return Err(bad("a grid animation needs its ms".into())),
    };
    list.iter()
        .zip(ms)
        .map(|(i, ms)| {
            let i = i
                .as_i64()
                .filter(|&i| i >= 0)
                .ok_or_else(|| bad(format!("frame index {i} isn't a cell")))?;
            let (col, row) = (i % g.columns, i / g.columns);
            let x = col.checked_mul(w + g.gap[0]).and_then(|v| v.checked_add(g.origin[0]));
            let y = row.checked_mul(h + g.gap[1]).and_then(|v| v.checked_add(g.origin[1]));
            match (x, y) {
                (Some(x), Some(y)) if x + w <= i64::from(s.width) && y + h <= i64::from(s.height) => {
                    Ok([x as u32, y as u32, w as u32, h as u32, ms])
                }
                _ => Err(bad(format!("cell {i} is outside {}", a.sheet))),
            }
        })
        .collect()
}

/// A sheet as an RGBA PNG.
pub fn encode_png(sheet: &Sheet) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, sheet.width, sheet.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::High);
        let mut w = enc.write_header().expect("a PNG header always writes to memory");
        let bytes: Vec<u8> = sheet
            .pixels
            .iter()
            .flat_map(|&c| [(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8])
            .collect();
        w.write_image_data(&bytes).expect("the sheet's size matches its pixels");
    }
    out
}

/// A PNG's width and height from its header, without decoding it.
pub fn png_size(bytes: &[u8]) -> Result<(u32, u32), String> {
    let reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .map_err(|e| format!("not a PNG: {e}"))?;
    let info = reader.info();
    Ok((info.width, info.height))
}

/// A PNG as a sheet of straight 0xAARRGGBB pixels: RGBA, RGB or grey, 8 bits a channel (palettes are expanded).
pub fn decode_png(bytes: &[u8]) -> Result<Sheet, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().map_err(|e| format!("not a PNG: {e}"))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("the PNG is too large")?];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("the PNG doesn't decode: {e}"))?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!("the PNG has {:?}-bit channels (8 expected)", info.bit_depth));
    }
    let px = &buf[..info.buffer_size()];
    let pixels = match info.color_type {
        png::ColorType::Rgba => px
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| (u32::from(p[3]) << 24) | (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2]))
            .collect(),
        png::ColorType::Rgb => px
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| 0xFF00_0000 | (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2]))
            .collect(),
        png::ColorType::GrayscaleAlpha => px
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| (u32::from(p[1]) << 24) | (u32::from(p[0]) * 0x01_0101))
            .collect(),
        png::ColorType::Grayscale => px.iter().map(|&g| 0xFF00_0000 | (u32::from(g) * 0x01_0101)).collect(),
        other => return Err(format!("the PNG is {other:?} (RGBA, RGB or grey expected)")),
    };
    Ok(Sheet {
        width: info.width,
        height: info.height,
        pixels,
    })
}

/// The files of a baked pack: `pack.json` and its sheets.
pub fn files(pack: &Pack, sheets: &[Sheet]) -> Vec<(String, Vec<u8>)> {
    let mut files = vec![("pack.json".to_owned(), write(pack).into_bytes())];
    for (layer, sheet) in pack.layers.iter().zip(sheets) {
        files.push((sheet_name(&layer.name), encode_png(sheet)));
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::Avatar;
    use crate::bake::bake;

    fn from(files: &[(String, Vec<u8>)]) -> impl FnMut(&str) -> Result<Sheet, String> + '_ {
        move |name| {
            let (_, bytes) = files.iter().find(|(n, _)| n == name).ok_or("missing")?;
            decode_png(bytes)
        }
    }

    #[test]
    fn a_baked_pack_writes_and_reads_back_the_same() {
        let b = bake(&Avatar::hood());
        let files = files(&b.pack, &b.sheets);
        let text = String::from_utf8(files[0].1.clone()).unwrap();
        let (pack, read_sheets) = read(&text, from(&files)).unwrap();
        assert_eq!(pack, b.pack);
        assert_eq!(read_sheets, b.sheets);
    }

    #[test]
    fn a_newer_format_version_or_an_unknown_channel_is_refused() {
        let b = bake(&Avatar::sprout());
        let text = write(&b.pack);
        let none = |_: &str| -> Result<Sheet, String> { Err("missing".into()) };
        let newer = text.replacen("\"format_version\": 1", "\"format_version\": 2", 1);
        assert!(read(&newer, none).unwrap_err().contains("newer"));
        let tail = text.replacen("\"channel\": \"state\"", "\"channel\": \"tail\"", 1);
        assert!(read(&tail, none).unwrap_err().contains("unknown channel"));
    }

    /// A vector pack with a body shape and the given eyes layer, for the R18 fields.
    fn read_parts(eyes: &str) -> Result<Pack, String> {
        let text = format!(
            r##"{{"format_version": 1, "name": "Parts", "kind": "vector", "pixel_size": 1, "anchor": [65, 110],
            "accent": "#F4A340", "glow": "#FFE08A", "layers": [
              {{"name": "body", "channel": "state", "default": "idle", "shapes": {{"idle": {{"d": "M0 0", "fill": "#000"}}}}}},
              {eyes}
            ]}}"##
        );
        read(&text, |_| Err("no sheets".into())).map(|(p, _)| p)
    }

    #[test]
    fn a_vector_pack_with_bad_shapes_or_an_outside_reference_is_refused_with_why() {
        let eyes = |shape: &str| {
            format!(
                r##"{{"name": "eyes", "channel": "eyes", "default": "open", "parts": [{{"id": "e", "origin": [60, 60]}}],
                "shapes": {{"open": {shape}}}}}"##
            )
        };
        let reason = |shape: &str| read_parts(&eyes(shape)).unwrap_err();
        // malformed path data, and numbers out of range inside it
        assert!(reason(r##"{"d": "M 0 0 L 1", "fill": "#000"}"##).contains("expected a number"));
        assert!(reason(r##"{"d": "M 1e999 0", "fill": "#000"}"##).contains("out of range"));
        assert!(reason(r##"{"d": "M 0 0", "fill": "#000", "stroke_width": 1e999}"##).contains("isn't valid"));
        // nothing from outside the pack: a path or URL where path data or a colour goes
        assert!(reason(r##"{"d": "file:///etc/passwd", "fill": "#000"}"##).contains("path data"));
        assert!(reason(r##"{"d": "M 0 0", "fill": "url(https://example.com/x.svg#g)"}"##).contains("colour"));
        assert!(reason(r##"{"d": "M 0 0", "fill": "#000", "href": "pip.svg"}"##).contains("isn't valid"));
        // a binding to a part the layer lacks
        let unknown = r##"{"name": "eyes", "channel": "eyes", "default": "open", "parts": [{"id": "e", "origin": [60, 60]}],
            "shapes": {"open": {"d": "M 0 0", "fill": "#000"}},
            "animations": {"open": {"part": "nose", "keys": [[0, 1, 100]]}}}"##;
        assert!(read_parts(unknown).unwrap_err().contains("unknown part \"nose\""));
        // and a good one, drawn as several shapes with their stroke
        let pack = read_parts(&eyes(
            r##"[{"d": "M -2 0 A 2 2 0 1 0 2 0 A 2 2 0 1 0 -2 0 Z", "fill": "#161A26"},
                 {"d": "M -1 -1 L 1 1", "stroke": "#FFF", "stroke_width": 0.5, "linecap": "round", "opacity": 0.8}]"##,
        ))
        .unwrap();
        let open = &pack.layers[1].shapes["open"];
        assert_eq!(open.len(), 2);
        assert_eq!(
            (open[1].stroke, open[1].fill, open[1].opacity),
            (Some(0xFFFFFFFF), None, 0.8)
        );
    }

    const EYES: &str = r##"{"name": "eyes", "channel": "eyes", "default": "open",
        "parts": [{"id": "eye-left", "origin": [55.5, 63.5]}, {"id": "eye-right", "origin": [74.5, 63.5]}],
        "shapes": {"open": {"d": "M0 0", "fill": "#5CFF9C"}},
        "look": {"center": [0, 0], "up": [0, -2.5]},
        "poses": {"wide": {"eye-left": [55, 62]}},
        "transition_ms": 120,
        "animations": {"open": {"parts": ["eye-left", "eye-right"], "phase": [0, 0.5], "loop": "loop",
                                "keys": [[0, 0, 90], {"dy": 1.5, "opacity": 0.5, "ms": 90}]}}}"##;

    #[test]
    fn the_part_motion_fields_read() {
        let pack = read_parts(EYES).unwrap();
        let eyes = &pack.layers[1];
        assert_eq!(eyes.parts.len(), 2);
        assert_eq!(eyes.poses["wide"]["eye-left"], (55.0, 62.0));
        assert_eq!(eyes.transition_ms, 120);
        assert_eq!(eyes.look[&Look { dx: 0, dy: -1 }], (0.0, -2.5));
        let a = &eyes.part_animations["open"];
        assert_eq!(a.parts, [("eye-left".to_owned(), 0.0), ("eye-right".to_owned(), 0.5)]);
        assert_eq!((a.keys[1].dy, a.keys[1].opacity, a.keys[1].ms), (1.5, 0.5, 90));
    }

    #[test]
    fn each_part_motion_error_is_a_reason() {
        let bad = [
            (EYES.replace("[55.5, 63.5]", "[1e999, 63.5]"), "isn't valid"),
            (EYES.replace("[55.5, 63.5]", "[500, 63.5]"), "outside the art box"),
            (
                EYES.replace(r#""wide": {"eye-left""#, r#""wide": {"nose""#),
                "unknown part \"nose\"",
            ),
            (
                EYES.replace(
                    r#"{"id": "eye-right", "origin": [74.5, 63.5]}"#,
                    r#"{"id": "eye-right", "origin": [74.5, 63.5]}, {"id": "eye-left", "origin": [60, 60]}"#,
                ),
                "used twice",
            ),
            (
                EYES.replace("[0, 0, 90]", &vec!["[0, 0, 9]"; 65].join(", ")),
                "keys (1 to 64)",
            ),
            (EYES.replace("\"ms\": 90", "\"ms\": 10001"), "lasts 10001 ms"),
            (
                EYES.replace("\"transition_ms\": 120", "\"transition_ms\": 10001"),
                "transition_ms",
            ),
        ];
        for (eyes, why) in bad {
            let err = read_parts(&eyes).unwrap_err();
            assert!(err.contains(why), "{why}: {err}");
        }
    }
}
