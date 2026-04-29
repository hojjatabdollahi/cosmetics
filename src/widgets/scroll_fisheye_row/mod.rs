// SPDX-License-Identifier: MPL-2.0

//! A scrolling fisheye row.
//!
//! The widget always devotes a fixed central fraction of its width
//! (`full_fraction`, default 60%) to full-size items and the remaining
//! width to progressively compressed previews on either side. Hovering
//! over a compressed region auto-scrolls the row toward that side, faster
//! the closer the cursor is to the widget edge. Mouse-leave snaps the
//! row so the active item is back in its place inside the full region.
//!
//! The layout split is asymmetric and follows the active item:
//! - active = first item → 0% left compressed, 60% full, 40% right compressed
//! - active in middle → 20% left compressed, 60% full, 20% right compressed
//! - active = last item → 40% left compressed, 60% full, 0% right compressed

mod widget;

pub use widget::{ScrollFisheyeRow, scroll_fisheye_row};
