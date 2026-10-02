//! Compile-check for the iced 0.14 API notes (spike-notes/iced.md). Not a product.
mod api_probe;
mod glow;
mod hit_regions;
mod pixel_canvas;
mod sprite_widget;
#[cfg(target_os = "linux")]
mod x11;

use iced::mouse;
use iced::theme;
use iced::time::Instant;
use iced::widget::{button, column, container, image, mouse_area, pin, stack, text};
use iced::window;
use iced::{
    Animation, Border, Color, Element, Event, Length, Point, Rectangle, Shadow, Size,
    Subscription, Task, Theme, Vector,
};

const PET_THEME: &str = "AiPet transparent";
const SCALE: f32 = 5.0;
const SPRITE_W: u32 = 26;
const SPRITE_H: u32 = 24;
const WIN_W: f32 = 360.0;
const WIN_H: f32 = 420.0;

pub fn main() -> iced::Result {
    iced::daemon(State::boot, State::update, State::view)
        .title(State::title)
        .theme(State::theme)
        .style(State::style)
        .subscription(State::subscription)
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    PetOpened(window::Id),
    Native(NativeHandle),
    OpenSettings,
    SettingsOpened(window::Id),
    WindowClosed(window::Id),
    Frame(Instant),
    PetPressed,
    PetReleased,
    CursorMoved(window::Id, Point),
    RightClick,
    HitRegions(Vec<Rectangle>),
    AddBubble,
    DismissBubble(usize),
    Position(Option<Point>),
    Quit,
}

#[derive(Debug, Clone, Copy)]
enum NativeHandle {
    X11 { window: u32 },
    Win32 { hwnd: isize },
    AppKit,
    Wayland,
    Other,
}

struct Bubble {
    text: String,
    shown: Animation<bool>,
    removing: bool,
}

struct DragState {
    grab: Point,
}

struct State {
    pet: window::Id,
    settings: Option<window::Id>,
    native: Option<NativeHandle>,
    frames: Vec<image::Handle>,
    frame: usize,
    next_frame_at: Instant,
    shadow: image::Handle,
    bubbles: Vec<Bubble>,
    menu: Animation<bool>,
    now: Instant,
    drag: Option<DragState>,
    window_pos: Option<Point>,
    regions: Vec<Rectangle>,
    #[cfg(target_os = "linux")]
    x11: Option<x11rb::rust_connection::RustConnection>,
}

fn pet_window_settings() -> window::Settings {
    window::Settings {
        size: Size::new(WIN_W, WIN_H),
        position: window::Position::Specific(Point::new(200.0, 200.0)),
        visible: true,
        resizable: false,
        decorations: false,
        transparent: true,
        level: window::Level::AlwaysOnTop,
        exit_on_close_request: false,
        #[cfg(target_os = "linux")]
        platform_specific: window::settings::PlatformSpecific {
            application_id: "aipet".to_owned(),
            override_redirect: false,
        },
        #[cfg(target_os = "windows")]
        platform_specific: window::settings::PlatformSpecific {
            skip_taskbar: true,
            undecorated_shadow: false,
            corner_preference: window::settings::platform::CornerPreference::DoNotRound,
            drag_and_drop: false,
        },
        ..window::Settings::default()
    }
}

fn settings_window_settings() -> window::Settings {
    window::Settings {
        size: Size::new(520.0, 420.0),
        position: window::Position::Centered,
        ..window::Settings::default()
    }
}

impl State {
    fn boot() -> (Self, Task<Message>) {
        let (pet, open) = window::open(pet_window_settings());

        // 26x24 straight-alpha RGBA frames, created ONCE (Handle::from_rgba gives a new id per call).
        let frames = (0..4u8)
            .map(|i| {
                let mut px = vec![0u8; (SPRITE_W * SPRITE_H * 4) as usize];
                for p in px.chunks_exact_mut(4) {
                    p.copy_from_slice(&[200, 120 + i * 20, 60, 255]);
                }
                image::Handle::from_rgba(SPRITE_W, SPRITE_H, px)
            })
            .collect();

        let now = Instant::now();
        let state = State {
            pet,
            settings: None,
            native: None,
            frames,
            frame: 0,
            next_frame_at: now,
            shadow: glow::soft_ellipse(120, 28),
            bubbles: Vec::new(),
            menu: Animation::new(false).quick(),
            now,
            drag: None,
            window_pos: None,
            regions: Vec::new(),
            #[cfg(target_os = "linux")]
            x11: x11rb::connect(None).ok().map(|(conn, _screen)| conn),
        };

        (state, open.map(Message::PetOpened))
    }

    fn title(&self, id: window::Id) -> String {
        if id == self.pet { "AiPet".into() } else { "AiPet Settings".into() }
    }

    fn theme(&self, id: window::Id) -> Theme {
        if id == self.pet {
            Theme::custom(PET_THEME, Theme::Dark.palette())
        } else {
            Theme::Dark
        }
    }

    // `style` gets no window id, so the pet window is recognised by its theme name.
    fn style(&self, theme: &Theme) -> theme::Style {
        let base = theme::Base::base(theme);
        if theme::Base::name(theme) == PET_THEME {
            theme::Style { background_color: Color::TRANSPARENT, ..base }
        } else {
            base
        }
    }

    fn is_animating(&self) -> bool {
        self.menu.is_animating(self.now)
            || self.bubbles.iter().any(|b| b.shown.is_animating(self.now))
    }

    fn subscription(&self) -> Subscription<Message> {
        let cursor = iced::event::listen_with(|event, _status, id| match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Message::CursorMoved(id, position))
            }
            _ => None,
        });

        // The sprite animates at ~8 fps: a timer is enough (needs the `tokio` or `smol` feature).
        let sprite = iced::time::every(std::time::Duration::from_millis(125)).map(Message::Frame);

        // UI tweens need vsync-rate frames, but only while something moves.
        let tweens = if self.is_animating() {
            window::frames().map(Message::Frame)
        } else {
            Subscription::none()
        };

        Subscription::batch([
            cursor,
            sprite,
            tweens,
            window::close_events().map(Message::WindowClosed),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PetOpened(id) => {
                let native = window::run(id, |w| {
                    use iced::window::raw_window_handle::RawWindowHandle;
                    match w.window_handle().map(|h| h.as_raw()) {
                        Ok(RawWindowHandle::Xlib(h)) => NativeHandle::X11 { window: h.window as u32 },
                        Ok(RawWindowHandle::Xcb(h)) => NativeHandle::X11 { window: h.window.get() },
                        Ok(RawWindowHandle::Win32(h)) => NativeHandle::Win32 { hwnd: h.hwnd.get() },
                        Ok(RawWindowHandle::AppKit(_)) => NativeHandle::AppKit,
                        Ok(RawWindowHandle::Wayland(_)) => NativeHandle::Wayland,
                        _ => NativeHandle::Other,
                    }
                })
                .map(Message::Native);
                Task::batch([native, window::position(id).map(Message::Position), window::raw_id::<Message>(id).discard()])
            }
            Message::Native(native) => {
                self.native = Some(native);
                self.apply_input_region()
            }
            Message::Position(p) => {
                self.window_pos = p;
                Task::none()
            }
            Message::OpenSettings => {
                if let Some(id) = self.settings {
                    return window::gain_focus(id);
                }
                let (id, open) = window::open(settings_window_settings());
                self.settings = Some(id);
                open.map(Message::SettingsOpened)
            }
            Message::SettingsOpened(_) => Task::none(),
            Message::WindowClosed(id) => {
                if Some(id) == self.settings {
                    self.settings = None;
                }
                Task::none()
            }
            Message::Frame(now) => {
                self.now = now;
                if now >= self.next_frame_at {
                    self.frame = (self.frame + 1) % self.frames.len();
                    self.next_frame_at = now + std::time::Duration::from_millis(125);
                }
                self.bubbles.retain(|b| !(b.removing && !b.shown.is_animating(now)));
                Task::none()
            }
            Message::PetPressed => {
                // Option A: compositor/WM driven move (X11, Windows, macOS, winit-Wayland).
                window::drag(self.pet)
            }
            Message::PetReleased => {
                self.drag = None;
                window::position(self.pet).map(Message::Position)
            }
            Message::CursorMoved(id, p) => {
                // Option B: app-driven move (not on Wayland): keep the grab point under the cursor.
                if let (true, Some(drag), Some(pos)) = (id == self.pet, &self.drag, self.window_pos) {
                    let next = Point::new(pos.x + p.x - drag.grab.x, pos.y + p.y - drag.grab.y);
                    self.window_pos = Some(next);
                    return window::move_to(self.pet, next);
                }
                Task::none()
            }
            Message::RightClick => {
                let now = Instant::now();
                self.now = now;
                let open = !self.menu.value();
                self.menu.go_mut(open, now);
                Task::none()
            }
            Message::AddBubble => {
                let now = Instant::now();
                self.now = now;
                self.bubbles.push(Bubble {
                    text: "Claude is waiting for you".into(),
                    shown: Animation::new(false).slow().go(true, now),
                    removing: false,
                });
                Task::none()
            }
            Message::DismissBubble(i) => {
                let now = Instant::now();
                self.now = now;
                if let Some(b) = self.bubbles.get_mut(i) {
                    b.removing = true;
                    b.shown.go_mut(false, now);
                }
                Task::none()
            }
            Message::HitRegions(rects) => {
                self.regions = rects;
                self.apply_input_region()
            }
            Message::Quit => iced::exit(),
        }
    }

    fn apply_input_region(&self) -> Task<Message> {
        match self.native {
            #[cfg(target_os = "linux")]
            Some(NativeHandle::X11 { window }) => {
                if let Some(conn) = &self.x11 {
                    // X11 shapes are in physical pixels; 1.0 assumes no HiDPI scaling here.
                    let _ = x11::set_input_region(conn, window, &self.regions, 1.0);
                }
                Task::none()
            }
            // Windows / macOS: no shaped input region from winit; toggle hit-testing instead
            // based on a polled global cursor position (see notes, section 5).
            _ => Task::none(),
        }
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        if id == self.pet { self.pet_view() } else { self.settings_view() }
    }

    fn settings_view(&self) -> Element<'_, Message> {
        container(column![text("Settings"), button("Close").on_press(Message::Quit)].spacing(8))
            .padding(16)
            .into()
    }

    fn pet_view(&self) -> Element<'_, Message> {
        let now = self.now;
        let sprite_w = SPRITE_W as f32 * SCALE;
        let sprite_h = SPRITE_H as f32 * SCALE;
        let sprite_x = (WIN_W - sprite_w) / 2.0;
        let sprite_y = WIN_H - sprite_h - 24.0;

        let sprite = image(self.frames[self.frame].clone())
            .width(sprite_w)
            .height(sprite_h)
            .filter_method(image::FilterMethod::Nearest);

        let sprite = container(
            mouse_area(sprite)
                .on_press(Message::PetPressed)
                .on_release(Message::PetReleased)
                .on_right_press(Message::RightClick)
                .on_double_click(Message::AddBubble)
                .interaction(mouse::Interaction::Grab),
        )
        .id("hit:sprite");

        let shadow = image(self.shadow.clone()).width(120).height(28);

        let mut layers = stack![
            pin(shadow).x((WIN_W - 120.0) / 2.0).y(WIN_H - 36.0),
            pin(sprite).x(sprite_x).y(sprite_y),
        ]
        .width(Length::Fill)
        .height(Length::Fill);

        let mut y = sprite_y - 8.0;
        for (i, b) in self.bubbles.iter().enumerate().rev() {
            let t = b.shown.interpolate(0.0f32, 1.0f32, now);
            let h = 44.0;
            y -= h + 6.0;
            let card = container(text(&b.text).size(13).wrapping(text::Wrapping::Word))
                .padding([8, 12])
                .width(220)
                .height(h)
                .style(move |_theme: &Theme| container::Style {
                    background: Some(Color::from_rgb8(0x24, 0x27, 0x2e).scale_alpha(0.95 * t).into()),
                    text_color: Some(Color::WHITE.scale_alpha(t)),
                    border: Border::default().rounded(12),
                    shadow: Shadow {
                        color: Color::BLACK.scale_alpha(0.35 * t),
                        offset: Vector::new(0.0, 2.0),
                        blur_radius: 8.0,
                    },
                    snap: true,
                });
            let card = container(mouse_area(card).on_press(Message::DismissBubble(i))).id(format!("hit:bubble:{i}"));
            // Slide up 12px while fading in.
            layers = layers.push(pin(card).x((WIN_W - 220.0) / 2.0).y(y + 12.0 * (1.0 - t)));
        }

        let menu_t = self.menu.interpolate(0.0f32, 1.0f32, now);
        if menu_t > 0.0 {
            let menu = container(
                column![
                    button(text("Settings").size(13)).on_press(Message::OpenSettings).style(button::text),
                    button(text("Quit").size(13)).on_press(Message::Quit).style(button::text),
                ]
                .spacing(2),
            )
            .padding(6)
            .style(move |theme: &Theme| container::rounded_box(theme).background(
                theme.palette().background.scale_alpha(menu_t),
            ))
            .id("hit:menu");
            layers = layers.push(pin(menu).x(sprite_x + sprite_w - 20.0).y(sprite_y + 10.0));
        }

        hit_regions::HitRegions::new(layers, Message::HitRegions).into()
    }
}
