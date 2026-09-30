//! Codex's registration (`CodexConfig`, `src/AiPet.Hook/CodexConfig.cs`): AiPet's hooks in `config.toml` in
//! `$CODEX_HOME` (else `~/.codex`), or in `hooks.json` there when the user's other hooks already are, since Codex
//! warns when one layer uses both.
//!
//! Rules, from how Codex 0.156 loads and trusts hooks:
//! - Codex trusts a hook by `<file>:<event>:<group>:<index>` plus a hash of its definition, and silently skips hooks
//!   that aren't trusted. So other tools' hooks must keep their position and bytes: config.toml is edited as text
//!   ([`toml_text`], never re-serialised), and AiPet's groups are always appended after everything else.
//! - AiPet's hooks are found by their command ([`is_ours`]), not only by the marker comments, since Codex rewrites
//!   the file when it records trust.
//! - Re-installing an identical definition changes nothing, wherever it sits, so trust is kept.
//!
//! One deliberate change from the C# (the spec's R4): config.toml is checked right before it is replaced, and when it
//! keeps changing under the edit (Codex writing it), the edit reads it [`ATTEMPTS`] times, and then writes nothing and
//! fails. The C# writes on its last attempt anyway, which can lose Codex's own changes and trust entries.
//!
//! What the C# reads with `StringComparison.OrdinalIgnoreCase` is compared in [`upper`] case, and its regular
//! expressions are matched as .NET matches them ([`legacy_command`], [`disabled_inline`]).

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::install::{NEWLINE, StagedBackup, backup, is_file, read_text, real_path, save, stage, thrown, upper};
use crate::json::{Node, Object};
use crate::json_out::{self, deep_equals, member, read};
use crate::toml_text::{self, Segment};

/// One of Codex's hook events AiPet registers for.
pub(crate) struct Event {
    pub(crate) name: &'static str,
    /// Codex doesn't wait for it (`async = true`).
    pub(crate) background: bool,
    /// Seconds.
    pub(crate) timeout: u32,
}

const fn event(name: &'static str, background: bool, timeout: u32) -> Event {
    Event {
        name,
        background,
        timeout,
    }
}

/// `CodexConfig.Events`. All run in the background (the hook prints nothing, so Codex needn't wait). On Windows each
/// run starts PowerShell (1-4 s), so events it can do without are left out there: PostToolUse (the next event
/// supersedes it), and Interrupt and SessionEnd, whose 3 s limit PowerShell can't reliably meet. Compaction and
/// sub-agents are rare enough to be worth it everywhere. (The plugin's events are `plugin_hooks::CODEX_EVENTS`.)
pub(crate) const EVENTS: &[Event] = if cfg!(windows) {
    &[
        event("SessionStart", true, 30),
        event("UserPromptSubmit", true, 30),
        event("PreToolUse", true, 30),
        event("PermissionRequest", true, 30),
        event("Stop", true, 30),
        event("PreCompact", true, 30),
        event("PostCompact", true, 30),
        event("SubagentStart", true, 30),
        event("SubagentStop", true, 30),
    ]
} else {
    &[
        event("SessionStart", true, 30),
        event("UserPromptSubmit", true, 30),
        event("PreToolUse", true, 30),
        event("PermissionRequest", true, 30),
        event("PostToolUse", true, 30),
        event("Stop", true, 30),
        event("PreCompact", true, 30),
        event("PostCompact", true, 30),
        event("SubagentStart", true, 30),
        event("SubagentStop", true, 30),
        event("Interrupt", false, 3),
        event("SessionEnd", false, 3),
    ]
};

const BEGIN: &str =
    "# >>> AiPet hooks (written by aipet-hook --install codex; Codex asks you to trust them again if they change) >>>";
const END: &str = "# <<< AiPet hooks <<<";

/// How many times an edit reads config.toml: when the file changed by the time the edit would be written (Codex
/// writing it meanwhile), it is read and edited again, and after the last attempt nothing is written.
pub(crate) const ATTEMPTS: usize = 3;

/// Where Codex keeps its user config (`Paths.CodexHome`), and the two files the hooks can be in.
struct Home {
    dir: PathBuf,
    config: PathBuf,
    hooks: PathBuf,
}

impl Home {
    fn at(dir: PathBuf) -> Home {
        Home {
            config: dir.join("config.toml"),
            hooks: dir.join("hooks.json"),
            dir,
        }
    }
}

/// `CodexConfig.Snake`: `PreToolUse` is `pre_tool_use`, the event's name in a trust key. A `_` goes before each ASCII
/// capital that follows an ASCII small letter, then it all goes to lower case as `ToLowerInvariant` does.
pub(crate) fn snake(event: &str) -> String {
    let mut out = String::with_capacity(event.len() + 4);
    let mut after_small = false;
    for c in event.chars() {
        if after_small && c.is_ascii_uppercase() {
            out.push('_');
        }
        after_small = c.is_ascii_lowercase();
        // the simple mapping: the one character there is, else the character itself (İ, whose full one is two)
        let mut lower = c.to_lowercase();
        out.push(match (lower.next(), lower.next()) {
            (Some(l), None) => l,
            _ => c,
        });
    }
    out
}

/// `CodexConfig.IsOurs`: AiPet's hooks run aipet-hook, or `AIPET-~<n>.EXE ... --agent codex` (older installs shortened
/// the file name too).
pub(crate) fn is_ours(command: Option<&str>) -> bool {
    command.is_some_and(|c| upper(c).contains("AIPET-HOOK") || legacy_command(c))
}

// ------------------------------------------------------------------ .NET's regular expressions
/// `AIPET-~\d+\.EXE\b.*--agent\s+codex` with `RegexOptions.IgnoreCase`, anywhere in the text. The hook runs in the
/// invariant culture (it is built with `InvariantGlobalization`), where these letters match only themselves in either
/// case. `.` is anything but a line feed, and `\s` .NET's white space, which is Rust's.
fn legacy_command(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    (0..chars.len()).any(|at| {
        let Some(rest) = literal(&chars[at..], "AIPET-~", true) else {
            return false;
        };
        let digits = rest.iter().take_while(|&&c| digit(c)).count();
        let Some(rest) = literal(&rest[digits..], ".EXE", true).filter(|_| digits > 0) else {
            return false;
        };
        // \b: E is a word character, so the next one mustn't be
        if rest.first().is_some_and(|&c| word(c)) {
            return false;
        }
        // .* up to --agent, on this line
        let line = rest.iter().position(|&c| c == '\n').unwrap_or(rest.len());
        (0..line).any(|from| {
            literal(&rest[from..], "--AGENT", true).is_some_and(|after| {
                let spaces = after.iter().take_while(|c| c.is_whitespace()).count();
                spaces > 0 && literal(&after[spaces..], "CODEX", true).is_some()
            })
        })
    })
}

/// `^\{.*\benabled\s*=\s*false\b`: an inline table that says `enabled = false` (`Plugin`).
fn disabled_inline(value: &str) -> bool {
    let chars: Vec<char> = value.chars().collect();
    if chars.first() != Some(&'{') {
        return false;
    }
    let line = chars.iter().position(|&c| c == '\n').unwrap_or(chars.len());
    let spaces = |s: &[char]| s.iter().take_while(|c| c.is_whitespace()).count();
    // \b before enabled (a word character): the one before it mustn't be one
    (1..line).any(|at| {
        !word(chars[at - 1])
            && literal(&chars[at..], "enabled", false)
                .map(|rest| &rest[spaces(rest)..])
                .and_then(|rest| literal(rest, "=", false))
                .map(|rest| &rest[spaces(rest)..])
                .and_then(|rest| literal(rest, "false", false))
                .is_some_and(|rest| rest.first().is_none_or(|&c| !word(c)))
    })
}

/// The text past `pattern` when it starts with it, the ASCII letters in either case when `ignore_case` (`pattern` has
/// them in upper case then).
fn literal<'c>(text: &'c [char], pattern: &str, ignore_case: bool) -> Option<&'c [char]> {
    let n = pattern.len();
    let same = |(p, &c): (char, &char)| {
        if ignore_case {
            c.to_ascii_uppercase() == p
        } else {
            c == p
        }
    };
    (text.len() >= n && pattern.chars().zip(text).all(same)).then(|| &text[n..])
}

/// .NET's `\d`: a decimal digit (Unicode's Nd) in the BMP, where each run of them is 0 to 9. .NET's regular
/// expressions read UTF-16, where a character past the BMP is two surrogates, which are no digits.
const DIGITS: [u32; 37] = [
    0x30, 0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66, 0xDE6, 0xE50, 0xED0,
    0xF20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620,
    0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10,
];

fn digit(c: char) -> bool {
    let c = u32::from(c);
    DIGITS.iter().any(|&zero| (zero..zero + 10).contains(&c))
}

/// What .NET's `\b` takes for a word character, as the first and last of each range: `\w` (letters, non-spacing
/// marks, decimal digits and connector punctuation) and the zero-width joiner and non-joiner, in the BMP (see
/// [`DIGITS`]).
#[rustfmt::skip]
const WORD: &[u32] = &[
    0x30, 0x39, 0x41, 0x5A, 0x5F, 0x5F, 0x61, 0x7A, 0xAA, 0xAA, 0xB5, 0xB5, 0xBA, 0xBA, 0xC0, 0xD6, 0xD8, 0xF6, 0xF8,
    0x2C1, 0x2C6, 0x2D1, 0x2E0, 0x2E4, 0x2EC, 0x2EC, 0x2EE, 0x2EE, 0x300, 0x374, 0x376, 0x377, 0x37A, 0x37D, 0x37F,
    0x37F, 0x386, 0x386, 0x388, 0x38A, 0x38C, 0x38C, 0x38E, 0x3A1, 0x3A3, 0x3F5, 0x3F7, 0x481, 0x483, 0x487, 0x48A,
    0x52F, 0x531, 0x556, 0x559, 0x559, 0x560, 0x588, 0x591, 0x5BD, 0x5BF, 0x5BF, 0x5C1, 0x5C2, 0x5C4, 0x5C5, 0x5C7,
    0x5C7, 0x5D0, 0x5EA, 0x5EF, 0x5F2, 0x610, 0x61A, 0x620, 0x669, 0x66E, 0x6D3, 0x6D5, 0x6DC, 0x6DF, 0x6E8, 0x6EA,
    0x6FC, 0x6FF, 0x6FF, 0x710, 0x74A, 0x74D, 0x7B1, 0x7C0, 0x7F5, 0x7FA, 0x7FA, 0x7FD, 0x7FD, 0x800, 0x82D, 0x840,
    0x85B, 0x860, 0x86A, 0x870, 0x887, 0x889, 0x88E, 0x897, 0x8E1, 0x8E3, 0x902, 0x904, 0x93A, 0x93C, 0x93D, 0x941,
    0x948, 0x94D, 0x94D, 0x950, 0x963, 0x966, 0x96F, 0x971, 0x981, 0x985, 0x98C, 0x98F, 0x990, 0x993, 0x9A8, 0x9AA,
    0x9B0, 0x9B2, 0x9B2, 0x9B6, 0x9B9, 0x9BC, 0x9BD, 0x9C1, 0x9C4, 0x9CD, 0x9CE, 0x9DC, 0x9DD, 0x9DF, 0x9E3, 0x9E6,
    0x9F1, 0x9FC, 0x9FC, 0x9FE, 0x9FE, 0xA01, 0xA02, 0xA05, 0xA0A, 0xA0F, 0xA10, 0xA13, 0xA28, 0xA2A, 0xA30, 0xA32,
    0xA33, 0xA35, 0xA36, 0xA38, 0xA39, 0xA3C, 0xA3C, 0xA41, 0xA42, 0xA47, 0xA48, 0xA4B, 0xA4D, 0xA51, 0xA51, 0xA59,
    0xA5C, 0xA5E, 0xA5E, 0xA66, 0xA75, 0xA81, 0xA82, 0xA85, 0xA8D, 0xA8F, 0xA91, 0xA93, 0xAA8, 0xAAA, 0xAB0, 0xAB2,
    0xAB3, 0xAB5, 0xAB9, 0xABC, 0xABD, 0xAC1, 0xAC5, 0xAC7, 0xAC8, 0xACD, 0xACD, 0xAD0, 0xAD0, 0xAE0, 0xAE3, 0xAE6,
    0xAEF, 0xAF9, 0xAFF, 0xB01, 0xB01, 0xB05, 0xB0C, 0xB0F, 0xB10, 0xB13, 0xB28, 0xB2A, 0xB30, 0xB32, 0xB33, 0xB35,
    0xB39, 0xB3C, 0xB3D, 0xB3F, 0xB3F, 0xB41, 0xB44, 0xB4D, 0xB4D, 0xB55, 0xB56, 0xB5C, 0xB5D, 0xB5F, 0xB63, 0xB66,
    0xB6F, 0xB71, 0xB71, 0xB82, 0xB83, 0xB85, 0xB8A, 0xB8E, 0xB90, 0xB92, 0xB95, 0xB99, 0xB9A, 0xB9C, 0xB9C, 0xB9E,
    0xB9F, 0xBA3, 0xBA4, 0xBA8, 0xBAA, 0xBAE, 0xBB9, 0xBC0, 0xBC0, 0xBCD, 0xBCD, 0xBD0, 0xBD0, 0xBE6, 0xBEF, 0xC00,
    0xC00, 0xC04, 0xC0C, 0xC0E, 0xC10, 0xC12, 0xC28, 0xC2A, 0xC39, 0xC3C, 0xC40, 0xC46, 0xC48, 0xC4A, 0xC4D, 0xC55,
    0xC56, 0xC58, 0xC5A, 0xC5D, 0xC5D, 0xC60, 0xC63, 0xC66, 0xC6F, 0xC80, 0xC81, 0xC85, 0xC8C, 0xC8E, 0xC90, 0xC92,
    0xCA8, 0xCAA, 0xCB3, 0xCB5, 0xCB9, 0xCBC, 0xCBD, 0xCBF, 0xCBF, 0xCC6, 0xCC6, 0xCCC, 0xCCD, 0xCDD, 0xCDE, 0xCE0,
    0xCE3, 0xCE6, 0xCEF, 0xCF1, 0xCF2, 0xD00, 0xD01, 0xD04, 0xD0C, 0xD0E, 0xD10, 0xD12, 0xD3D, 0xD41, 0xD44, 0xD4D,
    0xD4E, 0xD54, 0xD56, 0xD5F, 0xD63, 0xD66, 0xD6F, 0xD7A, 0xD7F, 0xD81, 0xD81, 0xD85, 0xD96, 0xD9A, 0xDB1, 0xDB3,
    0xDBB, 0xDBD, 0xDBD, 0xDC0, 0xDC6, 0xDCA, 0xDCA, 0xDD2, 0xDD4, 0xDD6, 0xDD6, 0xDE6, 0xDEF, 0xE01, 0xE3A, 0xE40,
    0xE4E, 0xE50, 0xE59, 0xE81, 0xE82, 0xE84, 0xE84, 0xE86, 0xE8A, 0xE8C, 0xEA3, 0xEA5, 0xEA5, 0xEA7, 0xEBD, 0xEC0,
    0xEC4, 0xEC6, 0xEC6, 0xEC8, 0xECE, 0xED0, 0xED9, 0xEDC, 0xEDF, 0xF00, 0xF00, 0xF18, 0xF19, 0xF20, 0xF29, 0xF35,
    0xF35, 0xF37, 0xF37, 0xF39, 0xF39, 0xF40, 0xF47, 0xF49, 0xF6C, 0xF71, 0xF7E, 0xF80, 0xF84, 0xF86, 0xF97, 0xF99,
    0xFBC, 0xFC6, 0xFC6, 0x1000, 0x102A, 0x102D, 0x1030, 0x1032, 0x1037, 0x1039, 0x103A, 0x103D, 0x1049, 0x1050,
    0x1055, 0x1058, 0x1061, 0x1065, 0x1066, 0x106E, 0x1082, 0x1085, 0x1086, 0x108D, 0x108E, 0x1090, 0x1099, 0x109D,
    0x109D, 0x10A0, 0x10C5, 0x10C7, 0x10C7, 0x10CD, 0x10CD, 0x10D0, 0x10FA, 0x10FC, 0x1248, 0x124A, 0x124D, 0x1250,
    0x1256, 0x1258, 0x1258, 0x125A, 0x125D, 0x1260, 0x1288, 0x128A, 0x128D, 0x1290, 0x12B0, 0x12B2, 0x12B5, 0x12B8,
    0x12BE, 0x12C0, 0x12C0, 0x12C2, 0x12C5, 0x12C8, 0x12D6, 0x12D8, 0x1310, 0x1312, 0x1315, 0x1318, 0x135A, 0x135D,
    0x135F, 0x1380, 0x138F, 0x13A0, 0x13F5, 0x13F8, 0x13FD, 0x1401, 0x166C, 0x166F, 0x167F, 0x1681, 0x169A, 0x16A0,
    0x16EA, 0x16F1, 0x16F8, 0x1700, 0x1714, 0x171F, 0x1733, 0x1740, 0x1753, 0x1760, 0x176C, 0x176E, 0x1770, 0x1772,
    0x1773, 0x1780, 0x17B5, 0x17B7, 0x17BD, 0x17C6, 0x17C6, 0x17C9, 0x17D3, 0x17D7, 0x17D7, 0x17DC, 0x17DD, 0x17E0,
    0x17E9, 0x180B, 0x180D, 0x180F, 0x1819, 0x1820, 0x1878, 0x1880, 0x18AA, 0x18B0, 0x18F5, 0x1900, 0x191E, 0x1920,
    0x1922, 0x1927, 0x1928, 0x1932, 0x1932, 0x1939, 0x193B, 0x1946, 0x196D, 0x1970, 0x1974, 0x1980, 0x19AB, 0x19B0,
    0x19C9, 0x19D0, 0x19D9, 0x1A00, 0x1A18, 0x1A1B, 0x1A1B, 0x1A20, 0x1A54, 0x1A56, 0x1A56, 0x1A58, 0x1A5E, 0x1A60,
    0x1A60, 0x1A62, 0x1A62, 0x1A65, 0x1A6C, 0x1A73, 0x1A7C, 0x1A7F, 0x1A89, 0x1A90, 0x1A99, 0x1AA7, 0x1AA7, 0x1AB0,
    0x1ABD, 0x1ABF, 0x1ACE, 0x1B00, 0x1B03, 0x1B05, 0x1B34, 0x1B36, 0x1B3A, 0x1B3C, 0x1B3C, 0x1B42, 0x1B42, 0x1B45,
    0x1B4C, 0x1B50, 0x1B59, 0x1B6B, 0x1B73, 0x1B80, 0x1B81, 0x1B83, 0x1BA0, 0x1BA2, 0x1BA5, 0x1BA8, 0x1BA9, 0x1BAB,
    0x1BE6, 0x1BE8, 0x1BE9, 0x1BED, 0x1BED, 0x1BEF, 0x1BF1, 0x1C00, 0x1C23, 0x1C2C, 0x1C33, 0x1C36, 0x1C37, 0x1C40,
    0x1C49, 0x1C4D, 0x1C7D, 0x1C80, 0x1C8A, 0x1C90, 0x1CBA, 0x1CBD, 0x1CBF, 0x1CD0, 0x1CD2, 0x1CD4, 0x1CE0, 0x1CE2,
    0x1CF6, 0x1CF8, 0x1CFA, 0x1D00, 0x1F15, 0x1F18, 0x1F1D, 0x1F20, 0x1F45, 0x1F48, 0x1F4D, 0x1F50, 0x1F57, 0x1F59,
    0x1F59, 0x1F5B, 0x1F5B, 0x1F5D, 0x1F5D, 0x1F5F, 0x1F7D, 0x1F80, 0x1FB4, 0x1FB6, 0x1FBC, 0x1FBE, 0x1FBE, 0x1FC2,
    0x1FC4, 0x1FC6, 0x1FCC, 0x1FD0, 0x1FD3, 0x1FD6, 0x1FDB, 0x1FE0, 0x1FEC, 0x1FF2, 0x1FF4, 0x1FF6, 0x1FFC, 0x200C,
    0x200D, 0x203F, 0x2040, 0x2054, 0x2054, 0x2071, 0x2071, 0x207F, 0x207F, 0x2090, 0x209C, 0x20D0, 0x20DC, 0x20E1,
    0x20E1, 0x20E5, 0x20F0, 0x2102, 0x2102, 0x2107, 0x2107, 0x210A, 0x2113, 0x2115, 0x2115, 0x2119, 0x211D, 0x2124,
    0x2124, 0x2126, 0x2126, 0x2128, 0x2128, 0x212A, 0x212D, 0x212F, 0x2139, 0x213C, 0x213F, 0x2145, 0x2149, 0x214E,
    0x214E, 0x2183, 0x2184, 0x2C00, 0x2CE4, 0x2CEB, 0x2CF3, 0x2D00, 0x2D25, 0x2D27, 0x2D27, 0x2D2D, 0x2D2D, 0x2D30,
    0x2D67, 0x2D6F, 0x2D6F, 0x2D7F, 0x2D96, 0x2DA0, 0x2DA6, 0x2DA8, 0x2DAE, 0x2DB0, 0x2DB6, 0x2DB8, 0x2DBE, 0x2DC0,
    0x2DC6, 0x2DC8, 0x2DCE, 0x2DD0, 0x2DD6, 0x2DD8, 0x2DDE, 0x2DE0, 0x2DFF, 0x2E2F, 0x2E2F, 0x3005, 0x3006, 0x302A,
    0x302D, 0x3031, 0x3035, 0x303B, 0x303C, 0x3041, 0x3096, 0x3099, 0x309A, 0x309D, 0x309F, 0x30A1, 0x30FA, 0x30FC,
    0x30FF, 0x3105, 0x312F, 0x3131, 0x318E, 0x31A0, 0x31BF, 0x31F0, 0x31FF, 0x3400, 0x4DBF, 0x4E00, 0xA48C, 0xA4D0,
    0xA4FD, 0xA500, 0xA60C, 0xA610, 0xA62B, 0xA640, 0xA66F, 0xA674, 0xA67D, 0xA67F, 0xA6E5, 0xA6F0, 0xA6F1, 0xA717,
    0xA71F, 0xA722, 0xA788, 0xA78B, 0xA7CD, 0xA7D0, 0xA7D1, 0xA7D3, 0xA7D3, 0xA7D5, 0xA7DC, 0xA7F2, 0xA822, 0xA825,
    0xA826, 0xA82C, 0xA82C, 0xA840, 0xA873, 0xA882, 0xA8B3, 0xA8C4, 0xA8C5, 0xA8D0, 0xA8D9, 0xA8E0, 0xA8F7, 0xA8FB,
    0xA8FB, 0xA8FD, 0xA92D, 0xA930, 0xA951, 0xA960, 0xA97C, 0xA980, 0xA982, 0xA984, 0xA9B3, 0xA9B6, 0xA9B9, 0xA9BC,
    0xA9BD, 0xA9CF, 0xA9D9, 0xA9E0, 0xA9FE, 0xAA00, 0xAA2E, 0xAA31, 0xAA32, 0xAA35, 0xAA36, 0xAA40, 0xAA4C, 0xAA50,
    0xAA59, 0xAA60, 0xAA76, 0xAA7A, 0xAA7A, 0xAA7C, 0xAA7C, 0xAA7E, 0xAAC2, 0xAADB, 0xAADD, 0xAAE0, 0xAAEA, 0xAAEC,
    0xAAED, 0xAAF2, 0xAAF4, 0xAAF6, 0xAAF6, 0xAB01, 0xAB06, 0xAB09, 0xAB0E, 0xAB11, 0xAB16, 0xAB20, 0xAB26, 0xAB28,
    0xAB2E, 0xAB30, 0xAB5A, 0xAB5C, 0xAB69, 0xAB70, 0xABE2, 0xABE5, 0xABE5, 0xABE8, 0xABE8, 0xABED, 0xABED, 0xABF0,
    0xABF9, 0xAC00, 0xD7A3, 0xD7B0, 0xD7C6, 0xD7CB, 0xD7FB, 0xF900, 0xFA6D, 0xFA70, 0xFAD9, 0xFB00, 0xFB06, 0xFB13,
    0xFB17, 0xFB1D, 0xFB28, 0xFB2A, 0xFB36, 0xFB38, 0xFB3C, 0xFB3E, 0xFB3E, 0xFB40, 0xFB41, 0xFB43, 0xFB44, 0xFB46,
    0xFBB1, 0xFBD3, 0xFD3D, 0xFD50, 0xFD8F, 0xFD92, 0xFDC7, 0xFDF0, 0xFDFB, 0xFE00, 0xFE0F, 0xFE20, 0xFE2F, 0xFE33,
    0xFE34, 0xFE4D, 0xFE4F, 0xFE70, 0xFE74, 0xFE76, 0xFEFC, 0xFF10, 0xFF19, 0xFF21, 0xFF3A, 0xFF3F, 0xFF3F, 0xFF41,
    0xFF5A, 0xFF66, 0xFFBE, 0xFFC2, 0xFFC7, 0xFFCA, 0xFFCF, 0xFFD2, 0xFFD7, 0xFFDA, 0xFFDC,
];

fn word(c: char) -> bool {
    let c = u32::from(c);
    WORD.as_chunks::<2>()
        .0
        .binary_search_by(|&[first, last]| {
            if last < c {
                std::cmp::Ordering::Less
            } else if first > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

// ------------------------------------------------------------------ the command
/// `CodexConfig.BuildCommand`: the command Codex runs. Windows runs it with PowerShell (`pwsh -NoProfile -Command
/// <command>`), and with cmd.exe in a rare fallback: a quoted program path is a PowerShell syntax error (exit 1 before
/// anything starts), so the path is written without quotes, with its folder in 8.3 short form if it has spaces. The
/// file name stays aipet-hook.exe (that's how AiPet knows its own hooks), and so do the backslashes, since cmd.exe
/// takes `/` for a switch. On Linux and macOS it runs with `sh -c` (or bash or zsh), where a single-quoted path is
/// always safe. These strings are frozen: Codex asks users to trust the hooks again when they change.
pub(crate) fn build_command(exe: &Path) -> String {
    if cfg!(windows) {
        let mut p = exe.to_string_lossy().into_owned();
        if !plain(&p)
            && let (Some(dir), Some(name)) = (exe.parent(), exe.file_name())
            && let Some(short) = short_path(dir)
        {
            p = Path::new(&short).join(name).to_string_lossy().into_owned();
        }
        if plain(&p) {
            format!("{p} --agent codex")
        } else {
            // PowerShell only
            format!("& '{}' --agent codex", p.replace('\'', "''"))
        }
    } else {
        format!("'{}' --agent codex", exe.to_string_lossy().replace('\'', "'\\''"))
    }
}

/// `^[A-Za-z0-9_.:/\\~-]+$`: a path any shell takes as it is (`$` may come before a last line feed).
fn plain(path: &str) -> bool {
    let path = path.strip_suffix('\n').unwrap_or(path);
    !path.is_empty()
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:/\\~-".contains(&b))
}

/// `ShortPath`: `C:\Program Files\...` as `C:\PROGRA~1\...`, so the path needs no quotes in any shell. `None` when
/// there's no short name to be had.
#[cfg(windows)]
fn short_path(path: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut buf = [0u16; 1024];
    // SAFETY: wide is a live, NUL-terminated string, and the call writes at most buf.len() units into buf
    let n = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    (n > 0 && n < buf.len()).then(|| String::from_utf16_lossy(&buf[..n]))
}

#[cfg(not(windows))]
fn short_path(_: &Path) -> Option<String> {
    None
}

// ------------------------------------------------------------------ install and uninstall
/// `CodexConfig.Install(exe)`: registers `exe`, or with the aipet plugin enabled, takes out the hooks it replaces.
/// Either way the pet gets every event, so that isn't a failure: installers run --install unchecked. What it says goes
/// to `say` and `warn`, a line at a time (`Console.WriteLine` and `Console.Error.WriteLine`).
pub(crate) fn install(exe: &Path, say: &mut dyn FnMut(&str), warn: &mut dyn FnMut(&str)) -> Result<i32, String> {
    let home = Home::at(aipet_ipc::paths::codex_home());
    let config = home.config.display();
    if let Some(plugin) = plugin(&home)? {
        // hooks registered here earlier would report every event a second time: the plugin replaces them
        let mut removed = edit_toml(&home.config, None, &[home.hooks.as_path()], say, &mut || {})?;
        removed |= edit_hooks_json(&home.hooks, None, say, warn)?;
        say(&if removed {
            format!(
                "The {plugin} plugin is enabled in {config} and reports every event, so the AiPet hooks registered \
                 earlier were removed (the plugin replaces them)."
            )
        } else {
            format!(
                "The {plugin} plugin is enabled in {config} and already reports every event, so no hooks were \
                 registered (they would report each one twice)."
            )
        });
        say(&format!(
            "To use hooks in Codex's config instead, remove the plugin (codex plugin remove {plugin}) and install again."
        ));
        return Ok(0);
    }
    let command = build_command(exe);
    let use_json = choose_hooks_json(&home);
    let target = if use_json { &home.hooks } else { &home.config };
    // TOML doesn't allow adding [[hooks.<Event>]] tables to hooks defined as values: appending would break the file
    if !use_json && let Some(clash) = inline_hooks(&toml_text::parse(&read_text(&home.config).unwrap_or_default())) {
        warn(&format!(
            "{config} defines hooks in a form AiPet can't add to without breaking the file:"
        ));
        warn(&format!("  {clash}"));
        warn(
            "Write those hooks as [[hooks.<Event>]] tables (or move them to hooks.json), then install again. Nothing \
             was changed.",
        );
        return Ok(1);
    }
    // config.toml first: it holds the trust entries, which are found from the hooks as they are before any edit
    let changed = if use_json {
        // AiPet leftovers in config.toml
        let changed = edit_toml(&home.config, None, &[], say, &mut || {})?;
        edit_hooks_json(&home.hooks, Some(&command), say, warn)? | changed
    } else {
        let changed = edit_toml(&home.config, Some(&command), &[home.hooks.as_path()], say, &mut || {})?;
        // an older install's hooks.json entries
        edit_hooks_json(&home.hooks, None, say, warn)? | changed
    };
    let target = target.display();
    say(&if changed {
        format!("AiPet hooks registered for Codex in {target}")
    } else {
        format!("AiPet hooks for Codex are already registered in {target} (unchanged)")
    });
    say(&format!("  command: {command}"));
    let events: Vec<&str> = EVENTS.iter().map(|e| e.name).collect();
    say(&format!("  events:  {}", events.join(", ")));
    say("");
    say("Codex runs new hooks only after you trust them:");
    say("  1. In a terminal, run: codex");
    say(
        "  2. At \"Hooks need review\" choose Review hooks and trust the AiPet ones (\"aipet-hook ... --agent codex\").",
    );
    say("     \"Trust all\" would also trust any other hooks waiting for review. Later: /hooks.");
    say("  3. Restart the ChatGPT/Codex desktop app so it picks up the trust.");
    say("Then check with: aipet-hook --doctor codex");
    Ok(0)
}

/// `CodexConfig.Uninstall`.
pub(crate) fn uninstall(say: &mut dyn FnMut(&str), warn: &mut dyn FnMut(&str)) -> Result<i32, String> {
    let home = Home::at(aipet_ipc::paths::codex_home());
    let changed = edit_toml(&home.config, None, &[home.hooks.as_path()], say, &mut || {})?;
    let changed = edit_hooks_json(&home.hooks, None, say, warn)? | changed;
    say(if changed {
        "AiPet hooks removed from Codex"
    } else {
        "No AiPet hooks were registered with Codex"
    });
    Ok(0)
}

/// `ChooseHooksJson`: hooks.json only when the user's other hooks already live there and config.toml has none.
fn choose_hooks_json(home: &Home) -> bool {
    let text = read_text(&home.config).unwrap_or_default();
    if toml_text::parse(&text)
        .iter()
        .any(|s| s.handler.is_some() && !is_ours(s.command.as_deref()))
    {
        return false;
    }
    // what reading hooks.json throws (it isn't there, isn't JSON, names a member twice) is a no too
    let elsewhere = || -> Result<bool, String> {
        let root = json_out::parse(&read_text(&home.hooks)?)?;
        let Some(Node::Object(hooks)) = member(&root, "hooks")? else {
            return Ok(false);
        };
        json_positions(hooks, |_, _, _, hook| Ok(!is_ours(command_of(hook)?)))
    };
    elsewhere().unwrap_or(false)
}

/// `CodexConfig.Plugin`: the AiPet plugin whose hooks Codex runs (its id, e.g. `aipet@aipet`). Codex runs them while
/// config.toml has a `[plugins."<id>"]` entry that isn't `enabled = false` and the plugin is in its cache
/// (plugins/cache/<marketplace>/<plugin>/<version>/); `codex plugin remove` takes both away.
fn plugin(home: &Home) -> Result<Option<String>, String> {
    // each id the first time it comes, and whether it's disabled
    let mut found: Vec<(String, bool)> = Vec::new();
    let mut set = |id: &str, disabled: Option<bool>| match found.iter().position(|(k, _)| k == id) {
        Some(at) => {
            if let Some(disabled) = disabled {
                found[at].1 = disabled;
            }
        }
        None => found.push((id.to_owned(), disabled.unwrap_or(false))),
    };
    let text = read_text(&home.config).unwrap_or_default();
    for s in toml_text::parse(&text) {
        if s.is_array || (s.header.is_some() && s.table.is_none()) {
            continue;
        }
        let table = s.table.unwrap_or_default();
        if let [plugins, name] = &table[..]
            && plugins == "plugins"
        {
            set(name, None);
        }
        for (key, value, _) in &s.values {
            let key: Vec<&str> = table.iter().chain(key).map(String::as_str).collect();
            match key[..] {
                ["plugins", id, "enabled"] => set(id, Some(value == "false")),
                // an inline table
                ["plugins", id] => set(id, Some(disabled_inline(value))),
                ["plugins", id, ..] => set(id, None),
                _ => {}
            }
        }
    }
    for (id, disabled) in found {
        let market = match id.split('@').collect::<Vec<_>>()[..] {
            ["aipet", market] if !disabled => market,
            _ => continue,
        };
        let cache = home.dir.join("plugins").join("cache").join(market).join("aipet");
        if !cache.is_dir() {
            continue;
        }
        let mut versions = fs::read_dir(&cache).map_err(|e| thrown(&e, &cache))?;
        if versions.any(|v| v.is_ok_and(|v| v.path().is_dir())) {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

// ------------------------------------------------------------------ config.toml as text
/// `InlineHooks`: a line (or header) of config.toml that defines hooks as values (`hooks = {...}`,
/// `hooks.Stop = [...]`, or `Stop = [...]` under `[hooks]`) or as a plain `[hooks.Stop]` table, for an event AiPet
/// adds `[[hooks.<Event>]]` tables to. TOML doesn't allow both. `None` if there's none.
fn inline_hooks(segs: &[Segment]) -> Option<String> {
    let clash = |key: &[&str]| match key {
        ["hooks"] => true,
        ["hooks", event, ..] => EVENTS.iter().any(|e| e.name == *event),
        _ => false,
    };
    for s in segs {
        // keys under [[...]] belong to its entry
        if s.is_array || (s.header.is_some() && s.table.is_none()) {
            continue;
        }
        let table: Vec<&str> = s.table.iter().flatten().map(String::as_str).collect();
        if table.len() >= 2 && clash(&table) {
            return s.header.map(str::to_owned);
        }
        for (key, _, line) in &s.values {
            let key: Vec<&str> = table.iter().copied().chain(key.iter().map(String::as_str)).collect();
            if clash(&key) {
                return Some(line.trim().to_owned());
            }
        }
    }
    None
}

/// What says a file is still the one that was read: which file it is (device and inode, or volume and file index) and
/// when it was last written.
#[derive(Debug, PartialEq)]
struct Stamp {
    id: (u64, u64),
    written: Option<SystemTime>,
}

/// The file's [`Stamp`], `None` when there's none to be had (it isn't there).
#[cfg(unix)]
fn stamp(path: &Path) -> Option<Stamp> {
    use std::os::unix::fs::MetadataExt;

    let m = fs::metadata(path).ok()?;
    Some(Stamp {
        id: (m.dev(), m.ino()),
        written: m.modified().ok(),
    })
}

#[cfg(windows)]
fn stamp(path: &Path) -> Option<Stamp> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};

    let file = fs::File::open(path).ok()?;
    // SAFETY: an all-zero BY_HANDLE_FILE_INFORMATION is a valid value to be filled in
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the handle is the open file's, and the call writes only the struct it is given
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) } == 0 {
        return None;
    }
    Some(Stamp {
        id: (
            u64::from(info.dwVolumeSerialNumber),
            (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        ),
        written: file.metadata().and_then(|m| m.modified()).ok(),
    })
}

/// `EditToml`: config.toml without AiPet's hooks (and their trust entries, also those of AiPet's hooks in the
/// `remove_trust_for` files), then, when `command` is given, AiPet's block appended. Whether the file changed.
///
/// The file must still be the one read when the edit replaces it: the edit and the backup are written next to it
/// first, and it is checked right before the replace. When it changed, both are removed again, and it is read and
/// edited again; after [`ATTEMPTS`] reads nothing is written and it fails. `meanwhile` runs right before that check,
/// with the edit and the backup written (a test changes the file there, as Codex could).
fn edit_toml(
    path: &Path,
    command: Option<&str>,
    remove_trust_for: &[&Path],
    say: &mut dyn FnMut(&str),
    meanwhile: &mut dyn FnMut(),
) -> Result<bool, String> {
    // a symlinked config.toml (dotfiles): edit the file it points at, not the link
    let real = real_path(path);
    for _ in 0..ATTEMPTS {
        if !is_file(&real) && command.is_none() {
            return Ok(false);
        }
        let stamp_read = stamp(&real);
        let text = read_text(&real).unwrap_or_default();
        let segs = toml_text::parse(&text);

        // trust entries of AiPet hooks in the other file (an older install's hooks.json), found before it's edited
        let mut other_trust = HashSet::new();
        for other in remove_trust_for {
            if upper(&other.to_string_lossy()) != upper(&path.to_string_lossy()) {
                other_trust.extend(our_json_keys(other)?);
            }
        }
        let trusted =
            |s: &Segment, keys: &HashSet<String>| s.state_key.as_ref().is_some_and(|k| keys.contains(&upper(k)));

        // already exactly right: keep AiPet's hooks and their trust, only drop other files' leftovers
        let keep_ours = command.is_some_and(|c| already_registered(&segs, c));
        if keep_ours && !segs.iter().any(|s| trusted(s, &other_trust)) {
            return Ok(false);
        }

        // otherwise drop AiPet's handlers and the groups left with no one else's (counted as Codex does), and see
        // where everything ends up once AiPet's block is appended again
        let mut drop: Vec<bool> = (0..segs.len())
            .map(|i| {
                let s = &segs[i];
                !keep_ours
                    && ((s.handler.is_some() && is_ours(s.command.as_deref()))
                        || (s.group.is_some() && {
                            let mut handlers = group_handlers(&segs, i).peekable();
                            handlers.peek().is_some() && handlers.all(|h| is_ours(h.command.as_deref()))
                        }))
            })
            .collect();
        let appended = command.filter(|_| !keep_ours).map(|c| block(c, "\n"));
        let appended = appended.as_deref().map(toml_text::parse).unwrap_or_default();
        let after: Vec<&Segment> = segs
            .iter()
            .zip(&drop)
            .filter(|(_, dropped)| !**dropped)
            .map(|(s, _)| s)
            .chain(&appended)
            .collect();

        // trust entries to drop: other files' AiPet leftovers, and AiPet's own whose key won't hold the same
        // definition any more (the rest still apply, so they stay)
        let mut trust_to_drop = other_trust;
        if !keep_ours {
            let was = our_defs(path, &segs.iter().collect::<Vec<_>>());
            let will = our_defs(path, &after);
            match segs.iter().find(|s| s.unreadable) {
                None => trust_to_drop.extend(
                    was.iter()
                        .filter(|(key, def)| will.get(*key) != Some(*def))
                        .map(|(key, _)| key.clone()),
                ),
                Some(odd) if !was.is_empty() => say(&format!(
                    "Note: AiPet can't make sense of {} in config.toml, so it left its old hooks' trust entries alone.",
                    odd.header.unwrap_or_default()
                )),
                Some(_) => {}
            }
            warn_if_shifting(&segs, &after, say);
        }
        for (s, dropped) in segs.iter().zip(&mut drop) {
            *dropped |= trusted(s, &trust_to_drop);
        }

        let rendered = render(&segs, &drop, keep_ours);
        let nl = if text.contains('\r') { "\r\n" } else { "\n" };
        let trimmed = || rendered.trim_end_matches(['\r', '\n', ' ', '\t']);
        let result = match command {
            Some(command) if !keep_ours => format!("{}{nl}{nl}{}", trimmed(), block(command, nl)),
            // tidy the gap our block left at the end of the file
            _ if rendered != text && !keep_ours => format!("{}{nl}", trimmed()),
            _ => rendered.clone(),
        };
        if result == text {
            return Ok(false);
        }

        if let Some(dir) = real.parent() {
            fs::create_dir_all(dir).map_err(|e| thrown(&e, dir))?;
        }
        // written first, so that nothing but the replace comes after the check (and dropped when it fails)
        let staged = stage(&real, &result)?;
        let backup = if is_file(path) { StagedBackup::make(path) } else { None };
        meanwhile();
        // Codex wrote it meanwhile: read it again (the C# wrote over it on its last attempt)
        if stamp(&real) != stamp_read {
            continue;
        }
        let replaced = staged.commit();
        // the C# backs up before it replaces, so the backup stays when the replace fails
        if let Some(backup) = backup {
            backup.keep();
        }
        replaced?;
        return Ok(true);
    }
    Err(format!(
        "{} kept changing while AiPet edited it (Codex may be writing it). Close Codex and try again; nothing was \
         changed.",
        path.display()
    ))
}

/// `Render`: the text without the dropped segments, but for comments that trail one (the user's, about what follows),
/// and without AiPet's marker lines unless `markers`.
fn render(segs: &[Segment], drop: &[bool], markers: bool) -> String {
    let marker = |l: &str| {
        let l = l.trim_start();
        l.starts_with("# >>> AiPet hooks") || l.starts_with("# <<< AiPet hooks")
    };
    let mut out = String::new();
    for (s, &dropped) in segs.iter().zip(drop) {
        let mut lines: &[&str] = &s.lines;
        if dropped {
            let tail = &s.lines[s.last_content.map_or(0, |i| i + 1)..];
            lines = if tail.iter().any(|l| l.trim_start().starts_with('#') && !marker(l)) {
                tail
            } else {
                &[]
            };
        }
        for line in lines {
            if markers || !marker(line) {
                out.push_str(line);
            }
        }
    }
    out
}

/// `GroupHandlers`: a group's handlers by TOML rules: every `[[hooks.<Event>.hooks]]` after it up to the next
/// `[[hooks.<Event>]]`, whatever other tables come between.
fn group_handlers<'s, 'a>(segs: &'s [Segment<'a>], group: usize) -> impl Iterator<Item = &'s Segment<'a>> {
    let event = segs[group].group.as_deref();
    segs[group + 1..]
        .iter()
        .take_while(move |s| s.group.as_deref() != event)
        .filter(move |s| s.handler.as_deref() == event)
}

/// `TomlPositions`: where Codex puts each handler: by TOML rules it belongs to the latest `[[hooks.<Event>]]` of its
/// event above it, whatever other tables come between. Each handler with its group and index there.
fn positions<'s, 'a>(segs: &[&'s Segment<'a>]) -> Vec<(&'s Segment<'a>, usize, usize)> {
    let mut groups: HashMap<&str, usize> = HashMap::new();
    let mut handlers: HashMap<&str, usize> = HashMap::new();
    let mut out = Vec::new();
    for &s in segs {
        if let Some(event) = s.group.as_deref() {
            *groups.entry(event).or_default() += 1;
            handlers.insert(event, 0);
        } else if let Some(event) = s.handler.as_deref()
            && let Some(&group) = groups.get(event)
        {
            let index = handlers.entry(event).or_default();
            out.push((s, group - 1, *index));
            *index += 1;
        }
    }
    out
}

/// The trust key Codex gives a handler in a hooks file, `<file>:<event>:<group>:<index>`, in [`upper`] case: the C#
/// keeps them in sets and maps that compare as `OrdinalIgnoreCase`.
fn trust_key(file: &Path, event: &str, group: usize, index: usize) -> String {
    upper(&format!("{}:{}:{group}:{index}", file.display(), snake(event)))
}

/// `OurDefs`: AiPet's handlers in config.toml by trust key, with their definitions.
fn our_defs(file: &Path, segs: &[&Segment]) -> HashMap<String, String> {
    positions(segs)
        .into_iter()
        .filter(|(s, _, _)| is_ours(s.command.as_deref()))
        .map(|(s, group, index)| {
            (
                trust_key(file, s.handler.as_deref().unwrap_or_default(), group, index),
                normal(s),
            )
        })
        .collect()
}

/// `AlreadyRegistered`: AiPet's handlers are exactly the ones we'd write (same events and definitions), wherever they
/// are: moving them to the end would only shift the hooks after them and cost their trust.
fn already_registered(segs: &[Segment], command: &str) -> bool {
    let ours: Vec<(&str, String)> = segs
        .iter()
        .filter(|s| is_ours(s.command.as_deref()))
        .filter_map(|s| Some((s.handler.as_deref()?, normal(s))))
        .collect();
    let block = block(command, "\n");
    let wanted = toml_text::parse(&block);
    let want: Vec<(&str, String)> = wanted
        .iter()
        .filter_map(|s| Some((s.handler.as_deref()?, normal(s))))
        .collect();
    ours.len() == want.len() && want.iter().all(|w| ours.contains(w))
}

/// `Normal`: a handler's definition, for comparing: its keys and values, strings by their value (so quoting and
/// escapes don't matter), without comments or layout, sorted as `StringComparer.Ordinal` sorts (by UTF-16 unit).
fn normal(s: &Segment) -> String {
    let mut lines: Vec<String> = s
        .values
        .iter()
        .map(|(key, value, _)| format!("{}={value}", key.join(".")))
        .collect();
    lines.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    lines.join("|")
}

/// `Block`: AiPet's hooks between the markers: a group for each event, with the one handler in it.
fn block(command: &str, nl: &str) -> String {
    let command = command.replace('\\', "\\\\").replace('"', "\\\"");
    let mut out = format!("{BEGIN}{nl}");
    for e in EVENTS {
        let name = e.name;
        let _ = write!(
            out,
            "[[hooks.{name}]]{nl}[[hooks.{name}.hooks]]{nl}type = \"command\"{nl}command = \"{command}\"{nl}timeout = \
             {}{nl}",
            e.timeout
        );
        if e.background {
            let _ = write!(out, "async = true{nl}");
        }
        out.push_str(nl);
    }
    out + END + nl
}

/// `WarnIfShifting`: removing AiPet's hooks moves the other handlers after them in the same event (to another group or
/// index), which breaks their recorded trust. AiPet appends its groups last, so this only happens if something was
/// added after them; say so instead of hiding it.
fn warn_if_shifting(before: &[Segment], after: &[&Segment], say: &mut dyn FnMut(&str)) {
    let now: HashMap<*const Segment, (usize, usize)> = positions(after)
        .into_iter()
        .map(|(s, group, index)| (std::ptr::from_ref(s), (group, index)))
        .collect();
    let mut warned = HashSet::new();
    for (s, group, index) in positions(&before.iter().collect::<Vec<_>>()) {
        let event = s.handler.as_deref().unwrap_or_default();
        if !is_ours(s.command.as_deref())
            && now.get(&std::ptr::from_ref(s)).is_some_and(|&at| at != (group, index))
            && warned.insert(event)
        {
            say(&format!(
                "Note: another {event} hook comes after AiPet's in config.toml; Codex will ask you to trust it again."
            ));
        }
    }
}

// ------------------------------------------------------------------ hooks.json
/// `JsonPositions`: every handler in a hooks.json `hooks` object, with the group and index its trust key uses, handed
/// to `visit` until it says stop (true, which this returns). What reading a node throws on the way (a name given
/// twice) is thrown where the C#'s enumeration throws it.
fn json_positions<'n>(
    hooks: &'n Object,
    mut visit: impl FnMut(&'n str, usize, usize, &'n Node) -> Result<bool, String>,
) -> Result<bool, String> {
    read(hooks)?;
    for (event, value) in hooks.iter() {
        let Node::Array(groups) = value else { continue };
        for (group, node) in groups.iter().enumerate() {
            if !matches!(node, Node::Object(_)) {
                continue;
            }
            let Some(Node::Array(list)) = member(node, "hooks")? else {
                continue;
            };
            for (index, hook) in list.iter().enumerate() {
                if visit(event, group, index, hook)? {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

/// `CommandOf`: a handler's command, when it is a string.
fn command_of(hook: &Node) -> Result<Option<&str>, String> {
    if !matches!(hook, Node::Object(_)) {
        return Ok(None);
    }
    Ok(match member(hook, "command")? {
        Some(Node::String(command)) => Some(command),
        _ => None,
    })
}

/// `OurKeys(file, JsonHandlers(file))`: the trust keys of AiPet's handlers in a hooks.json. A file that isn't there,
/// isn't JSON or has no `hooks` object has none; what reading its nodes throws (a name given twice) is thrown.
fn our_json_keys(path: &Path) -> Result<HashSet<String>, String> {
    let root = read_text(path).and_then(|text| json_out::parse(&text));
    let Ok(Ok(Some(Node::Object(hooks)))) = root.as_ref().map(|root| member(root, "hooks")) else {
        return Ok(HashSet::new());
    };
    let mut keys = HashSet::new();
    json_positions(hooks, |event, group, index, hook| {
        if is_ours(command_of(hook)?) {
            keys.insert(trust_key(path, event, group, index));
        }
        Ok(false)
    })?;
    Ok(keys)
}

/// `JsonHandler`: a handler in a user's hooks.json (the plugin's have `commandWindows` too, see plugin_hooks).
fn json_handler(e: &Event, command: &str) -> Node {
    let mut handler = Object::default();
    handler.push("type", Node::String("command".into()));
    handler.push("command", Node::String(command.into()));
    handler.push("timeout", Node::Number(e.timeout.to_string()));
    if e.background {
        handler.push("async", Node::Bool(true));
    }
    Node::Object(handler)
}

/// `JsonRegistered`: AiPet's handlers in hooks.json are exactly the ones we'd write, one per event, wherever they are.
fn json_registered(hooks: &Object, command: &str) -> Result<bool, String> {
    let mut ours = Vec::new();
    json_positions(hooks, |event, _, _, hook| {
        if is_ours(command_of(hook)?) {
            ours.push((event, hook));
        }
        Ok(false)
    })?;
    if ours.len() != EVENTS.len() {
        return Ok(false);
    }
    for e in EVENTS {
        let mut found = false;
        for (event, hook) in &ours {
            if *event == e.name && deep_equals(hook, &json_handler(e, command))? {
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The group and index of each handler that isn't AiPet's (nor `null`), in the file's order, with its event.
fn others(hooks: &Object) -> Result<Vec<(String, usize, usize)>, String> {
    let mut others = Vec::new();
    json_positions(hooks, |event, group, index, hook| {
        if *hook != Node::Null && !is_ours(command_of(hook)?) {
            others.push((event.to_owned(), group, index));
        }
        Ok(false)
    })?;
    Ok(others)
}

/// `string.IsNullOrWhiteSpace`.
fn blank(text: &str) -> bool {
    text.chars().all(char::is_whitespace)
}

/// `EditHooksJson`: hooks.json without AiPet's handlers, then, when `command` is given, AiPet's groups appended.
/// Nothing is written when AiPet's handlers are already exactly right (wherever they are, so the trust Codex recorded
/// holds), and the file is deleted when nothing but an empty `hooks` object is left. Whether it changed.
fn edit_hooks_json(
    path: &Path,
    command: Option<&str>,
    say: &mut dyn FnMut(&str),
    warn: &mut dyn FnMut(&str),
) -> Result<bool, String> {
    if !is_file(path) && command.is_none() {
        return Ok(false);
    }
    let before = read_text(path).unwrap_or_default();
    let mut root = if blank(&before) {
        Object::default()
    } else {
        match json_out::parse(&before) {
            Ok(Node::Object(root)) => root,
            Ok(_) => Object::default(),
            Err(_) => {
                warn(&format!("{} isn't valid JSON; leaving it alone", path.display()));
                return Ok(false);
            }
        }
    };
    read(&root)?;
    if !matches!(root.get("hooks"), Some(Node::Object(_))) {
        // nothing of AiPet's to remove, and no empty "hooks" to add
        if command.is_none() {
            return Ok(false);
        }
        match root.get_mut("hooks") {
            Some(hooks) => *hooks = Node::Object(Object::default()),
            None => root.push("hooks", Node::Object(Object::default())),
        }
    }
    let Some(Node::Object(hooks)) = root.get_mut("hooks") else {
        unreachable!("hooks is an object now")
    };
    if let Some(command) = command
        && json_registered(hooks, command)?
    {
        return Ok(false);
    }

    // where the other hooks are now, to say so if removing AiPet's moves them (their trust is tied to that)
    let was = others(hooks)?;

    let mut removed = false;
    let events: Vec<String> = hooks.iter().map(|(event, _)| event.to_owned()).collect();
    for event in events {
        let Some(Node::Array(groups)) = hooks.get_mut(&event) else {
            continue;
        };
        let mut here = false;
        let mut kept = Vec::with_capacity(groups.len());
        for group in groups.iter_mut() {
            let Node::Object(group) = group else {
                kept.push(true);
                continue;
            };
            read(group)?;
            let Some(Node::Array(list)) = group.get_mut("hooks") else {
                kept.push(true);
                continue;
            };
            let ours = list
                .iter()
                .map(|hook| command_of(hook).map(is_ours))
                .collect::<Result<Vec<bool>, String>>()?;
            if ours.contains(&true) {
                let mut ours = ours.into_iter();
                list.retain(|_| !ours.next().unwrap_or(false));
                here = true;
                kept.push(!list.is_empty());
            } else {
                kept.push(true);
            }
        }
        let mut kept = kept.into_iter();
        groups.retain(|_| kept.next().unwrap_or(true));
        // only events AiPet emptied, not the user's own empty ones
        if here && groups.is_empty() {
            hooks.remove(&event);
        }
        removed |= here;
    }
    if !removed && command.is_none() {
        return Ok(false);
    }
    if let Some(command) = command {
        for e in EVENTS {
            let mut group = Object::default();
            group.push("hooks", Node::Array(vec![json_handler(e, command)]));
            let group = Node::Object(group);
            match hooks.get_mut(e.name) {
                Some(Node::Array(groups)) => groups.push(group),
                Some(other) => *other = Node::Array(vec![group]),
                None => hooks.push(e.name, Node::Array(vec![group])),
            }
        }
    }
    // the other handlers keep their order, so each one's place is compared with where it was
    let mut warned = HashSet::new();
    for ((event, group, index), (_, was_group, was_index)) in others(hooks)?.into_iter().zip(was) {
        if (group, index) != (was_group, was_index) && warned.insert(event.clone()) {
            say(&format!(
                "Note: another {event} hook comes after AiPet's in hooks.json; Codex will ask you to trust it again."
            ));
        }
    }

    // a symlinked hooks.json (dotfiles): write the file it points at
    let real = real_path(path);
    let emptied = hooks.iter().len() == 0;
    if root.iter().len() == 1 && emptied && real.as_os_str() == path.as_os_str() {
        if !is_file(path) {
            return Ok(false);
        }
        backup(path);
        fs::remove_file(path).map_err(|e| thrown(&e, path))?;
        return Ok(true);
    }
    // other tools' commands keep their characters (& ' < > + and non-ASCII aren't escaped), and the file its line
    // endings
    let nl = if before.contains('\r') {
        "\r\n"
    } else if !before.is_empty() {
        "\n"
    } else {
        NEWLINE
    };
    let mut after = json_out::indented(&Node::Object(root), nl);
    if before.ends_with('\n') {
        after.push_str(nl);
    }
    let was = json_out::parse(if blank(&before) { "{}" } else { &before })?;
    if deep_equals(&was, &json_out::parse(&after)?)? {
        return Ok(false);
    }
    if is_file(path) {
        backup(path);
    }
    save(&real, &after)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    //! What the subprocess replay (tests/codex.rs) can't reach: the helpers against the C#'s own answers in
    //! tests/golden/codex/codex.json, and an edit of config.toml while something else changes it.

    use std::time::{Duration, UNIX_EPOCH};

    use serde_json::Value;

    use super::*;

    const THIS_OS: &str = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };

    fn corpus() -> Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/codex/codex.json");
        let text = fs::read_to_string(&path).unwrap_or_else(|e| {
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

    /// A scratch folder of the test's own, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("aipet-hook-codex-{test}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// IsOurs, Snake, the ordinal casing the C# compares with, and the classes of its regular expressions answer as
    /// the C#'s do. None of them depends on the OS.
    #[test]
    fn helpers_answer_as_the_csharps() {
        let corpus = corpus();
        for case in corpus["is_ours"].as_array().unwrap() {
            let command = case["command"].as_str();
            assert_eq!(is_ours(command), case["ours"] == true, "{command:?}");
        }
        for case in corpus["snake"].as_array().unwrap() {
            assert_eq!(snake(text(&case["event"])), text(&case["snake"]), "{case}");
        }
        for case in corpus["ordinal"].as_array().unwrap() {
            let (a, b) = (text(&case["a"]), text(&case["b"]));
            assert_eq!(upper(a) == upper(b), case["equal"] == true, "{a:?} {b:?}");
        }
        let ranges = |key: &str| -> Vec<[u32; 2]> {
            corpus["regex"][key]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| [0, 1].map(|i| r[i].as_u64().unwrap() as u32))
                .collect()
        };
        let digits: Vec<[u32; 2]> = DIGITS.iter().map(|&zero| [zero, zero + 9]).collect();
        assert_eq!(digits, ranges("digits"));
        let words = ranges("word");
        assert_eq!(WORD.as_chunks::<2>().0, words.as_slice());
        // and the lookup finds each range's ends, and not what lies just outside
        for &[first, last] in &words {
            for c in [first, last, first - 1, last + 1] {
                if let Some(c) = char::from_u32(c) {
                    let inside = words.iter().any(|&[f, l]| (f..=l).contains(&u32::from(c)));
                    assert_eq!(word(c), inside, "U+{:04X}", u32::from(c));
                }
            }
        }
    }

    /// BuildCommand in folders made as the C# made them: quotes, spaces (the 8.3 forms, where the volume makes them)
    /// and folders that aren't there (PowerShell's `&` form). On Unix it looks at no file.
    #[test]
    fn build_command_is_the_csharps() {
        let corpus = corpus();
        if text(&corpus["os"]) != THIS_OS {
            eprintln!(
                "skipped: codex.json was written on {} (the live replay covers this OS)",
                corpus["os"]
            );
            return;
        }
        let commands = &corpus["commands"];
        let scratch = Scratch::new("commands");
        let root = scratch.0.to_string_lossy().into_owned();
        for dir in commands["made"].as_array().into_iter().flatten() {
            fs::create_dir_all(scratch.0.join(text(dir))).unwrap();
        }
        let short_root = short_path(&scratch.0).unwrap_or_else(|| root.clone());
        let spaced = Path::new(&short_root).join("with space").to_string_lossy().into_owned();
        let short_names = short_path(&scratch.0.join("with space")) != Some(spaced);
        let mut compared = 0;
        for case in commands["cases"].as_array().unwrap() {
            let exe = text(&case["exe"]);
            let exe = if cfg!(unix) || case.get("as_is").is_some() {
                PathBuf::from(exe)
            } else {
                scratch.0.join(exe)
            };
            let via_short = !plain(&exe.to_string_lossy()) && exe.parent().and_then(short_path).is_some();
            if via_short && commands["short_names"] != short_names {
                eprintln!(
                    "{}: skipped, this volume makes 8.3 names otherwise than the C#'s",
                    exe.display()
                );
                continue;
            }
            let expected = text(&case["command"])
                .replace("{root-short}", &short_root)
                .replace("{root}", &root);
            assert_eq!(build_command(&exe), expected, "{}", exe.display());
            compared += 1;
        }
        assert!(compared >= 5, "only {compared} commands were compared");
    }

    /// Replaces config.toml as Codex does when it records trust: a new file moved over it.
    fn replace(path: &Path, text: &str) {
        let tmp = path.with_extension("codex-tmp");
        fs::write(&tmp, text).unwrap();
        fs::rename(&tmp, path).unwrap();
    }

    /// Writes config.toml in place, with a last-written time of its own: the same file, changed.
    fn rewrite(path: &Path, text: &str, seconds: u64) {
        fs::write(path, text).unwrap();
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(seconds)).unwrap();
    }

    /// The golden's "other-tools-hooks" fixture: its config.toml as it is set up in `config`.
    fn other_tools_hooks(corpus: &Value, config: &Path) -> String {
        let case = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "other-tools-hooks")
            .unwrap();
        text(&case["setup"]["files"]["codex/config.toml"]).replace("{config}", &config.to_string_lossy())
    }

    /// A Codex home with a config.toml: its config.toml and hooks.json.
    fn home(scratch: &Scratch) -> (PathBuf, PathBuf) {
        let home = scratch.0.join("codex");
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join("config.toml"), "model = \"o3\"\n").unwrap();
        (home.join("config.toml"), home.join("hooks.json"))
    }

    /// The files in a folder, by name.
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// What an edit has written next to config.toml when `meanwhile` runs: the edit and the backup, right before the
    /// replace.
    const STAGED: [&str; 3] = ["config.toml", "config.toml.aipet-backup-tmp", "config.toml.aipet-tmp"];

    /// config.toml changed once between the edit's read and its replace (at the last moment, with the edit and its
    /// backup already written): the edit reads it again, and the second attempt writes what an edit of the changed
    /// file writes when nothing is in the way, which is the C#'s (the golden's "other-tools-hooks", where it was
    /// written).
    #[test]
    fn an_edit_reads_again_when_the_file_changed_once() {
        let corpus = corpus();
        let command = text(&corpus["command"]);
        let scratch = Scratch::new("changed-once");
        let (config, hooks) = home(&scratch);
        let changed = other_tools_hooks(&corpus, &config);
        let mut said = Vec::new();
        let mut attempts = 0;
        let mut meanwhile = || {
            attempts += 1;
            assert_eq!(names(config.parent().unwrap()), STAGED, "attempt {attempts}");
            if attempts == 1 {
                replace(&config, &changed);
            }
        };
        let edited = edit_toml(
            &config,
            Some(command),
            &[hooks.as_path()],
            &mut |l| said.push(l.to_owned()),
            &mut meanwhile,
        );
        assert_eq!(edited, Ok(true));
        assert_eq!(attempts, 2, "the file was read twice");
        let written = fs::read_to_string(&config).unwrap();

        // the changed file, edited with nothing in the way
        let calm = Scratch::new("changed-once-calm");
        let (calm_config, calm_hooks) = home(&calm);
        fs::write(&calm_config, other_tools_hooks(&corpus, &calm_config)).unwrap();
        let mut calm_said = Vec::new();
        let calm_edit = edit_toml(
            &calm_config,
            Some(command),
            &[calm_hooks.as_path()],
            &mut |l| calm_said.push(l.to_owned()),
            &mut || {},
        );
        assert_eq!(calm_edit, Ok(true));
        let place = |text: String, config: &Path| text.replace(config.to_string_lossy().as_ref(), "{config}");
        assert_eq!(
            place(written.clone(), &config),
            place(fs::read_to_string(&calm_config).unwrap(), &calm_config)
        );
        assert_eq!(said, calm_said);
        if text(&corpus["os"]) == THIS_OS {
            let case = corpus["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["name"] == "other-tools-hooks")
                .unwrap();
            let csharp = text(&case["steps"][0]["files"]["codex/config.toml"]["text"])
                .replace("{command-escaped}", &command.replace('\\', "\\\\").replace('"', "\\\""))
                .replace("{config}", &config.to_string_lossy());
            assert_eq!(written, csharp);
        }
        // one backup, of the file that was edited: none for the attempt that wrote nothing
        let backups: Vec<String> = names(config.parent().unwrap())
            .into_iter()
            .filter(|n| n.starts_with("config.toml.aipet-"))
            .collect();
        assert_eq!(backups.len(), 1, "{backups:?}");
        assert_eq!(fs::read_to_string(config.with_file_name(&backups[0])).unwrap(), changed);
    }

    /// config.toml changed right before every replace (moved over, and rewritten in place): the edit reads it three
    /// times, then writes nothing and fails, saying config.toml kept changing and to close Codex and retry. --install
    /// and --uninstall print that as "Couldn't update codex settings: ..." and exit 1, as for any failure.
    #[test]
    fn an_edit_that_keeps_changing_writes_nothing() {
        let corpus = corpus();
        let scratch = Scratch::new("keeps-changing");
        let (config, hooks) = home(&scratch);
        let changed = other_tools_hooks(&corpus, &config);
        let mut attempts = 0;
        let mut meanwhile = || {
            attempts += 1;
            assert_eq!(names(config.parent().unwrap()), STAGED, "attempt {attempts}");
            let text = format!("{changed}# change {attempts}\n");
            match attempts {
                2 => rewrite(&config, &text, 1_000_000),
                _ => replace(&config, &text),
            }
        };
        let edited = edit_toml(
            &config,
            Some(text(&corpus["command"])),
            &[hooks.as_path()],
            &mut |_| {},
            &mut meanwhile,
        );
        assert_eq!(attempts, ATTEMPTS, "the file was read {attempts} times");
        let message = edited.unwrap_err();
        assert!(
            message.starts_with(&format!("{} kept changing", config.display()))
                && message.contains("Close Codex and try again"),
            "{message}"
        );
        // the last change is there as it was made, and nothing else was written: no backup, no temp file
        assert_eq!(fs::read_to_string(&config).unwrap(), format!("{changed}# change 3\n"));
        assert_eq!(names(config.parent().unwrap()), ["config.toml"]);
    }
}
