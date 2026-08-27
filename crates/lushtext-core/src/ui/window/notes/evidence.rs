// SPDX-License-Identifier: GPL-3.0-or-later

//! The notes and bookmarks workflow's one typed evidence surface.
//!
//! [`LushtextWindow::notes_evidence`] is the single source of this workflow's
//! observable state. Widget tests read this value; they do not read per-field
//! inspection functions, and no new one may be added — when a test needs a fact
//! this surface does not expose, the surface gains a field.
//!
//! # Three constraints this surface is written against
//!
//! **No evidence field may be read from inside a mutable borrow of the state the
//! accessor reads.** Because one accessor reads the whole surface and this
//! workflow's state lives behind `RefCell`, a nested read panics at runtime
//! rather than failing to compile. Every derived scalar below is computed into a
//! local and every `Ref` is dropped *before* the struct literal is built.
//! `test_notes_evidence_reads_stay_side_effect_free_across_mutation` drives the
//! workflow through each operation that takes such a borrow and reads the surface
//! **after** each one.
//!
//! **A disposed widget is a stage.** GTK4 clears template children in
//! `dispose()`, before Rust's `Drop`, so `notes_menu_open` reads its
//! `TemplateChild` through `try_get()` and answers `false` honestly rather than
//! panicking during teardown.
//!
//! **Reading must not materialize toolkit state or advance a metric it
//! reports.** This surface walks the tab view, which is a fully materialized
//! `AdwTabView` rather than a lazily created collection, and it counts note-save
//! captures **without pruning them** — the retired
//! `note_save_snapshot_count_for_test` pruned as a side effect of being read,
//! which is precisely the observer-changes-the-observed hazard the convention
//! forbids.
//!
//! # Fields deliberately not sourced from this surface
//!
//! `active_document_file_backed` is **not** notes-workflow state: it is the
//! active document's identity, reported identically by the `notes` and
//! `local_history` automation objects. It appears here because the menu's
//! availability decisions consume it, but the automation snapshot derives it once
//! from the editor page for both objects rather than sourcing the same fact from
//! two evidence surfaces. See `evidence/automation-no-widening.md`.

use glib::subclass::prelude::ObjectSubclassIsExt;

use gtk4::prelude::*;

use crate::ui::buffer_snapshot::BufferSnapshotHandle;
use crate::ui::editor_page::LushtextEditorPage;

use super::LushtextWindow;
use super::browser::{ActiveNotesBrowser, NotesBrowserRuntimeSnapshot};
#[cfg(feature = "test-utils")]
use super::policy;

/// Everything observable about one window's notes and bookmarks workflow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NotesEvidence {
    /// Bounded source, query, and preview ownership for the visible browser
    /// dialog; `None` when no browser is presented.
    pub browser: Option<NotesBrowserRuntimeSnapshot>,
    /// Chunked note-save captures still in flight, counted without pruning.
    pub active_note_save_captures: usize,
    /// Whether the header Notes menu's popover is open.
    pub notes_menu_open: bool,
    /// Whether the active editor has a stable saved path.
    pub active_document_file_backed: bool,
    /// Bookmarks projected by the active editor's source marks.
    pub active_document_bookmark_count: usize,
    /// Whether the active editor's cursor line carries a bookmark.
    pub active_line_has_bookmark: bool,
    /// Whether the active editor's bookmark sidecar has been **successfully**
    /// read back, or successfully written, at least once.
    ///
    /// While this is `false`, an **empty** live bookmark set is not written:
    /// `bookmark_service::save_document` deletes the sidecar when the set is
    /// empty, so writing before the sidecar has been read would destroy it. The
    /// field is on the surface because the guard it reports is the only thing
    /// standing between a restored tab whose file was renamed in a previous
    /// session and the deletion of all its bookmarks, and a guard nothing can
    /// observe is a guard nothing can test.
    pub active_document_sidecar_resolved: bool,
    /// Whether `Open Document Note…` can start immediately.
    pub document_note_available: bool,
    /// Whether `Open Folder Note…` can start immediately.
    pub folder_note_available: bool,
    /// Whether the command palette's note source owns active or latest work.
    pub palette_note_source_busy: bool,
    /// Whether a compact palette note-source request is parked on admission.
    pub palette_note_source_awaiting_admission: bool,
}

/// Bounded live-editor capture evidence for one explicit budget.
///
/// This is a **probe**, not a field: it takes the caller's item and byte budgets
/// and runs the real bounded capture, so it cannot be a passive read of the
/// surface. It replaces two separate tuple-returning seams
/// (`open_editor_note_snapshot_counts_for_test` and
/// `open_editor_note_snapshot_retained_evidence_for_test`) with one named
/// workflow operation returning a named value, on slot 4's
/// `draft_delete_is_tombstoned(&str)` precedent that a question taking an
/// argument is an operation rather than a surface field.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenEditorNoteCaptureEvidence {
    /// Editor snapshots admitted under the supplied budgets.
    pub snapshots: usize,
    /// Bookmark rows admitted alongside them.
    pub bookmarks: usize,
    /// Retained bytes the capture accounted for.
    pub retained_bytes: u64,
    /// Whether the capture stopped early against either budget.
    pub truncated: bool,
}

impl LushtextWindow {
    /// Read the whole notes and bookmarks workflow surface.
    ///
    /// Takes only shared borrows and must not be called from inside a mutable
    /// borrow of the state it reads. Every scalar is computed first and every
    /// borrow is dropped before the struct literal below.
    #[must_use]
    pub fn notes_evidence(&self) -> NotesEvidence {
        let imp = self.imp();

        let browser = {
            let active = imp.active_notes_browser.borrow();
            let snapshot = active
                .as_ref()
                .and_then(ActiveNotesBrowser::runtime_snapshot);
            drop(active);
            snapshot
        };

        let active_note_save_captures = {
            let captures = imp.note_save_snapshots.borrow();
            // Counted, never pruned: pruning here would make the observation
            // change the thing observed.
            let count = captures
                .iter()
                .filter(|handle| BufferSnapshotHandle::is_active(handle))
                .count();
            drop(captures);
            count
        };

        // A disposed window has already cleared its template children.
        let notes_menu_open = imp
            .notes_menu_button
            .try_get()
            .is_some_and(|button| button.is_active());

        // Every `TemplateChild` read below goes through `try_get()`. A disposed
        // window has already cleared `tab_view` and `sidebar`, and the panicking
        // accessor would turn a teardown observation into a crash — which is
        // exactly what this surface's own disposal proof caught before it
        // shipped.
        let editor = imp
            .tab_view
            .try_get()
            .and_then(|tab_view| tab_view.selected_page())
            .and_then(|page| page.child().downcast::<LushtextEditorPage>().ok());
        let active_document_file_backed = editor
            .as_ref()
            .and_then(LushtextEditorPage::file_path)
            .is_some();
        let active_document_bookmark_count = editor
            .as_ref()
            .map_or(0, |editor| editor.bookmark_records().len());
        let active_line_has_bookmark = editor
            .as_ref()
            .is_some_and(|editor| editor.current_bookmark().is_some());
        let active_document_sidecar_resolved = editor
            .as_ref()
            .is_some_and(|editor| editor.imp().bookmarks.persistence.sidecar_resolved.get());
        drop(editor);

        let document_note_available = active_document_file_backed;
        // Kept byte-identical to the exported contract: any configured folder in
        // the current shared scope makes the action reachable, which is weaker
        // than `policy::folder_note_action_available` over a concrete workspace.
        let folder_note_available = imp
            .sidebar
            .try_get()
            .is_some_and(|sidebar| !sidebar.current_scope_folder_paths().is_empty());

        let (palette_note_source_busy, palette_note_source_awaiting_admission) = {
            let refreshes = imp.command_palette_note_refreshes.borrow();
            let busy = refreshes.has_work();
            drop(refreshes);
            let admission = imp.command_palette_note_admission.borrow();
            let parked = admission.is_some();
            drop(admission);
            (busy, parked)
        };

        NotesEvidence {
            browser,
            active_note_save_captures,
            notes_menu_open,
            active_document_file_backed,
            active_document_bookmark_count,
            active_line_has_bookmark,
            active_document_sidecar_resolved,
            document_note_available,
            folder_note_available,
            palette_note_source_busy,
            palette_note_source_awaiting_admission,
        }
    }

    /// Run one bounded live-editor note capture against explicit budgets.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn capture_open_editor_note_evidence(
        &self,
        item_limit: usize,
        retained_byte_limit: u64,
    ) -> OpenEditorNoteCaptureEvidence {
        let workspaces_file = self.imp().sidebar.workspaces_file();
        let scope_snapshot = workspaces_file.current_scope_snapshot();
        let captured = self.open_editor_note_snapshots_bounded(
            scope_snapshot.folder_paths(),
            &workspaces_file.workspaces,
            item_limit,
            retained_byte_limit,
        );
        OpenEditorNoteCaptureEvidence {
            snapshots: captured.entries.len(),
            bookmarks: captured
                .entries
                .iter()
                .map(|snapshot| snapshot.bookmarks.len())
                .sum(),
            retained_bytes: captured.retained_bytes,
            truncated: captured.truncated,
        }
    }

    /// Run the capture against the workflow's own production budgets.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn capture_open_editor_note_evidence_at_item_limit(
        &self,
        item_limit: usize,
    ) -> OpenEditorNoteCaptureEvidence {
        self.capture_open_editor_note_evidence(
            item_limit,
            policy::NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT,
        )
    }
}
