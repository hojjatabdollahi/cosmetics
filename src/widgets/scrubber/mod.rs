// SPDX-License-Identifier: MPL-2.0

//! Player scrubber with playhead and trim handles for timeline editing.
//!
//!
//! # Example
//!
//! ```ignore
//! use cosmetics::widgets::scrubber::scrubber;
//!
//! scrubber(0.0..=10.0, self.position, (self.trim_start, self.trim_end))
//!     .on_scrub(Message::Seek)
//!     .on_trim(Message::TrimChanged)
//!     .step(0.01)
//! ```

mod widget;

pub use widget::{Scrubber, scrubber};
