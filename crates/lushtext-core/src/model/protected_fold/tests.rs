// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::model::paragraph_boundary::next_paragraph_break;

/// Closed paragraph lengths of `text`, the way C1's accumulator closes them
/// (delimiters excluded, the last paragraph included).
fn paragraph_lengths(text: &str) -> Vec<u64> {
    let mut lengths = Vec::new();
    let mut from = 0;
    while let Some(found) = next_paragraph_break(text, from) {
        lengths.push(text[from..found.content_end].chars().count() as u64);
        from = found.next_start;
    }
    lengths.push(text[from..].chars().count() as u64);
    lengths
}

/// The visible characters of every paragraph under the production plan.
fn visible(lengths: &[u64], thresholds: FoldThresholds) -> Vec<u64> {
    let mut visible = lengths.to_vec();
    for range in plan_folds(lengths, thresholds) {
        let paragraph = usize::try_from(range.paragraph).expect("a test paragraph index fits");
        assert_eq!(range.head_chars + range.tail_chars, lengths[paragraph]);
        visible[paragraph] = range.head_chars;
    }
    visible
}

/// The largest visible sum of any window (the whole sequence when shorter).
fn worst_window(visible: &[u64], r_fold: usize) -> u64 {
    if visible.len() < r_fold {
        return visible.iter().sum();
    }
    visible
        .windows(r_fold)
        .map(|window| window.iter().sum())
        .max()
        .unwrap_or(0)
}

/// The superseded "fold when the window would overflow" rule (first draft,
/// before the excess budget): each paragraph shows `min(len, n_visible)`,
/// folded to `n_dense` when adding it to the running window would exceed
/// `v_batch`. Kept to pin the hand-found counterexample.
fn superseded_overflow_rule(
    lengths: &[u64],
    n_visible: u64,
    n_dense: u64,
    v_batch: u64,
    r_batch: usize,
) -> Vec<u64> {
    let mut shown: Vec<u64> = Vec::new();
    for &length in lengths {
        let start = shown.len().saturating_sub(r_batch - 1);
        let window: u64 = shown[start..].iter().sum();
        let capped = length.min(n_visible);
        shown.push(if window + capped > v_batch {
            capped.min(n_dense)
        } else {
            capped
        });
    }
    shown
}

/// The superseded excess-budget rule (first draft): every paragraph capped at
/// `n_dense + (v_batch − r_batch · n_dense)`, whatever its neighbours hold.
fn superseded_excess_budget_rule(
    lengths: &[u64],
    n_dense: u64,
    v_batch: u64,
    r_batch: u64,
) -> Vec<u64> {
    let excess = v_batch - r_batch * n_dense;
    lengths
        .iter()
        .map(|&length| length.min(n_dense + excess))
        .collect()
}

fn small(head_cap: u64, n_dense: u64, v_batch: u64, r_fold: usize) -> FoldThresholds {
    FoldThresholds::new(head_cap, n_dense, v_batch, r_fold).expect("valid test thresholds")
}

#[test]
fn production_constants_take_the_measured_values() {
    assert_eq!(N_DENSE, 250);
    assert_eq!(R_FOLD, 224);
    assert_eq!(A28_SAFE_CHARS, 59_579);
    assert_eq!(HEAD_CAP, N_HEAD);
    let production = FoldThresholds::PRODUCTION;
    assert_eq!(
        (
            production.head_cap(),
            production.n_dense(),
            production.v_batch(),
            production.r_fold()
        ),
        (10_000, 250, 56_000, 224)
    );
    assert_eq!(FoldRange::RETAINED_BYTES, 24);
}

#[test]
fn thresholds_reject_broken_preconditions() {
    assert!(FoldThresholds::new(10, 0, 10, 1).is_none(), "n_dense = 0");
    assert!(
        FoldThresholds::new(4, 5, 100, 1).is_none(),
        "n_dense > head_cap"
    );
    assert!(FoldThresholds::new(10, 5, 100, 0).is_none(), "r_fold = 0");
    assert!(
        FoldThresholds::new(10, 5, 10_000, MAX_R_FOLD + 1).is_none(),
        "r_fold over the ring"
    );
    assert!(
        FoldThresholds::new(10, 5, 9, 2).is_none(),
        "r_fold · n_dense > v_batch"
    );
    assert!(
        FoldThresholds::new(u64::MAX, u64::MAX, u64::MAX, 2).is_none(),
        "the product overflows"
    );
    assert!(
        FoldThresholds::new(u64::MAX / 2 + 1, 1, 10, 2).is_none(),
        "r_fold · head_cap overflows"
    );
    assert!(FoldThresholds::new(u64::MAX / 2, 1, 10, 2).is_some());
    assert!(
        FoldThresholds::new(5, 5, 10, 2).is_some(),
        "every bound at equality"
    );
    assert!(FoldThresholds::new(10, 5, 10, MAX_R_FOLD).is_none());
    assert!(FoldThresholds::new(10, 1, 256, MAX_R_FOLD).is_some());
}

#[test]
fn a_state_refuses_a_window_larger_than_its_ring() {
    assert!(FoldState::<2>::new(small(10, 1, 10, 3)).is_none());
    assert!(FoldState::<3>::new(small(10, 1, 10, 3)).is_some());
}

#[test]
fn head_cap_edges() {
    let production = FoldThresholds::PRODUCTION;
    // At the cap, alone: shown whole.
    assert!(plan_folds(&[HEAD_CAP], production).is_empty());
    // One over: the one extra character is hidden.
    assert_eq!(
        plan_folds(&[HEAD_CAP + 1], production),
        vec![FoldRange {
            paragraph: 0,
            head_chars: HEAD_CAP,
            tail_chars: 1
        }]
    );
}

#[test]
fn n_dense_and_window_edges() {
    let thresholds = small(8, 5, 10, 2);
    // A window at exactly v_batch is within budget: nothing folds.
    assert!(plan_folds(&[5, 5, 5], thresholds).is_empty());
    assert!(plan_folds(&[8, 2], thresholds).is_empty());
    // One over: both members dense; one at n_dense stays whole.
    assert_eq!(visible(&[5, 6], thresholds), vec![5, 5]);
    assert_eq!(
        plan_folds(&[5, 6], thresholds),
        vec![FoldRange {
            paragraph: 1,
            head_chars: 5,
            tail_chars: 1
        }]
    );
    // Only paragraphs in an over-budget window fold.
    assert_eq!(
        visible(&[8, 1, 1, 8, 8, 1], thresholds),
        vec![8, 1, 1, 5, 5, 1]
    );
}

#[test]
fn a_document_shorter_than_one_window_is_one_window() {
    let thresholds = small(100, 10, 30, 3);
    assert!(plan_folds(&[15, 15], thresholds).is_empty());
    assert_eq!(visible(&[20, 20], thresholds), vec![10, 10]);
    assert!(plan_folds(&[], thresholds).is_empty());
    assert!(plan_folds(&[0], thresholds).is_empty());
}

#[test]
fn delimiters_follow_the_paragraph_boundary() {
    let production = FoldThresholds::PRODUCTION;
    let giant = "x".repeat(20_000);
    for delimiter in ["\n", "\r", "\r\n", "\u{2029}"] {
        let text = format!("short{delimiter}{giant}{delimiter}tail");
        let lengths = paragraph_lengths(&text);
        assert_eq!(lengths, vec![5, 20_000, 4], "delimiter {delimiter:?}");
        let plan = plan_folds(&lengths, production);
        assert_eq!(
            plan,
            vec![FoldRange {
                paragraph: 1,
                head_chars: HEAD_CAP,
                tail_chars: 20_000 - HEAD_CAP
            }]
        );
        // The split falls inside the paragraph, before its delimiter.
        let paragraph_start = 5 + delimiter.len();
        let paragraph = &text[paragraph_start..paragraph_start + giant.len()];
        assert_eq!(
            fold_byte_offset(paragraph, plan[0].head_chars),
            usize::try_from(HEAD_CAP).expect("HEAD_CAP fits a usize")
        );
    }
}

#[test]
fn moderately_long_prose_folds_nothing() {
    // Protected only because some paragraphs are 600 characters long, with
    // every window of R_FOLD paragraphs within V_BATCH.
    let lengths: Vec<u64> = (0..1_000)
        .map(|index| if index % 3 == 0 { 600 } else { 40 })
        .collect();
    assert!(worst_window(&lengths, R_FOLD) <= V_BATCH);
    assert!(plan_folds(&lengths, FoldThresholds::PRODUCTION).is_empty());
}

#[test]
fn a_lone_giant_paragraph_shows_head_cap() {
    let mut lengths = vec![80; 500];
    lengths[250] = 1_900_000;
    let shown = visible(&lengths, FoldThresholds::PRODUCTION);
    assert_eq!(shown[250], HEAD_CAP);
    assert_eq!(shown.iter().filter(|&&chars| chars != 80).count(), 1);
}

#[test]
fn dense_windows_fold_to_n_dense() {
    let mut lengths = vec![10; 300];
    lengths.extend(std::iter::repeat_n(700, 2 * R_FOLD));
    lengths.extend(std::iter::repeat_n(10, 300));
    let shown = visible(&lengths, FoldThresholds::PRODUCTION);
    assert!(
        shown[300..300 + 2 * R_FOLD]
            .iter()
            .all(|&chars| chars == N_DENSE)
    );
    assert!(shown[..300].iter().all(|&chars| chars == 10));
    assert!(shown[300 + 2 * R_FOLD..].iter().all(|&chars| chars == 10));
    assert!(worst_window(&shown, R_FOLD) <= V_BATCH);
}

#[test]
fn the_overflow_rule_counterexample_is_fixed() {
    // V_batch = 10, R_batch = 2, N_dense = 5, N_visible = 8, two 8-character
    // paragraphs: the superseded rule shows 8 + 5 = 13 > 10.
    let lengths = [8, 8];
    let superseded = superseded_overflow_rule(&lengths, 8, 5, 10, 2);
    assert_eq!(superseded, vec![8, 5]);
    assert!(
        worst_window(&superseded, 2) > 10,
        "the superseded rule overflows"
    );
    let shown = visible(&lengths, small(8, 5, 10, 2));
    assert_eq!(shown, vec![5, 5]);
    assert!(worst_window(&shown, 2) <= 10);
}

#[test]
fn the_excess_budget_defect_is_fixed() {
    // With R · N_dense = V (E = 0), a paragraph of N_dense + 1 in an
    // under-budget window must stay whole; the superseded rule folded it.
    let mut lengths = vec![10; 400];
    lengths[200] = N_DENSE + 1;
    let r_batch = R_BATCH as u64;
    assert_eq!(V_BATCH - r_batch * N_DENSE, 0);
    let superseded = superseded_excess_budget_rule(&lengths, N_DENSE, V_BATCH, r_batch);
    assert_eq!(superseded[200], N_DENSE, "the superseded rule folds it");
    assert!(plan_folds(&lengths, FoldThresholds::PRODUCTION).is_empty());
}

#[test]
fn streaming_emits_with_a_lookahead_of_r_fold_minus_one() {
    let thresholds = small(8, 2, 6, 3);
    let mut state = FoldState::<4>::new(thresholds).expect("r_fold fits the ring");
    assert_eq!(fold_step(&mut state, 8), None, "warm-up");
    assert_eq!(fold_step(&mut state, 0), None, "warm-up");
    // The first full window [8, 0, 8] is over budget: paragraph 0 is decided.
    assert_eq!(
        fold_step(&mut state, 8),
        Some(FoldRange {
            paragraph: 0,
            head_chars: 2,
            tail_chars: 6
        })
    );
    assert_eq!(state.paragraphs(), 3);
    assert_eq!(state.thresholds(), thresholds);
    let rest: Vec<FoldRange> = fold_finish(state).collect();
    assert_eq!(
        rest,
        vec![FoldRange {
            paragraph: 2,
            head_chars: 2,
            tail_chars: 6
        }]
    );
}

#[test]
fn locate_maps_visible_hidden_and_past_end() {
    let range = FoldRange {
        paragraph: 3,
        head_chars: 4,
        tail_chars: 6,
    };
    assert_eq!(range.locate(0), FoldLocation::Visible(0));
    assert_eq!(range.locate(3), FoldLocation::Visible(3));
    assert_eq!(range.locate(4), FoldLocation::Hidden(0));
    assert_eq!(range.locate(10), FoldLocation::Hidden(6));
    assert_eq!(range.locate(11), FoldLocation::PastEnd);
    assert_eq!(range.locate(u64::MAX), FoldLocation::PastEnd);
    for column in 0..=10 {
        assert_eq!(range.locate(column).column_of(range), Some(column));
    }
    assert_eq!(FoldLocation::PastEnd.column_of(range), None);
    let overflowing = FoldRange {
        paragraph: 0,
        head_chars: u64::MAX,
        tail_chars: 0,
    };
    assert_eq!(FoldLocation::Hidden(1).column_of(overflowing), None);
}

#[test]
fn fold_byte_offset_counts_scalars() {
    let text = "aé€𝄞z";
    assert_eq!(fold_byte_offset(text, 0), 0);
    assert_eq!(fold_byte_offset(text, 1), 1);
    assert_eq!(fold_byte_offset(text, 2), 3);
    assert_eq!(fold_byte_offset(text, 3), 6);
    assert_eq!(fold_byte_offset(text, 4), 10);
    assert_eq!(fold_byte_offset(text, 5), text.len());
    assert_eq!(fold_byte_offset(text, 99), text.len());
    assert_eq!(fold_byte_offset("", 3), 0);
}

#[test]
fn the_stage_functions_are_exact() {
    let thresholds = small(8, 5, 10, 2);
    assert_eq!(length_cap(7, thresholds), 7);
    assert_eq!(length_cap(9, thresholds), 8);
    assert_eq!(dense_visible(4, 5), 4);
    assert_eq!(dense_visible(6, 5), 5);
    assert_eq!(density_cap(8, true, thresholds), 5);
    assert_eq!(density_cap(8, false, thresholds), 8);
    assert!(!window_is_over_budget(10, 10));
    assert!(window_is_over_budget(11, 10));
    assert!(!window_is_over_budget(u64::MAX, u64::MAX));
}

#[test]
fn running_sum_equals_direct_sum() {
    // The Kani form of `window_capped_sum` adds the ring up directly; the
    // production form keeps a running sum. They must agree after every step.
    let thresholds = small(300, 7, 2_000, 5);
    let mut state = FoldState::<8>::new(thresholds).expect("r_fold fits the ring");
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for _ in 0..10_000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let length = match seed % 4 {
            0 => seed % 5,
            1 => seed % 400,
            2 => u64::MAX - seed % 3,
            _ => seed % 40,
        };
        let _ = fold_step(&mut state, length);
        assert_eq!(state.capped_sum, state.direct_capped_sum());
    }
}
