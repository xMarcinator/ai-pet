//! The bearer token MCP clients send: 32 random bytes as 64 hex digits, in the data folder's `mcp-token`
//! ([`aipet_ipc::paths::mcp_token`]), readable only by the user.
//!
//! Checked on every start. A file that is missing is made; one that can't be read, isn't 64 hex digits, is a link or
//! not a file, or isn't the user's alone (Unix: owned by another user, or open to group or others; Windows: another
//! owner, or an ACL other than the user's one entry, inherited ones not counted) is replaced, with one line in the
//! log. A new file is written whole beside the old one, then renamed over it: the rename replaces a link at the path
//! and never follows it. Nothing logs the token.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// The token's file, as found.
enum Found {
    Missing,
    /// What is wrong with it.
    Wrong(String),
    /// Its text, from a file that is the user's alone.
    Text(Vec<u8>),
}

/// The token in `path`, made first when it is missing, replaced first when it is wrong.
pub(crate) fn load_or_make(path: &Path, log: &dyn Fn(&str)) -> io::Result<String> {
    let why = match sys::find(path) {
        Found::Text(text) => match valid(&text) {
            Some(token) => return Ok(token),
            None => "it isn't 64 hex digits".to_owned(),
        },
        Found::Wrong(why) => why,
        Found::Missing => {
            let token = make(path)?;
            log("mcp: made the token file");
            return Ok(token);
        }
    };
    let token = make(path)?;
    log(&format!("mcp: replaced the token file: {why}"));
    Ok(token)
}

/// 64 hex digits, a line end after them allowed (an editor's); in lower case.
fn valid(text: &[u8]) -> Option<String> {
    let text = text.strip_suffix(b"\n").unwrap_or(text);
    let text = text.strip_suffix(b"\r").unwrap_or(text);
    (text.len() == 64 && text.iter().all(u8::is_ascii_hexdigit))
        .then(|| String::from_utf8_lossy(text).to_ascii_lowercase())
}

/// A new token, written to a file of the user's alone beside `path` and then renamed over it.
fn make(path: &Path) -> io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let mut tag = [0u8; 4];
    getrandom::fill(&mut tag).map_err(io::Error::other)?;
    let temp = dir.join(format!(
        ".mcp-token.{}.{:08x}.tmp",
        std::process::id(),
        u32::from_le_bytes(tag)
    ));
    let written = sys::create_private(&temp).and_then(|mut file| {
        file.write_all(token.as_bytes())?;
        file.sync_all()
    });
    if let Err(e) = written.and_then(|()| fs::rename(&temp, path)) {
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    Ok(token)
}

/// At most this much of a file is read: a token is 64 bytes.
const READ_LIMIT: u64 = 1024;

#[cfg(unix)]
mod sys {
    use std::fs::{File, OpenOptions};
    use std::io::{self, Read};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::path::Path;

    use super::{Found, READ_LIMIT};

    pub(super) fn find(path: &Path) -> Found {
        // no link followed, and a FIFO doesn't block the open
        let opened = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path);
        let file = match opened {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Found::Missing,
            Err(e) if e.raw_os_error() == Some(libc::ELOOP) => return Found::Wrong("it is a link".into()),
            Err(e) => return Found::Wrong(format!("it can't be read: {e}")),
        };
        let meta = match file.metadata() {
            Ok(meta) => meta,
            Err(e) => return Found::Wrong(format!("it can't be read: {e}")),
        };
        if !meta.is_file() {
            return Found::Wrong("it isn't a file".into());
        }
        // SAFETY: no arguments, and it can't fail
        if meta.uid() != unsafe { libc::geteuid() } {
            return Found::Wrong("another user owns it".into());
        }
        if meta.mode() & 0o077 != 0 {
            return Found::Wrong(format!("its mode is {:o}, not 600", meta.mode() & 0o777));
        }
        let mut text = Vec::new();
        match file.take(READ_LIMIT).read_to_end(&mut text) {
            Ok(_) => Found::Text(text),
            Err(e) => Found::Wrong(format!("it can't be read: {e}")),
        }
    }

    /// A new file only the user can read and write; never one that is there, nor through a link.
    pub(super) fn create_private(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
    }
}

#[cfg(windows)]
mod sys {
    use std::ffi::OsStr;
    use std::fs::{File, OpenOptions};
    use std::io::{self, Read};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::path::Path;

    use windows_sys::Win32::Foundation::{ERROR_SUCCESS, GENERIC_WRITE, INVALID_HANDLE_VALUE, LocalFree, PSID};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
        SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, DACL_SECURITY_INFORMATION, EqualSid, GetAce,
        GetAclInformation, GetSecurityDescriptorControl, GetTokenInformation, OWNER_SECURITY_INFORMATION,
        SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    use super::{Found, READ_LIMIT};

    /// winnt.h's ACCESS_ALLOWED_ACE_TYPE.
    const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;

    pub(super) fn find(path: &Path) -> Found {
        // a link is opened itself, not followed
        let opened = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path);
        let file = match opened {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Found::Missing,
            Err(e) => return Found::Wrong(format!("it can't be read: {e}")),
        };
        let meta = match file.metadata() {
            Ok(meta) => meta,
            Err(e) => return Found::Wrong(format!("it can't be read: {e}")),
        };
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Found::Wrong("it is a link".into());
        }
        if !meta.is_file() {
            return Found::Wrong("it isn't a file".into());
        }
        match users_alone(&file) {
            Ok(true) => {}
            Ok(false) => return Found::Wrong("its owner or ACL isn't the user's alone".into()),
            Err(e) => return Found::Wrong(format!("its ACL can't be read: {e}")),
        }
        let mut text = Vec::new();
        match file.take(READ_LIMIT).read_to_end(&mut text) {
            Ok(_) => Found::Text(text),
            Err(e) => Found::Wrong(format!("it can't be read: {e}")),
        }
    }

    /// Whether the file is owned by the user, and its ACL is protected (nothing inherited) and has one entry, which
    /// allows the user.
    fn users_alone(file: &File) -> io::Result<bool> {
        let me = User::current()?;
        let (mut owner, mut dacl): (PSID, *mut ACL) = (std::ptr::null_mut(), std::ptr::null_mut());
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: an open handle; the pointers are written only, and point into `descriptor`, freed below
        let got = unsafe {
            GetSecurityInfo(
                file.as_raw_handle() as _,
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                std::ptr::null_mut(),
                &mut dacl,
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        if got != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(got as i32));
        }
        // SAFETY: owner and dacl point into the descriptor GetSecurityInfo made, which lives until the LocalFree
        let alone = unsafe {
            let mut control = 0;
            let mut revision = 0;
            let protected = GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) != 0
                && control & SE_DACL_PROTECTED != 0;
            let mut size: ACL_SIZE_INFORMATION = std::mem::zeroed();
            let one_entry = !dacl.is_null()
                && GetAclInformation(
                    dacl,
                    (&raw mut size).cast(),
                    size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                ) != 0
                && size.AceCount == 1;
            let mut ace = std::ptr::null_mut();
            let allows_me = one_entry && GetAce(dacl, 0, &mut ace) != 0 && {
                let ace = ace.cast::<ACCESS_ALLOWED_ACE>();
                (*ace).Header.AceType == ACCESS_ALLOWED_ACE_TYPE
                    && EqualSid((&raw mut (*ace).SidStart).cast(), me.sid()) != 0
            };
            let mine = !owner.is_null() && EqualSid(owner, me.sid()) != 0;
            LocalFree(descriptor as _);
            mine && protected && allows_me
        };
        Ok(alone)
    }

    /// A new file only the user can open: owned by them, with a protected ACL of one entry giving them full access.
    /// Never one that is there.
    pub(super) fn create_private(path: &Path) -> io::Result<File> {
        let me = User::current()?.sid_text()?;
        let sddl: Vec<u16> = OsStr::new(&format!("O:{me}D:P(A;;FA;;;{me})"))
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: sddl is NUL-terminated; descriptor is written only
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: wide is NUL-terminated and security points at a live descriptor
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_WRITE,
                0,
                &security,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                0,
            )
        };
        let created = io::Error::last_os_error();
        // SAFETY: ConvertStringSecurityDescriptorToSecurityDescriptorW's allocation, freed once
        unsafe { LocalFree(descriptor as _) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(created);
        }
        // SAFETY: a handle just made, owned from here on
        Ok(File::from(unsafe { OwnedHandle::from_raw_handle(handle as _) }))
    }

    /// This process's user: its TOKEN_USER, and the SID in it.
    struct User(Vec<u64>);

    impl User {
        fn current() -> io::Result<User> {
            let mut token = 0;
            // SAFETY: the process's pseudo handle; token is written only
            if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: a handle just opened, owned from here on
            let token = unsafe { OwnedHandle::from_raw_handle(token as _) };
            let mut len = 0;
            // SAFETY: asks only for the size
            unsafe { GetTokenInformation(token.as_raw_handle() as _, TokenUser, std::ptr::null_mut(), 0, &mut len) };
            // TOKEN_USER and the SID it points into; u64 for its alignment
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            // SAFETY: buf holds at least `len` bytes
            if unsafe {
                GetTokenInformation(
                    token.as_raw_handle() as _,
                    TokenUser,
                    buf.as_mut_ptr().cast(),
                    len,
                    &mut len,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(User(buf))
        }

        fn sid(&self) -> PSID {
            // SAFETY: the buffer holds the TOKEN_USER GetTokenInformation wrote
            unsafe { (*self.0.as_ptr().cast::<TOKEN_USER>()).User.Sid }
        }

        fn sid_text(&self) -> io::Result<String> {
            let mut text = std::ptr::null_mut();
            // SAFETY: a valid SID; text is written only
            if unsafe { ConvertSidToStringSidW(self.sid(), &mut text) } == 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: text is the NUL-terminated string just made
            let len = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
            // SAFETY: the `len` units before the NUL were just read
            let sid = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
            // SAFETY: ConvertSidToStringSidW's allocation
            unsafe { LocalFree(text.cast()) };
            Ok(sid)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn dir(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("aipet-mcp-token-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The token, and what the log got.
    fn load(path: &Path) -> (String, Vec<String>) {
        let log = Mutex::new(Vec::new());
        let token = load_or_make(path, &|line| log.lock().unwrap().push(line.to_owned())).unwrap();
        (token, log.into_inner().unwrap())
    }

    fn is_token(token: &str) -> bool {
        token.len() == 64 && token.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }

    #[test]
    fn a_missing_token_is_made_once_and_then_kept() {
        let dir = dir("made");
        let path = dir.join("sub").join("mcp-token");
        let (made, log) = load(&path);
        assert!(is_token(&made), "{made}");
        assert_eq!(log, ["mcp: made the token file"]);
        assert_eq!(fs::read_to_string(&path).unwrap(), made);
        let (kept, log) = load(&path);
        assert_eq!(kept, made);
        assert!(log.is_empty(), "{log:?}");
        // nothing left beside it
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_token_file_is_replaced_and_logged() {
        let dir = dir("corrupt");
        let path = dir.join("mcp-token");
        let (good, _) = load(&path);
        for corrupt in [
            "",
            "not a token",
            &good[..63],
            &format!("{good}0"),
            &format!("{}zz", &good[..62]),
        ] {
            fs::write(&path, corrupt).unwrap();
            let (token, log) = load(&path);
            assert!(is_token(&token) && token != corrupt, "{corrupt:?}");
            assert_eq!(
                log,
                ["mcp: replaced the token file: it isn't 64 hex digits"],
                "{corrupt:?}"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), token);
        }
        // an editor's line end, and upper case, are still the token
        fs::write(&path, format!("{}\n", good.to_ascii_uppercase())).unwrap();
        let (token, log) = load(&path);
        assert_eq!((token, log), (good, vec![]));
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_token_file_others_can_read_is_replaced_and_logged() {
        use std::os::unix::fs::PermissionsExt;

        let dir = dir("mode");
        let path = dir.join("mcp-token");
        let (old, _) = load(&path);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let (token, log) = load(&path);
        assert_ne!(token, old, "a token others could read is never used again");
        assert_eq!(log, ["mcp: replaced the token file: its mode is 644, not 600"]);
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_at_the_token_file_is_replaced_not_followed() {
        let dir = dir("link");
        let path = dir.join("mcp-token");
        let elsewhere = dir.join("elsewhere");
        fs::write(&elsewhere, "a file the link points at").unwrap();
        std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
        let (token, log) = load(&path);
        assert!(is_token(&token));
        assert_eq!(log, ["mcp: replaced the token file: it is a link"]);
        assert!(!fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&elsewhere).unwrap(), "a file the link points at");
        let _ = fs::remove_dir_all(&dir);
    }
}
