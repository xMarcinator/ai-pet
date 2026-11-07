//! What the hook and the pet share: how a hook reaches the running pet, and where the app's files are.
//!
//! A port of `src/AiPet.Core/Ipc.cs` and `src/AiPet.Core/Paths.cs`, which the .NET hook links as source. During the
//! migration either side can be the other runtime (a Rust hook with a .NET pet, or the other way round), so this is
//! the seam, and it matches the C# exactly:
//! - [`protocol`]: the wire protocol's version, limits, field names and time format;
//! - [`endpoint`]: the pipe's name on Windows, the socket's path elsewhere;
//! - [`paths`]: the data folder and the files in it, and Codex's home;
//! - [`connect`]: connecting to the pet and asking it one thing, within the C#'s time limits.
//!
//! It has no async runtime and starts no process: the hook runs it on every agent event.

pub mod connect;
pub mod endpoint;
pub mod paths;
pub mod protocol;

#[cfg(test)]
mod csharp;

/// UTF-16 strings for the Win32 calls.
#[cfg(windows)]
mod wide {
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    pub(crate) fn nul_terminated(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(Some(0)).collect()
    }

    /// # Safety
    /// `p` points at a NUL-terminated UTF-16 string.
    pub(crate) unsafe fn from_ptr(p: *const u16) -> OsString {
        let mut len = 0;
        // SAFETY: the string goes on to its NUL, which the caller guarantees
        while unsafe { *p.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: the `len` units before the NUL were just read
        OsString::from_wide(unsafe { std::slice::from_raw_parts(p, len) })
    }
}
