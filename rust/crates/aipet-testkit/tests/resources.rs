//! The Windows exes' resources, ported from AiPet.Tests/ResourceTests.cs: AiPet.exe and aipet-hook.exe carry
//! the app's icon, a manifest, and a version resource that names the exe, from their build scripts
//! (rust/crates/aipet/build.rs, rust/crates/aipet-hook/build.rs) as the .NET app's and hook's did from the .NET
//! build's Win32Resources targets. Read here from the PE file, and on Windows also by Windows itself. Only a Windows
//! build has them, so these run there and are skipped elsewhere.

#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use aipet_testkit::{built, repo};

const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;
const RT_VERSION: u32 = 16;
const RT_MANIFEST: u32 = 24;

/// The exes, each with its file name and its description.
const EXES: [(&str, &str, &str); 2] = [
    ("AiPet", "AiPet.exe", "AiPet"),
    ("aipet-hook", "aipet-hook.exe", "AiPet hook"),
];

/// The version the build scripts gave both: `AIPET_VERSION`, else 0.0.0-dev.
fn version() -> String {
    std::env::var("AIPET_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "0.0.0-dev".into())
}

/// Its four numbers, as the build scripts make FileVersion of it: 0.3.0-rc.1 is 0.3.0.0.
fn file_version() -> String {
    let version = version();
    let release = version.split(['-', '+']).next().unwrap_or_default();
    let mut numbers = [0u16; 4];
    for (n, part) in numbers.iter_mut().zip(release.split('.')) {
        *n = part.parse().unwrap_or(0);
    }
    numbers.map(|n| n.to_string()).join(".")
}

// ------------------------------------------------------------------ reading them
fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

/// The Win32 resources of a PE file by type and numeric name (the first language of each).
fn resources(file: &Path) -> BTreeMap<(u32, u32), Vec<u8>> {
    let pe = fs::read(file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let header = u32_at(&pe, 0x3c) as usize;
    assert_eq!(&pe[header..header + 4], b"PE\0\0", "{} isn't a PE file", file.display());
    let coff = header + 4;
    let sections = u16_at(&pe, coff + 2) as usize;
    let optional = coff + 20;
    let optional_size = u16_at(&pe, coff + 16) as usize;
    // the data directories: after 96 bytes in PE32, 112 in PE32+; the resource table is the third
    let directories = optional + if u16_at(&pe, optional) == 0x20b { 112 } else { 96 };
    let (rva, size) = (u32_at(&pe, directories + 16), u32_at(&pe, directories + 20) as usize);
    assert!(size > 0, "{} has no Win32 resources", file.display());
    // a virtual address's place in the file, by the section that holds it
    let offset = |rva: u32| -> usize {
        (0..sections)
            .map(|i| optional + optional_size + 40 * i)
            .find_map(|s| {
                let (start, length) = (u32_at(&pe, s + 12), u32_at(&pe, s + 8).max(u32_at(&pe, s + 16)));
                (start..start + length)
                    .contains(&rva)
                    .then(|| (rva - start + u32_at(&pe, s + 20)) as usize)
            })
            .unwrap_or_else(|| panic!("no section holds {rva:#x}"))
    };
    let base = offset(rva);
    let rsrc = &pe[base..base + size];
    let entries = |at: usize| -> Vec<(u32, u32)> {
        let n = u16_at(rsrc, at + 12) as usize + u16_at(rsrc, at + 14) as usize;
        (0..n)
            .map(|i| (u32_at(rsrc, at + 16 + 8 * i), u32_at(rsrc, at + 20 + 8 * i)))
            .collect()
    };
    const SUB: u32 = 0x8000_0000;
    let mut result = BTreeMap::new();
    for (kind, kinds) in entries(0).into_iter().filter(|(id, t)| id & SUB == 0 && t & SUB != 0) {
        for (name, names) in entries((kinds & !SUB) as usize)
            .into_iter()
            .filter(|(id, t)| id & SUB == 0 && t & SUB != 0)
        {
            let (_, leaf) = entries((names & !SUB) as usize)[0];
            let (data, length) = (u32_at(rsrc, leaf as usize), u32_at(rsrc, leaf as usize + 4) as usize);
            let at = offset(data);
            result.insert((kind, name), pe[at..at + length].to_vec());
        }
    }
    result
}

/// VS_VERSIONINFO: the fixed file version, and the strings of its string table.
fn version_info(d: &[u8]) -> (String, BTreeMap<String, String>) {
    let align = |p: usize| (p + 3) & !3;
    // a block: length, value length, type (1: text), key, value, children
    let block = |at: usize| -> (String, Vec<u8>, Vec<usize>) {
        let (length, value_length, kind) = (u16_at(d, at) as usize, u16_at(d, at + 2) as usize, u16_at(d, at + 4));
        let mut p = at + 6;
        let mut key = Vec::new();
        while u16_at(d, p) != 0 {
            key.push(u16_at(d, p));
            p += 2;
        }
        p = align(p + 2);
        let size = if kind == 1 { value_length * 2 } else { value_length };
        let value = d[p..p + size].to_vec();
        let mut children = Vec::new();
        p = align(p + size);
        while p < at + length {
            children.push(p);
            p = align(p + u16_at(d, p) as usize);
        }
        (String::from_utf16(&key).unwrap(), value, children)
    };
    let (key, value, children) = block(0);
    assert_eq!(key, "VS_VERSION_INFO");
    assert_eq!(u32_at(&value, 0), 0xFEEF04BD);
    let word = |i: usize| u16_at(&value, 8 + i * 2);
    // dwFileVersionMS, LS: high word first
    let fixed = format!("{}.{}.{}.{}", word(1), word(0), word(3), word(2));
    let mut strings = BTreeMap::new();
    for (key, _, tables) in children.into_iter().map(block) {
        if key != "StringFileInfo" {
            continue;
        }
        for (_, _, entries) in tables.into_iter().map(block) {
            for (key, value, _) in entries.into_iter().map(block) {
                let units: Vec<u16> = value
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&c| u16::from_le_bytes(c))
                    .collect();
                strings.insert(
                    key,
                    String::from_utf16(&units).unwrap().trim_end_matches('\0').to_owned(),
                );
            }
        }
    }
    (fixed, strings)
}

// ------------------------------------------------------------------ the tests
#[cfg_attr(not(windows), ignore = "Windows only: only a Windows build has the resources")]
#[test]
fn version_names_the_exe() {
    for (bin, exe, title) in EXES {
        let res = resources(&built(bin));
        let (fixed, strings) = version_info(&res[&(RT_VERSION, 1)]);
        for (key, value) in [
            ("InternalName", exe),
            ("OriginalFilename", exe),
            ("FileDescription", title),
            ("ProductName", "AiPet"),
            ("CompanyName", "xMarcinator"),
        ] {
            assert_eq!(strings.get(key).map(String::as_str), Some(value), "{exe}: {key}");
        }
        assert!(strings["LegalCopyright"].starts_with("Copyright (c) "), "{exe}");
        // the version given to the build (AIPET_VERSION, else 0.0.0-dev)
        assert_eq!(strings["ProductVersion"], version(), "{exe}");
        assert_eq!(strings["FileVersion"], file_version(), "{exe}");
        assert_eq!(fixed, file_version(), "{exe}");
    }
}

/// The icon group lists every image of assets/icon/aipet.ico, each an RT_ICON with the image's bytes.
#[cfg_attr(not(windows), ignore = "Windows only: only a Windows build has the resources")]
#[test]
fn icon_is_the_app_icon() {
    let ico = fs::read(repo().join("assets/icon/aipet.ico")).unwrap();
    for (bin, exe, _) in EXES {
        let res = resources(&built(bin));
        let group = &res[&(RT_GROUP_ICON, 32512)];
        let count = u16_at(&ico, 4) as usize;
        assert!(count > 0);
        assert_eq!(ico[..6], group[..6], "{exe}");
        assert_eq!(group.len(), 6 + 14 * count, "{exe}");
        for i in 0..count {
            let (e, g) = (6 + 16 * i, 6 + 14 * i);
            assert_eq!(ico[e..e + 12], group[g..g + 12], "{exe}: image {i}");
            let id = u16_at(group, g + 12) as u32;
            let (size, offset) = (u32_at(&ico, e + 8) as usize, u32_at(&ico, e + 12) as usize);
            assert!(
                ico[offset..offset + size] == res[&(RT_ICON, id)][..],
                "{exe}: image {i}"
            );
        }
        assert_eq!(res.keys().filter(|(kind, _)| *kind == RT_ICON).count(), count, "{exe}");
    }
}

/// The manifests the .NET exes had: the app's app.manifest (per-monitor DPI awareness), the compiler's default for
/// the hook (asInvoker).
#[cfg_attr(not(windows), ignore = "Windows only: only a Windows build has the resources")]
#[test]
fn manifest_is_embedded() {
    for (bin, text) in [("AiPet", "PerMonitorV2"), ("aipet-hook", "level=\"asInvoker\"")] {
        let res = resources(&built(bin));
        let manifest = String::from_utf8_lossy(&res[&(RT_MANIFEST, 1)]).into_owned();
        assert!(manifest.contains(text), "{bin}: {manifest}");
    }
}

/// The hook carries the resources the .NET hook had, and no others: the icon group and its images, the manifest and
/// the version resource (the C# compared them byte for byte with the .NET hook's; their values are held above).
#[cfg_attr(not(windows), ignore = "Windows only: only a Windows build has the resources")]
#[test]
fn the_hook_has_the_dotnet_hooks_resources_and_no_others() {
    let ico = fs::read(repo().join("assets/icon/aipet.ico")).unwrap();
    let res = resources(&built("aipet-hook"));
    let group = &res[&(RT_GROUP_ICON, 32512)];
    let mut expected: Vec<(u32, u32)> = (0..u16_at(&ico, 4) as usize)
        .map(|i| (RT_ICON, u16_at(group, 6 + 14 * i + 12) as u32))
        .collect();
    expected.extend([(RT_GROUP_ICON, 32512), (RT_VERSION, 1), (RT_MANIFEST, 1)]);
    expected.sort();
    assert_eq!(res.keys().copied().collect::<Vec<_>>(), expected);
}

/// Windows reads the version resource as the tests do (`FileVersionInfo`: GetFileVersionInfoW and VerQueryValueW).
#[cfg_attr(not(windows), ignore = "Windows only")]
#[test]
fn windows_reads_the_version() {
    #[cfg(windows)]
    for (bin, exe, title) in EXES {
        let file = built(bin);
        for (key, value) in [
            ("OriginalFilename", exe),
            ("InternalName", exe),
            ("FileDescription", title),
            ("ProductName", "AiPet"),
            ("ProductVersion", &version()),
        ] {
            assert_eq!(windows::query(&file, key).as_deref(), Some(value), "{exe}: {key}");
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};

    fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
        s.as_ref().encode_wide().chain([0]).collect()
    }

    /// A string of the version resource in its first translation, as Windows reads it.
    pub fn query(file: &Path, key: &str) -> Option<String> {
        let path = wide(file);
        // SAFETY: a NUL-terminated path; the handle argument is unused
        let size = unsafe { GetFileVersionInfoSizeW(path.as_ptr(), std::ptr::null_mut()) };
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        // SAFETY: a buffer of the size Windows asked for
        if unsafe { GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast()) } == 0 {
            return None;
        }
        let value = |sub: &str| -> Option<(*const u8, u32)> {
            let (mut at, mut len) = (std::ptr::null_mut(), 0u32);
            let sub = wide(sub);
            // SAFETY: the block GetFileVersionInfoW filled, a NUL-terminated sub-block name
            let found = unsafe { VerQueryValueW(data.as_ptr().cast(), sub.as_ptr(), &mut at, &mut len) };
            (found != 0 && !at.is_null()).then_some((at.cast_const().cast(), len))
        };
        let (translation, _) = value("\\VarFileInfo\\Translation")?;
        // SAFETY: a translation is two u16s: the language, then the code page
        let (language, codepage) = unsafe {
            let words = translation.cast::<u16>();
            (words.read_unaligned(), words.add(1).read_unaligned())
        };
        let (text, len) = value(&format!("\\StringFileInfo\\{language:04x}{codepage:04x}\\{key}"))?;
        // SAFETY: len UTF-16 units, the NUL included
        let units = unsafe { std::slice::from_raw_parts(text.cast::<u16>(), len as usize) };
        Some(String::from_utf16_lossy(units).trim_end_matches('\0').to_owned())
    }
}
