//! The named pipe: owned by the user, network clients denied, first instance (`HookServer.cs:118-166, 330-351`).
//!
//! 8 synchronous instances wait for hooks, each with a thread in `ConnectNamedPipe`. A listener makes the next instance
//! before it hands its connection off, so a hook never finds no instance at all and takes the pet for not running.
//! A synchronous read or write can't be given up on from its own thread, so a thread of the server's own cancels
//! (`CancelIoEx`) one that still waits 2 s after the connection came, and again every 2 s: a read begun after a cancel
//! is cut too. That thread runs while connections do.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::time::Instant;

use aipet_ipc::protocol::{BUFFER_SIZE, TIMEOUT};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_PIPE_CONNECTED, FALSE, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
    LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::IO::CancelIoEx;
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::{LISTENERS, Shared, lock, run};

#[derive(Default)]
pub(super) struct State {
    /// Made once, for every instance.
    security: OnceLock<Security>,
    /// Set once the first instance exists, until `stop` wakes the listeners.
    listening: AtomicBool,
    cuts: Cuts,
}

pub(super) fn start(shared: &Arc<Shared>) -> io::Result<()> {
    start_with(shared, instance, run)
}

/// [`start`], given how instances are made and listeners run: a test's fail some.
fn start_with(
    shared: &Arc<Shared>,
    mut make: impl FnMut(&OsStr, &Security, bool) -> io::Result<File>,
    mut spawn: impl FnMut(Box<dyn FnOnce() + Send>) -> io::Result<()>,
) -> io::Result<()> {
    let made = Security::only_me()?;
    let security = shared.sys.security.get_or_init(|| made);
    let name = &shared.options.endpoint;
    // ERROR_ACCESS_DENIED when the name is taken
    let mut first = Some(make(name, security, true)?);
    shared.sys.listening.store(true, Ordering::SeqCst);
    let started = shared.start_listeners(|| {
        let pipe = match first.take() {
            Some(pipe) => pipe,
            None => make(name, security, false)?,
        };
        let shared = Arc::clone(shared);
        // a listener that can't run closes its instance
        spawn(Box::new(move || listen(&shared, pipe)))
    });
    if started.is_err() {
        // not even the first listener runs, and its instance, the name's first, is closed: hooks find no pet
        shared.sys.listening.store(false, Ordering::SeqCst);
    }
    started
}

/// A listener's wait for a hook can't be cancelled, so each is woken by a connection of its own, sees the pet stopping
/// and closes its instance instead of making the next. The connections being answered are still cut off.
pub(super) fn stop(shared: &Shared) {
    if !shared.sys.listening.swap(false, Ordering::SeqCst) {
        return;
    }
    let path = pipe_path(&shared.options.endpoint);
    for _ in 0..2 * LISTENERS {
        // opened at once or not at all, as Connect(0)
        // SAFETY: path is NUL-terminated; no security attributes or template
        let wake = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                0,
            )
        };
        if wake == INVALID_HANDLE_VALUE {
            // no instance left waiting
            break;
        }
        // SAFETY: the handle just opened
        unsafe { CloseHandle(wake) };
    }
}

/// One listener: waits for a hook, makes the next instance, hands the connection to a thread of its own, and waits
/// on the new instance.
fn listen(shared: &Arc<Shared>, pipe: File) {
    let Some(security) = shared.sys.security.get() else {
        return;
    };
    let name = &shared.options.endpoint;
    let mut waiting = Some(pipe);
    // stopping is read once the instance exists, so Stop's wake-up either finds the instance or comes before this
    while let Some(pipe) = waiting.take() {
        if shared.stopping() {
            break;
        }
        // SAFETY: a synchronous pipe instance of ours
        let connected = unsafe { ConnectNamedPipe(pipe.as_raw_handle() as HANDLE, std::ptr::null_mut()) } != 0
            // a client that came between the instance's making and the wait: connected too
            // SAFETY: reads this thread's last error
            || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if shared.stopping() {
            break;
        }
        if !connected {
            // a client that left at once. The new instance comes first, so the name never goes away.
            waiting = next(shared, name, security);
            continue;
        }
        // likewise the replacement is made before this one is handed off: a hook that found no instance at all would
        // take the pet for not running
        waiting = instance(name, security, false).ok();
        shared.serve(Conn::new(shared, pipe));
        if waiting.is_none() {
            waiting = next(shared, name, security);
        }
    }
}

/// A new listening instance. When none can be made just now (every allowed instance in use, say), it tries again
/// shortly rather than lose a listener; none once the pet is stopping.
fn next(shared: &Shared, name: &OsStr, security: &Security) -> Option<File> {
    let mut tried = false;
    while !shared.stopping() {
        match instance(name, security, false) {
            Ok(pipe) => return Some(pipe),
            Err(e) if !tried => shared.log(&format!("hooks: {e}")),
            Err(_) => {}
        }
        tried = true;
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    None
}

/// A listening instance: synchronous, for threads that block on it. The first is made as the first instance of its
/// name, which fails when the name is taken.
fn instance(name: &OsStr, security: &Security, first: bool) -> io::Result<File> {
    let path = pipe_path(name);
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security.0,
        bInheritHandle: FALSE,
    };
    let open = PIPE_ACCESS_DUPLEX | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
    // network clients are refused by the pipe too (.NET can't ask for that, hence the ACL's denial in the C#)
    let mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
    let buffer = BUFFER_SIZE as u32;
    // SAFETY: path is NUL-terminated; attributes and the descriptor it points to live through the call
    let pipe = unsafe {
        CreateNamedPipeW(
            path.as_ptr(),
            open,
            mode,
            PIPE_UNLIMITED_INSTANCES,
            buffer,
            buffer,
            0,
            &attributes,
        )
    };
    if pipe == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a handle just made, owned from here on
    Ok(File::from(unsafe { OwnedHandle::from_raw_handle(pipe as _) }))
}

/// `\\.\pipe\<name>`, NUL-terminated.
fn pipe_path(name: &OsStr) -> Vec<u16> {
    let mut path = OsString::from(r"\\.\pipe\");
    path.push(name);
    path.encode_wide().chain(Some(0)).collect()
}

/// `OnlyMe`: the pipe is owned by the user, who alone may open it, and network clients are denied. Not
/// `CurrentUserOnly`, which grants the token's owner: that's Administrators in an elevated pet, whose pipe a normal
/// hook then couldn't open (dotnet/runtime#123903). The owner is what the hook checks.
struct Security(PSECURITY_DESCRIPTOR);

// SAFETY: the descriptor is only read once made, and freed once, when dropped
unsafe impl Send for Security {}
// SAFETY: as above
unsafe impl Sync for Security {}

impl Security {
    fn only_me() -> io::Result<Security> {
        let me = user_sid()?;
        // PipeAccessRights.FullControl, the deny first: what .NET's PipeSecurity gives for the C#'s rules
        let sddl = format!("O:{me}D:(D;;0x1f019f;;;NU)(A;;0x1f019f;;;{me})");
        let sddl: Vec<u16> = OsStr::new(&sddl).encode_wide().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: sddl is NUL-terminated; descriptor is written only
        let made = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if made == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Security(descriptor))
    }
}

impl Drop for Security {
    fn drop(&mut self) {
        // SAFETY: ConvertStringSecurityDescriptorToSecurityDescriptorW's allocation, freed once
        unsafe { LocalFree(self.0 as _) };
    }
}

/// The SID of this process's user, as text (`WindowsIdentity.GetCurrent().User`).
fn user_sid() -> io::Result<String> {
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
    let got = unsafe {
        GetTokenInformation(
            token.as_raw_handle() as _,
            TokenUser,
            buf.as_mut_ptr().cast(),
            len,
            &mut len,
        )
    };
    if got == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: buf holds the TOKEN_USER GetTokenInformation wrote
    let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut text = std::ptr::null_mut();
    // SAFETY: a valid SID; text is written only
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
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

/// A hook's connection, cut off while its reads and writes wait too long.
pub(super) struct Conn {
    // taken off the cut-off's list before the pipe closes (fields drop in this order)
    _cut: CutOff,
    pipe: File,
}

impl Conn {
    fn new(shared: &Arc<Shared>, pipe: File) -> Conn {
        Conn {
            _cut: CutOff::new(shared, &pipe),
            pipe,
        }
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.pipe.read(buf)
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pipe.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The connections being answered, each with when it is cut off next, and whether a thread is there to cut them.
#[derive(Default)]
struct Cuts {
    list: Mutex<CutList>,
    changed: Condvar,
}

#[derive(Default)]
struct CutList {
    next_id: u64,
    due: Vec<(u64, HANDLE, Instant)>,
    cutting: bool,
}

/// A connection on the cut-off's list.
struct CutOff {
    shared: Arc<Shared>,
    id: u64,
}

impl CutOff {
    fn new(shared: &Arc<Shared>, pipe: &File) -> CutOff {
        let cuts = &shared.sys.cuts;
        let mut list = lock(&cuts.list);
        let id = list.next_id;
        list.next_id += 1;
        list.due
            .push((id, pipe.as_raw_handle() as HANDLE, Instant::now() + TIMEOUT));
        if !list.cutting {
            let cutter = Arc::clone(shared);
            // with no thread for it the connection isn't cut off, as with no thread pool for the C#'s timer
            list.cutting = run(move || cut_off(&cutter)).is_ok();
        }
        cuts.changed.notify_all();
        CutOff {
            shared: Arc::clone(shared),
            id,
        }
    }
}

impl Drop for CutOff {
    fn drop(&mut self) {
        let cuts = &self.shared.sys.cuts;
        lock(&cuts.list).due.retain(|&(id, _, _)| id != self.id);
        cuts.changed.notify_all();
    }
}

/// The cut-off's thread: cancels what each connection still waits for when it's due, until no connection is left.
fn cut_off(shared: &Shared) {
    let cuts = &shared.sys.cuts;
    let mut list = lock(&cuts.list);
    loop {
        let now = Instant::now();
        for (_, pipe, due) in list.due.iter_mut().filter(|(_, _, due)| *due <= now) {
            // SAFETY: an open instance: its connection closes it only once it's off the list, which takes this lock
            unsafe { CancelIoEx(*pipe, std::ptr::null()) };
            *due = now + TIMEOUT;
        }
        let Some(next) = list.due.iter().map(|&(_, _, due)| due).min() else {
            list.cutting = false;
            return;
        };
        list = cuts
            .changed
            .wait_timeout(list, next.saturating_duration_since(now))
            .unwrap_or_else(PoisonError::into_inner)
            .0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::{HookServer, Options};
    use crate::sessions::AgentSessions;
    use aipet_ipc::connect::connect_to;
    use aipet_ipc::protocol::CONNECT;
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SE_KERNEL_OBJECT,
    };
    use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION};

    /// The pipe's owner and ACL as .NET's `PipeSecurity` makes them for the C# (`GetSecurityDescriptorSddlForm`, taken
    /// from the C# once): the user, allowed; network logons, denied.
    #[test]
    fn the_pipe_is_the_users_alone() {
        let name = OsString::from(format!("aipet-core-server-test-{}-acl", std::process::id()));
        let pipe = instance(&name, &Security::only_me().unwrap(), true).unwrap();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: an open pipe; descriptor is written only
        let rc = unsafe {
            GetSecurityInfo(
                pipe.as_raw_handle() as HANDLE,
                SE_KERNEL_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        assert_eq!(rc, 0);
        let sddl = sddl_of(descriptor);
        // SAFETY: GetSecurityInfo's allocation, freed once
        unsafe { LocalFree(descriptor) };
        let me = user_sid().unwrap();
        assert_eq!(
            sddl,
            as_windows_writes(&format!("O:{me}D:(D;;0x1f019f;;;NU)(A;;0x1f019f;;;{me})")),
            "the C#'s O:<me>D:(D;;0x1f019f;;;NU)(A;;0x1f019f;;;<me>), with <me> = {me}"
        );
        // and the name is taken now: a first instance fails, a further one doesn't
        let security = Security::only_me().unwrap();
        let taken = instance(&name, &security, true).unwrap_err();
        assert_eq!(taken.raw_os_error(), Some(5), "{taken}");
        instance(&name, &security, false).unwrap();
    }

    /// `descriptor`'s owner and DACL as SDDL text.
    fn sddl_of(descriptor: PSECURITY_DESCRIPTOR) -> String {
        let mut text = std::ptr::null_mut();
        // SAFETY: a valid descriptor; text is written only
        let ok = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut text,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(ok, 0);
        // SAFETY: text is NUL-terminated
        let len = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
        // SAFETY: the `len` units before the NUL
        let sddl = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
        // SAFETY: the allocation above, freed once
        unsafe { LocalFree(text.cast()) };
        sddl
    }

    /// `sddl` as Windows writes it back. Windows writes a well-known account by its short name (the built-in
    /// Administrator, which GitHub's runners use, is `LA`), so expected text goes through the same conversion as the
    /// pipe's.
    fn as_windows_writes(sddl: &str) -> String {
        let wide: Vec<u16> = OsStr::new(sddl).encode_wide().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: wide is NUL-terminated; descriptor is written only
        let made = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(made, 0, "{sddl}");
        let back = sddl_of(descriptor);
        // SAFETY: the allocation above, freed once
        unsafe { LocalFree(descriptor) };
        back
    }

    /// A server at a pipe of its own, not started, and the lines it logs. Only pings come to it: it writes nothing.
    fn unstarted(test: &str) -> (HookServer, OsString, Arc<Mutex<Vec<String>>>) {
        let name = OsString::from(format!("aipet-core-server-test-{}-{test}", std::process::id()));
        let dir = std::env::temp_dir().join(&name);
        let log = Arc::new(Mutex::new(Vec::new()));
        let lines = Arc::clone(&log);
        let server = HookServer::with_options(
            Arc::new(AgentSessions::with_codex_home(dir.join("codex"))),
            || {},
            Options {
                endpoint: name.clone(),
                events_log: dir.join("hook-events.log"),
                log: Box::new(move |line| lock(&lines).push(line.to_owned())),
            },
        );
        (server, name, log)
    }

    /// Whether a pet at `name` answers a ping.
    fn pings(name: &OsStr) -> bool {
        let Ok(mut pet) = connect_to(name, CONNECT) else {
            return false;
        };
        matches!(pet.ask(r#"{"v":1,"type":"ping"}"#), Ok(Some(reply)) if reply.starts_with(r#"{"ok":true,"#))
    }

    /// A start that gets fewer than the 8 listeners, as no more instances or threads are to be had, serves with those
    /// it got, and the log says how many.
    #[test]
    fn a_start_serves_with_the_listeners_it_got() {
        // the third instance can't be made: two listeners
        let (server, name, log) = unstarted("fewer-instances");
        let mut made = 0;
        let started = start_with(
            &server.shared,
            |name, security, first| {
                made += 1;
                if made == 3 {
                    return Err(io::Error::other("no instance"));
                }
                instance(name, security, first)
            },
            run,
        );
        started.unwrap();
        assert_eq!(*lock(&log), ["hooks: 2 of 8 listeners: no instance"]);
        assert!(pings(&name) && pings(&name));

        // the second listener's thread can't be had: one listener
        let (server, name, log) = unstarted("fewer-threads");
        let mut spawned = 0;
        let started = start_with(&server.shared, instance, |listener| {
            spawned += 1;
            if spawned == 2 {
                return Err(io::Error::other("no thread"));
            }
            run(listener)
        });
        started.unwrap();
        assert_eq!(*lock(&log), ["hooks: 1 of 8 listeners: no thread"]);
        assert!(pings(&name) && pings(&name));
    }

    /// A start whose first listener can't run fails and leaves nothing listening: its instance, the name's first, is
    /// closed, so the name is free for another pet.
    #[test]
    fn a_start_without_a_listener_leaves_the_name_free() {
        let (server, name, log) = unstarted("no-listener");
        let started = start_with(&server.shared, instance, |_| Err(io::Error::other("no thread")));
        assert_eq!(started.unwrap_err().to_string(), "no thread");
        assert!(lock(&log).is_empty());
        assert!(!server.shared.sys.listening.load(Ordering::SeqCst));
        assert!(!pings(&name));
        instance(&name, &Security::only_me().unwrap(), true).unwrap();
    }
}
