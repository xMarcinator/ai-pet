//! The pet's menu, styled like the C#'s ContextMenu (App.axaml: a dark rounded panel, Fluent menu items).

use iced::widget::{Space, button, canvas, column, container, row, text};
use iced::{Alignment, Border, Color, Element, Length, Shadow};

use crate::geometry::{MENU, MENU_ITEM_H, MENU_RADIUS, MENU_SEPARATOR_H};
use crate::style::{self, Glyph, Icon, argb};
use crate::{MenuItem, Message, PetUi};

/// Room for the check mark before each item's label.
const CHECK_COLUMN: f32 = 28.0;

impl PetUi {
    /// The menu: the two quick toggles, Avatars…, Settings… and Quit. It is [`MENU`] in size, for a popup surface of
    /// that size or to draw inline above the pet ([`PetUi::set_inline_menu`]). Choosing an item sends
    /// [`Message::Menu`].
    pub fn menu_view(&self) -> Element<'_, Message> {
        let accent = self.accent();
        let item = |label: &'static str, checked: Option<bool>, choice: MenuItem| -> Element<'_, Message> {
            let check = Glyph {
                icon: Icon::Check,
                colour: accent,
                width: 1.9,
            };
            let size = check.size();
            let mark: Element<'_, Message> = if checked == Some(true) {
                canvas(check).width(size.width).height(size.height).into()
            } else {
                Space::new().width(size.width).height(size.height).into()
            };
            let label = text(label).size(14).font(style::UI).color(style::ICON);
            button(row![container(mark).width(CHECK_COLUMN), label].align_y(Alignment::Center))
                .width(Length::Fill)
                .height(MENU_ITEM_H)
                .padding([0, 11])
                .on_press(Message::Menu(choice))
                .style(|_, status| button::Style {
                    background: matches!(status, button::Status::Hovered | button::Status::Pressed)
                        .then(|| argb(0x1AFFFFFF).into()),
                    text_color: style::ICON,
                    border: Border {
                        radius: 6.0.into(),
                        ..Border::default()
                    },
                    shadow: Shadow::default(),
                    snap: true,
                })
                .into()
        };
        // a 1 px rule in the middle of the separator's height
        let rule = container(Space::new().width(Length::Fill).height(1)).style(|_| container::Style {
            background: Some(argb(0x33FFFFFF).into()),
            ..container::Style::default()
        });
        let separator = container(rule).padding([(MENU_SEPARATOR_H - 1.0) / 2.0, 4.0]);
        container(column![
            item("Show bubbles", Some(self.bubbles), MenuItem::Bubbles),
            item("Always on top", Some(self.on_top), MenuItem::OnTop),
            item("Avatars…", None, MenuItem::Avatars),
            item("Settings…", None, MenuItem::Settings),
            separator,
            item("Quit", None, MenuItem::Quit),
        ])
        .padding(5)
        .width(MENU.width)
        .height(MENU.height)
        .style(|_| container::Style {
            background: Some(style::PANEL_BG.into()),
            border: Border {
                color: style::PANEL_EDGE,
                width: 1.0,
                radius: MENU_RADIUS.into(),
            },
            text_color: Some(Color::WHITE),
            ..container::Style::default()
        })
        .into()
    }
}
