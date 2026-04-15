use cosmetics::widgets::spinner::{Spinner, SpinnerState};
use cosmic::{
    Application,
    app::{Settings, Task},
    executor,
    iced::Length,
    iced::core::Color,
    widget,
};
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default()
        .size(cosmic::iced::Size::new(500.0, 400.0))
        .debug(false);
    cosmic::app::run::<SpinnerDemo>(settings, ())?;
    Ok(())
}

struct SpinnerDemo {
    core: cosmic::Core,
    spinner_small: SpinnerState,
    spinner_medium: SpinnerState,
    spinner_large: SpinnerState,
    loading: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Tick(Instant),
    ToggleLoading,
}

impl Application for SpinnerDemo {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.example.spinner-demo";

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
                spinner_small: SpinnerState::new(),
                spinner_medium: SpinnerState::new(),
                spinner_large: SpinnerState::new(),
                loading: true,
            },
            Task::none(),
        )
    }

    fn subscription(&self) -> cosmic::iced::Subscription<Self::Message> {
        if self.loading {
            cosmic::iced::time::every(Duration::from_millis(16)).map(Message::Tick)
        } else {
            cosmic::iced::Subscription::none()
        }
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::Tick(now) => {
                self.spinner_small.tick(now);
                self.spinner_medium.tick(now);
                self.spinner_large.tick(now);
            }
            Message::ToggleLoading => {
                self.loading = !self.loading;
                if self.loading {
                    self.spinner_small.reset();
                    self.spinner_medium.reset();
                    self.spinner_large.reset();
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        let small = Spinner::new(&self.spinner_small)
            .size(24.0)
            .stroke_width(2.5);

        let medium = Spinner::new(&self.spinner_medium)
            .size(48.0)
            .stroke_width(4.0);

        let large = Spinner::new(&self.spinner_large)
            .size(80.0)
            .stroke_width(5.0);

        let custom_color = Spinner::new(&self.spinner_medium)
            .size(48.0)
            .stroke_width(4.0)
            .color(Color::from_rgb(0.9, 0.3, 0.4));

        let status = if self.loading { "Spinning" } else { "Paused" };

        let toggle_btn = widget::button::standard(format!("{} (click to toggle)", status))
            .on_press(Message::ToggleLoading);

        let spinners_row = widget::row::with_children(vec![
            widget::column::with_children(vec![small.into(), widget::text::body("24px").into()])
                .spacing(8)
                .align_x(cosmic::iced::Alignment::Center)
                .into(),
            widget::column::with_children(vec![medium.into(), widget::text::body("48px").into()])
                .spacing(8)
                .align_x(cosmic::iced::Alignment::Center)
                .into(),
            widget::column::with_children(vec![large.into(), widget::text::body("80px").into()])
                .spacing(8)
                .align_x(cosmic::iced::Alignment::Center)
                .into(),
            widget::column::with_children(vec![
                custom_color.into(),
                widget::text::body("custom color").into(),
            ])
            .spacing(8)
            .align_x(cosmic::iced::Alignment::Center)
            .into(),
        ])
        .spacing(32)
        .align_y(cosmic::iced::Alignment::Center);

        widget::container(
            widget::column::with_children(vec![
                widget::text::title3("Spinner Widget Demo").into(),
                spinners_row.into(),
                toggle_btn.into(),
            ])
            .spacing(24)
            .align_x(cosmic::iced::Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(cosmic::iced::Alignment::Center)
        .align_y(cosmic::iced::Alignment::Center)
        .into()
    }
}
