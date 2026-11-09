//! The Windows exe's resources, the ones the .NET pet's AiPet.exe had (build/Win32Resources.targets): the app's icon,
//! a version resource that names `AiPet.exe`, and the .NET pet's manifest (app.manifest). Antivirus heuristics dislike
//! an exe without them, and the release checks them. Nothing for any other OS.
//!
//! The version is `AIPET_VERSION`, which the release sets to the version it releases, else `0.0.0-dev` as
//! Directory.Build.props has it; the pet shows the same one in Settings (`updates::version`). FileVersion is its
//! numbers: 0.3.0-rc.1 makes 0.3.0.0, as the .NET SDK makes it.

use std::env;
use std::path::{Path, PathBuf};

use winresource::{VersionInfo, WindowsResource};

/// The icon group's id: the one csc gives an ApplicationIcon, as the .NET pet's.
const ICON_GROUP: &str = "32512";
/// VOS__WINDOWS32, as the .NET pet's version resource has it.
const FILE_OS: u64 = 0x4;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=AIPET_VERSION");
    let version = env::var("AIPET_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "0.0.0-dev".into());
    println!("cargo:rustc-env=AIPET_VERSION={version}");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let crate_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let icon = repo(&crate_dir).join("assets").join("icon").join("aipet.ico");
    let manifest = crate_dir.join("app.manifest");
    for file in [&icon, &manifest] {
        println!("cargo:rerun-if-changed={}", file.display());
    }
    let numbers = numbers(&version);

    let mut res = WindowsResource::new();
    res.set_icon_with_id(&text(&icon), ICON_GROUP)
        .set_manifest_file(&text(&manifest))
        .set("CompanyName", "xMarcinator")
        .set("FileDescription", "AiPet")
        .set("FileVersion", &numbers.map(|n| n.to_string()).join("."))
        .set("InternalName", "AiPet.exe")
        .set("LegalCopyright", "Copyright (c) 2026 xMarcinator")
        .set("OriginalFilename", "AiPet.exe")
        .set("ProductName", "AiPet")
        .set("ProductVersion", &version)
        .set_version_info(VersionInfo::FILEVERSION, packed(numbers))
        .set_version_info(VersionInfo::PRODUCTVERSION, packed(numbers))
        .set_version_info(VersionInfo::FILEOS, FILE_OS);
    if let Err(e) = res.compile() {
        panic!("couldn't compile AiPet.exe's resources (the Windows SDK's rc.exe): {e}");
    }
}

/// The repository's root: rust/crates/aipet is three folders down.
fn repo(crate_dir: &Path) -> PathBuf {
    crate_dir
        .ancestors()
        .nth(3)
        .expect("aipet is at rust/crates/aipet")
        .to_path_buf()
}

fn text(path: &Path) -> String {
    path.to_str().expect("the repository's path is Unicode").to_owned()
}

/// The version's four numbers, before any prerelease or build suffix; a missing or unreadable one is 0, as the .NET
/// version resource's are.
fn numbers(version: &str) -> [u16; 4] {
    let release = version.split(['-', '+']).next().unwrap_or_default();
    let mut numbers = [0; 4];
    for (n, part) in numbers.iter_mut().zip(release.split('.')) {
        *n = part.parse().unwrap_or(0);
    }
    numbers
}

/// The numbers as VS_FIXEDFILEINFO has them: the first in the high word of the most significant half.
fn packed(numbers: [u16; 4]) -> u64 {
    numbers.iter().fold(0, |packed, &n| packed << 16 | u64::from(n))
}
