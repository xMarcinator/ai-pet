//! Agents' commands (fnt-71, [`aipet_core::control`]): each is checked whole first, then done through the messages
//! Settings sends, so the pet changes, saves `config.json` and shows the change in an open Settings exactly as it
//! does for the user. A refused command changes nothing.

use std::collections::BTreeMap;

use aipet_core::control::{
    Answer, Avatar, Card, Command, Settings, SettingsChange, Skipped, Style, ThemeColours, Worn,
};
use aipet_sprite::vector::colour_name;

use crate::{Effect, Message, PetUi, geometry, pets};

/// Does `command` on `pet`: its answer, or why it was refused, and what the shell must do (keeping the pet on top).
pub(crate) fn run(pet: &mut PetUi, command: Command) -> (Result<Answer, String>, Option<Effect>) {
    match command {
        Command::ListAvatars => (Ok(avatars(pet)), None),
        Command::Wear { name, style, theme } => (wear(pet, &name, style.as_deref(), theme.as_deref()), None),
        Command::GetSettings => (Ok(Answer::Settings(settings(pet))), None),
        Command::SetSettings(change) => match check(&change) {
            Ok(()) => {
                let effect = set(pet, change);
                (Ok(Answer::Settings(settings(pet))), effect)
            }
            Err(e) => (Err(e), None),
        },
        Command::GetBoard => (Ok(Answer::Board(cards(pet))), None),
        Command::Dismiss(id) => (dismiss(pet, id), None),
        Command::ListThemes { pet: name, style } => (list_themes(pet, &name, style.as_deref()), None),
        Command::SaveTheme {
            pet: name,
            style,
            name: theme,
            palette,
            create,
        } => (save_theme(pet, &name, style.as_deref(), &theme, &palette, create), None),
        Command::DeleteTheme {
            pet: name,
            style,
            name: theme,
        } => (delete_theme(pet, &name, style.as_deref(), &theme), None),
        Command::Reload => {
            reload(pet);
            (Ok(avatars(pet)), None)
        }
    }
}

fn worn(pet: &PetUi) -> Worn {
    let pack = &pet.avatars[pet.avatar].pack;
    Worn {
        name: pack.pet().to_owned(),
        style: pack.style().to_owned(),
        theme: pet.theme.clone(),
    }
}

fn avatars(pet: &PetUi) -> Answer {
    let avatars = pets(&pet.avatars)
        .into_iter()
        .map(|g| Avatar {
            name: g.pet,
            styles: g
                .styles
                .iter()
                .map(|&i| {
                    let pack = &pet.avatars[i].pack;
                    Style {
                        style: pack.style().to_owned(),
                        kind: pack.kind.label().to_lowercase(),
                        // its default first
                        themes: pack.palette.names().map(str::to_owned).collect(),
                    }
                })
                .collect(),
        })
        .collect();
    let skipped = pet
        .skipped
        .iter()
        .map(|s| Skipped {
            path: s.file.clone(),
            reason: s.reason.clone(),
        })
        .collect();
    Answer::Avatars {
        worn: worn(pet),
        avatars,
        skipped,
    }
}

/// "a, b and c".
fn listed(names: &[&str]) -> String {
    match names {
        [] => "none".to_owned(),
        [one] => format!("\"{one}\""),
        [rest @ .., last] => {
            let rest: Vec<String> = rest.iter().map(|n| format!("\"{n}\"")).collect();
            format!("{} and \"{last}\"", rest.join(", "))
        }
    }
}

/// Wears the pet `name` in `style` and `theme`, else its default style and that style's default theme: as Settings'
/// picker does, saving the choice. Names match in any case.
fn wear(pet: &mut PetUi, name: &str, style: Option<&str>, theme: Option<&str>) -> Result<Answer, String> {
    let (pet_name, i) = drawing(pet, name, style)?;
    let palette = &pet.avatars[i].pack.palette;
    let theme = match theme {
        None => palette.theme.clone(),
        Some(theme) => palette
            .names()
            .find(|n| n.eq_ignore_ascii_case(theme))
            .map(str::to_owned)
            .ok_or_else(|| no_theme(pet, &pet_name, i, theme))?,
    };
    if i != pet.avatar || theme != pet.theme {
        pet.wear(i, &theme);
        pet.save_prefs();
    }
    Ok(Answer::Worn(worn(pet)))
}

/// Why `theme` isn't one of drawing `i`'s, naming those it has.
fn no_theme(pet: &PetUi, name: &str, i: usize, theme: &str) -> String {
    let names: Vec<&str> = pet.avatars[i].pack.palette.names().collect();
    format!(
        "{name} in {} has no theme \"{theme}\": its themes are {}",
        pet.avatars[i].pack.style(),
        listed(&names)
    )
}

/// The pet `name` (any case) and its drawing in `style`, else its default style; or why there is none, naming those
/// there are.
fn drawing(pet: &PetUi, name: &str, style: Option<&str>) -> Result<(String, usize), String> {
    let groups = pets(&pet.avatars);
    let Some(group) = groups.iter().find(|g| g.pet.eq_ignore_ascii_case(name)) else {
        let names: Vec<&str> = groups.iter().map(|g| g.pet.as_str()).collect();
        return Err(format!("no pet is called \"{name}\": the pets are {}", listed(&names)));
    };
    let i = match style {
        None => group.default,
        Some(style) => group
            .styles
            .iter()
            .copied()
            .find(|&i| pet.avatars[i].pack.style().eq_ignore_ascii_case(style))
            .ok_or_else(|| {
                let styles: Vec<&str> = group.styles.iter().map(|&i| pet.avatars[i].pack.style()).collect();
                format!(
                    "{} has no style \"{style}\": its styles are {}",
                    group.pet,
                    listed(&styles)
                )
            })?,
    };
    Ok((group.pet.clone(), i))
}

fn settings(pet: &PetUi) -> Settings {
    Settings {
        pet_size: pet.pet_size,
        bubble_size: pet.bubble_size,
        bubbles: pet.bubbles,
        on_top: pet.on_top,
        music: pet.music,
    }
}

/// A change the pet can make whole: sizes within their ranges.
fn check(change: &SettingsChange) -> Result<(), String> {
    let within = |what: &str, size: Option<i32>, range: &std::ops::RangeInclusive<i32>| match size {
        Some(size) if !range.contains(&size) => Err(format!(
            "{what} must be from {} to {} (percent), not {size}",
            range.start(),
            range.end()
        )),
        _ => Ok(()),
    };
    within("pet_size", change.pet_size, &geometry::PET_SIZES)?;
    within("bubble_size", change.bubble_size, &geometry::BUBBLE_SIZES)
}

/// Makes a checked change through Settings' messages; keeping the pet on top is the shell's.
fn set(pet: &mut PetUi, change: SettingsChange) -> Option<Effect> {
    let SettingsChange {
        pet_size,
        bubble_size,
        bubbles,
        on_top,
        music,
    } = change;
    let mut effect = None;
    let messages = [
        pet_size.map(Message::SetPetSize),
        bubble_size.map(Message::SetBubbleSize),
        bubbles.map(Message::SetBubbles),
        on_top.map(Message::SetOnTop),
        music.map(Message::SetMusic),
    ];
    for message in messages.into_iter().flatten() {
        effect = pet.update(message).or(effect);
    }
    effect
}

/// The avatars folder read again, as Settings' Reload does.
fn reload(pet: &mut PetUi) {
    pet.update(Message::Settings(crate::settings::Message::Avatars(
        crate::settings::avatars::Message::Reload,
    )));
}

/// The pet's theme by `theme` in any case, if it has it.
fn theme_named<'a>(pet: &'a PetUi, i: usize, theme: &str) -> Option<&'a str> {
    pet.avatars[i]
        .pack
        .palette
        .names()
        .find(|n| n.eq_ignore_ascii_case(theme))
}

fn list_themes(pet: &PetUi, name: &str, style: Option<&str>) -> Result<Answer, String> {
    let (pet_name, i) = drawing(pet, name, style)?;
    let pack = &pet.avatars[i].pack;
    let themes = pack
        .palette
        .names()
        .map(|n| ThemeColours {
            name: n.to_owned(),
            builtin: pack.palette.find(n).is_none_or(|t| t.file.is_none()),
            palette: pack
                .palette
                .in_theme(n)
                .unwrap_or_default()
                .into_iter()
                .map(|(c, v)| (c, colour_name(Some(v))))
                .collect(),
        })
        .collect();
    Ok(Answer::Themes {
        pet: pet_name,
        style: pack.style().to_owned(),
        themes,
    })
}

/// Saves a theme with Settings' code ([`aipet_sprite::themes::save`]) and reads the avatars folder again, so the pet
/// and an open Settings show it.
fn save_theme(
    pet: &mut PetUi,
    name: &str,
    style: Option<&str>,
    theme: &str,
    palette: &BTreeMap<String, String>,
    create: bool,
) -> Result<Answer, String> {
    let (pet_name, i) = drawing(pet, name, style)?;
    let editing = if create {
        None
    } else {
        Some(
            theme_named(pet, i, theme)
                .ok_or_else(|| format!("{} (create it with create: true)", no_theme(pet, &pet_name, i, theme)))?
                .to_owned(),
        )
    };
    let pack = &pet.avatars[i].pack;
    // a new theme from the default's colours, a changed one from its own
    let mut colours = pack
        .palette
        .in_theme(editing.as_deref().unwrap_or(&pack.palette.theme))
        .unwrap_or_default();
    for (n, text) in palette {
        let c = aipet_sprite::vector::colour(text.trim())
            .ok()
            .flatten()
            .ok_or_else(|| format!("{n}: {text:?} isn't a colour (#RRGGBB)"))?;
        colours.insert(n.clone(), c);
    }
    let avatars = aipet_sprite::Avatar::custom_dir(&pet.settings.services.data_dir);
    let saved = aipet_sprite::themes::save(&avatars, pack, theme, &colours, editing.as_deref())?;
    reload(pet);
    Ok(Answer::ThemeSaved {
        pet: pet_name,
        name: saved.name,
        file: saved.file,
    })
}

/// Deletes a theme of the user's with Settings' code: its file to the trash, the avatars folder read again, and a pet
/// that wore it in its default theme, saved.
fn delete_theme(pet: &mut PetUi, name: &str, style: Option<&str>, theme: &str) -> Result<Answer, String> {
    let (pet_name, i) = drawing(pet, name, style)?;
    let found = theme_named(pet, i, theme).ok_or_else(|| no_theme(pet, &pet_name, i, theme))?;
    let Some(file) = pet.avatars[i].pack.palette.find(found).and_then(|t| t.file.clone()) else {
        return Err(format!(
            "{found} is one of {pet_name}'s built-in themes, which can't be deleted"
        ));
    };
    let deleted = found.to_owned();
    let to = aipet_sprite::themes::delete(&pet.settings.services.data_dir, &file)?;
    reload(pet);
    pet.save_prefs();
    Ok(Answer::ThemeDeleted {
        deleted,
        trashed_to: to.display().to_string(),
    })
}

fn cards(pet: &PetUi) -> Vec<Card> {
    let Some(board) = &pet.board else {
        return Vec::new();
    };
    board
        .all()
        .iter()
        .map(|s| Card {
            id: s.id.clone(),
            source: s.agent.to_owned(),
            title: s.name.clone(),
            state: s.eff.to_owned(),
        })
        .collect()
}

/// Dismisses a card on the board, as its close button does.
fn dismiss(pet: &mut PetUi, id: String) -> Result<Answer, String> {
    let known = pet.board.as_ref().is_some_and(|b| b.all().iter().any(|s| s.id == id));
    if !known {
        return Err(format!(
            "no card on the board has the id \"{id}\" (get_board lists them)"
        ));
    }
    pet.update(Message::Dismiss(id.as_str().into()));
    Ok(Answer::Dismissed(id))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aipet_core::board::Sources;
    use aipet_core::control::{Answer, Command, Request, Settings, SettingsChange};

    use crate::tests::{Host, NOW, board, chat, detached};
    use crate::{Effect, Message, PetUi, Prefs, pets};

    /// Sends `command` as the shell hands it over, and takes the answer.
    fn ask(ui: &mut PetUi, command: Command) -> (Result<Answer, String>, Option<Effect>) {
        let (request, answer) = Request::new(command);
        let effect = ui.update(Message::Command(request));
        (answer.try_recv().expect("an answer at once"), effect)
    }

    /// The avatar index of `pet` in `style`.
    fn index(ui: &PetUi, pet: &str, style: &str) -> usize {
        pets(&ui.avatars)
            .into_iter()
            .find(|g| g.pet == pet)
            .and_then(|g| g.styles.into_iter().find(|&i| ui.avatars[i].pack.style() == style))
            .unwrap_or_else(|| panic!("no {pet} in {style}"))
    }

    /// What the pet keeps and shows: its preferences, and every pref the host was asked to save.
    fn kept(ui: &PetUi, host: &Host) -> (Prefs, Vec<Prefs>) {
        (ui.prefs(), host.prefs())
    }

    #[test]
    fn set_settings_does_what_settings_messages_do() {
        let (mut by_tool, tool_host) = detached();
        let (answer, effect) = ask(
            &mut by_tool,
            Command::SetSettings(SettingsChange {
                pet_size: Some(150),
                bubble_size: Some(90),
                bubbles: Some(false),
                // on by default: off is the change
                on_top: Some(false),
                music: Some(true),
            }),
        );
        let (mut by_settings, settings_host) = detached();
        let mut effects = Vec::new();
        for message in [
            Message::SetPetSize(150),
            Message::SetBubbleSize(90),
            Message::SetBubbles(false),
            Message::SetOnTop(false),
            Message::SetMusic(true),
        ] {
            effects.extend(by_settings.update(message));
        }
        assert_eq!(
            answer,
            Ok(Answer::Settings(Settings {
                pet_size: 150,
                bubble_size: 90,
                bubbles: false,
                on_top: false,
                music: true,
            }))
        );
        assert_eq!(kept(&by_tool, &tool_host), kept(&by_settings, &settings_host));
        assert_eq!(by_tool.sizes(), (150, 90));
        // keeping on top is the shell's, as it is for the switch
        assert_eq!(effect, Some(Effect::OnTop(false)));
        assert_eq!(effects, [Effect::OnTop(false)]);
        // get_settings reads it back, and changes nothing
        let saved = tool_host.prefs().len();
        let (again, _) = ask(&mut by_tool, Command::GetSettings);
        assert_eq!(again, answer);
        assert_eq!(tool_host.prefs().len(), saved);
    }

    #[test]
    fn an_out_of_range_size_changes_nothing_and_says_the_range() {
        for (pet_size, bubble_size, range) in [
            (Some(74), None, "75 to 200"),
            (Some(201), None, "75 to 200"),
            (None, Some(79), "80 to 120"),
            (None, Some(121), "80 to 120"),
            // a good size with a bad one: neither is made
            (Some(120), Some(130), "80 to 120"),
        ] {
            let (mut ui, host) = detached();
            let before = ui.prefs();
            let change = SettingsChange {
                pet_size,
                bubble_size,
                music: Some(true),
                ..SettingsChange::default()
            };
            let (answer, effect) = ask(&mut ui, Command::SetSettings(change));
            let e = answer.unwrap_err();
            assert!(e.contains(range), "{e}");
            assert_eq!(effect, None);
            assert_eq!(ui.prefs(), before, "{change:?}");
            assert!(host.prefs().is_empty(), "nothing saved");
        }
    }

    #[test]
    fn wear_avatar_wears_as_the_picker_does() {
        let (mut by_tool, _) = detached();
        let (answer, _) = ask(
            &mut by_tool,
            Command::Wear {
                name: "Hood".into(),
                style: Some("Vector".into()),
                theme: None,
            },
        );
        let Ok(Answer::Worn(worn)) = answer else {
            panic!("{answer:?}")
        };
        // the style's default theme, Green
        assert_eq!(
            (worn.name.as_str(), worn.style.as_str(), worn.theme.as_str()),
            ("Hood", "Vector", "Green")
        );
        assert_eq!(
            by_tool.avatars[by_tool.avatar].pack.kind,
            aipet_sprite::pack::Kind::Vector
        );
        // the picker: the pet's tile, then its Vector style, which keeps the Pixel's Blue; the tool, told Blue
        let (mut by_tool, tool_host) = detached();
        let (worn, _) = ask(
            &mut by_tool,
            Command::Wear {
                name: "Hood".into(),
                style: Some("Vector".into()),
                theme: Some("Blue".into()),
            },
        );
        assert!(worn.is_ok(), "{worn:?}");
        let (mut by_settings, settings_host) = detached();
        by_settings.update(Message::SelectAvatar(index(&by_settings, "Hood", "Pixel")));
        let vector = index(&by_settings, "Hood", "Vector");
        by_settings.update(Message::SelectStyle(vector));
        assert_eq!(by_tool.prefs(), by_settings.prefs());
        assert_eq!(tool_host.prefs().last(), settings_host.prefs().last());
        assert_eq!(by_tool.theme(), by_settings.theme());
    }

    #[test]
    fn wear_avatar_takes_a_theme_and_any_case_and_its_defaults() {
        let (mut ui, host) = detached();
        let hood = index(&ui, "Hood", "Pixel");
        let themes: Vec<String> = ui.avatars[hood].pack.palette.names().map(str::to_owned).collect();
        let other = themes.get(1).expect("Hood has more than one theme").clone();
        let (answer, _) = ask(
            &mut ui,
            Command::Wear {
                name: "hood".into(),
                style: Some("pixel".into()),
                theme: Some(other.to_uppercase()),
            },
        );
        let Ok(Answer::Worn(worn)) = answer else {
            panic!("{answer:?}")
        };
        assert_eq!(
            (worn.name, worn.style, &worn.theme),
            ("Hood".into(), "Pixel".into(), &other)
        );
        assert_eq!(ui.prefs().avatar_theme.as_deref(), Some(other.as_str()));
        // the same again changes nothing and saves nothing
        let saved = host.prefs().len();
        let (_, _) = ask(
            &mut ui,
            Command::Wear {
                name: "Hood".into(),
                style: Some("Pixel".into()),
                theme: Some(other.clone()),
            },
        );
        assert_eq!(host.prefs().len(), saved);
        // left out: the default style in its default theme
        let (answer, _) = ask(
            &mut ui,
            Command::Wear {
                name: "Hood".into(),
                style: None,
                theme: None,
            },
        );
        let Ok(Answer::Worn(worn)) = answer else {
            panic!("{answer:?}")
        };
        assert_eq!(worn.theme, themes[0]);
        assert_eq!(ui.prefs().avatar_theme, None);
    }

    #[test]
    fn an_unknown_pet_style_or_theme_names_the_valid_ones_and_changes_nothing() {
        for (name, style, theme, says) in [
            ("Dragon", None, None, "the pets are"),
            ("Hood", Some("Watercolour"), None, "its styles are \"Pixel\""),
            ("Hood", Some("Pixel"), Some("plaid"), "its themes are"),
        ] {
            let (mut ui, host) = detached();
            let before = (ui.prefs(), ui.avatar, ui.theme().to_owned());
            let (answer, _) = ask(
                &mut ui,
                Command::Wear {
                    name: name.into(),
                    style: style.map(Into::into),
                    theme: theme.map(Into::into),
                },
            );
            let e = answer.unwrap_err();
            assert!(e.contains(says), "{e}");
            assert_eq!((ui.prefs(), ui.avatar, ui.theme().to_owned()), before);
            assert!(host.prefs().is_empty());
        }
    }

    #[test]
    fn list_avatars_lists_the_three_built_in_pets_with_their_styles() {
        let (mut ui, _) = detached();
        let (answer, _) = ask(&mut ui, Command::ListAvatars);
        let Ok(Answer::Avatars { worn, avatars, skipped }) = answer else {
            panic!("{answer:?}")
        };
        // Sprout, Hood and Pip: the Hamster is no longer built in
        let names: Vec<&str> = avatars.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Sprout", "Hood", "Pip"]);
        assert!(skipped.is_empty());
        let hood = avatars.iter().find(|a| a.name == "Hood").expect("Hood");
        let styles: Vec<(&str, &str)> = hood
            .styles
            .iter()
            .map(|s| (s.style.as_str(), s.kind.as_str()))
            .collect();
        assert!(
            styles.contains(&("Pixel", "pixel")) && styles.contains(&("Vector", "vector")),
            "{styles:?}"
        );
        for style in avatars.iter().flat_map(|a| &a.styles) {
            assert!(!style.themes.is_empty(), "{style:?}");
        }
        // the worn pet is one of them, in its default style and theme
        let first = &avatars[0];
        assert_eq!(worn.name, first.name);
        assert_eq!(worn.style, first.styles[0].style);
        assert_eq!(worn.theme, first.styles[0].themes[0]);
    }

    #[test]
    fn get_board_lists_the_cards_and_dismiss_card_dismisses_as_the_close_button_does() {
        let (mut ui, host) = detached();
        let (empty, _) = ask(&mut ui, Command::GetBoard);
        assert_eq!(empty, Ok(Answer::Board(Vec::new())), "no board yet");
        let sources = Sources {
            chats: vec![chat(
                "claude:a",
                "working",
                "Fixing the build",
                "cargo test",
                "terminal",
                NOW,
            )],
            ..Sources::default()
        };
        ui.update(Message::Board(board(&sources)));
        let (answer, _) = ask(&mut ui, Command::GetBoard);
        let Ok(Answer::Board(cards)) = answer else {
            panic!("{answer:?}")
        };
        let card = cards.iter().find(|c| c.id == "claude:a").expect("the chat's card");
        assert_eq!(
            (card.source.as_str(), card.title.as_str(), card.state.as_str()),
            ("claude", "Fixing the build", "working")
        );

        let (unknown, _) = ask(&mut ui, Command::Dismiss("claude:nope".into()));
        assert!(unknown.unwrap_err().contains("get_board"));
        assert!(host.dismissed().is_empty());

        let (answer, _) = ask(&mut ui, Command::Dismiss("claude:a".into()));
        assert_eq!(answer, Ok(Answer::Dismissed("claude:a".into())));
        let (mut by_button, button_host) = detached();
        by_button.update(Message::Board(board(&sources)));
        by_button.update(Message::Dismiss(Arc::from("claude:a")));
        assert_eq!(host.dismissed(), button_host.dismissed());
        assert_eq!(host.dismissed(), ["claude:a"]);
    }

    /// A pet whose data folder is a fresh one of its own, and that folder.
    fn with_data(test: &str) -> (PetUi, std::path::PathBuf, Host) {
        let data = std::env::temp_dir().join(format!("aipet-ui-control-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).unwrap();
        let host = Host::default();
        let ui = PetUi::new(crate::Setup {
            host: Box::new(host.clone()),
            avatars: aipet_sprite::Avatar::custom_dir(&data),
            services: crate::settings::Services::detached(data.clone()),
            ..crate::Setup::detached()
        });
        (ui, data, host)
    }

    fn save(pet: &str, name: &str, palette: &[(&str, &str)], create: bool) -> Command {
        Command::SaveTheme {
            pet: pet.into(),
            style: Some("vector".into()),
            name: name.into(),
            palette: palette.iter().map(|&(n, c)| (n.to_owned(), c.to_owned())).collect(),
            create,
        }
    }

    fn themes_of(ui: &mut PetUi, pet: &str) -> Vec<(String, bool)> {
        match ask(
            ui,
            Command::ListThemes {
                pet: pet.into(),
                style: Some("Vector".into()),
            },
        )
        .0
        {
            Ok(Answer::Themes { themes, .. }) => themes.into_iter().map(|t| (t.name, t.builtin)).collect(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn theme_tools_save_list_and_delete_with_settings_files() {
        let (mut ui, data, _host) = with_data("themes");
        let themes = aipet_sprite::Avatar::custom_dir(&data).join("themes");
        assert_eq!(
            themes_of(&mut ui, "hood"),
            [("Green".into(), true), ("Blue".into(), true)]
        );
        // a new theme: the same file Settings writes (tests/settings.rs: hood-vector-red.json)
        let (answer, _) = ask(&mut ui, save("Hood", "Red", &[("body", "#441010")], true));
        assert_eq!(
            answer,
            Ok(Answer::ThemeSaved {
                pet: "Hood".into(),
                name: "Red".into(),
                file: "hood-vector-red.json".into()
            })
        );
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(themes.join("hood-vector-red.json")).unwrap()).unwrap();
        assert_eq!(
            saved,
            serde_json::json!({"pet": "Hood (vector)", "theme": "Red", "palette": {"body": "#441010"}})
        );
        assert_eq!(themes_of(&mut ui, "Hood").last(), Some(&("Red".into(), false)));
        // worn, then changed: the rest of its colours stay, and the pet wears it as it is now
        ask(
            &mut ui,
            Command::Wear {
                name: "Hood".into(),
                style: Some("Vector".into()),
                theme: Some("red".into()),
            },
        )
        .0
        .unwrap();
        ask(&mut ui, save("Hood", "Red", &[("hi", "#FF4A3A")], false))
            .0
            .unwrap();
        let worn = crate::settings::avatars::worn(&ui);
        assert_eq!(worn.pack.palette.colours["body"], 0xFF441010);
        assert_eq!(worn.pack.palette.colours["hi"], 0xFFFF4A3A);

        // refused, changing nothing
        let before: Vec<_> = std::fs::read_dir(&themes).unwrap().map(|e| e.unwrap().path()).collect();
        for (command, says) in [
            (save("Hood", "RED", &[], true), "already has a theme named Red"),
            (save("Hood", "Green", &[], false), "built-in"),
            (save("Hood", "Gone", &[], false), "create it with create: true"),
            (save("Hood", "a/b", &[], true), "only letters"),
            (save("Hood", "Odd", &[("body", "red")], true), "isn't a colour"),
            (save("Hood", "Odd", &[("fur", "#000000")], true), "no colour named fur"),
            (
                Command::DeleteTheme {
                    pet: "Hood".into(),
                    style: Some("Vector".into()),
                    name: "Blue".into(),
                },
                "can't be deleted",
            ),
        ] {
            let e = ask(&mut ui, command).0.unwrap_err();
            assert!(e.contains(says), "{e}");
        }
        let after: Vec<_> = std::fs::read_dir(&themes).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(before, after);

        // deleted while worn: to the trash, and the pet wears its default, saved
        let (answer, _) = ask(
            &mut ui,
            Command::DeleteTheme {
                pet: "hood".into(),
                style: Some("Vector".into()),
                name: "red".into(),
            },
        );
        let Ok(Answer::ThemeDeleted { deleted, trashed_to }) = answer else {
            panic!("{answer:?}")
        };
        assert_eq!(deleted, "Red");
        assert!(std::path::Path::new(&trashed_to).starts_with(data.join("trash")));
        assert!(!themes.join("hood-vector-red.json").exists());
        assert_eq!(ui.theme(), "Green");
        assert_eq!(ui.prefs().avatar_theme, None);
        // never listed: the trash is outside the avatars folder
        let Answer::Avatars { skipped, .. } = ask(&mut ui, Command::Reload).0.unwrap() else {
            panic!()
        };
        assert!(skipped.is_empty(), "{skipped:?}");
        assert_eq!(themes_of(&mut ui, "Hood").len(), 2);
    }
}

#[cfg(test)]
mod timing {
    use std::time::Instant;

    use aipet_core::control::{Command, Request};

    use crate::Message;
    use crate::tests::detached;

    /// How long wearing each pet's style and theme takes on the UI thread (a style switch's stall): run with
    /// `cargo test --release -p aipet-ui switch_times -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, not a check"]
    fn switch_times() {
        let (mut ui, _) = detached();
        let cases = [
            ("Hood", "Pixel", "Blue"),
            ("Hood", "Pixel", "Green"),
            ("Hood", "Vector", "Green"),
            ("Hood", "Vector", "Blue"),
            ("Sprout", "Pixel", "Default"),
            ("Sprout", "Vector", "Default"),
            ("Pip", "Vector", "Peach"),
            ("Pip", "Vector", "Lilac"),
            ("Pip", "Painted", "Peach"),
            ("Pip", "Painted", "Lilac"),
        ];
        for round in 0..2 {
            for (name, style, theme) in cases {
                let (request, answer) = Request::new(Command::Wear {
                    name: name.into(),
                    style: Some(style.into()),
                    theme: Some(theme.into()),
                });
                let start = Instant::now();
                ui.update(Message::Command(request));
                let wear = start.elapsed();
                let start = Instant::now();
                let _ = ui.settings_view();
                let view = start.elapsed();
                assert!(answer.try_recv().unwrap().is_ok());
                println!(
                    "round {round}: {name} {style} {theme}: wear {:.1} ms, Settings view {:.1} ms",
                    wear.as_secs_f64() * 1e3,
                    view.as_secs_f64() * 1e3
                );
            }
        }
    }
}
