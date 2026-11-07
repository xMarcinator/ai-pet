//! How the winit shell would drive `native` (compile-checked sketch, not run: it would show a window).
//! Open hidden -> setup -> show; input region from the layout, only when it changed and at most every 100 ms
//! (MainWindow.OnFrame); keep-on-top every 2 s while "Always on top" is on (MainWindow's Timer(2000, KeepOnTop));
//! on macOS the hit test ~30 times a second. All of it is driven by window::frames(), because iced::time::every needs
//! iced's `tokio` or `smol` feature (the thread-pool executor has no timers).

#[path = "../native.rs"]
mod native;

use std::time::{Duration, Instant};

use iced::widget::text;
use iced::{Element, Event, Size, Subscription, Task, event, window};

struct Pet {
    id: Option<window::Id>,
    shown: bool,
    scale: f32,
    on_top: bool,
    last_region: Vec<native::PxRect>,
    region_at: Instant,
    top_at: Instant,
    hit_at: Instant,
}

#[derive(Debug, Clone)]
enum Message {
    Opened(window::Id),
    Shown(f32),
    Frame(Instant),
    Rescaled(f32),
    Native(Result<(), native::NativeError>),
}

fn main() -> iced::Result {
    iced::daemon(boot, update, view).subscription(subscription).run()
}

fn pet_settings() -> window::Settings {
    window::Settings {
        size: Size::new(380.0, 600.0),
        visible: false, // shown by update() once setup_pet_window has run
        decorations: false,
        transparent: true,
        resizable: false,
        level: window::Level::AlwaysOnTop,
        exit_on_close_request: false,
        #[cfg(target_os = "linux")]
        platform_specific: window::settings::PlatformSpecific {
            application_id: "aipet".into(), // WM_CLASS / app_id; iced's default is "" (empty WM_CLASS)
            ..Default::default()
        },
        #[cfg(target_os = "windows")]
        platform_specific: window::settings::PlatformSpecific { skip_taskbar: true, ..Default::default() },
        ..window::Settings::default()
    }
}

fn boot() -> (Pet, Task<Message>) {
    let (_, open) = window::open(pet_settings());
    let now = Instant::now();
    let pet = Pet {
        id: None,
        shown: false,
        scale: 1.0,
        on_top: true,
        last_region: Vec::new(),
        region_at: now,
        top_at: now,
        hit_at: now,
    };
    (pet, open.map(Message::Opened))
}

/// Where the pet and its bubbles are, in logical units (the real app takes this from its layout, padded as the C#
/// does: sprite 6, cards 16, close buttons and the menu 2).
fn hit_rects() -> Vec<native::Rect> {
    vec![native::Rect::new(125.0, 476.0, 130.0, 120.0).padded(6.0)]
}

fn update(pet: &mut Pet, message: Message) -> Task<Message> {
    match message {
        Message::Opened(id) => {
            pet.id = Some(id);
            let opts = native::PetWindowOptions::default();
            window::run(id, move |w| native::setup_pet_window(w, &opts)).then(move |result| {
                if let Err(e) = result {
                    eprintln!("pet window setup: {e}");
                }
                Task::batch([
                    window::set_mode(id, window::Mode::Windowed),
                    // winit's own "above" went out while the window was unmapped; say it again now it is mapped
                    window::set_level(id, window::Level::AlwaysOnTop),
                    window::scale_factor(id).map(Message::Shown),
                ])
            })
        }
        Message::Shown(scale) => {
            pet.shown = true;
            pet.scale = scale;
            Task::none()
        }
        Message::Rescaled(scale) => {
            pet.scale = scale;
            pet.last_region.clear(); // force a new region at the new scale
            Task::none()
        }
        Message::Frame(now) => {
            let Some(id) = pet.id.filter(|_| pet.shown) else { return Task::none() };
            let mut tasks = Vec::new();
            if now - pet.region_at >= Duration::from_millis(100) {
                let px = native::to_physical(&hit_rects(), pet.scale);
                if px != pet.last_region {
                    pet.region_at = now;
                    pet.last_region = px.clone();
                    tasks.push(window::run(id, move |w| native::set_input_region_px(w, &px)).map(Message::Native));
                }
            }
            if pet.on_top && now - pet.top_at >= Duration::from_secs(2) {
                pet.top_at = now;
                tasks.push(window::run(id, |w| native::keep_on_top(w)).map(Message::Native));
            }
            if cfg!(target_os = "macos") && now - pet.hit_at >= Duration::from_millis(33) {
                pet.hit_at = now;
                tasks.push(window::run(id, |w| native::update_hit_test(w).map(|_| ())).map(Message::Native));
            }
            Task::batch(tasks)
        }
        Message::Native(result) => {
            if let Err(e) = result {
                eprintln!("native: {e}");
            }
            Task::none()
        }
    }
}

fn subscription(_: &Pet) -> Subscription<Message> {
    Subscription::batch([
        window::frames().map(Message::Frame),
        event::listen_with(|event, _status, _id| match event {
            Event::Window(window::Event::Rescaled(scale)) => Some(Message::Rescaled(scale)),
            _ => None,
        }),
    ])
}

fn view(_: &Pet, _: window::Id) -> Element<'_, Message> {
    text("pet").into()
}
