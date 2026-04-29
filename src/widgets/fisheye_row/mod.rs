// SPDX-License-Identifier: MPL-2.0

//! A horizontal "fisheye" row that fans items like the pages of a book.
//!
//! When the row contains more items than `comfortable_count`, items farther
//! from the focal point compress and the focal item stays at full width.
//! The focal point follows the cursor while the cursor is over the row;
//! when the cursor leaves, the row animates back so the active item is
//! the focal one. Items overlap, with the focal item drawn on top.
//!
//! # Example
//!
//! ```ignore
//! use cosmetics::widgets::fisheye_row::FisheyeRow;
//!
//! FisheyeRow::new()
//!     .extend(items.iter().map(|i| make_item(i)))
//!     .active(selected)
//!     .comfortable_count(4)
//!     .height(96.0)
//!     .on_select(Message::Selected)
//! ```

mod widget;

pub use widget::{FisheyeRow, fisheye_row};
