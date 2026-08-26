// SPDX-License-Identifier: GPL-3.0-or-later

//! The command-palette workflow's observable state, in one typed value.
//!
//! [`CommandPaletteEvidence`] is the single source of truth for observers of
//! this workflow. Widget tests read it instead of calling per-field `*_for_test`
//! getters, and the read-only D-Bus automation snapshot projects its documented
//! fields from it rather than re-deriving the same state from widget accessors.
//!
//! Reading evidence is pure observation: it never advances a generation, arms a
//! timer, drains the mutation queue, or requires the workflow to be in a
//! particular stage. The scalar accessors below are the primitives the surface
//! composes, and they stay here so the workflow's observation lives in one
//! place.
//!
//! Reentrancy constraint: [`LushtextCommandPalette::evidence`] takes shared
//! `RefCell` borrows of the search coordinator, the pending mutation queue, the
//! installed file index, the note source, and the open-tab source. It must
//! therefore be called from workflow code that is not already holding a
//! `borrow_mut()` on any of those cells, or the borrow would panic. Every
//! current caller observes from outside a mutation — widget tests and the
//! read-only automation snapshot — so no live path can reach that state.
//!
//! Reading evidence must not require the workflow to be in a particular stage,
//! and **a disposed widget is a stage**: a teardown test that disposes the
//! widget and then asks what the workflow recorded is a legitimate observation
//! point. GTK4 clears template children in `dispose()`, before Rust's `Drop`,
//! so every field derived from a `TemplateChild` here is read through
//! `try_get()` and gives an honest empty/false answer when the child is gone,
//! rather than panicking. This hazard is created by consolidation: scattered
//! per-field getters each read one narrow thing, while one surface makes every
//! field reachable from every observation point.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::palette::SearchMode;
use crate::services::palette::PaletteSearchCoordinatorSnapshot;

use super::LushtextCommandPalette;
use super::policy;

/// One consistent read of the command-palette workflow.
///
/// Field groups follow the workflow's two stage orders: the visible query
/// surface and its single-flight search, the palette's installed sources, and
/// the bounded incremental file-index mutation queue and its worker.
pub struct CommandPaletteEvidence {
    // --- query surface ---
    /// Current query text in the palette entry.
    pub query: String,
    /// Current search-mode filter.
    pub mode: SearchMode,
    /// Rows currently rendered by the results model, including section headers.
    pub result_count: u32,

    // --- query flight ---
    /// Whether the visible palette owns active or latest query work.
    pub searching: bool,
    /// Ownership and high-water evidence for the one-active/one-latest query coordinator.
    ///
    /// This *is* the query seam's value object: the coordinator owns the
    /// generation and exposes `is_current`, so no separate ticket type exists on
    /// this side of the workflow.
    pub search_flight: PaletteSearchCoordinatorSnapshot,
    /// Workers that cooperatively observed a superseding cancellation.
    ///
    /// Test-gated because it exists only so a widget test can prove cooperative
    /// cancellation happened at all; no production path reads it.
    #[cfg(feature = "test-utils")]
    pub observed_search_cancellations: usize,
    /// Candidate progress retained from the most recent cancelled worker.
    ///
    /// Test-gated for the same reason as
    /// [`CommandPaletteEvidence::observed_search_cancellations`].
    #[cfg(feature = "test-utils")]
    pub last_cancelled_search_examined: usize,

    // --- installed sources ---
    /// Files in the currently installed index.
    pub file_index_len: usize,
    /// Byte credit held by the installed guarded file index.
    pub file_index_reservation_weight: Option<u64>,
    /// Byte credit held by the installed guarded note source.
    pub note_source_reservation_weight: Option<u64>,
    /// Open file-backed tabs supplied by the window shell.
    pub open_tab_source_count: usize,

    // --- bounded file-index mutation ---
    /// Queued mutations plus any pending rebuild plus any active worker.
    ///
    /// This is the readiness-facing aggregate; the three components below are
    /// the parts it sums.
    pub pending_index_update_count: usize,
    /// Mutations currently retained by the bounded queue.
    pub queued_index_updates: usize,
    /// Exact bytes the bounded queue currently owns.
    pub queued_index_update_bytes: u64,
    /// Whether bounded overflow demoted the queue to a full filesystem rebuild.
    pub index_rebuild_pending: bool,
    /// Whether the serialized incremental index worker is active.
    pub index_update_worker_running: bool,
    /// Queue count ceiling before overflow escalates to a rebuild.
    pub max_queued_index_updates: usize,
    /// Queue byte ceiling before overflow escalates to a rebuild.
    pub max_queued_index_update_bytes: u64,
}

impl LushtextCommandPalette {
    /// Read this workflow's observable state as one consistent value.
    ///
    /// Every field the palette's retired `*_for_test` inspection functions
    /// exposed is readable here. Reading does not mutate workflow state.
    #[must_use]
    pub fn evidence(&self) -> CommandPaletteEvidence {
        let imp = self.imp();
        CommandPaletteEvidence {
            query: self.query(),
            mode: self.mode(),
            result_count: self.result_count(),

            searching: self.is_searching(),
            search_flight: imp.search_flight.borrow().snapshot(),
            #[cfg(feature = "test-utils")]
            observed_search_cancellations: imp.observed_search_cancellations.get(),
            #[cfg(feature = "test-utils")]
            last_cancelled_search_examined: imp.last_cancelled_search_examined.get(),

            file_index_len: self.file_index_len(),
            file_index_reservation_weight: self.file_index_reservation_weight(),
            note_source_reservation_weight: self.note_source_reservation_weight(),
            open_tab_source_count: self.open_tab_source_count(),

            pending_index_update_count: self.pending_index_update_count(),
            queued_index_updates: imp.pending_index_updates.borrow().len(),
            queued_index_update_bytes: imp.pending_index_update_bytes.get(),
            index_rebuild_pending: imp.index_update_rebuild_pending.get(),
            index_update_worker_running: imp.index_update_worker_running.get(),
            max_queued_index_updates: policy::MAX_PENDING_INDEX_UPDATES,
            max_queued_index_update_bytes: policy::MAX_PENDING_INDEX_UPDATE_BYTES,
        }
    }

    /// Current query text in the palette entry.
    ///
    /// Empty when the template child is gone, which is the disposed-widget case
    /// described in this module's reentrancy and observation notes.
    #[must_use]
    pub fn query(&self) -> String {
        self.imp()
            .search_entry
            .try_get()
            .map(|entry| entry.text().to_string())
            .unwrap_or_default()
    }

    /// The current search mode.
    #[must_use]
    pub fn mode(&self) -> SearchMode {
        self.imp().mode.get()
    }

    /// Number of rows currently rendered by the palette results model.
    #[must_use]
    pub fn result_count(&self) -> u32 {
        self.imp().results_store.n_items()
    }

    /// Whether the visible palette owns active or latest query work.
    #[must_use]
    pub fn is_searching(&self) -> bool {
        self.imp().searching.get()
    }

    /// Number of files in the current index (used as capacity hint for rebuilds).
    #[must_use]
    pub fn file_index_len(&self) -> usize {
        self.imp().file_index.borrow().len()
    }

    /// Byte credit held by the currently installed guarded file index.
    #[must_use]
    pub(crate) fn file_index_reservation_weight(&self) -> Option<u64> {
        self.imp().file_index.borrow().reservation_weight()
    }

    /// Byte credit held by the currently installed guarded note source.
    #[must_use]
    pub(crate) fn note_source_reservation_weight(&self) -> Option<u64> {
        self.imp().note_entries.borrow().reservation_weight()
    }

    /// Number of open file-backed tabs supplied by the window shell.
    #[must_use]
    pub fn open_tab_source_count(&self) -> usize {
        self.imp().open_tabs.borrow().len()
    }

    /// Queued mutations plus any pending rebuild plus any active worker.
    #[must_use]
    pub fn pending_index_update_count(&self) -> usize {
        let imp = self.imp();
        imp.pending_index_updates
            .borrow()
            .len()
            .saturating_add(usize::from(imp.index_update_rebuild_pending.get()))
            .saturating_add(usize::from(imp.index_update_worker_running.get()))
    }
}
