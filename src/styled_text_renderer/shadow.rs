// SPDX-License-Identifier: MPL-2.0

use image_crate::{Rgba, RgbaImage};

// Blur the alpha plane and tint it. Returns (padded image, pad in px per side).
pub fn blur_and_tint(alpha: &[u8], w: u32, h: u32, sigma: f32, tint: [u8; 4]) -> (RgbaImage, u32) {
    let pad = sigma.ceil().max(0.0) as u32;
    let pw = w + 2 * pad;
    let ph = h + 2 * pad;

    let mut plane = vec![0u8; (pw * ph) as usize];
    for y in 0..h {
        let src = (y * w) as usize;
        let dst = ((y + pad) * pw + pad) as usize;
        plane[dst..dst + w as usize].copy_from_slice(&alpha[src..src + w as usize]);
    }

    if sigma > 0.0 {
        let mut view =
            libblur::BlurImageMut::borrow(&mut plane, pw, ph, libblur::FastBlurChannels::Plane);
        let _ = libblur::stack_blur(
            &mut view,
            libblur::AnisotropicRadius::new(sigma.ceil().max(1.0) as u32),
            libblur::ThreadingPolicy::Adaptive,
        );
    }

    let tint_a = tint[3] as f32 / 255.0;
    let mut out = RgbaImage::new(pw, ph);
    for (i, pixel) in out.pixels_mut().enumerate() {
        let a = ((plane[i] as f32 / 255.0) * tint_a * 255.0).round() as u8;
        *pixel = Rgba([tint[0], tint[1], tint[2], a]);
    }
    (out, pad)
}
