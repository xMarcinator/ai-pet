//! Where the pet's window goes (MainWindow's PlaceWindow, SaveConfig and ResetPosition,
//! `src/AiPet.UI/MainWindow.axaml.cs:160-201`): the place `config.json` keeps, read at start, kept after a drop and
//! after Reset position, and written as `config.json`'s rules say. The shells place the window from it, each through
//! its own screens or outputs ([`Place`] is the whole window's top-left corner in desktop pixels).
//!
//! When it is written:
//! - a drop writes the new place at once (MainWindow's Sprite_Released saves the config);
//! - Reset position forgets the place, and the shell then saves its default corner, which writes it; a shell that
//!   can't tell where that corner is leaves the place forgotten, which still puts the pet there next time;
//! - the quit writes what is still unwritten (a forgotten place, or a write that failed);
//! - starting and quitting without a change write nothing (`config`).

use aipet_ui::Place;

use crate::config::Store;

/// The height (DIPs, margins included) of the toolbar row that used to sit under the pet (MainWindow.OldToolbarRow).
const OLD_TOOLBAR_ROW: f64 = 42.0;

/// Where the pet was left, read at start: `Left` and `Top`, with the height of the window it was saved with. A place
/// saved with the old toolbar showing had the pet a toolbar row higher, as if the window were shorter
/// (PlaceWindow's `savedHeight`). None, which puts the pet in its default corner, unless both `Left` and `Top` are
/// there.
pub fn load(config: &Store) -> Option<Place> {
    let config = config.get();
    let toolbar = if config.toolbar == Some(true) {
        OLD_TOOLBAR_ROW
    } else {
        0.0
    };
    Some(Place {
        left: config.left?,
        top: config.top?,
        window_height: config.window_height - toolbar,
    })
}

/// The window's place after a drop, or after Reset position put it back in its corner: kept as SaveConfig keeps it
/// (the old toolbar's flag goes), and written now.
pub fn save(config: &mut Store, place: Place) {
    config.keep(|config| {
        config.left = Some(place.left);
        config.top = Some(place.top);
        config.window_height = place.window_height;
        config.toolbar = None;
    });
    write(config);
}

/// Reset position was chosen: the place is forgotten, so the pet starts in its default corner, and the shell, which
/// puts the window back there, saves where that is ([`save`]). It is written with that, or at the quit.
pub fn reset(config: &mut Store) {
    config.keep(|config| {
        config.left = None;
        config.top = None;
        config.toolbar = None;
    });
}

/// The pet quits: a place not written yet is written now.
pub fn quit(config: &mut Store) {
    write(config);
}

fn write(config: &mut Store) {
    if let Err(e) = config.flush() {
        aipet_core::log::write(&format!("config: can't write the pet's place to config.json: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use aipet_core::config::Config;

    use super::*;
    use crate::config::tests::Scratch;

    /// The app's start and quit: the store read, the place read, then the quit's writes (`App::quit`).
    fn start_and_quit(scratch: &Scratch) {
        let mut config = Store::load(scratch.file());
        let _ = load(&config);
        quit(&mut config);
        config.flush().unwrap();
    }

    #[test]
    fn starting_and_quitting_leave_config_json_as_it_is_until_a_setting_changes() {
        // a file the pet would write otherwise (indented, with a field it drops): any write would show
        let valid = "{\n  \"Left\": 1500,\n  \"Top\": 480,\n  \"Pills\": false,\n  \"Extra\": 1\n}";
        for (case, file) in [
            ("missing", None),
            ("corrupt", Some("{\"Left\": 15")),
            ("valid", Some(valid)),
        ] {
            let scratch = Scratch::new(&format!("start-quit-{case}"));
            if let Some(file) = file {
                std::fs::write(scratch.file(), file).unwrap();
            }
            start_and_quit(&scratch);
            start_and_quit(&scratch);
            match file {
                Some(file) => assert_eq!(std::fs::read_to_string(scratch.file()).unwrap(), file, "{case}"),
                None => assert!(!scratch.file().exists(), "{case}"),
            }
            // one setting changed: written at once
            let mut config = Store::load(scratch.file());
            let music = !config.get().music;
            config.change(|c| c.music = music).unwrap();
            assert_eq!(Config::load_from(&scratch.file()).0.music, music, "{case}");
        }
    }

    #[test]
    fn the_place_is_read_as_the_csharp_reads_it_old_toolbar_and_all() {
        let place = |config: Config| {
            let scratch = Scratch::new("read");
            config.save_to(&scratch.file()).unwrap();
            load(&Store::load(scratch.file()))
        };
        let at = |toolbar, window_height| Config {
            left: Some(1500),
            top: Some(-20),
            toolbar,
            window_height,
            ..Config::default()
        };
        let saved = |window_height| {
            Some(Place {
                left: 1500,
                top: -20,
                window_height,
            })
        };
        assert_eq!(place(at(None, 600.0)), saved(600.0));
        assert_eq!(place(at(Some(false), 420.0)), saved(420.0));
        // saved with the toolbar showing: as if the window were a toolbar row (42) shorter
        assert_eq!(place(at(Some(true), 420.0)), saved(378.0));
        // nothing saved, or half a place: the default corner
        assert_eq!(place(Config::default()), None);
        assert_eq!(
            place(Config {
                top: None,
                ..at(None, 600.0)
            }),
            None
        );
    }

    #[test]
    fn a_drop_is_written_at_once_and_a_reset_forgets_the_place_until_its_corner_is_saved() {
        let scratch = Scratch::new("drop-reset");
        let written = || Config::load_from(&scratch.file()).0;
        std::fs::write(
            scratch.file(),
            r#"{"Left":10,"Top":20,"Toolbar":true,"WindowHeight":420}"#,
        )
        .unwrap();
        let mut config = Store::load(scratch.file());
        let corner = Place {
            left: 1516,
            top: 480,
            window_height: 600.0,
        };
        save(&mut config, corner);
        assert_eq!(
            (
                written().left,
                written().top,
                written().toolbar,
                written().window_height
            ),
            (Some(1516), Some(480), None, 600.0)
        );
        // a reset whose corner the shell saves: written with it
        reset(&mut config);
        assert_eq!(written().left, Some(1516), "not written before its corner");
        save(&mut config, Place { left: 1200, ..corner });
        assert_eq!(written().left, Some(1200));
        // a reset whose corner the shell can't tell: the place is forgotten at the quit
        reset(&mut config);
        quit(&mut config);
        assert_eq!((written().left, written().top), (None, None));
        assert_eq!(load(&Store::load(scratch.file())), None);
    }
}
