// SPDX-License-Identifier: MPL-2.0

//! Animated spinner for indicating loading/progress.
//!
//! Renders a rotating arc driven by periodic tick messages
//! (e.g. via `cosmic::iced::time::every`).
//!
//! # Example
//!
//! ```ignore
//! use cosmetics::widgets::spinner::{Spinner, SpinnerState};
//! use cosmic::iced::time;
//! use std::time::{Duration, Instant};
//!
//! struct App {
//!     spinner: SpinnerState,
//!     loading: bool,
//! }
//!
//! #[derive(Debug, Clone)]
//! enum Message {
//!     SpinnerTick(Instant),
//! }
//!
//! // In subscription():
//! fn subscription(&self) -> cosmic::iced::Subscription<Message> {
//!     if self.loading {
//!         time::every(Duration::from_millis(16)).map(Message::SpinnerTick)
//!     } else {
//!         cosmic::iced::Subscription::none()
//!     }
//! }
//!
//! // In update():
//! fn update(&mut self, message: Message) {
//!     match message {
//!         Message::SpinnerTick(now) => self.spinner.tick(now),
//!     }
//! }
//!
//! // In view():
//! fn view(&self) -> Element<'_, Message> {
//!     Spinner::new(&self.spinner)
//!         .size(48.0)
//!         .into()
//! }
//! ```

mod widget;

pub use widget::{Spinner, SpinnerState};
