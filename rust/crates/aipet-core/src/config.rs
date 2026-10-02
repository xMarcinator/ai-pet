//! `config.json`, `jira.json` and `github.json` as the .NET app reads and writes them
//! (`src/AiPet.UI/MainWindow.axaml.cs`, `Jira.cs`, `GitHub.cs`).
//!
//! The app reads and writes them with System.Text.Json's defaults, so these follow the same rules, and a Rust pet and
//! a .NET pet read each other's files (a rollback loses nothing):
//! - names match case for case; unknown fields are skipped when reading, and so aren't written back; a name that
//!   comes twice takes its last value;
//! - a value of the wrong type (a quoted number, a fraction for an int, null for a bool), text that isn't one JSON
//!   value, or nesting deeper than 64 levels fails the whole read, and the defaults stand ([`LoadStatus::Corrupt`]);
//! - a file is read as `File.ReadAllText` reads it: UTF-8, or the encoding its byte order mark names;
//! - `config.json` is written compact, `jira.json` and `github.json` indented, with the platform's line ending;
//!   strings are escaped as System.Text.Json's default encoder escapes them, and doubles written as .NET writes them.
//!
//! `None` stands for the C#'s null wherever the C# type can hold one. Loading never writes, nor creates the data
//! folder. Saving writes the whole file in place, as the app's `File.WriteAllText` does. The app writes
//! `config.json` only after a real change (a setting, a finished drag, Reset position), never on start or quit;
//! that choice is the caller's, and [`LoadStatus`] tells it whether there was a file it could read.

use std::fmt::{self, Write as _};
use std::fs;
use std::io;
use std::path::Path;

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::value::RawValue;

/// How a data file was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadStatus {
    /// The values are the file's. A file that holds only `null` reads as the defaults, as for the app.
    Loaded,
    /// There is no file: the defaults.
    Missing,
    /// There is a file the app can't read: the defaults, and the file is left as it is.
    Corrupt,
}

/// The pet's window and preferences (MainWindow's `Config`), in `config.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Where the whole window's top-left corner was saved, in desktop pixels.
    pub left: Option<i32>,
    pub top: Option<i32>,
    /// Show bubbles.
    pub pills: bool,
    /// Only in files from before the toolbar under the pet was removed: whether it showed when the position was
    /// saved. Written only when it is set, which the app never does.
    pub toolbar: Option<bool>,
    /// Always on top.
    pub on_top: bool,
    pub avatar: Option<String>,
    pub music: bool,
    /// The window's height (DIPs) when the position was saved, so a taller window keeps the pet in place. A file can
    /// hold one too big for a double, which reads as infinity; then the file can't be written (as for the app).
    pub window_height: f64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            left: None,
            top: None,
            pills: true,
            toolbar: None,
            on_top: true,
            avatar: Some("Sprout".into()),
            music: true,
            window_height: 420.0,
        }
    }
}

/// `JiraWatcher.DefaultJql`: the user's own open issues, with fields every Jira site has.
pub const DEFAULT_JQL: &str = "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC";

/// The Jira watcher's settings (`JiraWatcher.Settings`), in `jira.json`. The token isn't here: it is in the
/// secret store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JiraSettings {
    pub enabled: bool,
    /// The Jira Cloud address, as `your-team.atlassian.net`. Empty until the user enters one.
    pub site: Option<String>,
    pub email: Option<String>,
    pub jql: Option<String>,
    pub poll_seconds: i32,
}

impl Default for JiraSettings {
    fn default() -> Self {
        JiraSettings {
            enabled: false,
            site: Some(String::new()),
            email: Some(String::new()),
            jql: Some(DEFAULT_JQL.into()),
            poll_seconds: 120,
        }
    }
}

/// The GitHub watcher's settings (`GitHubWatcher.Settings`), in `github.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubSettings {
    pub enabled: bool,
    /// `github.com`, or a GitHub Enterprise Server host.
    pub host: Option<String>,
    /// The organisations searched for a Jira task's pull request.
    pub orgs: Option<Vec<Option<String>>>,
    /// Jira project keys recognised in pull requests' text.
    pub jira_projects: Option<Vec<Option<String>>>,
    pub poll_seconds: i32,
}

impl Default for GitHubSettings {
    fn default() -> Self {
        GitHubSettings {
            enabled: false,
            host: Some("github.com".into()),
            orgs: Some(Vec::new()),
            jira_projects: Some(Vec::new()),
            poll_seconds: 120,
        }
    }
}

macro_rules! data_file {
    ($type:ty, $file:literal, $indented:literal) => {
        impl $type {
            #[doc = concat!("Reads `", $file, "` in the data folder, as the app does.")]
            pub fn load() -> (Self, LoadStatus) {
                Self::load_from(&aipet_ipc::paths::data_dir().join($file))
            }

            /// Reads the file at `path`, as the app reads its own.
            pub fn load_from(path: &Path) -> (Self, LoadStatus) {
                load(path)
            }

            #[doc = concat!("Writes `", $file, "` in the data folder as the app does, creating the folder if it has to.")]
            pub fn save(&self) -> io::Result<()> {
                self.save_to(&aipet_ipc::paths::data_dir().join($file))
            }

            /// Writes the file at `path`, as the app writes its own. Nothing is written when the values can't be (a
            /// height that isn't finite).
            pub fn save_to(&self, path: &Path) -> io::Result<()> {
                save(self, path, $indented)
            }
        }
    };
}

data_file!(Config, "config.json", false);
data_file!(JiraSettings, "jira.json", true);
data_file!(GitHubSettings, "github.json", true);

/// A data file's fields, as System.Text.Json reads and writes them.
trait Fields: Default {
    /// Sets the field `name` (case for case) from its JSON text; an unknown name is skipped.
    fn set(&mut self, name: &str, value: &str) -> Result<(), Unreadable>;
    /// The fields in the order the C# declares them, which is the order it writes them in.
    fn fields(&self) -> Vec<(&str, Json<'_>)>;
}

impl Fields for Config {
    fn set(&mut self, name: &str, value: &str) -> Result<(), Unreadable> {
        match name {
            "Left" => self.left = nullable(value, int)?,
            "Top" => self.top = nullable(value, int)?,
            "Pills" => self.pills = boolean(value)?,
            "Toolbar" => self.toolbar = nullable(value, boolean)?,
            "OnTop" => self.on_top = boolean(value)?,
            "Avatar" => self.avatar = nullable(value, string)?,
            "Music" => self.music = boolean(value)?,
            "WindowHeight" => self.window_height = double(value)?,
            _ => {}
        }
        Ok(())
    }

    fn fields(&self) -> Vec<(&str, Json<'_>)> {
        let mut fields = vec![
            ("Left", self.left.map_or(Json::Null, Json::Int)),
            ("Top", self.top.map_or(Json::Null, Json::Int)),
            ("Pills", Json::Bool(self.pills)),
        ];
        // [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
        if let Some(toolbar) = self.toolbar {
            fields.push(("Toolbar", Json::Bool(toolbar)));
        }
        fields.extend([
            ("OnTop", Json::Bool(self.on_top)),
            ("Avatar", Json::of(&self.avatar)),
            ("Music", Json::Bool(self.music)),
            ("WindowHeight", Json::Double(self.window_height)),
        ]);
        fields
    }
}

impl Fields for JiraSettings {
    fn set(&mut self, name: &str, value: &str) -> Result<(), Unreadable> {
        match name {
            "Enabled" => self.enabled = boolean(value)?,
            "Site" => self.site = nullable(value, string)?,
            "Email" => self.email = nullable(value, string)?,
            "Jql" => self.jql = nullable(value, string)?,
            "PollSeconds" => self.poll_seconds = int(value)?,
            _ => {}
        }
        Ok(())
    }

    fn fields(&self) -> Vec<(&str, Json<'_>)> {
        vec![
            ("Enabled", Json::Bool(self.enabled)),
            ("Site", Json::of(&self.site)),
            ("Email", Json::of(&self.email)),
            ("Jql", Json::of(&self.jql)),
            ("PollSeconds", Json::Int(self.poll_seconds)),
        ]
    }
}

impl Fields for GitHubSettings {
    fn set(&mut self, name: &str, value: &str) -> Result<(), Unreadable> {
        match name {
            "Enabled" => self.enabled = boolean(value)?,
            "Host" => self.host = nullable(value, string)?,
            "Orgs" => self.orgs = nullable(value, strings)?,
            "JiraProjects" => self.jira_projects = nullable(value, strings)?,
            "PollSeconds" => self.poll_seconds = int(value)?,
            _ => {}
        }
        Ok(())
    }

    fn fields(&self) -> Vec<(&str, Json<'_>)> {
        vec![
            ("Enabled", Json::Bool(self.enabled)),
            ("Host", Json::of(&self.host)),
            ("Orgs", Json::list(&self.orgs)),
            ("JiraProjects", Json::list(&self.jira_projects)),
            ("PollSeconds", Json::Int(self.poll_seconds)),
        ]
    }
}

fn load<T: Fields>(path: &Path) -> (T, LoadStatus) {
    match fs::read(path) {
        // .NET's FileNotFoundException and DirectoryNotFoundException (which ENOTDIR is on Unix)
        Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => {
            (T::default(), LoadStatus::Missing)
        }
        Err(_) => (T::default(), LoadStatus::Corrupt),
        Ok(bytes) => match read_fields(&read_text(&bytes)) {
            Ok(value) => (value, LoadStatus::Loaded),
            Err(Unreadable) => (T::default(), LoadStatus::Corrupt),
        },
    }
}

fn save<T: Fields>(value: &T, path: &Path, indented: bool) -> io::Result<()> {
    // serialized first, as the app's `File.WriteAllText(path, JsonSerializer.Serialize(…))` does, so a value that
    // can't be written leaves the file as it is
    let text = write_object(&value.fields(), indented)?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, text)
}

fn read_fields<T: Fields>(text: &str) -> Result<T, Unreadable> {
    let mut value = T::default();
    for (name, raw) in read_members(text)?.unwrap_or_default() {
        value.set(&name, raw.get())?;
    }
    Ok(value)
}

// ---------------------------------------------------------------------------------------------------------------
// System.Text.Json's rules, which secrets.json (secrets::FileSecrets) follows too.

/// A file System.Text.Json can't read (the C# throws, and the app keeps its defaults).
#[derive(Debug)]
pub(crate) struct Unreadable;

/// A file's text, as `File.ReadAllText` decodes it: by its byte order mark (UTF-8, UTF-16 or UTF-32, either byte
/// order), else as UTF-8, and anything that isn't valid in its encoding as U+FFFD.
pub(crate) fn read_text(bytes: &[u8]) -> String {
    fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
        let (units, odd) = bytes.as_chunks::<2>();
        let mut text: String = char::decode_utf16(units.iter().map(|&u| unit(u)))
            .map(|c| c.unwrap_or('\u{FFFD}'))
            .collect();
        if !odd.is_empty() {
            text.push('\u{FFFD}');
        }
        text
    }
    fn utf32(bytes: &[u8], unit: fn([u8; 4]) -> u32) -> String {
        let (units, odd) = bytes.as_chunks::<4>();
        let mut text: String = units
            .iter()
            .map(|&u| char::from_u32(unit(u)).unwrap_or('\u{FFFD}'))
            .collect();
        if !odd.is_empty() {
            text.push('\u{FFFD}');
        }
        text
    }
    match bytes {
        [0xFF, 0xFE, 0, 0, rest @ ..] => utf32(rest, u32::from_le_bytes),
        [0, 0, 0xFE, 0xFF, rest @ ..] => utf32(rest, u32::from_be_bytes),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// The members of the JSON object `text` holds, in order, each value as its JSON text; `None` for `null`. Anything
/// else, or nesting deeper than System.Text.Json reads, is unreadable.
fn read_members(text: &str) -> Result<Option<Vec<Member>>, Unreadable> {
    if too_deep(text) {
        return Err(Unreadable);
    }
    let members: Option<Members> = serde_json::from_str(text).map_err(|_| Unreadable)?;
    Ok(members.map(|m| m.0))
}

/// A member's name, and its value's JSON text.
type Member = (String, Box<RawValue>);

/// A JSON object's members, duplicates and all.
struct Members(Vec<Member>);

impl<'de> Deserialize<'de> for Members {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Object;
        impl<'de> Visitor<'de> for Object {
            type Value = Members;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Members, A::Error> {
                let mut members = Vec::new();
                while let Some(member) = map.next_entry()? {
                    members.push(member);
                }
                Ok(Members(members))
            }
        }
        deserializer.deserialize_map(Object)
    }
}

/// System.Text.Json reads at most 64 levels of arrays and objects, the outermost one included. Brackets inside
/// strings don't count.
fn too_deep(text: &str) -> bool {
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for b in text.bytes() {
        if quoted {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                quoted = false;
            }
            continue;
        }
        match b {
            b'"' => quoted = true,
            b'[' | b'{' => {
                depth += 1;
                if depth > 64 {
                    return true;
                }
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    false
}

/// `null`, or what `read` reads.
fn nullable<T>(value: &str, read: fn(&str) -> Result<T, Unreadable>) -> Result<Option<T>, Unreadable> {
    if value == "null" {
        Ok(None)
    } else {
        read(value).map(Some)
    }
}

/// An `int`: an integer in range, with no fraction or exponent even when it is whole (`1.0`, `1e2`); `-0` is 0.
fn int(value: &str) -> Result<i32, Unreadable> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Unreadable);
    }
    value.parse().map_err(|_| Unreadable)
}

fn boolean(value: &str) -> Result<bool, Unreadable> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(Unreadable),
    }
}

/// A `double`: any JSON number, correctly rounded, as .NET reads them; one too big reads as infinity.
fn double(value: &str) -> Result<f64, Unreadable> {
    if value.starts_with(|c: char| c == '-' || c.is_ascii_digit()) {
        value.parse().map_err(|_| Unreadable)
    } else {
        Err(Unreadable)
    }
}

/// A `string`. An escaped lone surrogate can't be read into one.
fn string(value: &str) -> Result<String, Unreadable> {
    if value.starts_with('"') {
        serde_json::from_str(value).map_err(|_| Unreadable)
    } else {
        Err(Unreadable)
    }
}

/// A `string[]`, whose strings may be null.
fn strings(value: &str) -> Result<Vec<Option<String>>, Unreadable> {
    if value.starts_with('[') {
        serde_json::from_str(value).map_err(|_| Unreadable)
    } else {
        Err(Unreadable)
    }
}

/// secrets.json: a `Dictionary<string, string>`, keys in the file's order. A key that comes twice keeps its first
/// place and takes its last value, as the C#'s dictionary does.
pub(crate) type Dictionary = Vec<(String, Option<String>)>;

/// `None` for a file that holds `null`.
pub(crate) fn read_dictionary(text: &str) -> Result<Option<Dictionary>, Unreadable> {
    let Some(members) = read_members(text)? else {
        return Ok(None);
    };
    let mut all = Dictionary::new();
    for (key, raw) in members {
        set(&mut all, &key, nullable(raw.get(), string)?);
    }
    Ok(Some(all))
}

/// `dictionary[key] = value`: in its place when the key is there, else at the end.
pub(crate) fn set(all: &mut Dictionary, key: &str, value: Option<String>) {
    match all.iter_mut().find(|(k, _)| k == key) {
        Some((_, v)) => *v = value,
        None => all.push((key.to_owned(), value)),
    }
}

/// `JsonSerializer.Serialize(dictionary)`: compact.
pub(crate) fn write_dictionary(all: &[(String, Option<String>)]) -> String {
    let fields: Vec<(&str, Json)> = all.iter().map(|(k, v)| (k.as_str(), Json::of(v))).collect();
    write_object(&fields, false).expect("strings can always be written")
}

/// A value as the data files hold them.
enum Json<'a> {
    Null,
    Bool(bool),
    Int(i32),
    Double(f64),
    Str(&'a str),
    Strings(&'a [Option<String>]),
}

impl<'a> Json<'a> {
    fn of(s: &'a Option<String>) -> Json<'a> {
        s.as_deref().map_or(Json::Null, Json::Str)
    }

    fn list(l: &'a Option<Vec<Option<String>>>) -> Json<'a> {
        l.as_deref().map_or(Json::Null, Json::Strings)
    }
}

/// `JsonSerializer.Serialize` of an object with these fields: compact, or indented by two spaces with
/// `Environment.NewLine`. System.Text.Json refuses a double that isn't finite, and so does this.
fn write_object(fields: &[(&str, Json)], indented: bool) -> io::Result<String> {
    let newline = if cfg!(windows) { "\r\n" } else { "\n" };
    let mut out = String::from("{");
    for (i, (name, value)) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        if indented {
            out.push_str(newline);
            out.push_str("  ");
        }
        push_string(&mut out, name);
        out.push_str(if indented { ": " } else { ":" });
        match value {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Int(n) => write!(out, "{n}").expect("a String takes any write"),
            Json::Double(x) if !x.is_finite() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{name} is {x}, which can't be written as JSON"),
                ));
            }
            // .NET's double format, which the hook's `sent` uses too
            Json::Double(x) => out.push_str(&aipet_ipc::protocol::format_seconds(*x)),
            Json::Str(s) => push_string(&mut out, s),
            Json::Strings([]) => out.push_str("[]"),
            Json::Strings(list) => {
                out.push('[');
                for (j, s) in list.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    if indented {
                        out.push_str(newline);
                        out.push_str("    ");
                    }
                    match s {
                        Some(s) => push_string(&mut out, s),
                        None => out.push_str("null"),
                    }
                }
                if indented {
                    out.push_str(newline);
                    out.push_str("  ");
                }
                out.push(']');
            }
        }
    }
    if indented && !fields.is_empty() {
        out.push_str(newline);
    }
    out.push('}');
    Ok(out)
}

/// A JSON string as System.Text.Json's default encoder writes it: printable ASCII as it is, except `"&'+<>` and
/// the backtick; `\b \t \n \f \r` and `\\` in short form; everything else as `\uXXXX` UTF-16 units, upper case.
fn push_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{C}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            ' '..='~' if !matches!(c, '"' | '&' | '\'' | '+' | '<' | '>' | '`') => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    write!(out, "\\u{unit:04X}").expect("a String takes any write");
                }
            }
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nesting_is_counted_outside_strings() {
        let nested = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
        assert!(!too_deep(&nested(64)));
        assert!(too_deep(&nested(65)));
        assert!(!too_deep(&format!("{{\"a\":\"{}\"}}", "[".repeat(100))));
        // an escaped quote doesn't end the string
        assert!(!too_deep(&format!("{{\"a\":\"\\\"{}\"}}", "{".repeat(100))));
    }

    #[test]
    fn ints_are_whole_and_in_range() {
        assert_eq!(int("0").unwrap(), 0);
        assert_eq!(int("-0").unwrap(), 0);
        assert_eq!(int("-2147483648").unwrap(), i32::MIN);
        for bad in ["2147483648", "1.0", "1e2", "-", "", "\"1\"", "true", "+1"] {
            assert!(int(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn byte_order_marks_name_the_encoding() {
        assert_eq!(read_text(b"\xEF\xBB\xBF{}"), "{}");
        assert_eq!(read_text(&[0xFF, 0xFE, b'{', 0, b'}', 0]), "{}");
        assert_eq!(read_text(&[0xFE, 0xFF, 0, b'{', 0, b'}']), "{}");
        assert_eq!(read_text(&[0xFF, 0xFE, 0, 0, b'{', 0, 0, 0]), "{");
        assert_eq!(read_text(&[0, 0, 0xFE, 0xFF, 0, 0, 0, b'}']), "}");
        // an odd byte left over, and a lone surrogate
        assert_eq!(read_text(&[0xFF, 0xFE, b'a', 0, b'b']), "a\u{FFFD}");
        assert_eq!(read_text(&[0xFF, 0xFE, 0x00, 0xD8, b'a', 0]), "\u{FFFD}a");
        assert_eq!(read_text(b"a\xFFb"), "a\u{FFFD}b");
    }
}
