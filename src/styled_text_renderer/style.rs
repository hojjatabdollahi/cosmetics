// SPDX-License-Identifier: MPL-2.0

use cosmic::iced::core::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowStyle {
    pub color: Color,
    pub offset: (f32, f32),
    pub blur: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderStyle {
    pub color: Color,
    pub thickness: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    pub fn uniform(v: f32) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }
    pub fn xy(x: f32, y: f32) -> Self {
        Self {
            top: y,
            right: x,
            bottom: y,
            left: x,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub struct TextStyle {
    pub font_size: f32,
    // 0.0 -> font_size * 1.25
    pub line_height: f32,
    pub text_color: Color,
    pub font_family: Option<String>,
    pub font_weight: u16,
    pub italic: bool,
    pub text_align: TextAlign,
    pub max_width: Option<f32>,
    pub background: Option<Color>,
    pub border: Option<BorderStyle>,
    pub shadow: Option<ShadowStyle>,
    pub padding: Padding,
    pub border_radius: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 32.0,
            line_height: 0.0,
            text_color: Color::WHITE,
            font_family: None,
            font_weight: 400,
            italic: false,
            text_align: TextAlign::Left,
            max_width: None,
            background: None,
            border: None,
            shadow: None,
            padding: Padding::uniform(8.0),
            border_radius: 0.0,
        }
    }
}
