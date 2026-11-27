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

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_settle::Debounce;
use gtk_lush_tasks::spawn_blocking_then;

use crate::ui::editor_page::LushtextEditorPage;
use lushtext_model::persistence::recent_document::RecentDocumentEntry;
use lushtext_model::values::RecentDocumentsGeneration;
use lushtext_services::persistence::json_store;
use lushtext_services::persistence::recent_documents;

use super::LushtextWindow;

/// Startup load lifecycle for the recent-documents journal.
#[derive(Debug, Default)]
pub(crate) enum RecentDocumentsLoadPhase {
    #[default]
    Idle,
    /// Startup recent-document loading is in flight; tracks paths removed meanwhile.
    Loading { removed_while_loading: Vec<PathBuf> },
    /// Widget tests seeded recents before the async startup load returned.
    #[cfg(feature = "test-utils")]
    TestSeeded,
}

/// Debounced save lifecycle for the recent-documents journal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RecentDocumentsSavePhase {
    #[default]
    Idle,
    /// A recent-document save is running on a worker; `pending` owes another save after.
    Inflight { pending: bool },
}

/// Recent-document state owned by the window and projected into the Open popover.
#[derive(Default)]
pub struct RecentDocumentsState {
    /// Newest-first persisted recent-document entries.
    pub(crate) entries: RefCell<Vec<RecentDocumentEntry>>,
    /// Startup load phase (`Idle`, `Loading`, or `TestSeeded`).
    pub(crate) load: RefCell<RecentDocumentsLoadPhase>,
    /// Monotonic version advanced by in-memory user mutations.
    pub(crate) generation: Cell<RecentDocumentsGeneration>,
    /// Whether the popover projection should be rebuilt before the next popup.
    pub(crate) rows_dirty: Cell<bool>,
    /// Debounce for coalescing bursts of recent-document persistence writes.
    pub(crate) save_debounce: Debounce,
    /// Worker save lifecycle (`Idle` or `Inflight { pending }`).
    pub(crate) save: Cell<RecentDocumentsSavePhase>,
}

impl RecentDocumentsState {
    /// Whether the startup recent-document load is currently in flight.
    #[must_use]
    pub fn is_loading(&self) -> bool {
        matches!(
            *self.load.borrow(),
            RecentDocumentsLoadPhase::Loading { .. }
        )
    }

    /// Snapshot of paths removed while the startup load was in flight.
    #[must_use]
    pub fn removed_while_loading(&self) -> Vec<PathBuf> {
        match &*self.load.borrow() {
            RecentDocumentsLoadPhase::Loading {
                removed_while_loading,
            } => removed_while_loading.clone(),
            RecentDocumentsLoadPhase::Idle => Vec::new(),
            #[cfg(feature = "test-utils")]
            RecentDocumentsLoadPhase::TestSeeded => Vec::new(),
        }
    }

    /// Whether a worker save is currently in flight.
    #[must_use]
    pub fn is_save_inflight(&self) -> bool {
        matches!(self.save.get(), RecentDocumentsSavePhase::Inflight { .. })
    }

    /// Whether another worker save is owed once the active one completes.
    #[must_use]
    pub fn is_save_pending(&self) -> bool {
        matches!(
            self.save.get(),
            RecentDocumentsSavePhase::Inflight { pending: true }
        )
    }

    #[cfg(feature = "test-utils")]
    pub(super) fn is_test_seeded(&self) -> bool {
        matches!(*self.load.borrow(), RecentDocumentsLoadPhase::TestSeeded)
    }
}

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
        state.load.replace(RecentDocumentsLoadPhase::Loading {
            removed_while_loading: Vec::new(),
        });
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
                let removed_while_loading = {
                    let mut load = state.load.borrow_mut();
                    match std::mem::take(&mut *load) {
                        RecentDocumentsLoadPhase::Loading {
                            removed_while_loading,
                        } => removed_while_loading,
                        #[cfg(feature = "test-utils")]
                        RecentDocumentsLoadPhase::TestSeeded => {
                            *load = RecentDocumentsLoadPhase::TestSeeded;
                            return;
                        }
                        RecentDocumentsLoadPhase::Idle => Vec::new(),
                    }
                };
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
        if let RecentDocumentsLoadPhase::Loading {
            removed_while_loading,
        } = &mut *state.load.borrow_mut()
        {
            removed_while_loading.push(path.to_path_buf());
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
        state.generation.set(state.generation.get().next());
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
        if state.is_save_inflight() {
            state
                .save
                .set(RecentDocumentsSavePhase::Inflight { pending: true });
            return;
        }
        state
            .save
            .set(RecentDocumentsSavePhase::Inflight { pending: false });
        let data_dir = json_store::data_dir();
        let snapshot: Vec<RecentDocumentEntry> =
            self.imp().recent_documents.entries.borrow().clone();
        spawn_blocking_then(
            self.clone(),
            move || recent_documents::save(&data_dir, &snapshot).map_err(|error| error.to_string()),
            |window, result| {
                let state = &window.imp().recent_documents;
                let was_pending = matches!(
                    state.save.replace(RecentDocumentsSavePhase::Idle),
                    RecentDocumentsSavePhase::Inflight { pending: true }
                );
                if let Err(error) = result {
                    tracing::warn!("failed to save recent documents: {error}");
                }
                if was_pending {
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
        self.imp()
            .recent_documents
            .load
            .replace(RecentDocumentsLoadPhase::TestSeeded);
        self.imp().recent_documents.entries.replace(entries);
        self.refresh_open_popover_rows();
    }
}
