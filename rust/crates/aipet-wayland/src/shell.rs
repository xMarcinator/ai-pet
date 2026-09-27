//! The Wayland shell: an iced_exwlshell daemon with the pet on a layer surface, its menu in an xdg popup and
//! Settings in an xdg window.
//!
//! The pet's surface is [`SURFACE`] (380 × 600), anchored to its output's bottom-right corner on the Top layer
//! (Bottom when it isn't to stay on top), never taking the keyboard. At rest its input region is exactly
//! [`PetUi::hit_rects`], so everything but the pet and its bubbles clicks through. A left drag on the sprite moves
//! it, one of two ways (`AIPET_DRAG`, see [`crate::drag`]); a right press on the sprite opens the menu above it, in a popup
//! the compositor dismisses on a click elsewhere, or drawn in the pet's surface where no popup opens; Settings opens
//! in a window of its own, one at a time (`AIPET_OPEN_SETTINGS=1` opens it at start).
//!
//! The runtime knows a surface by its id only once it exists, and an action for an id that never appears waits
//! forever: windows are closed only once they have appeared, and a popup or Settings that doesn't appear in time is
//! given up on (and closed if it turns up after all). The daemon starts with no surface of its own and outlives the
//! pet's, which is made again when its output goes away.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use aipet_ui::{Effect, MENU, PetUi, Rect, SURFACE};
use iced::widget::{Space, container, pin};
use iced::{Color, Element, Event, Point, Subscription, Task, Theme, event, mouse, theme, time, window};
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

use crate::drag::{Action, Drag, Home, Input, SPRITE, Strategy};

/// The layer-shell namespace: compositor rules can match it (and it is not the real pet's).
const NAMESPACE: &str = "aipet-spike";
/// Where the surface starts: this far left of the output's bottom-right corner (the real pet lives in the corner).
const START: Home = Home { right: 420, bottom: 0 };
const SETTINGS_TITLE: &str = "AiPet Settings";
const SETTINGS_SIZE: (u32, u32) = (560, 640);
/// How long the menu's popup and Settings may take to appear before they are given up on (Settings is the first
/// surface when it opens at start, and waits for the renderer).
const POPUP_PATIENCE: Duration = Duration::from_millis(500);
const WINDOW_PATIENCE: Duration = Duration::from_secs(3);
/// How often, while the pet has no surface, a new one is tried.
const RETRY: Duration = Duration::from_secs(1);

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
enum Message {
    Ui(aipet_ui::Message),
    /// The pet's surface drew a frame: time to move everything on.
    Frame(Instant),
    /// A pointer, frame or size event on the pet's surface, and whether a widget took it.
    Pet(Event, event::Status),
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
    /// Closed (its output went away): made again once an output is there.
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
    /// Drawn in the pet's surface.
    Inline,
}

struct Shell {
    ui: PetUi,
    pet: window::Id,
    surface: Surface,
    drag: Drag,
    /// The input region the surface has, to send a new one only when it differs.
    region: Option<Vec<Rect>>,
    shell: ShellReceiver,
    /// The outputs connected, by registry name.
    outputs: BTreeSet<u32>,
    /// The output the pet's surface is on.
    output: Option<u32>,
    menu: Option<Menu>,
    /// No popup appeared last time: the menu is drawn in the pet's surface from now on.
    inline_menu: bool,
    settings: Option<Window>,
    /// AIPET_DEBUG=1: log what happens to the surfaces, the pointer's presses and the drags to stderr, stamped with
    /// the time since this start.
    debug: Option<Instant>,
}

/// Runs the pet on `connection`, which must offer layer-shell (see `probe`).
pub fn run(connection: Connection) -> Result<(), String> {
    let strategy = Strategy::from_env()?;
    let open_settings = std::env::var_os("AIPET_OPEN_SETTINGS").is_some_and(|v| v == "1");
    // nothing to type anywhere (Settings has switches and buttons): spare the clipboard's worker thread
    iced_exwlshell::disable_clipboard();
    // known before the daemon starts, so the 'static redraw scope can name it
    let pet = window::Id::unique();
    let (shell_tx, shell_rx) = iced_exwlshell::shell::channel();
    iced_exwlshell::daemon(
        move || Shell::boot(pet, strategy, shell_rx.clone(), open_settings),
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
    // the pet's frames and pointer redraw only the pet; what the views send may change any of them
    .redraw_scope(move |message: &Message| match message {
        Message::Frame(_) | Message::Pet(..) => Scope::Window(pet),
        Message::Shell(_) | Message::Retry => Scope::None,
        _ => Scope::All,
    })
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
    fn boot(pet: window::Id, strategy: Strategy, shell: ShellReceiver, open_settings: bool) -> (Self, Task<Message>) {
        let debug = std::env::var_os("AIPET_DEBUG")
            .is_some_and(|v| v == "1")
            .then(Instant::now);
        let mut me = Shell {
            ui: PetUi::new(),
            pet,
            surface: Surface::Gone,
            drag: Drag::new(strategy, START),
            region: None,
            shell,
            outputs: BTreeSet::new(),
            output: None,
            menu: None,
            inline_menu: false,
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
            frames_of(self.pet),
            pet_events(self.pet, self.drag.active()),
            self.shell.listen().map(|e| Message::Shell(Box::new(e))),
        ];
        if self.surface == Surface::Gone {
            subscriptions.push(time::every(RETRY).map(|_| Message::Retry));
        }
        Subscription::batch(subscriptions)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Frame(now) => {
                self.ui.tick(now);
                self.expire(now);
                self.sync_region()
            }
            Message::Ui(message) => {
                // choosing an item closes the menu
                let close = match message {
                    aipet_ui::Message::Menu(_) => self.close_menu(),
                    _ => Task::none(),
                };
                let effect = self.ui.update(message).map_or_else(Task::none, |e| self.apply(e));
                Task::batch([close, effect, self.sync_region()])
            }
            Message::Pet(event, status) => self.pet_event(event, status),
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
        self.drag.reset();
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

    /// Events on the pet's surface: presses on the sprite drag it or open the menu, and the drag hears the pointer,
    /// the frames and the sizes.
    fn pet_event(&mut self, event: Event, status: event::Status) -> Task<Message> {
        match event {
            Event::Mouse(mouse::Event::CursorEntered) => self.log(|| "pointer entered the pet's surface".to_owned()),
            Event::Mouse(mouse::Event::CursorLeft) => self.log(|| "pointer left the pet's surface".to_owned()),
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                // at rest the pet's box is the surface, so the pet's pointer is the surface's
                let at = self.ui.pointer();
                let on_sprite = at.is_some_and(|p| self.ui.sprite_hit(p));
                self.log(|| format!("{button:?} press at {at:?}: {}", press_target(on_sprite, status)));
                if status == event::Status::Captured {
                    return Task::none();
                }
                // a press anywhere on the pet closes the menu drawn in it (a right press on the sprite, too)
                let inline = matches!(self.menu, Some(Menu::Inline));
                if inline {
                    self.menu = None;
                    self.ui.set_inline_menu(false);
                }
                return match (button, at) {
                    (mouse::Button::Left, Some(at)) if on_sprite => self.drag_input(Input::Press(at)),
                    (mouse::Button::Right, _) if on_sprite && !inline => self.open_menu(),
                    _ => self.sync_region(),
                };
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
        if let Some(message) = step.pet {
            // the drag's news never asks anything of the shell
            self.ui.update(message);
        }
        Task::batch([self.change_surface(step.actions), self.sync_region()])
    }

    /// The drag's changes to the pet's surface, in order.
    fn change_surface(&self, actions: Vec<Action>) -> Task<Message> {
        if self.surface != Surface::Live {
            return Task::none();
        }
        if !actions.is_empty() {
            self.log(|| format!("surface: {actions:?}"));
        }
        let id = self.pet;
        actions.into_iter().fold(Task::none(), |task, action| {
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
        })
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
            Effect::Quit => iced::exit(),
        }
    }

    /// Opens the menu above the sprite: a popup, which the compositor places (below the sprite where there is no
    /// room above) and dismisses on a click elsewhere, opened on the press so its grab has the press to go by.
    fn open_menu(&mut self) -> Task<Message> {
        if self.menu.is_some() || self.drag.active() {
            // a popup open: the press outside it dismisses it (the compositor's grab)
            return Task::none();
        }
        if self.inline_menu {
            self.menu = Some(Menu::Inline);
            self.ui.set_inline_menu(true);
            self.log(|| "menu drawn in the pet's surface".to_owned());
            return self.sync_region();
        }
        let id = window::Id::unique();
        self.menu = Some(Menu::Popup(Window::asked(id, Instant::now() + POPUP_PATIENCE)));
        self.log(|| format!("menu popup {id:?} asked for"));
        // anchored to the middle of the sprite's top edge, growing up from it, centred. Flipped at the output's
        // edges but never slid: Hyprland 0.56 slides a layer surface's popup by the wrong amount while the surface
        // is still being animated (seen right after it maps), and flips alone land where asked
        let top = (SURFACE.width as i32 / 2, SPRITE.y as i32);
        let size = PixelSize::px(MENU.width as u32, MENU.height as u32);
        Task::done(Message::NewPopUp {
            settings: IcedNewPopupSettings::new(self.pet, size, top, PixelSize::px(1, 1))
                .anchor(PopupAnchor::Top)
                .gravity(PopupGravity::Top)
                .constraint_adjustment(PopupConstraintAdjustment::FlipX | PopupConstraintAdjustment::FlipY),
            id,
        })
    }

    /// Closes the menu. A popup that hasn't appeared can't be closed yet: it is forgotten, and closed if it turns
    /// up.
    fn close_menu(&mut self) -> Task<Message> {
        match self.menu.take() {
            Some(Menu::Popup(w)) if w.live => window::close(w.id),
            Some(Menu::Inline) => {
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
    /// surface instead, from now on.
    fn expire(&mut self, now: Instant) {
        if let Some(Menu::Popup(w)) = self.menu
            && !w.live
            && now > w.deadline
        {
            self.log(|| "no popup appeared: the menu is drawn in the pet's surface".to_owned());
            self.inline_menu = true;
            self.menu = Some(Menu::Inline);
            self.ui.set_inline_menu(true);
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
        self.change_surface(actions)
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
    /// away (its surface is made again, on the output the compositor picks, by the retry timer).
    fn closed(&mut self, id: window::Id) {
        if id == self.pet {
            self.log(|| "the pet's surface closed".to_owned());
            self.surface = Surface::Gone;
            self.output = None;
            self.drag.reset();
            // its popup went with it
            if matches!(self.menu, Some(Menu::Popup(_))) {
                self.menu = None;
            }
        } else if matches!(self.menu, Some(Menu::Popup(w)) if w.id == id) {
            self.log(|| "menu closed".to_owned());
            self.menu = None;
        } else if self.settings.is_some_and(|w| w.id == id) {
            self.log(|| "Settings closed".to_owned());
            self.settings = None;
        }
    }

    /// Sends the pet's hit rectangles as the surface's input region, when they changed. Only at rest: during a drag
    /// the pointer is the surface's until the release anyway, and the pet's box may be elsewhere on it.
    fn sync_region(&mut self) -> Task<Message> {
        if self.surface != Surface::Live || self.drag.active() {
            return Task::none();
        }
        let rects = self.ui.hit_rects();
        if self.region.as_ref() == Some(&rects) {
            return Task::none();
        }
        self.log(|| format!("input region: {} rects, {}", rects.len(), describe(&rects)));
        self.region = Some(rects.clone());
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

/// The pet surface's frames: each `RedrawRequested` of that surface, as the time it was drawn (other surfaces'
/// frames don't count, or Settings would speed the pet up).
fn frames_of(pet: window::Id) -> Subscription<Message> {
    event::listen_raw(|event, _status, id| match event {
        Event::Window(window::Event::RedrawRequested(at)) => Some((id, at)),
        _ => None,
    })
    .with(pet)
    .filter_map(|(pet, (id, at))| (id == pet).then_some(Message::Frame(at)))
}

/// The pet surface's pointer and size events; with `every` (during a drag) also each pointer move and frame, all
/// in one stream so the drag sees them in the order they happened.
fn pet_events(pet: window::Id, every: bool) -> Subscription<Message> {
    event::listen_raw(|event, status, id| match event {
        Event::Mouse(_) | Event::Window(window::Event::Resized(_) | window::Event::RedrawRequested(_)) => {
            Some((id, event, status))
        }
        _ => None,
    })
    .with((pet, every))
    .filter_map(|((pet, every), (id, event, status))| {
        let wanted = match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) | Event::Window(window::Event::RedrawRequested(_)) => every,
            Event::Mouse(mouse::Event::WheelScrolled { .. }) => false,
            _ => true,
        };
        (id == pet && wanted).then_some(Message::Pet(event, status))
    })
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
    use super::*;

    #[test]
    fn the_menu_popup_goes_where_the_pet_draws_its_inline_menu() {
        let mut ui = PetUi::new();
        let start = Instant::now();
        for i in 0..60 {
            ui.tick(start + Duration::from_millis(16 * i));
        }
        assert!(ui.sprite_hit(Point::new(SPRITE.center_x(), SPRITE.center_y())));
        let bare = ui.hit_rects();
        ui.set_inline_menu(true);
        let menu: Vec<Rect> = ui.hit_rects().into_iter().filter(|r| !bare.contains(r)).collect();
        let x0 = menu.iter().map(|r| r.x).min().unwrap();
        let y0 = menu.iter().map(|r| r.y).min().unwrap();
        let x1 = menu.iter().map(|r| r.x + r.width).max().unwrap();
        let y1 = menu.iter().map(|r| r.y + r.height).max().unwrap();
        // the popup is centred on its anchor, the middle of the sprite's top edge, right above it
        let (ax, ay) = (SURFACE.width / 2.0, SPRITE.y);
        assert_eq!(
            (x0, x1),
            ((ax - MENU.width / 2.0) as i32, (ax + MENU.width / 2.0) as i32)
        );
        assert_eq!((y0, y1), ((ay - MENU.height) as i32, ay as i32));
    }
}
