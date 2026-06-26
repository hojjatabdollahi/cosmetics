// SPDX-License-Identifier: MPL-2.0

//! 3D Cover Flow window switcher (Compiz Shift-Switcher "Cover" style).
//!
//! Cards are textured quads drawn in perspective with a depth buffer and a
//! glossy floor reflection; the selected card faces the viewer while neighbours
//! rotate away and recede. Rendering is a custom `iced_wgpu` primitive; the
//! widget eases a continuous scroll position toward the active index so Tabbing
//! slides the stack smoothly.
//!
//! Requires the `cover-flow` feature.

mod camera;
mod primitive;
mod widget;

pub use widget::{CoverFlow, CoverFlowItem, cover_flow};
