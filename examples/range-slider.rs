use cosmetics::widgets::range_slider::range_slider;
use cosmic::{
    Application,
    app::{Settings, Task},
    executor,
    iced::{Alignment, Length},
    widget,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default()
        .size(cosmic::iced::Size::new(600.0, 400.0))
        .debug(false);
    cosmic::app::run::<RangeSliderDemo>(settings, ())?;
    Ok(())
}

struct RangeSliderDemo {
    core: cosmic::Core,
    price_low: f32,
    price_high: f32,
    year_low: f32,
    year_high: f32,
    fine_low: f32,
    fine_high: f32,
}

#[derive(Debug, Clone)]
enum Message {
    Price((f32, f32)),
    Year((f32, f32)),
    Fine((f32, f32)),
}

impl Application for RangeSliderDemo {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.example.range-slider-demo";

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
                price_low: 20.0,
                price_high: 80.0,
                year_low: 2000.0,
                year_high: 2020.0,
                fine_low: 0.2,
                fine_high: 0.8,
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::Price((lo, hi)) => {
                self.price_low = lo;
                self.price_high = hi;
            }
            Message::Year((lo, hi)) => {
                self.year_low = lo;
                self.year_high = hi;
            }
            Message::Fine((lo, hi)) => {
                self.fine_low = lo;
                self.fine_high = hi;
            }
        }
        Task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        let price_slider = widget::column::with_children(vec![
            widget::text::body("Price Range").into(),
            range_slider(
                0.0..=100.0,
                (self.price_low, self.price_high),
                Message::Price,
            )
            .step(1.0)
            .into(),
            widget::text::caption(format!("${:.0} – ${:.0}", self.price_low, self.price_high))
                .into(),
        ])
        .spacing(8);

        let year_slider = widget::column::with_children(vec![
            widget::text::body("Year Range").into(),
            range_slider(
                1980.0..=2025.0,
                (self.year_low, self.year_high),
                Message::Year,
            )
            .step(1.0)
            .breakpoints(&[1990.0, 2000.0, 2010.0, 2020.0])
            .into(),
            widget::text::caption(format!("{:.0} – {:.0}", self.year_low, self.year_high)).into(),
        ])
        .spacing(8);

        let fine_slider = widget::column::with_children(vec![
            widget::text::body("Fine Control (Shift for 0.01 step)").into(),
            range_slider(0.0..=1.0, (self.fine_low, self.fine_high), Message::Fine)
                .step(0.05)
                .shift_step(0.01)
                .into(),
            widget::text::caption(format!("{:.2} – {:.2}", self.fine_low, self.fine_high)).into(),
        ])
        .spacing(8);

        widget::container(
            widget::column::with_children(vec![
                widget::text::title3("Range Slider Demo").into(),
                price_slider.into(),
                year_slider.into(),
                fine_slider.into(),
            ])
            .spacing(24)
            .width(Length::Fixed(400.0)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    }
}
