//! The built-in pets' packs, embedded in the binary, one per drawing: Sprout and Hood (in Blue, with a Green theme
//! from `avatars/hood-green.json`), baked ahead of time from their avatars (`tests/bake.rs` checks the committed
//! files are the bake's: the manifests as text, the sheets pixel for pixel); the vector Pip
//! (`avatars/reference/pip_gen.py` writes it, in Peach with a Lilac theme) with its painting, whose Lilac theme is
//! sheets (`tests/bake.rs` again); and the vector Hood (in
//! Green, with a Blue theme) and Sprout, which their reference generators write (`tests/vector_packs.rs` checks they
//! still do).

use std::collections::BTreeMap;

use crate::avatar::Avatar;
use crate::bake::Sheet;
use crate::load::LoadedPack;
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
            "feet-run.png", "headphones.png", "prop.png", "eyes.png", "screen.png", "dust.png", "fx.png", "sparkles.png",
            "aura-glow.png", "tears.png", "heart.png", "aura.png")
    };
}

const SPROUT: Files = baked!("sprout");
const HOOD: Files = baked!("hood");
const PIP: Files = pack!("pip": "pack.json");
const PIP_PAINTED: Files = pack!("pip-painted": "pack.json", "feet.png", "sprout.png", "body.png", "mouth.png", "eyes.png",
    "headphones.png", "prop.png", "arms.png", "screen.png", "zs.png", "notes.png", "dots.png", "alert.png",
    "sparkles.png", "dust.png", "feet-lilac.png", "body-lilac.png", "arms-lilac.png", "aura-glow.png", "tears.png",
    "heart.png", "aura.png");
const HOOD_VECTOR: Files = pack!("hood-vector": "pack.json");
const SPROUT_VECTOR: Files = pack!("sprout-vector": "pack.json");

/// The built-in pets, in order.
pub fn all() -> Vec<LoadedPack> {
    [SPROUT, HOOD, PIP, PIP_PAINTED, HOOD_VECTOR, SPROUT_VECTOR]
        .into_iter()
        .map(|files| {
            let (pack, sheets) = load(files).expect("the embedded packs load (tests/bake.rs)");
            LoadedPack {
                pack,
                sheets,
                builtin: true,
            }
        })
        .collect()
}

/// The embedded pack for `avatar`, if it is one of the built-in pixel pets exactly (a file that renames or recolours
/// one, such as `avatars/hood-green.json`, is baked at load instead).
pub fn pack_for(avatar: &Avatar) -> Option<(Pack, Vec<Sheet>)> {
    let files = if *avatar == Avatar::sprout() {
        SPROUT
    } else if *avatar == Avatar::hood() {
        HOOD
    } else {
        return None;
    };
    load(files).ok()
}

fn load(files: Files) -> Result<(Pack, Vec<Sheet>), String> {
    let by_name: BTreeMap<&str, &[u8]> = files.iter().copied().collect();
    let manifest = std::str::from_utf8(by_name.get("pack.json").ok_or("no pack.json")?).map_err(|e| e.to_string())?;
    read(manifest, |name| decode_png(by_name.get(name).ok_or("missing")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bake::{bake, built_in, hood_green};
    use crate::load::LoadedPack;

    #[test]
    fn each_built_in_pet_loads_its_embedded_pack_which_is_its_bake() {
        let baked = built_in();
        for (avatar, (_, baked)) in [Avatar::sprout(), Avatar::hood()].iter().zip(baked) {
            let (pack, sheets) = pack_for(avatar).unwrap_or_else(|| panic!("{} has no pack", avatar.name));
            assert_eq!((pack, sheets), (baked.pack, baked.sheets), "{}", avatar.name);
        }
        let mut renamed = Avatar::sprout();
        renamed.name = "Sprig".into();
        assert!(pack_for(&renamed).is_none());
        assert!(
            pack_for(&hood_green()).is_none(),
            "the green Hood is a theme of the Hood now"
        );
    }

    #[test]
    fn the_pixel_hood_in_green_is_the_green_hood_pixel_for_pixel() {
        let (pack, sheets) = pack_for(&Avatar::hood()).unwrap();
        let hood = LoadedPack {
            pack,
            sheets,
            builtin: true,
        };
        assert_eq!(hood.pack.palette.names().collect::<Vec<_>>(), ["Blue", "Green"]);
        let green = hood.in_theme("Green").expect("the Hood has a Green theme");
        let old = bake(&hood_green());
        assert_eq!(green.sheets.len(), old.sheets.len());
        for (i, (a, b)) in green.sheets.iter().zip(&old.sheets).enumerate() {
            assert!(a == b, "sheet {i} ({}) differs", hood.pack.layers[i].name);
        }
        assert_eq!((green.pack.accent, green.pack.glow), (old.pack.accent, old.pack.glow));
        // the effects that don't use the avatar's colours are as they were
        assert_eq!(hood.in_theme("Blue"), Some(hood.clone()));
        assert_eq!(hood.in_theme("Purple"), None);
    }
}
