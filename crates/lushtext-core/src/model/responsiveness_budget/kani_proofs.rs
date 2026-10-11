// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the responsiveness-budget arithmetic:
//! [`classify_dispatch`], [`histogram_bucket`], and
//! [`nearest_rank_percentile`].
//!
//! Domain: every `u64` lateness, tick, slack, budget, latency, and percent;
//! sample slices of zero to [`SAMPLES`] arbitrary `u64` values. The
//! percentile's `is_sorted` check and its single indexed read are the only
//! loops, bounded by the slice length, so the percentile harness uses
//! `unwind(5)`. Longer slices are covered by the property suite
//! (`tests/properties/responsiveness_budget.rs`), not proved.
//!
//! Compiled only under `cfg(kani)` (`make kani`); ordinary builds and tests
//! never see this module. The harnesses check the functions that ship.

use super::{
    HISTOGRAM_BUCKETS, StallVerdict, classify_dispatch, histogram_bucket,
    histogram_bucket_floor_us, nearest_rank_percentile,
};

/// The largest modelled sample slice.
const SAMPLES: usize = 4;

/// The verdict is `Within` exactly when the upper bound `L + T` fits the
/// budget, `Over` exactly when the lower bound `L − ε` exceeds it, and
/// `Indeterminate` otherwise, computed over the mathematical integers, so no
/// saturation or wrap-around can turn an overflowing bound into a pass.
#[kani::proof]
fn classification_is_sound_and_complete() {
    let lateness: u64 = kani::any();
    let tick: u64 = kani::any();
    let slack: u64 = kani::any();
    let budget: u64 = kani::any();
    let verdict = classify_dispatch(lateness, tick, slack, budget);

    let upper = u128::from(lateness) + u128::from(tick);
    let lower = i128::from(lateness) - i128::from(slack);
    let within = upper <= u128::from(budget);
    let over = lower > i128::from(budget);

    assert!(!(within && over), "the bounds cannot both decide");
    assert_eq!(verdict == StallVerdict::Within, within);
    assert_eq!(verdict == StallVerdict::Over, over);
    assert_eq!(verdict == StallVerdict::Indeterminate, !within && !over);

    kani::cover!(verdict == StallVerdict::Within);
    kani::cover!(verdict == StallVerdict::Over);
    kani::cover!(verdict == StallVerdict::Indeterminate);
    kani::cover!(lateness.checked_add(tick).is_none());
    kani::cover!(slack > lateness);
}

/// A larger lateness never yields a more favourable verdict: if a stall reads
/// `Over`, every longer one does too, and if one reads `Within`, every shorter
/// one does too.
#[kani::proof]
fn classification_is_monotone_in_the_lateness() {
    let shorter: u64 = kani::any();
    let longer: u64 = kani::any();
    kani::assume(shorter <= longer);
    let tick: u64 = kani::any();
    let slack: u64 = kani::any();
    let budget: u64 = kani::any();
    let short_verdict = classify_dispatch(shorter, tick, slack, budget);
    let long_verdict = classify_dispatch(longer, tick, slack, budget);
    if short_verdict == StallVerdict::Over {
        assert_eq!(long_verdict, StallVerdict::Over);
    }
    if long_verdict == StallVerdict::Within {
        assert_eq!(short_verdict, StallVerdict::Within);
    }
}

/// Every latency lands in a bucket below [`HISTOGRAM_BUCKETS`], the bucket is
/// monotone in the latency, and every bucket but the last starts at its
/// documented floor.
#[kani::proof]
fn buckets_are_monotone_and_in_range() {
    let small: u64 = kani::any();
    let large: u64 = kani::any();
    kani::assume(small <= large);
    let small_bucket = histogram_bucket(small);
    let large_bucket = histogram_bucket(large);
    assert!(large_bucket < HISTOGRAM_BUCKETS);
    assert!(small_bucket <= large_bucket);

    let bucket = histogram_bucket(small);
    let floor = histogram_bucket_floor_us(bucket).expect("an in-range bucket has a floor");
    assert!(
        floor <= small,
        "a latency is never below its bucket's floor"
    );
    if bucket + 1 < HISTOGRAM_BUCKETS {
        let next = histogram_bucket_floor_us(bucket + 1).expect("in range");
        assert!(
            small < next,
            "a latency below the last bucket is below the next floor"
        );
    }
    kani::cover!(bucket == 0);
    kani::cover!(bucket == HISTOGRAM_BUCKETS - 1);
}

/// For every ascending slice of up to [`SAMPLES`] samples, a percentile lies
/// between the minimum and the maximum, is monotone in the percent, and is
/// `None` exactly for an empty slice or a percent above 100.
#[kani::proof]
#[kani::unwind(5)]
fn percentile_is_bounded_and_monotone() {
    let storage: [u64; SAMPLES] = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= SAMPLES);
    let samples = &storage[..len];
    kani::assume(samples.is_sorted());
    let low: u64 = kani::any();
    let high: u64 = kani::any();
    kani::assume(low <= high);

    let at_low = nearest_rank_percentile(samples, low);
    let at_high = nearest_rank_percentile(samples, high);
    assert_eq!(at_low.is_none(), len == 0 || low > 100);
    assert_eq!(at_high.is_none(), len == 0 || high > 100);
    if let (Some(at_low), Some(at_high)) = (at_low, at_high) {
        let minimum = samples[0];
        let maximum = samples[len - 1];
        assert!(minimum <= at_low && at_low <= maximum);
        assert!(minimum <= at_high && at_high <= maximum);
        assert!(at_low <= at_high);
        kani::cover!(at_low < at_high);
    }
    kani::cover!(len == SAMPLES && high == 100);
}

/// An unsorted slice is refused rather than read as if it were sorted.
#[kani::proof]
#[kani::unwind(5)]
fn percentile_refuses_an_unsorted_slice() {
    let storage: [u64; SAMPLES] = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= SAMPLES);
    let samples = &storage[..len];
    kani::assume(!samples.is_sorted());
    let percent: u64 = kani::any();
    assert_eq!(nearest_rank_percentile(samples, percent), None);
}
