//! Only Mesa's EGL driver, where the pet renders through GL on an Intel or AMD GPU (Linux).
//!
//! The GL loader (glvnd) loads every vendor's EGL driver it finds, and asks them in turn which one draws. On a laptop
//! with an NVIDIA GPU beside the integrated one, that maps NVIDIA's driver (about 100 MB) into the pet although only
//! Mesa draws: the pet renders on the low-power GPU, which drives the screen. Pointing
//! `__EGL_VENDOR_LIBRARY_FILENAMES` at Mesa's vendor file loads Mesa's alone (rust/proofs/memory.md).
//!
//! It is only done when it can't change which driver draws: the user hasn't chosen the EGL vendors, the pet renders
//! through GL on the low-power GPU, an Intel or AMD GPU drives a screen and no NVIDIA GPU does (where NVIDIA's
//! driver would draw, Mesa alone would fall back to drawing in software), and Mesa's vendor file is there.

use std::env;
use std::path::{Path, PathBuf};

/// PCI vendor IDs: the GPUs Mesa drives, and NVIDIA's.
const INTEL: &str = "0x8086";
const AMD: &str = "0x1002";
const NVIDIA: &str = "0x10de";

/// Mesa's vendor file, in a folder glvnd reads.
const MESA_FILE: &str = "50_mesa.json";

/// A GPU, as `/sys/class/drm` tells: its PCI vendor, and whether a screen is connected to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gpu {
    pub vendor: String,
    pub drives_screen: bool,
}

/// The environment that matters here: the variables, read after the GPU defaults are set.
#[derive(Debug, Default)]
pub struct Env {
    /// `__EGL_VENDOR_LIBRARY_FILENAMES` or `__EGL_VENDOR_LIBRARY_DIRS` is set: the user chose the vendors.
    pub vendors_chosen: bool,
    pub power_pref: Option<String>,
    pub backend: Option<String>,
}

/// Mesa's vendor file to load alone, or none when the loader should load them all, as it does by default. `mesa_files`
/// are Mesa's vendor files found, in glvnd's order.
pub fn decide(gpus: &[Gpu], mesa_files: &[PathBuf], env: &Env) -> Option<PathBuf> {
    let low_power = env.power_pref.as_deref() == Some("low");
    let gl = env.backend.as_deref() == Some("gl");
    let mesa_screen = gpus
        .iter()
        .any(|g| g.drives_screen && (g.vendor == INTEL || g.vendor == AMD));
    let nvidia_screen = gpus.iter().any(|g| g.drives_screen && g.vendor == NVIDIA);
    if env.vendors_chosen || !low_power || !gl || !mesa_screen || nvidia_screen {
        return None;
    }
    mesa_files.first().cloned()
}

/// Sets `__EGL_VENDOR_LIBRARY_FILENAMES` to Mesa's vendor file when [`decide`] says so.
///
/// # Safety
/// No other thread may read or write the environment meanwhile.
pub unsafe fn load_only_mesa() {
    let env = Env {
        vendors_chosen: env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").is_some()
            || env::var_os("__EGL_VENDOR_LIBRARY_DIRS").is_some(),
        power_pref: env::var("WGPU_POWER_PREF").ok(),
        backend: env::var("WGPU_BACKEND").ok(),
    };
    if let Some(file) = decide(&gpus(Path::new("/sys/class/drm")), &mesa_files(), &env) {
        // SAFETY: the caller's
        unsafe { env::set_var("__EGL_VENDOR_LIBRARY_FILENAMES", file) };
    }
}

/// The GPUs under `drm` (`/sys/class/drm`): each `cardN` with its vendor, connected to a screen when one of its
/// `cardN-<connector>` entries says `connected`.
fn gpus(drm: &Path) -> Vec<Gpu> {
    let Ok(entries) = std::fs::read_dir(drm) else {
        return Vec::new();
    };
    let names: Vec<String> = entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect();
    names
        .iter()
        .filter(|n| {
            n.strip_prefix("card")
                .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        })
        .filter_map(|card| {
            let vendor = std::fs::read_to_string(drm.join(card).join("device/vendor")).ok()?;
            let prefix = format!("{card}-");
            let drives_screen = names
                .iter()
                .filter(|n| n.starts_with(&prefix))
                .any(|n| std::fs::read_to_string(drm.join(n).join("status")).is_ok_and(|s| s.trim() == "connected"));
            Some(Gpu {
                vendor: vendor.trim().to_owned(),
                drives_screen,
            })
        })
        .collect()
}

/// Mesa's vendor files, in the order glvnd reads its folders: `/etc/glvnd/egl_vendor.d`, then
/// `glvnd/egl_vendor.d` in each of `$XDG_DATA_DIRS` (`/usr/local/share:/usr/share` when unset), and
/// `/usr/share/glvnd/egl_vendor.d`, where glvnd looks by default.
fn mesa_files() -> Vec<PathBuf> {
    let data_dirs = env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    let mut dirs = vec![PathBuf::from("/etc/glvnd/egl_vendor.d")];
    dirs.extend(
        data_dirs
            .split(':')
            .filter(|d| !d.is_empty())
            .map(|d| Path::new(d).join("glvnd/egl_vendor.d")),
    );
    dirs.push(PathBuf::from("/usr/share/glvnd/egl_vendor.d"));
    dirs.into_iter()
        .map(|d| d.join(MESA_FILE))
        .filter(|f| f.is_file())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(vendor: &str, drives_screen: bool) -> Gpu {
        Gpu {
            vendor: vendor.to_owned(),
            drives_screen,
        }
    }

    fn pet_env() -> Env {
        Env {
            vendors_chosen: false,
            power_pref: Some("low".to_owned()),
            backend: Some("gl".to_owned()),
        }
    }

    fn mesa() -> Vec<PathBuf> {
        vec![PathBuf::from("/usr/share/glvnd/egl_vendor.d/50_mesa.json")]
    }

    #[test]
    fn a_laptop_whose_intel_gpu_drives_the_screen_loads_only_mesa() {
        let gpus = [gpu(NVIDIA, false), gpu(INTEL, true)];
        assert_eq!(decide(&gpus, &mesa(), &pet_env()), mesa().first().cloned());
        let amd = [gpu(AMD, true), gpu(NVIDIA, false)];
        assert_eq!(decide(&amd, &mesa(), &pet_env()), mesa().first().cloned());
        // the first file found, in glvnd's order
        let both = [PathBuf::from("/etc/glvnd/egl_vendor.d/50_mesa.json"), mesa()[0].clone()];
        assert_eq!(decide(&gpus, &both, &pet_env()), Some(both[0].clone()));
    }

    #[test]
    fn an_nvidia_screen_keeps_every_driver() {
        assert_eq!(decide(&[gpu(NVIDIA, true)], &mesa(), &pet_env()), None);
        // a screen on each: NVIDIA's driver may be the one that draws
        assert_eq!(
            decide(&[gpu(INTEL, true), gpu(NVIDIA, true)], &mesa(), &pet_env()),
            None
        );
    }

    #[test]
    fn no_screen_on_a_mesa_gpu_keeps_every_driver() {
        assert_eq!(
            decide(&[gpu(INTEL, false), gpu(NVIDIA, false)], &mesa(), &pet_env()),
            None
        );
        assert_eq!(decide(&[], &mesa(), &pet_env()), None);
        assert_eq!(decide(&[gpu("0x1af4", true)], &mesa(), &pet_env()), None);
    }

    #[test]
    fn without_mesas_file_nothing_changes() {
        assert_eq!(decide(&[gpu(INTEL, true)], &[], &pet_env()), None);
    }

    #[test]
    fn the_users_choices_win() {
        let gpus = [gpu(INTEL, true), gpu(NVIDIA, false)];
        let chosen = Env {
            vendors_chosen: true,
            ..pet_env()
        };
        assert_eq!(decide(&gpus, &mesa(), &chosen), None);
        let fast = Env {
            power_pref: Some("high".to_owned()),
            ..pet_env()
        };
        assert_eq!(decide(&gpus, &mesa(), &fast), None);
        let vulkan = Env {
            backend: Some("vulkan".to_owned()),
            ..pet_env()
        };
        assert_eq!(decide(&gpus, &mesa(), &vulkan), None);
    }

    #[test]
    fn the_gpus_are_read_from_sysfs() {
        let drm = std::env::temp_dir().join(format!("aipet-mesa-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&drm);
        for (dir, file, text) in [
            ("card1/device", "vendor", "0x10de\n"),
            ("card1-HDMI-A-1", "status", "disconnected\n"),
            ("card2/device", "vendor", "0x8086\n"),
            ("card2-DP-4", "status", "disconnected\n"),
            ("card2-eDP-1", "status", "connected\n"),
            ("renderD128", "dev", "226:128\n"),
        ] {
            std::fs::create_dir_all(drm.join(dir)).unwrap();
            std::fs::write(drm.join(dir).join(file), text).unwrap();
        }
        let mut found = gpus(&drm);
        found.sort_by(|a, b| a.vendor.cmp(&b.vendor));
        assert_eq!(found, [gpu(NVIDIA, false), gpu(INTEL, true)]);
        std::fs::remove_dir_all(&drm).unwrap();
    }
}
