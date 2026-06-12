// SPDX-License-Identifier: MIT OR Apache-2.0

//! Small reusable GTK widgets and render-hold helpers.
//!
//! GTK Lush widget primitives stay deliberately narrow. `ClipBin` is a
//! single-child clipping widget for flexible content that must yield to chrome.
//! `RenderHoldOverlay` owns the GTK mechanics for temporarily holding rendered
//! pixels while a caller-defined reflow or repair workflow settles.
//!
//! The crate does not own application timing, state machines, or readiness
//! rules, and it does not depend on any other GTK Lush crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod clip_bin;
mod render_hold;

pub use clip_bin::ClipBin;
pub use render_hold::{RenderHoldCapture, RenderHoldNotReady, RenderHoldOverlay};
