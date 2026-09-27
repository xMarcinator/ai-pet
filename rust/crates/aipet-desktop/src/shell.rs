//! The desktop shell: an iced daemon with the pet in a transparent, undecorated, always-on-top window.
//!
//! The window is [`SURFACE`] (380 × 600). It opens hidden, so the native setup (utility window, keep above, out of
//! the taskbar) is in place before the window manager sees it, and shows near the primary monitor's bottom-right
//! corner. Its input region is [`PetUi::hit_rects`] in physical pixels, set at most every 100 ms when it changed
//! (as MainWindow.OnFrame does), so everything else clicks through. A left press on the sprite drags the window after
//! the pointer, read from the desktop every frame; a press that doesn't move pokes the pet. A right press on the
//! sprite opens the menu above it, inside the window and its input region. Settings is an ordinary window.

use std::time::{Duration, Instant};

use aipet_ui::{Effect, PetUi, SURFACE};
use iced::widget::Space;
use iced::{Color, Element, Event, Point, Size, Subscription, Task, Theme, event, mouse, theme, window};

use crate::native::{self, NativeError, PxRect};

/// The pet window's title, and the WM_CLASS of the spike's windows: not the real pet's, so that its window rules
/// and scripts leave these alone.
const TITLE: &str = "AiPet spike";
#[cfg(target_os = "linux")]
const APP_ID: &str = "aipet-spike";
/// Where the window starts: this far left of the monitor's bottom-right corner (the real pet lives in the corner).
const START_RIGHT: f32 = 420.0;
/// Settings' size (SettingsWindow.axaml).
const SETTINGS: Size = Size::new(800.0, 640.0);
/// The input region changes at most this often (MainWindow.OnFrame's 0.1 s).
const REGION_EVERY: Duration = Duration::from_millis(100);
/// How far past what takes the mouse the pet draws: the bubbles' shadows (a 14 px blur, 4 px down) and the sprite's
/// glow halo (10 px). A region that clips drawing too (X11's bounding shape, a Windows window region) is grown by
/// this much.
const DRAWN_PAD: f32 = 18.0;
/// "Always on top" is said again this often while it is on (MainWindow's KeepOnTop timer).
const KEEP_ON_TOP_EVERY: Duration = Duration::from_secs(2);
/// macOS: the pointer is tested against the input region this often.
#[cfg(target_os = "macos")]
const HIT_TEST_EVERY: Duration = Duration::from_millis(33);

#[derive(Debug, Clone)]
enum Message {
    Ui(aipet_ui::Message),
    /// The pet window drew a frame: time to move everything on.
    Frame(Instant),
    /// A mouse or window event on one of the windows, and whether a widget took it.
    Input(window::Id, Event, event::Status),
    /// The pet window exists, hidden.
    PetOpened,
    /// The pet window is set up and shown, at this scale factor.
    Shown(f32),
    /// Where the pet window was when the drag began (`None` where the window system doesn't say).
    DragOrigin(Option<Point>),
    /// A native call failed: what it was doing, and why.
    Native(&'static str, NativeError),
}

struct Shell {
    ui: PetUi,
    pet: window::Id,
    /// The pet window's scale factor (physical px per logical px), once it is shown.
    scale: Option<f32>,
    /// The input region the window has, and when it was set.
    region: Option<Vec<PxRect>>,
    region_at: Instant,
    /// When "always on top" was last said again.
    top_at: Instant,
    #[cfg(target_os = "macos")]
    hit_at: Instant,
    drag: Option<Drag>,
    /// Whether the menu shows (inside the pet window, above the sprite).
    menu: bool,
    settings: Option<window::Id>,
    /// AIPET_OPEN_SETTINGS=1: Settings opens once the pet shows.
    open_settings: bool,
    /// The last native failure reported, so a failing call that repeats every frame is reported once.
    last_error: Option<String>,
    /// AIPET_DEBUG=1: log pointer, drag, menu and input-region events to stderr, stamped with the time since this.
    debug: Option<Instant>,
}

/// A left press on the sprite, until its release.
struct Drag {
    /// Where the pointer was on the desktop at the press (`None` where it can't be read: the pet can't be moved).
    pressed: Option<Point>,
    /// Where the window was at the press, once the window system has said.
    window: Option<Point>,
    /// The pointer when last read, to skip frames it didn't move in.
    last: Option<Point>,
}

/// Runs the pet until it quits.
pub fn run() -> Result<(), String> {
    iced::daemon(Shell::boot, Shell::update, Shell::view)
        .title(Shell::title)
        .theme(Theme::Dark)
        // one clear colour for every window: transparent (Settings paints its own background)
        .style(|_: &Shell, theme: &Theme| theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: theme.palette().text,
        })
        .subscription(Shell::subscription)
        .run()
        .map_err(|e| match e {
            iced::Error::GraphicsCreationFailed(e) => format!("no GPU surface for the pet window: {e}"),
            iced::Error::WindowCreationFailed(e) => format!("cannot make the pet window: {e}"),
            iced::Error::ExecutorCreationFailed(e) => format!("cannot start the async runtime: {e}"),
        })
}

impl Shell {
    fn boot() -> (Self, Task<Message>) {
        let ui = PetUi::new();
        let (pet, open) = window::open(pet_window(ui.on_top()));
        let now = Instant::now();
        let shell = Shell {
            ui,
            pet,
            scale: None,
            region: None,
            region_at: now,
            top_at: now,
            #[cfg(target_os = "macos")]
            hit_at: now,
            drag: None,
            menu: false,
            settings: None,
            open_settings: env_is_1("AIPET_OPEN_SETTINGS"),
            last_error: None,
            debug: env_is_1("AIPET_DEBUG").then_some(now),
        };
        (shell, open.map(|_| Message::PetOpened))
    }

    fn title(&self, id: window::Id) -> String {
        if id == self.pet {
            TITLE.to_owned()
        } else {
            format!("{TITLE} · Settings")
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let input = event::listen_with(|event, status, id| match event {
            Event::Mouse(
                mouse::Event::ButtonPressed(_)
                | mouse::Event::ButtonReleased(_)
                | mouse::Event::CursorEntered
                | mouse::Event::CursorLeft,
            )
            | Event::Window(
                window::Event::Rescaled(_)
                | window::Event::Unfocused
                | window::Event::CloseRequested
                | window::Event::Closed,
            ) => Some(Message::Input(id, event, status)),
            _ => None,
        });
        Subscription::batch([frames_of(self.pet), input])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PetOpened => {
                let (pet, on_top) = (self.pet, self.ui.on_top());
                let setup = native_call(pet, "setting up the pet window", move |w| {
                    native::setup_pet_window(w, on_top)
                });
                // then, whatever came of it, show it, and say "above" again now that it is mapped (winit said it
                // while it was not)
                setup.chain(Task::batch([
                    window::set_mode(pet, window::Mode::Windowed),
                    window::set_level(pet, level(on_top)),
                    window::scale_factor(pet).map(Message::Shown),
                ]))
            }
            Message::Shown(scale) => {
                self.scale = Some(scale);
                self.log(|| format!("pet window shown, scale factor {scale}"));
                if std::mem::take(&mut self.open_settings) {
                    return self.open_settings();
                }
                Task::none()
            }
            Message::Frame(now) => self.frame(now),
            Message::Ui(message) => {
                let chosen = matches!(message, aipet_ui::Message::Menu(_));
                let effect = self.ui.update(message).map_or_else(Task::none, |e| self.apply(e));
                if chosen {
                    return Task::batch([self.set_menu(false), effect]);
                }
                effect
            }
            Message::Input(id, event, status) if id == self.pet => self.pet_input(event, status),
            Message::Input(id, Event::Window(window::Event::Closed), _) if Some(id) == self.settings => {
                self.log(|| "Settings closed".to_owned());
                self.settings = None;
                Task::none()
            }
            Message::Input(..) => Task::none(),
            Message::DragOrigin(at) => {
                self.log(|| format!("drag from the window at {at:?}"));
                let Some(drag) = &mut self.drag else {
                    return Task::none();
                };
                drag.window = at;
                if at.is_none() {
                    let e = NativeError::Unsupported("the window system doesn't say where the window is");
                    self.report("dragging the pet", &e);
                }
                Task::none()
            }
            Message::Native(what, e) => {
                self.report(what, &e);
                Task::none()
            }
        }
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        if id == self.pet {
            self.ui.pet_view().map(Message::Ui)
        } else if Some(id) == self.settings {
            self.ui.settings_view().map(Message::Ui)
        } else {
            Space::new().into()
        }
    }

    /// Moves the pet on to `now`, then does what is due: follow a drag, update the input region, keep on top.
    fn frame(&mut self, now: Instant) -> Task<Message> {
        self.ui.tick(now);
        let Some(scale) = self.scale else {
            return Task::none();
        };
        let mut tasks = vec![self.follow_pointer(scale), self.sync_region(now, false)];
        if self.ui.on_top() && now.saturating_duration_since(self.top_at) >= KEEP_ON_TOP_EVERY {
            self.top_at = now;
            tasks.push(native_call(self.pet, "keeping the pet on top", native::keep_on_top));
        }
        #[cfg(target_os = "macos")]
        if now.saturating_duration_since(self.hit_at) >= HIT_TEST_EVERY {
            self.hit_at = now;
            tasks.push(native_call(self.pet, "testing the pointer", native::update_hit_test));
        }
        Task::batch(tasks)
    }

    /// Mouse and window events on the pet window.
    fn pet_input(&mut self, event: Event, status: event::Status) -> Task<Message> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(button)) => return self.press(button, status),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(drag) = self.drag.take() {
                    let moved = self.ui.drag_moved();
                    self.ui.update(aipet_ui::Message::DragEnded);
                    self.log(|| match drag.pressed.zip(drag.last) {
                        Some((pressed, last)) if moved => format!("dropped {:?} from the press", last - pressed),
                        _ => "poked".to_owned(),
                    });
                }
            }
            Event::Mouse(mouse::Event::CursorEntered) => self.log(|| "pointer entered the pet window".to_owned()),
            Event::Mouse(mouse::Event::CursorLeft) => self.log(|| "pointer left the pet window".to_owned()),
            Event::Window(window::Event::Rescaled(scale)) => {
                self.log(|| format!("scale factor {scale}"));
                self.scale = Some(scale);
                return self.sync_region(Instant::now(), true);
            }
            // a click elsewhere took the focus: the menu goes, as a menu does
            Event::Window(window::Event::Unfocused) => return self.set_menu(false),
            // it has no title bar, but Alt+F4 and the like quit
            Event::Window(window::Event::CloseRequested | window::Event::Closed) => return iced::exit(),
            _ => {}
        }
        Task::none()
    }

    /// A press on the pet window that no widget took: closes the menu if it shows; otherwise a left press on the
    /// sprite starts a drag and a right one opens the menu.
    fn press(&mut self, button: mouse::Button, status: event::Status) -> Task<Message> {
        let at = self.ui.pointer();
        let on_sprite = at.is_some_and(|p| self.ui.sprite_hit(p));
        self.log(|| format!("{button:?} press at {at:?}: {}", press_target(on_sprite, status)));
        // a bubble, its dismiss button or a menu item took it
        if status == event::Status::Captured {
            return Task::none();
        }
        if self.menu {
            return self.set_menu(false);
        }
        match button {
            mouse::Button::Left if on_sprite => {
                let scale = self.scale.unwrap_or(1.0);
                let pressed = native::pointer(scale)
                    .map_err(|e| self.report("reading the pointer", &e))
                    .ok();
                self.drag = Some(Drag {
                    pressed,
                    window: None,
                    last: None,
                });
                self.ui.update(aipet_ui::Message::DragStarted);
                window::position(self.pet).map(Message::DragOrigin)
            }
            mouse::Button::Right if on_sprite => self.set_menu(true),
            _ => Task::none(),
        }
    }

    /// While the sprite is held: tells the pet how far the pointer is from the press and, once that is a drag, puts
    /// the window where the pointer took it. The pointer comes from the desktop, not from the window's events (whose
    /// positions move with the window), so the pet stays under it without lagging or overshooting.
    fn follow_pointer(&mut self, scale: f32) -> Task<Message> {
        let Some(drag) = &mut self.drag else {
            return Task::none();
        };
        let (Some(pressed), Ok(now)) = (drag.pressed, native::pointer(scale)) else {
            return Task::none();
        };
        if drag.last == Some(now) {
            return Task::none();
        }
        drag.last = Some(now);
        let from_press = now - pressed;
        self.ui.update(aipet_ui::Message::DragMoved(from_press));
        match drag.window {
            Some(window) if self.ui.drag_moved() => window::move_to(self.pet, window + from_press),
            _ => Task::none(),
        }
    }

    /// Hands the pet's hit rectangles to the window as its input region, when they changed and at most every
    /// [`REGION_EVERY`], or at once if `force`.
    fn sync_region(&mut self, now: Instant, force: bool) -> Task<Message> {
        let Some(scale) = self.scale else {
            return Task::none();
        };
        let due = self.region.is_none() || now.saturating_duration_since(self.region_at) >= REGION_EVERY;
        if !force && !due {
            return Task::none();
        }
        let rects = self.ui.hit_rects();
        let input = native::to_physical(&rects, scale, 0.0);
        if self.region.as_ref() == Some(&input) {
            return Task::none();
        }
        let drawn = native::to_physical(&rects, scale, DRAWN_PAD);
        self.log(|| format!("input region: {} rects, {}", input.len(), describe(&input)));
        self.region = Some(input.clone());
        self.region_at = now;
        native_call(self.pet, "setting the input region", move |w| {
            native::set_region(w, &input, &drawn)
        })
    }

    /// Shows or hides the menu; it takes the mouse from the moment it shows.
    fn set_menu(&mut self, open: bool) -> Task<Message> {
        if self.menu == open {
            return Task::none();
        }
        self.menu = open;
        self.ui.set_inline_menu(open);
        self.log(|| format!("menu {}", if open { "opened" } else { "closed" }));
        self.sync_region(Instant::now(), true)
    }

    /// Does what the pet's UI asks of the shell.
    fn apply(&mut self, effect: Effect) -> Task<Message> {
        match effect {
            Effect::OnTop(on) => window::set_level(self.pet, level(on)),
            Effect::OpenSettings => self.open_settings(),
            Effect::Quit => iced::exit(),
        }
    }

    /// Opens Settings, or brings it forward if it is open.
    fn open_settings(&mut self) -> Task<Message> {
        if let Some(id) = self.settings {
            return window::gain_focus(id);
        }
        let (id, open) = window::open(settings_window());
        self.settings = Some(id);
        self.log(|| "Settings opened".to_owned());
        open.discard()
    }

    /// Says on stderr that a native call failed, unless it is the failure said last.
    fn report(&mut self, what: &str, e: &NativeError) {
        let line = format!("{what}: {e}");
        if self.last_error.as_ref() != Some(&line) {
            eprintln!("aipet: {line}");
            self.last_error = Some(line);
        }
    }

    fn log(&self, line: impl FnOnce() -> String) {
        if let Some(start) = self.debug {
            eprintln!("aipet [{:7.3}s]: {}", start.elapsed().as_secs_f64(), line());
        }
    }
}

/// The pet window: see the module's documentation.
fn pet_window(on_top: bool) -> window::Settings {
    window::Settings {
        size: SURFACE,
        position: window::Position::SpecificWith(start_position),
        // shown once the native setup is in (Message::PetOpened)
        visible: false,
        resizable: false,
        decorations: false,
        transparent: true,
        level: level(on_top),
        exit_on_close_request: false,
        #[cfg(target_os = "linux")]
        platform_specific: window::settings::PlatformSpecific {
            application_id: APP_ID.to_owned(),
            ..Default::default()
        },
        #[cfg(windows)]
        platform_specific: window::settings::PlatformSpecific {
            skip_taskbar: true,
            ..Default::default()
        },
        ..window::Settings::default()
    }
}

fn settings_window() -> window::Settings {
    window::Settings {
        size: SETTINGS,
        position: window::Position::Centered,
        resizable: false,
        #[cfg(target_os = "linux")]
        platform_specific: window::settings::PlatformSpecific {
            application_id: APP_ID.to_owned(),
            ..Default::default()
        },
        ..window::Settings::default()
    }
}

/// The pet window's top-left corner on the (primary) monitor: at its bottom, [`START_RIGHT`] from its right edge.
fn start_position(window: Size, monitor: Size) -> Point {
    Point::new(
        monitor.width - window.width - START_RIGHT,
        monitor.height - window.height,
    )
}

fn level(on_top: bool) -> window::Level {
    if on_top {
        window::Level::AlwaysOnTop
    } else {
        window::Level::Normal
    }
}

/// Runs a native call with the window, on the event-loop thread (see `native`); only a failure comes back, as
/// [`Message::Native`].
fn native_call(
    id: window::Id,
    what: &'static str,
    call: impl FnOnce(&dyn window::Window) -> Result<(), NativeError> + Send + 'static,
) -> Task<Message> {
    window::run(id, call).then(move |result| match result {
        Ok(()) => Task::none(),
        Err(e) => Task::done(Message::Native(what, e)),
    })
}

/// The pet window's frames: each of its `RedrawRequested`, as the time it was drawn (Settings' frames don't count,
/// or it would speed the pet up).
fn frames_of(pet: window::Id) -> Subscription<Message> {
    event::listen_raw(|event, _status, id| match event {
        Event::Window(window::Event::RedrawRequested(at)) => Some((id, at)),
        _ => None,
    })
    .with(pet)
    .filter_map(|(pet, (id, at))| (id == pet).then_some(Message::Frame(at)))
}

fn env_is_1(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| v == "1")
}

fn press_target(on_sprite: bool, status: event::Status) -> &'static str {
    match (on_sprite, status) {
        (_, event::Status::Captured) => "taken by a widget",
        (true, _) => "on the sprite",
        (false, _) => "not on the sprite",
    }
}

/// The rectangles' bounding box, for the log.
fn describe(rects: &[PxRect]) -> String {
    let Some(first) = rects.first() else {
        return "empty".to_owned();
    };
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x + first.w, first.y + first.h);
    for r in rects {
        (x0, y0) = (x0.min(r.x), y0.min(r.y));
        (x1, y1) = (x1.max(r.x + r.w), y1.max(r.y + r.h));
    }
    format!("within {x0},{y0} {}x{} px", x1 - x0, y1 - y0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_starts_at_the_bottom_left_of_the_monitors_corner() {
        let at = start_position(SURFACE, Size::new(1920.0, 1080.0));
        assert_eq!(at, Point::new(1920.0 - 380.0 - 420.0, 1080.0 - 600.0));
    }
}
