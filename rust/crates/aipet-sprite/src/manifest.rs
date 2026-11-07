//! A pack's manifest (`pack.json`) and its PNG sheets: read into the in-memory [`Pack`], and written by the bake.
//!
//! This is the subset the baked pixel packs use: kinds, layers bound to channels, raster frames
//! `[x, y, w, h, duration_ms]`, loop modes, the pet clock, fallbacks (a `null` one draws nothing), glow and mirror
//! per layer, and look tables. Task 3 adds the rest of the contract (parts, poses, vector shapes, Aseprite) and the
//! limits for packs from disk.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::bake::Sheet;
use crate::pack::{Animation, Art, Clock, Frame, Kind, Layer, LoopMode, Mirror, Pack};
use crate::pose::{Channel, Look};

/// The newest `format_version` this build reads.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct PackJson {
    format_version: u32,
    name: String,
    kind: String,
    pixel_size: f64,
    anchor: [f64; 2],
    mirror: String,
    accent: String,
    glow: String,
    layers: Vec<LayerJson>,
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
    #[serde(default)]
    animations: BTreeMap<String, AnimationJson>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnimationJson {
    sheet: String,
    frames: Vec<[u32; 5]>,
    #[serde(rename = "loop")]
    mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clock: Option<String>,
}

fn colour(c: u32) -> String {
    if c >> 24 == 0xFF {
        format!("#{:06X}", c & 0x00FF_FFFF)
    } else {
        format!("#{c:08X}")
    }
}

fn parse_colour(s: &str) -> Result<u32, String> {
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

/// The manifest for `pack`, whose frames' sheets are by index the layers' own (`<layer>.png`), as the bake makes them.
pub fn write(pack: &Pack) -> String {
    let json = PackJson {
        format_version: FORMAT_VERSION,
        name: pack.name.clone(),
        kind: match pack.kind {
            Kind::Pixel => "pixel",
            Kind::Painted => "painted",
            Kind::Vector => "vector",
        }
        .to_owned(),
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
                animations: l
                    .animations
                    .iter()
                    .map(|(value, a)| {
                        let frames = a
                            .frames
                            .iter()
                            .map(|f| match f.art {
                                Art::Raster { x, y, w, h, .. } => [x, y, w, h, f.ms],
                                Art::Vector => [0, 0, 0, 0, f.ms],
                            })
                            .collect();
                        let json = AnimationJson {
                            sheet: sheet_name(&l.name),
                            frames,
                            mode: match a.mode {
                                LoopMode::Loop => "loop",
                                LoopMode::Once => "once",
                                LoopMode::PingPong => "pingpong",
                            }
                            .to_owned(),
                            clock: (a.clock == Clock::Pet).then(|| "pet".to_owned()),
                        };
                        (value.clone(), json)
                    })
                    .collect(),
            })
            .collect(),
    };
    let mut text = serde_json::to_string_pretty(&json).expect("a pack's manifest always serialises");
    text.push('\n');
    text
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

/// The pack a manifest describes, with its sheets named as it names them; validated. `sheets` gives each file's
/// decoded sheet.
pub fn read(text: &str, sheets: &BTreeMap<String, Sheet>) -> Result<(Pack, Vec<Sheet>), String> {
    let json: PackJson = serde_json::from_str(text).map_err(|e| format!("the manifest isn't valid: {e}"))?;
    if json.format_version > FORMAT_VERSION {
        return Err(format!(
            "format_version {} is newer than this AiPet reads ({FORMAT_VERSION})",
            json.format_version
        ));
    }
    let kind = Kind::parse(&json.kind).ok_or_else(|| format!("unknown kind {:?}", json.kind))?;
    let mut used: Vec<String> = Vec::new();
    let mut layers = Vec::new();
    for l in json.layers {
        let channel =
            Channel::parse(&l.channel).ok_or_else(|| format!("layer {:?}: unknown channel {:?}", l.name, l.channel))?;
        let mut layer = Layer::new(&l.name, channel, &l.default);
        layer.hit = l.hit;
        layer.glow = l.glow;
        layer.mirror = l.mirror.as_deref().map(parse_mirror).transpose()?;
        layer.fallback = l.fallback;
        for (key, [dx, dy]) in l.look {
            let look = Look::parse(&key).ok_or_else(|| format!("layer {:?}: unknown look {key:?}", l.name))?;
            layer.look.insert(look, (dx, dy));
        }
        for (value, a) in l.animations {
            let sheet = match used.iter().position(|s| *s == a.sheet) {
                Some(i) => i,
                None => {
                    if !sheets.contains_key(&a.sheet) {
                        return Err(format!("layer {:?}: sheet {:?} is missing", l.name, a.sheet));
                    }
                    used.push(a.sheet.clone());
                    used.len() - 1
                }
            };
            let s = &sheets[&a.sheet];
            let mut frames = Vec::new();
            for [x, y, w, h, ms] in a.frames {
                if x.checked_add(w).is_none_or(|r| r > s.width) || y.checked_add(h).is_none_or(|b| b > s.height) {
                    return Err(format!(
                        "layer {:?}: a frame of {value:?} is outside {}",
                        l.name, a.sheet
                    ));
                }
                frames.push(Frame {
                    art: Art::Raster {
                        sheet,
                        x,
                        y,
                        w,
                        h,
                        pivot: (0.0, 0.0),
                    },
                    ms,
                });
            }
            let clock = match a.clock.as_deref() {
                None | Some("change") => Clock::Change,
                Some("pet") => Clock::Pet,
                Some(other) => return Err(format!("layer {:?}: unknown clock {other:?}", l.name)),
            };
            layer.animations.insert(
                value,
                Animation {
                    frames,
                    mode: LoopMode::parse(&a.mode).map_err(|e| format!("layer {:?}: {e}", l.name))?,
                    clock,
                },
            );
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
    Ok((pack, used.into_iter().map(|s| sheets[&s].clone()).collect()))
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

/// A PNG as a sheet of straight 0xAARRGGBB pixels: RGBA or RGB, 8 bits a channel.
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
        other => return Err(format!("the PNG is {other:?} (RGBA or RGB expected)")),
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

    #[test]
    fn a_baked_pack_writes_and_reads_back_the_same() {
        let b = bake(&Avatar::hood());
        let files = files(&b.pack, &b.sheets);
        let sheets: BTreeMap<String, Sheet> = files[1..]
            .iter()
            .map(|(name, bytes)| (name.clone(), decode_png(bytes).unwrap()))
            .collect();
        let text = String::from_utf8(files[0].1.clone()).unwrap();
        let (pack, read_sheets) = read(&text, &sheets).unwrap();
        assert_eq!(pack, b.pack);
        assert_eq!(read_sheets, b.sheets);
    }

    #[test]
    fn a_newer_format_version_or_an_unknown_channel_is_refused() {
        let b = bake(&Avatar::sprout());
        let text = write(&b.pack);
        let newer = text.replacen("\"format_version\": 1", "\"format_version\": 2", 1);
        assert!(read(&newer, &BTreeMap::new()).unwrap_err().contains("newer"));
        let tail = text.replacen("\"channel\": \"state\"", "\"channel\": \"tail\"", 1);
        assert!(read(&tail, &BTreeMap::new()).unwrap_err().contains("unknown channel"));
    }
}
