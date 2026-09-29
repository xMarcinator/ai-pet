//! The desktop shell: an iced daemon with the pet in a transparent, undecorated, always-on-top window.
//!
//! The window is [`SURFACE`] (380 × 600). It opens hidden, so the native setup (utility window, keep above, out of
//! the taskbar) is in place before the window manager sees it, and shows near the primary monitor's bottom-right
//! corner. Its input region is [`PetUi::hit_rects`] in physical pixels, so everything else clicks through; where the
//! platform also clips drawing to a region, that one is [`PetUi::drawn_rects`]. What is drawn is sent as soon as it
//! changes, and the input region with it; a change to the input region alone waits 100 ms since the last (as
//! MainWindow.OnFrame does). A left press on the sprite drags the window after the pointer, read from the desktop
//! every frame; a press that doesn't move pokes the pet. A right press on the sprite opens the menu above it, inside
//! the window and its input region. Settings is an ordinary window.
//!
//! The pet moves on every [`PetUi::frame_interval`] (60 or 30 times a second). iced's winit shell redraws every open
//! window after each message, so an open Settings window is drawn at that rate too.

use std::time::{Duration, Instant};

use aipet_ui::{Effect, INLINE_MENU, PetUi, SURFACE};
use iced::widget::Space;
use iced::{Color, Element, Event, Point, Size, Subscription, Task, Theme, event, mouse, theme, time, window};

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
/// How long a change to the input region alone waits since the last update (MainWindow.OnFrame's 0.1 s).
const REGION_EVERY: Duration = Duration::from_millis(100);
/// How long a drag waits for its release once the desktop says the button is up: the release event can come a
/// frame later. After that the release went elsewhere (a pointer grab, a lost capture) and the drag is let go.
const RELEASE_GRACE: Duration = Duration::from_millis(100);
/// "Always on top" is said again this often while it is on (MainWindow's KeepOnTop timer).
const KEEP_ON_TOP_EVERY: Duration = Duration::from_secs(2);
/// macOS: the pointer is tested against the input region this often.
#[cfg(target_os = "macos")]
const HIT_TEST_EVERY: Duration = Duration::from_millis(33);

#[derive(Debug, Clone)]
enum Message {
    Ui(aipet_ui::Message),
    /// Time to move everything on (every [`PetUi::frame_interval`]).
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
    /// AIPET_DEBUG: the lines [`describe_gpu`] wrote.
    Gpu(Vec<String>),
}

struct Shell {
    ui: PetUi,
    pet: window::Id,
    /// The pet window's scale factor (physical px per logical px), once it is shown.
    scale: Option<f32>,
    /// The regions the window has, and when they were set.
    region: Option<Region>,
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
    /// Since when the desktop has said the button is up, while its release hasn't come.
    up_since: Option<Instant>,
}

impl Drag {
    /// Takes whether the desktop has the button held at `now`, and says whether its release is lost: the button has
    /// been up for [`RELEASE_GRACE`] and no release came.
    fn release_lost(&mut self, held: bool, now: Instant) -> bool {
        if held {
            self.up_since = None;
            return false;
        }
        let up_since = *self.up_since.get_or_insert(now);
        now.saturating_duration_since(up_since) >= RELEASE_GRACE
    }
}

/// What the pet window was last told, in physical px.
#[derive(PartialEq)]
struct Region {
    /// Where it takes the mouse.
    input: Vec<PxRect>,
    /// Where it is drawn on at all.
    drawn: Vec<PxRect>,
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
        // read after every update: the pointer and a drag change it at once
        let frames = time::every(self.ui.frame_interval()).map(Message::Frame);
        Subscription::batch([frames, input])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PetOpened => {
                let (pet, on_top) = (self.pet, self.ui.on_top());
                let setup = native_call(pet, "setting up the pet window", move |w| {
                    native::setup_pet_window(w, on_top)
                });
                // AIPET_DEBUG's GPU probe blocks the event loop for up to seconds, so it runs while the window is
                // still hidden: input that queued up behind it came out of order (a press with no position)
                let gpu = if self.debug.is_some() {
                    window::run(pet, describe_gpu).map(Message::Gpu)
                } else {
                    Task::none()
                };
                // then, whatever came of it, show it, and say "above" again now that it is mapped (winit said it
                // while it was not)
                setup.chain(gpu).chain(Task::batch([
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
            Message::Gpu(lines) => {
                for line in lines {
                    self.log(|| line);
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
        // `now` is when the tick was due, and a tick can come late: the region's wait is counted from when it really
        // went out
        let mut tasks = vec![self.follow_pointer(now, scale), self.sync_region(Instant::now(), false)];
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
            // a click elsewhere took the focus: the menu goes, as a menu does. So does a drag: whatever took the focus
            // (the Start menu, Alt+Tab) takes its release too, and on Windows the button's state was then seen to
            // stay held
            Event::Window(window::Event::Unfocused) => {
                if self.drag.take().is_some() {
                    self.ui.update(aipet_ui::Message::DragCancelled);
                    self.log(|| "the pet lost the focus: drag cancelled".to_owned());
                }
                return self.set_menu(false);
            }
            // it has no title bar, but Alt+F4 and the like quit
            Event::Window(window::Event::CloseRequested | window::Event::Closed) => return iced::exit(),
            _ => {}
        }
        Task::none()
    }

    /// A press on the pet window. Anywhere but on the menu it closes the menu, if that shows, and does nothing else
    /// (a bubble it lands on takes it all the same). Otherwise, unless a widget took it, a left press on the sprite
    /// starts a drag and a right one opens the menu.
    fn press(&mut self, button: mouse::Button, status: event::Status) -> Task<Message> {
        let at = self.ui.pointer();
        let on_sprite = at.is_some_and(|p| self.ui.sprite_hit(p));
        self.log(|| format!("{button:?} press at {at:?}: {}", press_target(on_sprite, status)));
        // a press on the menu is its own: an item is chosen on the release, so the menu stays till then
        if self.menu && !at.is_some_and(|p| INLINE_MENU.contains(p)) {
            return self.set_menu(false);
        }
        // a bubble, its dismiss button or a menu item took it
        if status == event::Status::Captured {
            return Task::none();
        }
        match button {
            mouse::Button::Left if on_sprite => {
                let scale = self.scale.unwrap_or(1.0);
                let pressed = native::pointer(scale)
                    .map_err(|e| self.report("reading the pointer", &e))
                    .ok()
                    .map(|p| p.at);
                self.drag = Some(Drag {
                    pressed,
                    window: None,
                    last: None,
                    up_since: None,
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
    /// positions move with the window), so the pet stays under it without lagging or overshooting. If the desktop
    /// says the button is up and no release comes within [`RELEASE_GRACE`], the drag is cancelled.
    fn follow_pointer(&mut self, now: Instant, scale: f32) -> Task<Message> {
        let Some(drag) = &mut self.drag else {
            return Task::none();
        };
        let (Some(pressed), Ok(pointer)) = (drag.pressed, native::pointer(scale)) else {
            return Task::none();
        };
        if drag.release_lost(pointer.left_held, now) {
            self.drag = None;
            self.ui.update(aipet_ui::Message::DragCancelled);
            self.log(|| "the button is up and no release came: drag cancelled".to_owned());
            return Task::none();
        }
        // up, with its release on the way, or moved nowhere
        if !pointer.left_held || drag.last == Some(pointer.at) {
            return Task::none();
        }
        drag.last = Some(pointer.at);
        let from_press = pointer.at - pressed;
        self.ui.update(aipet_ui::Message::DragMoved(from_press));
        match drag.window {
            Some(window) if self.ui.drag_moved() => window::move_to(self.pet, window + from_press),
            _ => Task::none(),
        }
    }

    /// Hands the window the pet's hit rectangles as its input region, and what it draws on as the region drawn at
    /// all. A change to what is drawn goes at once, or a region that clips drawing would cut off what moved out of
    /// it; a change to the input region alone waits for [`REGION_EVERY`] since the last, unless `force`.
    fn sync_region(&mut self, now: Instant, force: bool) -> Task<Message> {
        let Some(scale) = self.scale else {
            return Task::none();
        };
        let drawn = native::to_physical(&self.ui.drawn_rects(), scale, 0.0);
        let due = force || now.saturating_duration_since(self.region_at) >= REGION_EVERY;
        if !due && self.region.as_ref().is_some_and(|r| r.drawn == drawn) {
            return Task::none();
        }
        let region = Region {
            input: native::to_physical(&self.ui.hit_rects(), scale, 0.0),
            drawn,
        };
        if self.region.as_ref() == Some(&region) {
            return Task::none();
        }
        if self.region.as_ref().is_none_or(|r| r.input != region.input) {
            self.log(|| {
                format!(
                    "input region: {} rects, {}",
                    region.input.len(),
                    describe(&region.input)
                )
            });
        }
        let (input, drawn) = (region.input.clone(), region.drawn.clone());
        self.region = Some(region);
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

/// AIPET_DEBUG: which GPU draws the pet, and what its surface offers. iced keeps its choice to itself, so this asks
/// wgpu again the way iced_wgpu 0.14 does (`window/compositor.rs`): the backends and the power preference from the
/// environment (all, and high performance, without), an adapter that can draw on the pet window, and its surface's
/// alpha modes, of which iced asks for PostMultiplied, else PreMultiplied, else Auto, which wgpu takes as Opaque (or
/// Inherit). The modes don't settle transparency on Windows: Vulkan and GL surfaces that offer only Opaque showed the
/// desktop through, a DX12 one didn't (rust/proofs/windows.md).
fn describe_gpu(w: &dyn window::Window) -> Vec<String> {
    use iced::advanced::graphics::color::GAMMA_CORRECTION;
    use iced::wgpu::{
        AdapterInfo, Backends, CompositeAlphaMode, Instance, InstanceDescriptor, InstanceFlags, PowerPreference,
        RequestAdapterOptions, SurfaceTargetUnsafe,
    };

    let describe = |i: &AdapterInfo| {
        format!(
            "{:?} {} ({:?}, driver {} {})",
            i.backend, i.name, i.device_type, i.driver, i.driver_info
        )
    };
    let backends = Backends::from_env().unwrap_or(Backends::all());
    let power = PowerPreference::from_env().unwrap_or(PowerPreference::HighPerformance);
    let instance = Instance::new(&InstanceDescriptor {
        backends,
        flags: InstanceFlags::empty(),
        ..Default::default()
    });
    let all: Vec<String> = instance
        .enumerate_adapters(backends)
        .iter()
        .map(|a| describe(&a.get_info()))
        .collect();
    let mut lines = vec![format!("GPU adapters for {backends:?}: {}", all.join("; "))];
    // SAFETY: the window outlives the surface, which is dropped before this returns
    let surface = match unsafe { SurfaceTargetUnsafe::from_window(&w) }
        .map_err(|e| e.to_string())
        .and_then(|target| unsafe { instance.create_surface_unsafe(target) }.map_err(|e| e.to_string()))
    {
        Ok(surface) => surface,
        Err(e) => {
            lines.push(format!("GPU: no surface for the pet window: {e}"));
            return lines;
        }
    };
    let options = RequestAdapterOptions {
        power_preference: power,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    };
    let adapter = match ready(instance.request_adapter(&options)) {
        Some(Ok(adapter)) => adapter,
        Some(Err(e)) => {
            lines.push(format!("GPU: no adapter for the pet window ({power:?}): {e}"));
            return lines;
        }
        None => {
            lines.push("GPU: the adapter request didn't finish at once".to_owned());
            return lines;
        }
    };
    let caps = surface.get_capabilities(&adapter);
    let alpha = &caps.alpha_modes;
    let asked = [CompositeAlphaMode::PostMultiplied, CompositeAlphaMode::PreMultiplied]
        .into_iter()
        .find(|mode| alpha.contains(mode))
        .unwrap_or(CompositeAlphaMode::Auto);
    let gets = match asked {
        CompositeAlphaMode::Auto => alpha
            .iter()
            .find(|mode| matches!(mode, CompositeAlphaMode::Opaque | CompositeAlphaMode::Inherit)),
        _ => Some(&asked),
    };
    let format = caps
        .formats
        .iter()
        .filter(|format| format.required_features().is_empty())
        .find(|format| format.is_srgb() == GAMMA_CORRECTION)
        .or(caps.formats.first());
    lines.push(format!(
        "GPU for the pet: {} ({power:?}); surface alpha modes {alpha:?}: iced asks {asked:?}, which is {gets:?}; format {format:?} of {:?}",
        describe(&adapter.get_info()),
        caps.formats
    ));
    lines
}

/// A future's output if it is ready at once, as wgpu's are on the desktop.
fn ready<F: Future>(future: F) -> Option<F::Output> {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match std::pin::pin!(future).poll(&mut context) {
        std::task::Poll::Ready(output) => Some(output),
        std::task::Poll::Pending => None,
    }
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

    /// A shell whose pet window is shown at scale 1.
    fn shown() -> Shell {
        let (mut shell, _) = Shell::boot();
        shell.scale = Some(1.0);
        shell
    }

    #[test]
    fn what_is_drawn_is_never_cut_off_and_the_input_region_is_at_most_100_ms_old() {
        let mut shell = shown();
        let start = Instant::now();
        let (mut updates, mut drawn_only) = (0, 0);
        // the demo's 40 s, at 60 fps
        for i in 0..2400 {
            let now = start + Duration::from_secs_f64(f64::from(i) / 60.0);
            shell.ui.tick(now);
            let before = shell.region_at;
            let _ = shell.sync_region(now, false);
            let region = shell.region.as_ref().unwrap();
            assert_eq!(region.drawn, native::to_physical(&shell.ui.drawn_rects(), 1.0, 0.0));
            if region.input != native::to_physical(&shell.ui.hit_rects(), 1.0, 0.0) {
                assert!(now - shell.region_at < REGION_EVERY, "at frame {i}");
            }
            if shell.region_at != before {
                updates += 1;
                if now - before < REGION_EVERY {
                    drawn_only += 1;
                }
            }
        }
        // the sprite bobs, hops and waves, and the bubbles come and go: it changes often, but not every frame
        assert!(updates > 100 && updates < 2400, "{updates} updates");
        assert!(drawn_only > 0, "no update came sooner than {REGION_EVERY:?}");
    }

    #[test]
    fn a_drag_whose_release_never_comes_is_let_go() {
        let t = Instant::now();
        let ms = |n| t + Duration::from_millis(n);
        let mut drag = Drag {
            pressed: None,
            window: None,
            last: None,
            up_since: None,
        };
        assert!(!drag.release_lost(true, t));
        // up, but its release may be a frame behind
        assert!(!drag.release_lost(false, ms(16)));
        assert!(!drag.release_lost(false, ms(99)));
        // held again: it starts over
        assert!(!drag.release_lost(true, ms(110)));
        assert!(!drag.release_lost(false, ms(130)));
        assert!(!drag.release_lost(false, ms(200)));
        assert!(drag.release_lost(false, ms(230)));
    }

    #[test]
    fn a_drag_is_let_go_when_the_pet_loses_the_focus() {
        let mut shell = shown();
        shell.drag = Some(Drag {
            pressed: Some(Point::ORIGIN),
            window: Some(Point::ORIGIN),
            last: None,
            up_since: None,
        });
        shell.ui.update(aipet_ui::Message::DragStarted);
        shell
            .ui
            .update(aipet_ui::Message::DragMoved(iced::Vector::new(50.0, 0.0)));
        assert!(shell.ui.drag_moved());
        // the Start menu, Alt+Tab or another app took the focus, and with it the release
        let _ = shell.pet_input(Event::Window(window::Event::Unfocused), event::Status::Ignored);
        assert!(shell.drag.is_none());
        assert!(!shell.ui.drag_moved());
    }

    #[test]
    fn a_press_beside_the_open_menu_closes_it_even_when_a_bubble_takes_it() {
        let mut shell = shown();
        let _ = shell.set_menu(true);
        // a bubble above the menu took it
        let _ = shell
            .ui
            .update(aipet_ui::Message::PointerMoved(Point::new(190.0, 200.0)));
        let _ = shell.press(mouse::Button::Left, event::Status::Captured);
        assert!(!shell.menu);
        // nothing took it
        let _ = shell.set_menu(true);
        let _ = shell.press(mouse::Button::Left, event::Status::Ignored);
        assert!(!shell.menu);
    }

    #[test]
    fn a_press_on_the_menu_leaves_it_open_for_the_item_to_be_chosen() {
        let mut shell = shown();
        let _ = shell.set_menu(true);
        let _ = shell.ui.update(aipet_ui::Message::PointerMoved(INLINE_MENU.center()));
        // an item (or the menu's padding) takes the press, and the item is chosen on the release
        let _ = shell.press(mouse::Button::Left, event::Status::Captured);
        assert!(shell.menu);
    }
}
