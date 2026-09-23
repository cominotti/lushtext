// SPDX-License-Identifier: MIT OR Apache-2.0

//! Small reusable GTK widgets and render-hold helpers.
//!
//! GTK Lush widget primitives stay deliberately narrow. `ClipBin` is a
//! single-child clipping widget for flexible content that must yield to chrome.
//! `ViewportSliceBin` hosts a `GtkScrollable` such as `GtkListView` inside an
//! outer scroller while keeping it virtualized, so long lists render every row
//! instead of stopping at `GtkListView`'s realized-widget cap; its two pure
//! decisions are exposed as [`viewport_slice`] (which band to allocate) and
//! [`outer_scroll_request`] (whether the child asked to scroll and how far the
//! outer scroller must travel). `RenderHoldOverlay` owns the GTK
//! mechanics for temporarily holding rendered pixels while a caller-defined
//! reflow or repair workflow settles.
//!
//! The crate does not own application timing, state machines, or readiness
//! rules, and it does not depend on any other GTK Lush crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod clip_bin;
mod render_hold;
mod scroll_request;
mod single_child;
mod slice_geometry;
mod viewport_slice_bin;

pub use clip_bin::ClipBin;
pub use render_hold::{RenderHoldCapture, RenderHoldNotReady, RenderHoldOverlay};
pub use scroll_request::{
    ADJUSTMENT_EPSILON, ChildScrollDecision, classify_child_scroll, outer_scroll_request,
};
pub use slice_geometry::{ViewportSlice, viewport_slice};
pub use viewport_slice_bin::ViewportSliceBin;
