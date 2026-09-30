//! The Unix socket: 0600 before it listens, and only peers running as this user (`HookServer.cs:168-205, 353-400`).
//!
//! One socket, only ever used blocking, with 8 threads in `accept` on it. A live pet at the path refuses the start;
//! what a crashed one left there is replaced. Each connection has 2 s from when it was taken for its request and the
//! reply: the socket's timeouts end a read or write that waits past that.

use std::fs;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aipet_ipc::connect::connect_to;
use aipet_ipc::protocol::{CONNECT, TIMEOUT};
use socket2::{Domain, SockAddr, Socket, Type};

use super::{LISTENERS, Shared, lock, run};

#[derive(Default)]
pub(super) struct State {
    /// The listening socket, until `stop`. The listeners hold it too, so it closes once the last of them is gone.
    listener: Mutex<Option<Arc<Socket>>>,
}

pub(super) fn start(shared: &Arc<Shared>) -> io::Result<()> {
    let path = Path::new(&shared.options.endpoint);
    refuse_live_server(path)?;
    let socket = Arc::new(listening(bound(path)?, path)?);
    *lock(&shared.sys.listener) = Some(Arc::clone(&socket));
    for started in 0..LISTENERS {
        let (shared_, socket_) = (Arc::clone(shared), Arc::clone(&socket));
        if let Err(e) = run(move || accept(&shared_, &socket_)) {
            if started > 0 {
                shared.log(&format!("hooks: {started} of {LISTENERS} listeners: {e}"));
                break;
            }
            // hooks would wait for answers nobody gives; with no socket they find no pet
            lock(&shared.sys.listener).take();
            let _ = fs::remove_file(path);
            return Err(e);
        }
    }
    Ok(())
}

/// A connection wakes each listener in `accept` (closing the socket doesn't do that everywhere), then the socket is
/// shut down and its file removed, as .NET removes the file its socket bound. Nothing is taken down twice.
pub(super) fn stop(shared: &Shared) {
    let Some(socket) = lock(&shared.sys.listener).take() else {
        return;
    };
    let path = Path::new(&shared.options.endpoint);
    for _ in 0..LISTENERS {
        // not blocking: a full queue fails the connect at once rather than hold Stop up
        let woke = Socket::new(Domain::UNIX, Type::STREAM, None).and_then(|wake| {
            wake.set_nonblocking(true)?;
            wake.connect(&SockAddr::unix(path)?)
        });
        if woke.is_err() {
            break;
        }
    }
    // an accept still waiting returns at once (Linux)
    let _ = socket.shutdown(Shutdown::Both);
    let _ = fs::remove_file(path);
}

/// `RefuseLiveServer`: a live pet takes a connect; a crashed one left a socket file that refuses it, which `bound`
/// replaces.
fn refuse_live_server(path: &Path) -> io::Result<()> {
    match connect_to(path.as_os_str(), CONNECT) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "another pet is listening there",
        )),
        Err(_) => Ok(()),
    }
}

/// The socket, bound in place of what a crashed pet left at the path, and 0600 before it listens: its mode is what
/// the umask made it, and until it listens every connect is refused anyway.
fn bound(path: &Path) -> io::Result<Socket> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let socket = Socket::new(Domain::UNIX, Type::STREAM, None)?;
    socket.bind(&SockAddr::unix(path)?)?;
    if let Err(e) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
        let _ = fs::remove_file(path);
        return Err(e);
    }
    Ok(socket)
}

/// Listens on the bound socket, with the longest queue the system allows, as .NET's `Listen()`.
fn listening(socket: Socket, path: &Path) -> io::Result<Socket> {
    if let Err(e) = socket.listen(i32::MAX) {
        let _ = fs::remove_file(path);
        return Err(e);
    }
    Ok(socket)
}

/// One listener: waits for a hook and hands the connection to a thread of its own.
fn accept(shared: &Arc<Shared>, socket: &Socket) {
    while !shared.stopping() {
        let conn = match socket.accept() {
            Ok((conn, _)) => conn,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            // woken by Stop
            Err(_) if shared.stopping() => break,
            Err(e) => {
                // out of file descriptors, say: the hooks wait in the kernel's queue meanwhile
                shared.warn(&e.to_string());
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        if shared.stopping() || !same_user(conn.as_raw_fd()) {
            continue;
        }
        shared.serve(Conn::new(conn));
    }
}

/// `SameUser`: whether the peer runs as this user, as `PipeOptions.CurrentUserOnly` checked: the file's mode keeps
/// other users out, but not root. A peer it can't tell is refused.
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

/// A hook's connection, with 2 s from when it was taken for its request and the reply.
pub(super) struct Conn {
    stream: UnixStream,
    deadline: Instant,
}

impl Conn {
    fn new(socket: Socket) -> Conn {
        Conn {
            stream: UnixStream::from(OwnedFd::from(socket)),
            deadline: Instant::now() + TIMEOUT,
        }
    }

    /// What is left of the 2 s: past them, a read or write fails at once.
    fn left(&self) -> io::Result<Duration> {
        match self.deadline.checked_duration_since(Instant::now()) {
            Some(left) if !left.is_zero() => Ok(left),
            _ => Err(io::ErrorKind::TimedOut.into()),
        }
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.set_read_timeout(Some(self.left()?))?;
        self.stream.read(buf)
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.set_write_timeout(Some(self.left()?))?;
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A folder of its own for a test's socket, removed with it; short, as a socket's path must be.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Scratch {
            let dir = PathBuf::from(format!("/tmp/aipet-core-{}-{test}", std::process::id()));
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

    /// The socket is 0600 before it listens, and refuses every connect until it does.
    #[test]
    fn the_socket_is_the_users_before_it_listens() {
        let scratch = Scratch::new("mode");
        let path = scratch.0.join("aipet.sock");
        // what a crashed pet left there is replaced
        fs::write(&path, "").unwrap();
        let socket = bound(&path).unwrap();
        let mode = fs::symlink_metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o7777, 0o600);
        assert_eq!(
            UnixStream::connect(&path).unwrap_err().kind(),
            io::ErrorKind::ConnectionRefused
        );
        let _socket = listening(socket, &path).unwrap();
        UnixStream::connect(&path).unwrap();
    }

    /// A peer running as this user is let in; one that can't be told, refused. (A peer running as another user is
    /// tests/server.rs's, with root as the other user, where sudo allows.)
    #[test]
    fn only_a_peer_known_to_be_this_user_is_let_in() {
        let (a, b) = UnixStream::pair().unwrap();
        assert!(same_user(a.as_raw_fd()) && same_user(b.as_raw_fd()));
        // not a socket: no peer to tell
        let file = fs::File::open("/dev/null").unwrap();
        assert!(!same_user(file.as_raw_fd()));
    }
}
