// SPDX-License-Identifier: MPL-2.0

//! A segmented toggle widget with a sliding indicator.
//!
//! Shows a pill-shaped background with N evenly-spaced segments, each optionally
//! displaying an icon. The selected segment has an accent-colored circle behind it
//! that slides between positions using smootherstep easing (200ms default).
//!
//! Supports 2, 3, or more items. Horizontal and vertical orientations.
//! Animation is fully built-in, no external state management needed.
//!
//! # Example
//!
//! ```ignore
//! use cosmetics::widgets::toggle::{Toggle, toggle, toggle3};
//!
//! // 2-item toggle with boolean callback
//! toggle("sun-symbolic", "moon-symbolic", is_dark)
//!     .on_toggle(Message::ThemeChanged)
//!
//! // 3-item toggle with index callback
//! toggle3("photo-symbolic", "video-symbolic", "gif-symbolic", mode)
//!     .on_select(Message::ModeChanged)
//!
//! // N-item toggle with custom sizing
//! Toggle::with_icons(&["a", "b", "c", "d"], selected)
//!     .pill_thickness(42.0)
//!     .circle_size(36.0)
//!     .on_select(Message::Selected)
//!
//! // Plain toggle (no icons)
//! Toggle::plain(3, selected)
//!     .on_select(Message::Selected)
//! ```

mod widget;

pub use widget::{Toggle, toggle, toggle3};
