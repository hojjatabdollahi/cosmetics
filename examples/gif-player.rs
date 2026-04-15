use cosmetics::widgets::gif_player::{self, Frames};
use cosmic::{
    Application,
    app::{Settings, Task},
    executor,
    iced::{Alignment, Length},
    widget,
};
use image_crate::codecs::gif::GifDecoder;
use image_crate::{AnimationDecoder, RgbaImage};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default()
        .size(cosmic::iced::Size::new(650.0, 500.0))
        .debug(false);
    cosmic::app::run::<GifPlayerDemo>(settings, ())?;
    Ok(())
}

struct GifPlayerDemo {
    core: cosmic::Core,
    gif_frames: Option<Frames>,
    playing: bool,
    current_frame: usize,
    total_frames: usize,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePlay,
    FrameChanged(usize),
}

fn load_gif(path: &std::path::Path) -> Result<(Vec<RgbaImage>, u32), Box<dyn std::error::Error>> {
    let file = BufReader::new(File::open(path)?);
    let decoder = GifDecoder::new(file)?;
    let raw_frames: Vec<image_crate::Frame> = decoder.into_frames().collect_frames()?;

    let mut delay_ms = 100u32;
    let mut images = Vec::with_capacity(raw_frames.len());

    for (i, frame) in raw_frames.iter().enumerate() {
        let (numer, denom) = frame.delay().numer_denom_ms();
        let ms: u32 = if denom > 0 { numer / denom } else { 100 };
        if i == 0 {
            delay_ms = ms;
        }
        images.push(frame.buffer().clone());
    }

    Ok((images, delay_ms))
}

impl Application for GifPlayerDemo {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.example.gif-player-demo";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let gif_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/data/1.gif");

        let (frames, delay_ms) = match load_gif(&gif_path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to load GIF: {}", e);
                (Vec::new(), 100)
            }
        };

        let total_frames = frames.len();
        let gif_frames = if !frames.is_empty() {
            let images: Vec<(u32, u32, &[u8])> = frames
                .iter()
                .map(|img| (img.width(), img.height(), img.as_raw().as_slice()))
                .collect();
            Some(Frames::from_rgba(&images, delay_ms))
        } else {
            None
        };

        (
            Self {
                core,
                gif_frames,
                playing: false,
                current_frame: 0,
                total_frames,
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::TogglePlay => {
                self.playing = !self.playing;
            }
            Message::FrameChanged(index) => {
                self.current_frame = index;
            }
        }
        Task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        let content: cosmic::Element<'_, Message> = if let Some(ref gf) = self.gif_frames {
            let player = gif_player::gif_player(gf)
                .playing(self.playing)
                .on_frame(Message::FrameChanged)
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(cosmic::iced::ContentFit::Contain);

            widget::container(player)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into()
        } else {
            widget::text::body("No GIF loaded").into()
        };

        let play_label = if self.playing { "Pause" } else { "Play" };
        let play_btn = widget::button::standard(play_label).on_press(Message::TogglePlay);

        let info = widget::text::caption(format!(
            "Frame: {} / {}",
            self.current_frame, self.total_frames,
        ));

        widget::column::with_children(vec![
            content,
            widget::row::with_children(vec![play_btn.into(), info.into()])
                .spacing(16)
                .align_y(Alignment::Center)
                .into(),
        ])
        .spacing(8)
        .padding(12)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}
