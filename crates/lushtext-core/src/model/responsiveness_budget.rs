// SPDX-License-Identifier: GPL-3.0-or-later

//! The responsiveness budgets, once, as integer microseconds, with the
//! arithmetic that reads a measurement against them.
//!
//! LushText must stay responsive with documents of any admitted size. The
//! long-line programme (`docs/next/long-line-documents.md`) states that as
//! budgets on **main-loop stalls**: the longest run of dispatches the GTK main
//! loop performs without returning to poll. A stall is what delays input, and
//! it is never shorter than any single dispatch inside it, so reading the
//! budgets against stalls is the conservative reading (maintainer decision,
//! 2026-10-11).
//!
//! The values were set by the maintainer on 2026-10-11 as working values and
//! are confirmed or revised by the reference run of `make responsiveness-smoke`
//! that `probe-long-line-toolkit-costs` records. A change to any of them cites
//! the measurement run that justifies it. Later changes (`document-scale-policy`,
//! `long-line-protected-view`) read these constants and never restate the
//! numbers.
//!
//! Every frame-relative limit uses a **fixed 120 Hz reference frame**, not the
//! refresh interval of whatever monitor a run happens to have, so a slower
//! virtual monitor can only make a run fail more often, never pass a slower
//! app.
//!
//! The module is GTK-free and float-free. The watchdog in
//! `ui/responsiveness_probe.rs` measures lateness `L` against a tick `T` with
//! an idle wake-up slack `ε`; [`classify_dispatch`] turns that into a verdict
//! that is only ever `Within` or `Over` when the bounds `[L − ε, L + T]` say
//! so. Kani proves the arithmetic (`responsiveness_budget/kani_proofs.rs`).

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

#[cfg(kani)]
mod kani_proofs;

/// The interaction-dispatch budget B_interact: the longest main-loop stall
/// allowed while the user scrolls, moves the caret, types, clicks, finds, or
/// switches tabs.
///
/// 8 ms, the maintainer's working value of 2026-10-11 (stricter than the
/// origin proposal's 16 ms), so an interaction leaves room for the frame it
/// must produce inside one 120 Hz reference frame.
pub const B_INTERACT_US: u64 = 8_000;

/// The settle-dispatch budget B_settle: the longest main-loop stall allowed
/// for work that follows an interaction or an open without the user waiting
/// on it directly (validation, background highlighting, projection refresh).
///
/// 16 ms, the maintainer's working value of 2026-10-11 (stricter than the
/// origin proposal's 50 ms): two reference frames.
pub const B_SETTLE_US: u64 = 16_000;

/// The fixed reference frame every frame-relative limit uses: 120 Hz,
/// rounded down to whole microseconds (maintainer decision, 2026-10-11).
pub const REFERENCE_FRAME_US: u64 = 8_333;

/// The input-to-presented-frame limit, in reference frames: the first frame
/// presented after an input event arrives at most this many reference frames
/// after the event (maintainer decision, 2026-10-11).
pub const INPUT_TO_FRAME_FRAMES: u64 = 2;

/// [`INPUT_TO_FRAME_FRAMES`] reference frames in microseconds (16,666 µs).
pub const INPUT_TO_FRAME_US: u64 = REFERENCE_FRAME_US * INPUT_TO_FRAME_FRAMES;

/// The scroll p95 frame-gap limit: while scrolling, 95 % of the gaps between
/// consecutive presented frames are at most one reference frame (maintainer
/// decision, 2026-10-11).
pub const SCROLL_P95_FRAME_GAP_US: u64 = REFERENCE_FRAME_US;

/// The percentile the scroll frame-gap limit applies to.
pub const SCROLL_FRAME_GAP_PERCENTILE: u64 = 95;

/// Number of buckets in a latency histogram (see [`histogram_bucket`]).
///
/// Bucket 0 holds 0 µs; bucket `i` for `1 ≤ i < 23` holds
/// `[2^(i−1), 2^i)` µs; bucket 23 holds everything from `2^22` µs (≈ 4.2 s)
/// up, so a hang still lands in a bucket.
pub const HISTOGRAM_BUCKETS: usize = 24;

/// Which budget a measured phase is read against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DispatchKind {
    /// The user is waiting on this dispatch: read against [`B_INTERACT_US`].
    Interaction,
    /// Follow-up work after an interaction or an open: read against
    /// [`B_SETTLE_US`].
    Settle,
}

impl DispatchKind {
    /// The budget this kind of dispatch is read against, in microseconds.
    #[must_use]
    pub const fn budget_us(self) -> u64 {
        match self {
            Self::Interaction => B_INTERACT_US,
            Self::Settle => B_SETTLE_US,
        }
    }

    /// The stable lower-case name reports use.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Interaction => "interaction",
            Self::Settle => "settle",
        }
    }
}

/// How one measured stall compares with a budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StallVerdict {
    /// Even the watchdog's upper bound on the stall fits the budget.
    Within,
    /// Even the watchdog's lower bound on the stall exceeds the budget.
    Over,
    /// The bounds straddle the budget: the measurement cannot decide.
    Indeterminate,
}

impl StallVerdict {
    /// The stable lower-case name reports use.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Within => "within",
            Self::Over => "over",
            Self::Indeterminate => "indeterminate",
        }
    }
}

/// Read a watchdog lateness against a budget.
///
/// The watchdog fires every `tick_us` and records `lateness_us`, how long
/// after its ready time it was dispatched. GLib cannot preempt a dispatch, so
/// the longest stall `S` that delayed it satisfies `S − T ≤ L ≤ S + ε`, where
/// `ε` (`slack_us`) is the idle wake-up slack the lane's calibration measured.
/// The stall therefore lies in `[L − ε, L + T]`, and the verdict is:
///
/// - [`StallVerdict::Within`] exactly when `L + T ≤ B` (the upper bound fits);
/// - [`StallVerdict::Over`] exactly when `L − ε > B` (the lower bound exceeds);
/// - [`StallVerdict::Indeterminate`] otherwise.
///
/// The comparisons are exact over the mathematical integers for every `u64`
/// input: `L + T` that does not fit a `u64` is not `Within`, and `L − ε` below
/// zero is not `Over`. Kani proves this for every `u64` quadruple
/// (`kani_proofs::classification_is_sound_and_complete`).
#[must_use]
pub const fn classify_dispatch(
    lateness_us: u64,
    tick_us: u64,
    slack_us: u64,
    budget_us: u64,
) -> StallVerdict {
    // `checked_add`: an upper bound that does not fit a `u64` exceeds every
    // budget, so it must never read as `Within`.
    if let Some(upper) = lateness_us.checked_add(tick_us)
        && upper <= budget_us
    {
        return StallVerdict::Within;
    }
    // `checked_sub`: a lower bound below zero exceeds no budget.
    if let Some(lower) = lateness_us.checked_sub(slack_us)
        && lower > budget_us
    {
        return StallVerdict::Over;
    }
    StallVerdict::Indeterminate
}

/// The histogram bucket a latency of `value_us` falls in, always below
/// [`HISTOGRAM_BUCKETS`].
///
/// The index is the bit length of `value_us` (0 for 0, 1 for 1, 2 for 2–3,
/// …), capped at the last bucket. It is monotone in `value_us`; Kani proves
/// both properties for every `u64` (`kani_proofs::buckets_are_monotone_and_in_range`).
#[must_use]
pub const fn histogram_bucket(value_us: u64) -> usize {
    let bit_length = (u64::BITS - value_us.leading_zeros()) as usize;
    if bit_length < HISTOGRAM_BUCKETS {
        bit_length
    } else {
        HISTOGRAM_BUCKETS - 1
    }
}

/// The smallest latency, in microseconds, that lands in `bucket`: 0 for
/// bucket 0, `2^(bucket−1)` otherwise. `None` past the last bucket.
#[must_use]
pub const fn histogram_bucket_floor_us(bucket: usize) -> Option<u64> {
    if bucket >= HISTOGRAM_BUCKETS {
        None
    } else if bucket == 0 {
        Some(0)
    } else {
        Some(1u64 << (bucket - 1))
    }
}

/// The nearest-rank `percent` percentile of `sorted`, which must be in
/// ascending order.
///
/// The rank is `⌈percent × n / 100⌉`, clamped to `[1, n]`, so percentile 0 is
/// the minimum and percentile 100 the maximum. Returns `None` for an empty
/// slice, a slice that is not ascending, or `percent > 100`.
///
/// Kani proves, for every slice of up to four `u64` samples and every `u64`
/// percent, that a returned value lies between the minimum and the maximum and
/// is monotone in `percent` (`kani_proofs::percentile_is_bounded_and_monotone`);
/// the property suite checks the same on longer generated slices.
#[must_use]
pub fn nearest_rank_percentile(sorted: &[u64], percent: u64) -> Option<u64> {
    if sorted.is_empty() || percent > 100 || !sorted.is_sorted() {
        return None;
    }
    let len = sorted.len();
    // `percent ≤ 100`, so the product fits a `u128` for any slice length.
    let numerator = u128::from(percent) * len as u128;
    let rank = numerator.div_ceil(100).clamp(1, len as u128);
    let index = usize::try_from(rank - 1).ok()?;
    sorted.get(index).copied()
}

/// How many whole reference frames (rounded up) a latency of `latency_us`
/// spans: the input-to-frame reading in frames.
#[must_use]
pub const fn reference_frames(latency_us: u64) -> u64 {
    latency_us.div_ceil(REFERENCE_FRAME_US)
}

/// Whether an input-to-presented-frame latency fits the
/// [`INPUT_TO_FRAME_US`] limit.
#[must_use]
pub const fn input_to_frame_within(latency_us: u64) -> bool {
    latency_us <= INPUT_TO_FRAME_US
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: u64 = 2_000;
    const EPS: u64 = 1_200;
    const B: u64 = B_INTERACT_US;

    #[test]
    fn upper_bound_on_the_budget_is_within() {
        assert_eq!(classify_dispatch(B - T, T, EPS, B), StallVerdict::Within);
    }

    #[test]
    fn upper_bound_one_over_the_budget_is_not_within() {
        assert_eq!(
            classify_dispatch(B - T + 1, T, EPS, B),
            StallVerdict::Indeterminate
        );
    }

    #[test]
    fn lower_bound_on_the_budget_is_not_over() {
        assert_eq!(
            classify_dispatch(B + EPS, T, EPS, B),
            StallVerdict::Indeterminate
        );
    }

    #[test]
    fn lower_bound_one_over_the_budget_is_over() {
        assert_eq!(
            classify_dispatch(B + EPS + 1, T, EPS, B),
            StallVerdict::Over
        );
    }

    #[test]
    fn lateness_below_the_slack_is_never_over() {
        assert_eq!(
            classify_dispatch(EPS - 1, T, EPS, 0),
            StallVerdict::Indeterminate
        );
        assert_eq!(classify_dispatch(0, 0, EPS, 0), StallVerdict::Within);
    }

    #[test]
    fn saturation_at_u64_max_is_never_within_and_still_over() {
        assert_eq!(
            classify_dispatch(u64::MAX, 1, 0, u64::MAX),
            StallVerdict::Indeterminate
        );
        assert_eq!(
            classify_dispatch(u64::MAX, u64::MAX, u64::MAX, u64::MAX),
            StallVerdict::Indeterminate
        );
        assert_eq!(
            classify_dispatch(u64::MAX, u64::MAX, 0, B),
            StallVerdict::Over
        );
        assert_eq!(
            classify_dispatch(u64::MAX, 0, 0, u64::MAX),
            StallVerdict::Within
        );
    }

    #[test]
    fn budgets_follow_the_2026_10_11_decisions() {
        assert_eq!(DispatchKind::Interaction.budget_us(), 8_000);
        assert_eq!(DispatchKind::Settle.budget_us(), 16_000);
        assert_eq!(INPUT_TO_FRAME_US, 16_666);
        assert_eq!(SCROLL_P95_FRAME_GAP_US, 8_333);
    }

    #[test]
    fn buckets_are_bit_lengths_capped_at_the_last() {
        assert_eq!(histogram_bucket(0), 0);
        assert_eq!(histogram_bucket(1), 1);
        assert_eq!(histogram_bucket(2), 2);
        assert_eq!(histogram_bucket(3), 2);
        assert_eq!(histogram_bucket(8_000), 13);
        assert_eq!(histogram_bucket((1 << 22) - 1), 22);
        assert_eq!(histogram_bucket(1 << 22), 23);
        assert_eq!(histogram_bucket(u64::MAX), HISTOGRAM_BUCKETS - 1);
        for bucket in 1..HISTOGRAM_BUCKETS {
            let floor = histogram_bucket_floor_us(bucket).expect("in range");
            assert_eq!(histogram_bucket(floor), bucket);
            assert_eq!(histogram_bucket(floor - 1), bucket - 1);
        }
        assert_eq!(histogram_bucket_floor_us(HISTOGRAM_BUCKETS), None);
    }

    #[test]
    fn nearest_rank_matches_the_textbook_examples() {
        let samples = [15, 20, 35, 40, 50];
        assert_eq!(nearest_rank_percentile(&samples, 0), Some(15));
        assert_eq!(nearest_rank_percentile(&samples, 5), Some(15));
        assert_eq!(nearest_rank_percentile(&samples, 30), Some(20));
        assert_eq!(nearest_rank_percentile(&samples, 40), Some(20));
        assert_eq!(nearest_rank_percentile(&samples, 50), Some(35));
        assert_eq!(nearest_rank_percentile(&samples, 95), Some(50));
        assert_eq!(nearest_rank_percentile(&samples, 100), Some(50));
    }

    #[test]
    fn nearest_rank_rejects_what_it_cannot_read() {
        assert_eq!(nearest_rank_percentile(&[], 50), None);
        assert_eq!(nearest_rank_percentile(&[2, 1], 50), None);
        assert_eq!(nearest_rank_percentile(&[1, 2], 101), None);
        assert_eq!(nearest_rank_percentile(&[7], u64::MAX), None);
    }

    #[test]
    fn frames_round_up_against_the_reference_frame() {
        assert_eq!(reference_frames(0), 0);
        assert_eq!(reference_frames(1), 1);
        assert_eq!(reference_frames(REFERENCE_FRAME_US), 1);
        assert_eq!(reference_frames(REFERENCE_FRAME_US + 1), 2);
        assert!(input_to_frame_within(INPUT_TO_FRAME_US));
        assert!(!input_to_frame_within(INPUT_TO_FRAME_US + 1));
    }
}
