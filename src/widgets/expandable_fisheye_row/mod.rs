// SPDX-License-Identifier: MPL-2.0

//! A windowed fisheye row with overflow caps.
//!
//! Only a legible window of items is shown at once: the focal item (under the
//! cursor, or the active item at rest) is largest and its neighbours taper down
//! gently — never to unreadable slivers. Items that don't fit collapse into a
//! count-bearing cap on each side (`‹N` / `N›`). Resting on a cap pans the
//! window one item at a time toward it; clicking a cap jumps a whole window;
//! the mouse wheel pans too. When the cursor leaves, the window snaps back to
//! centre the active item.

mod widget;

pub use widget::{ExpandableFisheyeRow, expandable_fisheye_row};
