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
pub(crate) mod a11;
pub(crate) mod a13;

use gtk4::prelude::*;

use std::time::Duration;

pub use a01::probe_a01;
pub use a02::probe_a02;
pub use a03::probe_a03;
pub use a04::probe_a04;
pub use a05::probe_a05;
pub use a06::probe_a06;
pub use a07::probe_a07;
pub use a09::probe_a09;
pub use a11::probe_a11;
pub use a13::probe_a13;

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
