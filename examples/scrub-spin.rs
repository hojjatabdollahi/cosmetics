use cosmetics::widgets::scrub_spin::scrub_spin;
use cosmic::{
    Application,
    app::{Settings, Task},
    executor,
    iced::{Alignment, Length},
    widget,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default()
        .size(cosmic::iced::Size::new(560.0, 360.0))
        .debug(false);
    cosmic::app::run::<Demo>(settings, ())?;
    Ok(())
}

struct Demo {
    core: cosmic::Core,
    count: f64,
    opacity: f64,
    angle: f64,
}

#[derive(Debug, Clone)]
enum Message {
    Count(f64),
    Opacity(f64),
    Angle(f64),
}

impl Application for Demo {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.example.scrub-spin-demo";

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
                count: 8.0,
                opacity: 50.0,
                angle: 0.0,
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::Count(v) => self.count = v,
            Message::Opacity(v) => self.opacity = v,
            Message::Angle(v) => self.angle = v,
        }
        Task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        fn labeled<'a>(
            label: &'static str,
            field: cosmic::Element<'a, Message>,
            readout: String,
        ) -> cosmic::Element<'a, Message> {
            widget::row::with_children(vec![
                widget::text::body(label).width(Length::Fixed(90.0)).into(),
                field,
                widget::text::caption(readout).into(),
            ])
            .spacing(16)
            .align_y(Alignment::Center)
            .into()
        }

        let count = scrub_spin(1.0..=99.0, self.count)
            .step(1.0)
            .on_change(Message::Count)
            .on_release(Message::Count);

        let opacity = scrub_spin(0.0..=100.0, self.opacity)
            .step(1.0)
            .shift_step(0.1)
            .decimals(1)
            .on_change(Message::Opacity)
            .on_release(Message::Opacity);

        let angle = scrub_spin(-180.0..=180.0, self.angle)
            .step(1.0)
            .shift_step(0.5)
            .decimals(1)
            .width(Length::Fixed(180.0))
            .on_change(Message::Angle)
            .on_release(Message::Angle);

        widget::container(
            widget::column::with_children(vec![
                widget::text::title3("Scrub-spin number field").into(),
                widget::text::caption(
                    "Drag the value to scrub · click to type (try 100/2) · ▲▼ to step · \
                     Shift = fine · wheel / arrows when hovered.",
                )
                .into(),
                labeled("Count", count.into(), format!("= {}", self.count as i64)),
                labeled("Opacity", opacity.into(), format!("= {:.1}%", self.opacity)),
                labeled("Angle", angle.into(), format!("= {:.1}°", self.angle)),
            ])
            .spacing(18)
            .width(Length::Fixed(460.0)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    }
}
