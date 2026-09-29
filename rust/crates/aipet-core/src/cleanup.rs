//! The uninstaller's hook cleanup: `aipet-hook --uninstall` for each agent this install registered
//! (`src/AiPet.Core/HookCleanup.cs`).
//!
//! install.ps1 registers the hook directly (`aipet-hook --install claude`) when Claude's plugin can't run for lack of
//! Git Bash, and that registration names the installed exe. Once the uninstaller has deleted it, Claude would fail to
//! run it on every event, so Velopack's uninstall hook runs `aipet-hook --uninstall <agent>` first, for each agent
//! whose config names this install's hook. Registrations of another copy (the portable zip, a build from source) are
//! left alone, and so are the plugins, which the agents manage themselves.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// How long each `--uninstall` gets.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The agents' config files the hook registers in, which the uninstaller reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigs {
    /// Claude's `settings.json`, found as the hook's `ClaudeConfig.Settings` finds it.
    pub claude_settings: PathBuf,
    /// Codex's user layer, where the hook's `CodexConfig` registers: `config.toml`, or `hooks.json`.
    pub codex_files: [PathBuf; 2],
}

impl AgentConfigs {
    /// `$CLAUDE_CONFIG_DIR/settings.json` (else `~/.claude`), and `config.toml` and `hooks.json` in Codex's home, as
    /// the environment has them now.
    pub fn current() -> Self {
        let claude = std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| aipet_ipc::paths::home().join(".claude"));
        let codex = aipet_ipc::paths::codex_home();
        AgentConfigs {
            claude_settings: claude.join("settings.json"),
            codex_files: [codex.join("config.toml"), codex.join("hooks.json")],
        }
    }

    /// The agents (`claude`, `codex`) whose config names one of these hook paths (`HookCleanup.Agents`).
    pub fn agents(&self, hooks: &[&str], ignore_case: bool) -> Vec<&'static str> {
        let read = |path: &Path| std::fs::read(path).ok().map(|bytes| crate::config::read_text(&bytes));
        let mut agents = Vec::new();
        if names(read(&self.claude_settings).as_deref(), hooks, ignore_case) {
            agents.push("claude");
        }
        if self
            .codex_files
            .iter()
            .any(|f| names(read(f).as_deref(), hooks, ignore_case))
        {
            agents.push("codex");
        }
        agents
    }
}

/// Whether a config file's text names one of the hook paths (`HookCleanup.Names`): with `/` or `\` (settings.json
/// has `/`, Codex's command `\`), escaped as in JSON and TOML strings (`\\`) or not (TOML literal strings), with a
/// `'` doubled as PowerShell quotes it, and as the whole file name: a path that only starts with it
/// (`aipet-hook.exe.old`) isn't the hook.
///
/// It compares as the C# does: in UTF-16 units, ignoring case as `OrdinalIgnoreCase` does (`ordinal_upper`), and
/// with what `char.IsLetterOrDigit` takes for more of the file name (`continues_name`).
pub fn names(text: Option<&str>, hooks: &[&str], ignore_case: bool) -> bool {
    let Some(text) = text.filter(|t| !t.is_empty()) else {
        return false;
    };
    let text = text.replace("\\\\", "\\");
    // the same length: a character's upper case is as long as it
    let original = units(&text, false);
    let searched = units(&text, ignore_case);
    for hook in hooks.iter().filter(|h| !h.is_empty()) {
        let plain = units(hook, ignore_case);
        let quoted = units(&hook.replace('\'', "''"), ignore_case);
        let forms = if quoted == plain {
            vec![plain]
        } else {
            vec![plain, quoted]
        };
        for form in forms {
            let mut from = 0;
            while let Some(at) = index_of(&searched, &form, from) {
                let end = at + form.len();
                if end == original.len() || !continues_name(original[end]) {
                    return true;
                }
                from = at + 1;
            }
        }
    }
    false
}

/// A string's UTF-16 units, with `\` as `/`, and each character in its [`ordinal_upper`] case when case is ignored.
fn units(s: &str, ignore_case: bool) -> Vec<u16> {
    let mut units = Vec::with_capacity(s.len());
    let mut buf = [0; 2];
    for c in s.chars() {
        let c = match c {
            '\\' => '/',
            c if ignore_case => ordinal_upper(c),
            c => c,
        };
        units.extend_from_slice(c.encode_utf16(&mut buf));
    }
    units
}

/// `OrdinalIgnoreCase`'s upper case of a character (`OrdinalCasing`): its simple upper case in Unicode's data, one
/// character as long as it, past the BMP too. .NET keeps dotless ı and long ſ apart from I and S, and on Windows,
/// the one platform where case is ignored, it cases with Windows' own ICU, which doesn't have the case pairs Unicode
/// 16 and 17 added yet. tests/golden/data/hook_cleanup.json has .NET's upper case of every character.
fn ordinal_upper(c: char) -> char {
    let shifted = |by: u32| char::from_u32(c as u32 + by).unwrap_or(c);
    match c {
        'ı' | 'ſ' => c,
        // Unicode 16's and 17's
        '\u{19B}' | '\u{264}' | '\u{1C8A}' | '\u{A7CD}' | '\u{A7CF}' | '\u{A7D3}' | '\u{A7D5}' | '\u{A7DB}' => c,
        '\u{16EBB}'..='\u{16ED3}' => c,
        // the letters whose full upper case is two (ᾳ's is ΑΙ) have a simple one too
        '\u{1F80}'..='\u{1F87}' | '\u{1F90}'..='\u{1F97}' | '\u{1FA0}'..='\u{1FA7}' => shifted(8),
        '\u{1FB3}' | '\u{1FC3}' | '\u{1FF3}' => shifted(9),
        _ => {
            let mut upper = c.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(u), None) if u.len_utf16() == c.len_utf16() => u,
                _ => c,
            }
        }
    }
}

fn index_of(text: &[u16], what: &[u16], from: usize) -> Option<usize> {
    if what.is_empty() || from > text.len() {
        return None;
    }
    text[from..]
        .windows(what.len())
        .position(|w| w == what)
        .map(|at| at + from)
}

/// `char.IsLetterOrDigit(c) || c is '.' or '-' or '_'`, on a UTF-16 unit: half a surrogate pair is neither.
fn continues_name(unit: u16) -> bool {
    let (ranges, _) = LETTERS_AND_DIGITS.as_chunks::<2>();
    matches!(unit, 0x2E | 0x2D | 0x5F) || ranges.iter().any(|&[first, last]| (first..=last).contains(&unit))
}

/// The UTF-16 units `char.IsLetterOrDigit` takes, by .NET's Unicode data: the letters (Lu, Ll, Lt, Lm and Lo) and
/// the decimal digits (Nd), in ranges, first and last. Not the marks or the other numbers, which Rust's
/// `is_alphanumeric` takes too. tests/golden/data/hook_cleanup.json has .NET's own.
const LETTERS_AND_DIGITS: [u16; 802] = [
    0x0030, 0x0039, 0x0041, 0x005A, 0x0061, 0x007A, 0x00AA, 0x00AA, 0x00B5, 0x00B5, 0x00BA, 0x00BA, 0x00C0, 0x00D6,
    0x00D8, 0x00F6, 0x00F8, 0x02C1, 0x02C6, 0x02D1, 0x02E0, 0x02E4, 0x02EC, 0x02EC, 0x02EE, 0x02EE, 0x0370, 0x0374,
    0x0376, 0x0377, 0x037A, 0x037D, 0x037F, 0x037F, 0x0386, 0x0386, 0x0388, 0x038A, 0x038C, 0x038C, 0x038E, 0x03A1,
    0x03A3, 0x03F5, 0x03F7, 0x0481, 0x048A, 0x052F, 0x0531, 0x0556, 0x0559, 0x0559, 0x0560, 0x0588, 0x05D0, 0x05EA,
    0x05EF, 0x05F2, 0x0620, 0x064A, 0x0660, 0x0669, 0x066E, 0x066F, 0x0671, 0x06D3, 0x06D5, 0x06D5, 0x06E5, 0x06E6,
    0x06EE, 0x06FC, 0x06FF, 0x06FF, 0x0710, 0x0710, 0x0712, 0x072F, 0x074D, 0x07A5, 0x07B1, 0x07B1, 0x07C0, 0x07EA,
    0x07F4, 0x07F5, 0x07FA, 0x07FA, 0x0800, 0x0815, 0x081A, 0x081A, 0x0824, 0x0824, 0x0828, 0x0828, 0x0840, 0x0858,
    0x0860, 0x086A, 0x0870, 0x0887, 0x0889, 0x088E, 0x08A0, 0x08C9, 0x0904, 0x0939, 0x093D, 0x093D, 0x0950, 0x0950,
    0x0958, 0x0961, 0x0966, 0x096F, 0x0971, 0x0980, 0x0985, 0x098C, 0x098F, 0x0990, 0x0993, 0x09A8, 0x09AA, 0x09B0,
    0x09B2, 0x09B2, 0x09B6, 0x09B9, 0x09BD, 0x09BD, 0x09CE, 0x09CE, 0x09DC, 0x09DD, 0x09DF, 0x09E1, 0x09E6, 0x09F1,
    0x09FC, 0x09FC, 0x0A05, 0x0A0A, 0x0A0F, 0x0A10, 0x0A13, 0x0A28, 0x0A2A, 0x0A30, 0x0A32, 0x0A33, 0x0A35, 0x0A36,
    0x0A38, 0x0A39, 0x0A59, 0x0A5C, 0x0A5E, 0x0A5E, 0x0A66, 0x0A6F, 0x0A72, 0x0A74, 0x0A85, 0x0A8D, 0x0A8F, 0x0A91,
    0x0A93, 0x0AA8, 0x0AAA, 0x0AB0, 0x0AB2, 0x0AB3, 0x0AB5, 0x0AB9, 0x0ABD, 0x0ABD, 0x0AD0, 0x0AD0, 0x0AE0, 0x0AE1,
    0x0AE6, 0x0AEF, 0x0AF9, 0x0AF9, 0x0B05, 0x0B0C, 0x0B0F, 0x0B10, 0x0B13, 0x0B28, 0x0B2A, 0x0B30, 0x0B32, 0x0B33,
    0x0B35, 0x0B39, 0x0B3D, 0x0B3D, 0x0B5C, 0x0B5D, 0x0B5F, 0x0B61, 0x0B66, 0x0B6F, 0x0B71, 0x0B71, 0x0B83, 0x0B83,
    0x0B85, 0x0B8A, 0x0B8E, 0x0B90, 0x0B92, 0x0B95, 0x0B99, 0x0B9A, 0x0B9C, 0x0B9C, 0x0B9E, 0x0B9F, 0x0BA3, 0x0BA4,
    0x0BA8, 0x0BAA, 0x0BAE, 0x0BB9, 0x0BD0, 0x0BD0, 0x0BE6, 0x0BEF, 0x0C05, 0x0C0C, 0x0C0E, 0x0C10, 0x0C12, 0x0C28,
    0x0C2A, 0x0C39, 0x0C3D, 0x0C3D, 0x0C58, 0x0C5A, 0x0C5D, 0x0C5D, 0x0C60, 0x0C61, 0x0C66, 0x0C6F, 0x0C80, 0x0C80,
    0x0C85, 0x0C8C, 0x0C8E, 0x0C90, 0x0C92, 0x0CA8, 0x0CAA, 0x0CB3, 0x0CB5, 0x0CB9, 0x0CBD, 0x0CBD, 0x0CDD, 0x0CDE,
    0x0CE0, 0x0CE1, 0x0CE6, 0x0CEF, 0x0CF1, 0x0CF2, 0x0D04, 0x0D0C, 0x0D0E, 0x0D10, 0x0D12, 0x0D3A, 0x0D3D, 0x0D3D,
    0x0D4E, 0x0D4E, 0x0D54, 0x0D56, 0x0D5F, 0x0D61, 0x0D66, 0x0D6F, 0x0D7A, 0x0D7F, 0x0D85, 0x0D96, 0x0D9A, 0x0DB1,
    0x0DB3, 0x0DBB, 0x0DBD, 0x0DBD, 0x0DC0, 0x0DC6, 0x0DE6, 0x0DEF, 0x0E01, 0x0E30, 0x0E32, 0x0E33, 0x0E40, 0x0E46,
    0x0E50, 0x0E59, 0x0E81, 0x0E82, 0x0E84, 0x0E84, 0x0E86, 0x0E8A, 0x0E8C, 0x0EA3, 0x0EA5, 0x0EA5, 0x0EA7, 0x0EB0,
    0x0EB2, 0x0EB3, 0x0EBD, 0x0EBD, 0x0EC0, 0x0EC4, 0x0EC6, 0x0EC6, 0x0ED0, 0x0ED9, 0x0EDC, 0x0EDF, 0x0F00, 0x0F00,
    0x0F20, 0x0F29, 0x0F40, 0x0F47, 0x0F49, 0x0F6C, 0x0F88, 0x0F8C, 0x1000, 0x102A, 0x103F, 0x1049, 0x1050, 0x1055,
    0x105A, 0x105D, 0x1061, 0x1061, 0x1065, 0x1066, 0x106E, 0x1070, 0x1075, 0x1081, 0x108E, 0x108E, 0x1090, 0x1099,
    0x10A0, 0x10C5, 0x10C7, 0x10C7, 0x10CD, 0x10CD, 0x10D0, 0x10FA, 0x10FC, 0x1248, 0x124A, 0x124D, 0x1250, 0x1256,
    0x1258, 0x1258, 0x125A, 0x125D, 0x1260, 0x1288, 0x128A, 0x128D, 0x1290, 0x12B0, 0x12B2, 0x12B5, 0x12B8, 0x12BE,
    0x12C0, 0x12C0, 0x12C2, 0x12C5, 0x12C8, 0x12D6, 0x12D8, 0x1310, 0x1312, 0x1315, 0x1318, 0x135A, 0x1380, 0x138F,
    0x13A0, 0x13F5, 0x13F8, 0x13FD, 0x1401, 0x166C, 0x166F, 0x167F, 0x1681, 0x169A, 0x16A0, 0x16EA, 0x16F1, 0x16F8,
    0x1700, 0x1711, 0x171F, 0x1731, 0x1740, 0x1751, 0x1760, 0x176C, 0x176E, 0x1770, 0x1780, 0x17B3, 0x17D7, 0x17D7,
    0x17DC, 0x17DC, 0x17E0, 0x17E9, 0x1810, 0x1819, 0x1820, 0x1878, 0x1880, 0x1884, 0x1887, 0x18A8, 0x18AA, 0x18AA,
    0x18B0, 0x18F5, 0x1900, 0x191E, 0x1946, 0x196D, 0x1970, 0x1974, 0x1980, 0x19AB, 0x19B0, 0x19C9, 0x19D0, 0x19D9,
    0x1A00, 0x1A16, 0x1A20, 0x1A54, 0x1A80, 0x1A89, 0x1A90, 0x1A99, 0x1AA7, 0x1AA7, 0x1B05, 0x1B33, 0x1B45, 0x1B4C,
    0x1B50, 0x1B59, 0x1B83, 0x1BA0, 0x1BAE, 0x1BE5, 0x1C00, 0x1C23, 0x1C40, 0x1C49, 0x1C4D, 0x1C7D, 0x1C80, 0x1C8A,
    0x1C90, 0x1CBA, 0x1CBD, 0x1CBF, 0x1CE9, 0x1CEC, 0x1CEE, 0x1CF3, 0x1CF5, 0x1CF6, 0x1CFA, 0x1CFA, 0x1D00, 0x1DBF,
    0x1E00, 0x1F15, 0x1F18, 0x1F1D, 0x1F20, 0x1F45, 0x1F48, 0x1F4D, 0x1F50, 0x1F57, 0x1F59, 0x1F59, 0x1F5B, 0x1F5B,
    0x1F5D, 0x1F5D, 0x1F5F, 0x1F7D, 0x1F80, 0x1FB4, 0x1FB6, 0x1FBC, 0x1FBE, 0x1FBE, 0x1FC2, 0x1FC4, 0x1FC6, 0x1FCC,
    0x1FD0, 0x1FD3, 0x1FD6, 0x1FDB, 0x1FE0, 0x1FEC, 0x1FF2, 0x1FF4, 0x1FF6, 0x1FFC, 0x2071, 0x2071, 0x207F, 0x207F,
    0x2090, 0x209C, 0x2102, 0x2102, 0x2107, 0x2107, 0x210A, 0x2113, 0x2115, 0x2115, 0x2119, 0x211D, 0x2124, 0x2124,
    0x2126, 0x2126, 0x2128, 0x2128, 0x212A, 0x212D, 0x212F, 0x2139, 0x213C, 0x213F, 0x2145, 0x2149, 0x214E, 0x214E,
    0x2183, 0x2184, 0x2C00, 0x2CE4, 0x2CEB, 0x2CEE, 0x2CF2, 0x2CF3, 0x2D00, 0x2D25, 0x2D27, 0x2D27, 0x2D2D, 0x2D2D,
    0x2D30, 0x2D67, 0x2D6F, 0x2D6F, 0x2D80, 0x2D96, 0x2DA0, 0x2DA6, 0x2DA8, 0x2DAE, 0x2DB0, 0x2DB6, 0x2DB8, 0x2DBE,
    0x2DC0, 0x2DC6, 0x2DC8, 0x2DCE, 0x2DD0, 0x2DD6, 0x2DD8, 0x2DDE, 0x2E2F, 0x2E2F, 0x3005, 0x3006, 0x3031, 0x3035,
    0x303B, 0x303C, 0x3041, 0x3096, 0x309D, 0x309F, 0x30A1, 0x30FA, 0x30FC, 0x30FF, 0x3105, 0x312F, 0x3131, 0x318E,
    0x31A0, 0x31BF, 0x31F0, 0x31FF, 0x3400, 0x4DBF, 0x4E00, 0xA48C, 0xA4D0, 0xA4FD, 0xA500, 0xA60C, 0xA610, 0xA62B,
    0xA640, 0xA66E, 0xA67F, 0xA69D, 0xA6A0, 0xA6E5, 0xA717, 0xA71F, 0xA722, 0xA788, 0xA78B, 0xA7CD, 0xA7D0, 0xA7D1,
    0xA7D3, 0xA7D3, 0xA7D5, 0xA7DC, 0xA7F2, 0xA801, 0xA803, 0xA805, 0xA807, 0xA80A, 0xA80C, 0xA822, 0xA840, 0xA873,
    0xA882, 0xA8B3, 0xA8D0, 0xA8D9, 0xA8F2, 0xA8F7, 0xA8FB, 0xA8FB, 0xA8FD, 0xA8FE, 0xA900, 0xA925, 0xA930, 0xA946,
    0xA960, 0xA97C, 0xA984, 0xA9B2, 0xA9CF, 0xA9D9, 0xA9E0, 0xA9E4, 0xA9E6, 0xA9FE, 0xAA00, 0xAA28, 0xAA40, 0xAA42,
    0xAA44, 0xAA4B, 0xAA50, 0xAA59, 0xAA60, 0xAA76, 0xAA7A, 0xAA7A, 0xAA7E, 0xAAAF, 0xAAB1, 0xAAB1, 0xAAB5, 0xAAB6,
    0xAAB9, 0xAABD, 0xAAC0, 0xAAC0, 0xAAC2, 0xAAC2, 0xAADB, 0xAADD, 0xAAE0, 0xAAEA, 0xAAF2, 0xAAF4, 0xAB01, 0xAB06,
    0xAB09, 0xAB0E, 0xAB11, 0xAB16, 0xAB20, 0xAB26, 0xAB28, 0xAB2E, 0xAB30, 0xAB5A, 0xAB5C, 0xAB69, 0xAB70, 0xABE2,
    0xABF0, 0xABF9, 0xAC00, 0xD7A3, 0xD7B0, 0xD7C6, 0xD7CB, 0xD7FB, 0xF900, 0xFA6D, 0xFA70, 0xFAD9, 0xFB00, 0xFB06,
    0xFB13, 0xFB17, 0xFB1D, 0xFB1D, 0xFB1F, 0xFB28, 0xFB2A, 0xFB36, 0xFB38, 0xFB3C, 0xFB3E, 0xFB3E, 0xFB40, 0xFB41,
    0xFB43, 0xFB44, 0xFB46, 0xFBB1, 0xFBD3, 0xFD3D, 0xFD50, 0xFD8F, 0xFD92, 0xFDC7, 0xFDF0, 0xFDFB, 0xFE70, 0xFE74,
    0xFE76, 0xFEFC, 0xFF10, 0xFF19, 0xFF21, 0xFF3A, 0xFF41, 0xFF5A, 0xFF66, 0xFFBE, 0xFFC2, 0xFFC7, 0xFFCA, 0xFFCF,
    0xFFD2, 0xFFD7, 0xFFDA, 0xFFDC,
];

/// Runs `<hook> --uninstall <agent>` for each agent whose config names this hook (`HookCleanup.Run`): by its path,
/// or on Windows with its folder, or the whole path, in 8.3 short form, as Codex's command has it when the path has
/// spaces. Each run gets 10 s and no window, and what it says goes to the app's log. Nothing here fails the
/// uninstall.
pub fn run(hook: &Path) {
    run_with(&AgentConfigs::current(), hook, &mut |line| crate::log::write(&line));
}

fn run_with(configs: &AgentConfigs, hook: &Path, log: &mut dyn FnMut(String)) {
    if !hook.is_file() {
        return;
    }
    let paths: Vec<String> = forms(hook).iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let hooks: Vec<&str> = paths.iter().map(String::as_str).collect();
    for agent in configs.agents(&hooks, cfg!(windows)) {
        log(match uninstall(hook, agent) {
            Ok(Some((code, said))) => {
                format!(
                    "uninstall: aipet-hook --uninstall {agent} (exit {code}): {}",
                    one_line(said.trim())
                )
            }
            Ok(None) => format!("uninstall: aipet-hook --uninstall {agent} took too long; stopped it"),
            Err(e) => format!("uninstall: couldn't run aipet-hook --uninstall {agent}: {e}"),
        });
    }
}

/// Runs the hook's `--uninstall`: its exit code and what it said (stdout, then stderr), or `None` when it took too
/// long and was stopped.
fn uninstall(hook: &Path, agent: &str) -> std::io::Result<Option<(i32, String)>> {
    let mut command = Command::new(hook);
    command
        .args(["--uninstall", agent])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn()?;
    // read as it comes, so a full pipe can't hold the hook up
    let output = drain(child.stdout.take());
    let errors = drain(child.stderr.take());
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            stop(child);
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(10));
    };
    let said = output.join().unwrap_or_default() + &errors.join().unwrap_or_default();
    Ok(Some((exit_code(status), said)))
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn stop(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// .NET's `Process.ExitCode`: 128 plus the signal for a process a signal ended.
fn exit_code(status: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return 128 + signal;
    }
    status.code().unwrap_or(-1)
}

/// `ReplaceLineEndings(" ")`: CR LF, CR, LF, NEL, LS, PS and FF each as one space.
fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                chars.next_if_eq(&'\n');
                out.push(' ');
            }
            '\n' | '\u{85}' | '\u{2028}' | '\u{2029}' | '\u{C}' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// The hook's path.
#[cfg(not(windows))]
fn forms(hook: &Path) -> Vec<PathBuf> {
    vec![hook.to_path_buf()]
}

/// The hook's path, then with its folder in 8.3 form, then all of it in 8.3 form (older installs shortened the file
/// name too).
#[cfg(windows)]
fn forms(hook: &Path) -> Vec<PathBuf> {
    let mut forms = vec![hook.to_path_buf()];
    if let Some(dir) = hook.parent().and_then(short_path)
        && let Some(name) = hook.file_name()
    {
        forms.push(dir.join(name));
    }
    forms.extend(short_path(hook));
    forms
}

/// `C:\Users\First Last\…` as `C:\Users\FIRSTL~1\…`; `None` when there's no short name.
#[cfg(windows)]
fn short_path(path: &Path) -> Option<PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    let long: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut buf = [0u16; 1024];
    // SAFETY: long is NUL-terminated; buf's length is what is passed
    let n = unsafe { GetShortPathNameW(long.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    (n > 0 && n < buf.len()).then(|| PathBuf::from(std::ffi::OsString::from_wide(&buf[..n])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> (Dir, AgentConfigs) {
        let dir = std::env::temp_dir().join(format!("aipet-core-cleanup-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let configs = AgentConfigs {
            claude_settings: dir.join("settings.json"),
            codex_files: [dir.join("config.toml"), dir.join("hooks.json")],
        };
        (Dir(dir), configs)
    }

    /// What the C# makes of every character (rust/golden's data mode; tests/data.rs has how to write it again).
    fn chars() -> serde_json::Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/data/hook_cleanup.json");
        let golden: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        golden["chars"].clone()
    }

    fn hex(s: &str) -> u32 {
        u32::from_str_radix(s, 16).unwrap()
    }

    /// Every character's upper case is `OrdinalIgnoreCase`'s.
    #[test]
    fn every_character_is_cased_as_dotnet_cases_it() {
        let mut dotnet = std::collections::HashMap::new();
        for pair in chars()["upper"].as_array().unwrap() {
            let (c, upper) = pair.as_str().unwrap().split_once(' ').unwrap();
            dotnet.insert(hex(c), hex(upper));
        }
        let wrong: Vec<String> = (0..=0x10FFFF)
            .filter_map(char::from_u32)
            .filter_map(|c| {
                let want = dotnet.get(&(c as u32)).copied().unwrap_or(c as u32);
                let got = ordinal_upper(c) as u32;
                (got != want).then(|| format!("U+{:04X}: U+{got:04X}, not U+{want:04X}", c as u32))
            })
            .collect();
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Every UTF-16 unit continues a file name, or doesn't, as `char.IsLetterOrDigit` and the C#'s `. - _` have it.
    #[test]
    fn every_unit_continues_a_name_as_in_dotnet() {
        let mut dotnet = [false; 0x10000];
        for range in chars()["letter_or_digit"].as_array().unwrap() {
            let (first, last) = range.as_str().unwrap().split_once("..").unwrap();
            dotnet[hex(first) as usize..=hex(last) as usize].fill(true);
        }
        for c in ['.', '-', '_'] {
            dotnet[c as usize] = true;
        }
        let wrong: Vec<String> = (0..=u16::MAX)
            .filter(|&u| continues_name(u) != dotnet[usize::from(u)])
            .map(|u| format!("U+{u:04X}"))
            .collect();
        assert!(wrong.is_empty(), "{wrong:?}");
    }

    #[test]
    fn line_endings_become_spaces() {
        assert_eq!(
            one_line("a\r\nb\rc\nd\u{85}e\u{2028}f\u{2029}g\u{C}h\r\n\ni"),
            "a b c d e f g h  i"
        );
    }

    #[test]
    fn the_configs_are_where_the_hook_registers() {
        let configs = AgentConfigs::current();
        assert!(configs.claude_settings.ends_with("settings.json"));
        let codex = aipet_ipc::paths::codex_home();
        assert_eq!(
            configs.codex_files,
            [codex.join("config.toml"), codex.join("hooks.json")]
        );
    }

    /// Run starts `<hook> --uninstall <agent>` for each agent that names it, and only those
    /// (HookCleanupTests.Run_UninstallsFromTheAgentsThatNameTheHook). The hook is a stand-in script.
    #[cfg(unix)]
    #[test]
    fn run_uninstalls_from_the_agents_that_name_the_hook() {
        use std::os::unix::fs::PermissionsExt;
        let (d, configs) = dir("run");
        let hook = d.0.join("aipet-hook");
        let calls = d.0.join("calls");
        fs::write(
            &hook,
            format!(
                "#!/bin/sh\necho \"$*\" >> '{}'\necho removed\necho warned >&2\n",
                calls.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            &configs.claude_settings,
            format!(
                r#"{{"hooks":{{"Stop":[{{"hooks":[{{"type":"command","command":"{}"}}]}}]}}}}"#,
                hook.display()
            ),
        )
        .unwrap();
        fs::write(
            &configs.codex_files[0],
            "[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ncommand = \"'/elsewhere/aipet-hook' --agent codex\"\n",
        )
        .unwrap();

        let mut logged = Vec::new();
        run_with(&configs, &hook, &mut |line| logged.push(line));
        assert_eq!(fs::read_to_string(&calls).unwrap(), "--uninstall claude\n");
        assert_eq!(
            logged,
            ["uninstall: aipet-hook --uninstall claude (exit 0): removed warned"]
        );

        // a hook that's gone (a broken install) has nothing to run
        fs::remove_file(&calls).unwrap();
        run_with(&configs, &d.0.join("missing").join("aipet-hook"), &mut |line| {
            logged.push(line)
        });
        assert!(!calls.exists());
        assert_eq!(logged.len(), 1);
    }

    /// A hook that hangs is stopped, and Run returns well within the 30 s Velopack gives the uninstall hook
    /// (HookCleanupTests.Run_StopsAHookThatHangs).
    #[cfg(unix)]
    #[test]
    fn run_stops_a_hook_that_hangs() {
        use std::os::unix::fs::PermissionsExt;
        let (d, configs) = dir("hang");
        let hook = d.0.join("aipet-hook");
        fs::write(&hook, "#!/bin/sh\nexec sleep 60\n").unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            &configs.claude_settings,
            format!(r#"{{"command":"{}"}}"#, hook.display()),
        )
        .unwrap();
        let started = Instant::now();
        let mut logged = Vec::new();
        run_with(&configs, &hook, &mut |line| logged.push(line));
        let took = started.elapsed().as_secs_f64();
        assert!((9.0..20.0).contains(&took), "{took}");
        assert_eq!(
            logged,
            ["uninstall: aipet-hook --uninstall claude took too long; stopped it"]
        );
    }

    /// On Windows a program that is surely there stands in for the hook: it refuses the arguments, and what it says
    /// is logged on one line with its exit code. Paths are matched whatever their case.
    #[cfg(windows)]
    #[test]
    fn run_logs_what_the_hook_said() {
        let (_d, configs) = dir("run");
        let windows = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| r"C:\Windows".into());
        let hook = windows.join("System32").join("whoami.exe");
        let named = hook.to_string_lossy().replace('\\', "/").to_uppercase();
        fs::write(&configs.claude_settings, format!(r#"{{"command":"{named}"}}"#)).unwrap();
        let mut logged = Vec::new();
        run_with(&configs, &hook, &mut |line| logged.push(line));
        assert_eq!(logged.len(), 1, "{logged:?}");
        assert!(
            logged[0].starts_with("uninstall: aipet-hook --uninstall claude (exit 1): "),
            "{}",
            logged[0]
        );
        assert!(!logged[0].contains(['\r', '\n']), "{}", logged[0]);
    }
}
