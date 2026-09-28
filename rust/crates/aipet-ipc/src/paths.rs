//! The app's data folder and the other places the hook and the pet both know (`src/AiPet.Core/Paths.cs`).
//!
//! - Windows: `%LOCALAPPDATA%\AiPet` (the known folder, not the variable).
//! - Linux: `$XDG_DATA_HOME/AiPet`, by default `~/.local/share/AiPet`.
//! - macOS: `~/Library/Application Support/AiPet` (not supported).
//!
//! `AIPET_DATA_DIR` points the app and the hooks at another folder (for testing without touching the real one),
//! made absolute as `Path.GetFullPath` does. The user's folders are .NET's `Environment.GetFolderPath` for
//! `LocalApplicationData` and `UserProfile`, found the same way, so the Rust and the .NET builds of either side use
//! the same files. Like the C#'s static fields, the folders and [`data_dir`] are read once per process.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Where environment variables come from: the process's own, or a test's.
pub(crate) type Vars<'a> = &'a dyn Fn(&str) -> Option<OsString>;

pub(crate) fn process_vars(name: &str) -> Option<OsString> {
    std::env::var_os(name)
}

/// A variable that is set and not empty (the C#'s `is { Length: > 0 }`).
pub(crate) fn nonempty(vars: Vars, name: &str) -> Option<OsString> {
    vars(name).filter(|v| !v.is_empty())
}

/// `Paths.DataDir`.
pub fn data_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| data_dir_in(&process_vars, folders()))
}

/// `Paths.Config`: the pet's window and preferences.
pub fn config() -> PathBuf {
    data_dir().join("config.json")
}

/// `Paths.Log`: the app's log.
pub fn log() -> PathBuf {
    data_dir().join("aipet.log")
}

/// `Paths.HookEventsLog`: the pet's line per hook event it got (trimmed to its last half past 64 KB), so you can see
/// the agents run them.
pub fn hook_events_log() -> PathBuf {
    data_dir().join("hook-events.log")
}

/// `Paths.CodexHome`: `$CODEX_HOME` as it is, else `~/.codex`. Read on every call, as the C#'s property is.
pub fn codex_home() -> PathBuf {
    codex_home_in(&process_vars, &folders().home)
}

/// .NET's `Environment.GetFolderPath(SpecialFolder.UserProfile)`, which the hook's registration and doctor read too
/// (`Install.cs:79`, `Doctor.cs:328`). Empty when .NET would give none (on Unix, a home folder that can't be read).
pub fn home() -> &'static Path {
    &folders().home
}

/// .NET's `Environment.GetFolderPath(SpecialFolder.LocalApplicationData)` (the doctor looks for Codex in it,
/// `Doctor.cs:117`). Empty when .NET would give none.
pub fn local_app_data() -> &'static Path {
    &folders().local_app_data
}

/// The user's folders, as .NET finds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Folders {
    pub(crate) local_app_data: PathBuf,
    pub(crate) home: PathBuf,
}

fn folders() -> &'static Folders {
    static FOLDERS: OnceLock<Folders> = OnceLock::new();
    FOLDERS.get_or_init(|| Folders::of(&process_vars))
}

impl Folders {
    /// Unix (`Environment.GetFolderPathCore.Unix.cs`): the home folder is `$HOME`, else the passwd entry's of the
    /// effective uid, else `/`. LocalApplicationData is `$XDG_DATA_HOME` when it is an absolute path, else
    /// `~/.local/share` (macOS: `~/Library/Application Support`). A folder the user can't read comes back empty.
    #[cfg(unix)]
    pub(crate) fn of(vars: Vars) -> Folders {
        use std::os::unix::ffi::OsStrExt;

        let home = nonempty(vars, "HOME")
            .map(PathBuf::from)
            .or_else(passwd_home)
            .unwrap_or_else(|| "/".into());
        let local_app_data = if cfg!(target_os = "macos") {
            home.join("Library").join("Application Support")
        } else {
            match nonempty(vars, "XDG_DATA_HOME") {
                Some(data) if data.as_bytes().starts_with(b"/") => data.into(),
                _ => home.join(".local").join("share"),
            }
        };
        Folders {
            local_app_data: readable(local_app_data),
            home: readable(home),
        }
    }

    /// Windows (`Environment.Win32.cs`): the known folders, which the variables don't change; `LOCALAPPDATA` and
    /// `USERPROFILE` only stand in when the shell can't give one.
    #[cfg(windows)]
    pub(crate) fn of(vars: Vars) -> Folders {
        use windows_sys::Win32::UI::Shell::{FOLDERID_LocalAppData, FOLDERID_Profile};

        let folder = |id, fallback| {
            known_folder(id)
                .or_else(|| vars(fallback).map(PathBuf::from))
                .unwrap_or_default()
        };
        Folders {
            local_app_data: folder(&FOLDERID_LocalAppData, "LOCALAPPDATA"),
            home: folder(&FOLDERID_Profile, "USERPROFILE"),
        }
    }
}

/// `Paths.DataDir` in an environment.
pub(crate) fn data_dir_in(vars: Vars, folders: &Folders) -> PathBuf {
    match nonempty(vars, "AIPET_DATA_DIR") {
        Some(dir) => full_path(&dir),
        None => {
            let base = if folders.local_app_data.as_os_str().is_empty() {
                folders.home.join(".local").join("share")
            } else {
                folders.local_app_data.clone()
            };
            base.join("AiPet")
        }
    }
}

/// `Paths.CodexHome` in an environment.
pub(crate) fn codex_home_in(vars: Vars, home: &Path) -> PathBuf {
    nonempty(vars, "CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"))
}

/// .NET's validation of a special folder (`SpecialFolderOption.None`): the path when the user can read it, else
/// empty.
#[cfg(unix)]
fn readable(path: PathBuf) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;

    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return PathBuf::new();
    };
    // SAFETY: c is a NUL-terminated path
    if unsafe { libc::access(c.as_ptr(), libc::R_OK) } == 0 {
        path
    } else {
        PathBuf::new()
    }
}

/// The effective uid's home folder in the passwd database, as .NET falls back to when `HOME` isn't set.
#[cfg(unix)]
fn passwd_home() -> Option<PathBuf> {
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;

    let mut buf = vec![0 as libc::c_char; 1024];
    loop {
        // SAFETY: an all-zero passwd is a valid value to be filled in
        let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found = std::ptr::null_mut();
        // SAFETY: the buffer and its length match; pwd and found are written only
        let rc = unsafe { libc::getpwuid_r(libc::geteuid(), &mut pwd, buf.as_mut_ptr(), buf.len(), &mut found) };
        if rc == libc::ERANGE && buf.len() < 1 << 20 {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        if rc != 0 || found.is_null() || pwd.pw_dir.is_null() {
            return None;
        }
        // SAFETY: getpwuid_r pointed pw_dir at a NUL-terminated string in buf
        let dir = unsafe { CStr::from_ptr(pwd.pw_dir) }.to_bytes();
        return (!dir.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(dir)));
    }
}

/// `SHGetKnownFolderPath` with no flags, so a folder that doesn't exist gives none, as for .NET.
#[cfg(windows)]
fn known_folder(id: &windows_sys::core::GUID) -> Option<PathBuf> {
    use windows_sys::Win32::Foundation::S_OK;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{KF_FLAG_DEFAULT, SHGetKnownFolderPath};

    let mut path = std::ptr::null_mut();
    // SAFETY: id is a valid GUID; path is written only
    let hr = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, 0, &mut path) };
    // SAFETY: on success path is a NUL-terminated string
    let found = (hr == S_OK && !path.is_null()).then(|| PathBuf::from(unsafe { crate::wide::from_ptr(path) }));
    // SAFETY: the shell's allocation, freed even on failure, as its documentation asks
    unsafe { CoTaskMemFree(path as *const _) };
    found
}

/// `Path.GetFullPath` on Unix: relative to the current directory, with `.`, `..` and repeated separators taken out
/// of the text (no symlink is resolved and nothing has to exist). A trailing `/` stays.
#[cfg(unix)]
pub(crate) fn full_path(path: &std::ffi::OsStr) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let bytes = path.as_bytes();
    let absolute = if bytes.starts_with(b"/") {
        bytes.to_vec()
    } else {
        // .NET throws without a current directory; the path stays as it is here, and so is never the pet's
        let Ok(cwd) = std::env::current_dir() else {
            return PathBuf::from(path);
        };
        let mut joined = cwd.into_os_string().into_vec();
        if !joined.ends_with(b"/") {
            joined.push(b'/');
        }
        joined.extend_from_slice(bytes);
        joined
    };
    PathBuf::from(OsString::from_vec(remove_relative_segments(&absolute)))
}

/// `Path.GetFullPath` on Windows: `GetFullPathNameW`, as std's `absolute` calls it (a `\\?\` path stays as it is),
/// and then 8.3 short names spelt out in full, as .NET does when the result has a `~`.
#[cfg(windows)]
pub(crate) fn full_path(path: &std::ffi::OsStr) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    let Ok(full) = std::path::absolute(path) else {
        return PathBuf::from(path);
    };
    let wide: Vec<u16> = full.as_os_str().encode_wide().collect();
    // .NET also expands a \\.\ device path; the pet's folders are never one, so that is left out here
    let device = [r"\\?\", r"\\.\", r"\??\"]
        .iter()
        .any(|p| wide.starts_with(&p.encode_utf16().collect::<Vec<_>>()));
    if !wide.contains(&(b'~' as u16)) || device {
        return full;
    }
    PathBuf::from(OsString::from_wide(&expand_short_names(&wide, long_path_name)))
}

/// .NET's `PathInternal.RemoveRelativeSegments` for an absolute Unix path: `//` and `/./` collapse, and `/../`
/// takes the segment before it away (never the root).
#[cfg(any(unix, test))]
fn remove_relative_segments(path: &[u8]) -> Vec<u8> {
    debug_assert!(path.starts_with(b"/"));
    let mut out = Vec::with_capacity(path.len());
    let mut i = 0;
    while i < path.len() {
        let c = path[i];
        if c == b'/' && i + 1 < path.len() {
            // "parent//child"
            if path[i + 1] == b'/' {
                i += 1;
                continue;
            }
            // "parent/./child"
            if (i + 2 == path.len() || path[i + 2] == b'/') && path[i + 1] == b'.' {
                i += 2;
                continue;
            }
            // "parent/child/../grandchild": back to the last separator (keeping the root's when it is the last)
            if i + 2 < path.len()
                && (i + 3 == path.len() || path[i + 3] == b'/')
                && path[i + 1] == b'.'
                && path[i + 2] == b'.'
            {
                match out.iter().rposition(|&b| b == b'/') {
                    Some(s) => out.truncate(if i + 3 >= path.len() && s == 0 { s + 1 } else { s }),
                    None => out.clear(),
                }
                i += 3;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    if out.is_empty() {
        out.push(b'/');
    }
    out
}

#[cfg(any(windows, test))]
const ERROR_FILE_NOT_FOUND: u32 = 2;
#[cfg(any(windows, test))]
const ERROR_PATH_NOT_FOUND: u32 = 3;

/// .NET's `PathHelper.TryExpandShortFileName` for a full `C:\…` or `\\server\share\…` path: `long_path` (that is
/// `GetLongPathNameW`, through `\\?\` so long paths work) on the path, or else on the longest folder of it that
/// exists, with the rest put back as it was. On any other error, or when no part past the root exists, the path
/// stays as it is.
#[cfg(any(windows, test))]
fn expand_short_names(full: &[u16], long_path: impl Fn(&[u16]) -> Result<Vec<u16>, u32>) -> Vec<u16> {
    const SEP: u16 = b'\\' as u16;
    let unc = full.starts_with(&[SEP, SEP]);
    let (prefix, kept) = if unc { (r"\\?\UNC\", 2) } else { (r"\\?\", 0) };
    let input: Vec<u16> = prefix.encode_utf16().chain(full[kept..].iter().copied()).collect();
    let added = input.len() - full.len();
    let root = root_length(full) + added;
    let mut end = input.len();
    loop {
        match long_path(&input[..end]) {
            Ok(mut long) => {
                long.extend_from_slice(&input[end..]);
                // back from \\?\UNC\server to \\server, or from \\?\C:\ to C:\
                let skip = if unc { 8 } else { 4 };
                if long.len() < skip {
                    return full.to_vec();
                }
                let back = if unc { &[SEP, SEP][..] } else { &[][..] };
                return back.iter().copied().chain(long[skip..].iter().copied()).collect();
            }
            Err(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) => {
                // the folder the part just tried is in; .NET's search starts before the path's last character
                let search = if end == input.len() { end - 1 } else { end };
                match input[..search].iter().rposition(|&c| c == SEP) {
                    Some(sep) if sep > root => end = sep,
                    _ => return full.to_vec(),
                }
            }
            Err(_) => return full.to_vec(),
        }
    }
}

/// .NET's `PathInternal.GetRootLength` for the forms a full path has here: `C:\` (3), or `\\server\share` (up to
/// the separator after the share).
#[cfg(any(windows, test))]
fn root_length(path: &[u16]) -> usize {
    let sep = |c: u16| c == b'\\' as u16 || c == b'/' as u16;
    if path.len() > 1 && sep(path[0]) && sep(path[1]) {
        // past server\share: the second separator after the prefix ends it
        let mut i = 2;
        let mut separators = 2;
        while i < path.len() {
            if sep(path[i]) {
                separators -= 1;
                if separators == 0 {
                    break;
                }
            }
            i += 1;
        }
        i
    } else if path.len() > 1 && path[1] == b':' as u16 {
        if path.len() > 2 && sep(path[2]) { 3 } else { 2 }
    } else {
        usize::from(!path.is_empty() && sep(path[0]))
    }
}

/// `GetLongPathNameW`: the path with its short names spelt out, or the Win32 error.
#[cfg(windows)]
fn long_path_name(short: &[u16]) -> Result<Vec<u16>, u32> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Storage::FileSystem::GetLongPathNameW;

    let input: Vec<u16> = short.iter().copied().chain(Some(0)).collect();
    let mut buf = vec![0u16; input.len() + 64];
    loop {
        // SAFETY: input is NUL-terminated; buf's length is what is passed
        let n = unsafe { GetLongPathNameW(input.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
        if n == 0 {
            // SAFETY: reads this thread's last error
            return Err(unsafe { GetLastError() });
        }
        // too small: n is the size it needs, with the NUL
        if n >= buf.len() {
            buf.resize(n + 1, 0);
            continue;
        }
        buf.truncate(n);
        return Ok(buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn vars_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = pairs.iter().map(|(k, v)| (k.to_string(), OsString::from(v))).collect();
        move |name| map.get(name).cloned()
    }

    fn folders(local_app_data: &str, home: &str) -> Folders {
        Folders {
            local_app_data: local_app_data.into(),
            home: home.into(),
        }
    }

    #[test]
    fn data_dir_is_local_app_data_or_the_override() {
        let f = folders("/home/u/.local/share", "/home/u");
        assert_eq!(data_dir_in(&vars_of(&[]), &f), Path::new("/home/u/.local/share/AiPet"));
        // empty is unset
        assert_eq!(
            data_dir_in(&vars_of(&[("AIPET_DATA_DIR", "")]), &f),
            Path::new("/home/u/.local/share/AiPet")
        );
        // no LocalApplicationData: the home's .local/share, even when that is relative (no home either)
        assert_eq!(
            data_dir_in(&vars_of(&[]), &folders("", "/home/u")),
            Path::new("/home/u/.local/share/AiPet")
        );
        assert_eq!(
            data_dir_in(&vars_of(&[]), &folders("", "")),
            Path::new(".local/share/AiPet")
        );
        #[cfg(unix)]
        {
            let over = vars_of(&[("AIPET_DATA_DIR", "/tmp/x/../pet//data/.")]);
            assert_eq!(data_dir_in(&over, &f), Path::new("/tmp/pet/data"));
            let relative = vars_of(&[("AIPET_DATA_DIR", "rel/../data")]);
            assert_eq!(
                data_dir_in(&relative, &f),
                std::env::current_dir().unwrap().join("data")
            );
        }
    }

    #[test]
    fn codex_home_is_the_variable_as_it_is() {
        assert_eq!(
            codex_home_in(&vars_of(&[]), Path::new("/home/u")),
            Path::new("/home/u/.codex")
        );
        assert_eq!(
            codex_home_in(&vars_of(&[("CODEX_HOME", "")]), Path::new("/home/u")),
            Path::new("/home/u/.codex")
        );
        // not made absolute
        assert_eq!(
            codex_home_in(&vars_of(&[("CODEX_HOME", "rel/../c")]), Path::new("/h")),
            Path::new("rel/../c")
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_folders_follow_home_and_xdg_data_home() {
        let tmp = std::env::temp_dir().join(format!("aipet-ipc-folders-{}", std::process::id()));
        let data = tmp.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let home = tmp.to_str().unwrap();
        let of = |pairs: &[(&str, &str)]| Folders::of(&vars_of(pairs));
        if !cfg!(target_os = "macos") {
            // an absolute XDG_DATA_HOME that exists
            assert_eq!(
                of(&[("HOME", home), ("XDG_DATA_HOME", data.to_str().unwrap())]),
                folders(data.to_str().unwrap(), home)
            );
            // a relative one is ignored; ~/.local/share doesn't exist here, so .NET gives none
            assert_eq!(of(&[("HOME", home), ("XDG_DATA_HOME", "data")]), folders("", home));
            // one that doesn't exist gives none, and isn't replaced by ~/.local/share
            std::fs::create_dir_all(tmp.join(".local/share")).unwrap();
            let missing = tmp.join("missing");
            assert_eq!(
                of(&[("HOME", home), ("XDG_DATA_HOME", missing.to_str().unwrap())]),
                folders("", home)
            );
            assert_eq!(
                of(&[("HOME", home)]),
                folders(tmp.join(".local/share").to_str().unwrap(), home)
            );
        }
        // a home that doesn't exist gives none
        let gone = tmp.join("gone");
        assert_eq!(of(&[("HOME", gone.to_str().unwrap())]).home, PathBuf::new());
        // no HOME: the passwd entry's
        assert_eq!(of(&[]).home, readable(passwd_home().unwrap_or_else(|| "/".into())));
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// Each expected path is what .NET 10's `Path.GetFullPath` gives for the same text.
    #[test]
    fn relative_segments_come_out_as_dotnet_takes_them_out() {
        for (path, net) in [
            ("/a/b/../c", "/a/c"),
            ("/a/b/..", "/a"),
            ("/a/..", "/"),
            ("/..", "/"),
            ("/../..", "/"),
            ("/a/b/../../../c", "/c"),
            ("/a/./b", "/a/b"),
            ("/a/.", "/a"),
            ("/.", "/"),
            ("/a//b", "/a/b"),
            ("//a", "/a"),
            ("/a/", "/a/"),
            ("/a//", "/a/"),
            ("/a/..b/.c/...", "/a/..b/.c/..."),
            ("/", "/"),
        ] {
            assert_eq!(
                String::from_utf8(remove_relative_segments(path.as_bytes())).unwrap(),
                net,
                "{path}"
            );
        }
    }

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn root_lengths_are_dotnets() {
        assert_eq!(root_length(&w(r"C:\Users\x")), 3);
        assert_eq!(root_length(&w(r"C:")), 2);
        assert_eq!(root_length(&w(r"\\server\share\x")), r"\\server\share".len());
        assert_eq!(root_length(&w(r"\\server\share")), r"\\server\share".len());
        assert_eq!(root_length(&w(r"\x")), 1);
    }

    /// A file system of long names: `C:\Users\runneradmin\AppData\Local\Temp` exists, and its short name is
    /// `RUNNER~1`; `\\srv\share\Long Name` is `\\srv\share\LONGNA~1`. It answers as `GetLongPathNameW` does,
    /// through `\\?\`.
    fn fake_long_path(p: &[u16]) -> Result<Vec<u16>, u32> {
        let p = String::from_utf16(p).unwrap();
        let known = [
            (r"\\?\C:\Users\RUNNER~1", r"\\?\C:\Users\runneradmin"),
            (r"\\?\C:\Users\RUNNER~1\AppData", r"\\?\C:\Users\runneradmin\AppData"),
            (
                r"\\?\C:\Users\RUNNER~1\AppData\Local\Temp",
                r"\\?\C:\Users\runneradmin\AppData\Local\Temp",
            ),
            (r"\\?\UNC\srv\share\LONGNA~1", r"\\?\UNC\srv\share\Long Name"),
        ];
        if p == r"\\?\C:\Locked~1" {
            return Err(5); // access denied
        }
        match known.iter().find(|(short, _)| *short == p) {
            Some((_, long)) => Ok(w(long)),
            None => Err(if p.matches('\\').count() > 5 {
                ERROR_PATH_NOT_FOUND
            } else {
                ERROR_FILE_NOT_FOUND
            }),
        }
    }

    #[test]
    fn short_names_are_spelt_out_for_the_part_that_exists() {
        let expand = |p: &str| String::from_utf16(&expand_short_names(&w(p), fake_long_path)).unwrap();
        // the whole path exists
        assert_eq!(
            expand(r"C:\Users\RUNNER~1\AppData\Local\Temp"),
            r"C:\Users\runneradmin\AppData\Local\Temp"
        );
        // only a folder of it: the rest is put back as it was
        assert_eq!(
            expand(r"C:\Users\RUNNER~1\AppData\Local\Temp\aipet-x\data"),
            r"C:\Users\runneradmin\AppData\Local\Temp\aipet-x\data"
        );
        assert_eq!(expand(r"C:\Users\RUNNER~1\new~dir\"), r"C:\Users\runneradmin\new~dir\");
        // UNC
        assert_eq!(expand(r"\\srv\share\LONGNA~1\x"), r"\\srv\share\Long Name\x");
        // nothing past the root exists, or another error: as it was
        assert_eq!(expand(r"C:\NOSUCH~1\x"), r"C:\NOSUCH~1\x");
        assert_eq!(expand(r"C:\Locked~1"), r"C:\Locked~1");
    }
}
