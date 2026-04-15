use cosmetics::widgets::toggle::{Toggle, toggle, toggle3};
use cosmic::{
    Application,
    app::{Settings, Task},
    executor,
    iced::{Alignment, Length},
    widget,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default()
        .size(cosmic::iced::Size::new(700.0, 500.0))
        .debug(false);
    cosmic::app::run::<ToggleDemo>(settings, ())?;
    Ok(())
}

struct ToggleDemo {
    core: cosmic::Core,
    theme_mode: bool,
    direction: bool,
    capture_mode: usize,
    plain_sel: usize,
    view_mode: usize,
}

#[derive(Debug, Clone)]
enum Message {
    ThemeChanged(bool),
    DirectionChanged(bool),
    CaptureMode(usize),
    PlainSel(usize),
    ViewMode(usize),
}

impl Application for ToggleDemo {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.example.toggle-demo";

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
                theme_mode: false,
                direction: false,
                capture_mode: 0,
                plain_sel: 1,
                view_mode: 0,
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::ThemeChanged(v) => self.theme_mode = v,
            Message::DirectionChanged(v) => self.direction = v,
            Message::CaptureMode(i) => self.capture_mode = i,
            Message::PlainSel(i) => self.plain_sel = i,
            Message::ViewMode(i) => self.view_mode = i,
        }
        Task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        // 2-item toggle (boolean callback)
        let two_item = widget::column::with_children(vec![
            widget::text::body("2 items").into(),
            toggle(
                "weather-clear-symbolic",
                "weather-clear-night-symbolic",
                self.theme_mode,
            )
            .on_toggle(Message::ThemeChanged)
            .into(),
            widget::text::caption(if self.theme_mode { "Night" } else { "Day" }).into(),
        ])
        .spacing(8)
        .align_x(Alignment::Center);

        // 2-item vertical
        let two_vertical = widget::column::with_children(vec![
            widget::text::body("Vertical").into(),
            toggle("go-up-symbolic", "go-down-symbolic", self.direction)
                .vertical()
                .on_toggle(Message::DirectionChanged)
                .into(),
            widget::text::caption(if self.direction { "Down" } else { "Up" }).into(),
        ])
        .spacing(8)
        .align_x(Alignment::Center);

        // 3-item toggle
        let three_item = widget::column::with_children(vec![
            widget::text::body("3 items").into(),
            toggle3(
                "camera-photo-symbolic",
                "camera-video-symbolic",
                "preferences-desktop-wallpaper-symbolic",
                self.capture_mode,
            )
            .on_select(Message::CaptureMode)
            .into(),
            widget::text::caption(match self.capture_mode {
                0 => "Photo",
                1 => "Video",
                _ => "Wallpaper",
            })
            .into(),
        ])
        .spacing(8)
        .align_x(Alignment::Center);

        // Plain 3-item (no icons)
        let plain = widget::column::with_children(vec![
            widget::text::body("Plain 3").into(),
            Toggle::plain(3, self.plain_sel)
                .pill_thickness(30.0)
                .circle_size(24.0)
                .on_select(Message::PlainSel)
                .into(),
            widget::text::caption(match self.plain_sel {
                0 => "A",
                1 => "B",
                _ => "C",
            })
            .into(),
        ])
        .spacing(8)
        .align_x(Alignment::Center);

        // 4-item toggle
        let four_item = widget::column::with_children(vec![
            widget::text::body("4 items").into(),
            Toggle::with_icons(
                &[
                    "view-grid-symbolic",
                    "view-list-symbolic",
                    "view-dual-symbolic",
                    "view-column-symbolic",
                ],
                self.view_mode,
            )
            .on_select(Message::ViewMode)
            .into(),
            widget::text::caption(match self.view_mode {
                0 => "Grid",
                1 => "List",
                2 => "Dual",
                _ => "Column",
            })
            .into(),
        ])
        .spacing(8)
        .align_x(Alignment::Center);

        let toggles_row = widget::row::with_children(vec![
            two_item.into(),
            two_vertical.into(),
            three_item.into(),
            plain.into(),
            four_item.into(),
        ])
        .spacing(32)
        .align_y(Alignment::Center);

        widget::container(
            widget::column::with_children(vec![
                widget::text::title3("Toggle Widget Demo").into(),
                toggles_row.into(),
            ])
            .spacing(32)
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    }
}
