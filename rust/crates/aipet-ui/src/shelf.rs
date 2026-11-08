//! The avatars the picker offers, with their sheets kept compressed.
//!
//! Every pet's sheets decoded come to about 7 MB for the built-ins alone (5 MB of it the painted Pip), yet only the
//! worn pet is drawn, and the [`Pet`](aipet_sprite::pet::Pet) holds its own copy of that one. So each avatar here
//! keeps its pack as it is and its sheets deflated, and [`Shelved::unpacked`] decodes them again when a pet is put on,
//! a theme previewed or the folder read again. Pixel art deflates to a few percent of its size.

use std::io::{Read, Write};
use std::ops::Deref;

use aipet_sprite::bake::Sheet;
use aipet_sprite::load::LoadedPack;
use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;

/// An avatar: its pack, read through [`Deref`], with no sheets, and its sheets deflated.
#[derive(Clone, Debug)]
pub struct Shelved {
    pet: LoadedPack,
    sheets: Vec<Packed>,
}

/// A sheet's size and its pixels deflated, little-endian.
#[derive(Clone, Debug)]
struct Packed {
    width: u32,
    height: u32,
    deflated: Vec<u8>,
}

impl Shelved {
    pub fn new(mut pet: LoadedPack) -> Shelved {
        let sheets = std::mem::take(&mut pet.sheets).iter().map(pack).collect();
        Shelved { pet, sheets }
    }

    /// The pet with its sheets decoded.
    pub fn unpacked(&self) -> LoadedPack {
        LoadedPack {
            pack: self.pet.pack.clone(),
            sheets: self.sheets.iter().map(unpack).collect(),
        }
    }
}

impl Deref for Shelved {
    type Target = LoadedPack;

    /// The pet's pack, its name and palette; its `sheets` are empty (see [`Shelved::unpacked`]).
    fn deref(&self) -> &LoadedPack {
        &self.pet
    }
}

/// Shelves each of `pets`.
pub fn shelve(pets: Vec<LoadedPack>) -> Vec<Shelved> {
    pets.into_iter().map(Shelved::new).collect()
}

fn pack(sheet: &Sheet) -> Packed {
    // the fastest level: a pet's sheets are mostly transparent, and read at start
    let mut out = DeflateEncoder::new(Vec::new(), Compression::fast());
    for px in &sheet.pixels {
        out.write_all(&px.to_le_bytes()).expect("writing to memory");
    }
    Packed {
        width: sheet.width,
        height: sheet.height,
        deflated: out.finish().expect("writing to memory"),
    }
}

fn unpack(p: &Packed) -> Sheet {
    let mut bytes = Vec::with_capacity(p.width as usize * p.height as usize * 4);
    DeflateDecoder::new(&p.deflated[..])
        .read_to_end(&mut bytes)
        .expect("what pack deflated inflates");
    Sheet {
        width: p.width,
        height: p.height,
        pixels: bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&b| u32::from_le_bytes(b))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_pet_unpacks_as_it_was_and_smaller() {
        let pets = aipet_sprite::load::all(std::path::Path::new("/nonexistent")).pets;
        let (mut decoded, mut deflated) = (0, 0);
        for pet in pets {
            let shelved = Shelved::new(pet.clone());
            assert!(shelved.sheets.is_empty() == pet.sheets.is_empty() && shelved.pet.sheets.is_empty());
            assert_eq!(shelved.unpacked(), pet, "{}", pet.name());
            assert_eq!(shelved.name(), pet.name());
            decoded += pet.sheets.iter().map(|s| s.pixels.len() * 4).sum::<usize>();
            deflated += shelved.sheets.iter().map(|s| s.deflated.len()).sum::<usize>();
        }
        assert!(deflated * 5 < decoded, "{deflated} bytes deflated of {decoded}");
    }
}
