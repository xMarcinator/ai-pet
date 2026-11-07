//! Finding and loading pets: the built-in packs, then the avatars folder's packs and legacy avatars, by name.
//!
//! A folder is a pack when it holds a `pack.json` (the manifest) or an Aseprite `json-array` export with its PNG (and
//! an optional `pack.json` beside it for the rest). A `.json` file is a legacy ellipse avatar, baked into a pack once
//! when it loads. Every pack loads completely or not at all, within the spec's limits, and reads only files inside
//! its own folder. Built-ins win a name; the others are taken in sorted order, and a later pet with a name already
//! taken is skipped with its reason.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::avatar::{Avatar, AvatarJson, drawable, read_all_text};
use crate::bake::{Sheet, bake};
use crate::manifest::{self, decode_png, png_size};
use crate::pack::{Animation, Art, Clock, Frame, Kind, Layer, LoopMode, Mirror, Pack, Part};
use crate::pose::{ART_H, ART_W, Channel, HALO_PAD};

/// A manifest or an Aseprite export's JSON: at most 256 KB, nested at most 64 deep.
pub const MAX_MANIFEST: u64 = 256 * 1024;
pub const MAX_DEPTH: usize = 64;
/// A sheet: at most 8 MB on disk, and 2048 px a side.
pub const MAX_PNG: u64 = 8 * 1024 * 1024;
pub const MAX_SIDE: u32 = 2048;
/// A pack's sheets decoded: at most 64 MB.
pub const MAX_DECODED: u64 = 64 * 1024 * 1024;

/// A pet ready to wear: its pack and the pack's sheets (by index, as its frames name them), straight colours.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedPack {
    pub pack: Pack,
    pub sheets: Vec<Sheet>,
}

impl LoadedPack {
    pub fn name(&self) -> &str {
        &self.pack.name
    }

    /// The built-in pet's embedded pack for `avatar`, else the avatar baked now.
    pub fn of_avatar(avatar: &Avatar) -> LoadedPack {
        let (pack, sheets) = crate::builtin::pack_for(avatar).unwrap_or_else(|| {
            let b = bake(avatar);
            (b.pack, b.sheets)
        });
        LoadedPack { pack, sheets }
    }
}

/// A pack or file that didn't load, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    /// The folder or file's name in the avatars folder.
    pub file: String,
    pub reason: String,
}

/// What the avatars folder gave: the pets in order (built-ins first), and what was skipped.
#[derive(Clone, Debug, Default)]
pub struct Found {
    pub pets: Vec<LoadedPack>,
    pub skipped: Vec<Skipped>,
}

/// The built-in pets, then those in `dir`, by name; a missing or unreadable folder gives just the built-ins.
pub fn all(dir: &Path) -> Found {
    let mut found = Found {
        pets: crate::builtin::all(),
        skipped: Vec::new(),
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return found;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let loaded = if path.is_dir() {
            load_folder(&path)
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
            load_legacy(&path)
        } else {
            continue;
        };
        match loaded {
            Ok(pet) if found.pets.iter().any(|p| p.name() == pet.name()) => found.skipped.push(Skipped {
                file,
                reason: format!("a pet named {:?} is already loaded", pet.name()),
            }),
            Ok(pet) => found.pets.push(pet),
            Err(reason) => found.skipped.push(Skipped { file, reason }),
        }
    }
    found
}

/// A legacy ellipse avatar, baked into a pack: exactly the drawing the pet made of it before packs.
pub fn load_legacy(path: &Path) -> Result<LoadedPack, String> {
    let len = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > MAX_MANIFEST {
        return Err(format!("the file is {len} bytes (at most {MAX_MANIFEST})"));
    }
    let text = read_all_text(path).map_err(|e| e.to_string())?;
    let json = AvatarJson::parse(&text).ok_or("not an avatar file")?;
    if !drawable(&json) {
        return Err("the avatar has nothing to draw".to_owned());
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let avatar = Avatar::from_json(json, &stem).ok_or("the avatar has no name")?;
    Ok(LoadedPack::of_avatar(&avatar))
}

/// The depth of a JSON text's nesting, counting brackets outside strings.
fn depth(text: &str) -> usize {
    let (mut depth, mut max, mut in_string, mut escaped) = (0usize, 0usize, false, false);
    for c in text.chars() {
        if in_string {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '[' | '{' => {
                depth += 1;
                max = max.max(depth);
            }
            ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    max
}

/// A JSON file inside a pack: within the size and depth limits.
fn read_json(path: &Path) -> Result<String, String> {
    let len = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > MAX_MANIFEST {
        return Err(format!("{} is {len} bytes (at most {MAX_MANIFEST})", name_of(path)));
    }
    let text = read_all_text(path).map_err(|e| format!("{}: {e}", name_of(path)))?;
    if depth(&text) > MAX_DEPTH {
        return Err(format!("{} is nested more than {MAX_DEPTH} deep", name_of(path)));
    }
    Ok(text)
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Device names Windows keeps for itself, which no file in a pack may have.
const DEVICES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// `name` as a file inside `folder`: relative, without `..`, not a device, not a link or reparse point, and inside
/// the folder once both are resolved.
fn inside(folder: &Path, name: &str) -> Result<PathBuf, String> {
    let rel = Path::new(name);
    if name.is_empty() || rel.is_absolute() || name.contains(':') || name.starts_with(['/', '\\']) {
        return Err(format!("{name:?} isn't a path inside the pack"));
    }
    for part in rel.components() {
        match part {
            Component::Normal(p) => {
                let stem = p.to_string_lossy();
                let stem = stem
                    .split('.')
                    .next()
                    .unwrap_or_default()
                    .trim_end()
                    .to_ascii_uppercase();
                if DEVICES.contains(&stem.as_str()) {
                    return Err(format!("{name:?} names a device"));
                }
            }
            Component::CurDir => {}
            _ => return Err(format!("{name:?} leaves the pack's folder")),
        }
    }
    let path = folder.join(rel);
    let meta = fs::symlink_metadata(&path).map_err(|_| format!("{name:?} is missing"))?;
    if meta.file_type().is_symlink() || reparse_point(&meta) {
        return Err(format!("{name:?} is a link"));
    }
    let root = folder.canonicalize().map_err(|e| e.to_string())?;
    let real = path.canonicalize().map_err(|_| format!("{name:?} is missing"))?;
    if !real.starts_with(&root) {
        return Err(format!("{name:?} leaves the pack's folder"));
    }
    Ok(real)
}

#[cfg(windows)]
fn reparse_point(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn reparse_point(_: &fs::Metadata) -> bool {
    false
}

/// Reads a pack's sheets, checking each one's size on disk and from its header before decoding, and the pack's
/// decoded total.
struct Sheets<'a> {
    folder: &'a Path,
    decoded: u64,
}

impl Sheets<'_> {
    fn read(&mut self, name: &str) -> Result<Sheet, String> {
        let path = inside(self.folder, name)?;
        let len = fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if len > MAX_PNG {
            return Err(format!("{name:?} is {len} bytes (at most {MAX_PNG})"));
        }
        let bytes = fs::read(&path).map_err(|e| format!("{name:?}: {e}"))?;
        let (w, h) = png_size(&bytes).map_err(|e| format!("{name:?}: {e}"))?;
        if w > MAX_SIDE || h > MAX_SIDE {
            return Err(format!("{name:?} is {w} × {h} px (at most {MAX_SIDE} a side)"));
        }
        self.decoded += u64::from(w) * u64::from(h) * 4;
        if self.decoded > MAX_DECODED {
            return Err(format!(
                "the pack's sheets come to more than {} MB decoded",
                MAX_DECODED >> 20
            ));
        }
        decode_png(&bytes).map_err(|e| format!("{name:?}: {e}"))
    }
}

/// A pack folder: a manifest, or an Aseprite export.
pub fn load_folder(folder: &Path) -> Result<LoadedPack, String> {
    let meta = fs::symlink_metadata(folder).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() || reparse_point(&meta) {
        return Err("the folder is a link".to_owned());
    }
    let mut sheets = Sheets { folder, decoded: 0 };
    let manifest = folder.join("pack.json");
    let export = find_export(folder)?;
    let (pack, sheets) = match export {
        Some(export) => {
            let extra = if manifest.exists() {
                Some(read_json(&inside(folder, "pack.json")?)?)
            } else {
                None
            };
            aseprite(&read_json(&export)?, extra.as_deref(), |name| sheets.read(name))?
        }
        None if manifest.exists() => {
            let text = read_json(&inside(folder, "pack.json")?)?;
            manifest::read(&text, |name| sheets.read(name))?
        }
        None => return Err("no pack.json and no Aseprite export".to_owned()),
    };
    fits(&pack)?;
    Ok(LoadedPack { pack, sheets })
}

/// Every raster frame within the art box and its halo, at the pack's pixel size.
fn fits(pack: &Pack) -> Result<(), String> {
    for layer in &pack.layers {
        for (value, a) in &layer.animations {
            for f in &a.frames {
                if let Art::Raster { w, h, .. } = f.art {
                    let (w, h) = (f64::from(w) * pack.pixel_size, f64::from(h) * pack.pixel_size);
                    if w > ART_W + 2.0 * HALO_PAD || h > ART_H + 2.0 * HALO_PAD {
                        return Err(format!(
                            "layer {:?}: a frame of {value:?} is {w} × {h} px, larger than the art box",
                            layer.name
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// The folder's Aseprite export: a `.json` other than `pack.json` with `frames` and `meta`.
fn find_export(folder: &Path) -> Result<Option<PathBuf>, String> {
    let mut found = None;
    let mut entries: Vec<PathBuf> = fs::read_dir(folder)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        let is_json = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"));
        if !is_json || path.file_name().is_some_and(|n| n == "pack.json") {
            continue;
        }
        let text = read_json(&inside(folder, &name_of(&path))?)?;
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", name_of(&path)))?;
        if v.get("frames").is_some() && v.get("meta").is_some() {
            found = Some(path);
            break;
        }
    }
    Ok(found)
}

/// The channel a layer named in an export defaults to, when no `pack.json` says.
fn channel_for(layer: &str) -> Option<Channel> {
    match layer {
        "body" => Some(Channel::State),
        other => Channel::parse(other),
    }
}

/// The value a layer shows when no `pack.json` gives its default: the channel's resting one, if the export has it.
fn resting(channel: Channel) -> &'static str {
    match channel {
        Channel::State => "idle",
        Channel::Eyes => "open",
        Channel::Arms => "rest",
        Channel::Feet => "stand",
        Channel::Prop => "laptop",
        Channel::Headphones => "on",
        Channel::Fx => "zs",
        Channel::Facing => "right",
    }
}

fn rect(v: &Value) -> Option<(i64, i64, i64, i64)> {
    Some((v["x"].as_i64()?, v["y"].as_i64()?, v["w"].as_i64()?, v["h"].as_i64()?))
}

/// An Aseprite `json-array` export (and an optional `pack.json` for the rest) as a pack. Tags `<layer>:<value>`
/// become that layer's animations; their direction and `repeat` the loop; frames keep their durations, and trimmed
/// ones their place through `spriteSourceSize`. A slice named `anchor` sets the anchor by its pivot; a slice named
/// after a part gives its origin (its first key's pivot) and, on a tag's first frame, that value's pose position.
pub fn aseprite(
    text: &str,
    extra: Option<&str>,
    mut sheet: impl FnMut(&str) -> Result<Sheet, String>,
) -> Result<(Pack, Vec<Sheet>), String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("the export isn't valid: {e}"))?;
    let frames = match &v["frames"] {
        Value::Array(f) => f,
        Value::Object(_) => return Err("the export is json-hash; export it as an array (json-array)".to_owned()),
        _ => return Err("the export has no frames".to_owned()),
    };
    let meta = &v["meta"];
    let image = meta["image"].as_str().ok_or("the export names no image")?;
    let image = Path::new(image)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let sheet = sheet(&image)?;
    let extra: Value = match extra {
        Some(t) => serde_json::from_str(t).map_err(|e| format!("pack.json isn't valid: {e}"))?,
        None => Value::Null,
    };
    let tags = meta["frameTags"].as_array().cloned().unwrap_or_default();
    if tags.is_empty() {
        return Err("the export has no tags (name them <layer>:<value>)".to_owned());
    }
    // the pack.json's layers, by name, give the channels, defaults, fallbacks and flags
    let declared: BTreeMap<String, &Value> = extra["layers"]
        .as_array()
        .map(|ls| {
            ls.iter()
                .filter_map(|l| Some((l["name"].as_str()?.to_owned(), l)))
                .collect()
        })
        .unwrap_or_default();
    let slices = meta["slices"].as_array().cloned().unwrap_or_default();
    let slice_at = |name: &str, frame: i64| -> Option<(f64, f64)> {
        let s = slices.iter().find(|s| s["name"] == name)?;
        let keys = s["keys"].as_array()?;
        let key = keys
            .iter()
            .rev()
            .find(|k| k["frame"].as_i64().is_some_and(|f| f <= frame))
            .or(keys.first())?;
        let (x, y, _, _) = rect(&key["bounds"])?;
        let (px, py) = (
            key["pivot"]["x"].as_i64().unwrap_or(0),
            key["pivot"]["y"].as_i64().unwrap_or(0),
        );
        Some(((x + px) as f64, (y + py) as f64))
    };
    let pixel_size = extra["pixel_size"].as_f64().unwrap_or(1.0);
    let mut layers: Vec<Layer> = Vec::new();
    for tag in &tags {
        let name = tag["name"].as_str().unwrap_or_default();
        let Some((layer_name, value)) = name.split_once(':') else {
            return Err(format!("tag {name:?} isn't <layer>:<value>"));
        };
        let channel = match declared.get(layer_name) {
            Some(l) => {
                let c = l["channel"].as_str().unwrap_or_default();
                Channel::parse(c).ok_or_else(|| format!("layer {layer_name:?}: unknown channel {c:?}"))?
            }
            None => channel_for(layer_name).ok_or_else(|| {
                format!("tag {name:?} names layer {layer_name:?}, which isn't a channel or in pack.json")
            })?,
        };
        let (from, to) = (tag["from"].as_i64().unwrap_or(0), tag["to"].as_i64().unwrap_or(-1));
        if from < 0 || to < from || to as usize >= frames.len() {
            return Err(format!(
                "tag {name:?} covers frames {from} to {to}, of {}",
                frames.len()
            ));
        }
        let mut order: Vec<i64> = (from..=to).collect();
        let direction = tag["direction"].as_str().unwrap_or("forward");
        if matches!(direction, "reverse" | "pingpong_reverse") {
            order.reverse();
        }
        let mut mode = if direction.starts_with("pingpong") {
            LoopMode::PingPong
        } else {
            LoopMode::Loop
        };
        // a repeat count plays the frames that many times, then holds the last
        if let Some(n) = tag["repeat"]
            .as_str()
            .and_then(|r| r.parse::<usize>().ok())
            .filter(|&n| n > 0)
        {
            let once: Vec<i64> = if mode == LoopMode::PingPong && order.len() > 1 {
                order.iter().chain(order.iter().rev().skip(1)).copied().collect()
            } else {
                order.clone()
            };
            order = (0..n).flat_map(|_| once.iter().copied()).collect();
            mode = LoopMode::Once;
        }
        let mut list = Vec::new();
        for i in order {
            let f = &frames[i as usize];
            let (x, y, w, h) = rect(&f["frame"]).ok_or_else(|| format!("frame {i} has no rectangle"))?;
            let (sx, sy) = rect(&f["spriteSourceSize"]).map_or((0, 0), |r| (r.0, r.1));
            let ms = f["duration"].as_u64().unwrap_or(100).min(u64::from(u32::MAX)) as u32;
            if x < 0 || y < 0 || w <= 0 || h <= 0 || x + w > i64::from(sheet.width) || y + h > i64::from(sheet.height) {
                return Err(format!("frame {i} is outside {image}"));
            }
            list.push(Frame {
                art: Art::Raster {
                    sheet: 0,
                    x: x as u32,
                    y: y as u32,
                    w: w as u32,
                    h: h as u32,
                    // a trimmed frame's pixels start this far into the sprite
                    pivot: (-sx as f64, -sy as f64),
                },
                ms,
            });
        }
        let layer = match layers.iter_mut().position(|l| l.name == layer_name) {
            Some(i) => &mut layers[i],
            None => {
                layers.push(Layer::new(layer_name, channel, ""));
                layers.last_mut().expect("just pushed")
            }
        };
        layer.animations.insert(
            value.to_owned(),
            Animation {
                frames: list,
                mode,
                clock: Clock::Change,
            },
        );
        // parts named by slices: the origin from the first key, each value's position from its first frame
        let ids: Vec<String> = declared
            .get(layer_name)
            .and_then(|l| l["parts"].as_array())
            .map(|ps| ps.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        for id in ids {
            let Some(origin) = slice_at(&id, 0) else {
                return Err(format!("layer {layer_name:?}: part {id:?} has no slice"));
            };
            if !layer.parts.iter().any(|p| p.id == id) {
                layer.parts.push(Part {
                    id: id.clone(),
                    origin: (origin.0 * pixel_size, origin.1 * pixel_size),
                    shape: None,
                });
            }
            if let Some(at) = slice_at(&id, from) {
                layer
                    .poses
                    .entry(value.to_owned())
                    .or_default()
                    .insert(id.clone(), (at.0 * pixel_size, at.1 * pixel_size));
            }
        }
    }
    for layer in &mut layers {
        let d = declared.get(&layer.name);
        layer.default = d
            .and_then(|l| l["default"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                let resting = resting(layer.channel);
                if layer.animations.contains_key(resting) {
                    resting.to_owned()
                } else {
                    layer.animations.keys().next().cloned().unwrap_or_default()
                }
            });
        if let Some(l) = d {
            layer.hit = l["hit"].as_bool().unwrap_or(false);
            layer.glow = l["glow"].as_bool().unwrap_or(false);
            if let Some(fb) = l["fallback"].as_object() {
                for (k, to) in fb {
                    layer.fallback.insert(k.clone(), to.as_str().map(str::to_owned));
                }
            }
        } else {
            layer.hit = matches!(layer.channel, Channel::State | Channel::Arms | Channel::Feet);
        }
    }
    let anchor = slice_at("anchor", 0)
        .or_else(|| {
            let a = extra["anchor"].as_array()?;
            Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?))
        })
        .unwrap_or((0.0, 0.0));
    let name = extra["name"].as_str().map(str::to_owned).unwrap_or_else(|| {
        Path::new(&image)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let colour = |key: &str, default: u32| -> Result<u32, String> {
        extra[key].as_str().map_or(Ok(default), manifest::parse_colour)
    };
    let pack = Pack {
        name,
        kind: match extra["kind"].as_str() {
            Some(k) => Kind::parse(k).ok_or_else(|| format!("unknown kind {k:?}"))?,
            None => Kind::Pixel,
        },
        pixel_size,
        anchor,
        mirror: match extra["mirror"].as_str() {
            Some("none") => Mirror::None,
            _ => Mirror::Flip,
        },
        accent: colour("accent", 0xFF5C_FF9C)?,
        glow: colour("glow", 0xFF7F_EFFF)?,
        layers,
    };
    pack.validate()?;
    Ok((pack, vec![sheet]))
}

/// `AiPet --check-pack <folder>`: the line to print and whether the pack loads.
pub fn check(folder: &Path) -> (String, bool) {
    if !folder.is_dir() {
        return (format!("{}: not a folder", folder.display()), false);
    }
    match load_folder(folder) {
        Ok(p) => {
            let kind = match p.pack.kind {
                Kind::Pixel => "pixel",
                Kind::Painted => "painted",
                Kind::Vector => "vector",
            };
            let line = format!(
                "OK {} ({kind}, {} layers, {} frames)",
                p.pack.name,
                p.pack.layers.len(),
                p.pack.frame_count()
            );
            (line, true)
        }
        Err(reason) => (reason, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{encode_png, files};

    /// A fresh folder for a test.
    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("aipet-load-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// The baked Hood written as a pack folder `at`.
    fn hood_pack(at: &Path) {
        fs::create_dir_all(at).unwrap();
        let b = bake(&Avatar::hood());
        for (name, bytes) in files(&b.pack, &b.sheets) {
            fs::write(at.join(name), bytes).unwrap();
        }
    }

    fn manifest(at: &Path) -> String {
        fs::read_to_string(at.join("pack.json")).unwrap()
    }

    #[test]
    fn a_pack_folder_loads_and_a_built_in_wins_its_name() {
        let d = dir("folder");
        hood_pack(&d.join("hood"));
        let renamed = d.join("other-hood");
        hood_pack(&renamed);
        fs::write(
            renamed.join("pack.json"),
            manifest(&renamed).replace("\"name\": \"Hood\"", "\"name\": \"Other\""),
        )
        .unwrap();
        let found = all(&d);
        let names: Vec<&str> = found.pets.iter().map(LoadedPack::name).collect();
        assert_eq!(names, ["Sprout", "Hood", "Hood (green)", "Other"]);
        assert_eq!(
            found.skipped,
            [Skipped {
                file: "hood".into(),
                reason: "a pet named \"Hood\" is already loaded".into()
            }]
        );
        assert!(check(&renamed).1);
    }

    #[test]
    fn a_legacy_avatar_is_baked_exactly_and_a_broken_one_skipped() {
        let d = dir("legacy");
        let green = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../avatars/hood-green.json");
        fs::copy(&green, d.join("mint.json")).unwrap();
        fs::write(
            d.join("mint.json"),
            fs::read_to_string(d.join("mint.json"))
                .unwrap()
                .replace("Hood (green)", "Mint"),
        )
        .unwrap();
        fs::write(d.join("broken.json"), "{ not json").unwrap();
        let found = all(&d);
        let mint = found
            .pets
            .iter()
            .find(|p| p.name() == "Mint")
            .expect("the legacy avatar loads");
        let mut avatar = Avatar::load(&green).unwrap();
        avatar.name = "Mint".into();
        let b = bake(&avatar);
        assert_eq!((&mint.pack, &mint.sheets), (&b.pack, &b.sheets));
        assert_eq!(found.skipped.len(), 1);
        assert_eq!(found.skipped[0].file, "broken.json");
    }

    fn reason(folder: &Path) -> String {
        load_folder(folder).unwrap_err()
    }

    #[test]
    fn each_limit_and_path_rule_gives_its_reason() {
        let d = dir("limits");
        // a sheet whose header says it's too wide
        let wide = d.join("wide");
        hood_pack(&wide);
        let big = Sheet {
            width: 2049,
            height: 1,
            pixels: vec![0; 2049],
        };
        fs::write(wide.join("body.png"), encode_png(&big)).unwrap();
        assert!(reason(&wide).contains("at most 2048 a side"), "{}", reason(&wide));
        // too much decoded: many 2048 × 2048 sheets
        let heavy = d.join("heavy");
        hood_pack(&heavy);
        let full = Sheet {
            width: 2048,
            height: 2048,
            pixels: vec![0; 2048 * 2048],
        };
        let bytes = encode_png(&full);
        for layer in ["body", "headphones-outline", "arms", "arms-run", "feet"] {
            fs::write(heavy.join(format!("{layer}.png")), &bytes).unwrap();
        }
        assert!(reason(&heavy).contains("MB decoded"), "{}", reason(&heavy));
        // a path out of the folder, and a device name
        let escape = d.join("escape");
        hood_pack(&escape);
        fs::write(
            escape.join("pack.json"),
            manifest(&escape).replacen("\"body.png\"", "\"../body.png\"", 1),
        )
        .unwrap();
        assert!(
            reason(&escape).contains("leaves the pack's folder"),
            "{}",
            reason(&escape)
        );
        fs::write(
            escape.join("pack.json"),
            manifest(&escape).replacen("\"../body.png\"", "\"NUL.png\"", 1),
        )
        .unwrap();
        assert!(reason(&escape).contains("names a device"), "{}", reason(&escape));
        // a cycle in the fallbacks, and a too-new format
        let cycle = d.join("cycle");
        hood_pack(&cycle);
        let text = manifest(&cycle).replacen(
            "\"default\": \"body\"",
            "\"default\": \"body\", \"fallback\": {\"done\": \"listen\", \"listen\": \"done\"}",
            1,
        );
        fs::write(cycle.join("pack.json"), &text).unwrap();
        assert!(reason(&cycle).contains("cycle"), "{}", reason(&cycle));
        fs::write(
            cycle.join("pack.json"),
            text.replacen("\"format_version\": 1", "\"format_version\": 9", 1),
        )
        .unwrap();
        assert!(reason(&cycle).contains("newer"), "{}", reason(&cycle));
        // a manifest too large, and too deep
        let huge = d.join("huge");
        fs::create_dir_all(&huge).unwrap();
        fs::write(huge.join("pack.json"), " ".repeat(MAX_MANIFEST as usize + 1)).unwrap();
        assert!(reason(&huge).contains("bytes (at most"), "{}", reason(&huge));
        fs::write(huge.join("pack.json"), format!("{}{}", "[".repeat(65), "]".repeat(65))).unwrap();
        assert!(reason(&huge).contains("nested more than 64"), "{}", reason(&huge));
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_sheet_or_folder_is_refused() {
        let d = dir("links");
        let pack = d.join("linked");
        hood_pack(&pack);
        let outside = d.join("elsewhere.png");
        fs::rename(pack.join("body.png"), &outside).unwrap();
        std::os::unix::fs::symlink(&outside, pack.join("body.png")).unwrap();
        assert!(reason(&pack).contains("is a link"), "{}", reason(&pack));
        let folder_link = d.join("folder-link");
        std::os::unix::fs::symlink(&pack, &folder_link).unwrap();
        assert!(reason(&folder_link).contains("folder is a link"));
    }

    /// A tiny Aseprite json-array export: two frames of a 4 × 4 sheet, a body tag, an eyes tag, and slices.
    fn export(frames: &str) -> String {
        format!(
            r#"{{"frames": {frames},
              "meta": {{"image": "pet.png", "size": {{"w": 8, "h": 4}},
                "frameTags": [
                  {{"name": "body:idle", "from": 0, "to": 1, "direction": "pingpong"}},
                  {{"name": "eyes:open", "from": 0, "to": 0, "direction": "forward", "repeat": "2"}}
                ],
                "slices": [
                  {{"name": "anchor", "keys": [{{"frame": 0, "bounds": {{"x": 1, "y": 2, "w": 2, "h": 2}}, "pivot": {{"x": 1, "y": 1}}}}]}},
                  {{"name": "eye-left", "keys": [{{"frame": 0, "bounds": {{"x": 0, "y": 1, "w": 1, "h": 1}}, "pivot": {{"x": 0, "y": 0}}}}]}}
                ]}}}}"#
        )
    }

    const ARRAY: &str = r#"[
        {"filename": "0", "frame": {"x": 0, "y": 0, "w": 4, "h": 4}, "spriteSourceSize": {"x": 0, "y": 0, "w": 4, "h": 4}, "duration": 120},
        {"filename": "1", "frame": {"x": 4, "y": 1, "w": 3, "h": 3}, "spriteSourceSize": {"x": 1, "y": 1, "w": 3, "h": 3}, "duration": 80}
    ]"#;

    fn sheet(_: &str) -> Result<Sheet, String> {
        Ok(Sheet {
            width: 8,
            height: 4,
            pixels: vec![0xFF00_0000; 32],
        })
    }

    #[test]
    fn an_aseprite_export_maps_tags_directions_repeats_trims_and_slices() {
        let extra = r#"{"name": "Ase", "pixel_size": 5, "layers": [{"name": "eyes", "channel": "eyes", "parts": ["eye-left"]}]}"#;
        let (pack, sheets) = aseprite(&export(ARRAY), Some(extra), sheet).unwrap();
        assert_eq!((pack.name.as_str(), sheets.len(), pack.anchor), ("Ase", 1, (2.0, 3.0)));
        let body = &pack.layers[0];
        assert_eq!((body.channel, body.default.as_str()), (Channel::State, "idle"));
        let idle = &body.animations["idle"];
        assert_eq!(idle.mode, LoopMode::PingPong);
        assert_eq!(idle.frames.iter().map(|f| f.ms).collect::<Vec<_>>(), [120, 80]);
        // the trimmed frame starts one pixel in
        assert!(matches!(
            idle.frames[1].art,
            Art::Raster {
                pivot: (-1.0, -1.0),
                w: 3,
                ..
            }
        ));
        // repeat 2: played twice, then held
        let open = &pack.layers[1].animations["open"];
        assert_eq!((open.mode, open.frames.len()), (LoopMode::Once, 2));
        // a part slice: its origin, and the value's position
        let eyes = &pack.layers[1];
        assert_eq!(eyes.parts[0].origin, (0.0, 5.0));
        assert_eq!(eyes.poses["open"]["eye-left"], (0.0, 5.0));
    }

    #[test]
    fn a_json_hash_export_untagged_frames_or_an_unknown_layer_are_reasons() {
        let hash = export(r#"{"0.png": {"frame": {"x": 0, "y": 0, "w": 4, "h": 4}, "duration": 100}}"#);
        assert!(aseprite(&hash, None, sheet).unwrap_err().contains("json-hash"));
        let untagged = export(ARRAY).replace("\"frameTags\": [", "\"frameTags\": [], \"x\": [");
        assert!(aseprite(&untagged, None, sheet).unwrap_err().contains("no tags"));
        let unknown = export(ARRAY).replace("eyes:open", "tail:wag");
        assert!(aseprite(&unknown, None, sheet).unwrap_err().contains("isn't a channel"));
    }

    #[test]
    fn check_pack_says_ok_or_why() {
        let d = dir("check");
        let good = d.join("good");
        hood_pack(&good);
        let (line, ok) = check(&good);
        assert!(ok && line.starts_with("OK Hood (pixel, 13 layers, "), "{line}");
        fs::remove_file(good.join("eyes.png")).unwrap();
        let (line, ok) = check(&good);
        assert!(!ok && line.contains("eyes.png") && line.contains("missing"), "{line}");
        assert!(!check(&d.join("nowhere")).1);
    }
}
