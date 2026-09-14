// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`journal`) for `WFR-RECENT-DOCUMENTS`.
//!
//! The workflow's canonical role home is `ui/open_popover/`; this module is the
//! nested half, and it owns exactly one job: the durable `recents.json` record
//! that a later stage of the same workflow reads back.
//!
//! * **Recover** it at startup on a worker, with stale-record cleanup — the
//!   service prunes entries whose files are gone.
//! * **Guard** the handoff with a generation counter. A user who removes or
//!   opens something while the load is in flight has moved the generation, so
//!   the completion **merges** instead of replacing, and paths removed during
//!   the load are subtracted from what came off disk. Replacing unconditionally
//!   would resurrect an entry the user had just removed.
//! * **Write** it back on a worker behind a debounce, with one save in flight at
//!   a time and at most one more owed.
//! * **Hand it back** by projecting rows through the window-side presentation
//!   surface in `recent_open.rs`.
//!
//! It is the `journal` role rather than `execution` because its whole contract
//! is the record's lifecycle across process restarts, not the running of a
//! stage; and it is not `retirement`, which destroys a payload the workflow has
//! finished with.
//!
//! Its observable state is read through
//! `ui::open_popover::evidence::recent_documents_journal_evidence`.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::model::recent_document::RecentDocumentEntry;
use crate::services::json_store;
use crate::services::recent_documents;
use crate::ui::editor_page::LushtextEditorPage;

use super::LushtextWindow;

/// Quiet window before a burst of recent-document mutations is written to disk.
///
/// A quarter second coalesces the rapid add/remove sequences a user produces by
/// opening several documents in a row, without letting an unsaved record
/// outlive a plausible crash window.
const RECENT_DOCUMENTS_SAVE_DEBOUNCE_MS: u64 = 250;

impl LushtextWindow {
    /// Stage 1: recover the durable record without blocking startup.
    pub(super) fn load_recent_documents_async(&self) {
        let state = &self.imp().recent_documents;
        state.loading.set(true);
        state.removed_while_loading.borrow_mut().clear();
        let load_generation = state.generation.get();
        spawn_blocking_then(
            self.clone(),
            move || {
                let data_dir = json_store::data_dir();
                let loaded = recent_documents::load(&data_dir);
                let should_save = loaded.pruned;
                (loaded, should_save)
            },
            move |window, (loaded, should_save)| {
                let state = &window.imp().recent_documents;
                state.loading.set(false);
                let removed_while_loading = state.removed_while_loading.take();
                #[cfg(feature = "test-utils")]
                if state.test_seeded.get() {
                    return;
                }
                for diagnostic in &loaded.diagnostics {
                    tracing::warn!("{diagnostic}");
                }
                let mut loaded_entries = loaded.entries;
                if !removed_while_loading.is_empty() {
                    loaded_entries.retain(|entry| {
                        !removed_while_loading
                            .iter()
                            .any(|path| entry.matches_path(path))
                    });
                }
                let changed_after_load_started = state.generation.get() != load_generation;
                let merged_loaded_rows = if changed_after_load_started {
                    let mut entries = state.entries.borrow_mut();
                    recent_documents::merge_loaded_entries(&mut entries, loaded_entries)
                } else {
                    state.entries.replace(loaded_entries);
                    true
                };
                window.refresh_open_popover_rows();
                if should_save || (changed_after_load_started && merged_loaded_rows) {
                    window.schedule_recent_documents_save();
                }
            },
        );
    }

    /// Record a successful explicit local file-backed open.
    pub(super) fn record_recent_open_for_editor(&self, editor: &LushtextEditorPage, path: &Path) {
        let canonical = editor.canonical_file_path();
        self.record_recent_path(path.to_path_buf(), canonical);
    }

    fn record_recent_path(&self, path: PathBuf, canonical_path: Option<PathBuf>) {
        {
            let mut entries = self.imp().recent_documents.entries.borrow_mut();
            recent_documents::add_or_update(
                &mut entries,
                path,
                canonical_path,
                recent_documents::now_secs(),
            );
        }
        self.refresh_open_popover_rows();
        self.mark_recent_documents_changed();
        self.schedule_recent_documents_save();
    }

    /// Forget one entry, recording it as removed if the startup load is still
    /// in flight so the completion cannot bring it back.
    pub(super) fn remove_recent_document(&self, path: &Path) {
        let state = &self.imp().recent_documents;
        if state.loading.get() {
            state
                .removed_while_loading
                .borrow_mut()
                .push(path.to_path_buf());
        }
        {
            let mut entries = state.entries.borrow_mut();
            recent_documents::remove(&mut entries, path);
        }
        self.refresh_open_popover_rows();
        self.mark_recent_documents_changed();
        self.schedule_recent_documents_save();
    }

    fn mark_recent_documents_changed(&self) {
        let state = &self.imp().recent_documents;
        state.generation.set(state.generation.get().wrapping_add(1));
    }

    fn schedule_recent_documents_save(&self) {
        self.imp().recent_documents.save_debounce.schedule(
            self,
            Duration::from_millis(RECENT_DOCUMENTS_SAVE_DEBOUNCE_MS),
            |window, _token| {
                window.start_recent_documents_save();
            },
        );
    }

    fn start_recent_documents_save(&self) {
        let state = &self.imp().recent_documents;
        if state.save_inflight.get() {
            state.save_pending.set(true);
            return;
        }
        state.save_inflight.set(true);
        let data_dir = json_store::data_dir();
        let snapshot: Vec<RecentDocumentEntry> =
            self.imp().recent_documents.entries.borrow().clone();
        spawn_blocking_then(
            self.clone(),
            move || recent_documents::save(&data_dir, &snapshot).map_err(|error| error.to_string()),
            |window, result| {
                let state = &window.imp().recent_documents;
                state.save_inflight.set(false);
                if let Err(error) = result {
                    tracing::warn!("failed to save recent documents: {error}");
                }
                if state.save_pending.replace(false) {
                    window.schedule_recent_documents_save();
                }
            },
        );
    }

    /// Forget one entry, as the popover's remove button does.
    ///
    /// An actuation seam. The production path is reached from the popover
    /// callback, which a test cannot emit without a realized row.
    #[cfg(feature = "test-utils")]
    pub fn remove_recent_document_for_test(&self, path: &Path) {
        self.remove_recent_document(path);
    }

    /// Apply a startup-load completion that arrived **after** a user mutation.
    ///
    /// An actuation seam for the merge branch of `load_recent_documents_async`:
    /// the generation moved while the worker was in flight, so the completion
    /// must merge rather than replace. Reaching it through the real worker would
    /// need a disk fixture plus a race, and the branch it drives is the one a
    /// stale generation guards.
    #[cfg(feature = "test-utils")]
    pub fn merge_loaded_recent_documents_for_test(&self, loaded: Vec<RecentDocumentEntry>) {
        let state = &self.imp().recent_documents;
        {
            let mut entries = state.entries.borrow_mut();
            recent_documents::merge_loaded_entries(&mut entries, loaded);
        }
        self.refresh_open_popover_rows();
    }

    /// Seed the journal for widget tests without disk I/O.
    ///
    /// An actuation seam: it drives the workflow into a state, and observing
    /// that state goes through the evidence surface instead.
    #[cfg(feature = "test-utils")]
    pub fn set_recent_documents_for_test(&self, entries: Vec<RecentDocumentEntry>) {
        self.imp().recent_documents.test_seeded.set(true);
        self.imp().recent_documents.entries.replace(entries);
        self.refresh_open_popover_rows();
    }
}
