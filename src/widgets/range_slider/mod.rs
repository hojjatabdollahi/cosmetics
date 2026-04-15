// SPDX-License-Identifier: MPL-2.0

//! Dual-handle range slider for selecting a (low, high) value pair.
//!
//! Styled to match cosmic/iced slider. Accent fill drawn between handles.
//!
//! # Example
//!
//! ```ignore
//! use cosmetics::widgets::range_slider::range_slider;
//!
//! range_slider(0.0..=100.0, (self.low, self.high), Message::RangeChanged)
//!     .step(1.0)
//! ```

mod widget;

pub use widget::{RangeSlider, range_slider};
