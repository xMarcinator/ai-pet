//! Credential Manager: generic credentials whose target name is the key (`WindowsPlatform.cs:179-226`).

use std::io;
use std::ptr;

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW, CredWriteW,
};

use super::SecretStore;

/// The .NET app's `CredentialManager`: a generic credential, persisted for this user on this machine, whose target
/// name is the key, whose user name is the one given, and whose blob is the secret in UTF-16.
#[derive(Clone, Copy, Debug, Default)]
pub struct CredentialManager;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

impl SecretStore for CredentialManager {
    /// A credential without a blob has no secret, as for the C#.
    fn read(&self, key: &str) -> Option<String> {
        let target = wide(key);
        let mut found: *mut CREDENTIALW = ptr::null_mut();
        // SAFETY: target is NUL-terminated; found is written only
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut found) } == 0 {
            return None;
        }
        // SAFETY: CredReadW succeeded, so found points at a credential, freed below
        let credential = unsafe { &*found };
        let secret = (!credential.CredentialBlob.is_null()).then(|| {
            // the blob's whole UTF-16 units, as Marshal.PtrToStringUni(blob, size / 2) takes them; read as bytes, as
            // nothing says the blob is aligned for u16
            // SAFETY: the blob holds CredentialBlobSize bytes
            let bytes = unsafe {
                std::slice::from_raw_parts(credential.CredentialBlob, credential.CredentialBlobSize as usize)
            };
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&b| u16::from_le_bytes(b))
                .collect();
            String::from_utf16_lossy(&units)
        });
        // SAFETY: found came from CredReadW
        unsafe { CredFree(found as *const _) };
        secret
    }

    fn write(&self, key: &str, user: &str, secret: &str) -> io::Result<()> {
        let target = wide(key);
        let mut user = wide(user);
        let mut blob: Vec<u16> = secret.encode_utf16().collect();
        let credential = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_ptr() as *mut u16,
            Comment: ptr::null_mut(),
            LastWritten: FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            },
            CredentialBlobSize: (blob.len() * 2) as u32,
            CredentialBlob: blob.as_mut_ptr().cast(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: ptr::null_mut(),
            TargetAlias: ptr::null_mut(),
            UserName: user.as_mut_ptr(),
        };
        // SAFETY: every pointer in credential points at a live buffer of the size given, NUL-terminated where a
        // string; CredWriteW only reads them
        let written = unsafe { CredWriteW(&credential, 0) } != 0;
        let result = if written {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        };
        // the secret's copy is wiped, as Marshal.ZeroFreeCoTaskMemUnicode does
        for unit in &mut blob {
            // SAFETY: unit is a valid, aligned u16 of blob
            unsafe { ptr::write_volatile(unit, 0) };
        }
        result
    }

    fn delete(&self, key: &str) {
        let target = wide(key);
        // SAFETY: target is NUL-terminated
        unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
    }
}
