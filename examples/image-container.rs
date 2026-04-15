// SPDX-License-Identifier: MPL-2.0

//! ImageContainer example with threaded preparation + blur modes.
//!
//! Run with: `cargo run --features image-container --example image-container`

use cosmetics::widgets::image_container::{
    PreparedImageCacheKey, PreparedImageEvent, PreparedImageRequest, PreparedImageSource,
    ThreadedImagePipeline, ThreadedImagePipelineConfig, image_container,
};
use cosmic::{
    Application, Element,
    app::{self, Settings, Task},
    executor,
    iced::core::image,
    iced::{Alignment, Color, Length},
    theme, widget,
};
use std::time::Duration;

const IMAGE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/data/nasa.jpg");

const EVENT_WAIT_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_DECODE_DIMENSION: u32 = 1600;
const BLUR_SIGMA: f32 = 18.0;
const CORNER_RADIUS: f32 = 24.0;
const PAGE_BG: Color = Color::from_rgb(0.10, 0.10, 0.14);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default()
        .size(cosmic::iced::Size::new(600.0, 500.0))
        .debug(false);
    cosmic::app::run::<App>(settings, ())?;
    Ok(())
}

fn wait_for_loader_event(loader: ThreadedImagePipeline) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || loader.wait_next_timeout(EVENT_WAIT_TIMEOUT))
                .await
                .ok()
                .flatten()
        },
        |event| cosmic::Action::App(Message::LoaderEvent(event)),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Original,
    Blurred,
    BlurredTranslucent,
    Translucent,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Mode::Original => "Original",
            Mode::Blurred => "Blurred",
            Mode::BlurredTranslucent => "Blurred + Translucent",
            Mode::Translucent => "Translucent",
        }
    }

    fn next(self) -> Self {
        match self {
            Mode::Original => Mode::Blurred,
            Mode::Blurred => Mode::BlurredTranslucent,
            Mode::BlurredTranslucent => Mode::Translucent,
            Mode::Translucent => Mode::Original,
        }
    }
}

struct App {
    core: app::Core,
    loader: ThreadedImagePipeline,
    key: PreparedImageCacheKey,
    original: Option<image::Handle>,
    blurred: Option<image::Handle>,
    source: Option<PreparedImageSource>,
    loading: bool,
    mode: Mode,
}

#[derive(Debug, Clone)]
enum Message {
    ToggleMode,
    LoaderEvent(Option<PreparedImageEvent>),
}

impl Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "com.cosmetics.image_container_example";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: app::Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let loader = ThreadedImagePipeline::new(ThreadedImagePipelineConfig {
            worker_threads: 2,
            disk_cache_enabled: true,
            ..Default::default()
        });

        let request = PreparedImageRequest::new(IMAGE_PATH)
            .max_dimension(MAX_DECODE_DIMENSION)
            .blur_sigma(BLUR_SIGMA);
        let key = loader.request(request);

        (
            Self {
                core,
                loader: loader.clone(),
                key,
                original: None,
                blurred: None,
                source: None,
                loading: true,
                mode: Mode::Original,
            },
            wait_for_loader_event(loader),
        )
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let (handle, opacity) = match self.mode {
            Mode::Original => (self.original.clone(), 1.0_f32),
            Mode::Blurred => (
                self.blurred.clone().or_else(|| self.original.clone()),
                1.0,
            ),
            Mode::BlurredTranslucent => (
                self.blurred.clone().or_else(|| self.original.clone()),
                0.4,
            ),
            Mode::Translucent => (self.original.clone(), 0.4),
        };

        let status = if self.loading {
            "Loading...".to_string()
        } else if let Some(source) = self.source {
            format!(
                "Ready from {}",
                match source {
                    PreparedImageSource::MemoryCache => "memory cache",
                    PreparedImageSource::DiskCache => "disk cache",
                    PreparedImageSource::WorkerGenerated => "worker thread",
                }
            )
        } else {
            "Waiting...".to_string()
        };

        let card_content = widget::container(
            widget::column::with_children(vec![
                widget::text::title3("nasa.jpg")
                    .class(theme::Text::Color(Color::WHITE))
                    .into(),
                widget::Space::new().height(4).into(),
                widget::text::body(format!("Mode: {}", self.mode.label()))
                    .class(theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.85)))
                    .into(),
                widget::Space::new().height(12).into(),
                widget::text::caption(status)
                    .class(theme::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.65)))
                    .into(),
            ])
            .align_x(Alignment::Start),
        )
        .padding(28)
        .width(Length::Fill);

        let card: Element<'_, Message> = if let Some(handle) = handle {
            image_container(handle, card_content)
                .width(Length::Fixed(400.0))
                .height(Length::Shrink)
                .opacity(opacity)
                .border_radius(CORNER_RADIUS)
                .overlay(Color::BLACK, 0.5)
                .border(2.0, Color::from_rgba(1.0, 1.0, 1.0, 0.3))
                .into()
        } else {
            widget::container(card_content)
                .width(Length::Fixed(400.0))
                .height(Length::Shrink)
                .style(|_| widget::container::Style {
                    background: Some(Color::from_rgba(0.15, 0.15, 0.20, 1.0).into()),
                    ..Default::default()
                })
                .into()
        };

        let toggle_btn = widget::button::standard("Toggle mode").on_press(Message::ToggleMode);

        widget::container(
            widget::column::with_children(vec![
                card,
                widget::Space::new().height(20).into(),
                toggle_btn.into(),
            ])
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(|_| widget::container::Style {
            background: Some(PAGE_BG.into()),
            ..Default::default()
        })
        .into()
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::ToggleMode => {
                self.mode = self.mode.next();
                Task::none()
            }
            Message::LoaderEvent(Some(event)) => {
                match event {
                    PreparedImageEvent::Ready {
                        key, image, source, ..
                    } => {
                        if key == self.key {
                            self.original = Some(image.original);
                            self.blurred = image.blurred;
                            self.source = Some(source);
                            self.loading = false;
                        }
                    }
                    PreparedImageEvent::Failed { key, .. } => {
                        if key == self.key {
                            self.loading = false;
                        }
                    }
                }

                if self.loading {
                    wait_for_loader_event(self.loader.clone())
                } else {
                    Task::none()
                }
            }
            Message::LoaderEvent(None) => {
                if self.loading {
                    wait_for_loader_event(self.loader.clone())
                } else {
                    Task::none()
                }
            }
        }
    }
}
