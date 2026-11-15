// SPDX-License-Identifier: GPL-3.0-or-later

//! V1 (calibration): the side-by-side preview-width clamp.
//!
//! Copied from `crates/lushtext-core/src/ui/markdown_preview/policy.rs` at
//! `85758eaf8fb8190439719dd6926a5b2b3ed21fb0` (`origin/main`, 2026-11-15).
//! The function keeps the production body; its specification is in the
//! `#[verus_spec]` attribute, so removing the attribute (and nothing else)
//! gives the production text back, which `verus-drift` checks.
//!
//! The postconditions are the five properties
//! `policy/kani_proofs.rs` proves (shard `core-geometry-policies`), one per
//! clause, plus the exact result (`clamp_spec`), which the monotonicity lemma
//! needs because it compares two calls. The Kani `should_panic` harness
//! `preview_width_is_not_always_a_third` is reproduced by a proved witness
//! (`the_one_third_form_is_refuted`), never by a failed proof.

use vstd::prelude::*;

verus! {

pub const PREVIEW_MIN_WIDTH_SP: i32 = 1;
pub const PREVIEW_MAX_WIDTH_DIVISOR: i32 = 3;

/// The available width as the function counts it (`max(available, 1)`).
pub open spec fn counted(available: i32) -> int {
    if available >= 1 { available as int } else { 1 }
}

/// The exact result, as integer arithmetic.
pub open spec fn clamp_spec(preferred: i32, available: i32) -> int {
    let bound = if counted(available) / 3 >= 1 { counted(available) / 3 } else { 1 };
    let floored = if preferred >= 1 { preferred as int } else { 1 };
    if floored <= bound { floored } else { bound }
}

} // verus!

#[verus_spec(width =>
    ensures
        // preview_width_respects_the_floor
        width >= PREVIEW_MIN_WIDTH_SP,
        // preview_width_is_at_most_a_third_above_three_sp
        available_width >= 3 ==> 3 * width <= counted(available_width),
        // preview_width_is_the_floor_below_three_sp
        available_width < 3 ==> width == PREVIEW_MIN_WIDTH_SP,
        // preview_width_keeps_an_in_band_preference
        preferred_width >= PREVIEW_MIN_WIDTH_SP && 3 * preferred_width <= counted(available_width)
            ==> width == preferred_width,
        // the exact result, for the two-call monotonicity lemma
        width == clamp_spec(preferred_width, available_width),
)]
pub fn clamped_preview_width(preferred_width: i32, available_width: i32) -> i32 {
    let max_width = (available_width.max(1) / PREVIEW_MAX_WIDTH_DIVISOR).max(PREVIEW_MIN_WIDTH_SP);
    preferred_width.max(PREVIEW_MIN_WIDTH_SP).min(max_width)
}

verus! {

/// preview_width_is_monotone_in_preference: the width never decreases as the
/// preferred width grows.
pub proof fn preview_width_is_monotone_in_preference(smaller: i32, larger: i32, available: i32)
    requires
        smaller <= larger,
    ensures
        clamp_spec(smaller, available) <= clamp_spec(larger, available),
{
}

/// The same property stated over two real calls of the function.
pub fn preview_width_is_monotone_over_calls(smaller: i32, larger: i32, available: i32)
    requires
        smaller <= larger,
{
    let a = clamped_preview_width(smaller, available);
    let b = clamped_preview_width(larger, available);
    proof { preview_width_is_monotone_in_preference(smaller, larger, available); }
    assert(a <= b);
}

/// preview_width_is_not_always_a_third (Kani: `should_panic`): refuted by a
/// proved witness. With 2 sp available and a 500 sp preference the width is
/// the 1 sp floor, and 3 * 1 > 2, so the unconditional one-third form is false.
pub fn the_one_third_form_is_refuted() {
    let width = clamped_preview_width(500, 2);
    assert(width == 1);
    assert(3 * width > counted(2));
}

} // verus!
