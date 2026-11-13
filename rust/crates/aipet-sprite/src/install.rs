//! Installing a pack from a folder (fnt-71's `install_pack`): the pet copies it into `<data>/staging/`, following no
//! links and within [`MAX_BYTES`] and [`MAX_ENTRIES`], checks the copy as `--check-pack` does ([`load::check`]), and
//! moves it into the avatars folder with renames (staging is on the same volume). A replacement swaps the existing
//! style's own folder, wherever it loaded from: the old one is renamed to a backup, the new one into its place, the
//! backup restored if that fails and deleted only once it worked. One install runs at a time. On any failure the
//! avatars folder is as it was and nothing is left in staging.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Avatar;
use crate::load::{self, LoadedPack, THEMES, clash, reparse_point};

/// The most a pack's folder may hold, in bytes.
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// The most files and folders a pack's folder may hold.
pub const MAX_ENTRIES: usize = 2_000;

/// One install at a time.
static ONE: Mutex<()> = Mutex::new(());

/// Where packs are copied before they are checked: `<data>/staging`.
pub fn staging_dir(data: &Path) -> PathBuf {
    data.join("staging")
}

/// A pack installed: its pet, its drawings' names, the folder it is in now, the check's report, and whether it
/// replaced one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub pet: String,
    pub names: Vec<String>,
    pub folder: PathBuf,
    pub report: String,
    pub replaced: bool,
}

/// Installs the pack or pet folder `source`, which must be a folder inside `home`, into `<data>/avatars`. A pack whose
/// pet and style, or whose name, is already there is refused unless `replace`, and a built-in one always is.
pub fn install(data: &Path, home: &Path, source: &Path, replace: bool) -> Result<Installed, String> {
    install_with(data, home, source, replace, &|from, to| fs::rename(from, to))
}

/// [`install`], moving folders with `rename` (tests make it fail).
fn install_with(
    data: &Path,
    home: &Path,
    source: &Path,
    replace: bool,
    rename: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> Result<Installed, String> {
    let shown = source.display();
    let meta = fs::symlink_metadata(source).map_err(|_| format!("{shown} isn't there"))?;
    if meta.file_type().is_symlink() || reparse_point(&meta) {
        return Err(format!("{shown} is a link: name the folder itself"));
    }
    if !meta.is_dir() {
        return Err(format!("{shown} isn't a folder"));
    }
    let real = source.canonicalize().map_err(|e| format!("{shown}: {e}"))?;
    let home = home
        .canonicalize()
        .map_err(|e| format!("the home folder {}: {e}", home.display()))?;
    if !real.starts_with(&home) || real == home {
        return Err(format!(
            "{shown} isn't a folder inside your home folder ({})",
            home.display()
        ));
    }
    let name = real
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| format!("{shown} has no name"))?;
    if name == THEMES {
        return Err(format!("a pack can't be named {THEMES}: that is the themes folder"));
    }
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());

    let staging = staging_dir(data);
    let id = unique();
    let stage = Cleanup(staging.join(&id));
    fs::create_dir_all(&stage.0).map_err(|e| format!("Couldn't make {}: {e}", stage.0.display()))?;
    let staged = stage.0.join(&name);
    copy(&real, &staged, Path::new(&name), &mut Budget::default())?;

    let (report, ok) = load::check(&staged);
    if !ok {
        return Err(format!("the pack's check failed:\n{report}"));
    }
    let new = load::drawings(&staged).pets;
    let avatars = Avatar::custom_dir(data);
    let existing = load::all(&avatars);
    let target = target(&avatars, &name, &new, &existing, load::is_pet_folder(&staged), replace)?;

    fs::create_dir_all(&avatars).map_err(|e| format!("Couldn't make the avatars folder: {e}"))?;
    let replaced = fs::symlink_metadata(&target).is_ok();
    if replaced {
        let backup = staging.join(format!("{id}.old"));
        rename(&target, &backup).map_err(|e| format!("Couldn't move the old pack aside: {e}"))?;
        if let Err(e) = rename(&staged, &target) {
            return Err(match rename(&backup, &target) {
                Ok(()) => format!("Couldn't move the new pack into place, so the old one stays: {e}"),
                Err(back) => format!(
                    "Couldn't move the new pack into place ({e}), nor the old one back ({back}): it is at {}",
                    backup.display()
                ),
            });
        }
        let _ = fs::remove_dir_all(&backup);
    } else {
        rename(&staged, &target).map_err(|e| format!("Couldn't move the pack into the avatars folder: {e}"))?;
    }
    Ok(Installed {
        pet: new.first().map(|p| p.pack.pet().to_owned()).unwrap_or_default(),
        names: new.iter().map(|p| p.name().to_owned()).collect(),
        folder: target,
        report,
        replaced,
    })
}

/// Where the pack `name` with the drawings `new` goes in `avatars`: its own name there, or, to replace, the folder of
/// the user's drawing it replaces (for a pet folder, the pet folder that holds it). Refused when it would clash with a
/// drawing it doesn't replace, replace a built-in, replace without `replace`, or replace more than one folder.
fn target(
    avatars: &Path,
    name: &str,
    new: &[LoadedPack],
    existing: &load::Found,
    pet_folder: bool,
    replace: bool,
) -> Result<PathBuf, String> {
    let own = avatars.join(name);
    let mut clashes: Vec<(&LoadedPack, String)> = Vec::new();
    for n in new {
        for p in &existing.pets {
            if let Some(why) = clash(std::slice::from_ref(p), &n.pack) {
                clashes.push((p, why));
            }
        }
    }
    let taken = fs::symlink_metadata(&own).is_ok();
    if !replace {
        if let Some((_, why)) = clashes.first() {
            return Err(format!("{why}: pass replace to replace it"));
        }
        if taken {
            return Err(format!(
                "the avatars folder already has {name}: pass replace to replace it"
            ));
        }
        return Ok(own);
    }
    if let Some((p, _)) = clashes.iter().find(|(p, _)| p.builtin) {
        return Err(format!(
            "{} in {} is built in, and a built-in style can't be replaced",
            p.pack.pet(),
            p.pack.style()
        ));
    }
    // the folders it replaces: each clashing drawing's own, or for a pet folder the pet folder holding it
    let mut targets: BTreeSet<PathBuf> = BTreeSet::new();
    for (p, _) in &clashes {
        let Some(place) = existing.places.get(p.name()) else {
            return Err(format!("couldn't find where {} loaded from", p.name()));
        };
        let place = if pet_folder {
            match place.strip_prefix(avatars).ok().and_then(|r| r.components().next()) {
                Some(top) => avatars.join(top),
                None => place.clone(),
            }
        } else {
            place.clone()
        };
        targets.insert(place);
    }
    if targets.is_empty() || taken && !targets.contains(&own) && clashes.is_empty() {
        targets.insert(own.clone());
    }
    if targets.len() > 1 {
        let names: Vec<String> = targets.iter().map(|t| t.display().to_string()).collect();
        return Err(format!("it would replace more than one pack: {}", names.join(", ")));
    }
    let target = targets.into_iter().next().expect("one target");
    if fs::symlink_metadata(&target).is_ok_and(|m| !m.is_dir()) {
        return Err(format!("{} is a file, not a pack's folder", target.display()));
    }
    // what stays must not clash with it
    for n in new {
        let stays = existing.pets.iter().filter(|p| {
            p.builtin
                || existing
                    .places
                    .get(p.name())
                    .is_none_or(|place| !place.starts_with(&target))
        });
        for p in stays {
            if let Some(why) = clash(std::slice::from_ref(p), &n.pack) {
                return Err(why);
            }
        }
    }
    Ok(target)
}

/// A name no other install has, now or before.
fn unique() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!(
        "{}-{nanos:x}-{}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    )
}

/// A folder removed, with all in it, when dropped.
struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// What a copy has used of the limits so far.
#[derive(Default)]
struct Budget {
    bytes: u64,
    entries: usize,
}

/// Copies the folder `from` to `to` (made here), counting against the limits and following no links. `rel` names it
/// in errors.
fn copy(from: &Path, to: &Path, rel: &Path, budget: &mut Budget) -> Result<(), String> {
    fs::create_dir(to).map_err(|e| format!("Couldn't copy {}: {e}", rel.display()))?;
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(from)
        .and_then(|e| e.collect::<io::Result<_>>())
        .map_err(|e| format!("Couldn't read {}: {e}", rel.display()))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let rel = rel.join(entry.file_name());
        budget.entries += 1;
        if budget.entries > MAX_ENTRIES {
            return Err(format!("the folder holds more than {MAX_ENTRIES} files and folders"));
        }
        let meta = fs::symlink_metadata(entry.path()).map_err(|e| format!("{}: {e}", rel.display()))?;
        let kind = meta.file_type();
        if kind.is_symlink() || reparse_point(&meta) {
            return Err(format!("{} is a link, which a pack can't hold", rel.display()));
        }
        let dest = to.join(entry.file_name());
        if kind.is_dir() {
            copy(&entry.path(), &dest, &rel, budget)?;
        } else if kind.is_file() {
            budget.bytes += meta.len();
            if budget.bytes > MAX_BYTES {
                return Err(format!("the folder holds more than {} MiB", MAX_BYTES >> 20));
            }
            fs::copy(entry.path(), &dest).map_err(|e| format!("Couldn't copy {}: {e}", rel.display()))?;
        } else {
            return Err(format!("{} isn't a file or a folder", rel.display()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::bake::bake;
    use crate::manifest::files;

    /// A fresh home with a data folder in it.
    fn home(name: &str) -> (PathBuf, PathBuf) {
        let home = std::env::temp_dir().join(format!("aipet-install-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        let data = home.join("data");
        fs::create_dir_all(&data).unwrap();
        (home, data)
    }

    /// The baked Hood written as a pack folder `at`, named `name` (a style "Mine" of `pet`, when given).
    fn pack(at: &Path, name: &str, pet: Option<&str>) {
        fs::create_dir_all(at).unwrap();
        let mut b = bake(&Avatar::hood());
        b.pack.name = name.to_owned();
        if let Some(pet) = pet {
            b.pack.pet = Some(pet.to_owned());
            b.pack.style = Some("Mine".to_owned());
        }
        for (file, bytes) in files(&b.pack, &b.sheets) {
            fs::write(at.join(file), bytes).unwrap();
        }
    }

    /// Every file under `dir` with its bytes, to compare a folder before and after.
    fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(dir) else {
            return out;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.push((p.clone(), Vec::new()));
                out.extend(snapshot(&p));
            } else {
                out.push((p.clone(), fs::read(&p).unwrap()));
            }
        }
        out.sort();
        out
    }

    fn empty_staging(data: &Path) {
        let left: Vec<_> = fs::read_dir(staging_dir(data))
            .map(|e| e.flatten().collect())
            .unwrap_or_default();
        assert!(left.is_empty(), "staging holds {left:?}");
    }

    #[test]
    fn a_pack_is_installed_and_loads() {
        let (home, data) = home("ok");
        pack(&home.join("src/Fox"), "Fox", None);
        let got = install(&data, &home, &home.join("src/Fox"), false).unwrap();
        assert_eq!(got.names, ["Fox"]);
        assert!(!got.replaced);
        assert!(got.report.starts_with("OK Fox"), "{}", got.report);
        let found = load::all(&Avatar::custom_dir(&data));
        assert!(found.pets.iter().any(|p| p.name() == "Fox"), "{:?}", found.skipped);
        assert!(found.skipped.is_empty(), "{:?}", found.skipped);
        empty_staging(&data);
    }

    #[test]
    fn the_source_must_be_a_folder_inside_home_with_no_links_and_within_the_limits() {
        let (home, data) = home("refused");
        let avatars = Avatar::custom_dir(&data);
        pack(&avatars.join("Kept"), "Kept", None);
        let before = snapshot(&avatars);
        let err = |source: &Path| install(&data, &home, source, false).unwrap_err();
        assert!(err(&home.join("nothing")).contains("isn't there"));
        fs::write(home.join("file"), "x").unwrap();
        assert!(err(&home.join("file")).contains("isn't a folder"));
        let outside = std::env::temp_dir().join(format!("aipet-install-{}-outside", std::process::id()));
        pack(&outside, "Out", None);
        assert!(err(&outside).contains("inside your home"));
        assert!(err(&home).contains("inside your home"));
        // a link inside
        pack(&home.join("linked"), "Linked", None);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/hostname", home.join("linked/x")).unwrap();
            assert!(err(&home.join("linked")).contains("is a link"));
            std::os::unix::fs::symlink(home.join("src"), home.join("alias")).unwrap();
            assert!(err(&home.join("alias")).contains("is a link"));
        }
        // too big, or too many: files the pack doesn't name
        pack(&home.join("big"), "Big", None);
        fs::File::create(home.join("big/junk.bin"))
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        assert!(err(&home.join("big")).contains("MiB"));
        pack(&home.join("many"), "Many", None);
        for i in 0..MAX_ENTRIES {
            fs::write(home.join(format!("many/{i}.txt")), "").unwrap();
        }
        assert!(err(&home.join("many")).contains("more than 2000"));
        // a failed check, with its report
        fs::create_dir_all(home.join("broken")).unwrap();
        fs::write(home.join("broken/pack.json"), "{").unwrap();
        assert!(err(&home.join("broken")).contains("check failed"));
        assert_eq!(snapshot(&avatars), before);
        empty_staging(&data);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn an_existing_name_or_pet_and_style_needs_replace_and_a_built_in_never_goes() {
        let (home, data) = home("clash");
        let avatars = Avatar::custom_dir(&data);
        pack(&avatars.join("fox"), "Fox", None);
        let before = snapshot(&avatars);
        pack(&home.join("Fox"), "Fox", None);
        let e = install(&data, &home, &home.join("Fox"), false).unwrap_err();
        assert!(e.contains("replace"), "{e}");
        // a built-in's pet and style
        pack(&home.join("hood2"), "Hood2", Some("Hood"));
        let mut json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(home.join("hood2/pack.json")).unwrap()).unwrap();
        json["style"] = serde_json::Value::Null;
        json.as_object_mut().unwrap().remove("style");
        fs::write(home.join("hood2/pack.json"), json.to_string()).unwrap();
        let e = install(&data, &home, &home.join("hood2"), true).unwrap_err();
        assert!(e.contains("built in"), "{e}");
        assert_eq!(snapshot(&avatars), before);
        empty_staging(&data);
    }

    #[test]
    fn a_replacement_swaps_the_styles_own_folder_whatever_it_is_named() {
        let (home, data) = home("replace");
        let avatars = Avatar::custom_dir(&data);
        pack(&avatars.join("my-fox"), "Fox", None);
        fs::write(avatars.join("my-fox/old.txt"), "old").unwrap();
        pack(&home.join("Fox"), "Fox", None);
        let got = install(&data, &home, &home.join("Fox"), true).unwrap();
        assert!(got.replaced);
        assert_eq!(got.folder, avatars.join("my-fox"));
        assert!(!avatars.join("my-fox/old.txt").exists());
        assert!(!avatars.join("Fox").exists());
        let found = load::all(&avatars);
        assert!(found.skipped.is_empty(), "{:?}", found.skipped);
        assert_eq!(found.places["Fox"], avatars.join("my-fox"));
        empty_staging(&data);
    }

    #[test]
    fn a_replacement_whose_last_rename_fails_puts_the_old_pack_back() {
        let (home, data) = home("restore");
        let avatars = Avatar::custom_dir(&data);
        pack(&avatars.join("Fox"), "Fox", None);
        fs::write(avatars.join("Fox/old.txt"), "old").unwrap();
        let before = snapshot(&avatars);
        pack(&home.join("Fox"), "Fox", None);
        let calls = Cell::new(0);
        let rename = |from: &Path, to: &Path| {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                Err(io::Error::other("the disk said no"))
            } else {
                fs::rename(from, to)
            }
        };
        let e = install_with(&data, &home, &home.join("Fox"), true, &rename).unwrap_err();
        assert!(e.contains("the old one stays"), "{e}");
        assert_eq!(snapshot(&avatars), before);
        empty_staging(&data);
    }

    #[test]
    fn a_pet_folder_installs_whole_and_replaces_its_pet_folder() {
        let (home, data) = home("pet-folder");
        let avatars = Avatar::custom_dir(&data);
        let src = home.join("Otter");
        pack(&src.join("pixel"), "Otter", None);
        pack(&src.join("other"), "Otter (other)", Some("Otter"));
        let got = install(&data, &home, &src, false).unwrap();
        assert_eq!(got.folder, avatars.join("Otter"));
        assert_eq!(got.names.len(), 2);
        let got = install(&data, &home, &src, true).unwrap();
        assert!(got.replaced);
        assert_eq!(got.folder, avatars.join("Otter"));
        assert!(load::all(&avatars).skipped.is_empty());
        empty_staging(&data);
    }
}
