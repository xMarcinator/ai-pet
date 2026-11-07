//! One pet per user, the .NET pet or the Rust one: the spec's single-instance contract (R7), taken before anything
//! else (`src/AiPet.UI/Program.cs:15-37`).
//!
//! - Windows: the named mutex `Local\AiPetApp`, made as .NET's `new Mutex(true, name, out created)` makes it. Another
//!   pet holds it when it was there already, or when this pet may not open it (a pet started elevated made it).
//! - Unix: an exclusive, non-blocking `flock` on the lock file next to the hooks' socket: the socket's path with
//!   `.lock` in place of `.sock`, so it is where `aipet-ipc`'s endpoint rule puts the socket whatever the launch
//!   environment says (`/run/user/<uid>` first), or `<AIPET_PIPE>.lock` under the override. It is opened 0600 without
//!   following a link, as the .NET pet opens it. Then, as the C#'s `OtherSessionsPet` does, a pet answering on the
//!   socket, or there but too busy to answer, is another pet too.
//!
//! Losing means another pet runs, and this one quits quietly. Whatever way a pet ends, the kernel lets go of its mutex
//! or lock. A lock that can't be taken for another reason (a link where the file goes, a folder that can't be written)
//! is an error: the pet doesn't start rather than risk two.

use std::io;

/// Proof that this is the only pet: held for the whole run, let go when dropped (or by the kernel at exit).
#[derive(Debug)]
pub struct Instance {
    #[cfg(windows)]
    _mutex: std::os::windows::io::OwnedHandle,
    #[cfg(unix)]
    _lock: std::fs::File,
}

/// What the single instance is taken under: the pet's own ([`Names::app`]), or a test's.
#[derive(Clone, Debug)]
pub struct Names {
    #[cfg(windows)]
    pub mutex: String,
    /// The lock file, and the socket another pet answers on.
    #[cfg(unix)]
    pub lock: std::path::PathBuf,
    #[cfg(unix)]
    pub endpoint: std::ffi::OsString,
}

/// The .NET pet's mutex (Program.cs).
#[cfg(windows)]
pub const MUTEX: &str = r"Local\AiPetApp";

impl Names {
    /// The pet's own: [`MUTEX`] on Windows; elsewhere the hooks' socket and the lock file beside it.
    pub fn app() -> Names {
        #[cfg(windows)]
        return Names { mutex: MUTEX.into() };
        #[cfg(unix)]
        {
            let endpoint = aipet_ipc::endpoint::endpoint().to_owned();
            let overridden = std::env::var_os("AIPET_PIPE").is_some_and(|pipe| !pipe.is_empty());
            Names {
                lock: lock_path(&endpoint, overridden),
                endpoint,
            }
        }
    }
}

/// Takes the single instance: the proof while this is the only pet, none when another pet runs.
pub fn take(names: &Names) -> io::Result<Option<Instance>> {
    #[cfg(windows)]
    return Ok(windows::take(&names.mutex)?.map(|mutex| Instance { _mutex: mutex }));
    #[cfg(unix)]
    {
        let Some(lock) = unix::take(&names.lock)? else {
            return Ok(None);
        };
        // a pet of another login session, or one that took the socket without the lock (a .NET pet from before it
        // had one); a crashed pet's socket answers nothing
        use aipet_ipc::connect::{NoPet, connect_to};
        use aipet_ipc::protocol::CONNECT;
        if !matches!(connect_to(&names.endpoint, CONNECT), Err(NoPet::Closed)) {
            return Ok(None);
        }
        Ok(Some(Instance { _lock: lock }))
    }
}

/// The lock file for the socket at `endpoint`: its path with `.lock` in place of `.sock`, or `<endpoint>.lock` when
/// `AIPET_PIPE` set it (`overridden`), which may name any path.
#[cfg(unix)]
pub fn lock_path(endpoint: &std::ffi::OsStr, overridden: bool) -> std::path::PathBuf {
    let path = std::path::Path::new(endpoint);
    if overridden {
        let mut lock = endpoint.to_owned();
        lock.push(".lock");
        lock.into()
    } else {
        path.with_extension("lock")
    }
}

#[cfg(windows)]
mod windows {
    use std::io;
    use std::os::windows::io::{FromRawHandle, OwnedHandle};

    use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    /// The mutex `name`, made and owned by this thread; none when it was there already or may not be opened.
    pub(super) fn take(name: &str) -> io::Result<Option<OwnedHandle>> {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // SAFETY: a NUL-terminated name and no security attributes (the token's default, as .NET's)
        let handle = unsafe { CreateMutexW(std::ptr::null(), 1, wide.as_ptr()) };
        // SAFETY: reads this thread's last error, which CreateMutexW just set
        let error = unsafe { GetLastError() };
        if handle == 0 {
            return match error {
                // made by a pet started elevated: only Administrators may open it
                ERROR_ACCESS_DENIED => Ok(None),
                _ => Err(io::Error::from_raw_os_error(error as i32)),
            };
        }
        // SAFETY: a handle CreateMutexW just opened, owned from here on
        let handle = unsafe { OwnedHandle::from_raw_handle(handle as _) };
        Ok((error != ERROR_ALREADY_EXISTS).then_some(handle))
    }
}

#[cfg(unix)]
mod unix {
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Path;

    /// The lock at `path`, made 0600 if it isn't there: an exclusive `flock` on it, held while the file is open;
    /// none when another process holds it. Its folder is made if it is missing (the data folder, on a first start).
    pub(super) fn take(path: &Path) -> io::Result<Option<File>> {
        if let Some(folder) = path.parent().filter(|folder| !folder.as_os_str().is_empty()) {
            std::fs::create_dir_all(folder)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        // SAFETY: the descriptor of a file this function owns
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(Some(file));
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
            Ok(None)
        } else {
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};

    use super::*;

    /// A name no other test, and no real pet, uses.
    fn unique(what: &str) -> String {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let n = COUNT.fetch_add(1, Ordering::SeqCst);
        format!("aipet-single-{what}-{}-{n}", std::process::id())
    }

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..")
    }

    /// Runs this test binary's test `name` as a pet of its own, with `env` added and `AIPET_PIPE` and
    /// `XDG_RUNTIME_DIR` taken out unless `env` sets them; what it printed. It ends within 30 s.
    fn child(name: &str, env: &[(&str, &str)]) -> String {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env("AIPET_SINGLE_CHILD", "1")
            .env_remove("AIPET_PIPE")
            .env_remove("XDG_RUNTIME_DIR")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in env {
            command.env(key, value);
        }
        let mut process = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while process.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                let _ = process.kill();
                panic!("{name} didn't end within 30 s");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let Output { status, stdout, stderr } = process.wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&stdout).into_owned();
        assert!(
            status.success(),
            "{name}: {status}\n{stdout}\n{}",
            String::from_utf8_lossy(&stderr)
        );
        stdout
    }

    /// What a child printed after `prefix`, to the end of that line (the test harness may have started the line).
    fn said<'a>(output: &'a str, prefix: &str) -> &'a str {
        output
            .lines()
            .find_map(|line| line.split_once(prefix).map(|(_, said)| said))
            .unwrap_or_else(|| panic!("no {prefix:?} in {output}"))
    }

    /// The child's half of the cross-process tests: takes the instance named in its environment and says whether it
    /// got it. It does nothing as a test of its own.
    #[test]
    fn contender() {
        if std::env::var_os("AIPET_SINGLE_CHILD").is_none() {
            return;
        }
        #[cfg(windows)]
        let names = Names {
            mutex: std::env::var("AIPET_SINGLE_MUTEX").unwrap(),
        };
        #[cfg(unix)]
        let names = Names::app();
        #[cfg(unix)]
        println!("lock: {}", names.lock.display());
        if std::env::var_os("AIPET_SINGLE_TAKE").is_some() {
            let taken = take(&names).unwrap();
            println!("taken: {}", taken.is_some());
        }
    }

    #[test]
    fn the_mutex_is_the_csharps() {
        let program = std::fs::read_to_string(repo().join("src").join("AiPet.UI").join("Program.cs"))
            .unwrap_or_else(|e| panic!("Program.cs: {e}. Once the C# is gone, MUTEX is the contract as it stands"));
        assert!(
            program.contains(r#"new Mutex(true, @"Local\AiPetApp", out created)"#),
            "{program}"
        );
        #[cfg(windows)]
        assert_eq!(MUTEX, r"Local\AiPetApp");
    }

    #[cfg(windows)]
    mod windows {
        use std::os::windows::io::{FromRawHandle, OwnedHandle};

        use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
        use windows_sys::Win32::Security::{
            ACL, InitializeAcl, InitializeSecurityDescriptor, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR,
            SetSecurityDescriptorDacl,
        };
        use windows_sys::Win32::System::Threading::{
            CREATE_MUTEX_INITIAL_OWNER, CreateMutexExW, CreateMutexW, MUTEX_MODIFY_STATE,
        };

        use super::*;

        fn names() -> Names {
            Names {
                mutex: format!(r"Local\{}", unique("mutex")),
            }
        }

        fn wide(name: &str) -> Vec<u16> {
            name.encode_utf16().chain(Some(0)).collect()
        }

        /// The mutex as .NET's `new Mutex(true, name, out created)` makes it (Mutex.Windows.cs: `CreateMutexEx` with
        /// the initial owner, for MAXIMUM_ALLOWED | SYNCHRONIZE | MUTEX_MODIFY_STATE): its handle, and `created`.
        fn csharp(name: &str) -> (OwnedHandle, bool) {
            const MAXIMUM_ALLOWED: u32 = 0x0200_0000;
            const SYNCHRONIZE: u32 = 0x0010_0000;
            let name = wide(name);
            // SAFETY: a NUL-terminated name, no security attributes
            let handle = unsafe {
                CreateMutexExW(
                    std::ptr::null(),
                    name.as_ptr(),
                    CREATE_MUTEX_INITIAL_OWNER,
                    MAXIMUM_ALLOWED | SYNCHRONIZE | MUTEX_MODIFY_STATE,
                )
            };
            // SAFETY: reads the error CreateMutexExW set
            let error = unsafe { GetLastError() };
            assert_ne!(handle, 0, "error {error}");
            // SAFETY: a handle just opened
            (
                unsafe { OwnedHandle::from_raw_handle(handle as _) },
                error != ERROR_ALREADY_EXISTS,
            )
        }

        #[test]
        fn a_mutex_the_csharp_made_keeps_this_pet_out_and_the_other_way_round() {
            let names = names();
            // the .NET pet first
            let (dotnet, created) = csharp(&names.mutex);
            assert!(created);
            assert!(take(&names).unwrap().is_none());
            drop(dotnet);
            // gone with it: this pet next, and a .NET pet started now doesn't create it
            let pet = take(&names).unwrap().expect("the only pet");
            let (_, created) = csharp(&names.mutex);
            assert!(!created, "a .NET pet started second is the second");
            drop(pet);
            assert!(csharp(&names.mutex).1, "let go with this pet");
        }

        #[test]
        fn pets_started_at_the_same_moment_give_one() {
            for _ in 0..20 {
                let names = names();
                let barrier = Arc::new(Barrier::new(8));
                let threads: Vec<_> = (0..8)
                    .map(|i| {
                        let (names, barrier) = (names.clone(), Arc::clone(&barrier));
                        std::thread::spawn(move || {
                            barrier.wait();
                            // half of them .NET pets
                            if i % 2 == 0 {
                                let (handle, created) = csharp(&names.mutex);
                                (created, Some(handle), None)
                            } else {
                                let pet = take(&names).unwrap();
                                (pet.is_some(), None, pet)
                            }
                        })
                    })
                    .collect();
                // every thread holds on to what it took until all are done
                let taken: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
                assert_eq!(taken.iter().filter(|(won, ..)| *won).count(), 1);
            }
        }

        #[test]
        fn a_mutex_this_pet_may_not_open_is_another_pets() {
            let names = names();
            // a mutex whose DACL lets no one in, as an elevated pet's lets in only Administrators
            let mut acl = [0u64; 8];
            let mut descriptor: SECURITY_DESCRIPTOR = unsafe { std::mem::zeroed() };
            // SAFETY: buffers of the sizes given, which outlive the mutex's creation
            unsafe {
                assert_ne!(
                    InitializeAcl(acl.as_mut_ptr().cast::<ACL>(), size_of_val(&acl) as u32, 2),
                    0
                );
                assert_ne!(InitializeSecurityDescriptor((&raw mut descriptor).cast(), 1), 0);
                assert_ne!(
                    SetSecurityDescriptorDacl((&raw mut descriptor).cast(), 1, acl.as_ptr().cast(), 0),
                    0
                );
            }
            let attributes = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: (&raw mut descriptor).cast(),
                bInheritHandle: 0,
            };
            let name = wide(&names.mutex);
            // SAFETY: valid attributes and a NUL-terminated name
            let handle = unsafe { CreateMutexW(&attributes, 1, name.as_ptr()) };
            assert_ne!(handle, 0);
            // SAFETY: a handle just opened
            let _elevated = unsafe { OwnedHandle::from_raw_handle(handle as _) };
            assert!(take(&names).unwrap().is_none());
        }

        #[test]
        fn a_pet_in_another_process_is_kept_out() {
            let names = names();
            let (_dotnet, created) = csharp(&names.mutex);
            assert!(created);
            let output = child(
                "single::tests::contender",
                &[("AIPET_SINGLE_MUTEX", &names.mutex), ("AIPET_SINGLE_TAKE", "1")],
            );
            assert_eq!(said(&output, "taken: "), "false");
        }
    }

    #[cfg(unix)]
    mod unix {
        use std::fs::{File, OpenOptions};
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        use super::*;

        /// A test's own folder, removed with it; its socket path stays short (a socket's must be under 104 bytes).
        struct Scratch(PathBuf);

        impl Scratch {
            fn new() -> Scratch {
                let dir = PathBuf::from("/tmp").join(unique("lock"));
                std::fs::create_dir_all(&dir).unwrap();
                Scratch(dir)
            }

            fn names(&self) -> Names {
                let endpoint = self.0.join("pet.sock");
                Names {
                    lock: lock_path(endpoint.as_os_str(), true),
                    endpoint: endpoint.into_os_string(),
                }
            }
        }

        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        /// The lock as the .NET pet takes it: open(O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC, 0600), then
        /// flock(LOCK_EX | LOCK_NB). The file while it holds the lock, none when another does.
        fn csharp(path: &Path) -> Option<File> {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path)
                .unwrap();
            // SAFETY: the descriptor of the file just opened
            (unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0).then_some(file)
        }

        #[test]
        fn the_lock_is_the_sockets_with_lock_for_sock() {
            let at = |endpoint: &str, overridden| lock_path(endpoint.as_ref(), overridden);
            assert_eq!(
                at("/run/user/1000/aipet.sock", false),
                Path::new("/run/user/1000/aipet.lock")
            );
            assert_eq!(
                at("/home/me/.local/share/AiPet/aipet.sock", false),
                Path::new("/home/me/.local/share/AiPet/aipet.lock")
            );
            // AIPET_PIPE names any path
            assert_eq!(at("/tmp/pets/test.sock", true), Path::new("/tmp/pets/test.sock.lock"));
            assert_eq!(at("/tmp/pets/test", true), Path::new("/tmp/pets/test.lock"));
        }

        #[test]
        fn a_lock_the_csharp_took_keeps_this_pet_out_and_the_other_way_round() {
            let scratch = Scratch::new();
            let names = scratch.names();
            let dotnet = csharp(&names.lock).expect("the first");
            assert!(take(&names).unwrap().is_none());
            drop(dotnet);
            let pet = take(&names).unwrap().expect("the only pet");
            assert!(csharp(&names.lock).is_none(), "a .NET pet started second is the second");
            let mode = std::fs::metadata(&names.lock).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
            drop(pet);
            assert!(csharp(&names.lock).is_some(), "let go with this pet");
        }

        #[test]
        fn pets_started_at_the_same_moment_give_one() {
            let scratch = Scratch::new();
            for _ in 0..20 {
                let names = scratch.names();
                let barrier = Arc::new(Barrier::new(8));
                let threads: Vec<_> = (0..8)
                    .map(|i| {
                        let (names, barrier) = (names.clone(), Arc::clone(&barrier));
                        std::thread::spawn(move || {
                            barrier.wait();
                            if i % 2 == 0 {
                                let file = csharp(&names.lock);
                                (file.is_some(), file, None)
                            } else {
                                let pet = take(&names).unwrap();
                                (pet.is_some(), None, pet)
                            }
                        })
                    })
                    .collect();
                let taken: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
                assert_eq!(taken.iter().filter(|(won, ..)| *won).count(), 1);
            }
        }

        #[test]
        fn a_link_where_the_lock_goes_is_an_error() {
            let scratch = Scratch::new();
            let names = scratch.names();
            std::os::unix::fs::symlink(scratch.0.join("elsewhere"), &names.lock).unwrap();
            assert!(take(&names).is_err());
            assert!(!scratch.0.join("elsewhere").exists(), "nothing made through the link");
        }

        #[test]
        fn pets_in_other_processes_with_other_runtime_folders_are_kept_out() {
            let scratch = Scratch::new();
            let names = scratch.names();
            let _dotnet = csharp(&names.lock).expect("the first");
            let pipe = names.endpoint.to_str().unwrap();
            let elsewhere = scratch.0.join("run");
            std::fs::create_dir_all(&elsewhere).unwrap();
            for runtime in [
                None,
                Some(elsewhere.to_str().unwrap()),
                Some(scratch.0.to_str().unwrap()),
            ] {
                let mut env = vec![("AIPET_PIPE", pipe), ("AIPET_SINGLE_TAKE", "1")];
                env.extend(runtime.map(|dir| ("XDG_RUNTIME_DIR", dir)));
                let output = child("single::tests::contender", &env);
                assert_eq!(said(&output, "lock: "), names.lock.to_str().unwrap());
                assert_eq!(said(&output, "taken: "), "false", "XDG_RUNTIME_DIR={runtime:?}");
            }
        }

        #[test]
        fn the_lock_follows_the_socket_whatever_xdg_runtime_dir_says() {
            // only where the lock would be: nothing is taken, so the user's own lock is never touched
            let scratch = Scratch::new();
            let home = scratch.0.to_str().unwrap();
            let runtimes = [scratch.0.join("a"), scratch.0.join("b")];
            for dir in &runtimes {
                std::fs::create_dir_all(dir).unwrap();
            }
            let lock_with = |runtime: Option<&Path>| {
                let mut env = vec![("HOME", home), ("XDG_DATA_HOME", "")];
                env.extend(runtime.map(|dir| ("XDG_RUNTIME_DIR", dir.to_str().unwrap())));
                PathBuf::from(said(&child("single::tests::contender", &env), "lock: "))
            };
            let locks = [
                lock_with(None),
                lock_with(Some(&runtimes[0])),
                lock_with(Some(&runtimes[1])),
            ];
            let login = aipet_ipc::endpoint::effective_uid().map(|uid| PathBuf::from(format!("/run/user/{uid}")));
            match login.filter(|dir| dir.is_dir()) {
                // the uid's own folder comes first: one lock, whatever the environment
                Some(dir) => assert!(locks.iter().all(|lock| *lock == dir.join("aipet.lock")), "{locks:?}"),
                // no login folder (a container): the runtime folder, else the data folder
                None => assert_eq!(
                    locks,
                    [
                        scratch.0.join(".local/share/AiPet/aipet.lock"),
                        runtimes[0].join("aipet.lock"),
                        runtimes[1].join("aipet.lock"),
                    ]
                ),
            }
        }
    }
}
