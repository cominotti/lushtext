// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated invariants for the Protected View fold policy at sizes and
//! thresholds beyond the Kani harnesses' domain (six paragraphs, `r_fold`
//! up to 3): long sequences, the production thresholds, and small generated
//! thresholds with windows up to 16 paragraphs.
//!
//! Each property compares the streamed plan with an independent reference
//! (every window enumerated, every member of an over-budget window dense), so
//! the window bound, "no gratuitous fold", the fold shape and slicing
//! independence all follow from equality plus the reference's own checks.

use lushtext_core::model::protected_fold::{
    FoldRange, FoldThresholds, ProductionFoldState, fold_finish, fold_step, plan_folds,
};
use proptest::prelude::*;

/// The specification: what each paragraph shows.
fn reference_visible(lengths: &[u64], thresholds: FoldThresholds) -> Vec<u64> {
    let capped: Vec<u64> = lengths
        .iter()
        .map(|&length| length.min(thresholds.head_cap()))
        .collect();
    let r_fold = thresholds.r_fold();
    let size = r_fold.min(capped.len());
    let mut dense = vec![false; capped.len()];
    if size > 0 {
        for start in 0..=capped.len() - size {
            let sum: u128 = capped[start..start + size]
                .iter()
                .map(|&c| u128::from(c))
                .sum();
            if sum > u128::from(thresholds.v_batch()) {
                dense[start..start + size]
                    .iter_mut()
                    .for_each(|member| *member = true);
            }
        }
    }
    capped
        .iter()
        .zip(dense)
        .map(|(&c, dense)| {
            if dense {
                c.min(thresholds.n_dense())
            } else {
                c
            }
        })
        .collect()
}

fn visible_of(lengths: &[u64], plan: &[FoldRange]) -> Vec<u64> {
    let mut visible = lengths.to_vec();
    for range in plan {
        visible[usize::try_from(range.paragraph).expect("a planned paragraph index fits")] =
            range.head_chars;
    }
    visible
}

fn assert_plan_is_the_specification(lengths: &[u64], thresholds: FoldThresholds) {
    let plan = plan_folds(lengths, thresholds);
    for pair in plan.windows(2) {
        assert!(
            pair[0].paragraph < pair[1].paragraph,
            "ordered and disjoint"
        );
    }
    for range in &plan {
        let length =
            lengths[usize::try_from(range.paragraph).expect("a planned paragraph index fits")];
        assert_eq!(
            range.head_chars + range.tail_chars,
            length,
            "only the paragraph"
        );
        assert!(range.tail_chars > 0, "only folded paragraphs are planned");
        assert!(range.head_chars >= length.min(thresholds.n_dense()).max(1));
    }
    let visible = visible_of(lengths, &plan);
    assert_eq!(visible, reference_visible(lengths, thresholds));
    let size = thresholds.r_fold().min(visible.len()).max(1);
    for window in visible.windows(size) {
        let sum: u128 = window.iter().map(|&v| u128::from(v)).sum();
        assert!(sum <= u128::from(thresholds.v_batch()), "window bound");
    }
    assert!(visible.iter().all(|&v| v <= thresholds.head_cap()));
}

fn small_thresholds() -> impl Strategy<Value = FoldThresholds> {
    (1usize..=16, 1u64..=64, 0u64..=200, 0u64..=400).prop_filter_map(
        "valid thresholds",
        |(r_fold, n_dense, head_extra, v_extra)| {
            let v_batch = r_fold as u64 * n_dense + v_extra;
            FoldThresholds::new(n_dense + head_extra, n_dense, v_batch, r_fold)
        },
    )
}

/// Lengths around the interesting edges of the small thresholds, plus giants.
fn small_lengths() -> impl Strategy<Value = Vec<u64>> {
    let length = prop_oneof![
        6 => 0u64..=80,
        2 => 80u64..=400,
        1 => Just(u64::MAX),
        1 => any::<u64>(),
    ];
    prop::collection::vec(length, 0..120)
}

/// Lengths around the production thresholds (N_DENSE 218, HEAD_CAP 8,192).
fn production_lengths() -> impl Strategy<Value = Vec<u64>> {
    let length = prop_oneof![
        8 => 0u64..=120,
        4 => 200u64..=800,
        1 => 9_000u64..=11_000,
        1 => 100_000u64..=2_000_000,
    ];
    prop::collection::vec(length, 0..1_200)
}

fn stream_split(lengths: &[u64], thresholds: FoldThresholds, cuts: &[usize]) -> Vec<FoldRange> {
    let mut cuts: Vec<usize> = cuts.iter().map(|&cut| cut % (lengths.len() + 1)).collect();
    cuts.sort_unstable();
    let mut plan = Vec::new();
    let mut state = ProductionFoldState::with_thresholds(thresholds);
    let mut from = 0;
    for cut in cuts.into_iter().chain([lengths.len()]) {
        for &length in &lengths[from..cut] {
            plan.extend(fold_step(&mut state, length));
        }
        // An install turn boundary: the state is carried to the next turn.
        state = state.clone();
        from = cut;
    }
    plan.extend(fold_finish(state));
    plan
}

proptest! {
    #[test]
    fn small_thresholds_plan_the_specification(
        thresholds in small_thresholds(),
        lengths in small_lengths(),
    ) {
        assert_plan_is_the_specification(&lengths, thresholds);
    }

    #[test]
    fn production_thresholds_plan_the_specification(lengths in production_lengths()) {
        assert_plan_is_the_specification(&lengths, FoldThresholds::PRODUCTION);
    }

    #[test]
    fn the_plan_is_independent_of_install_slicing(
        thresholds in small_thresholds(),
        lengths in small_lengths(),
        cuts in prop::collection::vec(any::<usize>(), 0..6),
    ) {
        prop_assert_eq!(
            stream_split(&lengths, thresholds, &cuts),
            plan_folds(&lengths, thresholds)
        );
    }

    #[test]
    fn production_slicing_independence(
        lengths in production_lengths(),
        cuts in prop::collection::vec(any::<usize>(), 0..6),
    ) {
        prop_assert_eq!(
            stream_split(&lengths, FoldThresholds::PRODUCTION, &cuts),
            plan_folds(&lengths, FoldThresholds::PRODUCTION)
        );
    }
}
