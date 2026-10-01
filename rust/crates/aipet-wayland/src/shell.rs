//! The Wayland shell: an iced_exwlshell daemon with the pet on a layer surface, its menu in an xdg popup and
//! Settings in an xdg window.
//!
//! The pet's surface is [`SURFACE`] (380 × 600), anchored to its output's bottom-right corner on the Top layer
//! (Bottom when it isn't to stay on top), never taking the keyboard. At rest its input region is exactly
//! [`PetUi::hit_rects`] (brought up to date at most every 100 ms, as in the C#), so everything but the pet and its
//! bubbles clicks through. A left drag on the sprite moves it, one of two ways (`AIPET_DRAG`, see [`crate::drag`]);
//! a right press on the sprite opens the menu above it, in a popup the compositor dismisses on a click elsewhere, or
//! drawn in the pet's surface when a popup doesn't appear in time; a press on the pet closes either, and does nothing
//! else. Settings opens in a window of its own, one at a time (`AIPET_OPEN_SETTINGS=1` opens it at start).
//!
//! The pet moves on every [`PetUi::frame_interval`] (30 or 60 times a second), and only its surface is drawn for
//! that: its own frames and the pointer's news draw nothing, so it doesn't draw at the display's rate. The core's news
//! is picked up on each of those frames and handed to the pet. The app's hooks ([`aipet_ui::Host`]) hear of a drop,
//! a reset and the quit; where a drop or a reset leaves the surface, in desktop pixels, isn't worked out yet, nor is
//! the place the pet was left read at start.
//!
//! The runtime knows a surface by its id only once it exists, and an action for an id that never appears waits
//! forever: windows are closed only once they have appeared, and a popup or Settings that doesn't appear in time is
//! given up on (and closed if it turns up after all). The daemon starts with no surface of its own and outlives the
//! pet's, which is made again when its output goes away.

use std::collections::BTreeSet;
use std::sync::Mutex;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use aipet_ui::{Effect, INLINE_MENU, Launch, MENU, News, PetUi, Rect, SPRITE, SURFACE};
use iced::widget::{Space, container, pin};
use iced::{Color, Element, Event, Point, Rectangle, Subscription, Task, Theme, event, mouse, theme, time, window};
use iced_exwlshell::actions::{ActionCallback, IcedNewPopupSettings, IcedXdgWindowSettings};
use iced_exwlshell::redraw::Scope;
use iced_exwlshell::reexport::{
    Anchor, KeyboardInteractivity, Layer, LayerSize, NewLayerShellSettings, OutputOption, PixelSize, PopupAnchor,
    PopupConstraintAdjustment, PopupGravity,
};
use iced_exwlshell::settings::{LayerShellSettings, Settings, StartMode};
use iced_exwlshell::shell::{ShellEvent, ShellReceiver};
use iced_exwlshell::to_layer_message;
use wayland_client::Connection;

use crate::drag::{Action, Drag, Home, Input, Strategy};

/// The layer-shell namespace, which compositor rules match.
const NAMESPACE: &str = "aipet";
/// Where the surface starts: this far left of the output's bottom-right corner (PlaceWindow's default).
const START: Home = Home { right: 24, bottom: 0 };
/// Settings' title and size, as the .NET pet's (SettingsWindow.axaml): the packaged window rules match the title.
const SETTINGS_TITLE: &str = "AiPet · Settings";
const SETTINGS_SIZE: (u32, u32) = (800, 640);
/// How long the menu's popup and Settings may take to appear before they are given up on (Settings is the first
/// surface when it opens at start, and waits for the renderer).
const POPUP_PATIENCE: Duration = Duration::from_millis(500);
const WINDOW_PATIENCE: Duration = Duration::from_secs(3);
/// How long the menu drawn in the pet's surface stays open with the pointer off the surface: the surface hears no
/// click elsewhere, so this is how it closes when you go on with something else.
const INLINE_MENU_PATIENCE: Duration = Duration::from_secs(1);
/// How often, while the pet has no surface, a new one is tried.
const RETRY: Duration = Duration::from_secs(1);
/// The input region changes at most this often (MainWindow.OnFrame's 0.1 s): the hit rects follow the sprite's
/// breathing and bobbing, and each change is a commit of its own.
const REGION_EVERY: Duration = Duration::from_millis(100);

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
enum Message {
    Ui(aipet_ui::Message),
    /// Time to move everything on, and draw the pet ([`PetUi::frame_interval`]).
    Frame(Instant),
    /// A pointer, frame, size or close event on the pet's surface, and whether a widget took it.
    Pet(Event, event::Status),
    /// The drag's last changes to the pet's surface have been made: this comes after them in one task, and the
    /// runtime makes (and commits) each change as it comes.
    Committed,
    /// Surfaces made and closed, outputs, and which output the pet is on.
    Shell(Box<ShellEvent>),
    /// The pet has no surface: time to try making one.
    Retry,
}

/// Where the pet's surface is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Surface {
    Opening,
    Live,
    /// Closed (its output went away, or the compositor had no output for it): made again once an output is there.
    Gone,
}

/// A popup or window asked for: whether it has appeared, and until when it may.
#[derive(Clone, Copy, Debug)]
struct Window {
    id: window::Id,
    live: bool,
    deadline: Instant,
}

impl Window {
    fn asked(id: window::Id, until: Instant) -> Window {
        Window {
            id,
            live: false,
            deadline: until,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Menu {
    Popup(Window),
    /// Drawn in the pet's surface; `away`: since when the pointer has been off the surface.
    Inline {
        away: Option<Instant>,
    },
}

/// The input region the surface has: when it was sent, and whether the menu drawn in the surface was open then.
struct Region {
    rects: Vec<Rect>,
    at: Instant,
    menu: bool,
}

impl Region {
    /// Whether a new region may go out at `now`, with the menu drawn in the surface open or not: after
    /// [`REGION_EVERY`], or at once when the menu opened or closed (it takes clicks from the moment it shows).
    fn stale(&self, now: Instant, menu: bool) -> bool {
        self.menu != menu || now.saturating_duration_since(self.at) >= REGION_EVERY
    }
}

struct Shell {
    ui: PetUi,
    /// The core's news, handed to the pet every frame.
    news: Receiver<News>,
    pet: window::Id,
    surface: Surface,
    drag: Drag,
    /// The input region the surface has, to send a new one only when it differs.
    region: Option<Region>,
    shell: ShellReceiver,
    /// The outputs connected, by registry name.
    outputs: BTreeSet<u32>,
    /// The output the pet's surface is on.
    output: Option<u32>,
    menu: Option<Menu>,
    settings: Option<Window>,
    /// AIPET_DEBUG=1: log what happens to the surfaces, the pointer's presses and the drags to stderr, stamped with
    /// the time since this start.
    debug: Option<Instant>,
}

/// Runs the pet the app handed over ([`Launch::hand_over`]) on `connection`, which must offer layer-shell (see
/// `probe`).
pub fn run(connection: Connection) -> Result<(), String> {
    let launch = Launch::take().ok_or("no pet was handed over to the Wayland shell")?;
    let strategy = Strategy::from_env()?;
    let open_settings = std::env::var_os("AIPET_OPEN_SETTINGS").is_some_and(|v| v == "1");
    // nothing to type anywhere (Settings has switches and buttons): spare the clipboard's worker thread
    iced_exwlshell::disable_clipboard();
    // known before the daemon starts, so the 'static redraw scope can name it
    let pet = window::Id::unique();
    let (shell_tx, shell_rx) = iced_exwlshell::shell::channel();
    // the daemon boots once
    let launch = Mutex::new(Some(launch));
    iced_exwlshell::daemon(
        move || {
            let launch = launch.lock().ok().and_then(|mut launch| launch.take());
            let launch = launch.expect("the Wayland shell boots once");
            Shell::boot(pet, strategy, shell_rx.clone(), open_settings, launch)
        },
        NAMESPACE,
        Shell::update,
        Shell::view,
    )
    .title(Shell::title)
    // one clear colour for every surface: transparent (Settings paints its own background)
    .style(|_: &Shell, theme: &Theme| theme::Style {
        background_color: Color::TRANSPARENT,
        text_color: theme.palette().text,
    })
    .subscription(Shell::subscription)
    .redraw_scope(move |message: &Message| redraw_scope(pet, message))
    .settings(Settings {
        layer_settings: LayerShellSettings {
            // no surface of its own at start (the pet's is made in boot, with a known id), and the daemon doesn't
            // end when a surface closes
            start_mode: StartMode::Background,
            ..LayerShellSettings::default()
        },
        with_connection: Some(connection.into()),
        shell_broadcast: shell_tx,
        ..Settings::default()
    })
    .run()
    .map_err(|e| e.to_string())
}

impl Shell {
    fn boot(
        pet: window::Id,
        strategy: Strategy,
        shell: ShellReceiver,
        open_settings: bool,
        launch: Launch,
    ) -> (Self, Task<Message>) {
        let debug = std::env::var_os("AIPET_DEBUG")
            .is_some_and(|v| v == "1")
            .then(Instant::now);
        let mut me = Shell {
            ui: launch.ui,
            news: launch.news,
            pet,
            surface: Surface::Gone,
            drag: Drag::new(strategy, START),
            region: None,
            shell,
            outputs: BTreeSet::new(),
            output: None,
            menu: None,
            settings: None,
            debug,
        };
        me.log(|| format!("dragging by {strategy:?}"));
        let mut tasks = vec![me.open_pet()];
        if open_settings {
            tasks.push(me.open_settings(Instant::now()));
        }
        (me, Task::batch(tasks))
    }

    fn title(&self, id: window::Id) -> Option<String> {
        self.settings
            .is_some_and(|w| w.id == id)
            .then(|| SETTINGS_TITLE.to_owned())
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            // two intervals only, so the timer is made anew only when the pet calms down or livens up
            time::every(self.ui.frame_interval()).map(Message::Frame),
            pet_events(self.pet),
            self.shell.listen().map(|e| Message::Shell(Box::new(e))),
        ];
        if self.drag.active() {
            subscriptions.push(drag_events(self.pet));
        }
        if self.surface == Surface::Gone {
            subscriptions.push(time::every(RETRY).map(|_| Message::Retry));
        }
        Subscription::batch(subscriptions)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Frame(now) => {
                while let Ok(news) = self.news.try_recv() {
                    self.ui.update(news.into());
                }
                self.ui.tick(now);
                self.expire(now);
                // `now` is when the tick was due, and a tick can come late: the region's wait is counted from when
                // it really went out
                self.sync_region(Instant::now())
            }
            Message::Ui(message) => {
                // choosing an item closes the menu
                let close = match message {
                    aipet_ui::Message::Menu(_) => self.close_menu(),
                    _ => Task::none(),
                };
                let effect = self.ui.update(message).map_or_else(Task::none, |e| self.apply(e));
                Task::batch([close, effect, self.sync_region(Instant::now())])
            }
            Message::Pet(event, status) => self.pet_event(event, status),
            Message::Committed => self.drag_input(Input::Committed),
            Message::Shell(event) => self.shell_event(*event),
            Message::Retry if self.surface == Surface::Gone && !self.outputs.is_empty() => self.open_pet(),
            // the layer-shell actions the macro adds are taken by the runtime and never get here
            _ => Task::none(),
        }
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        if id == self.pet {
            // the pet's box, where the drag draws it (the surface may be the whole output)
            let pet = container(self.ui.pet_view().map(Message::Ui))
                .width(SURFACE.width)
                .height(SURFACE.height);
            return pin(pet).position(Point::ORIGIN + self.drag.offset()).into();
        }
        if let Some(Menu::Popup(w)) = self.menu
            && w.id == id
        {
            return self.ui.menu_view().map(Message::Ui);
        }
        if self.settings.is_some_and(|w| w.id == id) {
            return self.ui.settings_view().map(Message::Ui);
        }
        Space::new().into()
    }

    /// Makes the pet's surface, at home: click-through until the first input region arrives.
    fn open_pet(&mut self) -> Task<Message> {
        self.surface = Surface::Opening;
        self.region = None;
        self.end_drag();
        let margin = self.drag.margins();
        self.log(|| format!("opening the pet's surface, margins {margin:?}"));
        Task::done(Message::NewLayerShell {
            settings: NewLayerShellSettings {
                size: LayerSize::px(SURFACE.width as u32, SURFACE.height as u32),
                layer: if self.ui.on_top() { Layer::Top } else { Layer::Bottom },
                anchor: Anchor::Bottom | Anchor::Right,
                // margins from the output's edges, whatever panels reserve
                exclusive_zone: Some(-1),
                margin: Some(margin),
                keyboard_interactivity: KeyboardInteractivity::None,
                output_option: OutputOption::Active,
                events_transparent: true,
                namespace: Some(NAMESPACE.to_owned()),
                ..NewLayerShellSettings::default()
            },
            id: self.pet,
        })
    }

    /// Events on the pet's surface: presses on the sprite drag it or open the menu, the drag hears the pointer,
    /// the frames and the sizes, and the surface may close.
    fn pet_event(&mut self, event: Event, status: event::Status) -> Task<Message> {
        match event {
            Event::Mouse(mouse::Event::CursorEntered) => self.log(|| "pointer entered the pet's surface".to_owned()),
            Event::Mouse(mouse::Event::CursorLeft) => self.log(|| "pointer left the pet's surface".to_owned()),
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                // at rest the pet's box is the surface, so the pet's pointer is the surface's
                let at = self.ui.pointer();
                let on_sprite = at.is_some_and(|p| self.ui.sprite_hit(p));
                self.log(|| format!("{button:?} press at {at:?}: {}", press_target(on_sprite, status)));
                // a press anywhere on the pet but on the menu drawn in it closes the menu, and does nothing else, as a
                // click outside a menu does (on the sprite too: no poke, drag or new menu). A popup's grab lets its
                // own client's surfaces hear presses, so this closes the popup as well
                let on_menu =
                    matches!(self.menu, Some(Menu::Inline { .. })) && at.is_some_and(|p| INLINE_MENU.contains(p));
                let closing = self.menu.is_some() && !on_menu;
                let close = if closing { self.close_menu() } else { Task::none() };
                if status == event::Status::Captured {
                    return Task::batch([close, self.sync_region(Instant::now())]);
                }
                let then = match (button, at) {
                    (mouse::Button::Left, Some(at)) if on_sprite && !closing => self.drag_input(Input::Press(at)),
                    (mouse::Button::Right, _) if on_sprite && !closing => self.open_menu(),
                    _ => self.sync_region(Instant::now()),
                };
                return Task::batch([close, then]);
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => return self.drag_input(Input::Motion(position)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => return self.drag_input(Input::Release),
            Event::Window(window::Event::RedrawRequested(at)) => return self.drag_input(Input::Redraw(at)),
            Event::Window(window::Event::Resized(size)) => {
                self.log(|| format!("surface resized to {size:?}"));
                // the region only covers the size the surface had when it was set: set it again
                self.region = None;
                return self.drag_input(Input::Resized(size));
            }
            // also heard when the compositor closes the surface before it ever appeared, which the shell's news
            // never tells
            Event::Window(window::Event::Closed) => self.closed(self.pet),
            _ => {}
        }
        Task::none()
    }

    /// Hands `input` to the drag, tells the pet, and changes the surface as the drag asks.
    fn drag_input(&mut self, input: Input) -> Task<Message> {
        let was = self.drag.describe();
        let step = self.drag.handle(input, Instant::now());
        if was != self.drag.describe() {
            self.log(|| format!("drag: {was} -> {}", self.drag.describe()));
        }
        // a drop asks for the pet's place to be saved
        let effect = step.pet.and_then(|message| self.ui.update(message));
        Task::batch([
            self.change_surface(step.actions, step.confirm),
            self.sync_region(Instant::now()),
            effect.map_or_else(Task::none, |e| self.apply(e)),
        ])
    }

    /// Ends whatever drag was on: the pet, if still held, is let go.
    fn end_drag(&mut self) {
        if let Some(message) = self.drag.reset() {
            self.log(|| "drag lost with the surface".to_owned());
            self.ui.update(message);
        }
    }

    /// The drag's changes to the pet's surface, in order, and then, if it wants to hear (`confirm`), that they have
    /// been made.
    fn change_surface(&self, actions: Vec<Action>, confirm: bool) -> Task<Message> {
        if self.surface != Surface::Live {
            return Task::none();
        }
        if !actions.is_empty() {
            self.log(|| format!("surface: {actions:?}"));
        }
        let id = self.pet;
        let changes = actions.into_iter().fold(Task::none(), |task, action| {
            task.chain(Task::done(match action {
                Action::Margins(top, right, bottom, left) => Message::MarginChange {
                    id,
                    margin: (top, right, bottom, left),
                },
                Action::Home => Message::LayoutChange {
                    id,
                    anchor: Anchor::Bottom | Anchor::Right,
                    size: LayerSize::px(SURFACE.width as u32, SURFACE.height as u32),
                },
                Action::Cover(w, h) => Message::LayoutChange {
                    id,
                    anchor: Anchor::Top | Anchor::Left,
                    size: LayerSize::px(w, h),
                },
            }))
        });
        if confirm {
            changes.chain(Task::done(Message::Committed))
        } else {
            changes
        }
    }

    /// Does what the pet's UI asks of the shell.
    fn apply(&mut self, effect: Effect) -> Task<Message> {
        match effect {
            Effect::OnTop(on) if self.surface == Surface::Live => Task::done(Message::LayerChange {
                id: self.pet,
                layer: if on { Layer::Top } else { Layer::Bottom },
            }),
            // a surface on its way gets the layer when it is made
            Effect::OnTop(_) => Task::none(),
            Effect::OpenSettings => self.open_settings(Instant::now()),
            Effect::Quit => {
                self.log(|| "quitting".to_owned());
                self.ui.host().quit();
                iced::exit()
            }
            Effect::Dropped => {
                self.log(|| format!("dropped, margins {:?}", self.drag.margins()));
                Task::none()
            }
            Effect::ResetPosition => {
                self.log(|| "reset position".to_owned());
                self.ui.host().reset_place();
                Task::none()
            }
        }
    }

    /// Opens the menu above the sprite: a popup, which the compositor places (below the sprite where there is no
    /// room above) and dismisses on a click elsewhere, opened on the press so its grab has the press to go by.
    fn open_menu(&mut self) -> Task<Message> {
        // one menu at a time, and none while the surface is being moved
        if self.menu.is_some() || self.drag.active() {
            return Task::none();
        }
        let id = window::Id::unique();
        self.menu = Some(Menu::Popup(Window::asked(id, Instant::now() + POPUP_PATIENCE)));
        self.log(|| format!("menu popup {id:?} asked for"));
        // flipped below the sprite at the top of the output, but never slid along it: Hyprland 0.56 slides a layer
        // surface's popup by the wrong amount while the surface is still being animated (seen right after it maps),
        // and flips alone land where asked. So the anchor is moved sideways here instead (a flip on x does nothing
        // for a centred menu)
        let anchor = menu_anchor(self.drag.placed());
        let size = PixelSize::px(MENU.width as u32, MENU.height as u32);
        let anchor_size = PixelSize::px(anchor.width as u32, anchor.height as u32);
        Task::done(Message::NewPopUp {
            settings: IcedNewPopupSettings::new(self.pet, size, (anchor.x as i32, anchor.y as i32), anchor_size)
                .anchor(PopupAnchor::Top)
                .gravity(PopupGravity::Top)
                .constraint_adjustment(PopupConstraintAdjustment::FlipY),
            id,
        })
    }

    /// Closes the menu. A popup that hasn't appeared can't be closed yet: it is forgotten, and closed if it turns
    /// up.
    fn close_menu(&mut self) -> Task<Message> {
        match self.menu.take() {
            Some(Menu::Popup(w)) if w.live => window::close(w.id),
            Some(Menu::Inline { .. }) => {
                self.ui.set_inline_menu(false);
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Opens Settings, or brings it to the front: nothing here can raise a window (no xdg-activation), and a new
    /// one comes up in front, so an open one is closed and opened anew.
    fn open_settings(&mut self, now: Instant) -> Task<Message> {
        let close = match self.settings {
            // on its way
            Some(Window { live: false, .. }) => return Task::none(),
            Some(Window { id, .. }) => {
                self.log(|| "Settings is open: opening it anew, in front".to_owned());
                window::close(id)
            }
            None => Task::none(),
        };
        let (id, open) = Message::base_window_open(IcedXdgWindowSettings {
            size: Some(PixelSize::px(SETTINGS_SIZE.0, SETTINGS_SIZE.1)),
            client_side_decorations: false,
        });
        self.settings = Some(Window::asked(id, now + WINDOW_PATIENCE));
        self.log(|| format!("Settings {id:?} asked for"));
        Task::batch([close, open])
    }

    /// Gives up on a popup or Settings that hasn't appeared by now. A menu popup that didn't is drawn in the pet's
    /// surface instead, this time (the next menu tries a popup again). The menu drawn there closes once the pointer
    /// has been off the surface for [`INLINE_MENU_PATIENCE`].
    fn expire(&mut self, now: Instant) {
        match self.menu {
            Some(Menu::Popup(w)) if !w.live && now > w.deadline => {
                self.log(|| "no popup appeared: the menu is drawn in the pet's surface".to_owned());
                self.menu = Some(Menu::Inline { away: None });
                self.ui.set_inline_menu(true);
            }
            Some(Menu::Inline { away }) => {
                let away = self.ui.pointer().is_none().then(|| away.unwrap_or(now));
                if away.is_some_and(|since| now.saturating_duration_since(since) > INLINE_MENU_PATIENCE) {
                    self.log(|| "the pointer went away: the menu drawn in the pet's surface closes".to_owned());
                    self.menu = None;
                    self.ui.set_inline_menu(false);
                } else {
                    self.menu = Some(Menu::Inline { away });
                }
            }
            _ => {}
        }
        if self.settings.is_some_and(|w| !w.live && now > w.deadline) {
            self.log(|| "Settings didn't appear: given up on".to_owned());
            self.settings = None;
        }
    }

    fn shell_event(&mut self, event: ShellEvent) -> Task<Message> {
        match event {
            ShellEvent::NewShell(info) => self.appeared(info.window),
            ShellEvent::Closed(id) => {
                self.closed(id);
                Task::none()
            }
            ShellEvent::WindowOutputChanged { window, output } if window == self.pet => {
                self.log(|| format!("the pet is on {:?}", output.as_ref().map(|o| (&o.name, o.logical_size))));
                self.output = output.as_ref().map(|o| o.id);
                self.set_output(output.and_then(|o| o.logical_size))
            }
            ShellEvent::OutputAdded(info) | ShellEvent::OutputUpdated(info) => {
                self.outputs.insert(info.id);
                if Some(info.id) == self.output {
                    return self.set_output(info.logical_size);
                }
                Task::none()
            }
            ShellEvent::OutputRemoved(info) => {
                self.outputs.remove(&info.id);
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// The drag keeps the pet on its output's size.
    fn set_output(&mut self, size: Option<(i32, i32)>) -> Task<Message> {
        let actions = self.drag.set_output(size);
        self.change_surface(actions, false)
    }

    /// A surface appeared. One this shell has given up on is closed.
    fn appeared(&mut self, id: window::Id) -> Task<Message> {
        if id == self.pet {
            self.surface = Surface::Live;
            self.log(|| "the pet's surface is up".to_owned());
            return Task::none();
        }
        let asked = match (&mut self.menu, &mut self.settings) {
            (Some(Menu::Popup(w)), _) | (_, Some(w)) if w.id == id => {
                w.live = true;
                true
            }
            _ => false,
        };
        if asked {
            self.log(|| format!("{id:?} appeared"));
            return Task::none();
        }
        self.log(|| format!("{id:?} appeared after it was given up on: closing it"));
        window::close(id)
    }

    /// A surface closed: the compositor dismissed the menu, the user closed Settings, or the pet's output went
    /// away or never was (its surface is made again, on the output the compositor picks, by the retry timer).
    fn closed(&mut self, id: window::Id) {
        if id == self.pet {
            // heard twice when it had appeared: in the shell's news and in its own events
            if self.surface == Surface::Gone {
                return;
            }
            self.log(|| "the pet's surface closed".to_owned());
            self.surface = Surface::Gone;
            self.output = None;
            self.end_drag();
            // the pointer is off the pet now: the widget that would say so went with the surface
            if self.ui.pointer().is_some() {
                self.ui.update(aipet_ui::Message::PointerLeft);
            }
            // its popup went with it, and the menu drawn in it goes
            if let Some(Menu::Inline { .. }) = self.menu.take() {
                self.ui.set_inline_menu(false);
            }
        } else if matches!(self.menu, Some(Menu::Popup(w)) if w.id == id) {
            self.log(|| "menu closed".to_owned());
            self.menu = None;
        } else if self.settings.is_some_and(|w| w.id == id) {
            self.log(|| "Settings closed".to_owned());
            self.settings = None;
        }
    }

    /// Sends the pet's hit rectangles as the surface's input region, when they changed and the region is
    /// [`Region::stale`] (a new surface or size has none). Only at rest: during a drag the pointer is the surface's
    /// until the release anyway, and the pet's box may be elsewhere on it.
    fn sync_region(&mut self, now: Instant) -> Task<Message> {
        if self.surface != Surface::Live || self.drag.active() {
            return Task::none();
        }
        let menu = matches!(self.menu, Some(Menu::Inline { .. }));
        if self.region.as_ref().is_some_and(|r| !r.stale(now, menu)) {
            return Task::none();
        }
        let rects = self.ui.hit_rects();
        if self.region.as_ref().is_some_and(|r| r.rects == rects) {
            return Task::none();
        }
        self.log(|| format!("input region: {} rects, {}", rects.len(), describe(&rects)));
        self.region = Some(Region {
            rects: rects.clone(),
            at: now,
            menu,
        });
        Task::done(Message::SetInputRegion {
            id: self.pet,
            callback: ActionCallback::new(move |region| {
                for r in &rects {
                    region.add(r.x, r.y, r.width, r.height);
                }
            }),
        })
    }

    fn log(&self, line: impl FnOnce() -> String) {
        if let Some(start) = self.debug {
            eprintln!("aipet [{:7.3}s]: {}", start.elapsed().as_secs_f64(), line());
        }
    }
}

/// What a message draws anew. The pet's ticks draw its surface, and so do its bubbles' clicks and its surface's own
/// events (during a drag the pet may be drawn elsewhere on it). Its frames draw nothing, or each would draw the next,
/// and neither does the pointer's news: the next tick shows it, a [`aipet_ui::SMOOTH_FRAME`] later while the pointer
/// is on the pet. What the menu and Settings send may change any window.
fn redraw_scope(pet: window::Id, message: &Message) -> Scope {
    use aipet_ui::Message as Ui;
    match message {
        Message::Pet(Event::Window(window::Event::RedrawRequested(_)), _)
        | Message::Ui(Ui::PointerMoved(_) | Ui::PointerLeft)
        | Message::Committed
        | Message::Shell(_)
        | Message::Retry => Scope::None,
        Message::Frame(_) | Message::Pet(..) | Message::Ui(Ui::CardPressed(_) | Ui::Dismiss(_)) => Scope::Window(pet),
        _ => Scope::All,
    }
}

/// The pet surface's presses, releases, sizes and close, in one stream that stays for the shell's life: a stream
/// that is replaced drops what it hasn't delivered yet, and a lost release would leave a drag on.
fn pet_events(pet: window::Id) -> Subscription<Message> {
    event::listen_raw(|event, status, id| match event {
        Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::WheelScrolled { .. }) => None,
        Event::Mouse(_) | Event::Window(window::Event::Resized(_) | window::Event::Closed) => Some((id, event, status)),
        _ => None,
    })
    .with(pet)
    .filter_map(|(pet, (id, event, status))| (id == pet).then_some(Message::Pet(event, status)))
}

/// During a drag, also each pointer move and frame of the pet's surface, in one stream so the drag sees them in the
/// order they happened. Presses and releases come in [`pet_events`], so a move may arrive after a release it came
/// before: the drag is over by then and ignores it, and ends at most that move short.
fn drag_events(pet: window::Id) -> Subscription<Message> {
    event::listen_raw(|event, status, id| match event {
        Event::Mouse(mouse::Event::CursorMoved { .. }) | Event::Window(window::Event::RedrawRequested(_)) => {
            Some((id, event, status))
        }
        _ => None,
    })
    .with(pet)
    .filter_map(|(pet, (id, event, status))| (id == pet).then_some(Message::Pet(event, status)))
}

/// Where the menu's popup is anchored on the pet's surface: the sprite's box at rest, the middle of whose top edge
/// the menu grows up from, centred, or whose bottom edge it hangs from when there is no room above. The box is moved
/// sideways, within the surface, as far as keeps the menu on the output, when `placed` (the surface's top-left on
/// its output, and the output's size) is known.
fn menu_anchor(placed: Option<(Point, (i32, i32))>) -> Rectangle {
    let dx = placed.map_or(0.0, |(origin, (width, _))| {
        let middle = origin.x + SPRITE.center_x();
        let half = MENU.width / 2.0;
        // its right edge on the output, then its left one (on an output narrower than the menu, the left wins)
        0f32.min(width as f32 - half - middle).max(half - middle)
    });
    // the protocol wants the anchor inside the surface
    let dx = dx.clamp(-SPRITE.x, SURFACE.width - SPRITE.x - SPRITE.width);
    Rectangle {
        x: SPRITE.x + dx,
        ..SPRITE
    }
}

fn press_target(on_sprite: bool, status: event::Status) -> &'static str {
    match (on_sprite, status) {
        (_, event::Status::Captured) => "taken by a widget",
        (true, _) => "on the sprite",
        (false, _) => "not on the sprite",
    }
}

/// The rectangles' bounding box, for the log.
fn describe(rects: &[Rect]) -> String {
    let Some(first) = rects.first() else {
        return "empty".to_owned();
    };
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x + first.width, first.y + first.height);
    for r in rects {
        (x0, y0) = (x0.min(r.x), y0.min(r.y));
        (x1, y1) = (x1.max(r.x + r.width), y1.max(r.y + r.height));
    }
    format!("within {x0},{y0} {}x{}", x1 - x0, y1 - y0)
}

#[cfg(test)]
mod tests {
    use aipet_ui::{MenuItem, SMOOTH_FRAME, Setup};
    use iced_exwlshell::shell::{ShellInfo, ShellType};

    use super::*;

    /// A shell that has booted with a pet that touches nothing and no news, not yet on a surface.
    fn booted() -> Shell {
        let (_, receiver) = iced_exwlshell::shell::channel();
        let (_, news) = std::sync::mpsc::channel();
        let launch = Launch {
            ui: PetUi::new(Setup::detached()),
            news,
        };
        Shell::boot(window::Id::unique(), Strategy::Margin, receiver, false, launch).0
    }

    const SPRITE_MIDDLE: Point = Point::new(SPRITE.x + SPRITE.width / 2.0, SPRITE.y + SPRITE.height / 2.0);

    /// A shell whose pet's surface is up, a second after it started, with the pointer on the sprite; and that second.
    fn live_shell() -> (Shell, Instant) {
        let mut shell = booted();
        let _ = shell.shell_event(ShellEvent::NewShell(ShellInfo {
            window: shell.pet,
            shell: ShellType::LayerShell,
        }));
        let start = Instant::now();
        for i in 0..60 {
            let _ = shell.update(Message::Frame(start + SMOOTH_FRAME * i));
        }
        let _ = shell.update(Message::Ui(aipet_ui::Message::PointerMoved(SPRITE_MIDDLE)));
        assert!(shell.ui.sprite_hit(SPRITE_MIDDLE));
        (shell, start + SMOOTH_FRAME * 60)
    }

    fn pet(event: Event) -> Message {
        Message::Pet(event, event::Status::Ignored)
    }

    fn press(button: mouse::Button) -> Message {
        pet(Event::Mouse(mouse::Event::ButtonPressed(button)))
    }

    fn appear(shell: &mut Shell, window: window::Id) {
        let _ = shell.shell_event(ShellEvent::NewShell(ShellInfo {
            window,
            shell: ShellType::PopUp,
        }));
    }

    #[test]
    fn the_menu_popup_opens_where_the_inline_menu_is_drawn_and_stays_on_the_output() {
        // the popup for an anchor: centred on the middle of its top edge, growing up
        let popup = |a: Rectangle| Rectangle::new(Point::new(a.center_x() - MENU.width / 2.0, a.y - MENU.height), MENU);
        let output = (1920, 1200);
        let home = Point::new((1920 - 380 - 420) as f32, 600.0);
        for placed in [None, Some((home, output))] {
            assert_eq!(popup(menu_anchor(placed)), INLINE_MENU);
        }
        // the sprite at the output's left edge, then at its right edge: the menu stops at the edge
        let left = Point::new(-SPRITE.x, 600.0);
        let right = Point::new(1920.0 - SPRITE.x - SPRITE.width, 600.0);
        let (at_left, at_right) = (menu_anchor(Some((left, output))), menu_anchor(Some((right, output))));
        assert_eq!(left.x + popup(at_left).x, 0.0);
        assert_eq!(right.x + popup(at_right).x + MENU.width, 1920.0);
        // the anchor stays on the surface, and a flip hangs the menu from the sprite's bottom edge
        for a in [at_left, at_right] {
            assert!(a.x >= 0.0 && a.x + a.width <= SURFACE.width);
            assert_eq!((a.y, a.size()), (SPRITE.y, SPRITE.size()));
        }
    }

    #[test]
    fn the_pets_ticks_draw_only_the_pet_and_its_frames_and_pointer_nothing() {
        use aipet_ui::Message as Ui;
        let id = window::Id::unique();
        let at = Instant::now();
        let scope = |message: Message| redraw_scope(id, &message);
        assert_eq!(scope(Message::Frame(at)), Scope::Window(id));
        assert_eq!(
            scope(pet(Event::Window(window::Event::RedrawRequested(at)))),
            Scope::None
        );
        assert_eq!(scope(Message::Ui(Ui::PointerMoved(SPRITE_MIDDLE))), Scope::None);
        assert_eq!(scope(Message::Ui(Ui::PointerLeft)), Scope::None);
        assert_eq!(scope(Message::Committed), Scope::None);
        let moved = mouse::Event::CursorMoved {
            position: SPRITE_MIDDLE,
        };
        assert_eq!(scope(pet(Event::Mouse(moved))), Scope::Window(id));
        assert_eq!(scope(Message::Ui(Ui::Dismiss("claude"))), Scope::Window(id));
        assert_eq!(scope(Message::Ui(Ui::Menu(MenuItem::Bubbles))), Scope::All);
        assert_eq!(scope(Message::Ui(Ui::SetMusic(true))), Scope::All);
    }

    #[test]
    fn the_input_region_changes_at_most_every_100_ms_but_at_once_for_the_menu() {
        let t = Instant::now();
        let region = Region {
            rects: Vec::new(),
            at: t,
            menu: false,
        };
        assert!(!region.stale(t + Duration::from_millis(50), false));
        assert!(region.stale(t + REGION_EVERY, false));
        assert!(region.stale(t + Duration::from_millis(10), true));
    }

    #[test]
    fn a_pet_surface_closed_before_it_appeared_is_made_again() {
        let mut shell = booted();
        assert_eq!(shell.surface, Surface::Opening);
        let _ = shell.update(pet(Event::Window(window::Event::Closed)));
        assert_eq!(shell.surface, Surface::Gone);
        // the shell's news of the same close changes nothing
        let _ = shell.shell_event(ShellEvent::Closed(shell.pet));
        assert_eq!(shell.surface, Surface::Gone);
    }

    #[test]
    fn losing_the_surface_mid_drag_lets_the_pet_go() {
        let (mut shell, _) = live_shell();
        let _ = shell.update(press(mouse::Button::Left));
        let far = mouse::Event::CursorMoved {
            position: SPRITE_MIDDLE + iced::Vector::new(30.0, 0.0),
        };
        let _ = shell.update(pet(Event::Mouse(far)));
        assert!(shell.ui.drag_moved());
        let _ = shell.update(pet(Event::Window(window::Event::Closed)));
        assert_eq!(shell.surface, Surface::Gone);
        assert!(!shell.drag.active() && !shell.ui.drag_moved());
        assert_eq!(shell.ui.pointer(), None);
    }

    #[test]
    fn losing_the_surface_under_the_pointer_takes_the_pointer_off_the_pet() {
        let (mut shell, _) = live_shell();
        let _ = shell.update(pet(Event::Window(window::Event::Closed)));
        // no hover, and no smooth frames, for a point on a surface that is gone
        assert_eq!(shell.ui.pointer(), None);
    }

    #[test]
    fn a_late_popup_falls_back_to_the_inline_menu_once_which_closes_when_the_pointer_goes() {
        let (mut shell, t) = live_shell();
        let _ = shell.update(press(mouse::Button::Right));
        assert!(matches!(shell.menu, Some(Menu::Popup(Window { live: false, .. }))));
        let t = t.max(Instant::now()) + POPUP_PATIENCE * 2;
        let _ = shell.update(Message::Frame(t));
        assert!(matches!(shell.menu, Some(Menu::Inline { away: None })));
        let _ = shell.update(Message::Ui(aipet_ui::Message::PointerLeft));
        let _ = shell.update(Message::Frame(t + SMOOTH_FRAME));
        let _ = shell.update(Message::Frame(t + INLINE_MENU_PATIENCE));
        assert!(matches!(shell.menu, Some(Menu::Inline { away: Some(_) })));
        let _ = shell.update(Message::Frame(t + INLINE_MENU_PATIENCE + SMOOTH_FRAME * 2));
        assert!(shell.menu.is_none());
        // the next menu tries a popup again
        let _ = shell.update(Message::Ui(aipet_ui::Message::PointerMoved(SPRITE_MIDDLE)));
        let _ = shell.update(press(mouse::Button::Right));
        assert!(matches!(shell.menu, Some(Menu::Popup(_))));
    }

    #[test]
    fn a_press_on_the_pet_closes_its_popup_and_does_nothing_else() {
        let (mut shell, _) = live_shell();
        for button in [mouse::Button::Right, mouse::Button::Left] {
            let _ = shell.update(press(mouse::Button::Right));
            let Some(Menu::Popup(w)) = shell.menu else {
                panic!("no popup asked for")
            };
            appear(&mut shell, w.id);
            // on the sprite: no new menu, no poke and no drag
            let _ = shell.update(press(button));
            assert!(shell.menu.is_none() && !shell.drag.active());
        }
    }
}
