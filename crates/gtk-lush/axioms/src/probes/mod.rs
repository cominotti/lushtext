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
pub(crate) mod a100;
pub(crate) mod a101;
pub(crate) mod a102;
pub(crate) mod a103;
pub(crate) mod a104;
pub(crate) mod a105;
pub(crate) mod a106;
pub(crate) mod a11;
pub(crate) mod a13;
pub(crate) mod a14;
pub(crate) mod a15;
pub(crate) mod a16;
pub(crate) mod a17;
pub(crate) mod a18;
pub(crate) mod a19;
pub(crate) mod a20;
pub(crate) mod a21;
pub(crate) mod a22;
pub(crate) mod a23;
pub(crate) mod a24;
pub(crate) mod a25;
pub(crate) mod a26;
pub(crate) mod a27;
pub(crate) mod a28;
pub(crate) mod a29;
pub(crate) mod a30;
pub(crate) mod a31;
pub(crate) mod a32;
pub(crate) mod a33;
pub(crate) mod a34;
pub(crate) mod a37;
pub(crate) mod a38;
pub(crate) mod a39;
pub(crate) mod a40;
pub(crate) mod a41;
pub(crate) mod a42;
pub(crate) mod a44;
pub(crate) mod a45;
pub(crate) mod a46;
pub(crate) mod a47;
pub(crate) mod a48;
pub(crate) mod a49;
pub(crate) mod a50;
pub(crate) mod a51;
pub(crate) mod a52;
pub(crate) mod a53;
pub(crate) mod a54;
pub(crate) mod a59;
pub(crate) mod a60;
pub(crate) mod a61;
pub(crate) mod a63;
pub(crate) mod a64;
pub(crate) mod a65;
pub(crate) mod a66;
pub(crate) mod a67;
pub(crate) mod a68;
pub(crate) mod a69;
pub(crate) mod a70;
pub(crate) mod a71;
pub(crate) mod a72;
pub(crate) mod a73;
pub(crate) mod a74;
pub(crate) mod a75;
pub(crate) mod a76;
pub(crate) mod a77;
pub(crate) mod a79;
pub(crate) mod a80;
pub(crate) mod a81;
pub(crate) mod a82;
pub(crate) mod a83;
pub(crate) mod a84;
pub(crate) mod a85;
pub(crate) mod a86;
pub(crate) mod a87;
pub(crate) mod a88;
pub(crate) mod a89;
pub(crate) mod a90;
pub(crate) mod a91;
pub(crate) mod a92;
pub(crate) mod a93;
pub(crate) mod a94;
pub(crate) mod a95;
pub(crate) mod a96;
pub(crate) mod a97;
pub(crate) mod a98;
pub(crate) mod a99;
pub(crate) mod adaptive;
pub(crate) mod composition;
pub(crate) mod gsettings;
pub(crate) mod lazy_sections;
pub(crate) mod post_open;
pub(crate) mod remote_input;
pub(crate) mod text;

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
pub use a21::probe_a21;
pub use a22::probe_a22;
pub use a23::probe_a23;
pub use a24::probe_a24;
pub use a25::probe_a25;
pub use a26::probe_a26;
pub use a27::probe_a27;
pub use a28::probe_a28;
pub use a29::probe_a29;
pub use a30::probe_a30;
pub use a31::probe_a31;
pub use a32::probe_a32;
pub use a33::probe_a33;
pub use a34::probe_a34;
pub use a37::probe_a37;
pub use a38::probe_a38;
pub use a39::probe_a39;
pub use a40::probe_a40;
pub use a41::probe_a41;
pub use a42::probe_a42;
pub use a44::probe_a44;
pub use a45::probe_a45;
pub use a46::probe_a46;
pub use a47::probe_a47;
pub use a48::probe_a48;
pub use a49::probe_a49;
pub use a50::probe_a50;
pub use a51::probe_a51;
pub use a52::probe_a52;
pub use a53::probe_a53;
pub use a54::probe_a54;
pub use a59::probe_a59;
pub use a60::probe_a60;
pub use a61::probe_a61;
pub use a63::probe_a63;
pub use a64::probe_a64;
pub use a65::probe_a65;
pub use a66::probe_a66;
pub use a67::probe_a67;
pub use a68::probe_a68;
pub use a69::probe_a69;
pub use a70::probe_a70;
pub use a71::probe_a71;
pub use a72::probe_a72;
pub use a73::probe_a73;
pub use a74::probe_a74;
pub use a75::probe_a75;
pub use a76::probe_a76;
pub use a77::probe_a77;
pub use a79::probe_a79;
pub use a80::probe_a80;
pub use a81::probe_a81;
pub use a82::probe_a82;
pub use a83::probe_a83;
pub use a84::probe_a84;
pub use a85::probe_a85;
pub use a86::probe_a86;
pub use a87::probe_a87;
pub use a88::probe_a88;
pub use a89::probe_a89;
pub use a90::probe_a90;
pub use a91::probe_a91;
pub use a92::probe_a92;
pub use a93::probe_a93;
pub use a94::probe_a94;
pub use a95::probe_a95;
pub use a96::probe_a96;
pub use a97::probe_a97;
pub use a98::probe_a98;
pub use a99::probe_a99;
pub use a100::probe_a100;
pub use a101::probe_a101;
pub use a102::probe_a102;
pub use a103::probe_a103;
pub use a104::probe_a104;
pub use a105::probe_a105;
pub use a106::probe_a106;

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
