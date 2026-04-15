use cosmetics::widgets::scrubber::scrubber;
use cosmic::{
    Application,
    app::{Settings, Task},
    executor,
    iced::{Alignment, Length, Subscription, time},
    widget,
};
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default()
        .size(cosmic::iced::Size::new(650.0, 350.0))
        .debug(false);
    cosmic::app::run::<ScrubberDemo>(settings, ())?;
    Ok(())
}

struct ScrubberDemo {
    core: cosmic::Core,
    duration: f32,
    position: f32,
    trim_start: f32,
    trim_end: f32,
    playing: bool,
    last_tick: Option<Instant>,
}

#[derive(Debug, Clone)]
enum Message {
    Seek(f32),
    TrimChanged((f32, f32)),
    ScrubRelease,
    TogglePlay,
    Tick(Instant),
}

impl Application for ScrubberDemo {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.example.scrubber-demo";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let dur = 8.5; // 8.5 second GIF
        (
            Self {
                core,
                duration: dur,
                position: 0.0,
                trim_start: 1.0,
                trim_end: 7.0,
                playing: false,
                last_tick: None,
            },
            Task::none(),
        )
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        if self.playing {
            time::every(Duration::from_millis(16)).map(Message::Tick)
        } else {
            Subscription::none()
        }
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::Seek(pos) => {
                self.position = pos;
            }
            Message::TrimChanged((s, e)) => {
                self.trim_start = s;
                self.trim_end = e;
                // Clamp playhead into trim region if playing
                if self.playing {
                    if self.position < self.trim_start {
                        self.position = self.trim_start;
                    } else if self.position > self.trim_end {
                        self.position = self.trim_end;
                    }
                }
            }
            Message::ScrubRelease => {
                // Could trigger a preview render here
            }
            Message::TogglePlay => {
                self.playing = !self.playing;
                if self.playing {
                    self.last_tick = None;
                    // Start from trim_start if outside trim region
                    if self.position < self.trim_start || self.position >= self.trim_end {
                        self.position = self.trim_start;
                    }
                }
            }
            Message::Tick(now) => {
                if let Some(prev) = self.last_tick {
                    let dt = (now - prev).as_secs_f32();
                    self.position += dt;
                    // Loop within trim region
                    if self.position >= self.trim_end {
                        self.position = self.trim_start;
                    }
                }
                self.last_tick = Some(now);
            }
        }
        Task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        let scrub = scrubber(
            0.0..=self.duration,
            self.position,
            (self.trim_start, self.trim_end),
        )
        .on_scrub(Message::Seek)
        .on_trim(Message::TrimChanged)
        .on_release(Message::ScrubRelease)
        .step(0.01)
        .shift_step(0.001)
        .height(28.0);

        let play_label = if self.playing { "Pause" } else { "Play" };
        let play_btn = widget::button::standard(play_label).on_press(Message::TogglePlay);

        let info = widget::text::caption(format!(
            "Position: {:.2}s  |  Trim: {:.2}s – {:.2}s  |  Output: {:.2}s",
            self.position,
            self.trim_start,
            self.trim_end,
            self.trim_end - self.trim_start,
        ));

        let hint = widget::text::caption(
            "Drag the playhead to scrub. Drag the thin bars at trim edges to adjust the output range.",
        );

        widget::container(
            widget::column::with_children(vec![
                widget::text::title3("GIF Scrubber Demo").into(),
                scrub.into(),
                widget::row::with_children(vec![play_btn.into(), info.into()])
                    .spacing(16)
                    .align_y(Alignment::Center)
                    .into(),
                hint.into(),
            ])
            .spacing(16)
            .width(Length::Fixed(500.0)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    }
}
