// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the side-by-side preview-width clamp,
//! [`clamped_preview_width`].
//!
//! Domain: every `i32` preferred width and every `i32` available content width
//! (zero or less counts as 1 sp, as the function says). The clamp is
//! integer-only, so the one-third bound is the exact comparison
//! `3 * result <= available`, computed in `i64`.
//!
//! Compiled only under `cfg(kani)` (`make kani`); ordinary builds and tests
//! never see this module. The harnesses check the function that ships.

use super::{PREVIEW_MIN_WIDTH_SP, clamped_preview_width};

/// The available width as the function counts it.
fn counted(available: i32) -> i64 {
    i64::from(available.max(1))
}

/// The width is never below the 1 sp floor.
#[kani::proof]
fn preview_width_respects_the_floor() {
    assert!(clamped_preview_width(kani::any(), kani::any()) >= PREVIEW_MIN_WIDTH_SP);
}

/// With at least 3 sp available, the width is at most one third of it.
#[kani::proof]
fn preview_width_is_at_most_a_third_above_three_sp() {
    let available: i32 = kani::any();
    kani::assume(available >= 3);
    let width = clamped_preview_width(kani::any(), available);
    assert!(3 * i64::from(width) <= counted(available));
}

/// Below 3 sp available, the 1 sp floor wins whatever the preference.
#[kani::proof]
fn preview_width_is_the_floor_below_three_sp() {
    let available: i32 = kani::any();
    kani::assume(available < 3);
    let preferred: i32 = kani::any();
    let width = clamped_preview_width(preferred, available);
    kani::cover!(
        preferred > 1 && 3 * i64::from(width) > counted(available),
        "the floor wins over the one-third bound"
    );
    assert!(width == PREVIEW_MIN_WIDTH_SP);
}

/// A preferred width between the floor and one third of the available width is
/// kept exactly.
#[kani::proof]
fn preview_width_keeps_an_in_band_preference() {
    let preferred: i32 = kani::any();
    let available: i32 = kani::any();
    kani::assume(preferred >= PREVIEW_MIN_WIDTH_SP);
    kani::assume(3 * i64::from(preferred) <= counted(available));
    assert!(clamped_preview_width(preferred, available) == preferred);
}

/// The width never decreases as the preferred width grows.
#[kani::proof]
fn preview_width_is_monotone_in_preference() {
    let smaller: i32 = kani::any();
    let larger: i32 = kani::any();
    let available: i32 = kani::any();
    kani::assume(smaller <= larger);
    assert!(clamped_preview_width(smaller, available) <= clamped_preview_width(larger, available));
}

/// The unconditional "at most one third" form is false: below 3 sp the floor
/// wins. Kept so a future claim of the unconditional rule has to confront it.
#[kani::proof]
#[kani::should_panic]
fn preview_width_is_not_always_a_third() {
    let available: i32 = kani::any();
    let width = clamped_preview_width(kani::any(), available);
    assert!(3 * i64::from(width) <= counted(available));
}
