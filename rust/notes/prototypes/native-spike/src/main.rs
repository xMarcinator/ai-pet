//! Spike harness: opens an iced window that is never shown (visible: false), runs the native calls on it through
//! `iced::window::run`, prints what the X server now has for it, and exits. Nothing ever appears on screen.
//! Run under X11: `env -u WAYLAND_DISPLAY cargo run`.

mod native;

use iced::widget::text;
use iced::window;
use iced::{Element, Size, Task};

#[derive(Debug, Clone)]
enum Message {
    Opened(window::Id),
    Report(String),
}

struct Harness;

fn main() -> iced::Result {
    iced::daemon(boot, update, view).run()
}

fn boot() -> (Harness, Task<Message>) {
    let (_id, open) = window::open(window::Settings {
        size: Size::new(380.0, 600.0),
        visible: false, // stays unmapped: setup runs before any show, and nothing is shown in this test
        decorations: false,
        transparent: true,
        resizable: false,
        level: window::Level::AlwaysOnTop,
        #[cfg(target_os = "windows")]
        platform_specific: window::settings::PlatformSpecific { skip_taskbar: true, ..Default::default() },
        ..window::Settings::default()
    });
    (Harness, open.map(Message::Opened))
}

fn update(_: &mut Harness, message: Message) -> Task<Message> {
    match message {
        Message::Opened(id) => window::run(id, |w| {
            let opts = native::PetWindowOptions { south_gravity: true, ..Default::default() };
            let mut out = Vec::new();
            out.push(format!("setup: {:?}", native::setup_pet_window(w, &opts)));
            // the sprite (26x24 x5 = 130x120 at the bottom-centre, padded 6) and one bubble card (padded 16)
            let rects = [native::Rect::new(125.0, 476.0, 130.0, 120.0).padded(6.0), native::Rect::new(40.0, 300.0, 300.0, 64.0).padded(16.0)];
            out.push(format!("set_input_region: {:?}", native::set_input_region(w, &rects, 1.25)));
            out.push(format!("keep_on_top: {:?}", native::keep_on_top(w)));
            out.push(format!("update_hit_test: {:?}", native::update_hit_test(w)));
            #[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
            {
                match native::x11_window_id(w) {
                    Ok(id) => out.push(native::x11_describe(id).unwrap_or_else(|e| e.to_string())),
                    Err(e) => out.push(format!("not X11: {e}")),
                }
                if std::env::var_os("HOLD").is_none() {
                    out.push(format!("clear_input_region: {:?}", native::clear_input_region(w)));
                }
                if let Ok(id) = native::x11_window_id(w) {
                    out.push(format!("after clear: {}", native::x11_describe(id).unwrap_or_else(|e| e.to_string())));
                }
            }
            out.join("\n")
        })
        .map(Message::Report),
        Message::Report(report) => {
            println!("{report}");
            // HOLD=1: stay a moment so another client (xprop) can look at the window
            if std::env::var_os("HOLD").is_some() {
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
            iced::exit()
        }
    }
}

fn view(_: &Harness, _: window::Id) -> Element<'_, Message> {
    text("pet").into()
}
