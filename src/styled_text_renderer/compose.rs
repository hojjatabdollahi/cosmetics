// SPDX-License-Identifier: MPL-2.0

use super::raster::Rasterized;
use super::shadow;
use super::style::TextStyle;
use cosmic::iced::core::Color;
use image_crate::{Rgba, RgbaImage};

pub fn compose(r: &Rasterized, style: &TextStyle) -> RgbaImage {
    let pad = style.padding;
    let border_t = style.border.map(|b| b.thickness.max(0.0)).unwrap_or(0.0);
    let (sox, soy) = style.shadow.map(|s| s.offset).unwrap_or((0.0, 0.0));
    let s_blur = style.shadow.map(|s| s.blur.max(0.0)).unwrap_or(0.0);

    let box_w = r.width as f32 + pad.left + pad.right + 2.0 * border_t;
    let box_h = r.height as f32 + pad.top + pad.bottom + 2.0 * border_t;

    // Canvas grows on whichever sides the shadow spills onto.
    let ext_l = s_blur + (-sox).max(0.0);
    let ext_r = s_blur + sox.max(0.0);
    let ext_t = s_blur + (-soy).max(0.0);
    let ext_b = s_blur + soy.max(0.0);

    let canvas_w = (box_w + ext_l + ext_r).ceil().max(1.0) as u32;
    let canvas_h = (box_h + ext_t + ext_b).ceil().max(1.0) as u32;

    let box_x = ext_l;
    let box_y = ext_t;
    let text_x = box_x + border_t + pad.left;
    let text_y = box_y + border_t + pad.top;

    let mut canvas = RgbaImage::from_pixel(canvas_w, canvas_h, Rgba([0u8, 0, 0, 0]));

    if let Some(bg) = style.background {
        fill_rounded_rect(
            &mut canvas,
            box_x,
            box_y,
            box_w,
            box_h,
            style.border_radius,
            color_to_u8(bg),
        );
    }

    if let Some(b) = style.border.filter(|b| b.thickness > 0.0) {
        let inset = b.thickness * 0.5;
        stroke_rounded_rect(
            &mut canvas,
            box_x + inset,
            box_y + inset,
            box_w - b.thickness,
            box_h - b.thickness,
            (style.border_radius - inset).max(0.0),
            b.thickness,
            color_to_u8(b.color),
        );
    }

    if let Some(s) = style.shadow {
        let (shadow_img, pad_px) = shadow::blur_and_tint(
            &r.alpha,
            r.width,
            r.height,
            s.blur.max(0.0),
            color_to_u8(s.color),
        );
        composite(
            &mut canvas,
            &shadow_img,
            (text_x + sox - pad_px as f32).round() as i32,
            (text_y + soy - pad_px as f32).round() as i32,
        );
    }

    composite(
        &mut canvas,
        &r.image,
        text_x.round() as i32,
        text_y.round() as i32,
    );
    canvas
}

pub fn color_to_u8(c: Color) -> [u8; 4] {
    [
        (c.r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (c.g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (c.b.clamp(0.0, 1.0) * 255.0).round() as u8,
        (c.a.clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

pub fn blend_src_over(dst: &mut Rgba<u8>, src: [u8; 4]) {
    if src[3] == 0 {
        return;
    }
    let sa = src[3] as f32 / 255.0;
    let da = dst.0[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a < 1e-6 {
        return;
    }
    for c in 0..3 {
        let s = src[c] as f32 / 255.0;
        let d = dst.0[c] as f32 / 255.0;
        let o = (s * sa + d * da * (1.0 - sa)) / out_a;
        dst.0[c] = (o * 255.0).clamp(0.0, 255.0).round() as u8;
    }
    dst.0[3] = (out_a * 255.0).clamp(0.0, 255.0).round() as u8;
}

fn composite(dst: &mut RgbaImage, src: &RgbaImage, dx: i32, dy: i32) {
    let (dw, dh) = (dst.width() as i32, dst.height() as i32);
    let (sw, sh) = (src.width() as i32, src.height() as i32);
    for sy in 0..sh {
        let ty = sy + dy;
        if ty < 0 || ty >= dh {
            continue;
        }
        for sx in 0..sw {
            let tx = sx + dx;
            if tx < 0 || tx >= dw {
                continue;
            }
            let s = src.get_pixel(sx as u32, sy as u32).0;
            if s[3] == 0 {
                continue;
            }
            blend_src_over(dst.get_pixel_mut(tx as u32, ty as u32), s);
        }
    }
}

fn rounded_rect_sdf(px: f32, py: f32, rx: f32, ry: f32, rw: f32, rh: f32, radius: f32) -> f32 {
    let half_w = rw * 0.5;
    let half_h = rh * 0.5;
    let cx = rx + half_w;
    let cy = ry + half_h;
    let dx = (px - cx).abs() - half_w + radius;
    let dy = (py - cy).abs() - half_h + radius;
    let outside = dx.max(0.0).hypot(dy.max(0.0));
    let inside = dx.max(dy).min(0.0);
    outside + inside - radius
}

pub fn fill_rounded_rect(
    canvas: &mut RgbaImage,
    rx: f32,
    ry: f32,
    rw: f32,
    rh: f32,
    radius: f32,
    color: [u8; 4],
) {
    if rw <= 0.0 || rh <= 0.0 {
        return;
    }
    let radius = radius.clamp(0.0, rw.min(rh) * 0.5);
    let x0 = rx.floor().max(0.0) as u32;
    let y0 = ry.floor().max(0.0) as u32;
    let x1 = ((rx + rw).ceil() as i32).min(canvas.width() as i32).max(0) as u32;
    let y1 = ((ry + rh).ceil() as i32).min(canvas.height() as i32).max(0) as u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let d = rounded_rect_sdf(x as f32 + 0.5, y as f32 + 0.5, rx, ry, rw, rh, radius);
            let coverage = (0.5 - d).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let a = ((color[3] as f32 / 255.0) * coverage * 255.0).round() as u8;
            blend_src_over(
                canvas.get_pixel_mut(x, y),
                [color[0], color[1], color[2], a],
            );
        }
    }
}

pub fn stroke_rounded_rect(
    canvas: &mut RgbaImage,
    rx: f32,
    ry: f32,
    rw: f32,
    rh: f32,
    radius: f32,
    thickness: f32,
    color: [u8; 4],
) {
    if rw <= 0.0 || rh <= 0.0 || thickness <= 0.0 {
        return;
    }
    let radius = radius.clamp(0.0, rw.min(rh) * 0.5);
    let half_t = thickness * 0.5;
    let x0 = (rx - half_t).floor().max(0.0) as u32;
    let y0 = (ry - half_t).floor().max(0.0) as u32;
    let x1 = ((rx + rw + half_t).ceil() as i32)
        .min(canvas.width() as i32)
        .max(0) as u32;
    let y1 = ((ry + rh + half_t).ceil() as i32)
        .min(canvas.height() as i32)
        .max(0) as u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let d = rounded_rect_sdf(x as f32 + 0.5, y as f32 + 0.5, rx, ry, rw, rh, radius);
            let stroke_d = d.abs() - half_t;
            let coverage = (0.5 - stroke_d).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let a = ((color[3] as f32 / 255.0) * coverage * 255.0).round() as u8;
            blend_src_over(
                canvas.get_pixel_mut(x, y),
                [color[0], color[1], color[2], a],
            );
        }
    }
}
