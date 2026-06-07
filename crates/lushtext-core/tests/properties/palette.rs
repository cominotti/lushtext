// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for command-palette merge ordering.
//!
//! The generated inputs model two already-scored result streams so the test can
//! focus on max truncation, descending score order, and left-side tie priority.

use lushtext_core::model::palette::{CommandCategory, CommandDef, ScoredResult, SearchResultItem};
use lushtext_core::services::palette::merge_sorted_for_property_test;
use proptest::prelude::*;

use crate::support;

/// Synthetic left-side command used to identify merge tie precedence.
static LEFT_COMMAND: CommandDef = CommandDef {
    id: "property.left",
    label: "Left Property Command",
    category: CommandCategory::App,
    shortcut: None,
};
/// Synthetic right-side command used to identify merge tie precedence.
static RIGHT_COMMAND: CommandDef = CommandDef {
    id: "property.right",
    label: "Right Property Command",
    category: CommandCategory::App,
    shortcut: None,
};

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn merge_preserves_order_limit_and_left_ties(
        left_scores in score_stream(),
        right_scores in score_stream(),
        max in 0usize..=support::MAX_VECTOR_LEN * 2,
    ) {
        let left_scores = sort_descending(left_scores);
        let right_scores = sort_descending(right_scores);
        let left = scored_results(&LEFT_COMMAND, &left_scores);
        let right = scored_results(&RIGHT_COMMAND, &right_scores);
        let expected = expected_merge(&left_scores, &right_scores, max);

        let actual = merge_sorted_for_property_test(left, right, max);
        let actual_pairs = result_pairs(&actual);

        prop_assert!(actual.len() <= max);
        prop_assert_eq!(actual_pairs, expected);
        prop_assert!(actual.windows(2).all(|window| window[0].score >= window[1].score));
    }
}

/// Generate a bounded stream of relevance scores.
fn score_stream() -> impl Strategy<Value = Vec<u32>> {
    prop::collection::vec(0u32..=10_000, 0..=support::MAX_VECTOR_LEN)
}

/// Sort generated scores into the precondition expected by the merge helper.
fn sort_descending(mut scores: Vec<u32>) -> Vec<u32> {
    scores.sort_unstable_by(|left, right| right.cmp(left));
    scores
}

/// Wrap generated scores with a synthetic command identity.
fn scored_results<'a>(command: &'a CommandDef, scores: &[u32]) -> Vec<ScoredResult<'a>> {
    scores
        .iter()
        .copied()
        .map(|score| ScoredResult {
            item: SearchResultItem::Command(command),
            score,
        })
        .collect()
}

/// Independently model the merge policy, including left-side tie priority.
fn expected_merge(
    left_scores: &[u32],
    right_scores: &[u32],
    max: usize,
) -> Vec<(&'static str, u32)> {
    let mut expected = Vec::new();
    let mut left_index = 0usize;
    let mut right_index = 0usize;

    while expected.len() < max {
        match (
            left_scores.get(left_index).copied(),
            right_scores.get(right_index).copied(),
        ) {
            (Some(left), Some(right)) if left >= right => {
                expected.push((LEFT_COMMAND.id, left));
                left_index += 1;
            }
            (Some(_) | None, Some(right)) => {
                expected.push((RIGHT_COMMAND.id, right));
                right_index += 1;
            }
            (Some(left), None) => {
                expected.push((LEFT_COMMAND.id, left));
                left_index += 1;
            }
            (None, None) => break,
        }
    }

    expected
}

/// Convert palette results into simple source-and-score pairs for assertions.
fn result_pairs(results: &[ScoredResult<'_>]) -> Vec<(&'static str, u32)> {
    results
        .iter()
        .map(|result| match result.item {
            SearchResultItem::Command(command) => (command.id, result.score),
            SearchResultItem::OpenFile(_) | SearchResultItem::File(_) => {
                ("unexpected.palette.result.kind", result.score)
            }
        })
        .collect()
}
