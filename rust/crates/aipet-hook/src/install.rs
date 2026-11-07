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

/// The text as `StringComparison.OrdinalIgnoreCase` compares it in the hook: each character as its simple upper case
/// (`ǆ` is `Ǆ`, `ᾳ` is `ᾼ`, `ß` stays), but for `ı` and `ſ`, which .NET's ordinal casing keeps as they are.
///
/// The hook is built with InvariantGlobalization, where .NET cases with its own Unicode data: Unicode 16 in .NET 10, so
/// the case pairs Unicode 17 added (which Rust's data has) aren't pairs there. The app cases with ICU instead, which
/// has fewer of Unicode 16's (aipet-core's cleanup.rs). tests/golden/codex/codex.json has the hook's upper case of
/// every character.
pub(crate) fn upper(s: &str) -> String {
    s.chars().map(ordinal_upper).collect()
}

fn ordinal_upper(c: char) -> char {
    match c {
        'ı' | 'ſ' => c,
        // Unicode 17's
        '\u{A7CF}' | '\u{A7D3}' | '\u{A7D5}' | '\u{16EBB}'..='\u{16ED3}' => c,
        // the small letters with ypogegrammeni: their full upper case is two characters (with a capital iota), their
        // simple one the letter with prosgegrammeni
        '\u{1F80}'..='\u{1F87}' | '\u{1F90}'..='\u{1F97}' | '\u{1FA0}'..='\u{1FA7}' => {
            char::from_u32(u32::from(c) + 8).unwrap_or(c)
        }
        '\u{1FB3}' => '\u{1FBC}',
        '\u{1FC3}' => '\u{1FCC}',
        '\u{1FF3}' => '\u{1FFC}',
        // anywhere else the full upper case is the simple one where it is one character of the same length in UTF-16
        // (.NET's casing never changes a string's length), and none otherwise
        _ => {
            let mut up = c.to_uppercase();
            match (up.next(), up.next()) {
                (Some(u), None) if u.len_utf16() == c.len_utf16() => u,
                _ => c,
            }
        }
    }
}

// ------------------------------------------------------------------ files
/// `File.Exists`: something that isn't a folder is at the path. That is a file or a symlink to one, and also a symlink
/// that can't be followed (its target isn't there, or it loops), since .NET then looks at the link itself: reading
/// through it then fails. A folder, a link to one, or a path that can't be looked at at all isn't.
pub(crate) fn is_file(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(m) => !m.is_dir(),
        Err(_) => fs::symlink_metadata(path).is_ok_and(|link| !folder(&link)),
    }
}

/// Whether a symlink's own metadata is a folder's: on Windows a link to a folder is one (it has the folder
/// attribute); on Unix a link never is.
fn folder(link: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        link.file_attributes() & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY != 0
    }
    #[cfg(not(windows))]
    {
        link.is_dir()
    }
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
    stage(real, text)?.commit()
}

/// [`save`]'s first half: the text written next to the file (`<file>.aipet-tmp`), which [`Staged::commit`] then moves
/// over it. An edit that must still find the file as it read it checks that in between, right before the replace.
pub(crate) fn stage(real: &Path, text: &str) -> Result<Staged, String> {
    let mut tmp = real.as_os_str().to_owned();
    tmp.push(".aipet-tmp");
    let staged = Staged {
        tmp: PathBuf::from(tmp),
        real: real.to_owned(),
        moved: false,
    };
    // a leftover would keep its own mode: the one given here only applies to a new file
    match fs::remove_file(&staged.tmp) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(thrown(&e, &staged.tmp)),
        _ => {}
    }
    write_new(&staged.tmp, text, mode_of(real))?;
    Ok(staged)
}

/// A replacement [`stage`] wrote next to its file. Dropped without [`Staged::commit`] (or when that fails), it is
/// removed, and the file stays as it was.
pub(crate) struct Staged {
    tmp: PathBuf,
    real: PathBuf,
    moved: bool,
}

impl Staged {
    /// [`save`]'s second half: the replacement moved over the file.
    pub(crate) fn commit(mut self) -> Result<(), String> {
        fs::rename(&self.tmp, &self.real).map_err(|e| moved(&e, &self.tmp))?;
        self.moved = true;
        Ok(())
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if !self.moved {
            let _ = fs::remove_file(&self.tmp);
        }
    }
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

/// The message .NET's file API throws for an error on `path`: `UnauthorizedAccessException` for a denied access (and
/// for a folder read as a file), `FileNotFoundException` for a missing file, `DirectoryNotFoundException` when its
/// folder is missing too, and else the system's own text with the path.
pub(crate) fn thrown(e: &io::Error, path: &Path) -> String {
    let shown = path.display();
    match e.kind() {
        io::ErrorKind::PermissionDenied | io::ErrorKind::IsADirectory => {
            format!("Access to the path '{shown}' is denied.")
        }
        io::ErrorKind::NotFound if file_not_found(e, path) => format!("Could not find file '{shown}'."),
        io::ErrorKind::NotFound => format!("Could not find a part of the path '{shown}'."),
        _ => format!("{} : '{shown}'", system_text(e)),
    }
}

/// Whether a missing `path` is `FileNotFoundException` rather than `DirectoryNotFoundException`: Windows tells the
/// two apart (`ERROR_FILE_NOT_FOUND`, `ERROR_PATH_NOT_FOUND`); on Unix .NET blames the file when its folder is there.
pub(crate) fn file_not_found(e: &io::Error, path: &Path) -> bool {
    e.kind() == io::ErrorKind::NotFound
        && if cfg!(windows) {
            e.raw_os_error() != Some(3)
        } else {
            path.parent()
                .is_none_or(|dir| dir.as_os_str().is_empty() || dir.is_dir())
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
/// the change.
pub(crate) fn backup(path: &Path) {
    let Some(named) = backup_path(path) else {
        return;
    };
    if fs::copy(path, named).is_ok() {
        prune_backups(path);
    }
}

/// The name [`backup`] copies a file to, `<file>.aipet-<local time>.bak`. `None` for a path with no folder or name.
fn backup_path(path: &Path) -> Option<PathBuf> {
    path.parent()?;
    path.file_name()?;
    let mut named = path.as_os_str().to_owned();
    named.push(format!(".aipet-{}.bak", local_stamp()));
    Some(PathBuf::from(named))
}

/// [`backup`] for an edit that is written only if the file is still the one it read: the copy is made under a name of
/// its own (`<file>.aipet-backup-tmp`) before that check, and takes its backup name, the older backups pruned, once the
/// file is replaced ([`StagedBackup::keep`]). Dropped instead, it is removed: an edit that wrote nothing leaves no
/// backup.
pub(crate) struct StagedBackup {
    path: PathBuf,
    copy: PathBuf,
    named: PathBuf,
    kept: bool,
}

impl StagedBackup {
    /// The copy. `None` when there's none to be had, which never fails the change.
    pub(crate) fn make(path: &Path) -> Option<StagedBackup> {
        let mut copy = path.as_os_str().to_owned();
        copy.push(".aipet-backup-tmp");
        let staged = StagedBackup {
            path: path.to_owned(),
            copy: PathBuf::from(copy),
            named: backup_path(path)?,
            kept: false,
        };
        fs::copy(path, &staged.copy).ok()?;
        Some(staged)
    }

    /// The file was replaced, or its replace was tried (the C# backs up before it replaces, so the backup is there
    /// even when that failed): the copy takes its backup name, and the newest three backups are kept. When it can't
    /// have that name, it is dropped, as a `File.Copy` that failed leaves no backup.
    pub(crate) fn keep(mut self) {
        if fs::rename(&self.copy, &self.named).is_ok() {
            self.kept = true;
            prune_backups(&self.path);
        }
    }
}

impl Drop for StagedBackup {
    fn drop(&mut self) {
        if !self.kept {
            let _ = fs::remove_file(&self.copy);
        }
    }
}

/// The newest three of a file's backups kept, as `Backup` keeps them.
fn prune_backups(path: &Path) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return;
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder of the test's own, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("aipet-hook-install-{test}-{}", std::process::id()));
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

    /// `File.Exists` as .NET answers it: a file, a link to one, and a link it can't follow (to nothing, or looping) are
    /// there; a folder, a link to one, and nothing at all aren't.
    #[test]
    fn is_file_answers_as_file_exists() {
        let scratch = Scratch::new("exists");
        let file = scratch.0.join("file");
        fs::write(&file, "").unwrap();
        assert!(is_file(&file));
        assert!(!is_file(&scratch.0));
        assert!(!is_file(&scratch.0.join("missing")));
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            for (link, target, there) in [
                ("to-file", "file", true),
                ("to-nothing", "missing", true),
                ("loop", "loop", true),
                ("to-folder", ".", false),
            ] {
                symlink(target, scratch.0.join(link)).unwrap();
                assert_eq!(is_file(&scratch.0.join(link)), there, "{link}");
            }
        }
    }

    /// What .NET throws for a file it can't read, by name: the file missing, its folder missing too, a folder in its
    /// place; and on Unix, a link to nothing (the file it names is missing) and one that loops (the system's words).
    #[test]
    fn a_file_that_cant_be_read_is_said_as_dotnet_says_it() {
        let scratch = Scratch::new("thrown");
        let missing = scratch.0.join("settings.json");
        assert_eq!(
            read_text(&missing),
            Err(format!("Could not find file '{}'.", missing.display()))
        );
        let deeper = scratch.0.join("claude").join("settings.json");
        assert_eq!(
            read_text(&deeper),
            Err(format!("Could not find a part of the path '{}'.", deeper.display()))
        );
        assert_eq!(
            read_text(&scratch.0),
            Err(format!("Access to the path '{}' is denied.", scratch.0.display()))
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let dangling = scratch.0.join("dangling.json");
            symlink("none.json", &dangling).unwrap();
            assert_eq!(
                read_text(&dangling),
                Err(format!("Could not find file '{}'.", dangling.display()))
            );
            let looping = scratch.0.join("loop.json");
            symlink("loop.json", &looping).unwrap();
            let words = system_text(&io::Error::from_raw_os_error(libc::ELOOP));
            assert_eq!(read_text(&looping), Err(format!("{words} : '{}'", looping.display())));
        }
    }

    /// `Path.GetFullPath` takes a relative path from the working folder before `..` comes out: none is lost where it
    /// climbs above the path's first folder.
    #[test]
    fn full_path_takes_a_relative_path_from_the_working_folder() {
        let here = std::env::current_dir().unwrap();
        assert_eq!(
            full_path(Path::new("claude/../../dotfiles/settings.json")),
            Some(here.parent().unwrap().join("dotfiles").join("settings.json"))
        );
    }
}
