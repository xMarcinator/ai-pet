//! The built-in pets' packs, embedded in the binary: Sprout, Hood and Hood (green), baked ahead of time from their
//! avatars (`tests/bake.rs` checks the committed files are the bake's, byte for byte).

use std::collections::BTreeMap;

use crate::avatar::Avatar;
use crate::bake::Sheet;
use crate::manifest::{decode_png, read};
use crate::pack::Pack;

/// A pack folder's files, by name.
type Files = &'static [(&'static str, &'static [u8])];

macro_rules! pack {
    ($dir:literal: $($file:literal),* $(,)?) => {
        &[$(($file, include_bytes!(concat!("../packs/", $dir, "/", $file)))),*]
    };
}

macro_rules! baked {
    ($dir:literal) => {
        pack!($dir: "pack.json", "body.png", "headphones-outline.png", "arms.png", "arms-run.png", "feet.png",
            "feet-run.png", "headphones.png", "prop.png", "eyes.png", "screen.png", "dust.png", "fx.png", "sparkles.png")
    };
}

const SPROUT: Files = baked!("sprout");
const HOOD: Files = baked!("hood");
const HOOD_GREEN: Files = baked!("hood-green");

/// The green Hood's avatar, as the repo's `avatars/hood-green.json` gives it.
fn hood_green() -> Option<Avatar> {
    Avatar::parse(include_str!("../../../../avatars/hood-green.json"), "hood-green")
}

/// The embedded pack for `avatar`, if it is one of the built-in pets exactly (a file that renames or recolours one
/// is baked at load instead).
pub fn pack_for(avatar: &Avatar) -> Option<(Pack, Vec<Sheet>)> {
    let files = if *avatar == Avatar::sprout() {
        SPROUT
    } else if *avatar == Avatar::hood() {
        HOOD
    } else if hood_green().as_ref() == Some(avatar) {
        HOOD_GREEN
    } else {
        return None;
    };
    load(files).ok()
}

fn load(files: Files) -> Result<(Pack, Vec<Sheet>), String> {
    let mut manifest = None;
    let mut sheets = BTreeMap::new();
    for (name, bytes) in files {
        if *name == "pack.json" {
            manifest = Some(std::str::from_utf8(bytes).map_err(|e| e.to_string())?);
        } else {
            sheets.insert((*name).to_owned(), decode_png(bytes)?);
        }
    }
    read(manifest.ok_or("no pack.json")?, &sheets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bake::bake;

    #[test]
    fn each_built_in_pet_loads_its_embedded_pack_which_is_its_bake() {
        for avatar in [Avatar::sprout(), Avatar::hood(), hood_green().unwrap()] {
            let (pack, sheets) = pack_for(&avatar).unwrap_or_else(|| panic!("{} has no pack", avatar.name));
            let baked = bake(&avatar);
            assert_eq!((pack, sheets), (baked.pack, baked.sheets), "{}", avatar.name);
        }
        let mut renamed = Avatar::sprout();
        renamed.name = "Sprig".into();
        assert!(pack_for(&renamed).is_none());
    }
}
