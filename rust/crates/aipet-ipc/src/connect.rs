//! Connecting to the pet and asking it one thing, within the C#'s time limits: `Ipc.Connect`, `Ipc.Ask` and
//! `Ipc.ReadLine` (`src/AiPet.Core/Ipc.cs:59-154`).
//!
//! A connect that finds no pet writes nothing anywhere: the hook then leaves no trace at all.
//!
//! - Windows: the pipe is opened at once. When it exists but every instance is taken (a burst of events), the wait
//!   for one is the kernel's (`WaitNamedPipeW`). The pipe must be owned by this user, as the pet makes it.
//! - Unix: a busy pet queues connects, but `connect()` has no time limit and blocks for as long as the queue is full
//!   (a suspended pet, say). So it runs on a thread of its own, left blocked when the wait is up: the hook's exit
//!   ends it. The peer must run as this user.

use std::ffi::OsStr;
use std::io::{self, Read};
use std::time::{Duration, Instant};

use crate::endpoint::endpoint;
use crate::protocol::{BUFFER_SIZE, TIMEOUT};

/// Why [`connect`] found no pet to talk to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoPet {
    /// Nothing listens there, or what does isn't this user's pet: no pipe or socket, a socket a crashed pet left
    /// behind, a pipe someone else owns, a socket another user serves.
    Closed,
    /// The pet's pipe or socket is there but took no connection within the wait: the pet runs, so its event is lost.
    Busy,
}

/// A connection to the pet at [`endpoint()`]. When the pipe or socket is there but every instance is taken, it waits
/// at most `busy` for one (on Unix at least 50 ms: a pet that takes connections does so at once, but the connecting
/// thread has to get to run first). The hook passes what its budget has left, the doctor and the pet
/// [`CONNECT`](crate::protocol::CONNECT).
pub fn connect(busy: Duration) -> Result<Pet, NoPet> {
    connect_to(endpoint(), busy)
}

/// The same for another endpoint: a pipe name on Windows, a socket path elsewhere, as [`endpoint()`] gives them.
pub fn connect_to(endpoint: &OsStr, busy: Duration) -> Result<Pet, NoPet> {
    sys::connect(endpoint, busy).map(|inner| Pet { inner })
}

/// One connection to the pet: one request, one reply.
pub struct Pet {
    inner: sys::Stream,
}

impl Pet {
    /// `Ipc.Ask`: sends `request` as one line and returns the reply line, or `None` when the pet closed without one.
    /// Writing the line and reading the reply share [`TIMEOUT`]; past it ([`io::ErrorKind::TimedOut`]), or when the
    /// connection breaks (a request over the pet's limit, say), it fails.
    pub fn ask(&mut self, request: &str) -> io::Result<Option<String>> {
        let deadline = Instant::now() + TIMEOUT;
        let mut line = Vec::with_capacity(request.len() + 1);
        line.extend_from_slice(request.as_bytes());
        line.push(b'\n');
        self.inner.write_all(&line, deadline)?;
        self.read_line(BUFFER_SIZE, deadline)
    }

    /// `Ipc.ReadLineAsync` with a deadline: one `\n`-terminated UTF-8 line, or `None` at the end of the stream.
    /// Fails past `max` bytes, or when `deadline` passes first ([`io::ErrorKind::TimedOut`]).
    pub fn read_line(&mut self, max: usize, deadline: Instant) -> io::Result<Option<String>> {
        read_line_with(max, |buf| self.inner.read(buf, deadline))
    }
}

/// `Ipc.ReadLine`: one `\n`-terminated UTF-8 line from a blocking reader, such as the pet's end of a connection,
/// whose own timeouts end a read. `None` at the end of the stream; fails past `max` bytes.
pub fn read_line(reader: &mut impl Read, max: usize) -> io::Result<Option<String>> {
    read_line_with(max, |buf| reader.read(buf))
}

/// The line reader both use (`Ipc.Append`): what comes after the `\n` is dropped, and the line may not be longer
/// than `max` without it. Invalid UTF-8 becomes U+FFFD, as .NET decodes it.
fn read_line_with(max: usize, mut read: impl FnMut(&mut [u8]) -> io::Result<usize>) -> io::Result<Option<String>> {
    let mut buf = vec![0u8; 16 << 10];
    let mut line = Vec::new();
    loop {
        let n = match read(&mut buf) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if n == 0 {
            return Ok(None);
        }
        let newline = buf[..n].iter().position(|&b| b == b'\n');
        line.extend_from_slice(&buf[..newline.unwrap_or(n)]);
        if line.len() > max {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "request too large"));
        }
        if newline.is_some() {
            return Ok(Some(String::from_utf8_lossy(&line).into_owned()));
        }
    }
}

/// What is left until `deadline`, or the timeout error when nothing is.
fn left(deadline: Instant) -> io::Result<Duration> {
    match deadline.checked_duration_since(Instant::now()) {
        Some(left) if !left.is_zero() => Ok(left),
        _ => Err(timed_out()),
    }
}

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "the pet didn't answer in time")
}

#[cfg(unix)]
mod sys {
    use std::ffi::OsStr;
    use std::io::{self, Read, Write};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::{AsRawFd, RawFd};
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{NoPet, left, timed_out};

    pub(super) struct Stream(UnixStream);

    pub(super) fn connect(endpoint: &OsStr, busy: Duration) -> Result<Stream, NoPet> {
        let bytes = endpoint.as_bytes();
        // NamedPipeClientStream takes a name that isn't a rooted path for one in the temp folder, and refuses one with
        // a '/' in it (so every relative endpoint), or a rooted one that ends in '/'
        if !bytes.starts_with(b"/") || bytes.ends_with(b"/") {
            return Err(NoPet::Closed);
        }
        let path = PathBuf::from(endpoint);
        let (sent, got) = mpsc::sync_channel(1);
        let target = path.clone();
        let started = thread::Builder::new().name("aipet-connect".into()).spawn(move || {
            // nobody waits any more once the wait is up: then the connection is simply dropped
            let _ = sent.send(UnixStream::connect(target));
        });
        if started.is_err() {
            return Err(NoPet::Closed);
        }
        match got.recv_timeout(busy.max(Duration::from_millis(50))) {
            Ok(Ok(stream)) if same_user(stream.as_raw_fd()) => Ok(Stream(stream)),
            // missing, a socket a crashed pet left (refused), not ours
            Ok(_) | Err(RecvTimeoutError::Disconnected) => Err(NoPet::Closed),
            // the socket is there, so something listens on it (a pet that quit leaves one that refuses at once)
            Err(RecvTimeoutError::Timeout) => Err(if file_exists(&path) { NoPet::Busy } else { NoPet::Closed }),
        }
    }

    /// Whether the peer runs as this user, as `PipeOptions.CurrentUserOnly` checks: the socket file's mode keeps other
    /// users out, but not root. A peer it can't tell is refused.
    fn same_user(fd: RawFd) -> bool {
        // SAFETY: geteuid has no preconditions
        peer_uid(fd) == Some(unsafe { libc::geteuid() })
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn peer_uid(fd: RawFd) -> Option<libc::uid_t> {
        // SAFETY: an all-zero ucred is a valid value to be filled in
        let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: cred and len describe a buffer of ucred's size
        let rc = unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&raw mut cred).cast(),
                &mut len,
            )
        };
        (rc == 0 && len as usize == std::mem::size_of::<libc::ucred>()).then_some(cred.uid)
    }

    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    fn peer_uid(fd: RawFd) -> Option<libc::uid_t> {
        let (mut uid, mut gid) = (0, 0);
        // SAFETY: uid and gid are written only
        (unsafe { libc::getpeereid(fd, &mut uid, &mut gid) } == 0).then_some(uid)
    }

    /// `File.Exists`: something that isn't a folder is at the path (a link to one counts as the folder).
    fn file_exists(path: &Path) -> bool {
        match std::fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_symlink() => std::fs::metadata(path).map_or(true, |target| !target.is_dir()),
            Ok(m) => !m.is_dir(),
            Err(_) => false,
        }
    }

    impl Stream {
        pub(super) fn read(&mut self, buf: &mut [u8], deadline: Instant) -> io::Result<usize> {
            self.0.set_read_timeout(Some(left(deadline)?))?;
            self.0.read(buf).map_err(timeout_is_timed_out)
        }

        pub(super) fn write_all(&mut self, mut bytes: &[u8], deadline: Instant) -> io::Result<()> {
            while !bytes.is_empty() {
                self.0.set_write_timeout(Some(left(deadline)?))?;
                match self.0.write(bytes) {
                    Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                    Ok(n) => bytes = &bytes[n..],
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(timeout_is_timed_out(e)),
                }
            }
            Ok(())
        }
    }

    /// A socket timeout comes back as EAGAIN.
    fn timeout_is_timed_out(e: io::Error) -> io::Error {
        if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) {
            timed_out()
        } else {
            e
        }
    }
}

#[cfg(windows)]
mod sys {
    use std::ffi::{OsStr, OsString};
    use std::io;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{
        BOOL, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, ERROR_PIPE_BUSY,
        ERROR_PIPE_NOT_CONNECTED, ERROR_SEM_TIMEOUT, ERROR_SUCCESS, FALSE, GENERIC_READ, GENERIC_WRITE, GetLastError,
        HANDLE, INVALID_HANDLE_VALUE, LocalFree, TRUE, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT};
    use windows_sys::Win32::Security::{EqualSid, OWNER_SECURITY_INFORMATION};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_NONE, OPEN_EXISTING, ReadFile, SECURITY_IDENTIFICATION,
        SECURITY_SQOS_PRESENT, WriteFile,
    };
    use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, GetOverlappedResultEx, OVERLAPPED};
    use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
    use windows_sys::Win32::System::Threading::CreateEventW;

    use super::{NoPet, left, timed_out};
    use crate::endpoint::CurrentUser;
    use crate::wide;

    pub(super) struct Stream {
        pipe: OwnedHandle,
        // signals the overlapped read or write in flight
        event: OwnedHandle,
    }

    pub(super) fn connect(endpoint: &OsStr, busy: Duration) -> Result<Stream, NoPet> {
        let mut name = OsString::from(r"\\.\pipe\");
        name.push(endpoint);
        let path = wide::nul_terminated(&name);
        let deadline = Instant::now() + busy;
        let pipe = match open(&path) {
            Ok(pipe) => pipe,
            // every instance is taken. The pipe exists, so wait for one.
            Err(ERROR_PIPE_BUSY) => wait_for_instance(&path, deadline)?,
            // no pipe at all, or one this user can't open
            Err(_) => return Err(NoPet::Closed),
        };
        // the pet sets itself (the user, not the token owner) as its pipe's owner, and nobody else can
        if !owned_by_me(&pipe) {
            return Err(NoPet::Closed);
        }
        // SAFETY: an unnamed manual-reset event, as overlapped I/O wants
        let event = unsafe { CreateEventW(std::ptr::null(), TRUE, FALSE, std::ptr::null()) };
        if event == 0 {
            return Err(NoPet::Closed);
        }
        // SAFETY: a handle just made, owned from here on
        Ok(Stream {
            pipe,
            event: unsafe { OwnedHandle::from_raw_handle(event as _) },
        })
    }

    /// Overlapped, so a read or write can be given up on; and with identification only, so no server can act as the
    /// hook's user (the pet never does, but a pipe put there by someone else could).
    fn open(path: &[u16]) -> Result<OwnedHandle, u32> {
        let flags = FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION;
        // SAFETY: path is NUL-terminated; no security attributes or template
        let h = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_NONE,
                std::ptr::null(),
                OPEN_EXISTING,
                flags,
                0,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            // SAFETY: reads this thread's last error
            return Err(unsafe { GetLastError() });
        }
        // SAFETY: a handle just opened, owned from here on
        Ok(unsafe { OwnedHandle::from_raw_handle(h as _) })
    }

    /// The kernel waits for a free instance, until the deadline. Giving up on a pipe that is still there is busy;
    /// one that went away meanwhile was a pet that quit.
    fn wait_for_instance(path: &[u16], deadline: Instant) -> Result<OwnedHandle, NoPet> {
        loop {
            // at least 1 ms: 0 would mean the pet's default wait, and u32::MAX forever
            let ms = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .clamp(1, u128::from(u32::MAX - 1));
            // SAFETY: path is NUL-terminated
            let free = unsafe { WaitNamedPipeW(path.as_ptr(), ms as u32) } != 0;
            // SAFETY: reads this thread's last error, WaitNamedPipeW's
            if !free && unsafe { GetLastError() } != ERROR_SEM_TIMEOUT {
                // the pipe went away meanwhile: that pet quit
                return Err(NoPet::Closed);
            }
            if free {
                match open(path) {
                    Ok(pipe) => return Ok(pipe),
                    // another client took the free instance first: wait on
                    Err(ERROR_PIPE_BUSY) => {}
                    Err(_) => return Err(NoPet::Closed),
                }
            }
            if Instant::now() >= deadline {
                return Err(if pipe_exists(path) { NoPet::Busy } else { NoPet::Closed });
            }
        }
    }

    /// False only when no instance of the pipe exists at all. Unlike opening it, `WaitNamedPipeW` never takes one.
    fn pipe_exists(path: &[u16]) -> bool {
        // SAFETY: path is NUL-terminated; GetLastError reads this thread's last error
        unsafe { WaitNamedPipeW(path.as_ptr(), 1) != 0 || GetLastError() != ERROR_FILE_NOT_FOUND }
    }

    /// `Ipc.OwnedByMe`: the pipe's owner is the user of this process's token.
    fn owned_by_me(pipe: &OwnedHandle) -> bool {
        let Some(me) = CurrentUser::get() else { return false };
        let mut owner = std::ptr::null_mut();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: an open pipe handle; owner and descriptor are written only
        let rc = unsafe {
            GetSecurityInfo(
                pipe.as_raw_handle() as HANDLE,
                SE_KERNEL_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        // SAFETY: owner points into the descriptor, which is still alive; both SIDs are valid
        let mine = rc == ERROR_SUCCESS && !owner.is_null() && unsafe { EqualSid(owner, me.sid()) } != 0;
        if !descriptor.is_null() {
            // SAFETY: GetSecurityInfo's allocation
            unsafe { LocalFree(descriptor) };
        }
        mine
    }

    impl Stream {
        pub(super) fn read(&mut self, buf: &mut [u8], deadline: Instant) -> io::Result<usize> {
            let len = buf.len().min(u32::MAX as usize) as u32;
            let ptr = buf.as_mut_ptr();
            // SAFETY: ptr and len describe buf, which outlives the operation (overlapped waits for it to end)
            let done = self.overlapped(deadline, |h, ov| unsafe {
                ReadFile(h, ptr, len, std::ptr::null_mut(), ov)
            });
            // the pet closed its end: the end of the stream, as for .NET's PipeStream
            let closed = [ERROR_BROKEN_PIPE, ERROR_PIPE_NOT_CONNECTED].map(|e| e as i32);
            match done {
                Err(e) if e.raw_os_error().is_some_and(|code| closed.contains(&code)) => Ok(0),
                done => done,
            }
        }

        pub(super) fn write_all(&mut self, mut bytes: &[u8], deadline: Instant) -> io::Result<()> {
            while !bytes.is_empty() {
                let len = bytes.len().min(u32::MAX as usize) as u32;
                let ptr = bytes.as_ptr();
                // SAFETY: ptr and len describe bytes, which outlive the operation
                let n = self.overlapped(deadline, |h, ov| unsafe {
                    WriteFile(h, ptr, len, std::ptr::null_mut(), ov)
                })?;
                if n == 0 {
                    return Err(io::ErrorKind::WriteZero.into());
                }
                bytes = &bytes[n..];
            }
            Ok(())
        }

        /// Starts one overlapped read or write and waits for it until the deadline. Past it the operation is cancelled,
        /// and waited for still, since it uses the buffer and the OVERLAPPED on this frame.
        fn overlapped(
            &mut self,
            deadline: Instant,
            start: impl FnOnce(HANDLE, *mut OVERLAPPED) -> BOOL,
        ) -> io::Result<usize> {
            let ms = left(deadline)?.as_millis().min(u128::from(u32::MAX - 1)) as u32;
            let h = self.pipe.as_raw_handle() as HANDLE;
            // SAFETY: an all-zero OVERLAPPED is the start of an operation
            let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
            ov.hEvent = self.event.as_raw_handle() as HANDLE;
            let mut n = 0;
            if start(h, &mut ov) == 0 {
                // SAFETY: reads this thread's last error
                let err = unsafe { GetLastError() };
                if err != ERROR_IO_PENDING {
                    return Err(io::Error::from_raw_os_error(err as i32));
                }
            }
            // SAFETY: ov is the operation just started on h
            if unsafe { GetOverlappedResultEx(h, &ov, &mut n, ms, FALSE) } != 0 {
                return Ok(n as usize);
            }
            // SAFETY: reads this thread's last error
            let err = unsafe { GetLastError() };
            if err != WAIT_TIMEOUT && err != ERROR_IO_INCOMPLETE {
                return Err(io::Error::from_raw_os_error(err as i32));
            }
            // SAFETY: cancels only this operation, then waits until it has let go of the buffer and ov
            unsafe { CancelIoEx(h, &ov) };
            // SAFETY: as above
            if unsafe { GetOverlappedResult(h, &ov, &mut n, TRUE) } != 0 {
                // it was done by the time the cancel came
                return Ok(n as usize);
            }
            Err(timed_out())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader that hands out its chunks one read at a time.
    fn chunks(parts: &[&[u8]]) -> impl FnMut(&mut [u8]) -> io::Result<usize> {
        let mut parts: Vec<Vec<u8>> = parts.iter().map(|p| p.to_vec()).collect();
        parts.reverse();
        move |buf| match parts.pop() {
            Some(p) => {
                buf[..p.len()].copy_from_slice(&p);
                Ok(p.len())
            }
            None => Ok(0),
        }
    }

    #[test]
    fn a_line_ends_at_its_newline() {
        let line = |parts: &[&[u8]], max| read_line_with(max, chunks(parts));
        assert_eq!(
            line(&[b"{\"ok\":true}\n"], 64).unwrap().as_deref(),
            Some("{\"ok\":true}")
        );
        // over several reads, and what follows the newline is dropped
        assert_eq!(line(&[b"ab", b"c\nde"], 64).unwrap().as_deref(), Some("abc"));
        // the end of the stream before a newline: no line
        assert_eq!(line(&[b"abc"], 64).unwrap(), None);
        assert_eq!(line(&[], 64).unwrap(), None);
        // invalid UTF-8 as U+FFFD
        assert_eq!(line(&[b"a\xffb\n"], 64).unwrap().as_deref(), Some("a\u{fffd}b"));
        // max counts the line without its newline, and holds also when the newline has come
        assert_eq!(line(&[b"abcd\n"], 4).unwrap().as_deref(), Some("abcd"));
        assert_eq!(line(&[b"abcde\n"], 4).unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            line(&[b"abc", b"de"], 4).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn interrupted_reads_are_tried_again() {
        let mut first = true;
        let got = read_line_with(8, |buf| {
            if std::mem::take(&mut first) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            buf[..3].copy_from_slice(b"ok\n");
            Ok(3)
        });
        assert_eq!(got.unwrap().as_deref(), Some("ok"));
    }

    #[test]
    fn a_blocking_reader_gives_the_same_line() {
        let mut reader: &[u8] = b"{\"v\":1}\nrest";
        assert_eq!(read_line(&mut reader, 1024).unwrap().as_deref(), Some("{\"v\":1}"));
    }

    /// How long a connect or an ask took, which must be at least `min` and not much more than `max`.
    #[track_caller]
    fn took(started: Instant, min: Duration, max: Duration) {
        let took = started.elapsed();
        assert!(
            took >= min && took < max + Duration::from_secs(1),
            "took {took:?}, not {min:?} to {max:?}"
        );
    }

    const PING: &str = "{\"v\":1,\"type\":\"ping\"}";
    const REPLY: &str = "{\"ok\":true,\"outcome\":\"working\"}";

    #[cfg(unix)]
    mod unix {
        use super::*;
        use crate::protocol::{CONNECT, MAX_REQUEST};
        use std::io::Write;
        use std::os::unix::net::UnixListener;
        use std::path::PathBuf;

        /// A folder of its own for a test's socket, removed with it.
        struct Scratch(PathBuf);

        impl Scratch {
            fn new(test: &str) -> Scratch {
                let dir = std::env::temp_dir().join(format!("aipet-ipc-{}-{test}", std::process::id()));
                std::fs::create_dir_all(&dir).unwrap();
                Scratch(dir)
            }

            fn socket(&self) -> PathBuf {
                self.0.join("aipet.sock")
            }
        }

        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        #[test]
        fn a_missing_or_stale_socket_is_closed_at_once() {
            let scratch = Scratch::new("stale");
            let started = Instant::now();
            assert_eq!(
                connect_to(scratch.socket().as_os_str(), CONNECT).err(),
                Some(NoPet::Closed)
            );
            took(started, Duration::ZERO, Duration::ZERO);
            // what a crashed pet leaves: the socket file, refusing connects
            drop(UnixListener::bind(scratch.socket()).unwrap());
            let started = Instant::now();
            assert_eq!(
                connect_to(scratch.socket().as_os_str(), CONNECT).err(),
                Some(NoPet::Closed)
            );
            took(started, Duration::ZERO, Duration::ZERO);
        }

        #[test]
        fn a_relative_or_folder_endpoint_is_never_reached() {
            assert_eq!(
                connect_to(OsStr::new("run/aipet.sock"), CONNECT).err(),
                Some(NoPet::Closed)
            );
            assert_eq!(connect_to(OsStr::new("aipet.sock"), CONNECT).err(), Some(NoPet::Closed));
            assert_eq!(
                connect_to(std::env::temp_dir().join("").as_os_str(), CONNECT).err(),
                Some(NoPet::Closed)
            );
        }

        /// A pet that takes no connections, with a queue a single connect fills: a connect blocks, so it is waited for
        /// as long as asked (at least 50 ms), then given up on as busy.
        #[cfg(target_os = "linux")]
        #[test]
        fn a_busy_socket_is_waited_for_then_busy() {
            use std::os::unix::io::AsRawFd;
            use std::os::unix::net::UnixStream;

            let scratch = Scratch::new("busy");
            let listener = UnixListener::bind(scratch.socket()).unwrap();
            // SAFETY: a listening socket of ours; a backlog of 0 queues one connect
            assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 0) }, 0);
            let _queued = UnixStream::connect(scratch.socket()).unwrap();
            for busy in [Duration::from_millis(300), Duration::ZERO] {
                let started = Instant::now();
                assert_eq!(connect_to(scratch.socket().as_os_str(), busy).err(), Some(NoPet::Busy));
                let min = busy.max(Duration::from_millis(50));
                took(started, min, min);
            }
        }

        /// A pet that takes the connection and never answers: the ask gives up after TIMEOUT.
        #[test]
        fn a_silent_pet_times_out() {
            let scratch = Scratch::new("silent");
            let _listener = UnixListener::bind(scratch.socket()).unwrap();
            let mut pet = connect_to(scratch.socket().as_os_str(), CONNECT).unwrap();
            let started = Instant::now();
            assert_eq!(pet.ask(PING).unwrap_err().kind(), io::ErrorKind::TimedOut);
            took(started, TIMEOUT, TIMEOUT);
        }

        #[test]
        fn the_reply_line_comes_back() {
            let scratch = Scratch::new("reply");
            let listener = UnixListener::bind(scratch.socket()).unwrap();
            let pet = std::thread::spawn(move || {
                let (mut answered, _) = listener.accept().unwrap();
                let first = read_line(&mut answered, MAX_REQUEST).unwrap();
                answered.write_all(format!("{REPLY}\n").as_bytes()).unwrap();
                // closed without a reply
                let (mut closed, _) = listener.accept().unwrap();
                let second = read_line(&mut closed, MAX_REQUEST).unwrap();
                (first, second)
            });
            let mut hook = connect_to(scratch.socket().as_os_str(), CONNECT).unwrap();
            assert_eq!(hook.ask(PING).unwrap().as_deref(), Some(REPLY));
            let mut hook = connect_to(scratch.socket().as_os_str(), CONNECT).unwrap();
            assert_eq!(hook.ask("{}").unwrap(), None);
            assert_eq!(pet.join().unwrap(), (Some(PING.to_owned()), Some("{}".to_owned())));
        }
    }

    #[cfg(windows)]
    mod windows {
        use super::*;
        use crate::endpoint::CurrentUser;
        use crate::protocol::{CONNECT, MAX_REQUEST};
        use crate::wide;
        use std::ffi::OsString;
        use std::fs::File;
        use std::io::Write;
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::Foundation::{
            ERROR_PIPE_CONNECTED, FALSE, GetLastError, INVALID_HANDLE_VALUE, LocalFree,
        };
        use windows_sys::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
        use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
        use windows_sys::Win32::System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
        };

        fn name(test: &str) -> OsString {
            format!("aipet-ipc-test-{}-{test}", std::process::id()).into()
        }

        fn me() -> String {
            CurrentUser::get().unwrap().sid_string().unwrap()
        }

        /// The one instance of a pipe as the pet makes it (synchronous, 64 KiB buffers), owned by `owner` (an SDDL
        /// SID), which alone may open it. None when this user may not make `owner` the owner.
        fn serve(name: &OsStr, owner: &str) -> Option<File> {
            let sddl = wide::nul_terminated(OsStr::new(&format!("O:{owner}D:(A;;GA;;;{})", me())));
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
            assert_ne!(made, 0, "the security descriptor");
            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor,
                bInheritHandle: FALSE,
            };
            let mut path = OsString::from(r"\\.\pipe\");
            path.push(name);
            let path = wide::nul_terminated(&path);
            // SAFETY: path is NUL-terminated; attributes lives through the call
            let pipe = unsafe {
                CreateNamedPipeW(
                    path.as_ptr(),
                    PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                    1,
                    64 << 10,
                    64 << 10,
                    0,
                    &attributes,
                )
            };
            // SAFETY: ConvertStringSecurityDescriptorToSecurityDescriptorW's allocation
            unsafe { LocalFree(descriptor) };
            // SAFETY: a handle just made, owned from here on
            (pipe != INVALID_HANDLE_VALUE).then(|| File::from(unsafe { OwnedHandle::from_raw_handle(pipe as _) }))
        }

        /// Waits for the client, as the pet's listener does.
        fn accept(pipe: &File) {
            // SAFETY: a synchronous pipe instance of ours
            let connected = unsafe { ConnectNamedPipe(pipe.as_raw_handle() as _, std::ptr::null_mut()) } != 0;
            // SAFETY: reads this thread's last error
            assert!(connected || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED);
        }

        #[test]
        fn a_missing_pipe_is_closed_at_once() {
            let started = Instant::now();
            assert_eq!(connect_to(&name("missing"), CONNECT).err(), Some(NoPet::Closed));
            took(started, Duration::ZERO, Duration::ZERO);
        }

        /// Every instance taken: the kernel's wait, as long as asked, then busy.
        #[test]
        fn a_busy_pipe_is_waited_for_then_busy() {
            let name = name("busy");
            let _pipe = serve(&name, &me()).unwrap();
            let _first = connect_to(&name, CONNECT).unwrap();
            let busy = Duration::from_millis(300);
            let started = Instant::now();
            assert_eq!(connect_to(&name, busy).err(), Some(NoPet::Busy));
            took(started, busy, busy);
        }

        #[test]
        fn a_pipe_someone_else_owns_is_closed() {
            let name = name("foreign");
            // Administrators as the owner, which only an elevated process (as on CI) may set
            let Some(_pipe) = serve(&name, "BA") else {
                eprintln!("skipped: this user can't make Administrators a pipe's owner (not elevated)");
                return;
            };
            assert_eq!(connect_to(&name, CONNECT).err(), Some(NoPet::Closed));
        }

        #[test]
        fn a_silent_pet_times_out() {
            let name = name("silent");
            let _pipe = serve(&name, &me()).unwrap();
            let mut pet = connect_to(&name, CONNECT).unwrap();
            let started = Instant::now();
            assert_eq!(pet.ask(PING).unwrap_err().kind(), io::ErrorKind::TimedOut);
            took(started, TIMEOUT, TIMEOUT);
        }

        #[test]
        fn the_reply_line_comes_back() {
            for reply in [Some(REPLY), None] {
                let name = name(if reply.is_some() { "reply" } else { "no-reply" });
                let mut pipe = serve(&name, &me()).unwrap();
                let pet = std::thread::spawn(move || {
                    accept(&pipe);
                    let got = read_line(&mut pipe, MAX_REQUEST).unwrap();
                    if let Some(reply) = reply {
                        pipe.write_all(format!("{reply}\n").as_bytes()).unwrap();
                        // FlushFileBuffers: until the hook has read it
                        pipe.sync_all().unwrap();
                    }
                    got
                });
                let mut hook = connect_to(&name, CONNECT).unwrap();
                assert_eq!(hook.ask(PING).unwrap().as_deref(), reply);
                assert_eq!(pet.join().unwrap().as_deref(), Some(PING));
            }
        }
    }
}
