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
//! one-active/one-latest coordinator, spawns the grouped fuzzy-search worker,
//! and arbitrates the completion.
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

use crate::model::palette::{PaletteFileEntry, PaletteNoteEntry, PaletteSearchRow, SearchMode};
use crate::services::palette::{
    self, FileIndex, GroupedSearchInput, PaletteSearchCancellation, PaletteSearchOutcome,
    grouped_search,
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
}

/// Run one grouped palette search on the worker thread.
fn search_palette_sources(
    request: &PaletteQueryRequest,
    cancellation: &PaletteSearchCancellation,
) -> PaletteSearchOutcome<Vec<PaletteSearchRow>> {
    #[cfg(feature = "test-utils")]
    super::test_policy::delay_search_worker();
    grouped_search(
        GroupedSearchInput {
            index: request.index.as_ref(),
            open_tabs: &request.open_tabs,
            note_entries: request.note_entries.as_ref(),
            workspace_group_label: &request.workspace_group_label,
            query: &request.query,
            mode: request.mode,
            max_per_source: policy::MAX_RESULTS_PER_SOURCE,
        },
        cancellation,
    )
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
                (outcome, request.query)
            },
            move |palette, (outcome, query)| {
                palette.settle_query_flight(generation, outcome, &query);
            },
        );
    }

    /// Query stage 4: arbitrate one worker completion, publish, and chain the latest request.
    ///
    /// This is where control resumes after the worker inversion. The
    /// coordinator's `is_current` is the query seam's freshness check — it owns
    /// the generation, so no separate ticket value exists on this side.
    fn settle_query_flight(
        &self,
        generation: u64,
        outcome: PaletteSearchOutcome<Vec<PaletteSearchRow>>,
        query: &str,
    ) {
        let imp = self.imp();
        let (is_current, next) = {
            let mut flight = imp.search_flight.borrow_mut();
            let is_current = flight.is_current(generation);
            let next = flight.finish(generation);
            (is_current, next)
        };

        match outcome {
            PaletteSearchOutcome::Complete { value, .. } if is_current => {
                imp.publish_search_rows(value, query);
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
