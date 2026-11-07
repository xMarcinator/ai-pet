//! The bubble's text for an event: a tool call's (`Describe`, `AgentSessions.cs:752-786`), a sub-agent's (`Helper`)
//! and text cut to fit (`Short`, `:792-798`); and the other ways the C# compares and cases text, which the Board and
//! the Codex log watcher share through [`AgentSessions`].
//!
//! The C# counts and cuts text in UTF-16 units. A character cut in half leaves a lone surrogate there, which a UTF-8
//! writer turns into U+FFFD; here it is U+FFFD straight away.

use std::borrow::Cow;

use serde_json::{Map, Value};

use super::AgentSessions;

/// The state, detail and prop a PreToolUse of `tool` shows.
pub(super) fn describe(tool: &str, input: Option<&Map<String, Value>>) -> (&'static str, String, Option<&'static str>) {
    let text = |k: &str| input.and_then(|i| i.get(k)).and_then(Value::as_str);
    match tool {
        "Bash" | "PowerShell" | "shell" | "local_shell" | "exec_command" => {
            ("working", "Running command".into(), Some("laptop"))
        }
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => (
            "working",
            format!("Editing {}", base_name(text("file_path").or(text("notebook_path")))),
            Some("laptop"),
        ),
        "apply_patch" => ("working", "Editing files".into(), Some("laptop")),
        "Read" => (
            "working",
            format!("Reading {}", base_name(text("file_path"))),
            Some("lens"),
        ),
        "Grep" | "Glob" | "ToolSearch" => ("working", "Searching the code".into(), Some("lens")),
        "WebFetch" | "WebSearch" | "web_search" => ("working", "Browsing the web".into(), Some("lens")),
        "Agent" | "Task" | "Workflow" => ("working", "Delegating to a helper".into(), Some("laptop")),
        "TodoWrite" | "EnterPlanMode" | "update_plan" => ("thinking", "Planning".into(), None),
        "AskUserQuestion" => ("attention", "Has a question for you".into(), None),
        "ExitPlanMode" => ("attention", "Plan ready for review".into(), None),
        "Skill" => {
            let skill = text("skill").unwrap_or("").rsplit(':').next().unwrap_or("");
            ("working", format!("Using skill {skill}"), Some("laptop"))
        }
        _ if tool.starts_with("mcp__") => {
            let server = tool.split("__").nth(1).unwrap_or("a tool");
            if server.contains("Browser") || server.contains("chrome") {
                ("working", "Using the browser".into(), Some("lens"))
            } else if utf16_len(server) > 20 {
                ("working", "Using a connector".into(), Some("laptop"))
            } else {
                ("working", format!("Using {}", server.replace('_', " ")), Some("laptop"))
            }
        }
        _ => ("working", format!("Using {tool}"), Some("laptop")),
    }
}

/// "a helper", or "a helper (Explore)" for a named kind of sub-agent.
pub(super) fn helper(agent_type: Option<&str>) -> String {
    match agent_type {
        Some(kind) if !kind.chars().all(char::is_whitespace) && kind != "general-purpose" && kind != "default" => {
            format!("a helper ({kind})")
        }
        _ => "a helper".into(),
    }
}

/// The text on one line with single spaces, its first letter upper-case, and at most `n` UTF-16 units: a longer one
/// is cut, and ends in "…".
pub(super) fn short(text: &str, n: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = text.chars();
    let Some(first) = chars.next() else { return text };
    let text = format!("{}{}", upper_invariant(first), chars.as_str());
    if utf16_len(&text) <= n {
        return text;
    }
    let cut = utf16_prefix(&text, n - 1);
    format!("{}…", cut.trim_end())
}

/// .NET's `char.ToUpperInvariant` of a string's first UTF-16 unit: the simple upper-case mapping of a character in
/// the Basic Multilingual Plane (one outside it is a surrogate pair there, which it leaves alone), except for ı,
/// which .NET keeps.
fn upper_invariant(c: char) -> char {
    if u32::from(c) > 0xFFFF || c == 'ı' {
        return c;
    }
    simple_upper(c)
}

/// `text.Contains(needle, StringComparison.OrdinalIgnoreCase)`: .NET's ordinal casing compares each character's simple
/// upper-case mapping, surrogate pairs too, but keeps ı and ſ, whose upper cases are ASCII.
pub(super) fn contains_ignoring_case(text: &str, needle: &str) -> bool {
    let upper = |s: &str| -> Vec<char> {
        s.chars()
            .map(|c| if c == 'ı' || c == 'ſ' { c } else { simple_upper(c) })
            .collect()
    };
    let (text, needle) = (upper(text), upper(needle));
    needle.is_empty() || text.windows(needle.len()).any(|w| w == needle)
}

/// .NET's `ToLowerInvariant`: each character's simple lower-case mapping, surrogate pairs too, but İ, whose lower
/// case is i, stays (its full mapping, i and a dot, is the only one of more than one character).
pub(super) fn lower_invariant(text: &str) -> String {
    text.chars()
        .map(|c| {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(l), None) => l,
                _ => c,
            }
        })
        .collect()
}

/// A character's simple upper-case mapping (UnicodeData.txt), which `char::to_uppercase` gives where it is a single
/// character.
fn simple_upper(c: char) -> char {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) => u,
        // a full mapping of several characters (ß to SS, say): the simple one is a single character only for these
        // Greek letters with ypogegrammeni, which map to their title-case forms
        _ => match u32::from(c) {
            0x1F80..=0x1F87 | 0x1F90..=0x1F97 | 0x1FA0..=0x1FA7 => char::from_u32(u32::from(c) + 8).unwrap_or(c),
            0x1FB3 => '\u{1FBC}',
            0x1FC3 => '\u{1FCC}',
            0x1FF3 => '\u{1FFC}',
            _ => c,
        },
    }
}

/// How long the text is in UTF-16 units, as the C#'s `Length`.
pub(super) fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// The text's first `n` UTF-16 units, as the C#'s `s[..n]`; a character cut in half leaves U+FFFD.
pub(super) fn utf16_prefix(s: &str, n: usize) -> Cow<'_, str> {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        units += c.len_utf16();
        if units > n {
            return if units - n == 1 && c.len_utf16() == 2 {
                Cow::Owned(format!("{}\u{FFFD}", &s[..i]))
            } else {
                Cow::Borrowed(&s[..i])
            };
        }
    }
    Cow::Borrowed(s)
}

/// The file's name, as `Path.GetFileName` finds it past any trailing separators, or "a file".
pub(super) fn base_name(path: Option<&str>) -> Cow<'_, str> {
    let path = path.unwrap_or("").trim_end_matches(['/', '\\']);
    match file_name(path) {
        "" => "a file".into(),
        name => name.into(),
    }
}

/// `Path.GetFileName` on Unix: what follows the last `/`.
#[cfg(not(windows))]
fn file_name(path: &str) -> &str {
    path.rfind('/').map_or(path, |i| &path[i + 1..])
}

/// `Path.GetFileName` on Windows: what follows the last `\` or `/`, and never any of the path's root (`C:` in
/// `C:name`, or all of `\\server\share`).
#[cfg(windows)]
fn file_name(path: &str) -> &str {
    let root = windows_root_len(path.as_bytes());
    match path.rfind(['\\', '/']) {
        Some(i) if i >= root => &path[i + 1..],
        _ => &path[root..],
    }
}

/// .NET's `PathInternal.GetRootLength` on Windows, in bytes (a root is ASCII but for the server and share names of a
/// UNC path, which it skips whole).
#[cfg(windows)]
fn windows_root_len(p: &[u8]) -> usize {
    let sep = |c: u8| c == b'\\' || c == b'/';
    // \\?\, \\.\ or \??\
    let extended = p.len() >= 4 && p[0] == b'\\' && (p[1] == b'\\' || p[1] == b'?') && p[2] == b'?' && p[3] == b'\\';
    let device = extended || (p.len() >= 4 && sep(p[0]) && sep(p[1]) && (p[2] == b'.' || p[2] == b'?') && sep(p[3]));
    let device_unc = device && p.len() >= 8 && sep(p[7]) && &p[4..7] == b"UNC";
    if (!device || device_unc) && !p.is_empty() && sep(p[0]) {
        if !device_unc && !(p.len() > 1 && sep(p[1])) {
            // rooted on the current drive: \foo
            return 1;
        }
        // UNC (\\ or \\?\UNC\): past the server and the share
        let mut i = if device_unc { 8 } else { 2 };
        let mut separators = 0;
        while i < p.len() {
            if sep(p[i]) {
                separators += 1;
                if separators == 2 {
                    break;
                }
            }
            i += 1;
        }
        i
    } else if device {
        // \\?\C:\, \\.\pipe\: past the first name, and its separator
        let mut i = 4;
        while i < p.len() && !sep(p[i]) {
            i += 1;
        }
        if i < p.len() && i > 4 && sep(p[i]) { i + 1 } else { i }
    } else if p.len() >= 2 && p[1] == b':' && p[0].is_ascii_alphabetic() {
        // C: or C:\
        if p.len() > 2 && sep(p[2]) { 3 } else { 2 }
    } else {
        0
    }
}

/// What the Board (`crate::board`) and the Codex log watcher (`crate::codex_watcher`) share with AgentSessions: its
/// statics that Board.cs calls, and the .NET rules they read ids and text by.
impl AgentSessions {
    /// `AgentSessions.V7Before`: whether UUIDv7 `a` was made before `b` (see `turns::v7_before`). None where the C#
    /// throws: a turn id that .NET's Guid parser takes but whose time `Convert.ToInt64` can't read.
    pub(crate) fn v7_before(a: &str, b: &str, now: f64) -> Option<bool> {
        super::turns::v7_before(a, b, now).ok()
    }

    /// `AgentSessions.IsHostId`: the Claude app's id for a chat, `local_<uuid>`, and nothing else.
    pub(crate) fn is_host_id(id: &str) -> bool {
        id.len() == 42 && id.starts_with("local_") && AgentSessions::is_guid(&id[6..])
    }

    /// `Guid.TryParseExact(id, "D")`, with the forms .NET also takes (see `claude::is_guid_d`).
    pub(crate) fn is_guid(id: &str) -> bool {
        super::claude::is_guid_d(id.as_bytes())
    }

    /// .NET's `ToLowerInvariant`.
    pub(crate) fn lower_invariant(text: &str) -> String {
        lower_invariant(text)
    }

    /// `Path.GetFileName`, by the rules of the OS it runs on.
    pub(crate) fn file_name(path: &str) -> &str {
        file_name(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Path.GetFileName's rules differ by OS; the golden data holds the ones of the OS it was written on, so both are
    /// held here (the Windows ones as .NET 10 gives them, in rust/golden's claude/describe-paths case).
    #[test]
    fn file_names_follow_the_os_rules() {
        #[cfg(windows)]
        let cases = [
            (r"C:\src\main.rs", "main.rs"),
            ("C:main.rs", "main.rs"),
            (r"C:\", "a file"),
            (r"\\server\share", "a file"),
            (r"\\server\share\x.txt", "x.txt"),
            (r"\\?\C:\x\y.txt", "y.txt"),
            (r"\\.\pipe\name", "name"),
            (r"a\b/c\d.txt", "d.txt"),
            (r"\\?\UNC\server\share", "a file"),
            (r"\\?\UNC\server\share\f.txt", "f.txt"),
            (r"\\?\", "a file"),
            ("1:\\x", "x"),
            ("a:b", "b"),
            ("/home/u/a.rs", "a.rs"),
        ];
        #[cfg(not(windows))]
        let cases = [
            (r"C:\src\main.rs", r"C:\src\main.rs"),
            ("C:main.rs", "C:main.rs"),
            (r"C:\", "C:"),
            (r"\\server\share", r"\\server\share"),
            (r"a\b/c\d.txt", r"c\d.txt"),
            ("a:b", "a:b"),
            ("/home/u/a.rs", "a.rs"),
            ("/", "a file"),
            ("dir/sub/", "sub"),
        ];
        for (path, name) in cases {
            assert_eq!(base_name(Some(path)), name, "{path}");
        }
    }

    #[test]
    fn utf16_prefixes_mend_a_cut_pair() {
        assert_eq!(utf16_prefix("ab😀c", 2), "ab");
        assert_eq!(utf16_prefix("ab😀c", 3), "ab\u{FFFD}");
        assert_eq!(utf16_prefix("ab😀c", 4), "ab😀");
        assert_eq!(utf16_prefix("ab😀c", 9), "ab😀c");
        assert_eq!(utf16_len("ab😀c"), 5);
    }
}
