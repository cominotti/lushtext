// SPDX-License-Identifier: GPL-3.0-or-later

//! SIMD-accelerated fuzzy scoring helpers for the command palette.
//!
//! This slice owns matching and ranking only. Higher-level indexing and
//! command-registry concerns stay in sibling modules.

use crate::model::palette::{ScoredResult, SearchResultItem};
use crate::services::fuzzy::FuzzyQuery;

/// Score a fuzzy subsequence match of `query` against `candidate` using nucleo.
#[must_use]
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    FuzzyQuery::new(query).score(candidate)
}

/// Generic search helper: filter + score items using nucleo, sort by score
/// descending, cap at max. Reuses a single `Matcher` and char buffer across
/// all candidates for efficiency.
pub(super) fn search_items<'a, I, T, F, G>(
    items: I,
    get_text: F,
    wrap: G,
    query: &str,
    max: usize,
) -> Vec<ScoredResult<'a>>
where
    I: Iterator<Item = &'a T>,
    T: 'a,
    F: Fn(&T) -> &str,
    G: Fn(&'a T) -> SearchResultItem<'a>,
{
    if query.is_empty() {
        return items
            .map(|item| ScoredResult {
                item: wrap(item),
                score: 0,
            })
            .take(max)
            .collect();
    }

    let mut fuzzy_query = FuzzyQuery::new(query);

    let mut results: Vec<ScoredResult<'a>> = items
        .filter_map(|item| {
            let text = get_text(item);
            fuzzy_query.score(text).map(|score| ScoredResult {
                item: wrap(item),
                score,
            })
        })
        .collect();
    results.sort_unstable_by_key(|result| std::cmp::Reverse(result.score));
    results.truncate(max);
    results
}
