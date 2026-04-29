// SPDX-License-Identifier: GPL-3.0-or-later

//! SIMD-accelerated fuzzy scoring helpers for the command palette.
//!
//! This slice owns matching and ranking only. Higher-level indexing and
//! command-registry concerns stay in sibling modules.

use crate::model::palette::{ScoredResult, SearchResultItem};
use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Score a fuzzy subsequence match of `query` against `candidate` using nucleo.
#[must_use]
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let atom = Atom::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    );
    let mut buf = Vec::new();
    let haystack = Utf32Str::new(candidate, &mut buf);
    atom.score(haystack, &mut matcher).map(u32::from)
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

    let mut matcher = Matcher::new(Config::DEFAULT);
    let atom = Atom::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    );
    let mut buf = Vec::new();

    let mut results: Vec<ScoredResult<'a>> = items
        .filter_map(|item| {
            let text = get_text(item);
            buf.clear();
            let haystack = Utf32Str::new(text, &mut buf);
            atom.score(haystack, &mut matcher)
                .map(|score| ScoredResult {
                    item: wrap(item),
                    score: u32::from(score),
                })
        })
        .collect();
    results.sort_unstable_by_key(|result| std::cmp::Reverse(result.score));
    results.truncate(max);
    results
}
