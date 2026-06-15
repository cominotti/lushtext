// SPDX-License-Identifier: GPL-3.0-or-later

//! SIMD-accelerated fuzzy scoring helpers for the command palette.
//!
//! This slice owns matching and ranking only. Higher-level indexing and
//! command-registry concerns stay in sibling modules.

use crate::model::palette::{ScoredResult, SearchResultItem};
use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Reusable nucleo fuzzy query state for scoring many candidates.
pub(super) struct FuzzyQuery {
    matcher: Matcher,
    atom: Atom,
    buf: Vec<char>,
}

impl FuzzyQuery {
    /// Build one matcher/atom pair with the palette-wide fuzzy configuration.
    pub(super) fn new(query: &str) -> Self {
        Self::with_kind(query, AtomKind::Fuzzy)
    }

    fn with_kind(query: &str, kind: AtomKind) -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
            atom: Atom::new(
                query,
                CaseMatching::Ignore,
                Normalization::Smart,
                kind,
                false,
            ),
            buf: Vec::new(),
        }
    }

    /// Score one candidate while reusing the matcher and UTF-32 conversion buffer.
    pub(super) fn score(&mut self, candidate: &str) -> Option<u32> {
        self.buf.clear();
        let haystack = Utf32Str::new(candidate, &mut self.buf);
        self.atom.score(haystack, &mut self.matcher).map(u32::from)
    }
}

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
