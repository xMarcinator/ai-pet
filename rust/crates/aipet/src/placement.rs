//! Where the pet's window goes (MainWindow's PlaceWindow, SaveConfig and ResetPosition,
//! `src/AiPet.UI/MainWindow.axaml.cs:160-201`): the place `config.json` keeps, read at start, kept after a drop and
//! after Reset position, and written as `config.json`'s rules say. The shells place the window from it.
//!
//! Still to come: nothing is read or kept yet, so the pet always starts in its default corner.

use aipet_ui::Place;

use crate::config::Store;

/// Where the pet was left, read at start; none puts it in its default corner.
pub fn load(_config: &Store) -> Option<Place> {
    None
}

/// The window's place after a drop, or after Reset position put it back in its corner.
pub fn save(_config: &mut Store, _place: Place) {}

/// Reset position was chosen: the shell puts the window back in its corner, and saves that place.
pub fn reset(_config: &mut Store) {}

/// The pet quits: a place not written yet is written now.
pub fn quit(_config: &mut Store) {}
