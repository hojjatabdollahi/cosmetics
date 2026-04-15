// SPDX-License-Identifier: MPL-2.0

//! Drag-to-reorder icons in a wrapping flex row.
//!
//! Run with: `cargo run --example flex-row-dnd`

use cosmetics::widgets::flex_row::flex_row;
use cosmic::{
    Application,
    app::{self, Settings, Task},
    executor,
    iced::{Alignment, Length},
    widget,
};

const ICONS: &[(&str, &str)] = &[
    ("folder-symbolic", "Files"),
    ("preferences-system-symbolic", "Settings"),
    ("system-software-install-symbolic", "Software"),
    ("utilities-terminal-symbolic", "Terminal"),
    ("web-browser-symbolic", "Browser"),
    ("accessories-text-editor-symbolic", "Editor"),
    ("system-file-manager-symbolic", "Manager"),
    ("preferences-desktop-wallpaper-symbolic", "Wallpaper"),
    ("camera-photo-symbolic", "Camera"),
    ("multimedia-audio-player-symbolic", "Music"),
    ("video-display-symbolic", "Display"),
    ("network-wireless-symbolic", "Network"),
    ("input-keyboard-symbolic", "Keyboard"),
    ("printer-symbolic", "Printer"),
    ("weather-clear-symbolic", "Weather"),
    ("mail-unread-symbolic", "Mail"),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default()
        .size(cosmic::iced::Size::new(800.0, 500.0))
        .debug(false);
    cosmic::app::run::<App>(settings, ())?;
    Ok(())
}

struct App {
    core: app::Core,
    icons: Vec<(usize, &'static str, &'static str)>,
}

#[derive(Debug, Clone)]
enum Message {
    Reordered { from: usize, to: usize },
}

impl Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "com.example.flex-row-dnd";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let icons = ICONS
            .iter()
            .enumerate()
            .map(|(i, (icon, name))| (i, *icon, *name))
            .collect();
        (Self { core, icons }, Task::none())
    }

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        let spacing = cosmic::theme::active().cosmic().spacing;

        let row = self.icons.iter().fold(
            flex_row::<usize, Message>(|from, to| Message::Reordered { from, to })
                .spacing(spacing.space_s as f32)
                .padding(spacing.space_m)
                .width(Length::Fill)
                .height(Length::Shrink)
                .align_y(Alignment::Start)
                .drag_lift(10.0),
            |row, &(id, icon_name, label)| {
                row.push(id, icon_tile(icon_name, label))
            },
        );

        widget::container(
            widget::column::with_children(vec![
                widget::text::title3("Drag icons to reorder").into(),
                widget::container(row)
                    .width(Length::Fill)
                    .class(cosmic::style::Container::Card)
                    .into(),
            ])
            .spacing(spacing.space_s)
            .padding(spacing.space_m)
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::Reordered { from, to } => {
                if from != to && from < self.icons.len() && to < self.icons.len() {
                    let item = self.icons.remove(from);
                    self.icons.insert(to, item);
                }
                Task::none()
            }
        }
    }
}

fn icon_tile<'a>(icon_name: &'a str, label: &'a str) -> cosmic::Element<'a, Message> {
    widget::container(
        widget::column::with_children(vec![
            widget::icon::from_name(icon_name)
                .size(32)
                .into(),
            widget::text::caption(label).into(),
        ])
        .spacing(4)
        .align_x(Alignment::Center),
    )
    .padding(12)
    .width(80)
    .align_x(Alignment::Center)
    .into()
}
