//! The user's theme files (fnt-71): naming, saving and deleting them, for Settings' Avatars page and the pet's MCP
//! tools alike. A theme file lives in the avatars folder's `themes` folder ([`THEMES`]) and holds the colours of a
//! drawing's palette that differ from its default. A deleted one moves to `<data>/trash/`, outside the avatars folder,
//! so the loader never reads it again.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Avatar;
use crate::load::{DEVICES, THEMES};
use crate::pack::{Pack, Theme};
use crate::vector::colour_name;

/// The most characters a theme's name has.
pub const MAX_NAME: usize = 40;

/// Where deleted themes go: `<data>/trash`.
pub fn trash_dir(data: &Path) -> PathBuf {
    data.join("trash")
}

/// `name` trimmed, when it is a theme's name: letters, digits, spaces, `-` and `_`, at most [`MAX_NAME`] characters,
/// and not a name Windows keeps for a device. Else why not.
pub fn check_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the theme a name.".to_owned());
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!("A theme's name has at most {MAX_NAME} characters."));
    }
    if let Some(c) = name
        .chars()
        .find(|&c| !(c.is_alphanumeric() || c == ' ' || c == '-' || c == '_'))
    {
        return Err(format!(
            "A theme's name has only letters, digits, spaces, - and _, not {c:?}."
        ));
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    if DEVICES.contains(&stem.as_str()) {
        return Err(format!("{name} is a name Windows keeps for a device."));
    }
    Ok(name)
}

/// A name as part of a file name: lower case letters and digits, words joined by `-`.
pub fn slug(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// A theme saved: its name and its file in the themes folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub name: String,
    pub file: String,
}

/// Saves a theme of `pack` (a drawing as loaded, with the user's themes on it) named `name`, in `colours` (any of its
/// palette's named colours; the rest are its default's), as a file in the themes folder of `avatars`. `editing` is
/// the user's theme it changes, by its name, which keeps its file (and may take a new name); `None` makes a new theme,
/// in a new file named after the drawing and the theme. Only the colours that differ from the default are written.
pub fn save(
    avatars: &Path,
    pack: &Pack,
    name: &str,
    colours: &BTreeMap<String, u32>,
    editing: Option<&str>,
) -> Result<Saved, String> {
    let name = check_name(name)?;
    let palette = &pack.palette;
    if !palette.has_colours() {
        return Err(format!("{} has no named colours to make a theme of.", pack.name));
    }
    let file = match editing {
        None => None,
        Some(own) if own == palette.theme => {
            return Err(format!(
                "{own} is one of {}'s built-in themes, which can't be changed.",
                pack.name
            ));
        }
        Some(own) => match palette.find(own) {
            None => return Err(format!("{} has no theme named {own}.", pack.name)),
            Some(Theme { file: None, .. }) => {
                return Err(format!(
                    "{own} is one of {}'s built-in themes, which can't be changed.",
                    pack.name
                ));
            }
            Some(Theme { file: Some(file), .. }) => Some(file.clone()),
        },
    };
    let clash = palette
        .names()
        .find(|n| n.to_lowercase() == name.to_lowercase() && Some(*n) != editing);
    if let Some(taken) = clash {
        return Err(format!("{} already has a theme named {taken}.", pack.name));
    }
    let mut changed = BTreeMap::new();
    for (n, &c) in colours {
        let Some(&default) = palette.colours.get(n) else {
            let names: Vec<&str> = palette.colours.keys().map(String::as_str).collect();
            return Err(format!(
                "{} has no colour named {n}: its colours are {}.",
                pack.name,
                names.join(", ")
            ));
        };
        if c != default {
            changed.insert(n.clone(), c);
        }
    }
    // checked as the loader will, beside the other themes: without the one it replaces
    let mut others = palette.clone();
    others.themes.retain(|t| Some(t.name.as_str()) != editing);
    others
        .check(&Theme::of_colours(name, changed.clone()))
        .map_err(|e| format!("{}'s theme {name}: {e}", pack.name))?;
    let folder = avatars.join(THEMES);
    let file = match file {
        Some(file) => file,
        None => {
            let stem = format!("{}-{}", slug(&pack.name), slug(name));
            (1..)
                .map(|i| {
                    if i == 1 {
                        format!("{stem}.json")
                    } else {
                        format!("{stem}-{i}.json")
                    }
                })
                .find(|f| !folder.join(f).exists())
                .expect("a file name is free")
        }
    };
    let palette: serde_json::Map<String, serde_json::Value> = changed
        .iter()
        .map(|(n, &c)| (n.clone(), colour_name(Some(c)).into()))
        .collect();
    let json = serde_json::json!({"pet": pack.name, "theme": name, "palette": palette});
    fs::create_dir_all(&folder).map_err(|e| format!("Couldn't make the themes folder: {e}"))?;
    let path = folder.join(&file);
    let text = serde_json::to_string_pretty(&json).expect("a theme always serialises") + "\n";
    fs::write(&path, text).map_err(|e| format!("Couldn't save {}: {e}", path.display()))?;
    Ok(Saved {
        name: name.to_owned(),
        file,
    })
}

/// Moves the theme file `file` (a file's name in the themes folder of `<data>/avatars`) to `<data>/trash/`, under a
/// name no other file there has. Where it went, or why it couldn't.
pub fn delete(data: &Path, file: &str) -> Result<PathBuf, String> {
    let plain =
        !file.is_empty() && Path::new(file).file_name().is_some_and(|n| n == file) && !file.contains(['/', '\\']);
    if !plain {
        return Err(format!("{file:?} isn't a theme file's name"));
    }
    let path = Avatar::custom_dir(data).join(THEMES).join(file);
    let trash = trash_dir(data);
    fs::create_dir_all(&trash).map_err(|e| format!("Couldn't make the trash folder {}: {e}", trash.display()))?;
    let stem = Path::new(file)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let to = (1..)
        .map(|i| {
            if i == 1 {
                trash.join(format!("{stem}-{at}.json"))
            } else {
                trash.join(format!("{stem}-{at}-{i}.json"))
            }
        })
        .find(|p| !p.exists())
        .expect("a file name is free");
    fs::rename(&path, &to).map_err(|e| format!("Couldn't delete {}: {e}", path.display()))?;
    Ok(to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("aipet-themes-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Sprout's vector drawing as the avatars folder `avatars` loads it, with its themes.
    fn sprout(avatars: &Path) -> Pack {
        load::all(avatars)
            .pets
            .into_iter()
            .find(|p| p.pack.pet() == "Sprout" && p.pack.style() == "Vector")
            .expect("Sprout's vector drawing")
            .pack
    }

    fn some_colour(pack: &Pack) -> (String, u32) {
        let (n, &c) = pack.palette.colours.iter().next().unwrap();
        (n.clone(), c ^ 0x0012_3456)
    }

    #[test]
    fn names_pass_the_allowlist_or_say_why_not() {
        assert_eq!(check_name("  Night_owl-2 "), Ok("Night_owl-2"));
        assert_eq!(check_name("Ünïcode ok"), Ok("Ünïcode ok"));
        for bad in ["", "   ", "a/b", "..\\x", "dots.json", "CON", "lpt1", &"x".repeat(41)] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
        assert!(check_name(&"x".repeat(40)).is_ok());
    }

    #[test]
    fn a_new_theme_is_written_and_loads_on_its_drawing() {
        let data = dir("new");
        let avatars = Avatar::custom_dir(&data);
        let pack = sprout(&avatars);
        let (n, c) = some_colour(&pack);
        let saved = save(&avatars, &pack, "Dusk", &BTreeMap::from([(n.clone(), c)]), None).unwrap();
        assert_eq!(saved.name, "Dusk");
        assert_eq!(saved.file, format!("{}-dusk.json", slug(&pack.name)));
        let found = load::all(&avatars);
        assert!(found.skipped.is_empty(), "{:?}", found.skipped);
        let pack = sprout(&avatars);
        assert_eq!(pack.palette.in_theme("Dusk").unwrap()[&n], c);
        // a default colour isn't written
        let text = fs::read_to_string(avatars.join(THEMES).join(&saved.file)).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["palette"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn taken_built_in_missing_and_unknown_are_refused_and_write_nothing() {
        let data = dir("refused");
        let avatars = Avatar::custom_dir(&data);
        let pack = sprout(&avatars);
        let builtin = pack.palette.theme.clone();
        let none = BTreeMap::new();
        // a new theme over a name taken, in any case
        let e = save(&avatars, &pack, &builtin.to_uppercase(), &none, None).unwrap_err();
        assert!(e.contains("already has"), "{e}");
        // a built-in theme
        let e = save(&avatars, &pack, &builtin, &none, Some(&builtin)).unwrap_err();
        assert!(e.contains("built-in"), "{e}");
        // an edit of a missing theme
        let e = save(&avatars, &pack, "Gone", &none, Some("Gone")).unwrap_err();
        assert!(e.contains("no theme named Gone"), "{e}");
        // a colour the pet hasn't
        let e = save(
            &avatars,
            &pack,
            "Odd",
            &BTreeMap::from([("nope".to_owned(), 0xFF00_0000)]),
            None,
        )
        .unwrap_err();
        assert!(e.contains("no colour named nope"), "{e}");
        let e = save(&avatars, &pack, "a/b", &none, None).unwrap_err();
        assert!(e.contains("only letters"), "{e}");
        assert!(!avatars.join(THEMES).exists());
    }

    #[test]
    fn an_edit_keeps_its_file_and_may_rename() {
        let data = dir("edit");
        let avatars = Avatar::custom_dir(&data);
        let pack = sprout(&avatars);
        let (n, c) = some_colour(&pack);
        let first = save(&avatars, &pack, "Dusk", &BTreeMap::from([(n.clone(), c)]), None).unwrap();
        let pack = sprout(&avatars);
        let again = save(&avatars, &pack, "Dawn", &BTreeMap::new(), Some("Dusk")).unwrap();
        assert_eq!(again.file, first.file);
        let pack = sprout(&avatars);
        assert!(pack.palette.find("Dawn").is_some() && pack.palette.find("Dusk").is_none());
    }

    #[test]
    fn a_deleted_theme_moves_to_the_trash_outside_the_avatars_folder() {
        let data = dir("delete");
        let avatars = Avatar::custom_dir(&data);
        let pack = sprout(&avatars);
        let saved = save(&avatars, &pack, "Dusk", &BTreeMap::new(), None).unwrap();
        let to = delete(&data, &saved.file).unwrap();
        assert!(to.starts_with(trash_dir(&data)) && to.is_file());
        assert!(!avatars.join(THEMES).join(&saved.file).exists());
        assert!(sprout(&avatars).palette.find("Dusk").is_none());
        assert!(load::all(&avatars).skipped.is_empty());
        // a second delete of the same name lands beside the first
        let saved = save(&avatars, &sprout(&avatars), "Dusk", &BTreeMap::new(), None).unwrap();
        let second = delete(&data, &saved.file).unwrap();
        assert_ne!(second, to);
        for bad in ["", "../x.json", "a/b.json", ".."] {
            assert!(delete(&data, bad).is_err(), "{bad:?}");
        }
    }
}
