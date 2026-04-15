// SPDX-License-Identifier: MPL-2.0

//! Keyed wrapping flex row with drag-to-reorder animation.
//!
//! Items wrap into rows and can be dragged to reorder. While dragging,
//! the active item lifts and siblings slide into their new positions.
//!
//! # Example
//!
//! ```ignore
//! use cosmetics::widgets::flex_row::flex_row;
//!
//! let row = flex_row(|from, to| Message::Reordered { from, to })
//!     .spacing(12.0)
//!     .push("alpha", widget::container("Alpha").padding(12))
//!     .push("beta", widget::container("Beta").padding(12))
//!     .push_locked("fixed", widget::container("Fixed").padding(12));
//! ```

mod widget;

pub use widget::{FlexRow, flex_row};
