// SPDX-License-Identifier: MPL-2.0

//! Container widget with image background, rounded corner clipping, blur, and overlay tint.
//!
//! # Example
//!
//! ```ignore
//! use cosmetics::widgets::image_container::{image_container, blur_image};
//! use cosmic::iced::core::{image, Color};
//!
//! let handle = image::Handle::from_path("background.jpg");
//! let blurred = blur_image(&handle, 15.0);
//!
//! image_container(blurred.clone(), cosmic::widget::text("Hello"))
//!     .width(cosmic::iced::Length::Fill)
//!     .border_radius(12.0)
//!     .overlay(Color::BLACK, 0.5)
//!     .into()
//! ```

mod rounded_primitive;
mod threaded_loader;
mod widget;

pub use threaded_loader::{
    PreparedImage, PreparedImageCacheKey, PreparedImageEvent, PreparedImageRequest,
    PreparedImageSource, ThreadedImagePipeline, ThreadedImagePipelineConfig,
};
pub use widget::ImageContainer;

use cosmic::iced::core::{Element, image};
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::{LazyLock, Mutex},
};

pub fn image_container<'a, Message>(
    handle: impl Into<image::Handle>,
    content: impl Into<Element<'a, Message, cosmic::Theme, cosmic::Renderer>>,
) -> ImageContainer<'a, Message>
where
    Message: 'a,
{
    ImageContainer::new(handle, content)
}

type BlurCacheKey = (u64, u32);

static BLUR_IMAGE_CACHE: LazyLock<Mutex<HashMap<BlurCacheKey, Option<image::Handle>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

const BLUR_IMAGE_CACHE_LIMIT: usize = 128;

/// Blur an image with shared in-process memoization.
pub fn blur_image_cached(handle: &image::Handle, sigma: f32) -> Option<image::Handle> {
    let key = (handle_hash(handle), sigma.to_bits());

    {
        let cache = BLUR_IMAGE_CACHE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = cache.get(&key) {
            return cached.clone();
        }
    }

    let blurred = blur_image(handle, sigma);

    let mut cache = BLUR_IMAGE_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if cache.len() >= BLUR_IMAGE_CACHE_LIMIT && !cache.contains_key(&key) {
        cache.clear();
    }

    cache.insert(key, blurred.clone());
    blurred
}

/// Blur an image on the CPU. No memoization; use `blur_image_cached` for reuse.
pub fn blur_image(handle: &image::Handle, sigma: f32) -> Option<image::Handle> {
    let rgba: image_crate::RgbaImage = match handle {
        image::Handle::Path(_, path) => image_crate::open(path).ok()?.into_rgba8(),
        image::Handle::Bytes(_, bytes) => image_crate::load_from_memory(bytes).ok()?.into_rgba8(),
        image::Handle::Rgba {
            width,
            height,
            pixels,
            ..
        } => image_crate::RgbaImage::from_raw(*width, *height, pixels.to_vec())?,
    };

    if sigma <= 0.0 {
        let (w, h) = rgba.dimensions();
        return Some(image::Handle::from_rgba(w, h, rgba.into_raw()));
    }

    // Downsample 4x before blur; GPU scales it back up
    let (w, h) = rgba.dimensions();
    let small = image_crate::imageops::resize(
        &rgba,
        (w / 4).max(1),
        (h / 4).max(1),
        image_crate::imageops::FilterType::Triangle,
    );

    let (sw, sh) = small.dimensions();
    let mut bytes = small.into_raw();
    let mut dst =
        libblur::BlurImageMut::borrow(&mut bytes, sw, sh, libblur::FastBlurChannels::Channels4);
    libblur::stack_blur(
        &mut dst,
        libblur::AnisotropicRadius::new(sigma as u32),
        libblur::ThreadingPolicy::Adaptive,
    )
    .ok()?;

    Some(image::Handle::from_rgba(sw, sh, bytes))
}

fn handle_hash(handle: &image::Handle) -> u64 {
    let mut hasher = DefaultHasher::new();

    match handle {
        image::Handle::Path(id, _) => id.hash(&mut hasher),
        image::Handle::Bytes(id, _) => id.hash(&mut hasher),
        image::Handle::Rgba { id, .. } => id.hash(&mut hasher),
    }

    hasher.finish()
}
