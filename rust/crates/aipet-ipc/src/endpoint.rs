//! Where the hooks reach the pet: `Ipc.Endpoint` (`src/AiPet.Core/Ipc.cs:52-55, 156-211`).
//!
//! - Windows: the named pipe `\\.\pipe\AiPet-<user SID>-<session id>`, or `AiPet-<user name>` when either can't be
//!   read. Pipe names are machine-wide: the user and the session keep two users' (or one user's two sessions') pets
//!   apart. The session comes from `ProcessIdToSessionId`, which doesn't list the machine's processes.
//! - Unix: the socket `/run/user/<effective uid>/aipet.sock` when that folder exists, else
//!   `$XDG_RUNTIME_DIR/aipet.sock`, else `<data dir>/aipet.sock`, and only a path under 104 bytes. The uid's folder
//!   comes first because the pet and every hook must reach the same path whatever their environments say: a hook run
//!   from a snap gets `XDG_RUNTIME_DIR=/run/user/<uid>/snap.<name>`, and one from an SSH login may get none.
//! - `AIPET_PIPE` overrides it, for tests next to a running pet: a pipe name on Windows, taken as it is, and a socket
//!   path elsewhere, made absolute.

use std::ffi::{OsStr, OsString};
use std::sync::OnceLock;

use crate::paths::{Vars, full_path, nonempty, process_vars};

/// The pipe's name (Windows) or the socket's path, read once per process as the C#'s static field is.
pub fn endpoint() -> &'static OsStr {
    static ENDPOINT: OnceLock<OsString> = OnceLock::new();
    ENDPOINT.get_or_init(|| endpoint_in(&process_vars))
}

/// The endpoint in an environment.
pub(crate) fn endpoint_in(vars: Vars) -> OsString {
    if let Some(e) = nonempty(vars, "AIPET_PIPE") {
        return if cfg!(windows) {
            e
        } else {
            full_path(&e).into_os_string()
        };
    }
    #[cfg(windows)]
    {
        windows_pipe()
    }
    #[cfg(unix)]
    {
        use crate::paths::{Folders, data_dir_in};
        unix_socket_in(vars, effective_uid().as_deref(), || {
            data_dir_in(vars, &Folders::of(vars))
        })
        .into_os_string()
    }
}

/// `Ipc.UnixSocket` for a given effective uid. A folder counts when it exists (a symlink to one too, and after
/// `Path.GetFullPath`, as `Directory.Exists` looks).
#[cfg(unix)]
pub(crate) fn unix_socket_in(
    vars: Vars,
    euid: Option<&str>,
    data_dir: impl FnOnce() -> std::path::PathBuf,
) -> std::path::PathBuf {
    use std::path::PathBuf;

    let login = euid.map(|uid| PathBuf::from(format!("/run/user/{uid}")));
    let runtime = nonempty(vars, "XDG_RUNTIME_DIR").map(PathBuf::from);
    for dir in [login, runtime].into_iter().flatten() {
        let socket = dir.join("aipet.sock");
        // the longest path a sockaddr_un takes, less a margin for macOS's shorter one
        if full_path(dir.as_os_str()).is_dir() && socket.as_os_str().len() < 104 {
            return socket;
        }
    }
    data_dir().join("aipet.sock")
}

/// `Ipc.EffectiveUid`: the effective uid, without starting anything: the second of the `Uid:` line's ids in
/// `/proc/self/status`. None without `/proc` (macOS), as for the C#; the hook's trace log is named after it too.
#[cfg(unix)]
pub fn effective_uid() -> Option<String> {
    let status = std::fs::read("/proc/self/status").ok()?;
    status_euid(&String::from_utf8_lossy(&status))
}

#[cfg(unix)]
fn status_euid(status: &str) -> Option<String> {
    let line = status.lines().find(|l| l.starts_with("Uid:"))?;
    line.split_whitespace().nth(2).map(str::to_owned)
}

/// `Ipc.WindowsPipe`.
#[cfg(windows)]
fn windows_pipe() -> OsString {
    let named = CurrentUser::get().and_then(|me| me.sid_string()).zip(session_id());
    let mut name = OsString::from("AiPet-");
    match named {
        Some((sid, session)) => name.push(format!("{sid}-{session}")),
        None => name.push(user_name()),
    }
    name
}

/// This process's session, from `ProcessIdToSessionId`: not from listing every process on the machine, which .NET's
/// `Process.SessionId` does after enabling `SeDebugPrivilege`.
#[cfg(windows)]
fn session_id() -> Option<u32> {
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;

    let mut session = 0;
    // SAFETY: session is written only
    (unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } != 0).then_some(session)
}

/// .NET's `Environment.UserName` on Windows: `GetUserNameExW(NameSamCompatible)` without its `DOMAIN\`. Empty when
/// it can't be read, as for .NET. The hook's trace log names the user with it too.
#[cfg(windows)]
pub fn user_name() -> OsString {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, GetLastError};
    use windows_sys::Win32::Security::Authentication::Identity::{GetUserNameExW, NameSamCompatible};

    let mut buf = vec![0u16; 40];
    let mut size = buf.len() as u32;
    // SAFETY: buf holds `size` units; size is updated to the length (or the length needed)
    while unsafe { GetUserNameExW(NameSamCompatible, buf.as_mut_ptr(), &mut size) } == 0 {
        // SAFETY: reads this thread's last error
        if unsafe { GetLastError() } != ERROR_MORE_DATA || size as usize <= buf.len() {
            return OsString::new();
        }
        buf.resize(size as usize, 0);
    }
    buf.truncate(size as usize);
    let name = match buf.iter().position(|&c| c == u16::from(b'\\')) {
        Some(domain) => &buf[domain + 1..],
        None => &buf[..],
    };
    OsString::from_wide(name)
}

/// The user of this process's token (`WindowsIdentity.GetCurrent().User`): the pipe is named after it, and the pet
/// makes it the pipe's owner.
#[cfg(windows)]
pub(crate) struct CurrentUser {
    // TOKEN_USER and the SID it points into; u64 for its alignment
    buf: Vec<u64>,
}

#[cfg(windows)]
impl CurrentUser {
    pub(crate) fn get() -> Option<CurrentUser> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TokenUser};
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        let mut token = 0;
        // SAFETY: the process's pseudo handle; token is written only
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return None;
        }
        // SAFETY: a handle just opened, owned from here on
        let token = unsafe { OwnedHandle::from_raw_handle(token as _) };
        let mut len = 0;
        // SAFETY: asks only for the size
        unsafe { GetTokenInformation(token.as_raw_handle() as _, TokenUser, std::ptr::null_mut(), 0, &mut len) };
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        // SAFETY: buf holds at least `len` bytes
        let ok = unsafe {
            GetTokenInformation(
                token.as_raw_handle() as _,
                TokenUser,
                buf.as_mut_ptr().cast(),
                len,
                &mut len,
            )
        };
        (ok != 0).then_some(CurrentUser { buf })
    }

    pub(crate) fn sid(&self) -> windows_sys::Win32::Foundation::PSID {
        // SAFETY: buf holds the TOKEN_USER GetTokenInformation wrote
        unsafe {
            (*self.buf.as_ptr().cast::<windows_sys::Win32::Security::TOKEN_USER>())
                .User
                .Sid
        }
    }

    /// The SID as text, `S-1-5-21-…` (`SecurityIdentifier.Value`).
    pub(crate) fn sid_string(&self) -> Option<String> {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;

        let mut text = std::ptr::null_mut();
        // SAFETY: a valid SID; text is written only
        if unsafe { ConvertSidToStringSidW(self.sid(), &mut text) } == 0 {
            return None;
        }
        // SAFETY: text is the NUL-terminated string just made
        let sid = unsafe { crate::wide::from_ptr(text) };
        // SAFETY: ConvertSidToStringSidW's allocation
        unsafe { LocalFree(text.cast()) };
        sid.into_string().ok()
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

    #[test]
    fn aipet_pipe_overrides_it() {
        let e = endpoint_in(&vars_of(&[("AIPET_PIPE", "pipes/../aipet-test.sock")]));
        if cfg!(windows) {
            assert_eq!(e, "pipes/../aipet-test.sock");
        } else {
            assert_eq!(
                e,
                std::env::current_dir()
                    .unwrap()
                    .join("aipet-test.sock")
                    .into_os_string()
            );
        }
        // empty is unset
        assert_ne!(endpoint_in(&vars_of(&[("AIPET_PIPE", "")])), "");
    }

    #[cfg(unix)]
    #[test]
    fn the_socket_is_in_the_first_folder_that_exists() {
        use std::path::{Path, PathBuf};

        let tmp = std::env::temp_dir().join(format!("aipet-ipc-endpoint-{}", std::process::id()));
        let run = tmp.join("run");
        std::fs::create_dir_all(&run).unwrap();
        let data = || PathBuf::from("/data/AiPet");
        let at = |pairs: &[(&str, &str)], uid: Option<&str>| unix_socket_in(&vars_of(pairs), uid, data);
        let runtime = run.to_str().unwrap();

        // a uid with no /run/user folder: XDG_RUNTIME_DIR, else the data folder
        let nobody = Some("4294967294");
        assert_eq!(at(&[("XDG_RUNTIME_DIR", runtime)], nobody), run.join("aipet.sock"));
        assert_eq!(at(&[], nobody), Path::new("/data/AiPet/aipet.sock"));
        assert_eq!(
            at(&[("XDG_RUNTIME_DIR", "")], None),
            Path::new("/data/AiPet/aipet.sock")
        );
        // a folder that doesn't exist, or whose socket path would be 104 bytes or more
        let missing = tmp.join("missing");
        assert_eq!(
            at(&[("XDG_RUNTIME_DIR", missing.to_str().unwrap())], nobody),
            Path::new("/data/AiPet/aipet.sock")
        );
        let long = tmp.join("l".repeat(104));
        std::fs::create_dir_all(&long).unwrap();
        assert_eq!(
            at(&[("XDG_RUNTIME_DIR", long.to_str().unwrap())], nobody),
            Path::new("/data/AiPet/aipet.sock")
        );
        // Directory.Exists takes out ".." before it looks, so a missing folder on the way doesn't matter
        let around = format!("{}/missing/../run", tmp.display());
        assert_eq!(
            at(&[("XDG_RUNTIME_DIR", &around)], nobody),
            Path::new(&around).join("aipet.sock")
        );
        // the uid's own folder comes first, when it exists
        let uid = effective_uid().unwrap();
        if Path::new(&format!("/run/user/{uid}")).is_dir() {
            let own = PathBuf::from(format!("/run/user/{uid}/aipet.sock"));
            assert_eq!(at(&[("XDG_RUNTIME_DIR", runtime)], Some(&uid)), own);
        }
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_effective_uid_is_the_status_files_second() {
        assert_eq!(
            status_euid("Name:\tx\nUid:\t1000\t1001\t1002\t1003\nGid:\t5\t5\t5\t5\n").as_deref(),
            Some("1001")
        );
        assert_eq!(status_euid("Uid: 7"), None);
        assert_eq!(status_euid(""), None);
        if cfg!(target_os = "linux") {
            // SAFETY: geteuid has no preconditions
            assert_eq!(effective_uid(), Some(unsafe { libc::geteuid() }.to_string()));
        }
    }

    #[cfg(windows)]
    #[test]
    fn the_pipe_is_named_after_the_user_and_the_session() {
        let name = windows_pipe().into_string().unwrap();
        let session = session_id().unwrap();
        let sid = CurrentUser::get().unwrap().sid_string().unwrap();
        assert!(sid.starts_with("S-1-5-"), "{sid}");
        assert_eq!(name, format!("AiPet-{sid}-{session}"));
        assert!(!user_name().is_empty());
    }
}
