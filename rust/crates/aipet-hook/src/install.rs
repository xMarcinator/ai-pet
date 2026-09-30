//! `--install` and `--uninstall` (`src/AiPet.Hook/Install.cs:1-71`): the checks before an agent's registration is
//! touched, the line that says why it failed, and the atomic write every registration saves with.
//!
//! What the C# prints goes through `Console`, one line at a time with `Environment.NewLine`; so does this ([`say`],
//! [`warn`]).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::{claude, codex};

/// `Environment.NewLine`, which `Console.WriteLine` ends a line with and the settings files are written with.
pub(crate) const NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

pub(crate) const USAGE: &str = "usage: aipet-hook --install|--uninstall claude|codex";

/// `Console.WriteLine`. A closed stdout is no failure of the registration's.
pub(crate) fn say(line: &str) {
    let _ = write!(io::stdout().lock(), "{line}{NEWLINE}");
}

/// `Console.Error.WriteLine`.
pub(crate) fn warn(line: &str) {
    let _ = write!(io::stderr().lock(), "{line}{NEWLINE}");
}

/// `Install.Run(action, agent)`: `agent` in lower case. What a registration throws (its `Exception.Message`) comes
/// back as the `Err` of [`claude::install`], [`codex::install`] and the uninstalls, and is said here.
pub(crate) fn run(action: &str, agent: &str) -> i32 {
    if agent != "claude" && agent != "codex" {
        warn(USAGE);
        return 2;
    }
    // what runs this is what gets registered: anything but aipet-hook (a copy under another name, or a runner that
    // loads it) would fail on every event, and --uninstall couldn't tell it from anyone else's hook
    let exe = std::env::current_exe().ok();
    let install = action == "--install";
    if install && !exe.as_deref().is_some_and(is_hook) {
        let shown = exe.as_deref().map_or_else(|| "unknown".into(), Path::to_string_lossy);
        warn(&format!(
            "--install registers the program that runs it, and that's {shown}, not aipet-hook."
        ));
        warn(
            "Run the aipet-hook executable itself (as the installers do), not `dotnet aipet-hook.dll`. Nothing was changed.",
        );
        return 1;
    }
    let (out, err) = (&mut |line: &str| say(line), &mut |line: &str| warn(line));
    let done = match (agent, exe) {
        ("claude", Some(exe)) if install => claude::install(&exe, &claude::process_vars, out),
        ("claude", _) => claude::uninstall(&claude::process_vars, out),
        (_, Some(exe)) if install => codex::install(&exe, out, err),
        _ => codex::uninstall(out, err),
    };
    done.unwrap_or_else(|message| {
        warn(&format!("Couldn't update {agent} settings: {message}"));
        1
    })
}

/// `IsHook`: the program's file name is aipet-hook, in any case, with or without `.exe`.
fn is_hook(exe: &Path) -> bool {
    exe.file_name()
        .map(|name| upper(&name.to_string_lossy()))
        .is_some_and(|name| name == "AIPET-HOOK" || name == "AIPET-HOOK.EXE")
}

/// The text as `StringComparison.OrdinalIgnoreCase` compares it: each character as its simple upper case (`ǆ` is
/// `Ǆ`, `ᾳ` is `ᾼ`, `ß` stays), but for `ı` and `ſ`, which .NET's ordinal casing keeps as they are.
pub(crate) fn upper(s: &str) -> String {
    s.chars().map(ordinal_upper).collect()
}

fn ordinal_upper(c: char) -> char {
    match c {
        'ı' | 'ſ' => c,
        // the small letters with ypogegrammeni: their full upper case is two characters (with a capital iota), their
        // simple one the letter with prosgegrammeni
        '\u{1F80}'..='\u{1F87}' | '\u{1F90}'..='\u{1F97}' | '\u{1FA0}'..='\u{1FA7}' => {
            char::from_u32(u32::from(c) + 8).unwrap_or(c)
        }
        '\u{1FB3}' => '\u{1FBC}',
        '\u{1FC3}' => '\u{1FCC}',
        '\u{1FF3}' => '\u{1FFC}',
        // anywhere else the full upper case is the simple one where it is one character, and none where it is more
        _ => {
            let mut up = c.to_uppercase();
            match (up.next(), up.next()) {
                (Some(u), None) => u,
                _ => c,
            }
        }
    }
}

// ------------------------------------------------------------------ files
/// `File.Exists`: a file, or a symlink to one. A folder, a dangling link or what can't be looked at isn't.
pub(crate) fn is_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| !m.is_dir())
}

/// `File.ReadAllText`: the text as a `StreamReader` decodes it (see [`crate::event::decoded`]).
pub(crate) fn read_text(path: &Path) -> Result<String, String> {
    fs::read(path)
        .map(|bytes| crate::event::decoded(&bytes))
        .map_err(|e| thrown(&e, path))
}

/// `Install.Save(real, text)`: the text replaces the file at once, written next to it first and then moved over it,
/// so a reader or a crash never sees half a file. `real` is the file itself, not a link to it. It keeps the old
/// file's Unix mode (Codex keeps config.toml 0600, and these files often hold tokens); a new file is 0600.
pub(crate) fn save(real: &Path, text: &str) -> Result<(), String> {
    let mut tmp = real.as_os_str().to_owned();
    tmp.push(".aipet-tmp");
    let tmp = PathBuf::from(tmp);
    // a leftover would keep its own mode: the one given here only applies to a new file
    match fs::remove_file(&tmp) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(thrown(&e, &tmp)),
        _ => {}
    }
    let written = write_new(&tmp, text, mode_of(real));
    let saved = written.and_then(|()| fs::rename(&tmp, real).map_err(|e| moved(&e, &tmp)));
    if saved.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    saved
}

/// The mode a replacement gets: the file's, or 0600 for a new one.
#[cfg(unix)]
fn mode_of(real: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(real)
        .ok()
        .filter(|m| !m.is_dir())
        .map_or(0o600, |m| m.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn mode_of(_: &Path) -> u32 {
    0o600
}

/// A new file with the text as UTF-8, no byte order mark. On Unix it is created with `mode`, which is set again
/// after, since the umask may have taken some away.
fn write_new(path: &Path, text: &str, mode: u32) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, mode);
    let mut file = options.open(path).map_err(|e| thrown(&e, path))?;
    file.write_all(text.as_bytes()).map_err(|e| thrown(&e, path))?;
    drop(file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| thrown(&e, path))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

/// The message .NET's file API throws for an error on `path`: `UnauthorizedAccessException` for a denied access,
/// else the system's own text with the path.
pub(crate) fn thrown(e: &io::Error, path: &Path) -> String {
    match e.kind() {
        io::ErrorKind::PermissionDenied => format!("Access to the path '{}' is denied.", path.display()),
        _ => format!("{} : '{}'", system_text(e), path.display()),
    }
}

/// What `File.Move` throws: on Windows a denied move names no path.
fn moved(e: &io::Error, from: &Path) -> String {
    if cfg!(windows) {
        match e.kind() {
            io::ErrorKind::PermissionDenied => "Access to the path is denied.".to_owned(),
            _ => system_text(e),
        }
    } else {
        thrown(e, from)
    }
}

/// An error's text without Rust's " (os error N)".
fn system_text(e: &io::Error) -> String {
    let text = e.to_string();
    match text.rfind(" (os error ") {
        Some(at) => text[..at].to_owned(),
        None => text,
    }
}

/// `RealPath`: the file a symlink ends at (`File.ResolveLinkTarget(path, returnFinalTarget: true)`), each link's
/// relative target taken from where that link is, as a full path (`FileSystemInfo.FullName`). Not a link, a chain
/// past .NET's 40 links, or no full path to be had (`FullName` throws, and `RealPath` catches): the path itself.
/// (Codex's registration; claude.rs has its own copy.)
pub(crate) fn real_path(path: &Path) -> PathBuf {
    const MAX_FOLLOWED_LINKS: usize = 40;
    let Ok(mut target) = fs::read_link(path) else {
        return path.to_owned();
    };
    let mut current = path.to_owned();
    for _ in 0..MAX_FOLLOWED_LINKS {
        current = match current.parent() {
            Some(dir) if target.is_relative() => dir.join(&target),
            _ => target,
        };
        match fs::read_link(&current) {
            Ok(next) => target = next,
            Err(_) => return full_path(&current).unwrap_or_else(|| path.to_owned()),
        }
    }
    path.to_owned()
}

/// `Path.GetFullPath`: a relative path taken from the working folder, then `.` and `..` taken out of the text. `None`
/// where .NET throws: there is no working folder.
#[cfg(unix)]
fn full_path(path: &Path) -> Option<PathBuf> {
    use std::path::Component;

    let mut full = PathBuf::new();
    for part in std::path::absolute(path).ok()?.components() {
        match part {
            Component::ParentDir => {
                full.pop();
            }
            Component::CurDir => {}
            other => full.push(other),
        }
    }
    Some(full)
}

#[cfg(not(unix))]
fn full_path(path: &Path) -> Option<PathBuf> {
    std::path::absolute(path).ok()
}

/// `Backup`: `<file>.aipet-<local time>.bak` before each change, and the newest three of them kept. It never fails
/// the change. (Codex's registration; claude.rs has its own copy.)
pub(crate) fn backup(path: &Path) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return;
    };
    let mut copy = path.as_os_str().to_owned();
    copy.push(format!(".aipet-{}.bak", local_stamp()));
    if fs::copy(path, copy).is_err() {
        return;
    }
    // Directory.GetFiles(dir, "<file>.aipet-*.bak"): the name's case counts on Unix only
    let fold = |s: &str| if cfg!(windows) { upper(s) } else { s.to_owned() };
    let prefix = fold(&format!("{}.aipet-", name.to_string_lossy()));
    let suffix = fold(".bak");
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut backups: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .filter(|e| {
            let name = fold(&e.file_name().to_string_lossy());
            name.len() >= prefix.len() + suffix.len() && name.starts_with(&prefix) && name.ends_with(&suffix)
        })
        .map(|e| e.path())
        .collect();
    backups.sort_unstable_by(|a, b| b.as_os_str().cmp(a.as_os_str()));
    for old in backups.iter().skip(3) {
        if fs::remove_file(old).is_err() {
            return;
        }
    }
}

/// The local time as `yyyyMMdd-HHmmss`.
#[cfg(unix)]
fn local_stamp() -> String {
    // SAFETY: time only returns the time when given no pointer; localtime_r writes only tm
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    // SAFETY: an all-zero tm is a valid value to be filled in
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are to live locals
    if unsafe { libc::localtime_r(&now, &mut tm) }.is_null() {
        return "00000000-000000".to_owned();
    }
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

#[cfg(windows)]
fn local_stamp() -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;

    // SAFETY: an all-zero SYSTEMTIME is a valid value to be filled in
    let mut now = unsafe { std::mem::zeroed() };
    // SAFETY: GetLocalTime only writes the SYSTEMTIME it is given
    unsafe { GetLocalTime(&mut now) };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute, now.wSecond
    )
}
