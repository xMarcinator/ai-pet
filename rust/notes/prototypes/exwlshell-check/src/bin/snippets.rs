//! Smaller API snippets referenced by the notes, compile-checked only.
#![allow(dead_code)]

use iced::widget::text;
use iced::{Color, Element, Task, Theme, window};
use iced_exwlshell::actions::{ActionCallback, IcedNewMenuSettings, IcedNewPopupSettings};
use iced_exwlshell::reexport::{Anchor, LayerSize, PixelSize, PopupGravity};
use iced_exwlshell::settings::{LayerShellSettings, Settings, StartMode};
use iced_exwlshell::shell::ShellType;

// ---- 1. Single-surface form: `layershell::application` + the non-multi macro ----
mod single {
    use super::*;
    use iced_exwlshell::layershell::application;
    use iced_exwlshell::to_layer_message;

    #[to_layer_message]
    #[derive(Debug, Clone)]
    pub enum Msg {
        Clicked,
    }

    pub fn run() -> iced_exwlshell::Result {
        application(|| 0u32, "single", update, view)
            .style(|_: &u32, theme: &Theme| iced::theme::Style {
                background_color: Color::TRANSPARENT,
                text_color: theme.palette().text,
            })
            .layer_settings(LayerShellSettings {
                size: LayerSize::px(220, 160),
                anchor: Anchor::Bottom | Anchor::Left,
                margin: (0, 0, 40, 40), // (top, right, bottom, left)
                // application() rejects Background and AllScreens (layershell.rs:403-410)
                start_mode: StartMode::Active,
                events_transparent: true,
                ..Default::default()
            })
            .run()
    }

    fn update(_: &mut u32, message: Msg) -> Task<Msg> {
        match message {
            // No id in the single form: the action targets the first surface
            // (multi_window.rs:831-836).
            Msg::Clicked => Task::batch([
                Task::done(Msg::SetInputRegion(ActionCallback::new(|region| {
                    region.add(0, 40, 130, 120);
                }))),
                Task::done(Msg::MarginChange((0, 0, 80, 120))),
                Task::done(Msg::LayoutChange {
                    anchor: Anchor::Bottom | Anchor::Left,
                    size: LayerSize::px(220, 200),
                }),
            ]),
            _ => Task::none(),
        }
    }

    fn view(_: &u32) -> Element<'_, Msg> {
        text("pet").into()
    }
}

// ---- 2. Multi form: menus, popup reposition, scale query, on_new_shell ----
mod multi {
    use super::*;
    use iced_exwlshell::daemon;
    use iced_exwlshell::to_layer_message;

    #[to_layer_message(multi)]
    #[derive(Debug, Clone)]
    pub enum Msg {
        OpenMenu,
        MovePopup(window::Id),
        Scale(window::Id, f32),
        Appeared(window::Id, ShellType),
    }

    #[derive(Default)]
    pub struct App;

    pub fn run() -> iced_exwlshell::Result {
        // Skip the always-on smithay-clipboard worker thread when no copy/paste is needed.
        iced_exwlshell::disable_clipboard();
        daemon(App::default, "multi", update, view)
            // Synchronous, before the surface's first frame (daemon.md:3-9).
            .on_new_shell(|info| Some(Msg::Appeared(info.window, info.shell)))
            .settings(Settings {
                layer_settings: LayerShellSettings {
                    start_mode: StartMode::Background,
                    ..Default::default()
                },
                ..Default::default()
            })
            .run()
    }

    fn update(_: &mut App, message: Msg) -> Task<Msg> {
        match message {
            // NewMenu: opens at the pointer position on the surface under the pointer,
            // with no grab, so nothing dismisses it for you.
            Msg::OpenMenu => Task::done(Msg::NewMenu {
                settings: IcedNewMenuSettings {
                    size: PixelSize::px(160, 96),
                    gravity: PopupGravity::TopRight,
                },
                id: window::Id::unique(),
            }),
            Msg::MovePopup(id) => Task::done(Msg::PopUpReposition {
                settings: IcedNewPopupSettings::at_position_on_current_surface(
                    PixelSize::px(200, 120),
                    (10, 10),
                ),
                id,
            }),
            // Returns the Wayland (fractional) scale of that surface.
            Msg::Appeared(id, ShellType::LayerShell) => {
                window::scale_factor(id).map(move |scale| Msg::Scale(id, scale))
            }
            _ => Task::none(),
        }
    }

    fn view(_: &App, _id: window::Id) -> Element<'_, Msg> {
        text("surface").into()
    }
}

// ---- 3. Backend choice: only use exwlshell when the compositor has layer-shell ----
mod probe {
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::{Connection, Dispatch, QueueHandle};

    struct Probe;

    impl Dispatch<WlRegistry, GlobalListContents> for Probe {
        fn event(
            _: &mut Self,
            _: &WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    /// Some(connection) when running under a Wayland compositor that offers
    /// zwlr_layer_shell_v1 (v3+, as exwlshellev binds 3..=4), xdg_wm_base and wp_viewporter.
    /// Hand the connection to `Settings::with_connection`.
    pub fn layer_shell_connection() -> Option<Connection> {
        let connection = Connection::connect_to_env().ok()?;
        let (globals, _queue) = registry_queue_init::<Probe>(&connection).ok()?;
        let ok = globals.contents().with_list(|list| {
            let has = |name: &str, min: u32| {
                list.iter().any(|g| g.interface == name && g.version >= min)
            };
            has("zwlr_layer_shell_v1", 3) && has("xdg_wm_base", 2) && has("wp_viewporter", 1)
        });
        ok.then_some(connection)
    }
}

fn main() {
    if let Some(connection) = probe::layer_shell_connection() {
        let _settings = Settings {
            with_connection: Some(connection.into()),
            ..Default::default()
        };
        let _ = (single::run as fn() -> _, multi::run as fn() -> _);
    }
}
