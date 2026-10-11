// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for the responsiveness-budget arithmetic.
//!
//! Kani proves the percentile bounds and monotonicity for every slice of up
//! to four samples (`model/responsiveness_budget/kani_proofs.rs`); the lane's
//! reports read percentiles of hundreds of frame gaps, so this suite checks the
//! same two properties on longer generated slices, and the dispatch
//! classification against a `u128` reference.

use lushtext_core::model::responsiveness_budget::{
    HISTOGRAM_BUCKETS, StallVerdict, classify_dispatch, histogram_bucket, nearest_rank_percentile,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn percentile_is_bounded_and_monotone_in_the_rank(
        mut samples in prop::collection::vec(any::<u64>(), 1..256),
        low in 0u64..=100,
        high in 0u64..=100,
    ) {
        samples.sort_unstable();
        let (low, high) = if low <= high { (low, high) } else { (high, low) };
        let at_low = nearest_rank_percentile(&samples, low).expect("sorted, non-empty");
        let at_high = nearest_rank_percentile(&samples, high).expect("sorted, non-empty");
        let minimum = samples[0];
        let maximum = samples[samples.len() - 1];
        prop_assert!(minimum <= at_low && at_low <= maximum);
        prop_assert!(minimum <= at_high && at_high <= maximum);
        prop_assert!(at_low <= at_high);
        prop_assert_eq!(nearest_rank_percentile(&samples, 0), Some(minimum));
        prop_assert_eq!(nearest_rank_percentile(&samples, 100), Some(maximum));
    }

    #[test]
    fn classification_matches_the_exact_bounds(
        lateness in any::<u64>(),
        tick in any::<u64>(),
        slack in any::<u64>(),
        budget in any::<u64>(),
    ) {
        let upper = u128::from(lateness) + u128::from(tick);
        let lower = i128::from(lateness) - i128::from(slack);
        let expected = if upper <= u128::from(budget) {
            StallVerdict::Within
        } else if lower > i128::from(budget) {
            StallVerdict::Over
        } else {
            StallVerdict::Indeterminate
        };
        prop_assert_eq!(classify_dispatch(lateness, tick, slack, budget), expected);
    }

    #[test]
    fn buckets_are_monotone_and_in_range(first in any::<u64>(), second in any::<u64>()) {
        let (small, large) = if first <= second { (first, second) } else { (second, first) };
        prop_assert!(histogram_bucket(large) < HISTOGRAM_BUCKETS);
        prop_assert!(histogram_bucket(small) <= histogram_bucket(large));
    }
}
