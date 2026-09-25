// SPDX-License-Identifier: MIT OR Apache-2.0

//! One module per probed axiom, each exporting `probe_aNN() -> Observation`.
//!
//! A probe is a chain of steps recorded through the crate's `Recorder`:
//! **control** steps first (the fixture reached the state the axiom talks
//! about; a failure is `FixtureInvalid`), then **axiom** steps (the behaviour
//! holds; a failure is `Violated`). Every value a step reads is recorded
//! before the step checks it, so a failing observation shows what GTK did.

pub(crate) mod a01;
pub(crate) mod a02;
pub(crate) mod a03;
pub(crate) mod a04;
pub(crate) mod a05;
pub(crate) mod a06;
pub(crate) mod a07;
pub(crate) mod a09;
pub(crate) mod a10;
pub(crate) mod a11;
pub(crate) mod a13;
pub(crate) mod a14;
pub(crate) mod a15;
pub(crate) mod a16;
pub(crate) mod a17;
pub(crate) mod a18;
pub(crate) mod a19;
pub(crate) mod a20;
pub(crate) mod adaptive;

use gtk4::prelude::*;

use std::fmt::Display;
use std::time::Duration;

pub use a01::probe_a01;
pub use a02::probe_a02;
pub use a03::probe_a03;
pub use a04::probe_a04;
pub use a05::probe_a05;
pub use a06::probe_a06;
pub use a07::probe_a07;
pub use a09::probe_a09;
pub use a10::probe_a10;
pub use a11::probe_a11;
pub use a13::probe_a13;
pub use a14::probe_a14;
pub use a15::probe_a15;
pub use a16::probe_a16;
pub use a17::probe_a17;
pub use a18::probe_a18;
pub use a19::probe_a19;
pub use a20::probe_a20;

/// Settle after an adjustment or height change, before reading it back.
pub(crate) const LAYOUT_SETTLE: Duration = Duration::from_millis(200);
/// Settle after a `scroll_to`, which takes one more allocation to apply.
pub(crate) const SCROLL_SETTLE: Duration = Duration::from_millis(300);

/// Whether two adjustment values are the same to within float noise.
pub(crate) fn same_value(left: f64, right: f64) -> bool {
    (left - right).abs() < f64::EPSILON
}

/// The drawn height of one probe row: the list's content height over its
/// row count. Rows are drawn taller than they request (CSS padding), so a
/// probe derives positions from this rather than from `ROW_HEIGHT`.
#[must_use]
pub fn row_stride(adjustment: &gtk4::Adjustment, rows: u32) -> f64 {
    adjustment.upper() / f64::from(rows)
}

/// The frame counter of `widget`'s frame clock, or `-1` without one.
pub(crate) fn frame_of(widget: &impl IsA<gtk4::Widget>) -> i64 {
    widget
        .frame_clock()
        .map_or(-1, |clock| clock.frame_counter())
}

/// `entries` joined with ` | `, the separator every probe's event and
/// allocation logs use in their observations.
pub(crate) fn join_entries<T: Display>(entries: impl IntoIterator<Item = T>) -> String {
    entries
        .into_iter()
        .map(|entry| entry.to_string())
        .collect::<Vec<_>>()
        .join(" | ")
}
