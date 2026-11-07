// SPDX-License-Identifier: GPL-3.0-or-later

//! GTK-free ranking of one palette category whole, for the category view
//! (`drill-down-palette-categories` D3).
//!
//! The search runs the **same** source call the grouped search runs for that
//! category — same selector, same comparator (descending score, then source
//! ordinal), same filters and the same open-tab exclusion — with a row limit
//! instead of the grouped view's 64. The comparator is a total order, so a
//! bounded heap of any size at least 64 yields the grouped rows as its first
//! 64: the category view begins with exactly what the user just saw.
//!
//! The result is source **ordinals** (`u32`), not rows: the GTK model builds a
//! row object only for a position the toolkit asks for. The row limit comes
//! from the caller's retained-byte budget through
//! `model::list_budget::rows_within` at [`PALETTE_CATEGORY_CANDIDATE_BYTES`] a
//! row, the selector's peak per ranked row.

use std::sync::Arc;

use crate::model::palette::{PaletteCategory, PaletteFileEntry, PaletteNoteEntry, ScoredResult};

use super::commands::{search_commands_cancellable, search_open_files_cancellable};
pub use super::fuzzy::PALETTE_CATEGORY_CANDIDATE_BYTES;
use super::grouped::open_tab_canonical_paths;
use super::index::FileIndex;
use super::notes::search_note_entries_cancellable;
use super::runtime::{PaletteSearchCancellation, PaletteSearchOutcome};

/// Borrowed sources and policy for one category ranking.
#[derive(Clone, Copy)]
pub struct CategorySearchInput<'a> {
    /// Shared workspace file index snapshot.
    pub index: &'a FileIndex,
    /// Open file-backed tabs in visible source order.
    pub open_tabs: &'a [PaletteFileEntry],
    /// Note and bookmark rows in source order.
    pub note_entries: &'a [PaletteNoteEntry],
    /// The category ranked whole.
    pub category: PaletteCategory,
    /// Current fuzzy query.
    pub query: &'a str,
    /// Most rows the ranking may keep (the retained-byte budget's row count).
    pub row_limit: usize,
}

/// One category ranked whole: source ordinals in rank order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CategoryRanking {
    /// Ordinals into the category's source, best first. For workspace files
    /// they index `FileIndex::files`, for open tabs the open-tab slice, for a
    /// note category the whole note slice, for commands `all_commands()`.
    pub ordinals: Arc<[u32]>,
    /// Every item the category matched, listed or not.
    pub matching: usize,
    /// Whether the row limit cut the ranking (`matching > ordinals.len()`).
    pub limited: bool,
}

/// Rank every match of one category, bounded by `row_limit`.
#[must_use]
pub fn category_search(
    input: CategorySearchInput<'_>,
    cancellation: &PaletteSearchCancellation,
) -> PaletteSearchOutcome<CategoryRanking> {
    let CategorySearchInput {
        index,
        open_tabs,
        note_entries,
        category,
        query,
        row_limit,
    } = input;
    let outcome = match category {
        PaletteCategory::OpenTabs => {
            search_open_files_cancellable(open_tabs, query, row_limit, cancellation)
        }
        PaletteCategory::WorkspaceFiles => {
            let excluded = open_tab_canonical_paths(open_tabs);
            return ranking_from(index.search_cancellable_excluding(
                query,
                row_limit,
                &excluded,
                cancellation,
            ));
        }
        PaletteCategory::Note(note_category) => search_note_entries_cancellable(
            note_entries,
            Some(note_category),
            query,
            row_limit,
            cancellation,
        ),
        PaletteCategory::Commands => {
            return ranking_from(search_commands_cancellable(query, row_limit, cancellation));
        }
    };
    ranking_from(outcome)
}

/// Turn ranked results into their source ordinals. The ranked results are
/// dropped once their ordinals exist: both coexist only during this copy,
/// which is the peak [`PALETTE_CATEGORY_CANDIDATE_BYTES`] charges.
fn ranking_from(
    outcome: PaletteSearchOutcome<Vec<ScoredResult<'_>>>,
) -> PaletteSearchOutcome<CategoryRanking> {
    match outcome {
        PaletteSearchOutcome::Complete { value, metrics } => {
            let ordinals: Arc<[u32]> = value
                .iter()
                .map(|result| u32::try_from(result.source_ordinal).unwrap_or(u32::MAX))
                .collect();
            drop(value);
            let matching = metrics.matching_candidates.max(ordinals.len());
            PaletteSearchOutcome::Complete {
                value: CategoryRanking {
                    limited: matching > ordinals.len(),
                    ordinals,
                    matching,
                },
                metrics,
            }
        }
        PaletteSearchOutcome::Cancelled { metrics } => PaletteSearchOutcome::Cancelled { metrics },
    }
}
