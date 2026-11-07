// SPDX-License-Identifier: GPL-3.0-or-later

//! Single-flight execution of the palette's query stage order.
//!
//! Named `query_execution` rather than `execution` because the palette owns two
//! ordered stage orders and both need an `execution` module; the stage-order
//! qualifier keeps each role name accurate without widening the bounded set (see
//! `openspec/specs/gtk-adapter-module-boundaries/spec.md`).
//!
//! This module owns the query flight: it snapshots the query plus its four
//! shared sources into one compact request, submits that request to the
//! one-active/one-latest coordinator, spawns the fuzzy-search worker (the
//! grouped search, or the category search when a category view is shown,
//! `drill-down-palette-categories` D4), and arbitrates the completion: it
//! publishes only when the generation is current **and** the request's
//! category is still the one shown.
//!
//! Control inversion: [`LushtextCommandPalette::start_query_flight`] returns as
//! soon as the worker is spawned. Control resumes in the `spawn_blocking_then`
//! completion closure below, which asks the coordinator whether this generation
//! is still current before publishing anything. A completion that finds a
//! retained latest request starts that request **from inside the completion**
//! rather than returning to the facade, so the chain can run several
//! generations deep without the facade being re-entered.

use std::sync::Arc;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;

use crate::model::palette::{
    PaletteCategory, PaletteFileEntry, PaletteNoteEntry, PaletteSearchRow, SearchMode,
};
use crate::services::palette::{
    self, CategoryRanking, CategorySearchInput, FileIndex, GroupedSearchInput,
    PaletteSearchCancellation, PaletteSearchOutcome, category_search, grouped_search,
};
use crate::ui::plain_disposal::DisposalOwned;

use super::LushtextCommandPalette;
use super::policy;

/// One compact query plus the shared source snapshots retained by the latest slot.
///
/// Every field is an `Arc`, so a superseded request keeps sources alive without
/// copying an index, a note body, or a tab list.
pub(super) struct PaletteQueryRequest {
    pub query: Arc<str>,
    pub mode: SearchMode,
    pub index: Arc<DisposalOwned<FileIndex>>,
    pub open_tabs: Arc<[PaletteFileEntry]>,
    pub note_entries: Arc<DisposalOwned<Box<[PaletteNoteEntry]>>>,
    pub workspace_group_label: Arc<str>,
    /// The category view shown when the request was captured, `None` for the
    /// grouped view.
    pub category: Option<PaletteCategory>,
}

/// What one flight computed: grouped rows, or one category ranked whole.
pub(super) enum PaletteQueryResult {
    Grouped(Vec<PaletteSearchRow>),
    Category(CategoryRanking),
}

/// Run one palette search on the worker thread.
fn search_palette_sources(
    request: &PaletteQueryRequest,
    cancellation: &PaletteSearchCancellation,
) -> PaletteSearchOutcome<PaletteQueryResult> {
    #[cfg(feature = "test-utils")]
    super::test_policy::delay_search_worker();
    if let Some(category) = request.category {
        return category_search(
            CategorySearchInput {
                index: request.index.as_ref(),
                open_tabs: &request.open_tabs,
                note_entries: request.note_entries.as_ref(),
                category,
                query: &request.query,
                row_limit: category_row_limit(),
            },
            cancellation,
        )
        .map(PaletteQueryResult::Category);
    }
    grouped_search(
        GroupedSearchInput {
            index: request.index.as_ref(),
            open_tabs: &request.open_tabs,
            note_entries: request.note_entries.as_ref(),
            workspace_group_label: &request.workspace_group_label,
            query: &request.query,
            mode: request.mode,
            max_per_source: max_results_per_source(),
        },
        cancellation,
    )
    .map(PaletteQueryResult::Grouped)
}

/// The category view's row limit: the retained-byte budget's, or a test
/// override.
fn category_row_limit() -> usize {
    #[cfg(feature = "test-utils")]
    if let Some(limit) = super::test_policy::category_row_limit_override() {
        return limit;
    }
    policy::category_row_limit()
}

/// The grouped view's per-group cap, or a test override that forces "More"
/// rows on small sources.
fn max_results_per_source() -> usize {
    #[cfg(feature = "test-utils")]
    if let Some(limit) = super::test_policy::results_per_source_override() {
        return limit;
    }
    policy::MAX_RESULTS_PER_SOURCE
}

impl LushtextCommandPalette {
    /// Query stages 1 and 2: capture the query with its sources and admit one flight.
    ///
    /// Advances the search debounce so a pending debounced query from an earlier
    /// keystroke cannot re-fire behind this one. The coordinator either starts
    /// the request or retains it as the single replaceable latest request.
    pub(super) fn start_query_flight(&self, query: String) {
        let imp = self.imp();
        let _ = imp.search_debounce.advance();
        let request = PaletteQueryRequest {
            query: Arc::from(query),
            mode: imp.mode.get(),
            index: Arc::clone(&imp.file_index.borrow()),
            open_tabs: Arc::clone(&imp.open_tabs.borrow()),
            note_entries: Arc::clone(&imp.note_entries.borrow()),
            workspace_group_label: Arc::from(imp.workspace_group_label.borrow().as_str()),
            category: imp.category.get(),
        };
        let start = imp.search_flight.borrow_mut().submit(request);
        if let Some(start) = start {
            self.dispatch_query_worker(start);
        }
        self.refresh_searching_state();
    }

    /// Query stage 3: hand one admitted request to the worker lane.
    fn dispatch_query_worker(&self, start: palette::PaletteSearchStart<PaletteQueryRequest>) {
        let generation = start.generation;
        let cancellation = start.cancellation;
        let request = start.request;
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || {
                let outcome = search_palette_sources(&request, &cancellation);
                (outcome, request)
            },
            move |palette, (outcome, request)| {
                palette.settle_query_flight(generation, outcome, &request);
            },
        );
    }

    /// Query stage 4: arbitrate one worker completion, publish, and chain the latest request.
    ///
    /// This is where control resumes after the worker inversion. The
    /// coordinator's `is_current` is the query seam's freshness check — it owns
    /// the generation, so no separate ticket value exists on this side. The
    /// request's category is checked beside it: a category entered or left
    /// while the worker ran publishes nothing.
    fn settle_query_flight(
        &self,
        generation: u64,
        outcome: PaletteSearchOutcome<PaletteQueryResult>,
        request: &PaletteQueryRequest,
    ) {
        let imp = self.imp();
        let (is_current, next) = {
            let mut flight = imp.search_flight.borrow_mut();
            let is_current = flight.is_current(generation);
            let next = flight.finish(generation);
            (is_current, next)
        };

        let is_current = is_current && request.category == imp.category.get();
        match outcome {
            PaletteSearchOutcome::Complete {
                value: PaletteQueryResult::Grouped(rows),
                ..
            } if is_current => {
                imp.publish_search_rows(rows, &request.query);
            }
            PaletteSearchOutcome::Complete {
                value: PaletteQueryResult::Category(ranking),
                ..
            } if is_current => {
                imp.publish_category_ranking(ranking, request);
            }
            PaletteSearchOutcome::Cancelled { metrics } => {
                #[cfg(feature = "test-utils")]
                {
                    imp.observed_search_cancellations
                        .set(imp.observed_search_cancellations.get().saturating_add(1));
                    imp.last_cancelled_search_examined
                        .set(metrics.candidates_examined);
                }
                #[cfg(not(feature = "test-utils"))]
                let _ = metrics;
            }
            // A stale completion publishes nothing.
            PaletteSearchOutcome::Complete { .. } => {}
        }

        if let Some(next) = next {
            self.dispatch_query_worker(next);
        }
        self.refresh_searching_state();
    }

    /// Actuation seam: restart the query flight with an explicit query text.
    ///
    /// Widget tests need to drive query stage 1 with a query that is *not* the
    /// search entry's text, which no production entry point does — the public
    /// `set_query` writes the entry, and writing it makes `GtkSearchEntry` run
    /// its own `search-changed` timer and start a second flight. This is
    /// therefore an actuation seam in the programme's retained/deferred
    /// category, not an inspection accessor: see
    /// `docs/next/workflow-readability.md`, "Actuation test seams".
    #[cfg(feature = "test-utils")]
    pub fn restart_query_for_test(&self, query: &str) {
        self.start_query_flight(query.to_string());
    }

    /// Discard visible query intent and cancel the active generation.
    pub(super) fn abandon_query_flight(&self) {
        let imp = self.imp();
        let _ = imp.search_debounce.advance();
        imp.search_flight.borrow_mut().invalidate();
        imp.searching.set(false);
    }

    /// Recompute whether the visible palette still owns query work.
    pub(super) fn refresh_searching_state(&self) {
        let imp = self.imp();
        let searching = imp.palette_open.get() && imp.search_flight.borrow().has_work();
        imp.searching.set(searching);
        imp.refresh_accessibility_state();
    }
}
