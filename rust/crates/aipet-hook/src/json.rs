//! A JSON tree that keeps an event as it came: read as System.Text.Json's `JsonNode.Parse` reads text, and written
//! as the hook's request line (`src/AiPet.Hook/Program.cs:176-187`).
//!
//! The hook changes little of an event (it cuts long strings and drops `tool_response`) and passes the rest on as it
//! came: members in their order, and numbers as their text (`1.0`, `-0` and `1e400` stay as they are written). The
//! C#'s `JsonNode` keeps both, and the pet tells tool calls apart by their input's JSON text (`AgentSessions.CallKey`),
//! so both matter. serde_json's `Value` sorts the members and reads numbers into doubles, so it isn't used here.
//!
//! The grammar is RFC 8259's, as System.Text.Json's defaults read it: no comments, no trailing commas, and at most
//! `depth` objects and arrays inside each other. An object may name a member twice, as it may for `JsonNode.Parse`,
//! which fails only once that object is read ([`Object::duplicate`]).
//!
//! Strings are written with the escapes JSON requires and nothing more: non-ASCII text as UTF-8. The C# writes
//! characters outside the BMP (and a few others) as `\uXXXX` escapes instead; the pet reads both the same.

use std::fmt;

/// A JSON value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Node {
    Null,
    Bool(bool),
    /// The number's text, as it came.
    Number(String),
    String(String),
    Array(Vec<Node>),
    Object(Object),
}

/// An object's members, in their order.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Object {
    members: Vec<(String, Node)>,
}

impl Object {
    /// The first member named `key`.
    pub(crate) fn get(&self, key: &str) -> Option<&Node> {
        self.members.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub(crate) fn get_mut(&mut self, key: &str) -> Option<&mut Node> {
        self.members.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// `Program.Str`: the member's value when it is a string.
    pub(crate) fn str(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(Node::String(s)) => Some(s),
            _ => None,
        }
    }

    /// Adds a member at the end (the caller knows the name is new).
    pub(crate) fn push(&mut self, key: impl Into<String>, value: Node) {
        self.members.push((key.into(), value));
    }

    /// Removes the first member named `key`.
    pub(crate) fn remove(&mut self, key: &str) -> Option<Node> {
        let at = self.members.iter().position(|(k, _)| k == key)?;
        Some(self.members.remove(at).1)
    }

    /// Keeps the members whose names `keep` accepts, in their order.
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.members.retain(|(k, _)| keep(k));
    }

    #[cfg(test)]
    pub(crate) fn keys(&self) -> impl Iterator<Item = &str> {
        self.members.iter().map(|(k, _)| k.as_str())
    }

    /// The members, in their order.
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &Node)> {
        self.members.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut Node> {
        self.members.iter_mut().map(|(_, v)| v)
    }

    /// The first name that a later member has too. System.Text.Json's `JsonObject` reads its members into a
    /// dictionary the first time it is used, and throws there when a name comes twice.
    pub(crate) fn duplicate(&self) -> Option<&str> {
        let mut seen = std::collections::HashSet::with_capacity(self.members.len());
        self.members.iter().map(|(k, _)| k.as_str()).find(|k| !seen.insert(*k))
    }

    /// The object as one line of JSON.
    pub(crate) fn to_json(&self) -> String {
        let mut out = String::new();
        write_object(self, &mut out);
        out
    }
}

/// Why a text isn't JSON, and where: the byte it was found at.
#[derive(Debug, PartialEq)]
pub(crate) struct Error {
    at: usize,
    what: &'static str,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.what, self.at)
    }
}

/// `JsonNode.Parse(text)` with at most `depth` objects and arrays inside each other (System.Text.Json's `MaxDepth`).
pub(crate) fn parse(text: &str, depth: usize) -> Result<Node, Error> {
    let mut p = Parser {
        text,
        bytes: text.as_bytes(),
        at: 0,
        depth_left: depth,
    };
    p.whitespace();
    let value = p.value()?;
    p.whitespace();
    if p.at < p.bytes.len() {
        return Err(p.error("text after the JSON value"));
    }
    Ok(value)
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
    depth_left: usize,
}

impl Parser<'_> {
    fn error(&self, what: &'static str) -> Error {
        Error { at: self.at, what }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// Takes `b` when it comes next.
    fn eat(&mut self, b: u8) -> bool {
        let next = self.peek() == Some(b);
        if next {
            self.at += 1;
        }
        next
    }

    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn value(&mut self) -> Result<Node, Error> {
        match self.peek() {
            Some(b'{') => self.nested(Self::object),
            Some(b'[') => self.nested(Self::array),
            Some(b'"') => self.string().map(Node::String),
            Some(b't') => self.literal("true", Node::Bool(true)),
            Some(b'f') => self.literal("false", Node::Bool(false)),
            Some(b'n') => self.literal("null", Node::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("not a JSON value")),
            None => Err(self.error("the text ends before a value")),
        }
    }

    /// An object or an array, one level deeper.
    fn nested(&mut self, read: fn(&mut Self) -> Result<Node, Error>) -> Result<Node, Error> {
        if self.depth_left == 0 {
            return Err(self.error("nested deeper than the depth allowed"));
        }
        self.depth_left -= 1;
        let node = read(self)?;
        self.depth_left += 1;
        Ok(node)
    }

    fn object(&mut self) -> Result<Node, Error> {
        self.at += 1;
        let mut object = Object::default();
        self.whitespace();
        if self.eat(b'}') {
            return Ok(Node::Object(object));
        }
        loop {
            self.whitespace();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a member name"));
            }
            let key = self.string()?;
            self.whitespace();
            if !self.eat(b':') {
                return Err(self.error("expected ':'"));
            }
            self.whitespace();
            let value = self.value()?;
            object.push(key, value);
            self.whitespace();
            if self.eat(b'}') {
                return Ok(Node::Object(object));
            }
            if !self.eat(b',') {
                return Err(self.error("expected ',' or '}'"));
            }
        }
    }

    fn array(&mut self) -> Result<Node, Error> {
        self.at += 1;
        let mut items = Vec::new();
        self.whitespace();
        if self.eat(b']') {
            return Ok(Node::Array(items));
        }
        loop {
            self.whitespace();
            items.push(self.value()?);
            self.whitespace();
            if self.eat(b']') {
                return Ok(Node::Array(items));
            }
            if !self.eat(b',') {
                return Err(self.error("expected ',' or ']'"));
            }
        }
    }

    fn literal(&mut self, word: &'static str, node: Node) -> Result<Node, Error> {
        if !self.bytes[self.at..].starts_with(word.as_bytes()) {
            return Err(self.error("not a JSON value"));
        }
        self.at += word.len();
        Ok(node)
    }

    /// `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`, kept as its text.
    fn number(&mut self) -> Result<Node, Error> {
        let start = self.at;
        self.eat(b'-');
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(self.error("a number without digits")),
        }
        if self.eat(b'.') && !self.digits() {
            return Err(self.error("a number without digits after its '.'"));
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if !self.digits() {
                return Err(self.error("a number without digits in its exponent"));
            }
        }
        Ok(Node::Number(self.text[start..self.at].to_owned()))
    }

    /// Takes the digits that come next; whether there was one.
    fn digits(&mut self) -> bool {
        let start = self.at;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        self.at > start
    }

    fn string(&mut self) -> Result<String, Error> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let run = self.at;
            while matches!(self.peek(), Some(b) if b != b'"' && b != b'\\' && b >= 0x20) {
                self.at += 1;
            }
            // the run ends at an ASCII byte (or the end), so on a character's boundary
            out.push_str(&self.text[run..self.at]);
            match self.peek() {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => self.escape(&mut out)?,
                Some(_) => return Err(self.error("a control character in a string")),
                None => return Err(self.error("the text ends in a string")),
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<(), Error> {
        let c = match self.bytes.get(self.at + 1) {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                self.at += 2;
                let unit = self.hex()?;
                out.push(self.unit(unit)?);
                return Ok(());
            }
            _ => return Err(self.error("an invalid escape in a string")),
        };
        self.at += 2;
        out.push(c);
        Ok(())
    }

    /// The character a `\uXXXX` escape starts, with the low half of a surrogate pair from the escape after it. A
    /// lone half is U+FFFD (the hook mends those before it parses, see `event::mended`).
    fn unit(&mut self, unit: u16) -> Result<char, Error> {
        if !(0xD800..0xDC00).contains(&unit) {
            return Ok(char::from_u32(u32::from(unit)).unwrap_or(char::REPLACEMENT_CHARACTER));
        }
        if !self.bytes[self.at..].starts_with(b"\\u") {
            return Ok(char::REPLACEMENT_CHARACTER);
        }
        let back = self.at;
        self.at += 2;
        let low = self.hex()?;
        if !(0xDC00..0xE000).contains(&low) {
            // not the pair's other half: read as an escape of its own
            self.at = back;
            return Ok(char::REPLACEMENT_CHARACTER);
        }
        let c = 0x10000 + ((u32::from(unit) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
        Ok(char::from_u32(c).unwrap_or(char::REPLACEMENT_CHARACTER))
    }

    /// The UTF-16 code unit of the 4 hex digits that come next.
    fn hex(&mut self) -> Result<u16, Error> {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .ok_or_else(|| self.error("a short \\u escape"))?;
        let mut unit = 0u16;
        for &d in digits {
            let v = (d as char)
                .to_digit(16)
                .ok_or_else(|| self.error("a \\u escape that isn't hex"))?;
            unit = unit << 4 | v as u16;
        }
        self.at += 4;
        Ok(unit)
    }
}

fn write(node: &Node, out: &mut String) {
    match node {
        Node::Null => out.push_str("null"),
        Node::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Node::Number(n) => out.push_str(n),
        Node::String(s) => write_string(s, out),
        Node::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(item, out);
            }
            out.push(']');
        }
        Node::Object(object) => write_object(object, out),
    }
}

fn write_object(object: &Object, out: &mut String) {
    out.push('{');
    for (i, (key, value)) in object.members.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_string(key, out);
        out.push(':');
        write(value, out);
    }
    out.push('}');
}

/// A string with the escapes JSON requires: the quote, the backslash and the control characters.
fn write_string(s: &str, out: &mut String) {
    out.push('"');
    let mut run = 0;
    for (i, b) in s.bytes().enumerate() {
        let escaped = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0x08 => "\\b",
            0x0C => "\\f",
            0..0x20 => "",
            _ => continue,
        };
        out.push_str(&s[run..i]);
        if escaped.is_empty() {
            out.push_str(&format!("\\u{b:04X}"));
        } else {
            out.push_str(escaped);
        }
        run = i + 1;
    }
    out.push_str(&s[run..]);
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A text read and written back.
    fn round_trip(text: &str) -> String {
        let mut out = String::new();
        write(&parse(text, 64).unwrap(), &mut out);
        out
    }

    /// What System.Text.Json accepts is kept as it came (the C# hook sends these back the same, but for its own
    /// string escapes), with the whitespace between tokens left out.
    #[test]
    fn values_are_kept_as_they_came() {
        for (text, written) in [
            (r#"{"b":1,"a":2}"#, r#"{"b":1,"a":2}"#),
            (
                " {\"n\":1e400,\"m\":-0,\"k\":1.0E+2,\"i\":123456789012345678901234567890} ",
                "",
            ),
            (r#"{"x":[true,false,null,{},[],"",0.5e-3]}"#, ""),
            ("{ \"a\" :\t[ 1 ,\r\n2 ] }", r#"{"a":[1,2]}"#),
            (r#"{"":1}"#, ""),
            ("[1,2]", ""),
            ("\"x\"", ""),
            ("-12.5", ""),
            (r#"{"a":1,"a":2}"#, ""),
        ] {
            let expected = if written.is_empty() { text.trim() } else { written };
            assert_eq!(round_trip(text), expected, "{text}");
        }
    }

    #[test]
    fn strings_are_unescaped_and_written_with_json_escapes_only() {
        let text = r#""q\"\\\/\b\f\n\r\t\u0041\u00e9\u00E9\uD83D\ude00\u0000\u001f\u2028 é😀<>&'""#;
        let Node::String(s) = parse(text, 64).unwrap() else {
            panic!()
        };
        assert_eq!(s, "q\"\\/\u{8}\u{c}\n\r\tAéé😀\u{0}\u{1f}\u{2028} é😀<>&'");
        let mut out = String::new();
        write_string(&s, &mut out);
        assert_eq!(out, "\"q\\\"\\\\/\\b\\f\\n\\r\\tAéé😀\\u0000\\u001F\u{2028} é😀<>&'\"");
        // a lone half of a surrogate pair (the hook mends these before, so only in case one gets through)
        for (lone, read) in [
            (r#""\ud800""#, "\u{fffd}"),
            (r#""\udc00x""#, "\u{fffd}x"),
            (r#""\ud800\u0041""#, "\u{fffd}A"),
        ] {
            assert_eq!(parse(lone, 64).unwrap(), Node::String(read.into()), "{lone}");
        }
    }

    /// What System.Text.Json refuses (the probe of the C# hook in the task's notes found the same).
    #[test]
    fn what_isnt_json_is_refused() {
        for text in [
            "",
            "   ",
            "{not json",
            "{\"a\":1} x",
            "{\"a\":1/*c*/}",
            "{\"a\":1,}",
            "[1,]",
            "{'a':1}",
            "{\"a\":01}",
            "{\"a\":-}",
            "{\"a\":1.}",
            "{\"a\":.5}",
            "{\"a\":1e}",
            "{\"a\":+1}",
            "{\"a\":NaN}",
            "{\"a\":tru}",
            "{\"a\":\"x\\x\"}",
            "{\"a\":\"\\u12\"}",
            "{\"a\":\"\\u12g4\"}",
            "{\"a\":\"a\u{1}b\"}",
            "{\"a\":\"open",
            "{\"a\" 1}",
            "{1:1}",
            "[1 2]",
        ] {
            assert!(parse(text, 64).is_err(), "{text:?}");
        }
    }

    /// `depth` objects and arrays inside each other are read, one more is refused.
    #[test]
    fn depth_is_limited() {
        let nested = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
        assert!(parse(&nested(64), 64).is_ok());
        assert_eq!(parse(&nested(65), 64).unwrap_err().at, 64);
        assert!(parse(&format!("{{\"x\":{}}}", nested(63)), 64).is_ok());
        assert!(parse(&format!("{{\"x\":{}}}", nested(64)), 64).is_err());
        // side by side isn't deeper
        assert!(parse(&format!("[{},{}]", nested(63), nested(63)), 64).is_ok());
    }

    #[test]
    fn objects_keep_their_order_and_find_a_name_given_twice() {
        let Node::Object(mut o) = parse(r#"{"c":1,"a":"x","b":2,"a":3}"#, 64).unwrap() else {
            panic!()
        };
        assert_eq!(o.duplicate(), Some("a"));
        assert_eq!(o.str("a"), Some("x"));
        assert_eq!(o.str("c"), None);
        assert_eq!(o.remove("a"), Some(Node::String("x".into())));
        assert_eq!(o.duplicate(), None);
        o.retain(|k| k != "c");
        o.push("d", Node::Null);
        assert_eq!(o.keys().collect::<Vec<_>>(), ["b", "a", "d"]);
        assert_eq!(o.to_json(), r#"{"b":2,"a":3,"d":null}"#);
    }
}
