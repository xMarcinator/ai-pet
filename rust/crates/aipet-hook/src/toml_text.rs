//! The line-based `config.toml` reader the Codex registration edits with (`src/AiPet.Hook/CodexConfig.cs:199-400`).
//!
//! Codex trusts a hook by where it is in the file and by its definition, so the file is never re-serialised: it is
//! split into tables ([`Segment`]), each kept as its lines, and an edit leaves out or adds whole lines. The reader
//! knows just enough TOML for that: table headers, `key = value` lines, and strings (one-line and multi-line, basic
//! and literal), so a `[` or a `#` inside a string isn't taken for a header or a comment.
//!
//! Every decision is on an ASCII character, so the text is walked by byte: a UTF-8 byte of any other character is
//! never one of those, and the C#'s positions (in UTF-16 units) fall on the same characters.

use crate::install::upper;

/// One table of the file: its header line and the lines up to the next header (`Segment`).
#[derive(Debug, Default)]
pub(crate) struct Segment<'a> {
    /// The header line, trimmed, e.g. `[[hooks.Stop.hooks]]`; `None` for the text before the first table.
    pub(crate) header: Option<&'a str>,
    /// The header's key, e.g. `["hooks", "Stop", "hooks"]`; `None` without a header, or when it can't be read.
    pub(crate) table: Option<Vec<String>>,
    /// `[[...]]`: an entry of an array of tables.
    pub(crate) is_array: bool,
    /// A hooks table whose header AiPet can't make sense of.
    pub(crate) unreadable: bool,
    /// The lines, each with its line ending.
    pub(crate) lines: Vec<&'a str>,
    /// The index in `lines` of the last line that isn't blank or a comment.
    pub(crate) last_content: Option<usize>,
    /// The `key = value` lines: their key, their value (a one-line string as `s:` and what it stands for, anything
    /// else as written) and the line itself.
    pub(crate) values: Vec<(Vec<String>, String, &'a str)>,
    /// The event, for `[[hooks.<Event>]]`.
    pub(crate) group: Option<String>,
    /// The event, for `[[hooks.<Event>.hooks]]`.
    pub(crate) handler: Option<String>,
    /// The key, for `[hooks.state.'<key>']`.
    pub(crate) state_key: Option<String>,
    /// A handler's `command`, when it is a one-line string.
    pub(crate) command: Option<String>,
}

/// `ParseToml`: the text as its tables, the text before the first one included (with no header).
pub(crate) fn parse(text: &str) -> Vec<Segment<'_>> {
    let mut segs = vec![Segment::default()];
    // the delimiter of a multi-line string still open at the start of the line
    let mut open: Option<&'static str> = None;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if open.is_none() && t.starts_with('[') {
            let (table, is_array) = header_key(t);
            let mut s = Segment {
                header: Some(t),
                is_array,
                ..Segment::default()
            };
            match table.as_deref() {
                Some([hooks, event]) if is_array && hooks == "hooks" => s.group = Some(event.clone()),
                Some([hooks, event, inner]) if is_array && hooks == "hooks" && inner == "hooks" => {
                    s.handler = Some(event.clone());
                }
                Some([hooks, state, key]) if !is_array && hooks == "hooks" && state == "state" => {
                    s.state_key = Some(key.clone());
                }
                None => s.unreadable = upper(t).contains("HOOKS"),
                Some(table) => {
                    s.unreadable = table[0] == "hooks"
                        && (is_array || table.len() > 2 || (table.len() == 2 && table[1] != "state"));
                }
            }
            s.table = table;
            segs.push(s);
        }
        let seg = segs
            .last_mut()
            .expect("the text before the first table is always there");
        seg.lines.push(line);
        // track strings, so a "[" or "#" inside one (even a multi-line one) isn't taken for a header or a comment
        let in_string = open.is_some();
        let (still_open, comment) = scan_line(line, open);
        open = still_open;
        if in_string || (!t.is_empty() && !t.starts_with('#')) {
            seg.last_content = Some(seg.lines.len() - 1);
        }
        if in_string || t.is_empty() || t.starts_with(['[', '#']) {
            continue;
        }
        let mut i = 0;
        let Some(key) = read_key(line, &mut i) else { continue };
        if line.as_bytes().get(i) != Some(&b'=') {
            continue;
        }
        let end = comment.filter(|&c| c > i).unwrap_or(line.len());
        let value = line[i + 1..end].trim();
        let string = string_value(value);
        if seg.handler.is_some() && key.len() == 1 && key[0] == "command" {
            seg.command.clone_from(&string);
        }
        let value = string.map_or_else(|| value.to_owned(), |s| format!("s:{s}"));
        seg.values.push((key, value, line));
    }
    segs
}

/// `ScanLine`: walks a line and returns the delimiter (`"""` or `'''`) of a multi-line string still open at its end,
/// and where a comment starts on it.
fn scan_line(line: &str, mut open: Option<&'static str>) -> (Option<&'static str>, Option<usize>) {
    let s = line.as_bytes();
    let mut i = 0;
    while i < s.len() {
        if let Some(delim) = open {
            match close_multiline(s, i, delim) {
                Some(end) => {
                    i = end;
                    open = None;
                }
                None => return (open, None),
            }
        } else if s[i] == b'#' {
            return (None, Some(i));
        } else if s[i] == b'"' || s[i] == b'\'' {
            let triple = if s[i] == b'"' { "\"\"\"" } else { "'''" };
            if s[i..].starts_with(triple.as_bytes()) {
                open = Some(triple);
                i += 3;
            } else {
                match skip_string(s, i) {
                    Some(end) => i = end,
                    None => return (None, None),
                }
            }
        } else {
            i += 1;
        }
    }
    (open, None)
}

/// `SkipString`: the index just past a one-line string that starts at `s[i]` (`"basic"` or `'literal'`), or `None`
/// when it isn't closed.
fn skip_string(s: &[u8], i: usize) -> Option<usize> {
    let quote = s[i];
    let mut j = i + 1;
    while j < s.len() {
        if s[j] == b'\\' && quote == b'"' {
            j += 1;
        } else if s[j] == quote {
            return Some(j + 1);
        }
        j += 1;
    }
    None
}

/// `CloseMultiline`: the index just past the delimiter that closes a multi-line string, looking from `s[i]`; `None`
/// when it isn't on this line.
fn close_multiline(s: &[u8], i: usize, delim: &str) -> Option<usize> {
    let quote = delim.as_bytes()[0];
    let mut j = i;
    while j < s.len() {
        if s[j] == b'\\' && quote == b'"' {
            j += 2;
            continue;
        }
        if s[j..].starts_with(delim.as_bytes()) {
            // up to two more quotes are the string's own
            let mut k = j + 3;
            while k < s.len() && k < j + 5 && s[k] == quote {
                k += 1;
            }
            return Some(k);
        }
        j += 1;
    }
    None
}

/// `Ws`: past the spaces and tabs from `s[i]`.
fn ws(s: &[u8], i: &mut usize) {
    while *i < s.len() && (s[*i] == b' ' || s[*i] == b'\t') {
        *i += 1;
    }
}

/// `ReadKey`: a dotted key from `s[i]` (bare or quoted parts, spaces around the dots allowed): `hooks . "Stop"` is
/// `["hooks", "Stop"]`. `i` ends past it and the spaces after it. `None` when there's no well-formed key there.
fn read_key(s: &str, i: &mut usize) -> Option<Vec<String>> {
    let b = s.as_bytes();
    let mut parts = Vec::new();
    loop {
        ws(b, i);
        if *i >= b.len() {
            return None;
        }
        let start = *i;
        if b[start] == b'"' || b[start] == b'\'' {
            // keys can't be multi-line
            if b[start..].starts_with(&[b[start]; 3]) {
                return None;
            }
            let end = skip_string(b, start)?;
            let inner = &s[start + 1..end - 1];
            parts.push(if b[start] == b'"' {
                unescape(inner)?
            } else {
                inner.to_owned()
            });
            *i = end;
        } else {
            while *i < b.len() && (b[*i].is_ascii_alphanumeric() || b[*i] == b'_' || b[*i] == b'-') {
                *i += 1;
            }
            if *i == start {
                return None;
            }
            parts.push(s[start..*i].to_owned());
        }
        ws(b, i);
        if *i < b.len() && b[*i] == b'.' {
            *i += 1;
            continue;
        }
        return Some(parts);
    }
}

/// `HeaderKey`: the key of a table header, e.g. `[[ hooks . "Stop" ]]` is `["hooks", "Stop"]` and an array; `None`
/// when the line isn't a well-formed header.
fn header_key(header: &str) -> (Option<Vec<String>>, bool) {
    let b = header.as_bytes();
    let is_array = header.starts_with("[[");
    let close: &[u8] = if is_array { b"]]" } else { b"]" };
    let mut i = if is_array { 2 } else { 1 };
    let key = read_key(header, &mut i)
        .filter(|_| b[i..].starts_with(close))
        .and_then(|key| {
            i += close.len();
            ws(b, &mut i);
            (i == b.len() || b[i] == b'#').then_some(key)
        });
    (key, is_array)
}

/// `StringValue`: what a one-line TOML string (`"basic"` or `'literal'`) stands for; `None` for anything else.
fn string_value(v: &str) -> Option<String> {
    let b = v.as_bytes();
    if b.len() < 2 || (b[0] != b'"' && b[0] != b'\'') || b.starts_with(&[b[0]; 3]) {
        return None;
    }
    if skip_string(b, 0) != Some(b.len()) {
        return None;
    }
    let inner = &v[1..v.len() - 1];
    if b[0] == b'"' {
        unescape(inner)
    } else {
        Some(inner.to_owned())
    }
}

/// `Unescape`: what a TOML basic string (the part between the quotes) stands for; `None` when it has an escape TOML
/// doesn't know.
fn unescape(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut run = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' {
            i += 1;
            continue;
        }
        out.push_str(&s[run..i]);
        i += 1;
        let c = *b.get(i)?;
        let digits = match c {
            b'x' => 2,
            b'u' => 4,
            b'U' => 8,
            _ => 0,
        };
        if digits > 0 {
            let hex = b
                .get(i + 1..i + 1 + digits)
                .filter(|h| h.iter().all(u8::is_ascii_hexdigit))?;
            let point = u32::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
            out.push(char::from_u32(point)?);
            i += digits + 1;
        } else {
            out.push(match c {
                b'b' => '\u{8}',
                b't' => '\t',
                b'n' => '\n',
                b'f' => '\u{c}',
                b'r' => '\r',
                b'e' => '\u{1b}',
                b'"' => '"',
                b'\\' => '\\',
                _ => return None,
            });
            i += 1;
        }
        run = i;
    }
    out.push_str(&s[run..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    type Values = Vec<(String, String)>;

    fn keys(text: &str) -> Vec<(Option<&str>, Values)> {
        parse(text)
            .into_iter()
            .map(|s| {
                let values = s.values.iter().map(|(k, v, _)| (k.join("."), v.clone())).collect();
                (s.header, values)
            })
            .collect()
    }

    fn pair(key: &str, value: &str) -> (String, String) {
        (key.to_owned(), value.to_owned())
    }

    /// Headers and values are found as the C# finds them; a `[` or `#` in a string is neither a header nor a
    /// comment, also in a multi-line string.
    #[test]
    fn tables_values_and_strings() {
        let text = "a = 1 # one\n\
                    [ x . \"y z\" ] # t\n\
                    s = \"q\\\"#\\u00e9\" # c\n\
                    l = 'C:\\x#' \n\
                    m = \"\"\"\n\
                    [not.a.table]\n\
                    # not a comment \"\"\" \n\
                    n = '''a\n\
                    [b]'''\n\
                    [[hooks.Stop.hooks]]\n\
                    command = \"x\"";
        assert_eq!(
            keys(text),
            [
                (None, vec![pair("a", "1")]),
                (
                    Some("[ x . \"y z\" ] # t"),
                    vec![
                        pair("s", "s:q\"#é"),
                        pair("l", "s:C:\\x#"),
                        pair("m", "\"\"\""),
                        pair("n", "'''a")
                    ]
                ),
                (Some("[[hooks.Stop.hooks]]"), vec![pair("command", "s:x")]),
            ]
        );
        let segs = parse(text);
        assert_eq!(segs[1].table.as_deref(), Some(&["x".to_owned(), "y z".to_owned()][..]));
        assert_eq!(segs[2].handler.as_deref(), Some("Stop"));
        assert_eq!(segs[2].command.as_deref(), Some("x"));
        // the lines are kept as they were, each with its ending
        assert_eq!(
            segs.iter().flat_map(|s| s.lines.iter().copied()).collect::<String>(),
            text
        );
    }

    #[test]
    fn escapes_toml_knows() {
        for (text, value) in [
            (
                r#""\x41\u00e9\U0001F600\e\b\t\n\f\r\"\\""#,
                Some("Aé😀\u{1b}\u{8}\t\n\u{c}\r\"\\"),
            ),
            (r#""\q""#, None),
            (r#""\u12""#, None),
            (r#""\uD800""#, None),
            (r#""\U00110000""#, None),
            (r#""\u00e""#, None),
            (r#""\x4g""#, None),
            (r#""é\u00e9""#, Some("éé")),
            ("'a\\b'", Some("a\\b")),
            ("\"a\" x", None),
            ("\"\"\"a\"\"\"", None),
            ("\"", None),
        ] {
            assert_eq!(string_value(text).as_deref(), value, "{text}");
        }
    }

    #[test]
    fn headers() {
        for (header, key, is_array) in [
            ("[[hooks.Stop]]", Some(vec!["hooks", "Stop"]), true),
            ("[[ hooks . 'Stop' ]] # x", Some(vec!["hooks", "Stop"]), true),
            (
                "[hooks.state.'/x/config.toml:stop:0:0']",
                Some(vec!["hooks", "state", "/x/config.toml:stop:0:0"]),
                false,
            ),
            ("[hooks.Stop] x", None, false),
            ("[ [hooks.Stop]]", None, false),
            ("[hooks.\"\"\"x\"\"\"]", None, false),
            ("[hooks.]", None, false),
            ("[hooks", None, false),
        ] {
            let (found, array) = header_key(header);
            let key = key.map(|k| k.into_iter().map(String::from).collect::<Vec<_>>());
            assert_eq!(found, key, "{header}");
            assert_eq!(array, is_array, "{header}");
        }
    }
}
