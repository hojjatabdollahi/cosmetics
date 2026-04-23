// SPDX-License-Identifier: MPL-2.0

use super::compose::{blend_src_over, color_to_u8};
use super::style::{TextAlign, TextStyle};
use cosmic_text::{
    Align, Attrs, Buffer, Color as CosmicColor, Family, FontSystem, Metrics, Shaping, Style,
    SwashCache, Weight,
};
use image_crate::{Rgba, RgbaImage};

pub struct Rasterized {
    pub image: RgbaImage,
    pub alpha: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn rasterize(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    text: &str,
    style: &TextStyle,
) -> Rasterized {
    let font_size = style.font_size.max(1.0);
    let line_height = if style.line_height > 0.0 {
        style.line_height
    } else {
        font_size * 1.25
    };

    let family = style
        .font_family
        .as_deref()
        .map(Family::Name)
        .unwrap_or(Family::SansSerif);
    let attrs = Attrs::new()
        .family(family)
        .weight(Weight(style.font_weight))
        .style(if style.italic {
            Style::Italic
        } else {
            Style::Normal
        });
    let align = match style.text_align {
        TextAlign::Left => None,
        TextAlign::Center => Some(Align::Center),
        TextAlign::Right => Some(Align::Right),
    };

    let mut buffer = Buffer::new(font_system, Metrics::new(font_size, line_height));
    buffer.set_size(style.max_width.filter(|w| w.is_finite() && *w > 0.0), None);
    buffer.set_text(text, &attrs, Shaping::Advanced, align);
    buffer.shape_until_scroll(font_system, false);

    let mut text_w: f32 = 0.0;
    let mut line_count: u32 = 0;
    for run in buffer.layout_runs() {
        if run.line_w.is_finite() {
            text_w = text_w.max(run.line_w);
        }
        line_count += 1;
    }
    let width = text_w.ceil().max(1.0) as u32;
    let height = (line_count.max(1) as f32 * line_height).ceil().max(1.0) as u32;

    let mut image = RgbaImage::from_pixel(width, height, Rgba([0u8, 0, 0, 0]));
    let mut alpha = vec![0u8; (width * height) as usize];

    let [r, g, b, _] = color_to_u8(style.text_color);
    let default_color = CosmicColor::rgb(r, g, b);

    buffer.draw(
        font_system,
        swash_cache,
        default_color,
        |px, py, pw, ph, color| {
            let a = color.a();
            if a == 0 {
                return;
            }
            let src = [color.r(), color.g(), color.b(), a];
            for dy in 0..ph {
                let y = py + dy as i32;
                if y < 0 || y as u32 >= height {
                    continue;
                }
                let y = y as u32;
                for dx in 0..pw {
                    let x = px + dx as i32;
                    if x < 0 || x as u32 >= width {
                        continue;
                    }
                    let x = x as u32;
                    blend_src_over(image.get_pixel_mut(x, y), src);
                    let idx = (y * width + x) as usize;
                    if a > alpha[idx] {
                        alpha[idx] = a;
                    }
                }
            }
        },
    );

    Rasterized {
        image,
        alpha,
        width,
        height,
    }
}
