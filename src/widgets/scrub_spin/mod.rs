// SPDX-License-Identifier: MPL-2.0

//! A numeric field that unifies drag-to-scrub with click-to-type.
//!
//! One control replaces a slider + label for entering exact numbers:
//! - **Drag** the value horizontally to scrub (Shift = fine, Ctrl on the
//!   steppers = ×10).
//! - **Click** the value to type an exact number; the field evaluates math
//!   expressions (`100/2`, `10+2*3`). Enter commits, Escape cancels.
//! - **▼/▲ buttons** step by one, with hold-to-accelerate.
//! - **Scroll wheel** steps; **arrows / PageUp-Down / Home-End** work when
//!   hovered. Typing a digit while hovered begins type-in mode.
//!
//! Deliberately has no rotary dial or momentum: research shows both hurt
//! precise value entry. See `memory/project_number_input_design.md`.

mod widget;

pub use widget::{ScrubSpin, scrub_spin};
