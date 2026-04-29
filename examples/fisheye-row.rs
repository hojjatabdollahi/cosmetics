// SPDX-License-Identifier: MPL-2.0

//! Two fisheye rows: one with icons, one with numbered buttons.
//!
//! Run with: `cargo run --example fisheye-row`

use cosmetics::widgets::expandable_fisheye_row::ExpandableFisheyeRow;
use cosmetics::widgets::fisheye_row::FisheyeRow;
use cosmetics::widgets::scroll_fisheye_row::ScrollFisheyeRow;
use cosmic::{
    Application,
    app::{self, Settings, Task},
    executor,
    iced::{Alignment, Length, Subscription, keyboard},
    widget,
};
use cosmic::iced::widget::operation as iced_operation;

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
    ("preferences-desktop-screensaver-symbolic", "Screensaver"),
    ("preferences-desktop-display-symbolic", "Monitor"),
    ("audio-headphones-symbolic", "Audio"),
    ("battery-symbolic", "Battery"),
];

const NUMBER_COUNT: usize = 20;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default()
        .size(cosmic::iced::Size::new(1000.0, 540.0))
        .debug(false);
    cosmic::app::run::<App>(settings, ())?;
    Ok(())
}

struct App {
    core: app::Core,
    icon_active: usize,
    number_active: usize,
    scroll_active: usize,
    expand_active: usize,
}

#[derive(Debug, Clone)]
enum Message {
    IconSelected(usize),
    NumberSelected(usize),
    ScrollSelected(usize),
    ExpandSelected(usize),
    FocusNext,
    FocusPrev,
}

impl Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "com.example.fisheye-row";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        (
            Self {
                core,
                icon_active: 0,
                number_active: 0,
                scroll_active: 0,
                expand_active: 0,
            },
            Task::none(),
        )
    }

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        let spacing = cosmic::theme::active().cosmic().spacing;

        let icon_row = ICONS.iter().enumerate().fold(
            FisheyeRow::<Message>::new()
                .active(self.icon_active)
                .comfortable_count(4)
                .height(108.0)
                .max_item_width(96.0)
                .on_select(Message::IconSelected),
            |row, (_, (icon, label))| row.push(icon_tile(icon, label)),
        );

        let number_row = (0..NUMBER_COUNT).fold(
            FisheyeRow::<Message>::new()
                .active(self.number_active)
                .comfortable_count(4)
                .height(72.0)
                .max_item_width(72.0)
                .on_select(Message::NumberSelected),
            |row, n| row.push(number_button(n)),
        );

        let scroll_row = (0..NUMBER_COUNT).fold(
            ScrollFisheyeRow::<Message>::new()
                .active(self.scroll_active)
                .visible_count(4)
                .height(72.0)
                .full_fraction(0.6)
                .max_item_width(80.0)
                .on_select(Message::ScrollSelected),
            |row, n| row.push(number_button(n)),
        );

        let expand_row = (0..NUMBER_COUNT).fold(
            ExpandableFisheyeRow::<Message>::new()
                .active(self.expand_active)
                .comfortable_count(4)
                .height(72.0)
                .max_item_width(80.0)
                .on_select(Message::ExpandSelected),
            |row, n| row.push(number_button(n)),
        );

        let icon_label = ICONS
            .get(self.icon_active)
            .map(|(_, n)| *n)
            .unwrap_or("");
        let icon_caption = format!("Selected: {}  ({}/{})", icon_label, self.icon_active + 1, ICONS.len());
        let number_caption = format!("Selected: {}  ({}/{})", self.number_active, self.number_active + 1, NUMBER_COUNT);
        let scroll_caption = format!("Selected: {}  ({}/{})", self.scroll_active, self.scroll_active + 1, NUMBER_COUNT);
        let expand_caption = format!("Selected: {}  ({}/{})", self.expand_active, self.expand_active + 1, NUMBER_COUNT);

        let column = widget::column::with_children(vec![
            widget::text::title3("Fisheye row").into(),
            widget::text::body(
                "Hover to focus an item; the row fans like book pages around the cursor. \
                 Move the cursor away and the row snaps back to the active item.",
            )
            .into(),
            widget::text::caption("Icons").into(),
            widget::container(icon_row)
                .width(Length::Fill)
                .padding(spacing.space_xs)
                .class(cosmic::style::Container::Card)
                .into(),
            widget::text::caption(icon_caption).into(),
            widget::Space::new().height(spacing.space_s).into(),
            widget::text::caption("Numbered buttons").into(),
            widget::container(number_row)
                .width(Length::Fill)
                .padding(spacing.space_xs)
                .class(cosmic::style::Container::Card)
                .into(),
            widget::text::caption(number_caption).into(),
            widget::Space::new().height(spacing.space_s).into(),
            widget::text::caption(
                "Scrolling fisheye — hover the compressed ends to auto-scroll",
            )
            .into(),
            widget::container(scroll_row)
                .width(Length::Fill)
                .padding(spacing.space_xs)
                .class(cosmic::style::Container::Card)
                .into(),
            widget::text::caption(scroll_caption).into(),
            widget::Space::new().height(spacing.space_s).into(),
            widget::text::caption(
                "Expandable fisheye — hover expands to a scrollable strip with arrows",
            )
            .into(),
            widget::container(expand_row)
                .width(Length::Fill)
                .padding(spacing.space_xs)
                .class(cosmic::style::Container::Card)
                .into(),
            widget::text::caption(expand_caption).into(),
        ])
        .spacing(spacing.space_xs)
        .padding(spacing.space_m)
        .width(Length::Fill);

        widget::container(column)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Start)
            .into()
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::IconSelected(i) => {
                if i < ICONS.len() {
                    self.icon_active = i;
                }
            }
            Message::NumberSelected(i) => {
                if i < NUMBER_COUNT {
                    self.number_active = i;
                }
            }
            Message::ScrollSelected(i) => {
                if i < NUMBER_COUNT {
                    self.scroll_active = i;
                }
            }
            Message::ExpandSelected(i) => {
                if i < NUMBER_COUNT {
                    self.expand_active = i;
                }
            }
            Message::FocusNext => {
                return iced_operation::focus_next();
            }
            Message::FocusPrev => {
                return iced_operation::focus_previous();
            }
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        // Translate global Tab / Shift+Tab into focus traversal commands so
        // the keyboard can move focus through focusable widgets.
        cosmic::iced::keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed { key, modifiers, .. } => match key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Tab) => {
                    Some(if modifiers.shift() {
                        Message::FocusPrev
                    } else {
                        Message::FocusNext
                    })
                }
                _ => None,
            },
            _ => None,
        })
    }
}

fn icon_tile<'a>(icon_name: &'a str, label: &'a str) -> cosmic::Element<'a, Message> {
    widget::column::with_children(vec![
        widget::icon::from_name(icon_name).size(40).into(),
        widget::text::caption(label).into(),
    ])
    .spacing(6)
    .align_x(Alignment::Center)
    .padding(8)
    .into()
}

fn number_button<'a>(n: usize) -> cosmic::Element<'a, Message> {
    widget::container(widget::text::title2(n.to_string()))
        .padding(8)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}
