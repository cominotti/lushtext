// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated invariants for the sliced protected install planner
//! (`sliced-protected-install`, task 3.4) beyond the Kani domain: many
//! paragraphs of every delimiter and scalar length, C2's fold plans at small
//! thresholds, small and production slice budgets, and multi-megabyte tails.
//!
//! The whole-plan properties of design D5 (K1–K6, K8, K9; Kani could not
//! check them at any domain tried, `docs/next/formal-verification.md`) are
//! checked on each input: progress and exact coverage, scalar and CRLF safety,
//! homogeneity, the per-slice bounds (hidden slices within `tail_slice_bytes`
//! and, but for a tail's last, at least `tail_slice_bytes − 3`; a visible slice
//! within the budget or one whole unfolded paragraph of at most `N_visible`
//! characters), the boundary placement (`last_of_tail`), the head ending
//! exactly at the fold point, a fold's head and tail covering its paragraph
//! (`head_chars + tail_chars`), the slice-count bound with `step = B − 3`, and
//! the clear plan's coverage. Not checked here: K8's "the head holds exactly
//! `N_visible` characters", which depends on C2's density rule, which may
//! shorten a head (K8b proves C2's fold of a lone paragraph).

use lushtext_core::model::buffer_replacement::SliceBudget;
use lushtext_core::model::paragraph_boundary::next_paragraph_break;
use lushtext_core::model::protected_fold::{
    FoldRange, FoldThresholds, fold_byte_offset, plan_folds,
};
use lushtext_core::model::protected_install::{
    PRODUCTION_LIMITS, ProtectedInstallCursor, ProtectedInstallLimits, SliceKind,
    clear_ranges_for_turn, plan_protected_install,
};
use proptest::prelude::*;

fn spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    while let Some(found) = next_paragraph_break(text, start) {
        spans.push((start, found.content_end));
        start = found.next_start;
    }
    spans.push((start, text.len()));
    spans
}

fn folds(text: &str, thresholds: FoldThresholds) -> Vec<FoldRange> {
    let lengths: Vec<u64> = spans(text)
        .iter()
        .map(|(start, end)| text[*start..*end].chars().count() as u64)
        .collect();
    plan_folds(&lengths, thresholds)
}

/// Every whole-plan property on one input. `head_cap` is C2's `N_visible`
/// for the fold plan `ranges` came from: a paragraph it did not fold has at
/// most that many characters.
fn check(
    text: &str,
    ranges: &[FoldRange],
    budget: SliceBudget,
    limits: ProtectedInstallLimits,
    head_cap: u64,
) {
    let bytes = text.as_bytes();
    let spans = spans(text);
    // K8, the fold's half: a folded paragraph's head and tail are its whole
    // content (`head_chars + tail_chars` = the paragraph's characters).
    for range in ranges {
        let (start, end) = spans[usize::try_from(range.paragraph).expect("fits")];
        assert_eq!(
            range.head_chars + range.tail_chars,
            text[start..end].chars().count() as u64,
            "a fold covers its paragraph exactly"
        );
    }
    // Paragraph starts after the first, for K5's visible half.
    let paragraph_starts: Vec<usize> = spans.iter().skip(1).map(|(start, _)| *start).collect();
    let tails: Vec<(usize, usize)> = ranges
        .iter()
        .map(|range| {
            let (start, end) = spans[usize::try_from(range.paragraph).expect("fits")];
            (
                start + fold_byte_offset(&text[start..end], range.head_chars),
                end,
            )
        })
        .collect();
    let mut cursor = ProtectedInstallCursor::new();
    let mut start = 0;
    let mut steps = 0usize;
    let mut tail_slices = vec![0usize; tails.len()];
    let mut rebuilt = Vec::with_capacity(bytes.len());
    while let Some(slice) = plan_protected_install(text, ranges, &mut cursor, budget, limits) {
        let end = slice.end;
        assert!(
            end > start || slice.kind == SliceKind::Head,
            "progress at {start}"
        );
        steps += 1;
        assert!(steps <= 2 * bytes.len() + 2, "terminates");
        assert!(text.is_char_boundary(end), "scalar boundary at {end}");
        assert!(
            !(end > 0 && end < bytes.len() && bytes[end - 1] == b'\r' && bytes[end] == b'\n'),
            "CRLF at {end}"
        );
        rebuilt.extend_from_slice(&bytes[start..end]);
        let tail = tails
            .iter()
            .position(|(tail_start, tail_end)| start >= *tail_start && start < *tail_end);
        match slice.kind {
            SliceKind::HiddenTail => {
                let index = tail.expect("a hidden slice lies in a tail");
                let tail_end = tails[index].1;
                assert!(end <= tail_end);
                assert_eq!(slice.last_of_tail, end == tail_end);
                assert!(end - start <= limits.tail_slice_bytes());
                if !slice.last_of_tail {
                    assert!(end - start + 3 >= limits.tail_slice_bytes());
                }
                tail_slices[index] += 1;
            }
            SliceKind::Head => {
                assert!(
                    tails.iter().any(|(tail_start, _)| *tail_start == end),
                    "head ends at a fold point"
                );
            }
            SliceKind::Visible => {
                assert!(tail.is_none());
                assert!(
                    !tails
                        .iter()
                        .any(|(tail_start, tail_end)| tail_start < tail_end
                            && *tail_start >= start
                            && *tail_start < end)
                );
                // K5, the visible half: a visible slice fits the budget (bytes
                // and visible characters, delimiters excluded), or it is one
                // whole paragraph unit, which C2 left unfolded and so holds at
                // most `N_visible` characters.
                let visible_chars: u64 = spans
                    .iter()
                    .map(|(p_start, p_end)| {
                        let (lo, hi) = ((*p_start).max(start), (*p_end).min(end));
                        if lo < hi {
                            text[lo..hi].chars().count() as u64
                        } else {
                            0
                        }
                    })
                    .sum();
                let fits = (end - start) as u64 <= budget.bytes && visible_chars <= budget.chars;
                if !fits {
                    assert!(
                        !paragraph_starts.iter().any(|p| *p > start && *p < end),
                        "an over-budget visible slice at {start}..{end} holds one paragraph"
                    );
                    assert!(
                        visible_chars <= head_cap,
                        "an over-budget visible paragraph at {start}..{end} has {visible_chars} characters, more than N_visible {head_cap}"
                    );
                }
            }
        }
        start = end;
    }
    assert_eq!(start, bytes.len());
    assert_eq!(rebuilt, bytes);
    let step = limits.tail_slice_bytes() - 3;
    for (index, (tail_start, tail_end)) in tails.iter().enumerate() {
        assert!(
            tail_slices[index] <= (tail_end - tail_start).div_ceil(step),
            "slice count"
        );
    }
}

fn paragraph() -> impl Strategy<Value = String> {
    let scalar = prop_oneof![Just("a"), Just("é"), Just("€"), Just("𝄞"), Just(" ")];
    let length = prop_oneof![6 => 0usize..40, 2 => 40usize..400, 1 => 2_000usize..20_000];
    let delimiter = prop_oneof![Just("\n"), Just("\r"), Just("\r\n"), Just("\u{2029}")];
    length
        .prop_flat_map(move |length| {
            (
                prop::collection::vec(scalar.clone(), length),
                delimiter.clone(),
            )
        })
        .prop_map(|(scalars, delimiter)| format!("{}{delimiter}", scalars.concat()))
}

fn document() -> impl Strategy<Value = String> {
    (prop::collection::vec(paragraph(), 0..24), any::<bool>()).prop_map(|(paragraphs, trim)| {
        let mut text = paragraphs.concat();
        if trim {
            // An unterminated last paragraph.
            while text.ends_with(['\n', '\r', '\u{2029}']) {
                text.pop();
            }
        }
        text
    })
}

fn thresholds() -> impl Strategy<Value = FoldThresholds> {
    (1usize..=8, 1u64..=16, 0u64..=40, 0u64..=200).prop_filter_map(
        "valid thresholds",
        |(r_fold, n_dense, head_extra, v_extra)| {
            FoldThresholds::new(
                n_dense + head_extra,
                n_dense,
                r_fold as u64 * n_dense + v_extra,
                r_fold,
            )
        },
    )
}

fn budget() -> impl Strategy<Value = SliceBudget> {
    prop_oneof![
        Just(SliceBudget::BYTES_ONLY),
        Just(SliceBudget::PACED),
        (1u64..=4_096, 1u64..=512).prop_map(|(bytes, chars)| SliceBudget { bytes, chars }),
    ]
}

proptest! {
    #[test]
    fn small_budgets_hold_every_planner_property(
        text in document(),
        thresholds in thresholds(),
        budget in budget(),
        tail in 4usize..=64,
    ) {
        let ranges = folds(&text, thresholds);
        let limits = ProtectedInstallLimits::new(tail, tail).expect("valid limits");
        check(&text, &ranges, budget, limits, thresholds.head_cap());
    }

    #[test]
    fn clear_turns_cover_every_range_once_from_the_start(
        sizes in prop::collection::vec(0u64..300_000, 0..64),
    ) {
        let mut first = 0;
        let mut deleted = 0u64;
        while first < sizes.len() {
            let taken = clear_ranges_for_turn(&sizes[first..], PRODUCTION_LIMITS);
            prop_assert!(taken >= 1 && first + taken <= sizes.len());
            let bytes: u64 = sizes[first..first + taken].iter().sum();
            prop_assert!(taken == 1 || bytes <= PRODUCTION_LIMITS.clear_slice_bytes() as u64);
            deleted += bytes;
            first += taken;
        }
        prop_assert_eq!(deleted, sizes.iter().sum::<u64>());
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]

    #[test]
    fn production_budgets_hold_on_multi_megabyte_tails(
        prefix in document(),
        giant in 1_000_000usize..4_000_000,
        unit in prop_oneof![Just("g"), Just("é"), Just("€a"), Just("𝄞ab")],
        delimiter in prop_oneof![Just(""), Just("\n"), Just("\r\n"), Just("\u{2029}")],
    ) {
        let text = format!("{prefix}\n{}{delimiter}tail\n", unit.repeat(giant / unit.len()));
        let thresholds = FoldThresholds::PRODUCTION;
        let ranges = folds(&text, thresholds);
        prop_assert!(!ranges.is_empty());
        check(&text, &ranges, SliceBudget::BYTES_ONLY, PRODUCTION_LIMITS, thresholds.head_cap());
    }
}

/// Whether `bytes` fit `budget_us` at `cost` ns/KiB with `margin` percent:
/// `ceil(ceil(bytes · cost / 1024) · (100 + margin) / 100) ≤ budget ns`, the
/// formula `for_settle_budget` documents, computed directly.
fn fits(bytes: u128, budget_us: u64, cost: u64, margin: u64) -> bool {
    let ns = (bytes * u128::from(cost)).div_ceil(1024);
    let with_margin = (ns * (100 + u128::from(margin))).div_ceil(100);
    with_margin <= u128::from(budget_us.saturating_mul(1000))
}

/// Inputs from every magnitude: small values, values near the cap's
/// boundary, and full-range `u64`s.
fn magnitude() -> impl Strategy<Value = u64> {
    prop_oneof![
        0u64..=64,
        0u64..=200_000,
        0u64..=u64::from(u32::MAX),
        any::<u64>()
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// The half of K10 Kani does not finish (128-bit multiplication and
    /// division by symbolic divisors): an `Ok` value fits the budget with its
    /// margin and is the largest that does (or the cap); `Err` exactly when
    /// the minimum does not fit.
    #[test]
    fn settle_budget_derivation_fits_and_is_the_largest(
        budget_us in magnitude(),
        cost in magnitude(),
        margin in magnitude(),
    ) {
        use lushtext_core::model::protected_install::{MAX_SLICE_BYTES, MIN_TAIL_SLICE_BYTES};
        match ProtectedInstallLimits::for_settle_budget(budget_us, cost, margin) {
            Ok(bytes) => {
                prop_assert!(fits(bytes as u128, budget_us, cost, margin));
                prop_assert!(bytes == MAX_SLICE_BYTES || !fits(bytes as u128 + 1, budget_us, cost, margin));
            }
            Err(_) => prop_assert!(!fits(MIN_TAIL_SLICE_BYTES as u128, budget_us, cost, margin)),
        }
    }
}
