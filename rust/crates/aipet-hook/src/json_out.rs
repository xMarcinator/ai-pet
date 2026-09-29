//! System.Text.Json as the registration meets it (`src/AiPet.Hook/Install.cs`, `PluginHooks.cs`), so that what the
//! Rust writes and says is what the C# writes and says:
//! - [`indented`]: `ToJsonString` with `WriteIndented` and `UnsafeRelaxedJsonEscaping`, as the settings files and the
//!   plugin's hook files are written;
//! - [`to_string`]: `JsonNode.ToString`, which `ClaudeConfig.IsOurs` reads a command with;
//! - [`deep_equals`]: `JsonNode.DeepEquals`, which decides whether anything changed;
//! - [`parse`]: `JsonNode.Parse`, with the message of `Utf8JsonReader`'s exception for a text it refuses;
//! - [`read`] and [`member`]: what reading a node throws, since `JsonObject` reads its members into a dictionary.
//!
//! All of it is held against the golden generator's output (`tests/golden/registration/json.json`, written by
//! `rust/golden/Registration.cs`): which characters each encoder escapes and how, and every message.

use std::cmp::Ordering;
use std::fmt::Write as _;

use crate::json::{self, Node, Object};

/// How deep `JsonNode.Parse` reads by default (`JsonDocumentOptions.MaxDepth`).
pub(crate) const MAX_DEPTH: usize = 64;

/// `JsonNode`'s indexer on a node that isn't an object: its `InvalidOperationException`.
pub(crate) const NOT_AN_OBJECT: &str = "The node must be of type 'JsonObject'.";

/// One of System.Text.Json's encoders.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Encoder {
    /// `JavaScriptEncoder.UnsafeRelaxedJsonEscaping`, which the registration writes with.
    Relaxed,
    /// `JavaScriptEncoder.Default`, which `JsonNode.ToString` writes with.
    Default,
}

/// The scalar values `UnsafeRelaxedJsonEscaping` escapes, as the first and last of each range: the controls, `"` and
/// `\`, and what .NET 10's Unicode data leaves unassigned, private, a separator other than the space, or outside the
/// BMP. A range may span the surrogates, which no `char` holds.
#[rustfmt::skip]
const RELAXED: &[u32] = &[
    0x0, 0x1F, 0x22, 0x22, 0x5C, 0x5C, 0x7F, 0xA0, 0x378, 0x379, 0x380, 0x383, 0x38B, 0x38B, 0x38D, 0x38D, 0x3A2,
    0x3A2, 0x530, 0x530, 0x557, 0x558, 0x58B, 0x58C, 0x590, 0x590, 0x5C8, 0x5CF, 0x5EB, 0x5EE, 0x5F5, 0x5FF, 0x70E,
    0x70E, 0x74B, 0x74C, 0x7B2, 0x7BF, 0x7FB, 0x7FC, 0x82E, 0x82F, 0x83F, 0x83F, 0x85C, 0x85D, 0x85F, 0x85F, 0x86B,
    0x86F, 0x88F, 0x88F, 0x892, 0x896, 0x984, 0x984, 0x98D, 0x98E, 0x991, 0x992, 0x9A9, 0x9A9, 0x9B1, 0x9B1, 0x9B3,
    0x9B5, 0x9BA, 0x9BB, 0x9C5, 0x9C6, 0x9C9, 0x9CA, 0x9CF, 0x9D6, 0x9D8, 0x9DB, 0x9DE, 0x9DE, 0x9E4, 0x9E5, 0x9FF,
    0xA00, 0xA04, 0xA04, 0xA0B, 0xA0E, 0xA11, 0xA12, 0xA29, 0xA29, 0xA31, 0xA31, 0xA34, 0xA34, 0xA37, 0xA37, 0xA3A,
    0xA3B, 0xA3D, 0xA3D, 0xA43, 0xA46, 0xA49, 0xA4A, 0xA4E, 0xA50, 0xA52, 0xA58, 0xA5D, 0xA5D, 0xA5F, 0xA65, 0xA77,
    0xA80, 0xA84, 0xA84, 0xA8E, 0xA8E, 0xA92, 0xA92, 0xAA9, 0xAA9, 0xAB1, 0xAB1, 0xAB4, 0xAB4, 0xABA, 0xABB, 0xAC6,
    0xAC6, 0xACA, 0xACA, 0xACE, 0xACF, 0xAD1, 0xADF, 0xAE4, 0xAE5, 0xAF2, 0xAF8, 0xB00, 0xB00, 0xB04, 0xB04, 0xB0D,
    0xB0E, 0xB11, 0xB12, 0xB29, 0xB29, 0xB31, 0xB31, 0xB34, 0xB34, 0xB3A, 0xB3B, 0xB45, 0xB46, 0xB49, 0xB4A, 0xB4E,
    0xB54, 0xB58, 0xB5B, 0xB5E, 0xB5E, 0xB64, 0xB65, 0xB78, 0xB81, 0xB84, 0xB84, 0xB8B, 0xB8D, 0xB91, 0xB91, 0xB96,
    0xB98, 0xB9B, 0xB9B, 0xB9D, 0xB9D, 0xBA0, 0xBA2, 0xBA5, 0xBA7, 0xBAB, 0xBAD, 0xBBA, 0xBBD, 0xBC3, 0xBC5, 0xBC9,
    0xBC9, 0xBCE, 0xBCF, 0xBD1, 0xBD6, 0xBD8, 0xBE5, 0xBFB, 0xBFF, 0xC0D, 0xC0D, 0xC11, 0xC11, 0xC29, 0xC29, 0xC3A,
    0xC3B, 0xC45, 0xC45, 0xC49, 0xC49, 0xC4E, 0xC54, 0xC57, 0xC57, 0xC5B, 0xC5C, 0xC5E, 0xC5F, 0xC64, 0xC65, 0xC70,
    0xC76, 0xC8D, 0xC8D, 0xC91, 0xC91, 0xCA9, 0xCA9, 0xCB4, 0xCB4, 0xCBA, 0xCBB, 0xCC5, 0xCC5, 0xCC9, 0xCC9, 0xCCE,
    0xCD4, 0xCD7, 0xCDC, 0xCDF, 0xCDF, 0xCE4, 0xCE5, 0xCF0, 0xCF0, 0xCF4, 0xCFF, 0xD0D, 0xD0D, 0xD11, 0xD11, 0xD45,
    0xD45, 0xD49, 0xD49, 0xD50, 0xD53, 0xD64, 0xD65, 0xD80, 0xD80, 0xD84, 0xD84, 0xD97, 0xD99, 0xDB2, 0xDB2, 0xDBC,
    0xDBC, 0xDBE, 0xDBF, 0xDC7, 0xDC9, 0xDCB, 0xDCE, 0xDD5, 0xDD5, 0xDD7, 0xDD7, 0xDE0, 0xDE5, 0xDF0, 0xDF1, 0xDF5,
    0xE00, 0xE3B, 0xE3E, 0xE5C, 0xE80, 0xE83, 0xE83, 0xE85, 0xE85, 0xE8B, 0xE8B, 0xEA4, 0xEA4, 0xEA6, 0xEA6, 0xEBE,
    0xEBF, 0xEC5, 0xEC5, 0xEC7, 0xEC7, 0xECF, 0xECF, 0xEDA, 0xEDB, 0xEE0, 0xEFF, 0xF48, 0xF48, 0xF6D, 0xF70, 0xF98,
    0xF98, 0xFBD, 0xFBD, 0xFCD, 0xFCD, 0xFDB, 0xFFF, 0x10C6, 0x10C6, 0x10C8, 0x10CC, 0x10CE, 0x10CF, 0x1249, 0x1249,
    0x124E, 0x124F, 0x1257, 0x1257, 0x1259, 0x1259, 0x125E, 0x125F, 0x1289, 0x1289, 0x128E, 0x128F, 0x12B1, 0x12B1,
    0x12B6, 0x12B7, 0x12BF, 0x12BF, 0x12C1, 0x12C1, 0x12C6, 0x12C7, 0x12D7, 0x12D7, 0x1311, 0x1311, 0x1316, 0x1317,
    0x135B, 0x135C, 0x137D, 0x137F, 0x139A, 0x139F, 0x13F6, 0x13F7, 0x13FE, 0x13FF, 0x1680, 0x1680, 0x169D, 0x169F,
    0x16F9, 0x16FF, 0x1716, 0x171E, 0x1737, 0x173F, 0x1754, 0x175F, 0x176D, 0x176D, 0x1771, 0x1771, 0x1774, 0x177F,
    0x17DE, 0x17DF, 0x17EA, 0x17EF, 0x17FA, 0x17FF, 0x181A, 0x181F, 0x1879, 0x187F, 0x18AB, 0x18AF, 0x18F6, 0x18FF,
    0x191F, 0x191F, 0x192C, 0x192F, 0x193C, 0x193F, 0x1941, 0x1943, 0x196E, 0x196F, 0x1975, 0x197F, 0x19AC, 0x19AF,
    0x19CA, 0x19CF, 0x19DB, 0x19DD, 0x1A1C, 0x1A1D, 0x1A5F, 0x1A5F, 0x1A7D, 0x1A7E, 0x1A8A, 0x1A8F, 0x1A9A, 0x1A9F,
    0x1AAE, 0x1AAF, 0x1ACF, 0x1AFF, 0x1B4D, 0x1B4D, 0x1BF4, 0x1BFB, 0x1C38, 0x1C3A, 0x1C4A, 0x1C4C, 0x1C8B, 0x1C8F,
    0x1CBB, 0x1CBC, 0x1CC8, 0x1CCF, 0x1CFB, 0x1CFF, 0x1F16, 0x1F17, 0x1F1E, 0x1F1F, 0x1F46, 0x1F47, 0x1F4E, 0x1F4F,
    0x1F58, 0x1F58, 0x1F5A, 0x1F5A, 0x1F5C, 0x1F5C, 0x1F5E, 0x1F5E, 0x1F7E, 0x1F7F, 0x1FB5, 0x1FB5, 0x1FC5, 0x1FC5,
    0x1FD4, 0x1FD5, 0x1FDC, 0x1FDC, 0x1FF0, 0x1FF1, 0x1FF5, 0x1FF5, 0x1FFF, 0x200A, 0x2028, 0x2029, 0x202F, 0x202F,
    0x205F, 0x205F, 0x2065, 0x2065, 0x2072, 0x2073, 0x208F, 0x208F, 0x209D, 0x209F, 0x20C1, 0x20CF, 0x20F1, 0x20FF,
    0x218C, 0x218F, 0x242A, 0x243F, 0x244B, 0x245F, 0x2B74, 0x2B75, 0x2B96, 0x2B96, 0x2CF4, 0x2CF8, 0x2D26, 0x2D26,
    0x2D28, 0x2D2C, 0x2D2E, 0x2D2F, 0x2D68, 0x2D6E, 0x2D71, 0x2D7E, 0x2D97, 0x2D9F, 0x2DA7, 0x2DA7, 0x2DAF, 0x2DAF,
    0x2DB7, 0x2DB7, 0x2DBF, 0x2DBF, 0x2DC7, 0x2DC7, 0x2DCF, 0x2DCF, 0x2DD7, 0x2DD7, 0x2DDF, 0x2DDF, 0x2E5E, 0x2E7F,
    0x2E9A, 0x2E9A, 0x2EF4, 0x2EFF, 0x2FD6, 0x2FEF, 0x3000, 0x3000, 0x3040, 0x3040, 0x3097, 0x3098, 0x3100, 0x3104,
    0x3130, 0x3130, 0x318F, 0x318F, 0x31E6, 0x31EE, 0x321F, 0x321F, 0xA48D, 0xA48F, 0xA4C7, 0xA4CF, 0xA62C, 0xA63F,
    0xA6F8, 0xA6FF, 0xA7CE, 0xA7CF, 0xA7D2, 0xA7D2, 0xA7D4, 0xA7D4, 0xA7DD, 0xA7F1, 0xA82D, 0xA82F, 0xA83A, 0xA83F,
    0xA878, 0xA87F, 0xA8C6, 0xA8CD, 0xA8DA, 0xA8DF, 0xA954, 0xA95E, 0xA97D, 0xA97F, 0xA9CE, 0xA9CE, 0xA9DA, 0xA9DD,
    0xA9FF, 0xA9FF, 0xAA37, 0xAA3F, 0xAA4E, 0xAA4F, 0xAA5A, 0xAA5B, 0xAAC3, 0xAADA, 0xAAF7, 0xAB00, 0xAB07, 0xAB08,
    0xAB0F, 0xAB10, 0xAB17, 0xAB1F, 0xAB27, 0xAB27, 0xAB2F, 0xAB2F, 0xAB6C, 0xAB6F, 0xABEE, 0xABEF, 0xABFA, 0xABFF,
    0xD7A4, 0xD7AF, 0xD7C7, 0xD7CA, 0xD7FC, 0xF8FF, 0xFA6E, 0xFA6F, 0xFADA, 0xFAFF, 0xFB07, 0xFB12, 0xFB18, 0xFB1C,
    0xFB37, 0xFB37, 0xFB3D, 0xFB3D, 0xFB3F, 0xFB3F, 0xFB42, 0xFB42, 0xFB45, 0xFB45, 0xFBC3, 0xFBD2, 0xFD90, 0xFD91,
    0xFDC8, 0xFDCE, 0xFDD0, 0xFDEF, 0xFE1A, 0xFE1F, 0xFE53, 0xFE53, 0xFE67, 0xFE67, 0xFE6C, 0xFE6F, 0xFE75, 0xFE75,
    0xFEFD, 0xFF00, 0xFFBF, 0xFFC1, 0xFFC8, 0xFFC9, 0xFFD0, 0xFFD1, 0xFFD8, 0xFFD9, 0xFFDD, 0xFFDF, 0xFFE7, 0xFFE7,
    0xFFEF, 0xFFF8, 0xFFFE, 0x10FFFF,
];

/// The scalar values `JavaScriptEncoder.Default` escapes: the controls, `"&'+<>\` and the backquote, and everything
/// past ASCII.
#[rustfmt::skip]
const DEFAULT: &[u32] = &[
    0x0, 0x1F, 0x22, 0x22, 0x26, 0x27, 0x2B, 0x2B, 0x3C, 0x3C, 0x3E, 0x3E, 0x5C, 0x5C, 0x60, 0x60, 0x7F, 0x10FFFF,
];

impl Encoder {
    fn table(self) -> &'static [[u32; 2]] {
        match self {
            Encoder::Relaxed => RELAXED.as_chunks::<2>().0,
            Encoder::Default => DEFAULT.as_chunks::<2>().0,
        }
    }

    fn escapes(self, c: char) -> bool {
        let c = u32::from(c);
        self.table()
            .binary_search_by(|&[first, last]| {
                if last < c {
                    Ordering::Less
                } else if first > c {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            })
            .is_ok()
    }
}

// ------------------------------------------------------------------ writing
/// `node.ToJsonString(new JsonSerializerOptions { WriteIndented = true, NewLine = newline, Encoder =
/// UnsafeRelaxedJsonEscaping })`: two spaces a level, `"name": value`, and `{}` or `[]` for an empty one. Numbers are
/// written as they were read.
pub(crate) fn indented(node: &Node, newline: &str) -> String {
    let mut out = String::new();
    write(node, Encoder::Relaxed, newline, 0, &mut out);
    out
}

/// `JsonNode.ToString`: a string as it is, anything else as JSON, indented and escaped by the default encoder, with
/// `newline` (`Environment.NewLine`).
pub(crate) fn to_string(node: &Node, newline: &str) -> String {
    match node {
        Node::String(s) => s.clone(),
        _ => {
            let mut out = String::new();
            write(node, Encoder::Default, newline, 0, &mut out);
            out
        }
    }
}

fn write(node: &Node, encoder: Encoder, newline: &str, depth: usize, out: &mut String) {
    let line = |depth: usize, out: &mut String| {
        out.push_str(newline);
        for _ in 0..depth {
            out.push_str("  ");
        }
    };
    match node {
        Node::Null => out.push_str("null"),
        Node::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Node::Number(n) => out.push_str(n),
        Node::String(s) => write_string(s, encoder, out),
        Node::Array(items) if items.is_empty() => out.push_str("[]"),
        Node::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                line(depth + 1, out);
                write(item, encoder, newline, depth + 1, out);
            }
            line(depth, out);
            out.push(']');
        }
        Node::Object(object) if object.iter().len() == 0 => out.push_str("{}"),
        Node::Object(object) => {
            out.push('{');
            for (i, (key, value)) in object.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                line(depth + 1, out);
                write_string(key, encoder, out);
                out.push_str(": ");
                write(value, encoder, newline, depth + 1, out);
            }
            line(depth, out);
            out.push('}');
        }
    }
}

/// A string as the encoder writes it: what it escapes as `\b \t \n \f \r \\` (and `\"` when relaxed), or as its
/// UTF-16 units, `\uXXXX` in upper case.
fn write_string(s: &str, encoder: Encoder, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        if !encoder.escapes(c) {
            out.push(c);
            continue;
        }
        match c {
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            '"' if encoder == Encoder::Relaxed => out.push_str("\\\""),
            _ => {
                for unit in c.encode_utf16(&mut [0; 2]) {
                    let _ = write!(out, "\\u{unit:04X}");
                }
            }
        }
    }
    out.push('"');
}

// ------------------------------------------------------------------ reading a node
/// `ArgumentException`'s message when an object names a member twice: `JsonObject` reads its members into a
/// dictionary the first time it is used, and throws there.
pub(crate) fn duplicate(key: &str) -> String {
    format!("An item with the same key has already been added. Key: {key} (Parameter 'key')")
}

/// An object as `JsonObject` first reads it: it throws when a name comes twice.
pub(crate) fn read(object: &Object) -> Result<(), String> {
    object.duplicate().map_or(Ok(()), |key| Err(duplicate(key)))
}

/// `node[key]`: the member, or `None` when there is none or it is `null`, as `JsonNode` has it. On anything but an
/// object it throws.
pub(crate) fn member<'a>(node: &'a Node, key: &str) -> Result<Option<&'a Node>, String> {
    let Node::Object(object) = node else {
        return Err(NOT_AN_OBJECT.to_owned());
    };
    read(object)?;
    Ok(object.get(key).filter(|v| **v != Node::Null))
}

/// `JsonNode.DeepEquals(a, b)`: objects equal with their members in any order, arrays in theirs, numbers by value
/// (`1`, `1.0` and `1e0`), and strings once unescaped. An object that names a member twice throws once it is
/// compared, as it is read.
pub(crate) fn deep_equals(a: &Node, b: &Node) -> Result<bool, String> {
    Ok(match (a, b) {
        (Node::Object(a), Node::Object(b)) => {
            read(a)?;
            read(b)?;
            if a.iter().len() != b.iter().len() {
                return Ok(false);
            }
            for (key, value) in a.iter() {
                match b.get(key) {
                    Some(other) if deep_equals(value, other)? => {}
                    _ => return Ok(false),
                }
            }
            true
        }
        (Node::Array(a), Node::Array(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for (x, y) in a.iter().zip(b) {
                if !deep_equals(x, y)? {
                    return Ok(false);
                }
            }
            true
        }
        (Node::Number(x), Node::Number(y)) => number(x) == number(y),
        (Node::String(x), Node::String(y)) => x == y,
        (Node::Bool(x), Node::Bool(y)) => x == y,
        (Node::Null, Node::Null) => true,
        _ => false,
    })
}

/// A JSON number's value as `JsonHelpers.AreEqualJsonNumbers` compares them: its sign, its significant digits, and
/// the power of ten of the last of them. Zero has no sign.
fn number(text: &str) -> (bool, String, i128) {
    let (negative, rest) = text.strip_prefix('-').map_or((false, text), |r| (true, r));
    let (mantissa, exponent) = match rest.find(['e', 'E']) {
        Some(at) => (&rest[..at], exponent(&rest[at + 1..])),
        None => (rest, 0),
    };
    let (integral, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = format!("{integral}{fraction}");
    let significant = digits.trim_start_matches('0');
    let trimmed = significant.trim_end_matches('0');
    if trimmed.is_empty() {
        return (false, String::new(), 0);
    }
    let shift = (significant.len() - trimmed.len()) as i128 - fraction.len() as i128;
    (negative, trimmed.to_owned(), exponent.saturating_add(shift))
}

/// An exponent's value (`[+-]digits`), held at a bound past which no two exponents the file could hold differ.
fn exponent(text: &str) -> i128 {
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let value = digits
        .bytes()
        .fold(0i128, |v, d| (v * 10 + i128::from(d - b'0')).min(i128::from(u64::MAX)));
    if negative { -value } else { value }
}

// ------------------------------------------------------------------ parsing
/// `JsonNode.Parse(text)` with its default options (no comments, no trailing commas, [`MAX_DEPTH`]): the tree, or the
/// message of what it throws.
pub(crate) fn parse(text: &str) -> Result<Node, String> {
    json::parse(text, MAX_DEPTH).map_err(|e| refusal(text).unwrap_or_else(|| e.to_string()))
}

/// Why `Utf8JsonReader` refuses a text, as its `JsonReaderException` says it, with the line (counted from 0) and the
/// byte in it where it stopped. `None` when it reads the text through.
fn refusal(text: &str) -> Option<String> {
    let mut reader = Reader {
        text,
        b: text.as_bytes(),
        consumed: 0,
        line: 0,
        pos: 0,
        open: Vec::new(),
        token: Token::None,
        not_primitive: false,
    };
    loop {
        match reader.read() {
            Ok(true) => {}
            Ok(false) => return None,
            Err(why) => {
                return Some(format!(
                    "{why} LineNumber: {} | BytePositionInLine: {}.",
                    reader.line, reader.pos
                ));
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Token {
    None,
    StartObject,
    StartArray,
    PropertyName,
    End,
    Value,
}

/// `Utf8JsonReader` over one final block, as `JsonDocument.Parse` drives it (`Read` until it returns false), ported
/// for the message and the place of its first error. json.rs builds the tree.
struct Reader<'a> {
    text: &'a str,
    b: &'a [u8],
    consumed: usize,
    line: u64,
    pos: u64,
    /// Whether each open container is an object (`_bitStack`).
    open: Vec<bool>,
    token: Token,
    /// The first token opened an object or an array (`_isNotPrimitive`).
    not_primitive: bool,
}

/// `JsonConstants.Delimiters`: what may end a number.
const DELIMITERS: &[u8] = b",}] \n\r\t/";

const END_OF_DATA_IN_NUMBER: &str = "Expected a digit ('0'-'9'), but instead reached end of data.";
const OPEN_STRING: &str = "Expected end of string, but instead reached end of data.";

/// A byte as the messages show it: itself when it is printable ASCII, else `0xXX`.
fn shown(b: u8) -> String {
    if (0x20..0x7F).contains(&b) {
        char::from(b).to_string()
    } else {
        format!("0x{b:02X}")
    }
}

fn after_sign(b: u8) -> String {
    format!(
        "'{}' is invalid within a number, immediately after a sign character ('+' or '-'). Expected a digit ('0'-'9').",
        shown(b)
    )
}

fn end_of_number(b: u8) -> String {
    format!("'{}' is an invalid end of a number. Expected a delimiter.", shown(b))
}

fn no_property_name(b: u8) -> String {
    format!(
        "'{}' is an invalid start of a property name. Expected a '\"'.",
        shown(b)
    )
}

impl Reader<'_> {
    fn in_object(&self) -> bool {
        self.open.last().copied().unwrap_or(false)
    }

    fn step(&mut self, n: usize) {
        self.consumed += n;
        self.pos += n as u64;
    }

    fn read(&mut self) -> Result<bool, String> {
        let read = self.read_single_segment()?;
        if !read && self.token == Token::None {
            return Err(
                "The input does not contain any JSON tokens. Expected the input to start with a valid JSON \
                        token, when isFinalBlock is true."
                    .to_owned(),
            );
        }
        Ok(read)
    }

    fn read_single_segment(&mut self) -> Result<bool, String> {
        if !self.has_more_data()? {
            return Ok(false);
        }
        let mut first = self.b[self.consumed];
        if first <= b' ' {
            self.skip_whitespace();
            if !self.has_more_data()? {
                return Ok(false);
            }
            first = self.b[self.consumed];
        }
        match self.token {
            Token::None => self.read_first_token(first),
            _ if first == b'/' => self.consume_next_token(first),
            Token::StartObject if first == b'}' => self.end_container(true),
            Token::StartObject if first != b'"' => Err(no_property_name(first)),
            Token::StartObject => self.consume_property_name(),
            Token::StartArray if first == b']' => self.end_container(false),
            Token::StartArray | Token::PropertyName => self.consume_value(first),
            Token::End | Token::Value => self.consume_next_token(first),
        }
    }

    /// `HasMoreData()`: at the end, a text that opened an object or an array must have closed it.
    fn has_more_data(&self) -> Result<bool, String> {
        if self.consumed < self.b.len() {
            return Ok(true);
        }
        if self.not_primitive && !self.open.is_empty() {
            return Err(
                "Expected depth to be zero at the end of the JSON payload. There is an open JSON object or \
                        array that should be closed."
                    .to_owned(),
            );
        }
        Ok(false)
    }

    /// `HasMoreData(resource)`: more must follow.
    fn more_or(&self, why: &str) -> Result<(), String> {
        if self.consumed < self.b.len() {
            Ok(())
        } else {
            Err(why.to_owned())
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(&c) = self.b.get(self.consumed) {
            match c {
                b'\n' => {
                    self.line += 1;
                    self.pos = 0;
                }
                b' ' | b'\r' | b'\t' => self.pos += 1,
                _ => break,
            }
            self.consumed += 1;
        }
    }

    fn read_first_token(&mut self, first: u8) -> Result<bool, String> {
        match first {
            b'{' | b'[' => {
                self.open.push(first == b'{');
                self.token = if first == b'{' {
                    Token::StartObject
                } else {
                    Token::StartArray
                };
                self.step(1);
                self.not_primitive = true;
            }
            b'0'..=b'9' | b'-' => {
                let n = self.number()?;
                self.token = Token::Value;
                self.step(n);
            }
            _ => {
                self.consume_value(first)?;
            }
        }
        Ok(true)
    }

    fn consume_value(&mut self, marker: u8) -> Result<bool, String> {
        match marker {
            b'"' => self.consume_string()?,
            b'{' | b'[' => self.start_container(marker == b'{')?,
            b'0'..=b'9' | b'-' => {
                let n = self.number()?;
                self.token = Token::Value;
                self.step(n);
                if self.consumed >= self.b.len() && self.not_primitive {
                    return Err(end_of_number(self.b[self.consumed - 1]));
                }
            }
            b'f' => self.consume_literal("false")?,
            b't' => self.consume_literal("true")?,
            b'n' => self.consume_literal("null")?,
            _ => return Err(format!("'{}' is an invalid start of a value.", shown(marker))),
        }
        Ok(true)
    }

    fn start_container(&mut self, object: bool) -> Result<(), String> {
        if self.open.len() >= MAX_DEPTH {
            let what = if object { "object" } else { "array" };
            return Err(format!(
                "The maximum configured depth of {MAX_DEPTH} has been exceeded. Cannot read next JSON {what}."
            ));
        }
        self.open.push(object);
        self.step(1);
        self.token = if object { Token::StartObject } else { Token::StartArray };
        Ok(())
    }

    /// `EndObject` or `EndArray`.
    fn end_container(&mut self, object: bool) -> Result<bool, String> {
        if self.open.is_empty() || self.in_object() != object {
            let c = if object { '}' } else { ']' };
            return Err(format!("'{c}' is invalid without a matching open."));
        }
        self.token = Token::End;
        self.step(1);
        self.open.pop();
        Ok(true)
    }

    /// `ConsumeNextToken`: what may follow a value or the end of a container, a comma and the next member or item,
    /// or an end.
    fn consume_next_token(&mut self, marker: u8) -> Result<bool, String> {
        const NOTHING_AFTER_COMMA: &str =
            "Expected start of a property name or value, but instead reached end of data.";
        if self.open.is_empty() {
            return Err(format!(
                "'{}' is invalid after a single JSON value. Expected end of data.",
                shown(marker)
            ));
        }
        match marker {
            b',' => {
                self.step(1);
                if self.consumed >= self.b.len() {
                    self.consumed -= 1;
                    self.pos -= 1;
                    return Err(NOTHING_AFTER_COMMA.to_owned());
                }
                let mut first = self.b[self.consumed];
                if first <= b' ' {
                    self.skip_whitespace();
                    self.more_or(NOTHING_AFTER_COMMA)?;
                    first = self.b[self.consumed];
                }
                match (self.in_object(), first) {
                    (true, b'"') => self.consume_property_name(),
                    (true, b'}') => Err(
                        "The JSON object contains a trailing comma at the end which is not supported \
                                         in this mode. Change the reader options."
                            .to_owned(),
                    ),
                    (true, _) => Err(no_property_name(first)),
                    (false, b']') => Err(
                        "The JSON array contains a trailing comma at the end which is not supported \
                                          in this mode. Change the reader options."
                            .to_owned(),
                    ),
                    (false, _) => self.consume_value(first),
                }
            }
            b'}' | b']' => self.end_container(marker == b'}'),
            _ => Err(format!(
                "'{}' is invalid after a value. Expected either ',', '}}', or ']'.",
                shown(marker)
            )),
        }
    }

    fn consume_property_name(&mut self) -> Result<bool, String> {
        const NO_VALUE: &str = "Expected a value, but instead reached end of data.";
        self.consume_string()?;
        self.more_or(NO_VALUE)?;
        let mut first = self.b[self.consumed];
        if first <= b' ' {
            self.skip_whitespace();
            self.more_or(NO_VALUE)?;
            first = self.b[self.consumed];
        }
        if first != b':' {
            return Err(format!(
                "'{}' is invalid after a property name. Expected a ':'.",
                shown(first)
            ));
        }
        self.step(1);
        self.token = Token::PropertyName;
        Ok(true)
    }

    /// `ConsumeString`, and `ConsumeStringAndValidate` once an escape or a control character comes.
    fn consume_string(&mut self) -> Result<(), String> {
        let data = &self.b[self.consumed + 1..];
        let Some(first) = data.iter().position(|&c| c == b'"' || c == b'\\' || c < b' ') else {
            self.pos += data.len() as u64 + 1;
            return Err(OPEN_STRING.to_owned());
        };
        if data[first] == b'"' {
            self.step(first + 2);
            self.token = Token::Value;
            return Ok(());
        }
        self.pos += first as u64 + 1;
        let mut escaped = false;
        let mut i = first;
        while i < data.len() {
            let c = data[i];
            if c == b'"' {
                if !escaped {
                    self.pos += 1;
                    self.consumed += i + 2;
                    self.token = Token::Value;
                    return Ok(());
                }
                escaped = false;
            } else if c == b'\\' {
                escaped = !escaped;
            } else if escaped {
                if !b"\"nrt/ubf\\".contains(&c) {
                    return Err(format!(
                        "'{}' is an invalid escapable character within a JSON string. The string should be correctly \
                         escaped.",
                        shown(c)
                    ));
                }
                if c == b'u' {
                    self.pos += 1;
                    if !self.hex_digits(data, i + 1)? {
                        break;
                    }
                    i += 4;
                }
                escaped = false;
            } else if c < b' ' {
                return Err(format!(
                    "'{}' is invalid within a JSON string. The string should be correctly escaped.",
                    shown(c)
                ));
            }
            self.pos += 1;
            i += 1;
        }
        Err(OPEN_STRING.to_owned())
    }

    /// `ValidateHexDigits`: the 4 digits of a `\u` escape from `at`, or false when the text ends first.
    fn hex_digits(&mut self, data: &[u8], at: usize) -> Result<bool, String> {
        for (j, &c) in data.iter().enumerate().skip(at) {
            if !c.is_ascii_hexdigit() {
                return Err(format!(
                    "'{}' is not a hex digit following '\\u' within a JSON string. The string should be correctly \
                     escaped.",
                    shown(c)
                ));
            }
            if j - at >= 3 {
                return Ok(true);
            }
            self.pos += 1;
        }
        Ok(false)
    }

    /// `ConsumeLiteral`: the message shows the text from the literal's start to the end.
    fn consume_literal(&mut self, literal: &str) -> Result<(), String> {
        let span = &self.b[self.consumed..];
        if span.starts_with(literal.as_bytes()) {
            self.step(literal.len());
            self.token = Token::Value;
            return Ok(());
        }
        let at = (1..literal.len())
            .find(|&i| span.get(i) != Some(&literal.as_bytes()[i]))
            .unwrap_or(literal.len());
        self.pos += at as u64;
        Err(format!(
            "'{}' is an invalid JSON literal. Expected the literal '{literal}'.",
            &self.text[self.consumed..]
        ))
    }

    /// `TryGetNumber`: how many bytes the number that starts here takes. `pos` is at its start; an error moves it to
    /// where the number went wrong.
    fn number(&mut self) -> Result<usize, String> {
        let data = &self.b[self.consumed..];
        let fail = |pos: &mut u64, i: usize, why: String| {
            *pos += i as u64;
            Err(why)
        };
        // the digits from i on, which end the number when a delimiter or the end follows them
        let digits_end_it = |i: &mut usize| {
            while data.get(*i).is_some_and(u8::is_ascii_digit) {
                *i += 1;
            }
            data.get(*i).is_none_or(|c| DELIMITERS.contains(c))
        };
        let mut i = 0;
        if data[0] == b'-' {
            i = 1;
            match data.get(i) {
                None => return fail(&mut self.pos, i, END_OF_DATA_IN_NUMBER.to_owned()),
                Some(&c) if !c.is_ascii_digit() => return fail(&mut self.pos, i, after_sign(c)),
                Some(_) => {}
            }
        }
        let mut next;
        if data[i] == b'0' {
            i += 1;
            match data.get(i) {
                None => return Ok(i),
                Some(c) if DELIMITERS.contains(c) => return Ok(i),
                Some(&c) => next = c,
            }
            if !matches!(next, b'.' | b'E' | b'e') {
                let why = if next.is_ascii_digit() {
                    format!("Invalid leading zero before '{}'.", shown(next))
                } else {
                    end_of_number(next)
                };
                return fail(&mut self.pos, i, why);
            }
        } else {
            i += 1;
            if digits_end_it(&mut i) {
                return Ok(i);
            }
            next = data[i];
            if !matches!(next, b'.' | b'E' | b'e') {
                return fail(&mut self.pos, i, end_of_number(next));
            }
        }
        if next == b'.' {
            i += 1;
            match data.get(i) {
                None => return fail(&mut self.pos, i, END_OF_DATA_IN_NUMBER.to_owned()),
                Some(&c) if !c.is_ascii_digit() => {
                    let why = format!(
                        "'{}' is invalid within a number, immediately after a decimal point ('.'). Expected a digit \
                         ('0'-'9').",
                        shown(c)
                    );
                    return fail(&mut self.pos, i, why);
                }
                Some(_) => {}
            }
            i += 1;
            if digits_end_it(&mut i) {
                return Ok(i);
            }
            next = data[i];
            if next != b'E' && next != b'e' {
                let why = format!("'{}' is an invalid end of a number. Expected 'E' or 'e'.", shown(next));
                return fail(&mut self.pos, i, why);
            }
        }
        // the exponent, and its sign
        i += 1;
        let Some(&c) = data.get(i) else {
            return fail(&mut self.pos, i, END_OF_DATA_IN_NUMBER.to_owned());
        };
        next = c;
        if next == b'+' || next == b'-' {
            i += 1;
            let Some(&c) = data.get(i) else {
                return fail(&mut self.pos, i, END_OF_DATA_IN_NUMBER.to_owned());
            };
            next = c;
        }
        if !next.is_ascii_digit() {
            return fail(&mut self.pos, i, after_sign(next));
        }
        i += 1;
        if digits_end_it(&mut i) {
            return Ok(i);
        }
        let why = end_of_number(data[i]);
        fail(&mut self.pos, i, why)
    }
}

#[cfg(test)]
mod tests {
    //! Replays tests/golden/registration/json.json (see the module's doc).

    use std::path::Path;

    use serde_json::Value;

    use super::*;

    fn golden() -> Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/registration/json.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{}: {e} (write it with: dotnet run --project rust/golden -c Release -- registration)",
                path.display()
            )
        });
        serde_json::from_str(&text).unwrap()
    }

    fn text(v: &Value) -> &str {
        v.as_str().unwrap_or_else(|| panic!("not a string: {v}"))
    }

    /// Each encoder escapes what System.Text.Json's does: the same ranges, and the lookup finds each one's ends and
    /// not what lies just outside.
    #[test]
    fn each_encoder_escapes_what_system_text_json_escapes() {
        let golden = golden();
        for (encoder, name) in [(Encoder::Relaxed, "relaxed"), (Encoder::Default, "default")] {
            let ranges: Vec<[u32; 2]> = golden["escaped"][name]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| [0, 1].map(|i| r[i].as_u64().unwrap() as u32))
                .collect();
            assert_eq!(encoder.table(), ranges.as_slice(), "{name}");
            for &[first, last] in encoder.table() {
                for c in [first, last, first.wrapping_sub(1), last + 1] {
                    let Some(c) = char::from_u32(c) else { continue };
                    let inside = ranges.iter().any(|&[f, l]| (f..=l).contains(&u32::from(c)));
                    assert_eq!(encoder.escapes(c), inside, "{name} U+{:04X}", u32::from(c));
                }
            }
        }
    }

    #[test]
    fn escapes_are_written_as_system_text_json_writes_them() {
        for case in golden()["escapes"].as_array().unwrap() {
            let c = char::from_u32(case["char"].as_u64().unwrap() as u32).unwrap();
            for (encoder, name) in [(Encoder::Relaxed, "relaxed"), (Encoder::Default, "default")] {
                let mut out = String::new();
                write_string(&c.to_string(), encoder, &mut out);
                assert_eq!(out, text(&case[name]), "{name} U+{:04X}", u32::from(c));
            }
        }
    }

    #[test]
    fn indented_is_what_system_text_json_writes() {
        for case in golden()["writes"].as_array().unwrap() {
            let node = parse(text(&case["text"])).unwrap();
            assert_eq!(indented(&node, "\n"), text(&case["lf"]), "{}", case["text"]);
            assert_eq!(indented(&node, "\r\n"), text(&case["crlf"]), "{}", case["text"]);
        }
    }

    #[test]
    fn to_string_is_json_nodes() {
        for case in golden()["to_string"].as_array().unwrap() {
            let node = parse(text(&case["text"])).unwrap();
            assert_eq!(to_string(&node, "\n"), text(&case["string"]), "{}", case["text"]);
        }
    }

    #[test]
    fn deep_equals_is_json_nodes() {
        for case in golden()["deep_equals"].as_array().unwrap() {
            let (a, b) = (parse(text(&case["a"])).unwrap(), parse(text(&case["b"])).unwrap());
            let expected = match case.get("equal") {
                Some(equal) => Ok(equal.as_bool().unwrap()),
                None => Err(text(&case["error"])
                    .trim_start_matches("ArgumentException: ")
                    .to_owned()),
            };
            assert_eq!(deep_equals(&a, &b), expected, "{} {}", case["a"], case["b"]);
        }
    }

    /// What JsonNode.Parse refuses is refused with its message; what it reads, json.rs reads, and reading its member
    /// x throws where the C# throws.
    #[test]
    fn parse_refuses_as_json_node_parse_does() {
        for case in golden()["parse"].as_array().unwrap() {
            let source = text(&case["text"]);
            let parsed = parse(source);
            if !case["parsed"].as_bool().unwrap() {
                let expected = text(&case["error"]).trim_start_matches("JsonReaderException: ");
                assert_eq!(parsed.as_ref().err().map(String::as_str), Some(expected), "{source:?}");
                continue;
            }
            assert_eq!(refusal(source), None, "{source:?}");
            let node = parsed.unwrap_or_else(|e| panic!("{source:?}: {e}"));
            // JsonNode.Parse("null") is null, which isn't read
            if node == Node::Null {
                continue;
            }
            let x = member(&node, "x").map(|x| x.map_or("(none)", |_| "(value)"));
            let expected = match (case.get("x"), case.get("x_error")) {
                (Some(x), _) if text(x) == "(none)" => Ok("(none)"),
                (Some(_), _) => Ok("(value)"),
                (None, Some(e)) => Err(text(e).split_once(": ").unwrap().1.to_owned()),
                (None, None) => panic!("{source:?}: no x in the golden"),
            };
            assert_eq!(x, expected, "{source:?}");
        }
    }
}
