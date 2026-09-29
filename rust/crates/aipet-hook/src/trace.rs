//! The hook's trace log in the temp folder, written only once the pet was reached
//! (`src/AiPet.Hook/Program.cs:193-220`).
//!
//! Problems only, one line each: `HH:mm:ss user=<user> pipe=<endpoint> <line>`. The temp folder is writable even when
//! an agent runs hooks as a restricted sandbox user. The log is deleted and begun again once it is past 64 KB.
//! - Windows: `%TMP%\aipet-hook.log` (else `%TEMP%`). The folder is the user's own.
//! - Unix: `$TMPDIR/aipet-hook-<euid>.log` (else `/tmp`). That folder is shared, so the log is the user's own and
//!   only they can read it, and another user may have put a file or a link at its name first (where
//!   `fs.protected_regular` or `protected_symlinks` is off). This goes further than the C# on purpose (R2):
//!   - it opens the name without following a link (`O_NOFOLLOW`: a link, even a dangling one, is refused by the open
//!     itself, so none is followed and no target is made), and without waiting (`O_NONBLOCK`: a FIFO put there fails
//!     at once instead of holding up the hook);
//!   - through the open file it must be a regular file with one link, owned by the effective user, with mode exactly
//!     0600, and on Linux `/proc/self/fd` must name it where it was opened. Otherwise nothing is written;
//!   - rotation deletes the name only while it is still the file that was checked (its device and inode), then
//!     starts a new file the same way. A name swapped meanwhile is left alone, and nothing is written.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Past this size the log is begun again.
const ROTATE_AT: u64 = 64 * 1024;

const NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// `Program.Trace`: one line in the log. It never fails: a log that can't be written is left alone.
pub(crate) fn trace(line: &str) {
    let endpoint = aipet_ipc::endpoint::endpoint().to_string_lossy();
    let text = format!("{} user={} pipe={endpoint} {line}{NEWLINE}", clock(), user_name());
    let _ = append(&path(), text.as_bytes());
}

/// `$TMPDIR/aipet-hook-<euid>.log`: `Path.GetTempPath()`, which is `/tmp` when `TMPDIR` is unset or empty.
#[cfg(unix)]
fn path() -> PathBuf {
    let temp = match std::env::var_os("TMPDIR") {
        Some(t) if !t.is_empty() => PathBuf::from(t),
        _ => PathBuf::from("/tmp"),
    };
    // SAFETY: geteuid has no preconditions
    temp.join(format!("aipet-hook-{}.log", unsafe { libc::geteuid() }))
}

/// `%TMP%\aipet-hook.log`, else `%TEMP%`: not `GetTempPath` first, which in .NET never returns when `TMP` is longer
/// than `MAX_PATH`. A `TMP` that is set but empty doesn't fall back to `TEMP`, as in the C#.
#[cfg(windows)]
fn path() -> PathBuf {
    let temp = match std::env::var_os("TMP").or_else(|| std::env::var_os("TEMP")) {
        Some(t) if !t.is_empty() => PathBuf::from(t),
        _ => std::env::temp_dir(),
    };
    temp.join("aipet-hook.log")
}

/// The local time, `HH:mm:ss`.
#[cfg(unix)]
fn clock() -> String {
    // SAFETY: time only returns the time when given no pointer; localtime_r writes only tm
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    // SAFETY: an all-zero tm is a valid value to be filled in
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are to live locals
    if unsafe { libc::localtime_r(&now, &mut tm) }.is_null() {
        return "--:--:--".to_owned();
    }
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

#[cfg(windows)]
fn clock() -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;

    // SAFETY: an all-zero SYSTEMTIME is a valid value to be filled in
    let mut now = unsafe { std::mem::zeroed() };
    // SAFETY: GetLocalTime only writes the SYSTEMTIME it is given
    unsafe { GetLocalTime(&mut now) };
    format!("{:02}:{:02}:{:02}", now.wHour, now.wMinute, now.wSecond)
}

/// .NET's `Environment.UserName` on Unix: the effective user's name in the password database, or empty.
#[cfg(unix)]
fn user_name() -> String {
    // SAFETY: an all-zero passwd is a valid value to be filled in
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found = std::ptr::null_mut();
    let mut buf = vec![0 as libc::c_char; 1024];
    loop {
        // SAFETY: entry, buf and found are live, and buf.len() is buf's size; geteuid has no preconditions
        let rc = unsafe { libc::getpwuid_r(libc::geteuid(), &mut entry, buf.as_mut_ptr(), buf.len(), &mut found) };
        if rc == libc::ERANGE && buf.len() < 1 << 20 {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        if rc != 0 || found.is_null() || entry.pw_name.is_null() {
            return String::new();
        }
        // SAFETY: pw_name points at a NUL-terminated string in buf, which is still alive
        return unsafe { std::ffi::CStr::from_ptr(entry.pw_name) }
            .to_string_lossy()
            .into_owned();
    }
}

#[cfg(windows)]
fn user_name() -> String {
    aipet_ipc::endpoint::user_name().to_string_lossy().into_owned()
}

/// Adds `text` to the log at `path`, the way the module's doc says; whether it was written.
#[cfg(unix)]
fn append(path: &Path, text: &[u8]) -> bool {
    unix::append(path, text)
}

/// The C#'s own way: a log past the size is deleted, then opened for appending (made when missing).
#[cfg(windows)]
fn append(path: &Path, text: &[u8]) -> bool {
    let big = std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() > ROTATE_AT);
    if big && std::fs::remove_file(path).is_err() {
        return false;
    }
    // shared for reading, writing and deleting, as the C# opens it (Rust's default on Windows)
    let log = std::fs::OpenOptions::new().append(true).create(true).open(path);
    log.and_then(|mut log| log.write_all(text)).is_ok()
}

#[cfg(unix)]
mod unix {
    use std::fs::{File, Metadata, OpenOptions};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::path::Path;

    use super::{ROTATE_AT, Write};

    pub(super) fn append(path: &Path, text: &[u8]) -> bool {
        let Some((log, seen)) = opened(path, false) else {
            return false;
        };
        let log = if seen.size > ROTATE_AT {
            match rotated(&seen, path) {
                Some(log) => log,
                None => return false,
            }
        } else {
            log
        };
        (&log).write_all(text).is_ok()
    }

    /// The file at `path`, opened for appending (made 0600 when missing, or only made when `fresh`) and checked.
    pub(super) fn opened(path: &Path, fresh: bool) -> Option<(File, Seen)> {
        let mut options = OpenOptions::new();
        options
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY);
        if fresh {
            options.create_new(true);
        } else {
            options.create(true);
        }
        let log = options.open(path).ok()?;
        let seen = checked(&log, path)?;
        Some((log, seen))
    }

    /// What the checks look at, as the open file has it.
    #[derive(Clone, Copy, Debug)]
    pub(super) struct Seen {
        pub(super) regular: bool,
        pub(super) links: u64,
        pub(super) uid: u32,
        pub(super) mode: u32,
        pub(super) size: u64,
        pub(super) dev: u64,
        pub(super) ino: u64,
    }

    impl Seen {
        fn of(m: &Metadata) -> Seen {
            Seen {
                regular: m.file_type().is_file(),
                links: m.nlink(),
                uid: m.uid(),
                mode: m.mode(),
                size: m.size(),
                dev: m.dev(),
                ino: m.ino(),
            }
        }

        /// Ours alone: a regular file with no other name (a hard link would lead to a file of another name), owned
        /// by `euid`, and with no permission but its owner's reading and writing.
        pub(super) fn ours(&self, euid: u32) -> bool {
            self.regular && self.links == 1 && self.uid == euid && self.mode & 0o7777 == 0o600
        }
    }

    /// The open file's checks (`fstat` on it, and where it is open); what it saw when they pass.
    pub(super) fn checked(log: &File, path: &Path) -> Option<Seen> {
        let seen = Seen::of(&log.metadata().ok()?);
        // SAFETY: geteuid has no preconditions
        (seen.ours(unsafe { libc::geteuid() }) && opened_as(log, path)).then_some(seen)
    }

    /// Whether `/proc/self/fd/<n>` names the file at `path`: the folder as it really is (links resolved), and the
    /// log's name in it. A file renamed or deleted since it was opened is named otherwise.
    #[cfg(target_os = "linux")]
    fn opened_as(log: &File, path: &Path) -> bool {
        use std::os::unix::io::AsRawFd;

        let Ok(open) = std::fs::read_link(format!("/proc/self/fd/{}", log.as_raw_fd())) else {
            return false;
        };
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            return false;
        };
        let dir = if dir.as_os_str().is_empty() {
            Path::new(".")
        } else {
            dir
        };
        std::fs::canonicalize(dir).is_ok_and(|dir| open == dir.join(name))
    }

    /// No `/proc` elsewhere: the checks on the open file are what there is.
    #[cfg(not(target_os = "linux"))]
    fn opened_as(_: &File, _: &Path) -> bool {
        true
    }

    /// The log begun again: its name deleted only while it still is the file `seen` describes (`lstat` against the
    /// open file's `fstat`), then a new file made there the same way. `None` when the name leads elsewhere now, or
    /// the new file isn't ours alone.
    pub(super) fn rotated(seen: &Seen, path: &Path) -> Option<File> {
        let now = std::fs::symlink_metadata(path).ok()?;
        if (now.dev(), now.ino()) != (seen.dev, seen.ino) {
            return None;
        }
        std::fs::remove_file(path).ok()?;
        opened(path, true).map(|(log, _)| log)
    }
}

#[cfg(all(test, unix))]
mod tests {
    #[cfg(target_os = "linux")]
    use super::unix::checked;
    use super::unix::{Seen, opened, rotated};
    use super::*;
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    /// A folder of its own for a test's log, removed with it.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("aipet-hook-trace-{}-{test}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn log(&self) -> PathBuf {
            self.0.join("aipet-hook-test.log")
        }

        /// A file of its own in the folder, with `content` and `mode`.
        fn file(&self, name: &str, content: &[u8], mode: u32) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, content).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn mode(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().mode() & 0o7777
    }

    const LINE: &[u8] = b"a line\n";

    #[test]
    fn a_missing_log_is_made_private_and_appended_to() {
        let s = Scratch::new("new");
        assert!(append(&s.log(), LINE));
        assert!(append(&s.log(), LINE));
        assert_eq!(fs::read(s.log()).unwrap(), [LINE, LINE].concat());
        assert_eq!(mode(&s.log()), 0o600);
    }

    /// Past 64 KB the log is begun again with the new line; at 64 KB it is appended to.
    #[test]
    fn a_log_past_64_kb_is_begun_again() {
        let s = Scratch::new("rotate");
        for (size, after) in [
            (ROTATE_AT, ROTATE_AT as usize + LINE.len()),
            (ROTATE_AT + 1, LINE.len()),
        ] {
            let log = s.file("aipet-hook-test.log", &vec![b'x'; size as usize], 0o600);
            assert!(append(&log, LINE), "{size}");
            let content = fs::read(&log).unwrap();
            assert_eq!(content.len(), after, "{size}");
            assert!(content.ends_with(LINE));
            assert_eq!(mode(&log), 0o600);
        }
    }

    /// What someone else may have put at the log's name, each left as it was, with nothing written anywhere.
    #[test]
    fn what_isnt_the_users_own_log_is_left_alone() {
        enum Put {
            Link,
            DanglingLink,
            Mode(u32),
            HardLink,
            Fifo,
        }
        for put in [
            Put::Link,
            Put::DanglingLink,
            Put::Mode(0o644),
            Put::Mode(0o666),
            Put::Mode(0o700),
            Put::Mode(0o4600),
            Put::HardLink,
            Put::Fifo,
        ] {
            let s = Scratch::new("foreign");
            let log = s.log();
            let target = s.0.join("keys");
            let what = match put {
                Put::Link => {
                    s.file("keys", b"secret", 0o600);
                    std::os::unix::fs::symlink(&target, &log).unwrap();
                    "a link"
                }
                Put::DanglingLink => {
                    std::os::unix::fs::symlink(&target, &log).unwrap();
                    "a dangling link"
                }
                Put::Mode(m) => {
                    s.file("aipet-hook-test.log", b"theirs", m);
                    "a foreign mode"
                }
                Put::HardLink => {
                    s.file("keys", b"secret", 0o600);
                    fs::hard_link(&target, &log).unwrap();
                    "a hard link"
                }
                Put::Fifo => {
                    let c = std::ffi::CString::new(log.as_os_str().as_encoded_bytes()).unwrap();
                    // SAFETY: a NUL-terminated path
                    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
                    "a FIFO"
                }
            };
            let before = fs::symlink_metadata(&log).unwrap();
            assert!(!append(&log, LINE), "{what}");
            let after = fs::symlink_metadata(&log).unwrap();
            assert_eq!(
                (after.ino(), after.mode(), after.len()),
                (before.ino(), before.mode(), before.len()),
                "{what}"
            );
            match put {
                Put::Link | Put::HardLink => assert_eq!(fs::read(&target).unwrap(), b"secret", "{what}"),
                Put::DanglingLink => assert!(!target.exists(), "{what}: the link's target was made"),
                Put::Mode(_) => assert_eq!(fs::read(&log).unwrap(), b"theirs", "{what}"),
                Put::Fifo => {}
            }
        }
    }

    /// The owner, the type and the mode the checks accept: the effective user's regular file at 0600, one link.
    #[test]
    fn only_the_users_own_private_file_passes() {
        let ours = Seen {
            regular: true,
            links: 1,
            uid: 1000,
            mode: 0o100600,
            size: 0,
            dev: 1,
            ino: 1,
        };
        assert!(ours.ours(1000));
        for (seen, what) in [
            (Seen { uid: 65534, ..ours }, "another user's"),
            (Seen { uid: 0, ..ours }, "root's"),
            (Seen { mode: 0o100640, ..ours }, "group-readable"),
            (Seen { mode: 0o104600, ..ours }, "setuid"),
            (Seen { mode: 0o100400, ..ours }, "read-only"),
            (
                Seen {
                    regular: false,
                    mode: 0o010600,
                    ..ours
                },
                "a FIFO",
            ),
            (Seen { links: 2, ..ours }, "hard-linked"),
        ] {
            assert!(!seen.ours(1000), "{what}");
        }
    }

    /// Run as root, a file another user owns at 0600 can be opened, so the owner check is what refuses it. CI runs
    /// this with sudo (cross-runtime.yml); as anyone else such a file can't be opened for writing at all.
    #[test]
    fn a_file_another_user_owns_is_left_alone() {
        // SAFETY: geteuid has no preconditions
        if unsafe { libc::geteuid() } != 0 {
            eprintln!("skipped: only root can give a file to another user and still open it");
            return;
        }
        let s = Scratch::new("owner");
        let log = s.file("aipet-hook-test.log", b"theirs", 0o600);
        std::os::unix::fs::chown(&log, Some(65534), Some(65534)).unwrap();
        assert!(!append(&log, LINE));
        assert_eq!(fs::read(&log).unwrap(), b"theirs");
        assert_eq!(fs::metadata(&log).unwrap().uid(), 65534);
    }

    /// A file renamed after it was opened is no longer the log's: `/proc/self/fd` names it where it is now.
    #[cfg(target_os = "linux")]
    #[test]
    fn an_open_file_named_otherwise_is_refused() {
        let s = Scratch::new("fd");
        let (log, _) = opened(&s.log(), false).unwrap();
        assert!(checked(&log, &s.log()).is_some());
        fs::rename(s.log(), s.0.join("elsewhere.log")).unwrap();
        assert!(checked(&log, &s.log()).is_none());
        // and one deleted since
        let (log, _) = opened(&s.log(), false).unwrap();
        fs::remove_file(s.log()).unwrap();
        assert!(checked(&log, &s.log()).is_none());
    }

    /// The name swapped for another file between the checks and the rotation: that file isn't deleted, and nothing
    /// is written.
    #[test]
    fn a_name_swapped_before_the_rotation_is_left_alone() {
        let s = Scratch::new("swap");
        let log = s.file("aipet-hook-test.log", &vec![b'x'; ROTATE_AT as usize + 1], 0o600);
        let (open, seen) = opened(&log, false).unwrap();
        let other = s.file("other", b"theirs", 0o600);
        fs::rename(&other, &log).unwrap();
        assert!(rotated(&seen, &log).is_none());
        assert_eq!(fs::read(&log).unwrap(), b"theirs");
        assert_eq!(
            open.metadata().unwrap().len(),
            ROTATE_AT + 1,
            "the checked file was written to"
        );
    }
}
