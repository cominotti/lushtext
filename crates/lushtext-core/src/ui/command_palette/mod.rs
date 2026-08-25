// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette: the workflow facade for palette search and file indexing.
//!
//! Opened with Ctrl+Shift+P (`win.toggle-command-palette`), this floating
//! overlay searches open tabs, workspace files, notes, and commands. This module is the workflow's narrative facade:
//! it names the ordered stages of both of the workflow's stage orders and
//! delegates each one. It owns no timers, generation counters, ledgers, or
//! admission bookkeeping. Apart from installing the sources the window shell
//! hands it and the trivial reads and writes of the visible query controls that
//! make up the palette's entry-point surface, widget mutation belongs to the
//! coordination and adapter roles.
//!
//! # Stage order: query search
//!
//! 1. **Capture the query with its sources.** Opening the palette
//!    ([`LushtextCommandPalette::open`]), a mode change
//!    ([`LushtextCommandPalette::set_search_mode`], Tab cycling in `imp`),
//!    [`LushtextCommandPalette::set_query`], a source
//!    replacement, or a search-entry edit snapshots the query text, the mode,
//!    and all four shared sources into one compact `PaletteQueryRequest`
//!    (`query_execution::start_query_flight`). Typed input is debounced in
//!    `imp`; an empty query bypasses the debounce.
//! 2. **Admit one flight.** The one-active/one-latest coordinator either starts
//!    the request or keeps it as the single replaceable latest request.
//! 3. **Score on a worker.** `query_execution` hands the request to
//!    `grouped_search` with the per-source result cap.
//! 4. **Publish rows.** One `splice` replaces the whole model, the first
//!    activatable row is auto-selected, and the accessible projection refreshes
//!    (`imp::publish_search_rows`).
//!
//! Three inversions connect those stages:
//!
//! - Typed input returns after arming a 150 ms debounce. Control resumes in the
//!   debounce callback in `imp`, which re-enters stage 1.
//! - Stage 3 returns as soon as the worker is spawned. Control resumes in the
//!   worker completion closure in `query_execution`, which asks the coordinator
//!   whether that generation is still current before publishing anything.
//! - A completion that finds a retained latest request starts it **from inside
//!   the completion** rather than returning to this facade, so the chain can run
//!   several generations deep without the facade being re-entered.
//!
//! # Stage order: incremental file-index mutation
//!
//! This half has no visible surface. It keeps the palette's index consistent
//! with the filesystem after sidebar operations and watcher reconciliation.
//!
//! 1. **Retain the mutation.** [`LushtextCommandPalette::update_index_file_created`],
//!    [`LushtextCommandPalette::update_index_file_deleted`], and
//!    [`LushtextCommandPalette::update_index_file_renamed`] hand one
//!    mutation to `index_admission::admit_index_update`, which retains it under
//!    a bounded count cap and an exact retained-byte cap. Overflow escalates the
//!    queue to a full filesystem rebuild rather than dropping anything.
//! 2. **Coalesce the burst.** A 75 ms debounce collapses a burst of mutations
//!    into one flush turn.
//! 3. **Reserve replacement capacity.** `index_admission::flush_index_updates`
//!    reserves the replacement index's byte weight from the disposal budget.
//! 4. **Build and dispatch the batch.** `index_execution` takes the queue,
//!    selects the batch kind, and captures the batch's identity once as a
//!    `policy::FileIndexMutationTicket`.
//! 5. **Mutate on a worker.** The worker clones and mutates the index under its
//!    mutation ledger, or rebuilds from the workspace folders.
//! 6. **Arbitrate the applied batch.** The ticket is validated against live
//!    `policy::FileIndexMutationFacts`. A current batch is installed and the
//!    generation advances; a stale batch is rejected and replayed.
//! 7. **Retire the released index.** `retirement` classifies whether a
//!    last-owned at-cap index reached the bounded worker lane; the disposal lane
//!    performs the destruction itself.
//! 8. **Drain the tail.** A queue that refilled while the worker ran gets one
//!    more flush turn, re-entering stage 3.
//!
//! Five inversions connect those stages:
//!
//! - Stage 2's debounce resumes in its callback in `index_admission`, re-entering
//!   stage 3.
//! - When stage 3's reservation is refused, the flush arms the
//!   disposal-capacity wakeup and returns. Control resumes in that wakeup when
//!   the capacity epoch changes, re-entering the same flush. A refusal delays a
//!   mutation; it never loses one.
//! - Stage 5 returns as soon as the worker is spawned. Control resumes in the
//!   worker completion closure in `index_execution`, holding the ticket.
//! - A rejected batch in stage 6 re-arms the flush debounce rather than calling
//!   the worker, so the lost mutations are replayed through a rebuild. Control
//!   resumes in `index_admission` again.
//! - Stage 8 is armed from that same completion rather than called: the tail
//!   flush turn resumes in the flush debounce callback in `index_admission`,
//!   which is why a refilled queue never runs a second worker concurrently.
//!
//! A **full** index replacement (`set_guarded_file_index`, driven by workspace
//! folder changes rather than by file operations) is not part of that queue. It
//! installs a new index directly through
//! `index_execution::install_replacement_file_index`, advancing the same
//! generation counter, which is exactly what makes stage 6's arbitration
//! necessary.
//!
//! # Roles
//!
//! | Role | Module |
//! | --- | --- |
//! | facade | this module |
//! | pure policy | `policy` |
//! | coordination | `query_execution` (query flight), `index_admission` (bounded mutation queue), `index_execution` (mutation worker), `retirement` (index retirement accounting) |
//! | evidence | `evidence` |
//! | adapter detail | `imp`, `item` |
//!
//! See `docs/workflow-readability-matrix.md`, row `WFR-COMMAND-PALETTE`.

mod evidence;
// Private implementation module required by gtk-rs: imp.rs owns template
// children, state, and trait impls; this file exposes the public widget API.
mod imp;
mod index_admission;
mod index_execution;
pub mod item;
// Public because the GTK-free policy benchmarks and the widget harness address
// these pure types directly; nothing else outside this workflow does.
pub mod policy;
mod query_execution;
mod retirement;
#[cfg(feature = "test-utils")]
mod test_policy;

// Internal typed evidence surface: `evidence()` is callable in-crate by
// `ui/automation.rs`, and only the external widget harness needs to name the
// type. Re-exporting it unconditionally would widen this crate's default public
// API for an internal readability goal.
#[cfg(feature = "test-utils")]
pub use evidence::CommandPaletteEvidence;
#[cfg(feature = "test-utils")]
pub use retirement::{FileIndexRetirementSnapshot, file_index_retirement_snapshot_for_test};
#[cfg(feature = "test-utils")]
pub use test_policy::CommandPaletteTestPolicy;

use crate::model::palette::{PaletteFileEntry, PaletteNoteEntry, SearchMode};
use crate::services::palette::FileIndex;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use item::PaletteItem;
use std::path::Path;

use self::policy::FileIndexUpdate;

// glib::wrapper! generates the public wrapper type for this widget.
// @extends declares the GTK class hierarchy; @implements lists interfaces.
glib::wrapper! {
    /// Floating command/search widget owned by the window shell.
    ///
    /// The widget stays on the GTK main thread; expensive indexing and fuzzy
    /// matching live in the GTK-free palette service.
    pub struct LushtextCommandPalette(ObjectSubclass<imp::LushtextCommandPalette>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextCommandPalette {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Open the palette: reset to All mode, clear the query, show initial results.
    ///
    /// Query stage 1 for the empty query, which bypasses the input debounce.
    pub fn open(&self) {
        let imp = self.imp();
        imp.palette_open.set(true);
        imp.set_mode(SearchMode::All);
        imp.search_entry.set_text("");
        self.start_query_flight(String::new());
        imp.search_entry.grab_focus();
    }

    /// Close the palette: abandon query intent and clear the visible surface.
    ///
    /// `query_execution::abandon_query_flight` cancels the active generation and
    /// discards the retained latest request. The active worker still drains on
    /// its own; it publishes nothing, because its generation is no longer
    /// current.
    pub fn close(&self) {
        let imp = self.imp();
        imp.palette_open.set(false);
        self.abandon_query_flight();
        imp.search_entry.set_text("");
        imp.results_store.remove_all();
        imp.no_results_label.set_visible(false);
        imp.refresh_accessibility_state();
    }

    /// Set the visible search mode and restart the query through the normal path.
    pub fn set_search_mode(&self, mode: SearchMode) {
        let imp = self.imp();
        imp.set_mode(mode);
        self.start_query_flight(imp.search_entry.text().to_string());
        imp.search_entry.grab_focus();
    }

    /// Set the visible query text and restart the query through the normal path.
    pub fn set_query(&self, query: &str) {
        let imp = self.imp();
        if imp.search_entry.text().as_str() != query {
            imp.search_entry.set_text(query);
        }
        self.start_query_flight(query.to_string());
        imp.search_entry.grab_focus();
    }

    /// Replace the whole file index. Called when workspace folders change.
    ///
    /// This is the full-replacement path, not the incremental queue: it installs
    /// the index directly and advances the generation that the incremental
    /// stage order's stage-6 arbitration compares against.
    pub(crate) fn set_guarded_file_index(
        &self,
        index: crate::ui::plain_disposal::DisposalOwned<FileIndex>,
    ) {
        self.install_replacement_file_index(index);
    }

    /// Install an in-memory index through the widget-test compatibility surface.
    ///
    /// # Panics
    ///
    /// Panics when the test process has deliberately saturated disposal admission.
    #[cfg(feature = "test-utils")]
    pub fn set_file_index(&self, index: FileIndex) {
        let weight = index.retained_byte_weight();
        let index = crate::ui::plain_disposal::try_own_for_gtk(weight, index)
            .expect("widget-test file index should fit disposal admission");
        self.set_guarded_file_index(index);
    }

    /// Replace the open file-backed tab source used by grouped file results.
    pub fn set_open_tabs(&self, open_tabs: Vec<PaletteFileEntry>) {
        self.imp().install_open_tabs(open_tabs);
    }

    /// Replace the cached note rows used by Notes and All mode.
    pub(crate) fn set_guarded_note_entries(
        &self,
        note_entries: crate::ui::plain_disposal::DisposalOwned<Box<[PaletteNoteEntry]>>,
    ) {
        self.imp().install_note_entries(note_entries);
    }

    /// Install in-memory note rows through the widget-test compatibility surface.
    ///
    /// # Panics
    ///
    /// Panics when the test process has deliberately saturated disposal admission.
    #[cfg(feature = "test-utils")]
    pub fn set_note_entries(&self, note_entries: Vec<PaletteNoteEntry>) {
        let entries = note_entries.into_boxed_slice();
        let retained = crate::model::palette::palette_note_entries_retained_byte_weight(&entries);
        let entries = crate::ui::plain_disposal::try_own_for_gtk(retained, entries)
            .expect("widget-test note rows should fit disposal admission");
        self.set_guarded_note_entries(entries);
    }

    /// Drop the cached note rows without replacing them.
    pub(crate) fn clear_note_entries(&self) {
        self.set_guarded_note_entries(crate::ui::plain_disposal::DisposalOwned::small_unreserved(
            Vec::<PaletteNoteEntry>::new().into_boxed_slice(),
        ));
    }

    /// Set the label for the workspace-indexed file group.
    pub fn set_workspace_group_label(&self, label: impl Into<String>) {
        self.imp().install_workspace_group_label(label.into());
    }

    /// Refresh all source metadata that is owned by the window shell.
    pub fn set_sources(&self, open_tabs: Vec<PaletteFileEntry>, workspace_group_label: &str) {
        self.imp().install_sources(open_tabs, workspace_group_label);
    }

    /// Register a callback for when an item is activated (Enter or click).
    pub fn connect_item_activated<F: Fn(&PaletteItem) + 'static>(&self, f: F) {
        *self.imp().activate_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Register a callback for when the palette should close (Escape).
    pub fn connect_close_requested<F: Fn() + 'static>(&self, f: F) {
        *self.imp().close_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Index stage 1: add a newly created file to the search index.
    pub fn update_index_file_created(&self, path: &Path) {
        let folder = self.imp().file_index.borrow().workspace_folder_for(path);
        if let Some(workspace_folder) = folder {
            self.admit_index_update(FileIndexUpdate::Create {
                path: path.to_path_buf(),
                workspace_folder,
            });
        }
    }

    /// Index stage 1: remove a deleted file, or every file under a directory.
    pub fn update_index_file_deleted(&self, path: &Path) {
        self.admit_index_update(FileIndexUpdate::Delete(path.to_path_buf()));
    }

    /// Index stage 1: update a renamed file, or a renamed directory prefix.
    pub fn update_index_file_renamed(&self, old_path: &Path, new_path: &Path) {
        self.admit_index_update(FileIndexUpdate::Rename {
            old_path: old_path.to_path_buf(),
            new_path: new_path.to_path_buf(),
        });
    }

    /// Test seam for forcing accessibility projection after mutating adapter state.
    #[cfg(feature = "test-utils")]
    pub fn refresh_accessibility_state_for_test(&self) {
        self.imp().refresh_accessibility_state();
    }
}

impl Default for LushtextCommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

/// Test seam for asserting the palette row metadata used by the list factory.
#[cfg(feature = "test-utils")]
pub fn apply_palette_row_accessibility_for_test(
    row: &gtk4::Box,
    item: &PaletteItem,
    selected: bool,
    position: i32,
    set_size: i32,
) {
    imp::apply_palette_row_accessibility(row, item, selected, position, set_size);
}
