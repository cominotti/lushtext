// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::document::paragraph_boundary::next_paragraph_break;

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
    let char_lengths: Vec<CharCount> = lengths.iter().copied().map(CharCount::new).collect();
    let mut visible = lengths.to_vec();
    for range in plan_folds(&char_lengths, thresholds) {
        let paragraph = usize::try_from(range.paragraph).expect("a test paragraph index fits");
        assert_eq!(
            range.head_chars.as_chars() + range.tail_chars.as_chars(),
            lengths[paragraph]
        );
        visible[paragraph] = range.head_chars.as_chars();
    }
    visible
}

fn c(chars: u64) -> CharCount {
    CharCount::new(chars)
}

fn plan_raw(lengths: &[u64], thresholds: FoldThresholds) -> Vec<FoldRange> {
    let char_lengths: Vec<CharCount> = lengths.iter().copied().map(CharCount::new).collect();
    plan_folds(&char_lengths, thresholds)
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
    FoldThresholds::new(c(head_cap), c(n_dense), c(v_batch), r_fold).expect("valid test thresholds")
}

#[test]
fn production_constants_take_the_measured_values() {
    assert_eq!(N_DENSE, c(256));
    assert_eq!(R_FOLD, 256);
    assert_eq!(A28_SAFE_CHARS, c(59_579));
    assert_eq!(HEAD_CAP, N_HEAD);
    let production = FoldThresholds::PRODUCTION;
    assert_eq!(
        (
            production.head_cap(),
            production.n_dense(),
            production.v_batch(),
            production.r_fold()
        ),
        (c(10_000), c(256), c(64 * 1024), 256)
    );
    assert_eq!(FoldRange::RETAINED_BYTES, Bytes::from_bytes(24));
}

#[test]
fn thresholds_reject_broken_preconditions() {
    assert!(
        FoldThresholds::new(c(10), c(0), c(10), 1).is_none(),
        "n_dense = 0"
    );
    assert!(
        FoldThresholds::new(c(4), c(5), c(100), 1).is_none(),
        "n_dense > head_cap"
    );
    assert!(
        FoldThresholds::new(c(10), c(5), c(100), 0).is_none(),
        "r_fold = 0"
    );
    assert!(
        FoldThresholds::new(c(10), c(5), c(10_000), MAX_R_FOLD + 1).is_none(),
        "r_fold over the ring"
    );
    assert!(
        FoldThresholds::new(c(10), c(5), c(9), 2).is_none(),
        "r_fold · n_dense > v_batch"
    );
    assert!(
        FoldThresholds::new(c(u64::MAX), c(u64::MAX), c(u64::MAX), 2).is_none(),
        "the product overflows"
    );
    assert!(
        FoldThresholds::new(c(u64::MAX / 2 + 1), c(1), c(10), 2).is_none(),
        "r_fold · head_cap overflows"
    );
    assert!(FoldThresholds::new(c(u64::MAX / 2), c(1), c(10), 2).is_some());
    assert!(
        FoldThresholds::new(c(5), c(5), c(10), 2).is_some(),
        "every bound at equality"
    );
    assert!(FoldThresholds::new(c(10), c(5), c(10), MAX_R_FOLD).is_none());
    assert!(FoldThresholds::new(c(10), c(1), c(256), MAX_R_FOLD).is_some());
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
        plan_folds(&[HEAD_CAP.saturating_add(c(1))], production),
        vec![FoldRange {
            paragraph: 0,
            head_chars: HEAD_CAP,
            tail_chars: c(1),
        }]
    );
}

#[test]
fn n_dense_and_window_edges() {
    let thresholds = small(8, 5, 10, 2);
    // A window at exactly v_batch is within budget: nothing folds.
    assert!(plan_raw(&[5, 5, 5], thresholds).is_empty());
    assert!(plan_raw(&[8, 2], thresholds).is_empty());
    // One over: both members dense; one at n_dense stays whole.
    assert_eq!(visible(&[5, 6], thresholds), vec![5, 5]);
    assert_eq!(
        plan_raw(&[5, 6], thresholds),
        vec![FoldRange {
            paragraph: 1,
            head_chars: c(5),
            tail_chars: c(1),
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
    assert!(plan_raw(&[15, 15], thresholds).is_empty());
    assert_eq!(visible(&[20, 20], thresholds), vec![10, 10]);
    assert!(plan_raw(&[], thresholds).is_empty());
    assert!(plan_raw(&[0], thresholds).is_empty());
}

#[test]
fn delimiters_follow_the_paragraph_boundary() {
    let production = FoldThresholds::PRODUCTION;
    let giant = "x".repeat(20_000);
    for delimiter in ["\n", "\r", "\r\n", "\u{2029}"] {
        let text = format!("short{delimiter}{giant}{delimiter}tail");
        let lengths = paragraph_lengths(&text);
        assert_eq!(lengths, vec![5, 20_000, 4], "delimiter {delimiter:?}");
        let plan = plan_raw(&lengths, production);
        assert_eq!(
            plan,
            vec![FoldRange {
                paragraph: 1,
                head_chars: HEAD_CAP,
                tail_chars: c(20_000 - HEAD_CAP.as_chars()),
            }]
        );
        // The split falls inside the paragraph, before its delimiter.
        let paragraph_start = 5 + delimiter.len();
        let paragraph = &text[paragraph_start..paragraph_start + giant.len()];
        assert_eq!(
            fold_byte_offset(paragraph, plan[0].head_chars),
            usize::try_from(HEAD_CAP.as_chars()).expect("HEAD_CAP fits a usize")
        );
    }
}

#[test]
fn moderately_long_prose_folds_nothing() {
    // Protected only because some paragraphs are 600 characters long, with
    // every window of R_FOLD paragraphs within V_BATCH.
    let lengths: Vec<u64> = (0..1_000)
        .map(|index| if index % 4 == 0 { 600 } else { 40 })
        .collect();
    assert!(worst_window(&lengths, R_FOLD) <= V_BATCH);
    assert!(plan_raw(&lengths, FoldThresholds::PRODUCTION).is_empty());
}

#[test]
fn a_lone_giant_paragraph_shows_head_cap() {
    let mut lengths = vec![80; 500];
    lengths[250] = 1_900_000;
    let shown = visible(&lengths, FoldThresholds::PRODUCTION);
    assert_eq!(shown[250], HEAD_CAP.as_chars());
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
            .all(|&chars| chars == N_DENSE.as_chars())
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
    // The thresholds before `power-of-two-thresholds` (224 · 250 = 56,000)
    // had E = 0, and so does production since the amendment (D12):
    // 65,536 - 256 · 256 = 0.
    let (n_dense, v_batch, r_fold) = (250, 56_000, 224usize);
    let mut lengths = vec![10; 400];
    lengths[200] = n_dense + 1;
    let r_batch = r_fold as u64;
    assert_eq!(v_batch - r_batch * n_dense, 0);
    let superseded = superseded_excess_budget_rule(&lengths, n_dense, v_batch, r_batch);
    assert_eq!(superseded[200], n_dense, "the superseded rule folds it");
    assert!(
        plan_raw(
            &lengths,
            small(HEAD_CAP.as_chars(), n_dense, v_batch, r_fold)
        )
        .is_empty()
    );
    assert_eq!(V_BATCH - (R_BATCH as u64) * N_DENSE.as_chars(), 0);
    let mut production = vec![10; 400];
    production[200] = N_DENSE.as_chars() + 1;
    assert!(plan_raw(&production, FoldThresholds::PRODUCTION).is_empty());
}

#[test]
fn streaming_emits_with_a_lookahead_of_r_fold_minus_one() {
    let thresholds = small(8, 2, 6, 3);
    let mut state = FoldState::<4>::new(thresholds).expect("r_fold fits the ring");
    assert_eq!(fold_step(&mut state, c(8)), None, "warm-up");
    assert_eq!(fold_step(&mut state, c(0)), None, "warm-up");
    // The first full window [8, 0, 8] is over budget: paragraph 0 is decided.
    assert_eq!(
        fold_step(&mut state, c(8)),
        Some(FoldRange {
            paragraph: 0,
            head_chars: c(2),
            tail_chars: c(6),
        })
    );
    assert_eq!(state.paragraphs(), 3);
    assert_eq!(state.thresholds(), thresholds);
    let rest: Vec<FoldRange> = fold_finish(state).collect();
    assert_eq!(
        rest,
        vec![FoldRange {
            paragraph: 2,
            head_chars: c(2),
            tail_chars: c(6),
        }]
    );
}

#[test]
fn locate_maps_visible_hidden_and_past_end() {
    let range = FoldRange {
        paragraph: 3,
        head_chars: c(4),
        tail_chars: c(6),
    };
    assert_eq!(range.locate(c(0)), FoldLocation::Visible(c(0)));
    assert_eq!(range.locate(c(3)), FoldLocation::Visible(c(3)));
    assert_eq!(range.locate(c(4)), FoldLocation::Hidden(c(0)));
    assert_eq!(range.locate(c(10)), FoldLocation::Hidden(c(6)));
    assert_eq!(range.locate(c(11)), FoldLocation::PastEnd);
    assert_eq!(range.locate(c(u64::MAX)), FoldLocation::PastEnd);
    for column in 0..=10 {
        assert_eq!(range.locate(c(column)).column_of(range), Some(c(column)));
    }
    assert_eq!(FoldLocation::PastEnd.column_of(range), None);
    let overflowing = FoldRange {
        paragraph: 0,
        head_chars: c(u64::MAX),
        tail_chars: c(0),
    };
    assert_eq!(FoldLocation::Hidden(c(1)).column_of(overflowing), None);
}

#[test]
fn fold_byte_offset_counts_scalars() {
    let text = "aé€𝄞z";
    assert_eq!(fold_byte_offset(text, c(0)), 0);
    assert_eq!(fold_byte_offset(text, c(1)), 1);
    assert_eq!(fold_byte_offset(text, c(2)), 3);
    assert_eq!(fold_byte_offset(text, c(3)), 6);
    assert_eq!(fold_byte_offset(text, c(4)), 10);
    assert_eq!(fold_byte_offset(text, c(5)), text.len());
    assert_eq!(fold_byte_offset(text, c(99)), text.len());
    assert_eq!(fold_byte_offset("", c(3)), 0);
}

#[test]
fn the_stage_functions_are_exact() {
    let thresholds = small(8, 5, 10, 2);
    assert_eq!(length_cap(c(7), thresholds), c(7));
    assert_eq!(length_cap(c(9), thresholds), c(8));
    assert_eq!(dense_visible(c(4), c(5)), c(4));
    assert_eq!(dense_visible(c(6), c(5)), c(5));
    assert_eq!(density_cap(c(8), true, thresholds), c(5));
    assert_eq!(density_cap(c(8), false, thresholds), c(8));
    assert!(!window_is_over_budget(c(10), c(10)));
    assert!(window_is_over_budget(c(11), c(10)));
    assert!(!window_is_over_budget(c(u64::MAX), c(u64::MAX)));
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
        let _ = fold_step(&mut state, c(length));
        assert_eq!(state.capped_sum, state.direct_capped_sum());
    }
}
