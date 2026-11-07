//! Compile-check of the recommended AiPet Wayland shell on iced_exwlshell 0.20.1.
//!
//! One daemon with three kinds of surfaces:
//! - the pet: a layer surface anchored Bottom|Left, moved with margins, exact input region
//! - the context menu: an xdg_popup parented to the pet (grabbed, so the compositor
//!   dismisses it on outside click)
//! - Settings: a normal xdg_toplevel
use std::time::{Duration, Instant};

use iced::widget::{button, canvas, column, container, image, pin, stack, text};
use iced::{
    Color, Element, Event, Length, Point, Radians, Rectangle, Renderer, Subscription, Task, Theme,
    Vector, mouse, window,
};
use iced_exwlshell::actions::{ActionCallback, IcedNewPopupSettings, IcedXdgWindowSettings};
use iced_exwlshell::daemon;
use iced_exwlshell::redraw::Scope;
use iced_exwlshell::reexport::{
    Anchor, KeyboardInteractivity, Layer, LayerSize, NewLayerShellSettings, OutputOption,
    PixelSize, PopupGravity,
};
use iced_exwlshell::settings::{LayerShellSettings, Settings, StartMode};
use iced_exwlshell::to_layer_message;
use iced_wayland_subscriber::shell::{ShellEvent, ShellReceiver};

const SPRITE_W: u32 = 26;
const SPRITE_H: u32 = 24;
const ZOOM: u32 = 5;
const PET_W: u32 = SPRITE_W * ZOOM; // 130 logical px
const PET_H: u32 = SPRITE_H * ZOOM; // 120 logical px
const SHADOW_H: u32 = 14;
const BUBBLE_W: u32 = 220;
const BUBBLE_H: u32 = 44;
const GAP: u32 = 6;
const SURFACE_W: u32 = BUBBLE_W; // >= PET_W
const MENU: (u32, u32) = (160, 96);

fn main() -> iced_exwlshell::Result {
    // Known up front, so redraw_scope (a 'static closure without state access) can use it.
    let pet_id = window::Id::unique();
    let (shell_tx, shell_rx) = iced_wayland_subscriber::shell::channel();

    daemon(
        move || Pet::boot(pet_id, shell_rx.clone()),
        "aipet",
        Pet::update,
        Pet::view,
    )
    .title(Pet::title)
    // One style for every surface: the clear colour must be transparent for the pet.
    // The Settings window paints its own opaque background.
    .style(|_state: &Pet, theme: &Theme| iced::theme::Style {
        background_color: Color::TRANSPARENT,
        text_color: theme.palette().text,
    })
    .subscription(Pet::subscription)
    // Without this every message redraws every surface (redraw.rs:46-49).
    .redraw_scope(move |message: &Message| match message {
        Message::Tick(_) => Scope::Window(pet_id),
        // While dragging, listen_raw also reports other surfaces' frames: do not let
        // those feed a redraw loop. The pet's own frames keep the drag pipeline moving.
        Message::Input(id, Event::Window(window::Event::RedrawRequested(_))) => {
            if *id == pet_id {
                Scope::Window(pet_id)
            } else {
                Scope::None
            }
        }
        Message::Input(id, _) => Scope::Window(*id),
        _ => Scope::All,
    })
    .settings(Settings {
        layer_settings: LayerShellSettings {
            // No initial surface: the pet is created by boot() with a known id, and the
            // event loop does not exit when the last surface closes (lib.rs:3550-3556).
            start_mode: StartMode::Background,
            ..Default::default()
        },
        shell_broadcast: shell_tx,
        ..Default::default()
    })
    .run()
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
enum Message {
    Tick(Instant),
    /// A runtime event and the surface it happened on.
    Input(window::Id, Event),
    Shell(ShellEvent),
    WindowClosed(window::Id),
    MenuSettings,
    MenuQuit,
    CloseSettings,
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    /// Where the sprite was grabbed, surface-local logical px.
    grab: Point,
    /// Margins sent but not yet known to be applied: (left, bottom, pet frames to wait).
    pending: Option<(i32, i32, u8)>,
    moved: bool,
}

struct Pet {
    pet_id: window::Id,
    menu_id: Option<window::Id>,
    settings_id: Option<window::Id>,
    shell_rx: ShellReceiver,
    output: Option<u32>,
    output_size: Option<(i32, i32)>,
    /// Margins known to be applied by the compositor (anchor Bottom|Left).
    left: i32,
    bottom: i32,
    cursor: Option<Point>,
    drag: Option<Drag>,
    bubbles: Vec<String>,
    sprite: image::Handle,
    sprite_rgba: Vec<u8>,
    frame: u32,
    last_tick: Option<Instant>,
}

impl Pet {
    fn boot(pet_id: window::Id, shell_rx: ShellReceiver) -> (Self, Task<Message>) {
        let sprite_rgba = demo_sprite();
        let pet = Pet {
            pet_id,
            menu_id: None,
            settings_id: None,
            shell_rx,
            output: None,
            output_size: None,
            left: 40,
            bottom: 40,
            cursor: None,
            drag: None,
            bubbles: vec!["Claude is thinking...".into()],
            sprite: image::Handle::from_rgba(SPRITE_W, SPRITE_H, sprite_rgba.clone()),
            sprite_rgba,
            frame: 0,
            last_tick: None,
        };
        let open = pet.open_pet_surface();
        (pet, open)
    }

    fn surface_height(&self) -> u32 {
        self.bubbles.len() as u32 * (BUBBLE_H + GAP) + PET_H + SHADOW_H
    }

    fn open_pet_surface(&self) -> Task<Message> {
        let open = Task::done(Message::NewLayerShell {
            settings: NewLayerShellSettings {
                size: LayerSize::px(SURFACE_W, self.surface_height()),
                layer: Layer::Top,
                anchor: Anchor::Bottom | Anchor::Left,
                exclusive_zone: None,
                // (top, right, bottom, left), lib.rs:801
                margin: Some((0, 0, self.bottom, self.left)),
                keyboard_interactivity: KeyboardInteractivity::None,
                output_option: OutputOption::Active,
                // Empty input region from the first commit (lib.rs:3193-3197); the real one follows.
                events_transparent: true,
                namespace: Some("aipet".into()),
                ..Default::default()
            },
            id: self.pet_id,
        });
        // Queued until the surface exists (multi_window.rs:854-861).
        open.chain(self.input_region())
    }

    /// Top-left of the sprite inside the pet surface.
    fn sprite_origin(&self) -> Point {
        Point::new(0.0, (self.surface_height() - SHADOW_H - PET_H) as f32)
    }

    fn sprite_rect(&self) -> Rectangle {
        Rectangle::new(
            self.sprite_origin(),
            iced::Size::new(PET_W as f32, PET_H as f32),
        )
    }

    /// Exact input region: opaque sprite pixels (row runs) plus each bubble card.
    /// Surface-local logical px, the same units as iced layout at scale_factor 1.0.
    fn input_region(&self) -> Task<Message> {
        let origin = self.sprite_origin();
        let mut rects = alpha_runs(
            &self.sprite_rgba,
            SPRITE_W,
            SPRITE_H,
            ZOOM,
            origin.x as i32,
            origin.y as i32,
        );
        for i in 0..self.bubbles.len() as u32 {
            rects.push((0, (i * (BUBBLE_H + GAP)) as i32, BUBBLE_W as i32, BUBBLE_H as i32));
        }
        Task::done(Message::SetInputRegion {
            id: self.pet_id,
            callback: ActionCallback::new(move |region| {
                for &(x, y, w, h) in &rects {
                    region.add(x, y, w, h);
                }
            }),
        })
    }

    fn clamp(&self, left: i32, bottom: i32) -> (i32, i32) {
        match self.output_size {
            Some((w, h)) => (
                left.clamp(0, (w - PET_W as i32).max(0)),
                bottom.clamp(0, (h - self.surface_height() as i32).max(0)),
            ),
            None => (left.max(0), bottom.max(0)),
        }
    }

    fn title(&self, id: window::Id) -> Option<String> {
        (Some(id) == self.settings_id).then(|| "AiPet Settings".to_owned())
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            window::close_events().map(Message::WindowClosed),
            self.shell_rx.listen().map(Message::Shell),
            iced::time::every(Duration::from_millis(100)).map(Message::Tick),
        ];
        if self.drag.is_some() {
            // listen_raw keeps RedrawRequested, which listen_with drops, and keeps both in
            // one ordered stream. Only while dragging: it is a self-sustaining redraw loop.
            subscriptions.push(iced::event::listen_raw(|event, _status, id| match event {
                Event::Mouse(_)
                | Event::Window(window::Event::RedrawRequested(_) | window::Event::Resized(_)) => {
                    Some(Message::Input(id, event))
                }
                _ => None,
            }));
        } else {
            subscriptions.push(iced::event::listen_with(|event, _status, id| match event {
                Event::Mouse(_) | Event::Window(window::Event::Resized(_)) => {
                    Some(Message::Input(id, event))
                }
                _ => None,
            }));
        }
        Subscription::batch(subscriptions)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick(now) => {
                self.frame = self.frame.wrapping_add(1);
                self.last_tick = Some(now);
                Task::none()
            }
            Message::Input(id, event) if id == self.pet_id => self.pet_event(event),
            Message::Input(..) => Task::none(),
            Message::Shell(ShellEvent::WindowOutputChanged { window, output }) => {
                if window == self.pet_id {
                    self.output = output.as_ref().map(|info| info.id);
                    self.output_size = output.and_then(|info| info.logical_size);
                }
                Task::none()
            }
            Message::Shell(ShellEvent::OutputUpdated(info)) => {
                if Some(info.id) == self.output {
                    self.output_size = info.logical_size;
                }
                Task::none()
            }
            Message::Shell(_) => Task::none(),
            Message::WindowClosed(id) => {
                if Some(id) == self.menu_id {
                    self.menu_id = None;
                } else if Some(id) == self.settings_id {
                    self.settings_id = None;
                } else if id == self.pet_id {
                    // e.g. its output was unplugged: recreate it on the active output
                    self.drag = None;
                    return self.open_pet_surface();
                }
                Task::none()
            }
            Message::MenuSettings => {
                let close = self.close_menu();
                if self.settings_id.is_some() {
                    return close;
                }
                let (id, open) = Message::base_window_open(IcedXdgWindowSettings {
                    size: Some(PixelSize::px(520, 420)),
                    client_side_decorations: false,
                });
                self.settings_id = Some(id);
                Task::batch([close, open])
            }
            Message::CloseSettings => match self.settings_id {
                Some(id) => window::close(id),
                None => Task::none(),
            },
            Message::MenuQuit => iced::exit(),
            // The macro-generated layer-shell variants never reach update (multi_window.rs:1282-1287).
            _ => Task::none(),
        }
    }

    fn close_menu(&mut self) -> Task<Message> {
        match self.menu_id.take() {
            Some(id) => window::close(id),
            None => Task::none(),
        }
    }

    fn pet_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                self.cursor = Some(position);
                let Some(drag) = self.drag.as_mut() else {
                    return Task::none();
                };
                if drag.pending.is_some() {
                    // Relative to an origin that may already be stale: ignore.
                    return Task::none();
                }
                let dx = (position.x - drag.grab.x).round() as i32;
                let dy = (position.y - drag.grab.y).round() as i32;
                if dx == 0 && dy == 0 {
                    return Task::none();
                }
                drag.moved = true;
                let (left, bottom) = self.clamp(self.left + dx, self.bottom - dy);
                if let Some(drag) = self.drag.as_mut() {
                    drag.pending = Some((left, bottom, 2));
                }
                Task::done(Message::MarginChange {
                    id: self.pet_id,
                    margin: (0, 0, bottom, left),
                })
            }
            Event::Window(window::Event::RedrawRequested(_)) => {
                // Two pet frames after the margin commit, a frame callback for a later
                // commit has fired, so the margins are applied and every later motion
                // event is relative to the new origin.
                if let Some(drag) = self.drag.as_mut()
                    && let Some((left, bottom, frames)) = drag.pending
                {
                    if frames <= 1 {
                        self.left = left;
                        self.bottom = bottom;
                        drag.pending = None;
                    } else {
                        drag.pending = Some((left, bottom, frames - 1));
                    }
                }
                Task::none()
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = self.cursor
                    && self.sprite_rect().contains(position)
                {
                    self.drag = Some(Drag {
                        grab: position,
                        pending: None,
                        moved: false,
                    });
                }
                Task::none()
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let Some(drag) = self.drag.take() else {
                    return Task::none();
                };
                if let Some((left, bottom, _)) = drag.pending {
                    self.left = left;
                    self.bottom = bottom;
                }
                if drag.moved {
                    return Task::none();
                }
                // A click without movement: toggle a bubble, which resizes the surface.
                if self.bubbles.len() < 3 {
                    self.bubbles.push(format!("bubble {}", self.bubbles.len() + 1));
                } else {
                    self.bubbles.clear();
                }
                let (left, bottom) = self.clamp(self.left, self.bottom);
                self.left = left;
                self.bottom = bottom;
                Task::batch([
                    Task::done(Message::LayoutChange {
                        id: self.pet_id,
                        anchor: Anchor::Bottom | Anchor::Left,
                        size: LayerSize::px(SURFACE_W, self.surface_height()),
                    }),
                    Task::done(Message::MarginChange {
                        id: self.pet_id,
                        margin: (0, 0, bottom, left),
                    }),
                    self.input_region(),
                ])
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let Some(position) = self.cursor else {
                    return Task::none();
                };
                if !self.sprite_rect().contains(position) || self.menu_id.is_some() {
                    return Task::none();
                }
                let id = window::Id::unique();
                self.menu_id = Some(id);
                Task::done(Message::NewPopUp {
                    // Anchor rect = the click point on the parent (surface-local); the popup
                    // grows up-right and flips/slides to stay on the output.
                    settings: IcedNewPopupSettings::new(
                        self.pet_id,
                        PixelSize::px(MENU.0, MENU.1),
                        (position.x as i32, position.y as i32),
                        PixelSize::px(1, 1),
                    )
                    .gravity(PopupGravity::TopRight),
                    id,
                })
            }
            Event::Mouse(mouse::Event::CursorLeft) => {
                self.cursor = None;
                Task::none()
            }
            // SetInputRegion only clears the surface's size at the time it runs
            // (multi_window.rs:898-902), which is the old size until the configure lands.
            Event::Window(window::Event::Resized(_)) => self.input_region(),
            _ => Task::none(),
        }
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        if id == self.pet_id {
            self.pet_view()
        } else if Some(id) == self.menu_id {
            menu_view()
        } else if Some(id) == self.settings_id {
            settings_view()
        } else {
            iced::widget::space::horizontal().into()
        }
    }

    fn pet_view(&self) -> Element<'_, Message> {
        let height = self.surface_height() as f32;
        let mut layers: Vec<Element<'_, Message>> = Vec::new();
        for (i, bubble) in self.bubbles.iter().enumerate() {
            let y = (i as u32 * (BUBBLE_H + GAP)) as f32;
            layers.push(
                pin(container(text(bubble).size(13))
                    .padding(8)
                    .width(BUBBLE_W as f32)
                    .height(BUBBLE_H as f32)
                    .style(|theme: &Theme| container::Style {
                        background: Some(theme.palette().background.into()),
                        border: iced::border::rounded(10),
                        ..Default::default()
                    }))
                .position(Point::new(0.0, y))
                .into(),
            );
        }
        layers.push(
            pin(canvas(ShadowOval)
                .width(PET_W as f32)
                .height(SHADOW_H as f32))
            .position(Point::new(0.0, height - SHADOW_H as f32))
            .into(),
        );
        layers.push(
            pin(image(self.sprite.clone())
                .filter_method(image::FilterMethod::Nearest)
                .width(PET_W as f32)
                .height(PET_H as f32))
            .position(self.sprite_origin())
            .into(),
        );
        stack(layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

fn menu_view<'a>() -> Element<'a, Message> {
    container(
        column![
            button("Settings").on_press(Message::MenuSettings),
            button("Quit").on_press(Message::MenuQuit),
        ]
        .spacing(4),
    )
    .padding(6)
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().background.into()),
        border: iced::border::rounded(6),
        ..Default::default()
    })
    .into()
}

fn settings_view<'a>() -> Element<'a, Message> {
    container(
        column![
            text("AiPet settings").size(20),
            button("Close").on_press(Message::CloseSettings),
        ]
        .spacing(12),
    )
    .padding(20)
    .width(Length::Fill)
    .height(Length::Fill)
    // Opaque: the shared clear colour is transparent.
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().background.into()),
        ..Default::default()
    })
    .into()
}

/// The soft elliptical shadow under the pet.
struct ShadowOval;

impl<Message> canvas::Program<Message> for ShadowOval {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = frame.center();
        let oval = canvas::Path::new(|builder| {
            builder.ellipse(canvas::path::arc::Elliptical {
                center,
                radii: Vector::new(bounds.width / 2.0, bounds.height / 2.0),
                rotation: Radians(0.0),
                start_angle: Radians(0.0),
                end_angle: Radians(std::f32::consts::TAU),
            });
        });
        frame.fill(&oval, Color::from_rgba(0.0, 0.0, 0.0, 0.22));
        vec![frame.into_geometry()]
    }
}

/// Horizontal runs of non-transparent sprite pixels, scaled by `zoom` and offset,
/// as (x, y, width, height) rectangles for wl_region.add.
fn alpha_runs(rgba: &[u8], w: u32, h: u32, zoom: u32, ox: i32, oy: i32) -> Vec<(i32, i32, i32, i32)> {
    let alpha = |x: u32, y: u32| rgba[((y * w + x) * 4 + 3) as usize];
    let mut rects = Vec::new();
    for y in 0..h {
        let mut x = 0;
        while x < w {
            if alpha(x, y) == 0 {
                x += 1;
                continue;
            }
            let start = x;
            while x < w && alpha(x, y) != 0 {
                x += 1;
            }
            rects.push((
                ox + (start * zoom) as i32,
                oy + (y * zoom) as i32,
                ((x - start) * zoom) as i32,
                zoom as i32,
            ));
        }
    }
    rects
}

/// A 26x24 RGBA test sprite: an opaque ellipse on a transparent background.
fn demo_sprite() -> Vec<u8> {
    let mut rgba = vec![0u8; (SPRITE_W * SPRITE_H * 4) as usize];
    for y in 0..SPRITE_H {
        for x in 0..SPRITE_W {
            let nx = (x as f32 + 0.5 - SPRITE_W as f32 / 2.0) / (SPRITE_W as f32 / 2.0);
            let ny = (y as f32 + 0.5 - SPRITE_H as f32 / 2.0) / (SPRITE_H as f32 / 2.0);
            if nx * nx + ny * ny <= 1.0 {
                let i = ((y * SPRITE_W + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[0xE8, 0x9A, 0x4F, 0xFF]);
            }
        }
    }
    rgba
}
