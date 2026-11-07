// SPDX-License-Identifier: GPL-3.0-or-later

//! SIMD-accelerated fuzzy scoring and bounded ranking for the command palette.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::model::palette::{ScoredResult, SearchResultItem};
use crate::services::fuzzy::FuzzyQuery;

use super::runtime::{PaletteSearchCancellation, PaletteSearchMetrics, PaletteSearchOutcome};

/// Candidate interval between cooperative cancellation checks.
///
/// This keeps atomic reads out of the inner matcher call while bounding obsolete
/// work to at most this many additional candidates after cancellation.
///
/// Kind: cap; a power of two, rounded down.
pub const PALETTE_CANCEL_CHECK_INTERVAL: usize = 256;

/// Score a fuzzy subsequence match of `query` against `candidate` using nucleo.
#[must_use]
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    FuzzyQuery::new(query).score(candidate)
}

/// Compare the one total palette rank: higher score, then lower source ordinal.
#[must_use]
pub fn compare_palette_rank(
    left_score: u32,
    left_ordinal: usize,
    right_score: u32,
    right_ordinal: usize,
) -> Ordering {
    right_score
        .cmp(&left_score)
        .then_with(|| left_ordinal.cmp(&right_ordinal))
}

struct RetainedCandidate<'a> {
    item: SearchResultItem<'a>,
    score: u32,
    source_ordinal: usize,
}

/// Bytes one ranked row costs the selector at its peak: the retained
/// candidate (the borrowed item, its score and its source ordinal) plus the
/// `u32` ordinal it becomes, which coexist while the ranking is converted
/// (`drill-down-palette-categories` D3): a 16-byte `SearchResultItem` (four
/// reference variants, which no niche packs), a `u32` and a `usize` make a
/// 32-byte candidate, plus 4.
///
/// Written as the literal the layout gives on a 64-bit target, because the
/// threshold inventory evaluates it (`PALETTE_CATEGORY_FLOOR_RETENTION`); the
/// assertion below fails the build if the layout moves, and on a 32-bit
/// target the literal overstates the charge, which only lowers the row limit.
///
/// Kind: external fact; the pinned layout of `RetainedCandidate` plus a `u32`.
pub const PALETTE_CATEGORY_CANDIDATE_BYTES: u64 = 36;

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    PALETTE_CATEGORY_CANDIDATE_BYTES
        == (std::mem::size_of::<RetainedCandidate<'static>>() + std::mem::size_of::<u32>()) as u64
);

/// How a selector counts the included items after its retained prefix on the
/// empty query (`drill-down-palette-categories` D1). The count retains and
/// scores nothing and observes cancellation at the selector's interval.
pub(super) enum RestCount<'p, T> {
    /// Count with the retention predicate itself.
    Walk,
    /// The predicate admits every item: count the rest by its exact length,
    /// in O(1).
    Length,
    /// Count with a cheaper predicate that equals the retention predicate.
    WalkWith(&'p mut dyn FnMut(&T) -> bool),
}

#[derive(Clone, Copy)]
struct SelectionPolicy<'a> {
    max: usize,
    cancellation: &'a PaletteSearchCancellation,
    cancellation_interval: usize,
    progress: Option<&'a dyn Fn(usize)>,
}

#[derive(Clone, Copy)]
pub(super) struct SearchProgressPolicy<'a> {
    pub max: usize,
    pub cancellation: &'a PaletteSearchCancellation,
    pub progress: &'a dyn Fn(usize),
}

impl PartialEq for RetainedCandidate<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score && self.source_ordinal == other.source_ordinal
    }
}

impl Eq for RetainedCandidate<'_> {}

impl PartialOrd for RetainedCandidate<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RetainedCandidate<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_palette_rank(
            self.score,
            self.source_ordinal,
            other.score,
            other.source_ordinal,
        )
    }
}

/// Select one source's exact top results without retaining every match.
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
    let cancellation = PaletteSearchCancellation::default();
    match search_items_cancellable(
        items,
        |_| true,
        |item, fuzzy_query| fuzzy_query.score(get_text(item)),
        wrap,
        query,
        max,
        &cancellation,
    ) {
        PaletteSearchOutcome::Complete { value, .. } => value,
        PaletteSearchOutcome::Cancelled { .. } => unreachable!("fresh token cannot cancel"),
    }
}

/// Select one source with filtering, custom scoring, and cooperative cancellation.
///
/// On the empty query the included items after the retained prefix are
/// counted with `include` itself ([`RestCount::Walk`]).
pub(super) fn search_items_cancellable<'a, I, T, P, F, G>(
    items: I,
    include: P,
    score: F,
    wrap: G,
    query: &str,
    max: usize,
    cancellation: &PaletteSearchCancellation,
) -> PaletteSearchOutcome<Vec<ScoredResult<'a>>>
where
    I: Iterator<Item = &'a T>,
    T: 'a,
    P: FnMut(&T) -> bool,
    F: FnMut(&T, &mut FuzzyQuery) -> Option<u32>,
    G: Fn(&'a T) -> SearchResultItem<'a>,
{
    search_items_cancellable_counting(
        items,
        include,
        score,
        wrap,
        query,
        CountedSelection {
            max,
            cancellation,
            rest: RestCount::Walk,
        },
    )
}

/// One counted selection: the row limit, the cancellation token and the rule
/// for counting the empty query's rest, crossing from a source into the
/// selector as one value.
pub(super) struct CountedSelection<'c, 'p, T> {
    pub max: usize,
    pub cancellation: &'c PaletteSearchCancellation,
    pub rest: RestCount<'p, T>,
}

/// [`search_items_cancellable`] with an explicit rule for counting the
/// empty query's rest.
pub(super) fn search_items_cancellable_counting<'a, I, T, P, F, G>(
    items: I,
    mut include: P,
    mut score: F,
    wrap: G,
    query: &str,
    selection: CountedSelection<'_, '_, T>,
) -> PaletteSearchOutcome<Vec<ScoredResult<'a>>>
where
    I: Iterator<Item = &'a T>,
    T: 'a,
    P: FnMut(&T) -> bool,
    F: FnMut(&T, &mut FuzzyQuery) -> Option<u32>,
    G: Fn(&'a T) -> SearchResultItem<'a>,
{
    let CountedSelection {
        max,
        cancellation,
        rest,
    } = selection;
    search_items_cancellable_controlled(
        items,
        &mut include,
        &mut score,
        wrap,
        query,
        SelectionPolicy {
            max,
            cancellation,
            cancellation_interval: PALETTE_CANCEL_CHECK_INTERVAL,
            progress: None,
        },
        rest,
    )
}

pub(super) fn search_items_cancellable_with_progress<'a, I, T, P, F, G>(
    items: I,
    mut include: P,
    mut score: F,
    wrap: G,
    query: &str,
    policy: SearchProgressPolicy<'_>,
) -> PaletteSearchOutcome<Vec<ScoredResult<'a>>>
where
    I: Iterator<Item = &'a T>,
    T: 'a,
    P: FnMut(&T) -> bool,
    F: FnMut(&T, &mut FuzzyQuery) -> Option<u32>,
    G: Fn(&'a T) -> SearchResultItem<'a>,
{
    let SearchProgressPolicy {
        max,
        cancellation,
        progress,
    } = policy;
    search_items_cancellable_controlled(
        items,
        &mut include,
        &mut score,
        wrap,
        query,
        SelectionPolicy {
            max,
            cancellation,
            cancellation_interval: PALETTE_CANCEL_CHECK_INTERVAL,
            progress: Some(progress),
        },
        RestCount::Walk,
    )
}

fn search_items_cancellable_controlled<'a, I, T, P, F, G>(
    items: I,
    include: &mut P,
    score: &mut F,
    wrap: G,
    query: &str,
    policy: SelectionPolicy<'_>,
    rest: RestCount<'_, T>,
) -> PaletteSearchOutcome<Vec<ScoredResult<'a>>>
where
    I: Iterator<Item = &'a T>,
    T: 'a,
    P: FnMut(&T) -> bool,
    F: FnMut(&T, &mut FuzzyQuery) -> Option<u32>,
    G: Fn(&'a T) -> SearchResultItem<'a>,
{
    let SelectionPolicy {
        max,
        cancellation,
        cancellation_interval,
        progress,
    } = policy;
    let mut metrics = PaletteSearchMetrics::default();
    if cancellation.is_cancelled() {
        return PaletteSearchOutcome::Cancelled { metrics };
    }
    if max == 0 {
        return PaletteSearchOutcome::Complete {
            value: Vec::new(),
            metrics,
        };
    }

    if query.is_empty() {
        let reserve = items.size_hint().1.map_or(0, |upper| upper.min(max));
        let mut results = Vec::with_capacity(reserve);
        let mut items = items.enumerate();
        for (source_ordinal, item) in items.by_ref() {
            if metrics.candidates_examined % cancellation_interval.max(1) == 0 {
                if let Some(progress) = progress {
                    progress(metrics.candidates_examined);
                }
                if cancellation.is_cancelled() {
                    return PaletteSearchOutcome::Cancelled { metrics };
                }
            }
            metrics.candidates_examined = metrics.candidates_examined.saturating_add(1);
            if !include(item) {
                continue;
            }
            metrics.matching_candidates = metrics.matching_candidates.saturating_add(1);
            results.push(ScoredResult {
                item: wrap(item),
                score: 0,
                source_ordinal,
            });
            if results.len() == max {
                break;
            }
        }
        metrics.peak_retained_per_source = results.len();
        if results.len() == max {
            let rest = count_rest(items.map(|(_, item)| item), include, rest, &policy);
            match rest {
                Some(counted) => {
                    metrics.matching_candidates =
                        metrics.matching_candidates.saturating_add(counted.matching);
                    metrics.candidates_counted = counted.visited;
                }
                None => return PaletteSearchOutcome::Cancelled { metrics },
            }
        }
        return PaletteSearchOutcome::Complete {
            value: results,
            metrics,
        };
    }

    let mut fuzzy_query = FuzzyQuery::new(query);
    let reserve = items.size_hint().1.map_or(0, |upper| upper.min(max));
    let mut retained = BinaryHeap::with_capacity(reserve);
    for (source_ordinal, item) in items.enumerate() {
        if metrics.candidates_examined % cancellation_interval.max(1) == 0 {
            if let Some(progress) = progress {
                progress(metrics.candidates_examined);
            }
            if cancellation.is_cancelled() {
                return PaletteSearchOutcome::Cancelled { metrics };
            }
        }
        metrics.candidates_examined = metrics.candidates_examined.saturating_add(1);
        if !include(item) {
            continue;
        }
        let Some(candidate_score) = score(item, &mut fuzzy_query) else {
            continue;
        };
        metrics.matching_candidates = metrics.matching_candidates.saturating_add(1);
        let candidate = RetainedCandidate {
            item: wrap(item),
            score: candidate_score,
            source_ordinal,
        };
        if retained.len() < max {
            retained.push(candidate);
        } else if retained.peek().is_some_and(|worst| candidate < *worst) {
            retained.pop();
            retained.push(candidate);
        }
        metrics.peak_retained_per_source = metrics.peak_retained_per_source.max(retained.len());
    }

    let mut results = retained.into_vec();
    if cancellation.is_cancelled() {
        return PaletteSearchOutcome::Cancelled { metrics };
    }
    results.sort_unstable_by(|left, right| {
        compare_palette_rank(
            left.score,
            left.source_ordinal,
            right.score,
            right.source_ordinal,
        )
    });
    PaletteSearchOutcome::Complete {
        value: results
            .into_iter()
            .map(|result| ScoredResult {
                item: result.item,
                score: result.score,
                source_ordinal: result.source_ordinal,
            })
            .collect(),
        metrics,
    }
}

/// What [`count_rest`] found after the retained prefix.
struct RestCounted {
    matching: usize,
    visited: usize,
}

/// Count the included items after the empty query's retained prefix without
/// retaining or scoring them; `None` when cancellation was observed.
fn count_rest<'a, T: 'a, P>(
    items: impl Iterator<Item = &'a T>,
    include: &mut P,
    rest: RestCount<'_, T>,
    policy: &SelectionPolicy<'_>,
) -> Option<RestCounted>
where
    P: FnMut(&T) -> bool,
{
    match rest {
        RestCount::Walk => walk_rest(items, include, policy),
        RestCount::WalkWith(predicate) => walk_rest(items, predicate, policy),
        RestCount::Length => {
            let (lower, upper) = items.size_hint();
            if upper == Some(lower) {
                Some(RestCounted {
                    matching: lower,
                    visited: 0,
                })
            } else {
                walk_rest(items, include, policy)
            }
        }
    }
}

fn walk_rest<'a, T: 'a>(
    items: impl Iterator<Item = &'a T>,
    predicate: &mut dyn FnMut(&T) -> bool,
    policy: &SelectionPolicy<'_>,
) -> Option<RestCounted> {
    let interval = policy.cancellation_interval.max(1);
    let mut counted = RestCounted {
        matching: 0,
        visited: 0,
    };
    for item in items {
        if counted.visited.is_multiple_of(interval) && policy.cancellation.is_cancelled() {
            return None;
        }
        counted.visited = counted.visited.saturating_add(1);
        if predicate(item) {
            counted.matching = counted.matching.saturating_add(1);
        }
    }
    Some(counted)
}

pub(super) fn search_items_full_sort_reference<'a, I, T, P, F, G>(
    items: I,
    mut include: P,
    mut score: F,
    wrap: G,
    query: &str,
    max: usize,
) -> Vec<ScoredResult<'a>>
where
    I: Iterator<Item = &'a T>,
    T: 'a,
    P: FnMut(&T) -> bool,
    F: FnMut(&T, &mut FuzzyQuery) -> Option<u32>,
    G: Fn(&'a T) -> SearchResultItem<'a>,
{
    if query.is_empty() {
        return items
            .enumerate()
            .filter(|(_, item)| include(item))
            .take(max)
            .map(|(source_ordinal, item)| ScoredResult {
                item: wrap(item),
                score: 0,
                source_ordinal,
            })
            .collect();
    }

    let mut fuzzy_query = FuzzyQuery::new(query);
    let mut results: Vec<_> = items
        .enumerate()
        .filter(|(_, item)| include(item))
        .filter_map(|(source_ordinal, item)| {
            score(item, &mut fuzzy_query).map(|score| ScoredResult {
                item: wrap(item),
                score,
                source_ordinal,
            })
        })
        .collect();
    results.sort_unstable_by(|left, right| {
        compare_palette_rank(
            left.score,
            left.source_ordinal,
            right.score,
            right.source_ordinal,
        )
    });
    results.truncate(max);
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::palette::{CommandCategory, CommandDef};

    static COMMANDS: [CommandDef; 4] = [
        CommandDef {
            id: "first",
            label: "same",
            category: CommandCategory::App,
            shortcut: None,
        },
        CommandDef {
            id: "second",
            label: "same",
            category: CommandCategory::App,
            shortcut: None,
        },
        CommandDef {
            id: "third",
            label: "same",
            category: CommandCategory::App,
            shortcut: None,
        },
        CommandDef {
            id: "fourth",
            label: "same",
            category: CommandCategory::App,
            shortcut: None,
        },
    ];

    #[test]
    fn bounded_selector_matches_reference_and_keeps_source_ties() {
        let cancellation = PaletteSearchCancellation::default();
        let bounded = search_items_cancellable(
            COMMANDS.iter(),
            |_| true,
            |command, fuzzy| fuzzy.score(command.label),
            SearchResultItem::Command,
            "same",
            2,
            &cancellation,
        );
        let reference = search_items_full_sort_reference(
            COMMANDS.iter(),
            |_| true,
            |command, fuzzy| fuzzy.score(command.label),
            SearchResultItem::Command,
            "same",
            2,
        );
        let PaletteSearchOutcome::Complete { value, metrics } = bounded else {
            panic!("fresh search should complete");
        };
        assert_eq!(metrics.peak_retained_per_source, 2);
        assert_eq!(value[0].source_ordinal, 0);
        assert_eq!(value[1].source_ordinal, 1);
        assert_eq!(
            value
                .iter()
                .map(|result| (result.score, result.source_ordinal))
                .collect::<Vec<_>>(),
            reference
                .iter()
                .map(|result| (result.score, result.source_ordinal))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn empty_query_directly_takes_the_requested_prefix() {
        let cancellation = PaletteSearchCancellation::default();
        let outcome = search_items_cancellable(
            COMMANDS.iter(),
            |_| true,
            |_command, _fuzzy| panic!("empty query must not score"),
            SearchResultItem::Command,
            "",
            2,
            &cancellation,
        );
        let PaletteSearchOutcome::Complete { value, metrics } = outcome else {
            panic!("fresh search should complete");
        };
        assert_eq!(value.len(), 2);
        assert_eq!(metrics.candidates_examined, 2);
        assert_eq!(metrics.peak_retained_per_source, 2);
    }

    #[test]
    fn empty_query_counts_the_rest_by_length_when_nothing_is_excluded() {
        let cancellation = PaletteSearchCancellation::default();
        let outcome = search_items_cancellable_counting(
            COMMANDS.iter(),
            |_| true,
            |_command, _fuzzy| panic!("empty query must not score"),
            SearchResultItem::Command,
            "",
            CountedSelection {
                max: 1,
                cancellation: &cancellation,
                rest: RestCount::Length,
            },
        );
        let PaletteSearchOutcome::Complete { value, metrics } = outcome else {
            panic!("fresh search should complete");
        };
        assert_eq!(value.len(), 1);
        assert_eq!(metrics.matching_candidates, COMMANDS.len());
        assert_eq!(
            metrics.candidates_counted, 0,
            "an O(1) count visits nothing"
        );
        assert_eq!(metrics.peak_retained_per_source, 1);
    }

    #[test]
    fn empty_query_counts_the_rest_with_the_predicate() {
        let cancellation = PaletteSearchCancellation::default();
        let outcome = search_items_cancellable(
            COMMANDS.iter(),
            |command| command.id != "third",
            |_command, _fuzzy| panic!("empty query must not score"),
            SearchResultItem::Command,
            "",
            1,
            &cancellation,
        );
        let PaletteSearchOutcome::Complete { value, metrics } = outcome else {
            panic!("fresh search should complete");
        };
        assert_eq!(value.len(), 1);
        assert_eq!(metrics.matching_candidates, 3);
        assert_eq!(metrics.candidates_counted, 3);
    }

    #[test]
    fn empty_query_count_uses_the_cheaper_predicate_when_given() {
        let cancellation = PaletteSearchCancellation::default();
        let mut cheaper = |command: &CommandDef| command.id.starts_with('f');
        let outcome = search_items_cancellable_counting(
            COMMANDS.iter(),
            |command| command.id.starts_with('f'),
            |_command, _fuzzy| panic!("empty query must not score"),
            SearchResultItem::Command,
            "",
            CountedSelection {
                max: 1,
                cancellation: &cancellation,
                rest: RestCount::WalkWith(&mut cheaper),
            },
        );
        let PaletteSearchOutcome::Complete { metrics, .. } = outcome else {
            panic!("fresh search should complete");
        };
        // "first" retained; "fourth" counted.
        assert_eq!(metrics.matching_candidates, 2);
    }

    #[test]
    fn empty_query_count_observes_cancellation() {
        const SAME: CommandDef = CommandDef {
            id: "same",
            label: "same",
            category: CommandCategory::App,
            shortcut: None,
        };
        static MANY: [CommandDef; 600] = [SAME; 600];
        let cancellation = PaletteSearchCancellation::default();
        let calls = std::cell::Cell::new(0usize);
        let outcome = search_items_cancellable(
            MANY.iter(),
            |_| {
                calls.set(calls.get() + 1);
                if calls.get() == 10 {
                    let _ = cancellation.cancel();
                }
                true
            },
            |_command, _fuzzy| None,
            SearchResultItem::Command,
            "",
            1,
            &cancellation,
        );
        assert!(matches!(outcome, PaletteSearchOutcome::Cancelled { .. }));
        assert!(
            calls.get() <= 1 + PALETTE_CANCEL_CHECK_INTERVAL,
            "the count stops within one check interval"
        );
    }

    #[test]
    fn the_category_candidate_charge_is_36_bytes_on_x86_64() {
        #[cfg(target_arch = "x86_64")]
        assert_eq!(PALETTE_CATEGORY_CANDIDATE_BYTES, 36);
        assert_eq!(std::mem::size_of::<SearchResultItem<'static>>(), 16);
    }

    #[test]
    fn extreme_limit_reserves_only_the_available_source_bound() {
        let cancellation = PaletteSearchCancellation::default();
        let outcome = search_items_cancellable(
            COMMANDS.iter(),
            |_| true,
            |command, fuzzy| fuzzy.score(command.label),
            SearchResultItem::Command,
            "same",
            usize::MAX,
            &cancellation,
        );
        let PaletteSearchOutcome::Complete { value, metrics } = outcome else {
            panic!("fresh search should complete");
        };
        assert_eq!(value.len(), COMMANDS.len());
        assert_eq!(metrics.peak_retained_per_source, COMMANDS.len());
    }
}
