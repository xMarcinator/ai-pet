//! Settings in the pet process without the GPU (fnt-75 task 9): an iced daemon on its CPU renderer (tiny-skia; the
//! wgpu feature is off, so no GPU library can be loaded) owns the main thread with no window open, and the minimal
//! prototype's layer-surface pet runs on a second thread with its own Wayland connection. They talk over channels:
//! the pet's click (or SPIKE_TOGGLE=<seconds>, a timer standing in for clicks) opens or closes a Settings-like
//! window; iced tells the pet when its window opens or closes.
//!
//! SPIKE_SECONDS=<n> quits after n seconds; SPIKE_DEBUG=1 logs the pet's frame rate every 2 s.

mod pet;

use std::sync::Mutex;

use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use iced::widget::{button, checkbox, column, container, pick_list, row, scrollable, slider, text, text_input, toggler};
use iced::{Element, Length, Subscription, Task, window};
use smithay_client_toolkit::reexports::calloop::channel::{Sender as PetSender, channel as pet_channel};

/// The pet to iced.
#[derive(Debug, Clone)]
pub enum ToUi {
    Toggle,
    Quit,
}

/// iced to the pet.
#[derive(Debug, Clone)]
pub enum FromUi {
    SettingsOpen(bool),
}

/// The pet's receiver, handed to the subscription once (Subscription::run takes a plain fn).
static FROM_PET: Mutex<Option<UnboundedReceiver<ToUi>>> = Mutex::new(None);

fn main() -> iced::Result {
    let (to_ui, from_pet) = unbounded::<ToUi>();
    let (to_pet, from_ui) = pet_channel::<FromUi>();
    *FROM_PET.lock().unwrap() = Some(from_pet);
    let pet = std::thread::Builder::new()
        .name("pet".into())
        .spawn(move || pet::linux::run(to_ui, from_ui))
        .unwrap();
    let to_pet = Mutex::new(Some(to_pet));
    let result = iced::daemon(
        move || {
            let to_pet = to_pet.lock().unwrap().take().expect("boots once");
            (Settings::new(to_pet), Task::none())
        },
        Settings::update,
        Settings::view,
    )
    .title(|_: &Settings, _| "Spike · Settings".to_string())
    .subscription(Settings::subscription)
    .run();
    let _ = pet.join();
    result
}

#[derive(Debug, Clone)]
enum Message {
    FromPet(ToUi),
    Opened(window::Id),
    Closed(window::Id),
    Name(String),
    Pick(&'static str),
    Check(bool),
    Toggle(bool),
    Size(f32),
    Save,
}

const AVATARS: [&str; 4] = ["Sprout", "Hood", "Pip", "Hamster"];

struct Settings {
    to_pet: PetSender<FromUi>,
    window: Option<window::Id>,
    name: String,
    avatar: Option<&'static str>,
    check: bool,
    toggle: bool,
    size: f32,
    saves: u32,
}

impl Settings {
    fn new(to_pet: PetSender<FromUi>) -> Self {
        Settings {
            to_pet,
            window: None,
            name: String::new(),
            avatar: Some("Sprout"),
            check: true,
            toggle: false,
            size: 100.0,
            saves: 0,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::FromPet(ToUi::Toggle) => match self.window {
                Some(id) => window::close(id),
                None => {
                    let (id, open) = window::open(window::Settings {
                        size: iced::Size::new(640.0, 420.0),
                        ..window::Settings::default()
                    });
                    self.window = Some(id);
                    open.map(Message::Opened)
                }
            },
            Message::FromPet(ToUi::Quit) => iced::exit(),
            Message::Opened(_) => {
                eprintln!("ui: window open");
                let _ = self.to_pet.send(FromUi::SettingsOpen(true));
                Task::none()
            }
            Message::Closed(id) => {
                if self.window == Some(id) {
                    self.window = None;
                    eprintln!("ui: window closed");
                    let _ = self.to_pet.send(FromUi::SettingsOpen(false));
                }
                Task::none()
            }
            Message::Name(s) => {
                self.name = s;
                Task::none()
            }
            Message::Pick(a) => {
                self.avatar = Some(a);
                Task::none()
            }
            Message::Check(b) => {
                self.check = b;
                Task::none()
            }
            Message::Toggle(b) => {
                self.toggle = b;
                Task::none()
            }
            Message::Size(v) => {
                self.size = v;
                Task::none()
            }
            Message::Save => {
                self.saves += 1;
                Task::none()
            }
        }
    }

    fn view(&self, _: window::Id) -> Element<'_, Message> {
        let mut rows = column![
            text("Settings on the CPU").size(22),
            text_input("Your name", &self.name).on_input(Message::Name),
            pick_list(&AVATARS[..], self.avatar, Message::Pick),
            checkbox(self.check).label("Watch Jira").on_toggle(Message::Check),
            toggler(self.toggle).label("Listen along").on_toggle(Message::Toggle),
            row![slider(50.0..=200.0, self.size, Message::Size).width(200), text(format!("{:.0} %", self.size))]
                .spacing(10),
            button("Save").on_press(Message::Save),
            text(format!("saved {} times", self.saves)),
        ]
        .spacing(12);
        for i in 0..20 {
            rows = rows.push(text(format!("Filler line {i}, to scroll")));
        }
        container(scrollable(rows.padding(20))).width(Length::Fill).height(Length::Fill).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            Subscription::run(from_pet).map(Message::FromPet),
            window::close_events().map(Message::Closed),
        ])
    }
}

fn from_pet() -> UnboundedReceiver<ToUi> {
    FROM_PET.lock().unwrap().take().expect("the pet's channel is subscribed once")
}
