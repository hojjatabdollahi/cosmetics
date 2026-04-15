// SPDX-License-Identifier: MPL-2.0

//! Frame-by-frame GIF player with trim support.
//!
//! Decodes all frames up-front into RGBA handles, then advances
//! via window redraw requests. No subscriptions or file I/O during playback.
//!
//! # Example
//!
//! ```ignore
//! use cosmetics::widgets::gif_player::{Frames, gif_player};
//!
//! let frames = Frames::from_rgba(&images, delay_ms);
//!
//! gif_player(&frames)
//!     .playing(self.playing)
//!     .trim(self.trim_start, self.trim_end)
//!     .on_frame(Message::FrameChanged)
//! ```

mod widget;

pub use widget::{Frames, GifPlayer, gif_player};
