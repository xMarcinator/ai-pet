//! The avatars the picker offers, with their sheets kept compressed.
//!
//! Every pet's sheets decoded come to about 7 MB for the built-ins alone (5 MB of it the painted Pip), yet only the
//! worn pet is drawn, and the [`Pet`](aipet_sprite::pet::Pet) holds its own copy of that one. So each avatar here
//! keeps its pack as it is and its sheets deflated, and [`Shelved::unpacked`] decodes them again when a pet is put on,
//! a theme previewed or the folder read again. Pixel art deflates to a few percent of its size.
//!
//! Each also keeps how high its effects reach in each theme ([`Shelved::fx_reach`]), which the bubbles keep clear
//! of. A vector pet's takes tens of ms to find (its effects are drawn through 3 s of their animations), which held
//! the pet still when it was put on, so [`shelve`] finds every vector pet's off the UI thread.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::ops::Deref;
use std::sync::{Arc, Mutex, PoisonError};

use aipet_sprite::bake::Sheet;
use aipet_sprite::load::LoadedPack;
use aipet_sprite::pack::Kind;
use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;

/// An avatar: its pack, read through [`Deref`], with no sheets, and its sheets deflated.
#[derive(Clone, Debug)]
pub struct Shelved {
    pet: LoadedPack,
    sheets: Vec<Packed>,
    reaches: Reaches,
}

/// How high a pet's effects reach, by theme, once found.
type Reaches = Arc<Mutex<HashMap<String, f32>>>;

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
        Shelved {
            pet,
            sheets,
            reaches: Arc::default(),
        }
    }

    /// How high its effects reach in `theme` ([`aipet_sprite::fx_reach`]), with `worn` the pet in that theme: found
    /// once, here or by [`shelve`]'s thread.
    pub fn fx_reach(&self, theme: &str, worn: &LoadedPack) -> f32 {
        let found = self
            .reaches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(theme)
            .copied();
        found.unwrap_or_else(|| {
            let reach = aipet_sprite::fx_reach(&worn.pack, &worn.sheets) as f32;
            self.reaches
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(theme.to_owned(), reach);
            reach
        })
    }

    /// The pet with its sheets decoded.
    pub fn unpacked(&self) -> LoadedPack {
        LoadedPack {
            pack: self.pet.pack.clone(),
            sheets: self.sheets.iter().map(unpack).collect(),
            builtin: self.pet.builtin,
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

/// Shelves each of `pets`, and finds how high each vector pet's effects reach in each of its themes on a thread of
/// its own. One put on before its thread got to it finds its own.
pub fn shelve(pets: Vec<LoadedPack>) -> Vec<Shelved> {
    let shelved: Vec<Shelved> = pets.into_iter().map(Shelved::new).collect();
    let vectors: Vec<(LoadedPack, Reaches)> = shelved
        .iter()
        .filter(|s| s.pack.kind == Kind::Vector)
        .map(|s| (s.pet.clone(), Arc::clone(&s.reaches)))
        .collect();
    if !vectors.is_empty() {
        let found = std::thread::Builder::new().name("aipet-reach".into()).spawn(move || {
            for (pet, reaches) in vectors {
                for theme in pet.pack.palette.names() {
                    let known = reaches
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .contains_key(theme);
                    if known {
                        continue;
                    }
                    let Some(themed) = pet.in_theme(theme) else {
                        continue;
                    };
                    let reach = aipet_sprite::fx_reach(&themed.pack, &themed.sheets) as f32;
                    reaches
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(theme.to_owned(), reach);
                }
            }
        });
        // without a thread, each is found when it is put on, as before
        drop(found);
    }
    shelved
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

    /// The reach [`shelve`]'s thread finds for each vector pet's themes is the one wearing it would find, and a pet
    /// worn before the thread got to it finds its own.
    #[test]
    fn the_effects_reach_found_off_the_ui_thread_is_the_one_found_when_worn() {
        let pets = aipet_sprite::load::all(std::path::Path::new("/nonexistent")).pets;
        let shelved = shelve(pets.clone());
        let vectors: Vec<usize> = (0..pets.len()).filter(|&i| pets[i].pack.kind == Kind::Vector).collect();
        assert!(!vectors.is_empty());
        let started = std::time::Instant::now();
        for &i in &vectors {
            for theme in pets[i].pack.palette.names() {
                let worn = pets[i].in_theme(theme).unwrap();
                let direct = aipet_sprite::fx_reach(&worn.pack, &worn.sheets) as f32;
                // the thread's, once it has it
                while !shelved[i].reaches.lock().unwrap().contains_key(theme) {
                    assert!(
                        started.elapsed().as_secs() < 60,
                        "the thread never found {} in {theme}",
                        pets[i].name()
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                assert_eq!(
                    shelved[i].fx_reach(theme, &worn),
                    direct,
                    "{} in {theme}",
                    pets[i].name()
                );
            }
        }
        // a pixel pet's is found when it is worn, and kept
        let pixel = pets.iter().position(|p| p.pack.kind == Kind::Pixel).unwrap();
        let alone = Shelved::new(pets[pixel].clone());
        let theme = pets[pixel].pack.palette.theme.clone();
        let reach = alone.fx_reach(&theme, &pets[pixel]);
        assert_eq!(
            reach,
            aipet_sprite::fx_reach(&pets[pixel].pack, &pets[pixel].sheets) as f32
        );
        assert_eq!(alone.reaches.lock().unwrap().get(&theme), Some(&reach));
    }

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
