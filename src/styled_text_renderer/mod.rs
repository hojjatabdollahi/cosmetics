// SPDX-License-Identifier: MPL-2.0

//! CPU rasterizer that turns styled text into an `image::Handle`.
//!
//! Text + style -> rounded container + optional border + optional shadow +
//! shaped glyphs, composited on a transparent RGBA canvas. Not a widget
//! layering/drag/resize is the caller's job.

mod compose;
mod raster;
mod shadow;
mod style;

pub use style::{BorderStyle, Padding, ShadowStyle, TextAlign, TextStyle};

use cosmic::iced::core::image;
use cosmic_text::{FontSystem, SwashCache};
use std::sync::{LazyLock, Mutex};

static FONT_SYSTEM: LazyLock<Mutex<FontSystem>> = LazyLock::new(|| Mutex::new(FontSystem::new()));

static SWASH_CACHE: LazyLock<Mutex<SwashCache>> = LazyLock::new(|| Mutex::new(SwashCache::new()));

pub fn render_styled_text(text: &str, style: &TextStyle) -> image::Handle {
    let mut fs = FONT_SYSTEM.lock().unwrap();
    let mut sc = SWASH_CACHE.lock().unwrap();
    render_styled_text_with(&mut fs, &mut sc, text, style)
}

/// Variant that takes a caller-owned `FontSystem` / `SwashCache`. Use this when
/// the host app already has one it wants to share.
pub fn render_styled_text_with(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    text: &str,
    style: &TextStyle,
) -> image::Handle {
    let rgba = render_styled_text_rgba_with(font_system, swash_cache, text, style);
    let (w, h) = (rgba.width(), rgba.height());
    image::Handle::from_rgba(w, h, rgba.into_raw())
}

/// Rasterize styled text and return the composited RGBA image directly. Useful
/// when the caller needs to composite the result onto another image (e.g. for
/// saving an annotated screenshot) rather than display it as an iced widget.
pub fn render_styled_text_rgba(text: &str, style: &TextStyle) -> image_crate::RgbaImage {
    let mut fs = FONT_SYSTEM.lock().unwrap();
    let mut sc = SWASH_CACHE.lock().unwrap();
    render_styled_text_rgba_with(&mut fs, &mut sc, text, style)
}

pub fn render_styled_text_rgba_with(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    text: &str,
    style: &TextStyle,
) -> image_crate::RgbaImage {
    let rasterized = raster::rasterize(font_system, swash_cache, text, style);
    compose::compose(&rasterized, style)
}
