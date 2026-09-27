//! Avatars: a port of `src/AiPet.Core/Avatar.cs`.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::{self, Deserialize, Deserializer, IgnoredAny, MapAccess, Unexpected, Visitor};
use serde_json::value::RawValue;

use crate::pet::GH;

/// `JsonSerializerOptions.MaxDepth` left at its default: System.Text.Json fails a file that nests arrays and objects
/// deeper than this, even inside a property it ignores.
const MAX_DEPTH: usize = 64;

/// A pet look: silhouette, palette, shading style and eye glow. All avatars share the 26×24 pixel grid
/// and the same anchor points (face at x 8–17 / y 9–14, eyes on rows 10–13, arms and feet at the sides
/// and bottom), so every animation works with every avatar.
///
/// Custom avatars: drop a JSON file in the avatars folder ([`Avatar::custom_dir`]) with the same fields as
/// the built-ins below (colours as "#RRGGBB" or "#AARRGGBB", shapes as [cx, cy, rx, ry] ellipses).
/// palette.accent is the avatar's theme colour (menu check marks, send button).
///
/// An `Avatar` is always resolved (its colours are set from the palette); a file is read into an [`AvatarJson`]
/// first and becomes an `Avatar` only when the pet can draw it ([`drawable`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Avatar {
    pub name: String,
    /// Silhouette as a union of ellipses on the pixel grid: [cx, cy, rx, ry].
    pub shapes: Vec<[f64; 4]>,
    /// "soft": highlight top-left, shade bottom-right (a lit toy). "rim": dark body with a glowing edge.
    /// `None` when a file sets it to null; the pet draws anything but "rim" as "soft".
    pub shading: Option<String>,
    pub blush: bool,
    /// Face panel rows (inclusive); columns are always 8–17.
    pub face_top: i32,
    pub face_bottom: i32,
    /// Colour per key ("outline", "eyeHi", ...; keys are case-sensitive). A key a file sets to null is left out,
    /// which resolves the same way (to the fallback).
    pub palette: HashMap<String, String>,

    // resolved colours (0xAARRGGBB)
    pub outline: u32,
    pub deep: u32,
    pub shade: u32,
    pub body: u32,
    pub hi: u32,
    pub spec: u32,
    pub blush_c: u32,
    pub screen: u32,
    pub bezel: u32,
    pub glint: u32,
    pub eye: u32,
    pub eye_hi: u32,
    pub eye_lo: u32,
    pub glow: u32,
    pub accent: u32,
}

impl Avatar {
    pub fn sprout() -> Avatar {
        Avatar::built_in_avatar(
            "Sprout",
            &[
                [13.0, 14.0, 7.3, 7.3],
                [8.6, 9.2, 4.0, 4.0],
                [13.0, 7.2, 4.6, 4.6],
                [17.4, 9.2, 4.0, 4.0],
            ],
            "soft",
            true,
            9,
            14,
            &[
                ("outline", "#3E1C12"),
                ("deep", "#A24A2C"),
                ("shade", "#C65F3B"),
                ("body", "#E47E55"),
                ("hi", "#F5A57D"),
                ("spec", "#FFD6BC"),
                ("blush", "#F08A7C"),
                ("screen", "#171A2C"),
                ("bezel", "#2A2F4D"),
                ("glint", "#3D4674"),
                ("eye", "#7FEFFF"),
                ("eyeHi", "#E4FFFF"),
                ("eyeLo", "#3FB8D8"),
                ("glow", "#7FEFFF"),
                ("accent", "#E27A52"),
            ],
        )
    }

    /// A hooded figure in the dark: rim-lit blue hood, an empty black face and two glowing eyes.
    pub fn hood() -> Avatar {
        Avatar::built_in_avatar(
            "Hood",
            &[
                [13.0, 10.6, 7.2, 7.4],  // hood
                [13.0, 4.4, 3.2, 3.0],   // hood peak
                [13.0, 19.6, 10.2, 4.3], // shoulders / cloak
            ],
            "rim",
            false,
            8,
            15,
            &[
                ("outline", "#04050B"),
                ("deep", "#0A1330"),
                ("shade", "#0F1C45"),
                ("body", "#16285E"),
                ("hi", "#3E74E8"),
                ("spec", "#8DBBFF"),
                ("blush", "#16285E"),
                ("screen", "#030409"),
                ("bezel", "#070A16"),
                ("glint", "#070A16"),
                ("eye", "#66C8FF"),
                ("eyeHi", "#E8F7FF"),
                ("eyeLo", "#2F8FF0"),
                ("glow", "#3D9BFF"),
                ("accent", "#4A8CFF"),
            ],
        )
    }

    /// Sprout and Hood.
    pub fn built_in() -> Vec<Avatar> {
        vec![Avatar::sprout(), Avatar::hood()]
    }

    /// The custom avatars folder in a data folder: `<data dir>/avatars`.
    pub fn custom_dir(data_dir: &Path) -> PathBuf {
        data_dir.join("avatars")
    }

    /// Built-ins plus any valid JSON avatars in the custom folder (`*.json`, in directory order).
    pub fn all(custom_dir: &Path) -> Vec<Avatar> {
        let mut list = Avatar::built_in();
        // Directory.GetFiles lists the folder in one go: a folder that can't be listed adds nothing
        let entries = fs::read_dir(custom_dir).and_then(|rd| rd.collect::<io::Result<Vec<_>>>());
        if let Ok(entries) = entries {
            for entry in entries {
                if !matches_json_pattern(&entry.file_name().to_string_lossy()) {
                    continue;
                }
                // skip broken files
                if let Some(a) = Avatar::load(&entry.path()) {
                    list.push(a);
                }
            }
        }
        list
    }

    /// One custom avatar file as [`Avatar::all`] reads it: `None` when the file can't be read or parsed, or the pet
    /// can't draw it. Without a name in the file, the avatar is named after the file.
    pub fn load(path: &Path) -> Option<Avatar> {
        let text = read_all_text(path).ok()?;
        let fallback = path
            .file_name()
            .map(|n| file_name_without_extension(&n.to_string_lossy()).to_owned());
        Avatar::parse(&text, &fallback.unwrap_or_default())
    }

    /// An avatar from JSON text (property names are case-insensitive), or `None` when the text isn't an avatar the
    /// pet can draw. `fallback_name` names it when the JSON has no name.
    pub fn parse(json: &str, fallback_name: &str) -> Option<Avatar> {
        Avatar::from_json(AvatarJson::parse(json)?, fallback_name)
    }

    /// A deserialized avatar, checked with [`drawable`] and resolved.
    pub fn from_json(j: AvatarJson, fallback_name: &str) -> Option<Avatar> {
        if !drawable(&j) {
            return None;
        }
        let shapes = j
            .shapes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|e| [e[0], e[1], e[2], e[3]])
            .collect();
        let palette = j
            .palette
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(k, v)| Some((k, v?)))
            .collect();
        let mut a = Avatar::unresolved(
            j.name.unwrap_or_else(|| fallback_name.to_owned()),
            shapes,
            j.shading,
            j.blush,
            j.face_top,
            j.face_bottom,
            palette,
        );
        a.resolve();
        Some(a)
    }

    fn built_in_avatar(
        name: &str,
        shapes: &[[f64; 4]],
        shading: &str,
        blush: bool,
        face_top: i32,
        face_bottom: i32,
        palette: &[(&str, &str)],
    ) -> Avatar {
        let palette = palette.iter().map(|&(k, v)| (k.to_owned(), v.to_owned())).collect();
        let mut a = Avatar::unresolved(
            name.to_owned(),
            shapes.to_vec(),
            Some(shading.to_owned()),
            blush,
            face_top,
            face_bottom,
            palette,
        );
        a.resolve();
        a
    }

    fn unresolved(
        name: String,
        shapes: Vec<[f64; 4]>,
        shading: Option<String>,
        blush: bool,
        face_top: i32,
        face_bottom: i32,
        palette: HashMap<String, String>,
    ) -> Avatar {
        Avatar {
            name,
            shapes,
            shading,
            blush,
            face_top,
            face_bottom,
            palette,
            outline: 0,
            deep: 0,
            shade: 0,
            body: 0,
            hi: 0,
            spec: 0,
            blush_c: 0,
            screen: 0,
            bezel: 0,
            glint: 0,
            eye: 0,
            eye_hi: 0,
            eye_lo: 0,
            glow: 0,
            accent: 0,
        }
    }

    fn resolve(&mut self) {
        let palette = &self.palette;
        let c = |key: &str, fallback: u32| {
            palette
                .get(key)
                .and_then(|hex| try_parse_color(hex))
                .unwrap_or(fallback)
        };
        self.outline = c("outline", 0xFF3E1C12);
        self.deep = c("deep", 0xFFA24A2C);
        self.shade = c("shade", 0xFFC65F3B);
        self.body = c("body", 0xFFE47E55);
        self.hi = c("hi", 0xFFF5A57D);
        self.spec = c("spec", 0xFFFFD6BC);
        self.blush_c = c("blush", 0xFFF08A7C);
        self.screen = c("screen", 0xFF171A2C);
        self.bezel = c("bezel", 0xFF2A2F4D);
        self.glint = c("glint", 0xFF3D4674);
        self.eye = c("eye", 0xFF7FEFFF);
        self.eye_hi = c("eyeHi", 0xFFE4FFFF);
        self.eye_lo = c("eyeLo", 0xFF3FB8D8);
        self.glow = c("glow", self.eye);
        // theme colour for check marks, the send button and other accents; defaults to the rim/highlight colour
        self.accent = c("accent", self.hi);
    }
}

/// An avatar file as `JsonSerializer.Deserialize<Avatar>` leaves it in the C#, before [`drawable`] checks it:
/// property names match case-insensitively (a repeated property: the last one wins), unknown ones are ignored, a
/// missing property keeps the default below, and a JSON null stays null. A value of the wrong type fails the
/// whole file. Numbers are read as System.Text.Json reads them: `-0` is a valid int, and a double too large for the
/// type is ±Infinity. The depth limit is checked by [`AvatarJson::parse`], which is what [`Avatar::parse`] uses.
#[derive(Clone, Debug, PartialEq)]
pub struct AvatarJson {
    pub name: Option<String>,
    pub shapes: Option<Vec<Option<Vec<f64>>>>,
    /// Default "soft".
    pub shading: Option<String>,
    /// Default true.
    pub blush: bool,
    /// Default 9.
    pub face_top: i32,
    /// Default 14.
    pub face_bottom: i32,
    /// Default empty. Keys are case-sensitive.
    pub palette: Option<HashMap<String, Option<String>>>,
}

impl Default for AvatarJson {
    fn default() -> Self {
        AvatarJson {
            name: None,
            shapes: None,
            shading: Some("soft".to_owned()),
            blush: true,
            face_top: 9,
            face_bottom: 14,
            palette: Some(HashMap::new()),
        }
    }
}

impl AvatarJson {
    /// The avatar in a JSON text, as `JsonSerializer.Deserialize<Avatar>` reads it: `None` when it fails (the text
    /// isn't JSON, a value has the wrong type, or it nests deeper than System.Text.Json's `MaxDepth` of 64) or the
    /// text is `null`.
    pub fn parse(json: &str) -> Option<AvatarJson> {
        if !within_max_depth(json) {
            return None;
        }
        serde_json::from_str::<Option<AvatarJson>>(json).ok()?
    }
}

/// Whether a JSON text nests arrays and objects at most [`MAX_DEPTH`] deep (the root object is depth 1). Brackets
/// inside strings don't count. Only valid JSON matters: serde_json rejects the rest anyway, and so does the C#.
fn within_max_depth(json: &str) -> bool {
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for b in json.bytes() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else {
            match b {
                b'"' => in_string = true,
                b'[' | b'{' => {
                    depth += 1;
                    if depth > MAX_DEPTH {
                        return false;
                    }
                }
                b']' | b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    true
}

/// JSON's whitespace: what may surround a value.
const JSON_WHITESPACE: [char; 4] = [' ', '\t', '\n', '\r'];

/// A value's JSON text (serde_json hands the raw text over; any JSON value is accepted here).
fn raw_json<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Box<RawValue>, D::Error> {
    Box::<RawValue>::deserialize(deserializer)
}

/// A JSON number read into a `double` as System.Text.Json reads it: correctly rounded, and a number too large for a
/// double is ±Infinity (serde_json refuses it as out of range). Anything but a number fails.
///
/// `Drawable` only checks the first four numbers of a shape, so `[13,14,7,7,1e400]` loads in the C#.
struct JsonDouble(f64);

impl<'de> Deserialize<'de> for JsonDouble {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = raw_json(deserializer)?;
        let text = raw.get().trim_matches(JSON_WHITESPACE);
        // the text is valid JSON: a number starts with '-' or a digit, and Rust parses every JSON number (to the
        // nearest double, overflowing to infinity, as .NET does)
        if text.starts_with(|c: char| c == '-' || c.is_ascii_digit())
            && let Ok(v) = text.parse::<f64>()
        {
            return Ok(JsonDouble(v));
        }
        Err(de::Error::invalid_type(Unexpected::Other(text), &"a number"))
    }
}

/// A JSON number read into an `int` as System.Text.Json reads it (`Utf8JsonReader.TryGetInt32`): digits with an
/// optional minus, in range, and nothing else. So `-0` is 0 (serde_json reads it as the float -0.0, which an i32
/// refuses), while `8.0`, `8e0`, `-0.0` and 2147483648 fail.
struct JsonInt32(i32);

impl<'de> Deserialize<'de> for JsonInt32 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = raw_json(deserializer)?;
        let text = raw.get().trim_matches(JSON_WHITESPACE);
        // valid JSON never has a '+' or leading zeros, so what i32 parses is exactly what TryGetInt32 takes
        match text.parse::<i32>() {
            Ok(v) => Ok(JsonInt32(v)),
            Err(_) => Err(de::Error::invalid_type(Unexpected::Other(text), &"an int32")),
        }
    }
}

impl<'de> Deserialize<'de> for AvatarJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct AvatarVisitor;

        impl<'de> Visitor<'de> for AvatarVisitor {
            type Value = AvatarJson;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an avatar object")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<AvatarJson, M::Error> {
                let mut a = AvatarJson::default();
                while let Some(key) = map.next_key::<String>()? {
                    // PropertyNameCaseInsensitive; the fields (Outline, Deep, ...) aren't properties, so a file
                    // can't set them
                    match key.to_ascii_lowercase().as_str() {
                        "name" => a.name = map.next_value()?,
                        "shapes" => {
                            let shapes: Option<Vec<Option<Vec<JsonDouble>>>> = map.next_value()?;
                            a.shapes = shapes.map(|shapes| {
                                shapes
                                    .into_iter()
                                    .map(|e| e.map(|e| e.into_iter().map(|v| v.0).collect()))
                                    .collect()
                            });
                        }
                        "shading" => a.shading = map.next_value()?,
                        "blush" => a.blush = map.next_value()?,
                        "facetop" => a.face_top = map.next_value::<JsonInt32>()?.0,
                        "facebottom" => a.face_bottom = map.next_value::<JsonInt32>()?.0,
                        "palette" => a.palette = map.next_value()?,
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                Ok(a)
            }
        }

        deserializer.deserialize_map(AvatarVisitor)
    }
}

/// What the pet can draw without crashing: every shape a real ellipse (`Pet::build_body` reads all four numbers of
/// each) and the face rows on the grid (`Pet::build_face` loops over them). A hand-edited file easily misses a number.
pub fn drawable(a: &AvatarJson) -> bool {
    matches!(&a.shapes, Some(shapes) if !shapes.is_empty() && shapes.iter().all(|e| matches!(e,
        Some(e) if e.len() >= 4 && e[..4].iter().all(|v| v.is_finite()) && e[2] > 0.0 && e[3] > 0.0)))
        && a.face_top >= 0
        && a.face_top <= a.face_bottom
        && a.face_bottom < GH
}

/// "#RRGGBB" or "#AARRGGBB" -> 0xAARRGGBB.
///
/// The C# trims whitespace, then every leading '#', and hands the rest to `uint.TryParse(h, NumberStyles.HexNumber)`
/// when it is 8 characters long (6 get "FF" in front). That parse also takes ASCII whitespace before the digits (and
/// after them, then trailing NULs), so "#   FFFFF" is 0x000FFFFF. This keeps every one of those rules.
pub fn try_parse_color(hex: &str) -> Option<u32> {
    // string.IsNullOrWhiteSpace
    if hex.chars().all(char::is_whitespace) {
        return None;
    }
    let mut h = hex.trim().trim_start_matches('#').to_owned();
    // string.Length counts UTF-16 code units
    if h.encode_utf16().count() == 6 {
        h.insert_str(0, "FF");
    }
    if h.encode_utf16().count() != 8 {
        return None;
    }
    parse_hex_number(&h)
}

/// `uint.TryParse(s, NumberStyles.HexNumber, CultureInfo.InvariantCulture)`: ASCII hex digits with optional leading
/// and trailing whitespace (tab, LF, VT, FF, CR, space), then optional NULs; no sign, no "0x".
fn parse_hex_number(s: &str) -> Option<u32> {
    let white = |c: char| matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ');
    let s = s.trim_start_matches(white);
    let end = s.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(s.len());
    let (digits, rest) = s.split_at(end);
    if digits.is_empty() || !rest.trim_start_matches(white).chars().all(|c| c == '\0') {
        return None;
    }
    // leading zeros don't overflow
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return Some(0);
    }
    if digits.len() > 8 {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

/// Reads a text file as `File.ReadAllText` does: a byte order mark picks UTF-8, UTF-16 (LE/BE) or UTF-32 (LE/BE)
/// and is dropped; without one the bytes are UTF-8. Invalid sequences become U+FFFD.
pub fn read_all_text(path: &Path) -> io::Result<String> {
    Ok(decode_text(&fs::read(path)?))
}

fn decode_text(bytes: &[u8]) -> String {
    fn utf16(b: &[u8], unit: fn([u8; 2]) -> u16) -> String {
        let units: Vec<u16> = b.chunks_exact(2).map(|c| unit([c[0], c[1]])).collect();
        let mut s = String::from_utf16_lossy(&units);
        if b.len() % 2 != 0 {
            s.push(char::REPLACEMENT_CHARACTER);
        }
        s
    }
    fn utf32(b: &[u8], unit: fn([u8; 4]) -> u32) -> String {
        let mut s: String = b
            .chunks_exact(4)
            .map(|c| char::from_u32(unit([c[0], c[1], c[2], c[3]])).unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect();
        if b.len() % 4 != 0 {
            s.push(char::REPLACEMENT_CHARACTER);
        }
        s
    }
    // the order StreamReader.DetectEncoding checks them in
    match bytes {
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        [0xFF, 0xFE, 0, 0, rest @ ..] => utf32(rest, u32::from_le_bytes),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0, 0, 0xFE, 0xFF, rest @ ..] => utf32(rest, u32::from_be_bytes),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// `Directory.GetFiles(dir, "*.json")`: the pattern ignores case on Windows and macOS, not elsewhere.
fn matches_json_pattern(file_name: &str) -> bool {
    let name = file_name.as_bytes();
    if cfg!(any(windows, target_os = "macos")) {
        name.len() >= 5 && name[name.len() - 5..].eq_ignore_ascii_case(b".json")
    } else {
        name.ends_with(b".json")
    }
}

/// `Path.GetFileNameWithoutExtension` of a file name: everything before the last '.'.
fn file_name_without_extension(file_name: &str) -> &str {
    file_name.rfind('.').map_or(file_name, |i| &file_name[..i])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_avatars() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../avatars")
    }

    #[test]
    fn try_parse_color_reads_rgb_and_argb() {
        assert_eq!(try_parse_color("#3366CC"), Some(0xFF3366CC));
        assert_eq!(try_parse_color("3366cc"), Some(0xFF3366CC));
        assert_eq!(try_parse_color("#80FF0000"), Some(0x80FF0000));
        assert_eq!(try_parse_color("#00000000"), Some(0));
        assert_eq!(try_parse_color("  #3366CC\t"), Some(0xFF3366CC));
        assert_eq!(try_parse_color("\u{a0}#3366CC\u{2028}"), Some(0xFF3366CC));
        assert_eq!(try_parse_color("###3366CC"), Some(0xFF3366CC));
    }

    #[test]
    fn try_parse_color_rejects_what_uint_try_parse_rejects() {
        for bad in [
            "",
            " ",
            "\t\n",
            "#",
            "##",
            "#3366C",
            "#3366CCD",
            "#1122334455",
            "#GG0000",
            "fff",
            "#+1234567",
            "#-1234567",
            "#0x123456",
            "#FF 00000",
            "#\u{a0}FFFFFFF",
            "#\u{FF26}F0000",
            "#é12345",
        ] {
            assert_eq!(try_parse_color(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn try_parse_color_keeps_the_number_style_whitespace_quirks() {
        // "   FFFFF" is 8 characters: leading whitespace is allowed by NumberStyles.HexNumber
        assert_eq!(try_parse_color("#   FFFFF"), Some(0x000FFFFF));
        assert_eq!(try_parse_color("#\tFFFFFFF"), Some(0x0FFFFFFF));
        assert_eq!(try_parse_color("#\u{b}3366CCF"), Some(0x03366CCF));
        // 7 characters: neither 6 nor 8
        assert_eq!(try_parse_color("#  FFFFF"), None);
        // trailing whitespace, then NULs, after the digits
        assert_eq!(try_parse_color("#ABCDEF0\0"), Some(0x0ABCDEF0));
        assert_eq!(try_parse_color("#ABCDEF \0"), Some(0x00ABCDEF));
        assert_eq!(try_parse_color("#ABCDEF\0 "), None);
    }

    fn json(s: &str) -> AvatarJson {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn drawable_needs_real_ellipses_and_face_rows_on_the_grid() {
        // the C# AvatarTests.AnAvatarThePetCantDraw_IsSkipped cases. 1e400 parses, as in System.Text.Json: a number
        // too large for a double is Infinity, which Drawable refuses in the first four numbers of a shape.
        for bad in [
            r#"{"name":"Short","shapes":[[13,14,7]]}"#,
            r#"{"name":"NullShape","shapes":[[13,14,7,7],null]}"#,
            r#"{"name":"Empty","shapes":[]}"#,
            r#"{"name":"NoShapes"}"#,
            r#"{"name":"NullShapes","shapes":null}"#,
            r#"{"name":"Flat","shapes":[[13,14,0,7]]}"#,
            r#"{"name":"Negative","shapes":[[13,14,7,-2]]}"#,
            r#"{"name":"Huge","shapes":[[13,14,1e400,7]]}"#,
            r#"{"name":"FaceOff","shapes":[[13,14,7,7]],"faceBottom":2147483647}"#,
            r#"{"name":"FaceLow","shapes":[[13,14,7,7]],"faceBottom":24}"#,
            r#"{"name":"FaceUp","shapes":[[13,14,7,7]],"faceTop":-5}"#,
            r#"{"name":"FaceFlip","shapes":[[13,14,7,7]],"faceTop":14,"faceBottom":9}"#,
            r#"{"shapes":[[-1e400,14,7,7]]}"#,
            r#"{"shapes":[[13,14,7,7]],"faceTop":-2147483648}"#,
        ] {
            assert!(!drawable(&json(bad)), "{bad}");
        }
        assert!(drawable(&json(r#"{"shapes":[[13,14,7.3,7.3,1],[13,7,4,4]]}"#)));
        assert!(drawable(&json(
            r#"{"shapes":[[13,14,7,7]],"faceTop":23,"faceBottom":23}"#
        )));
        assert!(drawable(&json(
            r#"{"shapes":[[13,14,7,7]],"faceTop":0,"faceBottom":0}"#
        )));
    }

    #[test]
    fn json_numbers_read_as_in_system_text_json() {
        // -0 is the int 0 (serde_json alone reads it as the float -0.0 and refuses it for an i32)
        let j = json(r#"{"shapes":[[13,14,7,7]],"faceTop":-0}"#);
        assert_eq!((j.face_top, j.face_bottom), (0, 14));
        let j = json(r#"{"shapes":[[13,14,7,7]],"faceTop":0,"faceBottom":-0}"#);
        assert_eq!((j.face_top, j.face_bottom), (0, 0));
        assert!(drawable(&j));
        assert_eq!(json(r#"{"faceTop":-2147483648}"#).face_top, i32::MIN);
        assert_eq!(json(r#"{"faceBottom": 23 }"#).face_bottom, 23);

        // numbers too large for a double are ±Infinity; past the fourth number of a shape nothing checks them
        let j = json(r#"{"shapes":[[13,14,7,7,1e400],[13,8,4,4,-1e400,1.8e308,1.7976931348623159e308]]}"#);
        let shapes = j.shapes.clone().unwrap();
        assert_eq!(shapes[0].as_deref().unwrap()[4], f64::INFINITY);
        assert_eq!(
            shapes[1].as_deref().unwrap()[4..],
            [f64::NEG_INFINITY, f64::INFINITY, f64::INFINITY]
        );
        assert!(drawable(&j));
        let a = Avatar::parse(r#"{"shapes":[[13,14,7,7,1e99999999999999999999]]}"#, "big").unwrap();
        assert_eq!(a.shapes, vec![[13.0, 14.0, 7.0, 7.0]]);
        // the largest double, rounded down to it; tiny numbers round to (signed) zero
        let j = json(r#"{"shapes":[[13,14,7,7,1.7976931348623158e308,1e-400,-0,-1e-400]]}"#);
        let e = j.shapes.unwrap()[0].clone().unwrap();
        assert_eq!(e[4], f64::MAX);
        assert_eq!(
            [e[5].to_bits(), e[6].to_bits(), e[7].to_bits()],
            [0.0f64.to_bits(), (-0.0f64).to_bits(), (-0.0f64).to_bits()]
        );

        // what the C# refuses (JsonException): an int that isn't all digits, or out of range; a shape number
        // that isn't a number
        for bad in [
            r#"{"faceTop":-0.0}"#,
            r#"{"faceTop":8.0}"#,
            r#"{"faceTop":8e0}"#,
            r#"{"faceTop":0e0}"#,
            r#"{"faceTop":1e1}"#,
            r#"{"faceTop":-2147483649}"#,
            r#"{"faceBottom":null}"#,
            r#"{"faceBottom":true}"#,
            r#"{"faceBottom":[9]}"#,
            r#"{"shapes":[[13,14,7,7,null]]}"#,
            r#"{"shapes":[[13,14,7,7,"1"]]}"#,
            r#"{"shapes":[[13,14,7,7,true]]}"#,
            r#"{"shapes":[[13,14,7,[7]]]}"#,
            r#"{"shapes":[[13,14,7,7,01]]}"#,
            r#"{"shapes":[[13,14,7,7,1.]]}"#,
            r#"{"shapes":[[13,14,7,7,.5]]}"#,
            r#"{"shapes":[[13,14,7,7,+1]]}"#,
            r#"{"shapes":[[13,14,7,7,NaN]]}"#,
            r#"{"shapes":[[13,14,7,7,Infinity]]}"#,
        ] {
            assert!(AvatarJson::parse(bad).is_none(), "{bad}");
        }
        // an unknown property is skipped whatever its number
        assert!(Avatar::parse(r#"{"shapes":[[13,14,7,7]],"x":1e400,"y":-0}"#, "f").is_some());
    }

    #[test]
    fn json_deeper_than_64_levels_fails_as_in_system_text_json() {
        let nested = |n: usize, open: &str, close: &str, inner: &str| {
            format!(
                r#"{{"shapes":[[13,14,7,7]],"x":{}{inner}{}}}"#,
                open.repeat(n),
                close.repeat(n)
            )
        };
        // the root object is level 1: 63 more levels load, 64 don't, even in a property the C# ignores
        for n in [0, 1, 62, 63] {
            assert!(Avatar::parse(&nested(n, "[", "]", "1"), "f").is_some(), "{n} arrays");
            assert!(
                Avatar::parse(&nested(n, r#"{"a":"#, "}", "1"), "f").is_some(),
                "{n} objects"
            );
        }
        for n in [64, 65, 100, 127, 128, 130, 1000] {
            assert!(Avatar::parse(&nested(n, "[", "]", "1"), "f").is_none(), "{n} arrays");
            assert!(
                Avatar::parse(&nested(n, r#"{"a":"#, "}", "1"), "f").is_none(),
                "{n} objects"
            );
        }
        // brackets in strings (and escaped quotes in them) don't nest
        assert!(Avatar::parse(&nested(63, "[", "]", r#""[[[[\"[[{\\""#), "f").is_some());
        assert!(Avatar::parse(&nested(0, "", "", &format!("{:?}", "[".repeat(100))), "f").is_some());
        assert!(Avatar::parse(&format!(r#"{{"{}":1,"shapes":[[13,14,7,7]]}}"#, "{".repeat(100)), "f").is_some());
        assert!(within_max_depth(&"[".repeat(64)) && !within_max_depth(&"[".repeat(65)));
    }

    #[test]
    fn json_property_names_ignore_case_and_the_last_one_wins() {
        let j = json(
            r##"{"NAME":"First","Name":"Cased","Shapes":[[13,14,7,7]],"SHADING":"rim","Blush":false,"FaceTOP":8,
                "facebottom":15,"face_top":3,"outline":"#FF0000","unknown":{"a":[1,null]},
                "PALETTE":{"Body":"#112233","body":"#445566","EYEHI":"#FFFFFF","eye":null}}"##,
        );
        assert_eq!(j.name.as_deref(), Some("Cased"));
        assert_eq!(j.shading.as_deref(), Some("rim"));
        assert!(!j.blush);
        assert_eq!((j.face_top, j.face_bottom), (8, 15));
        let a = Avatar::from_json(j, "file").unwrap();
        // palette keys are case-sensitive: "Body" and "EYEHI" are not "body" and "eyeHi"
        assert_eq!(a.body, 0xFF445566);
        assert_eq!(a.eye_hi, 0xFFE4FFFF);
        // a null colour falls back; glow follows eye, accent follows hi
        assert_eq!(a.eye, 0xFF7FEFFF);
        assert_eq!(a.glow, 0xFF7FEFFF);
        assert_eq!(a.accent, 0xFFF5A57D);
        // the top-level "outline" is not a property
        assert_eq!(a.outline, 0xFF3E1C12);
    }

    #[test]
    fn json_defaults_nulls_and_wrong_types() {
        let j = json(r#"{"shapes":[[13,14,7,7]]}"#);
        assert_eq!(
            j,
            AvatarJson {
                shapes: Some(vec![Some(vec![13.0, 14.0, 7.0, 7.0])]),
                ..AvatarJson::default()
            }
        );
        let a = Avatar::from_json(j, "noname").unwrap();
        assert_eq!(a.name, "noname");
        assert_eq!(a.shading.as_deref(), Some("soft"));
        assert!(a.blush);

        let j = json(r#"{"name":null,"shapes":[[13,14,7,7]],"shading":null,"palette":null}"#);
        assert_eq!(
            (j.name.as_ref(), j.shading.as_ref(), j.palette.as_ref()),
            (None, None, None)
        );
        let a = Avatar::from_json(j, "nullname").unwrap();
        assert_eq!(
            (a.name.as_str(), a.body, a.outline),
            ("nullname", 0xFFE47E55, 0xFF3E1C12)
        );
        assert_eq!(
            Avatar::parse(r#"{"name":"","shapes":[[13,14,7,7]]}"#, "file")
                .unwrap()
                .name,
            ""
        );

        assert_eq!(serde_json::from_str::<Option<AvatarJson>>("null").unwrap(), None);
        for bad in [
            "[]",
            "{",
            "",
            r#"{"shapes":[[13,14,7,7]],}"#,
            r#"{"shapes":[[13,14,7,7]]} // c"#,
            r#"{"blush":"yes"}"#,
            r#"{"blush":null}"#,
            r#"{"faceTop":9.0}"#,
            r#"{"faceTop":"9"}"#,
            r#"{"faceTop":2147483648}"#,
            r#"{"shapes":[[13,14,null,7]]}"#,
            r#"{"shapes":[13,14,7,7]}"#,
            r#"{"palette":{"body":5}}"#,
            r#"{"palette":[]}"#,
            r#"{"name":5}"#,
        ] {
            assert!(serde_json::from_str::<Option<AvatarJson>>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn built_ins_are_resolved() {
        let s = Avatar::sprout();
        assert_eq!(
            (s.name.as_str(), s.outline, s.eye_hi, s.glow, s.accent),
            ("Sprout", 0xFF3E1C12, 0xFFE4FFFF, 0xFF7FEFFF, 0xFFE27A52)
        );
        let h = Avatar::hood();
        assert_eq!(
            (h.name.as_str(), h.shading.as_deref(), h.blush),
            ("Hood", Some("rim"), false)
        );
        assert_eq!(
            (h.face_top, h.face_bottom, h.glow, h.accent),
            (8, 15, 0xFF3D9BFF, 0xFF4A8CFF)
        );
        assert_eq!(Avatar::built_in(), vec![s, h]);
    }

    #[test]
    fn the_example_avatar_file_loads() {
        let a = Avatar::load(&repo_avatars().join("hood-green.json")).expect("avatars/hood-green.json loads");
        assert_eq!(a.name, "Hood (green)");
        assert_eq!(
            a.shapes,
            vec![[13.0, 10.6, 7.2, 7.4], [13.0, 4.4, 3.2, 3.0], [13.0, 19.6, 10.2, 4.3]]
        );
        assert_eq!(
            (a.shading.as_deref(), a.blush, a.face_top, a.face_bottom),
            (Some("rim"), false, 8, 15)
        );
        assert_eq!(
            (a.outline, a.hi, a.eye, a.eye_hi, a.glow, a.accent),
            (0xFF030805, 0xFF2FD36B, 0xFF5CFF9C, 0xFFE8FFF0, 0xFF2BFF7A, 0xFF2FD36B)
        );
        let all = Avatar::all(&repo_avatars());
        assert_eq!(all[..2], Avatar::built_in()[..]);
        assert!(all.contains(&a));
    }

    #[test]
    fn a_missing_folder_gives_the_built_ins() {
        assert_eq!(
            Avatar::all(Path::new("/nonexistent/aipet-sprite/avatars")),
            Avatar::built_in()
        );
        assert_eq!(Avatar::custom_dir(Path::new("/data")), Path::new("/data/avatars"));
    }

    #[test]
    fn text_decoding_follows_the_byte_order_mark() {
        let s = r#"{"name":"é"}"#;
        let utf16le: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(s.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        let utf16be: Vec<u8> = [0xFE, 0xFF]
            .into_iter()
            .chain(s.encode_utf16().flat_map(u16::to_be_bytes))
            .collect();
        let utf32le: Vec<u8> = [0xFF, 0xFE, 0, 0]
            .into_iter()
            .chain(s.chars().flat_map(|c| (c as u32).to_le_bytes()))
            .collect();
        let utf32be: Vec<u8> = [0, 0, 0xFE, 0xFF]
            .into_iter()
            .chain(s.chars().flat_map(|c| (c as u32).to_be_bytes()))
            .collect();
        let utf8bom: Vec<u8> = [0xEF, 0xBB, 0xBF].into_iter().chain(s.bytes()).collect();
        for b in [utf16le, utf16be, utf32le, utf32be, utf8bom, s.as_bytes().to_vec()] {
            assert_eq!(decode_text(&b), s);
        }
        assert_eq!(decode_text(b"a\xFFb"), "a\u{FFFD}b");
    }

    #[test]
    fn file_names() {
        assert_eq!(file_name_without_extension("hood-green.json"), "hood-green");
        assert_eq!(file_name_without_extension("a.b.json"), "a.b");
        assert_eq!(file_name_without_extension(".json"), "");
        assert!(matches_json_pattern("a.json") && matches_json_pattern(".json"));
        assert!(!matches_json_pattern("a.json.txt") && !matches_json_pattern("json"));
    }
}
