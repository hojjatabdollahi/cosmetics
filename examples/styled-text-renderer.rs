// SPDX-License-Identifier: MPL-2.0

//! cargo run --features styled-text-renderer --example styled-text-renderer

use cosmetics::styled_text_renderer::{
    BorderStyle, Padding, ShadowStyle, TextStyle, render_styled_text,
};
use cosmic::{
    Application, Element,
    app::{self, Settings, Task},
    executor,
    iced::core::image,
    iced::{Alignment, Color, Length},
    widget,
};

const PAGE_BG: Color = Color::from_rgb(0.08, 0.09, 0.12);
const INITIAL_TEXT: &str = "Cosmetic Text";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default()
        .size(cosmic::iced::Size::new(820.0, 720.0))
        .debug(false);
    cosmic::app::run::<App>(settings, ())?;
    Ok(())
}

struct App {
    core: app::Core,
    text: String,
    font_size: f32,
    border_radius: f32,
    border_thickness: f32,
    shadow_blur: f32,
    shadow_offset_x: f32,
    shadow_offset_y: f32,
    text_color: Color,
    shadow_color: Color,
    cached: Option<image::Handle>,
}

#[derive(Debug, Clone)]
enum Message {
    Text(String),
    FontSize(f32),
    BorderRadius(f32),
    BorderThickness(f32),
    ShadowBlur(f32),
    ShadowOffsetX(f32),
    ShadowOffsetY(f32),
    TextColor(Color),
    ShadowColor(Color),
}

impl App {
    fn rebuild(&mut self) {
        let style = TextStyle {
            font_size: self.font_size,
            text_color: self.text_color,
            background: Some(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.4,
            }),
            border: (self.border_thickness > 0.0).then(|| BorderStyle {
                color: Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.85,
                },
                thickness: self.border_thickness,
            }),
            shadow: Some(ShadowStyle {
                color: self.shadow_color,
                offset: (self.shadow_offset_x, self.shadow_offset_y),
                blur: self.shadow_blur,
            }),
            padding: Padding::xy(18.0, 12.0),
            border_radius: self.border_radius,
            ..TextStyle::default()
        };
        self.cached = Some(render_styled_text(&self.text, &style));
    }
}

impl Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "com.cosmetics.styled_text_renderer_example";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: app::Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let mut app = Self {
            core,
            text: INITIAL_TEXT.to_string(),
            font_size: 64.0,
            border_radius: 16.0,
            border_thickness: 2.0,
            shadow_blur: 6.0,
            shadow_offset_x: 3.0,
            shadow_offset_y: 3.0,
            text_color: Color::WHITE,
            shadow_color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.85,
            },
            cached: None,
        };
        app.rebuild();
        (app, Task::none())
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::Text(v) => self.text = v,
            Message::FontSize(v) => self.font_size = v,
            Message::BorderRadius(v) => self.border_radius = v,
            Message::BorderThickness(v) => self.border_thickness = v,
            Message::ShadowBlur(v) => self.shadow_blur = v,
            Message::ShadowOffsetX(v) => self.shadow_offset_x = v,
            Message::ShadowOffsetY(v) => self.shadow_offset_y = v,
            Message::TextColor(c) => self.text_color = c,
            Message::ShadowColor(c) => self.shadow_color = c,
        }
        self.rebuild();
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let preview: Element<'_, Message> = if let Some(handle) = self.cached.clone() {
            widget::image(handle).into()
        } else {
            widget::Space::new().width(1).height(1).into()
        };

        let preview_area = widget::container(preview)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);

        let controls = widget::column::with_children(vec![
            widget::text::body("Text").into(),
            widget::text_input("", &self.text)
                .on_input(Message::Text)
                .into(),
            widget::Space::new().height(8).into(),
            labeled_slider("Font size", self.font_size, 8.0, 160.0, Message::FontSize),
            labeled_slider(
                "Border radius",
                self.border_radius,
                0.0,
                80.0,
                Message::BorderRadius,
            ),
            labeled_slider(
                "Border thickness",
                self.border_thickness,
                0.0,
                12.0,
                Message::BorderThickness,
            ),
            labeled_slider(
                "Shadow blur",
                self.shadow_blur,
                0.0,
                40.0,
                Message::ShadowBlur,
            ),
            labeled_slider(
                "Shadow offset X",
                self.shadow_offset_x,
                -20.0,
                20.0,
                Message::ShadowOffsetX,
            ),
            labeled_slider(
                "Shadow offset Y",
                self.shadow_offset_y,
                -20.0,
                20.0,
                Message::ShadowOffsetY,
            ),
            color_row("Text color", self.text_color, Message::TextColor),
            color_row("Shadow color", self.shadow_color, Message::ShadowColor),
        ])
        .spacing(6);

        let layout = widget::column::with_children(vec![
            preview_area.into(),
            widget::container(controls).padding(16).into(),
        ])
        .spacing(0);

        widget::container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| widget::container::Style {
                background: Some(PAGE_BG.into()),
                ..Default::default()
            })
            .into()
    }
}

fn labeled_slider<'a>(
    label: &'a str,
    value: f32,
    min: f32,
    max: f32,
    on_change: impl Fn(f32) -> Message + 'a,
) -> Element<'a, Message> {
    widget::row::with_children(vec![
        widget::text::body(label).width(Length::Fixed(140.0)).into(),
        widget::slider(min..=max, value, on_change).into(),
        widget::text::body(format!("{:.1}", value))
            .width(Length::Fixed(60.0))
            .into(),
    ])
    .align_y(Alignment::Center)
    .spacing(12)
    .into()
}

fn swatch<'a>(color: Color) -> Element<'a, Message> {
    widget::container(widget::Space::new().width(24).height(24))
        .width(Length::Fixed(24.0))
        .height(Length::Fixed(24.0))
        .style(move |_| widget::container::Style {
            background: Some(color.into()),
            ..Default::default()
        })
        .into()
}

fn color_row<'a>(label: &'a str, color: Color, ctor: fn(Color) -> Message) -> Element<'a, Message> {
    let channel = move |ch_label: &'a str, value: f32, set: fn(Color, f32) -> Color| {
        widget::row::with_children(vec![
            widget::text::body(ch_label)
                .width(Length::Fixed(14.0))
                .into(),
            widget::slider(0.0..=1.0, value, move |v| ctor(set(color, v))).into(),
        ])
        .align_y(Alignment::Center)
        .spacing(4)
        .width(Length::Fill)
        .into()
    };

    widget::row::with_children(vec![
        widget::text::body(label).width(Length::Fixed(140.0)).into(),
        swatch(color),
        channel("R", color.r, |c, v| Color { r: v, ..c }),
        channel("G", color.g, |c, v| Color { g: v, ..c }),
        channel("B", color.b, |c, v| Color { b: v, ..c }),
        channel("A", color.a, |c, v| Color { a: v, ..c }),
    ])
    .align_y(Alignment::Center)
    .spacing(12)
    .into()
}
