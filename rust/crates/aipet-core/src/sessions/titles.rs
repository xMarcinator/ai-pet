//! Chat titles, read from the transcripts (`ChatTitle`, `AgentSessions.cs:266-289`).

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

use super::describe::short;
use super::members;

/// How much of the end of a transcript is read for its title.
const TAIL: u64 = 512 * 1024;

/// How deep System.Text.Json parses a line by default: past this the C# can't read the line.
const MAX_DEPTH: usize = 64;

/// The chat's name as the Claude app shows it, from the last custom-title line in the last 512 KB of its transcript;
/// "" when there's none, or no file to read.
pub(super) fn chat_title(transcript: Option<&str>) -> String {
    let Some(path) = transcript.filter(|p| !p.is_empty()) else {
        return String::new();
    };
    let Ok(text) = read_tail(path) else {
        return String::new();
    };
    text.rsplit('\n')
        .filter(|line| line.contains("\"custom-title\""))
        .find_map(custom_title)
        .map(|title| short(&title, 44))
        .unwrap_or_default()
}

/// The last 512 KB of the file, as text.
fn read_tail(path: &str) -> io::Result<String> {
    // like the C#'s FileStream, it lets the agent go on writing, and even delete the file
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(TAIL)))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(decode(&bytes))
}

/// The text as .NET's StreamReader over UTF-8 decodes it: by a byte order mark where there's one (UTF-8, UTF-16 or
/// UTF-32), else as UTF-8, with U+FFFD for what doesn't decode.
fn decode(bytes: &[u8]) -> String {
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        [0xFF, 0xFE, 0, 0, rest @ ..] => utf32(rest, u32::from_le_bytes),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0, 0, 0xFE, 0xFF, rest @ ..] => utf32(rest, u32::from_be_bytes),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let (units, rest) = bytes.as_chunks::<2>();
    let mut text: String = char::decode_utf16(units.iter().map(|&u| unit(u)))
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    if !rest.is_empty() {
        text.push(char::REPLACEMENT_CHARACTER);
    }
    text
}

fn utf32(bytes: &[u8], unit: fn([u8; 4]) -> u32) -> String {
    let (units, rest) = bytes.as_chunks::<4>();
    let mut text: String = units
        .iter()
        .map(|&u| char::from_u32(unit(u)).unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    if !rest.is_empty() {
        text.push(char::REPLACEMENT_CHARACTER);
    }
    text
}

/// A line's customTitle, where the C# finds one: the line parses (JsonNode.Parse, within its depth) as an object
/// with no key twice, and its customTitle is a string. Only that string is decoded, as the C#'s lazy JsonObject
/// decodes nothing else (the other values are only checked for their syntax).
fn custom_title(line: &str) -> Option<String> {
    if depth(line) > MAX_DEPTH {
        return None;
    }
    let fields = members(line)?;
    let mut keys = HashSet::new();
    if !fields.iter().all(|(key, _)| keys.insert(key.as_str())) {
        return None;
    }
    let (_, title) = fields.iter().find(|(key, _)| key == "customTitle")?;
    serde_json::from_str(title.get()).ok()
}

/// How deep a JSON text's arrays and objects nest (outside its strings).
fn depth(text: &str) -> usize {
    let (mut depth, mut deepest, mut in_string, mut escaped) = (0usize, 0usize, false, false);
    for b in text.bytes() {
        if in_string {
            match b {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'[' | b'{' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}
