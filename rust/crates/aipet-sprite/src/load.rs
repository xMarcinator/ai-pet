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
    // recolour files go last, so a pet in the folder can be a base
    let mut recolours = Vec::new();
    for path in paths {
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let loaded = if path.is_dir() {
            load_folder(&path)
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
            if let Some(recolour) = recolour_file(&path) {
                recolours.push((file, recolour));
                continue;
            }
            load_legacy(&path)
        } else {
            continue;
        };
        keep(&mut found, file, loaded);
    }
    // a recolour of a recolour waits for its base: each pass takes those whose base is there
    let mut waiting = recolours;
    loop {
        let before = waiting.len();
        let mut later = Vec::new();
        for (file, recolour) in waiting {
            match recolour {
                Ok(r) if !found.pets.iter().any(|p| p.name() == r.base) => later.push((file, Ok(r))),
                recolour => {
                    let loaded = recolour.and_then(|r| recoloured(&found.pets, r));
                    keep(&mut found, file, loaded);
                }
            }
        }
        if later.is_empty() || later.len() == before {
            for (file, recolour) in later {
                let loaded = recolour.and_then(|r| recoloured(&found.pets, r));
                keep(&mut found, file, loaded);
            }
            break;
        }
        waiting = later;
    }
    found
}

/// A loaded pet joins the list unless one by its name is there; what didn't load is skipped with why.
fn keep(found: &mut Found, file: String, loaded: Result<LoadedPack, String>) {
    match loaded {
        Ok(pet) if found.pets.iter().any(|p| p.name() == pet.name()) => found.skipped.push(Skipped {
            file,
            reason: format!("a pet named {:?} is already loaded", pet.name()),
        }),
        Ok(pet) => found.pets.push(pet),
        Err(reason) => found.skipped.push(Skipped { file, reason }),
    }
}

/// A recolour file: a vector pet by name in other colours, under a name of its own.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recolour {
    pub base: String,
    pub name: String,
    pub palette: BTreeMap<String, String>,
}

/// `path` read as a recolour file, if it is one (a JSON object with a `base`); `None` for anything else.
fn recolour_file(path: &Path) -> Option<Result<Recolour, String>> {
    if fs::metadata(path).ok()?.len() > MAX_MANIFEST {
        return None;
    }
    let text = read_all_text(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("base")?;
    Some(serde_json::from_value(value).map_err(|e| format!("the recolour file isn't valid: {e}")))
}

/// The pet `r` recolours, from `pets`, in its new colours and name.
fn recoloured(pets: &[LoadedPack], r: Recolour) -> Result<LoadedPack, String> {
    let base = pets
        .iter()
        .find(|p| p.name() == r.base)
        .ok_or_else(|| format!("there is no pet named {:?} to recolour", r.base))?;
    if base.pack.kind != Kind::Vector {
        return Err(format!(
            "{} isn't a vector pet: only vector pets can be recoloured",
            r.base
        ));
    }
    if r.name.trim().is_empty() {
        return Err("the recolour has no name".to_owned());
    }
    let mut colours = BTreeMap::new();
    for (name, value) in &r.palette {
        let c = crate::vector::colour(value)
            .map_err(|e| format!("colour {name:?}: {e}"))?
            .ok_or_else(|| format!("colour {name:?} is none"))?;
        colours.insert(name.clone(), c);
    }
    let mut pet = base.clone();
    pet.pack.name = r.name;
    pet.pack.recolour(&colours)?;
    pet.pack.validate()?;
    Ok(pet)
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
    let native = if export.is_none() { find_native(folder)? } else { None };
    let (pack, sheets) = match export {
        Some(export) => {
            let extra = if manifest.exists() {
                Some(read_json(&inside(folder, "pack.json")?)?)
            } else {
                None
            };
            aseprite(&read_json(&export)?, extra.as_deref(), |name| sheets.read(name))?
        }
        None if native.is_some() => {
            let native = native.expect("just checked");
            let file = name_of(&native);
            let path = inside(folder, &file)?;
            let len = fs::metadata(&path).map_err(|e| e.to_string())?.len();
            if len > MAX_PNG {
                return Err(format!("{file} is {len} bytes (at most {MAX_PNG})"));
            }
            let bytes = fs::read(&path).map_err(|e| format!("{file}: {e}"))?;
            let extra = if manifest.exists() {
                Some(read_json(&inside(folder, "pack.json")?)?)
            } else {
                None
            };
            aseprite_native(&bytes, &file, extra.as_deref())?
        }
        None if manifest.exists() => {
            let text = read_json(&inside(folder, "pack.json")?)?;
            manifest::read(&text, |name| sheets.read(name))?
        }
        None => return Err("no pack.json, Aseprite export or .aseprite file".to_owned()),
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

/// The folder's native Aseprite file (`.aseprite` or `.ase`), the first by name.
fn find_native(folder: &Path) -> Result<Option<PathBuf>, String> {
    let mut entries: Vec<PathBuf> = fs::read_dir(folder)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("aseprite") || e.eq_ignore_ascii_case("ase"))
        })
        .collect();
    entries.sort();
    Ok(entries.into_iter().next())
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

/// A frame of an Aseprite source: its rectangle on the sheet, the art pixel it starts at, and its duration.
#[derive(Clone, Copy, Debug)]
struct AseFrame {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    /// Where the frame's top-left is in the sprite (a trimmed or cropped frame starts inside it).
    at: (i64, i64),
    ms: u32,
}

/// A tag: `<layer>:<value>`, the frames it covers, its direction, and its repeat count (none: forever).
struct AseTag {
    name: String,
    from: usize,
    to: usize,
    direction: &'static str,
    repeat: Option<usize>,
}

/// A slice's points by key frame: where its pivot is in the sprite.
type SlicePoints = Vec<(String, Vec<(usize, (f64, f64))>)>;
/// The frame a source gives for a layer at an index.
type FrameOf = Box<dyn Fn(&str, usize) -> Result<AseFrame, String>>;

/// What an Aseprite source gives a pack: a sheet, the frame a layer shows at an index, tags and slice points.
struct AseSource {
    sheet: Sheet,
    image: String,
    frames: usize,
    tags: Vec<AseTag>,
    /// A slice's points (key frame, then the key's pivot in the sprite), by name.
    slices: SlicePoints,
    frame: FrameOf,
}

impl AseSource {
    /// A slice's point at `frame`: its last key at or before it, else its first.
    fn slice_at(&self, name: &str, frame: usize) -> Option<(f64, f64)> {
        let (_, keys) = self.slices.iter().find(|(n, _)| n == name)?;
        keys.iter()
            .rev()
            .find(|(f, _)| *f <= frame)
            .or(keys.first())
            .map(|(_, p)| *p)
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
        Value::Array(f) => f.clone(),
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
    let mut list = Vec::with_capacity(frames.len());
    for (i, f) in frames.iter().enumerate() {
        let (x, y, w, h) = rect(&f["frame"]).ok_or_else(|| format!("frame {i} has no rectangle"))?;
        let (sx, sy) = rect(&f["spriteSourceSize"]).map_or((0, 0), |r| (r.0, r.1));
        if x < 0 || y < 0 || w <= 0 || h <= 0 || x + w > i64::from(sheet.width) || y + h > i64::from(sheet.height) {
            return Err(format!("frame {i} is outside {image}"));
        }
        let ms = f["duration"].as_u64().unwrap_or(100).min(u64::from(u32::MAX)) as u32;
        list.push(AseFrame {
            x: x as u32,
            y: y as u32,
            w: w as u32,
            h: h as u32,
            at: (sx, sy),
            ms,
        });
    }
    let mut tags = Vec::new();
    for tag in meta["frameTags"].as_array().cloned().unwrap_or_default() {
        let name = tag["name"].as_str().unwrap_or_default().to_owned();
        let (from, to) = (tag["from"].as_i64().unwrap_or(0), tag["to"].as_i64().unwrap_or(-1));
        if from < 0 || to < from || to as usize >= list.len() {
            return Err(format!("tag {name:?} covers frames {from} to {to}, of {}", list.len()));
        }
        let direction = match tag["direction"].as_str().unwrap_or("forward") {
            "reverse" => "reverse",
            "pingpong" => "pingpong",
            "pingpong_reverse" => "pingpong_reverse",
            _ => "forward",
        };
        // Aseprite writes repeat as a string, and leaves it out for forever
        let repeat = tag["repeat"]
            .as_str()
            .and_then(|r| r.parse::<usize>().ok())
            .filter(|&n| n > 0);
        tags.push(AseTag {
            name,
            from: from as usize,
            to: to as usize,
            direction,
            repeat,
        });
    }
    let slices = meta["slices"]
        .as_array()
        .map(|ss| {
            ss.iter()
                .filter_map(|s| {
                    let keys = s["keys"]
                        .as_array()?
                        .iter()
                        .filter_map(|k| {
                            let (x, y, _, _) = rect(&k["bounds"])?;
                            let (px, py) = (
                                k["pivot"]["x"].as_i64().unwrap_or(0),
                                k["pivot"]["y"].as_i64().unwrap_or(0),
                            );
                            Some((
                                k["frame"].as_u64().unwrap_or(0) as usize,
                                ((x + px) as f64, (y + py) as f64),
                            ))
                        })
                        .collect();
                    Some((s["name"].as_str()?.to_owned(), keys))
                })
                .collect()
        })
        .unwrap_or_default();
    let source = AseSource {
        sheet,
        image,
        frames: list.len(),
        tags,
        slices,
        frame: Box::new(move |_, i| Ok(list[i])),
    };
    build_aseprite(source, extra)
}

/// The most frames a native file may have, and layers (groups included).
const MAX_ASE_FRAMES: u32 = 512;
const MAX_ASE_LAYERS: usize = 64;

/// A native `.aseprite` file (and an optional `pack.json`) as a pack, by the export's mapping: a tag
/// `<layer>:<value>` takes the Aseprite layer of that name, each of its frames cropped to the opaque pixels.
pub fn aseprite_native(bytes: &[u8], file: &str, extra: Option<&str>) -> Result<(Pack, Vec<Sheet>), String> {
    let limits = crate::aseprite_file::Limits {
        max_file: MAX_PNG as usize,
        max_side: MAX_SIDE,
        max_frames: MAX_ASE_FRAMES,
        max_layers: MAX_ASE_LAYERS,
        max_decoded: MAX_DECODED as usize,
    };
    let f = crate::aseprite_file::read(bytes, &limits).map_err(|e| format!("{file}: {e}"))?;
    let directions = ["forward", "reverse", "pingpong", "pingpong_reverse"];
    let tags: Vec<AseTag> = f
        .tags
        .iter()
        .map(|t| AseTag {
            name: t.name.clone(),
            from: usize::from(t.from),
            to: usize::from(t.to),
            direction: directions[usize::from(t.direction.min(3))],
            repeat: (t.repeat > 0).then_some(usize::from(t.repeat)),
        })
        .collect();
    // every (layer, frame) a tag uses, cropped to its opaque box, on one sheet
    // (layer, frame index, cropped pixels, size, where it starts in the sprite, ms)
    type Crop = (String, usize, Vec<u32>, (u32, u32), (i64, i64), u32);
    let mut crops: Vec<Crop> = Vec::new();
    for t in &tags {
        let Some((layer, _)) = t.name.split_once(':') else {
            continue;
        };
        let li = f.layers.iter().position(|l| !l.group && l.name == layer);
        for i in t.from..=t.to {
            if crops.iter().any(|c| c.0 == layer && c.1 == i) {
                continue;
            }
            let px = li.and_then(|li| f.layer_pixels(i, li)).unwrap_or_default();
            let (w, h) = (f.width as usize, f.height as usize);
            let solid = |x: usize, y: usize| px.get(y * w + x).is_some_and(|&p| p >> 24 != 0);
            let xs: Vec<usize> = (0..w).filter(|&x| (0..h).any(|y| solid(x, y))).collect();
            let ys: Vec<usize> = (0..h).filter(|&y| (0..w).any(|x| solid(x, y))).collect();
            let (crop, size, at) = match (xs.first(), xs.last(), ys.first(), ys.last()) {
                (Some(&x0), Some(&x1), Some(&y0), Some(&y1)) => {
                    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
                    let mut c = Vec::with_capacity(cw * ch);
                    for y in y0..=y1 {
                        c.extend_from_slice(&px[y * w + x0..=y * w + x1]);
                    }
                    (c, (cw as u32, ch as u32), (x0 as i64, y0 as i64))
                }
                // nothing drawn there: one clear pixel
                _ => (vec![0], (1, 1), (0, 0)),
            };
            crops.push((layer.to_owned(), i, crop, size, at, f.durations[i]));
        }
    }
    // the crops in rows, at most 2048 px wide
    let (mut x, mut y, mut row_h, mut width) = (0u32, 0u32, 0u32, 1u32);
    let mut placed = Vec::with_capacity(crops.len());
    for (_, _, _, (w, h), _, _) in &crops {
        if x + w > MAX_SIDE {
            (x, y, row_h) = (0, y + row_h, 0);
        }
        placed.push((x, y));
        (x, row_h) = (x + w, row_h.max(*h));
        width = width.max(x);
    }
    let height = (y + row_h).max(1);
    if height > MAX_SIDE {
        return Err(format!("{file}: its frames don't fit a {MAX_SIDE} px sheet"));
    }
    let mut pixels = vec![0u32; (width * height) as usize];
    let mut index = std::collections::HashMap::new();
    for ((layer, i, crop, (w, h), at, ms), (px, py)) in crops.into_iter().zip(placed) {
        for row in 0..h {
            let to = ((py + row) * width + px) as usize;
            pixels[to..to + w as usize].copy_from_slice(&crop[(row * w) as usize..((row + 1) * w) as usize]);
        }
        index.insert(
            (layer, i),
            AseFrame {
                x: px,
                y: py,
                w,
                h,
                at,
                ms,
            },
        );
    }
    let slices = f
        .slices
        .iter()
        .map(|s| {
            let keys = s
                .keys
                .iter()
                .map(|k| {
                    let (px, py) = k.pivot.unwrap_or((0, 0));
                    (k.frame as usize, (f64::from(k.x + px), f64::from(k.y + py)))
                })
                .collect();
            (s.name.clone(), keys)
        })
        .collect();
    let file_name = file.to_owned();
    let source = AseSource {
        sheet: Sheet { width, height, pixels },
        image: file.to_owned(),
        frames: f.durations.len(),
        tags,
        slices,
        frame: Box::new(move |layer, i| {
            index
                .get(&(layer.to_owned(), i))
                .copied()
                .ok_or_else(|| format!("{file_name}: layer {layer:?} has no frame {i}"))
        }),
    };
    build_aseprite(source, extra)
}

/// A pack from an Aseprite source and an optional `pack.json` (as text) for the rest.
fn build_aseprite(src: AseSource, extra: Option<&str>) -> Result<(Pack, Vec<Sheet>), String> {
    let extra: Value = match extra {
        Some(t) => serde_json::from_str(t).map_err(|e| format!("pack.json isn't valid: {e}"))?,
        None => Value::Null,
    };
    if src.tags.is_empty() {
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
    let pixel_size = extra["pixel_size"].as_f64().unwrap_or(1.0);
    let mut layers: Vec<Layer> = Vec::new();
    for tag in &src.tags {
        let name = tag.name.as_str();
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
        if tag.to >= src.frames {
            return Err(format!(
                "tag {name:?} covers frames {} to {}, of {}",
                tag.from, tag.to, src.frames
            ));
        }
        let mut order: Vec<usize> = (tag.from..=tag.to).collect();
        if tag.direction.ends_with("reverse") {
            order.reverse();
        }
        let mut mode = if tag.direction.starts_with("pingpong") {
            LoopMode::PingPong
        } else {
            LoopMode::Loop
        };
        // a repeat count plays the frames that many times (a ping-pong each way), then holds the last
        if let Some(n) = tag.repeat {
            let once: Vec<usize> = if mode == LoopMode::PingPong && order.len() > 1 {
                order.iter().chain(order.iter().rev().skip(1)).copied().collect()
            } else {
                order.clone()
            };
            order = (0..n).flat_map(|_| once.iter().copied()).collect();
            mode = LoopMode::Once;
        }
        let mut list = Vec::new();
        for i in order {
            let f = (src.frame)(layer_name, i)?;
            list.push(Frame {
                art: Art::Raster {
                    sheet: 0,
                    x: f.x,
                    y: f.y,
                    w: f.w,
                    h: f.h,
                    // the frame's pixels start this far into the sprite
                    pivot: (-f.at.0 as f64, -f.at.1 as f64),
                },
                ms: f.ms,
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
            let Some(origin) = src.slice_at(&id, 0) else {
                return Err(format!("layer {layer_name:?}: part {id:?} has no slice"));
            };
            if !layer.parts.iter().any(|p| p.id == id) {
                layer.parts.push(Part {
                    id: id.clone(),
                    origin: (origin.0 * pixel_size, origin.1 * pixel_size),
                    shape: None,
                });
            }
            if let Some(at) = src.slice_at(&id, tag.from) {
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
    let anchor = src
        .slice_at("anchor", 0)
        .or_else(|| {
            let a = extra["anchor"].as_array()?;
            Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?))
        })
        .unwrap_or((0.0, 0.0));
    let name = extra["name"].as_str().map(str::to_owned).unwrap_or_else(|| {
        Path::new(&src.image)
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
        palette: Default::default(),
    };
    pack.validate()?;
    Ok((pack, vec![src.sheet]))
}

/// `AiPet --check-pack <folder>`: the line to print and whether the pack loads.
pub fn check(folder: &Path) -> (String, bool) {
    // a recolour file, checked against the built-ins and the pets beside it
    if folder.is_file()
        && let Some(recolour) = recolour_file(folder)
    {
        let found = all(folder.parent().unwrap_or(Path::new(".")));
        let loaded = recolour.and_then(|r| recoloured(&found.pets, r));
        return match loaded {
            Ok(p) => (format!("OK {} (a recolour of a vector pet)", p.pack.name), true),
            Err(reason) => (reason, false),
        };
    }
    if !folder.is_dir() {
        return (format!("{}: not a folder or a recolour file", folder.display()), false);
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
        assert_eq!(
            names,
            [
                "Sprout",
                "Hood",
                "Hood (green)",
                "Pip",
                "Pip (painted)",
                "Hood (green, vector)",
                "Hamster",
                "Sprout (vector)",
                "Hamster (pixel)",
                "Other"
            ]
        );
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

    use crate::aseprite_file::writer::{
        Cel, CelData, Depth, Frame as WFrame, Layer as WLayer, Slice, SliceKey, Sprite, Tag, write,
    };

    /// A two-layer, three-frame sprite: a body that bobs, eyes open then closed, an anchor slice.
    fn sprite() -> Sprite {
        let disc = |lift: i32| -> Vec<u8> {
            (0..26 * 24)
                .flat_map(|i| {
                    let (x, y) = (i % 26, i / 26);
                    let (dx, dy) = (x as f64 + 0.5 - 13.0, y as f64 + 0.5 - (14.0 - f64::from(lift)));
                    if dx * dx / 64.0 + dy * dy / 56.25 <= 1.0 {
                        [0xB8, 0x8C, 0xF0, 0xFF]
                    } else {
                        [0; 4]
                    }
                })
                .collect()
        };
        let eyes = |rows: &[usize]| -> Vec<u8> {
            (0..26 * 24)
                .flat_map(|i| {
                    if [9, 10, 15, 16].contains(&(i % 26)) && rows.contains(&(i / 26)) {
                        [0xFF, 0xF2, 0xA8, 0xFF]
                    } else {
                        [0; 4]
                    }
                })
                .collect()
        };
        let cel = |layer: u16, data: Vec<u8>| Cel {
            layer,
            x: 0,
            y: 0,
            w: 26,
            h: 24,
            data: CelData::Zlib(data),
        };
        Sprite {
            width: 26,
            height: 24,
            depth: Depth::Rgba,
            palette: vec![],
            layers: vec![
                WLayer {
                    name: "body".into(),
                    group: false,
                    child_level: 0,
                    visible: true,
                },
                WLayer {
                    name: "eyes".into(),
                    group: false,
                    child_level: 0,
                    visible: true,
                },
            ],
            frames: vec![
                WFrame {
                    ms: 400,
                    cels: vec![cel(0, disc(0)), cel(1, eyes(&[11, 12]))],
                },
                WFrame {
                    ms: 400,
                    cels: vec![
                        cel(0, disc(1)),
                        Cel {
                            layer: 1,
                            x: 0,
                            y: 0,
                            w: 0,
                            h: 0,
                            data: CelData::Linked(0),
                        },
                    ],
                },
                WFrame {
                    ms: 1000,
                    cels: vec![
                        Cel {
                            layer: 0,
                            x: 0,
                            y: 0,
                            w: 0,
                            h: 0,
                            data: CelData::Linked(0),
                        },
                        cel(1, eyes(&[13])),
                    ],
                },
            ],
            tags: vec![
                Tag {
                    name: "body:idle".into(),
                    from: 0,
                    to: 1,
                    direction: 0,
                    repeat: 0,
                },
                Tag {
                    name: "eyes:open".into(),
                    from: 0,
                    to: 0,
                    direction: 0,
                    repeat: 0,
                },
                Tag {
                    name: "eyes:closed".into(),
                    from: 2,
                    to: 2,
                    direction: 0,
                    repeat: 0,
                },
            ],
            slices: vec![Slice {
                name: "anchor".into(),
                keys: vec![SliceKey {
                    frame: 0,
                    x: 12,
                    y: 21,
                    w: 2,
                    h: 2,
                    pivot: Some((1, 1)),
                }],
            }],
            layer_uuids: false,
        }
    }

    /// Every frame of every layer drawn on the 26 × 24 art box, by its pivot: what a pack shows, however it is stored.
    fn drawn(pack: &Pack, sheets: &[Sheet]) -> Vec<(String, String, Vec<u32>)> {
        let mut out = Vec::new();
        for layer in &pack.layers {
            for (value, a) in &layer.animations {
                for f in &a.frames {
                    let Art::Raster {
                        sheet,
                        x,
                        y,
                        w,
                        h,
                        pivot,
                    } = f.art
                    else {
                        continue;
                    };
                    let s = &sheets[sheet];
                    let mut canvas = vec![0u32; 26 * 24];
                    for fy in 0..h {
                        for fx in 0..w {
                            let (cx, cy) = (fx as i64 - pivot.0 as i64, fy as i64 - pivot.1 as i64);
                            let px = s.pixels[((y + fy) * s.width + x + fx) as usize];
                            if px != 0 && (0..26).contains(&cx) && (0..24).contains(&cy) {
                                canvas[(cy * 26 + cx) as usize] = px;
                            }
                        }
                    }
                    out.push((layer.name.clone(), value.clone(), canvas));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn a_native_file_and_its_json_array_export_give_the_same_pack() {
        let native = write(&sprite());
        let (pack, sheets) = aseprite_native(&native, "pet.aseprite", None).unwrap();
        assert_eq!(pack.anchor, (13.0, 22.0));
        assert_eq!(
            pack.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
            ["body", "eyes"]
        );
        // the same art exported: untrimmed frames side by side, the frames' order as Aseprite numbers them
        let f = crate::aseprite_file::read(
            &native,
            &crate::aseprite_file::Limits {
                max_file: 1 << 20,
                max_side: 2048,
                max_frames: 64,
                max_layers: 8,
                max_decoded: 1 << 24,
            },
        )
        .unwrap();
        let mut sheet = Sheet {
            width: 26 * 6,
            height: 24,
            pixels: vec![0; 26 * 6 * 24],
        };
        let mut frames = Vec::new();
        for (n, (layer, frame)) in [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2)].into_iter().enumerate() {
            let px = f.layer_pixels(frame, layer).unwrap();
            for y in 0..24 {
                for x in 0..26 {
                    sheet.pixels[y * 26 * 6 + n * 26 + x] = px[y * 26 + x];
                }
            }
            frames.push(serde_json::json!({"frame": {"x": n * 26, "y": 0, "w": 26, "h": 24},
                "spriteSourceSize": {"x": 0, "y": 0, "w": 26, "h": 24}, "duration": f.durations[frame]}));
        }
        // tags point into this export's frames: the body's are 0–2, the eyes' 3–5
        let export = serde_json::json!({"frames": frames, "meta": {"image": "pet.png",
            "frameTags": [{"name": "body:idle", "from": 0, "to": 1, "direction": "forward"},
                          {"name": "eyes:open", "from": 3, "to": 3, "direction": "forward"},
                          {"name": "eyes:closed", "from": 5, "to": 5, "direction": "forward"}],
            "slices": [{"name": "anchor", "keys": [{"frame": 0, "bounds": {"x": 12, "y": 21, "w": 2, "h": 2}, "pivot": {"x": 1, "y": 1}}]}]}});
        let (exported, export_sheets) = aseprite(&export.to_string(), None, |_| Ok(sheet.clone())).unwrap();
        assert_eq!(drawn(&pack, &sheets), drawn(&exported, &export_sheets));
        assert_eq!(
            pack.layers[0].animations["idle"]
                .frames
                .iter()
                .map(|f| f.ms)
                .collect::<Vec<_>>(),
            exported.layers[0].animations["idle"]
                .frames
                .iter()
                .map(|f| f.ms)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn hostile_native_files_are_refused_before_any_large_allocation() {
        let mut bomb = sprite();
        bomb.frames[0].cels[0] = Cel {
            layer: 0,
            x: 0,
            y: 0,
            w: 16,
            h: 16,
            data: CelData::Zlib(vec![0; 10_000_000]),
        };
        let err = aseprite_native(&write(&bomb), "bomb.aseprite", None).unwrap_err();
        assert!(err.contains("cel data is"), "{err}");
        let mut huge = sprite();
        huge.frames[0].cels[0] = Cel {
            layer: 0,
            x: 0,
            y: 0,
            w: 65535,
            h: 65535,
            data: CelData::Raw(vec![]),
        };
        let err = aseprite_native(&write(&huge), "huge.aseprite", None).unwrap_err();
        assert!(err.contains("byte limit"), "{err}");
        let mut wide = sprite();
        wide.width = 4096;
        let err = aseprite_native(&write(&wide), "wide.aseprite", None).unwrap_err();
        assert!(err.contains("outside the limits"), "{err}");
    }

    #[test]
    fn the_editors_own_files_read_and_a_tilemap_is_refused() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asefile");
        let limits = crate::aseprite_file::Limits {
            max_file: MAX_PNG as usize,
            max_side: MAX_SIDE,
            max_frames: 512,
            max_layers: 64,
            max_decoded: MAX_DECODED as usize,
        };
        for name in [
            "indexed",
            "grayscale",
            "layers_and_tags",
            "linked_cels",
            "256_color_old_palette_chunk",
        ] {
            let bytes = fs::read(dir.join(format!("{name}.aseprite"))).unwrap();
            let f = crate::aseprite_file::read(&bytes, &limits).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(!f.durations.is_empty() && !f.layers.is_empty(), "{name}");
        }
        let err = crate::aseprite_file::read(&fs::read(dir.join("tilemap.aseprite")).unwrap(), &limits)
            .err()
            .unwrap();
        assert!(err.contains("tilemap") || err.contains("tileset"), "{err}");
    }

    /// A pack with one body layer on `sheet`, whose idle animation is `idle` (a manifest animation object).
    fn grid_pack(idle: &str) -> Result<Pack, String> {
        let text = format!(
            r##"{{"format_version": 1, "name": "Grid", "kind": "pixel", "pixel_size": 5, "anchor": [13, 22],
            "accent": "#B88CF0", "glow": "#FFF2A8",
            "layers": [{{"name": "body", "channel": "state", "default": "idle", "animations": {{"idle": {idle}}}}}]}}"##
        );
        let sheet = Sheet {
            width: 4 * 26 + 3 * 2,
            height: 2 * 24 + 1,
            pixels: vec![0xFF00_0000; (4 * 26 + 6) * 49],
        };
        manifest::read(&text, |_| Ok(sheet.clone())).map(|(p, _)| p)
    }

    #[test]
    fn a_grid_animation_is_the_same_frames_given_one_by_one() {
        let grid = grid_pack(
            r#"{"sheet": "body.png", "grid": {"frame": [26, 24], "origin": [0, 0], "gap": [2, 1], "columns": 4},
            "frames": [0, 5, 3], "ms": [100, 120, 140], "loop": "loop"}"#,
        )
        .unwrap();
        let explicit = grid_pack(r#"{"sheet": "body.png", "frames": [[0, 0, 26, 24, 100], [28, 25, 26, 24, 120], [84, 0, 26, 24, 140]], "loop": "loop"}"#).unwrap();
        assert_eq!(grid, explicit);
        let one_ms = grid_pack(
            r#"{"sheet": "body.png", "grid": {"frame": [26, 24], "columns": 4}, "frames": [1, 2], "ms": 90}"#,
        )
        .unwrap();
        assert!(one_ms.layers[0].animations["idle"].frames.iter().all(|f| f.ms == 90));
    }

    #[test]
    fn each_grid_error_gives_its_reason() {
        let cases = [
            (
                r#"{"frame": [26, 24], "columns": 4}, "frames": [8], "ms": 90"#,
                "cell 8 is outside",
            ),
            (
                r#"{"frame": [0, 24], "columns": 4}, "frames": [0], "ms": 90"#,
                "frame is 0 × 24",
            ),
            (
                r#"{"frame": [26, 24], "columns": 0}, "frames": [0], "ms": 90"#,
                "0 columns",
            ),
            (
                r#"{"frame": [26, 24], "columns": 4}, "frames": [0, 1], "ms": [90]"#,
                "1 durations for 2 frames",
            ),
            (
                r#"{"frame": [26, 24], "columns": 4}, "frames": [-1], "ms": 90"#,
                "isn't a cell",
            ),
            (
                r#"{"frame": [26, 24], "origin": [-1, 0], "columns": 4}, "frames": [0], "ms": 90"#,
                "negative",
            ),
        ];
        for (grid, why) in cases {
            let err = grid_pack(&format!(r#"{{"sheet": "body.png", "grid": {grid}}}"#)).unwrap_err();
            assert!(err.contains(why), "{why}: {err}");
        }
    }

    /// The samples in the repo's `avatars/samples/`: `native` is [`sprite`] as an `.aseprite` file with a `pack.json`
    /// for the rest, and `grid` is the basic sample's sheets as grids. With `AIPET_BAKE=1` this writes them.
    #[test]
    fn the_native_and_grid_samples_load() {
        let samples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../avatars/samples");
        let extra = serde_json::json!({
            "name": "Sample (Aseprite)", "pixel_size": 5, "accent": "#B88CF0", "glow": "#FFF2A8",
            "layers": [
                {"name": "body", "channel": "state", "hit": true, "default": "idle"},
                {"name": "eyes", "channel": "eyes", "glow": true, "default": "open",
                 "fallback": {"blink": "closed", "happy": "closed"}}
            ]
        });
        let grid = serde_json::json!({
            "format_version": 1, "name": "Sample (grid)", "kind": "pixel", "pixel_size": 5, "anchor": [13, 22],
            "accent": "#B88CF0", "glow": "#FFF2A8",
            "layers": [
                {"name": "body", "channel": "state", "hit": true, "default": "idle", "fallback": {"listen": "sleep"},
                 "animations": {
                     "idle": {"sheet": "body.png", "grid": {"frame": [26, 24], "columns": 3}, "frames": [0, 1], "ms": 400, "loop": "loop"},
                     "sleep": {"sheet": "body.png", "grid": {"frame": [26, 24], "columns": 3}, "frames": [2], "ms": 1000, "loop": "once"}}},
                {"name": "eyes", "channel": "eyes", "glow": true, "default": "open", "fallback": {"blink": "closed", "happy": "closed"},
                 "look": {"left": [-5, 0], "right": [5, 0], "far-left": [-10, 0], "far-right": [10, 0]},
                 "animations": {
                     "open": {"sheet": "eyes.png", "grid": {"frame": [26, 24], "columns": 2}, "frames": [0], "ms": 1000, "loop": "once"},
                     "closed": {"sheet": "eyes.png", "grid": {"frame": [26, 24], "columns": 2}, "frames": [1], "ms": 1000, "loop": "once"}}}
            ]
        });
        if std::env::var_os("AIPET_BAKE").is_some_and(|v| v == "1") {
            let native = samples.join("native");
            let _ = fs::remove_dir_all(&native);
            fs::create_dir_all(&native).unwrap();
            fs::write(native.join("sample.aseprite"), write(&sprite())).unwrap();
            fs::write(
                native.join("pack.json"),
                serde_json::to_string_pretty(&extra).unwrap() + "\n",
            )
            .unwrap();
            let g = samples.join("grid");
            let _ = fs::remove_dir_all(&g);
            fs::create_dir_all(&g).unwrap();
            for sheet in ["body.png", "eyes.png"] {
                fs::copy(samples.join("basic").join(sheet), g.join(sheet)).unwrap();
            }
            fs::write(g.join("pack.json"), serde_json::to_string_pretty(&grid).unwrap() + "\n").unwrap();
        }
        let native = load_folder(&samples.join("native")).unwrap();
        assert_eq!(native.name(), "Sample (Aseprite)");
        let grid = load_folder(&samples.join("grid")).unwrap();
        let basic = load_folder(&samples.join("basic")).unwrap();
        // the grid sample is the basic one, its frames given as cells
        assert_eq!(drawn(&grid.pack, &grid.sheets), drawn(&basic.pack, &basic.sheets));
        for dir in ["basic", "native", "grid"] {
            assert!(check(&samples.join(dir)).1, "{dir}");
        }
    }

    #[test]
    fn a_recolour_file_is_a_vector_pet_in_other_colours_under_its_own_name() {
        let d = dir("recolour");
        let write = |file: &str, text: &str| fs::write(d.join(file), text).unwrap();
        write(
            "hood-red.json",
            r##"{"base": "Hood (green, vector)", "name": "Hood (red)", "palette": {"body": "#4A1010", "hi": "#FF5A3C"}}"##,
        );
        write(
            "bad-name.json",
            r##"{"base": "Pip", "name": "Pip (blue)", "palette": {"fur": "#3050FF"}}"##,
        );
        write("bad-base.json", r##"{"base": "Nobody", "name": "X", "palette": {}}"##);
        write(
            "pixel.json",
            r##"{"base": "Sprout", "name": "Sprout (blue)", "palette": {}}"##,
        );
        write(
            "url.json",
            r##"{"base": "Pip", "name": "Pip (url)", "palette": {"body": "url(http://x/y)"}}"##,
        );
        let found = all(&d);
        let green = found.pets.iter().find(|p| p.name() == "Hood (green, vector)").unwrap();
        let red = found
            .pets
            .iter()
            .find(|p| p.name() == "Hood (red)")
            .expect("the recolour loads");
        // the named colours changed everywhere they're used, the accent with them; nothing else did
        assert_eq!(red.pack.palette.colours["body"], 0xFF4A1010);
        assert_eq!(red.pack.accent, 0xFFFF5A3C);
        let fills = |p: &LoadedPack, name: &str| -> Vec<Option<u32>> {
            p.pack
                .layers
                .iter()
                .flat_map(|l| l.shapes.values())
                .flatten()
                .filter(|s| s.fill_name.as_deref() == Some(name))
                .map(|s| s.fill)
                .collect()
        };
        assert!(!fills(red, "body").is_empty());
        assert!(fills(red, "body").iter().all(|&f| f == Some(0xFF4A1010)));
        assert_eq!(fills(red, "eye"), fills(green, "eye"));
        let reason = |file: &str| {
            found
                .skipped
                .iter()
                .find(|s| s.file == file)
                .map(|s| s.reason.clone())
                .unwrap_or_else(|| panic!("{file} wasn't skipped"))
        };
        assert!(
            reason("bad-name.json").contains("no colour named \"fur\""),
            "{}",
            reason("bad-name.json")
        );
        assert!(reason("bad-base.json").contains("no pet named \"Nobody\""));
        assert!(reason("pixel.json").contains("only vector pets"));
        assert!(reason("url.json").contains("colour"));
        // --check-pack takes the file too
        assert_eq!(
            check(&d.join("hood-red.json")),
            ("OK Hood (red) (a recolour of a vector pet)".into(), true)
        );
        assert!(!check(&d.join("bad-name.json")).1);
    }

    #[test]
    fn a_shape_names_a_palette_colour_the_pack_has() {
        let pack = |palette: &str, fill: &str| {
            format!(
                r##"{{"format_version": 1, "name": "P", "kind": "vector", "pixel_size": 1, "anchor": [65, 110],
                "accent": "$fur", "glow": "#FFE08A", "palette": {palette}, "layers": [
                  {{"name": "body", "channel": "state", "default": "idle",
                    "shapes": {{"idle": {{"d": "M0 0 L 1 1", "fill": "{fill}"}}}}}}]}}"##
            )
        };
        let read = |text: String| manifest::read(&text, |_| Err("no sheets".into())).map(|(p, _)| p);
        let p = read(pack(r##"{"fur": "#3050FF"}"##, "$fur")).unwrap();
        assert_eq!(p.accent, 0xFF3050FF);
        assert_eq!(p.palette.accent.as_deref(), Some("fur"));
        assert_eq!(p.layers[0].shapes["idle"][0].fill, Some(0xFF3050FF));
        assert!(
            read(pack(r##"{"fur": "#3050FF"}"##, "$skin"))
                .unwrap_err()
                .contains("no palette colour named \"skin\"")
        );
        assert!(
            read(pack(r##"{"fur": "none"}"##, "$fur"))
                .unwrap_err()
                .contains("palette colour \"fur\"")
        );
        assert!(read(pack(r##"{"fur": "url(x)"}"##, "$fur")).is_err());
    }
}
