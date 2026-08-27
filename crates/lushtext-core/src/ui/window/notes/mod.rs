// SPDX-License-Identifier: GPL-3.0-or-later

//! Notes and bookmarks: the workflow facade.
//!
//! Bookmarks, document notes, and folder notes are **sidecars** that hang off
//! paths the user's documents live at. This module is the workflow's narrative
//! facade: it names the ordered stages and delegates every one. It owns no
//! timer, no admission bookkeeping, no generation counter, and no widget
//! mutation.
//!
//! # Stage order: editor note resolution
//!
//! 1. **A file finishes loading.** `bookmark_execution::wire_note_callbacks`
//!    connects the editor's `file_loaded`, `bookmarks_changed`,
//!    `bookmark_activated`, and cursor `mark_set` signals once per tab.
//! 2. **Load the sidecar.** `resolve_notes_for_editor` reads the bookmark
//!    document on a worker and installs it only if the editor still owns the same
//!    path and the same bookmark generation.
//! 3. **Republish.** The palette note source is refreshed and the status bar
//!    re-reads its metadata. Save As resets live state and re-enters stage 2.
//!
//! # Stage order: bookmark lifecycle
//!
//! 1. **Toggle or edit.** The Notes menu, `Ctrl+Alt+K`, or activating a source
//!    mark reaches `bookmark_execution`, whose edit dialog validates the typed
//!    line through `policy::parse_bookmark_target_line` and reports failures
//!    through `policy::bookmark_edit_error_message`.
//! 2. **Persist, debounced.** One burst of edits produces one sidecar write
//!    (`policy::NOTES_SAVE_DEBOUNCE_MS`). A write that fails re-arms the dirty
//!    flag and reschedules; a tab or window closing flushes synchronously.
//! 3. **Preview a closed file.** Selecting a bookmark row for a file with no open
//!    tab admits one excerpt load, validated at completion against a
//!    `seams::NotesBrowserTicket<PreviewFlight>`.
//!
//! # Stage order: note editors
//!
//! 1. **Resolve the target.** `targets` answers whether there is a saved active
//!    document, and `policy::folder_note_target_for_workspace` decides the
//!    zero/one/many folder case rather than falling back to the first folder.
//! 2. **Present the editor.** `editor_execution` builds the shared Edit/Render
//!    surface, pre-rendering the hidden Render page so the first mode switch is
//!    geometry-stable.
//! 3. **Save or discard.** Large bodies are captured in bounded chunks; the write
//!    runs on a worker and a failure re-presents the editor with the typed text
//!    rather than losing it.
//!
//! # Stage order: notes browser
//!
//! 1. **Present and begin a mode.** `browser` builds the dialog;
//!    `source_execution::begin_mode` advances the source and query generations
//!    that make the requested mode the only publishable one.
//! 2. **Build a bounded source.** `source_execution` captures live-editor
//!    metadata under `policy`'s budgets, reserves disposal capacity, and hands a
//!    compact request to the shared note-source coordinator.
//! 3. **Query it.** `query_execution` debounces typing and matches against the
//!    published source, publishing grouped rows or the mode's own empty copy.
//!
//! # Stage order: sidecar migration on rename
//!
//! 1. **A rename completes in the workspace tree.** `WFR-WORKSPACE-TREE` settles
//!    its own cache, watch-row, and expansion updates and then calls
//!    [`LushtextWindow::migrate_note_sidecars_after_rename`] through
//!    `ui/window/documents.rs`. That ordering guarantee is the tree's, not this
//!    workflow's.
//! 2. **Record before moving.** `journal` writes pending ledger state for all
//!    three sidecar kinds *before* the first move, then runs bookmarks, document
//!    notes, and folder notes in that fixed order.
//! 3. **Reconcile whatever is left.** `reconcile_pending_migrations_on_startup`
//!    finishes tracked kinds a previous run did not.
//!
//! # Inversions
//!
//! One line each; the point where control resumes is named.
//!
//! - **The longest-lived inversion in the codebase is stage 2→3 of the migration
//!   order: control leaves in `journal`'s worker and resumes in
//!   `reconcile_pending_migrations_on_startup` on a *later app launch*, bounded
//!   by the ledger's attempt cap.**
//! - Editor note resolution returns once its worker is dispatched and resumes in
//!   a `bookmark_execution` completion that re-checks path and bookmark
//!   generation.
//! - Bookmark persistence resumes after the debounce quiet window, and again in
//!   the write completion, which re-arms the dirty flag on failure.
//! - The closed-file excerpt load resumes in a `bookmark_execution` completion
//!   validated against a preview ticket, then starts the one retained request.
//! - Note editor save resumes twice: once in the chunked buffer capture's
//!   completion, once in the durable write's.
//! - The folder chooser and both note dialogs resume in
//!   `AdwAlertDialog::choose` after the dialog has already closed.
//! - Browser source construction resumes in the disposal-capacity wakeup when
//!   admission refuses, and in `source_execution`'s worker completion otherwise.
//! - Browser query resumes after the search debounce and in
//!   `query_execution`'s worker completion.
//! - The palette note-source refresh resumes after its own debounce window.
//!
//! # Roles
//!
//! | Role | Module |
//! | --- | --- |
//! | facade | this module |
//! | pure policy | `policy` |
//! | seam value objects | `seams` (`NotesBrowserTicket`, phantom-typed by flight) |
//! | coordination | `journal` (the migration ledger), `source_execution` (bounded source build for both the browser and the palette), `query_execution` (the browser query), `bookmark_execution` (bookmark lifecycle and editor note resolution), `editor_execution` (document and folder note editors) |
//! | evidence | `evidence` |
//! | called presentation surfaces, carrying **no** role | `browser` (dialog shell, session state, rows, preview projection), `menu` (the header Notes menu), `chrome` (dialog close/Escape/focus/empty-state helpers shared by three dialogs), `targets` (window-side target resolution) |
//! | test-only configuration | `test_policy` |
//!
//! # State this workflow shares with others
//!
//! | State | Shared with | How |
//! | --- | --- | --- |
//! | `services/palette/notes.rs` (≈1,840 shared production lines) | `WFR-COMMAND-PALETTE` | both call the same bounded source engine; `source_execution` serves both consumers |
//! | `command_palette_note_refreshes` on the window imp | `WFR-COMMAND-PALETTE` | this workflow refreshes it; the palette's `command-palette-index` readiness blocker reads it |
//! | `services/migration_ledger.rs` | `WFR-LOCAL-HISTORY` | `journal` records `MigrationKind`s the local-history journal also uses |
//! | `resolve_notes_for_editor` | `WFR-LOCAL-HISTORY` | called from two restore terminals in that migrated row |
//! | The startup call into `reconcile_pending_migrations_on_startup` | cross-cutting `ui/window/startup_data.rs` | that module is owned by neither row; it orders five workflows |
//! | `active_document_file_backed` | `WFR-LOCAL-HISTORY` | the same fact about the same active document; the automation snapshot derives it once for both objects |
//!
//! See `docs/workflow-readability-matrix.md`, row `WFR-NOTES-BOOKMARKS`.

mod bookmark_execution;
mod browser;
mod chrome;
mod editor_execution;
mod evidence;
mod journal;
mod menu;
pub(crate) mod policy;
mod query_execution;
mod seams;
mod source_execution;
mod targets;
#[cfg(feature = "test-utils")]
mod test_policy;

pub(in crate::ui::window) use browser::ActiveNotesBrowser;
// Internal typed evidence surface: `notes_evidence()` is callable in-crate by
// `ui/automation.rs` without naming the type, and only the external widget
// harness needs the name. Re-exporting it unconditionally would widen this
// crate's default public API for an internal readability goal.
#[cfg(feature = "test-utils")]
pub use browser::NotesBrowserRuntimeSnapshot;
#[cfg(feature = "test-utils")]
pub use evidence::{NotesEvidence, OpenEditorNoteCaptureEvidence};
// The note-source delay and the browser-query delay stay in
// `services/palette/notes.rs`, which owns the behavior they change and shares it
// with the migrated command-palette row; only this workflow's own two overrides
// live in `test_policy`.
#[cfg(feature = "test-utils")]
pub use crate::services::palette::{
    set_note_source_delay_for_test, set_notes_browser_query_delay_for_test,
};
#[cfg(feature = "test-utils")]
pub use test_policy::{
    set_bookmark_excerpt_preview_delay_for_test, set_notes_browser_source_entry_limit_for_test,
};

use crate::services::palette as palette_service;

use super::LushtextWindow;

/// Return the effective browser source policy for this process.
///
/// Production reads `policy`'s budgets directly; only a `test-utils` build can
/// narrow the entry ceiling, and it does so through the workflow's one
/// test-policy value rather than through a second module static.
#[cfg(feature = "test-utils")]
fn notes_browser_source_limits() -> palette_service::NoteSourceLimits {
    test_policy::notes_browser_source_limits()
}

#[cfg(not(feature = "test-utils"))]
fn notes_browser_source_limits() -> palette_service::NoteSourceLimits {
    // Narrowing to the production ceiling is a no-op, and going through the same
    // function the `test-utils` path uses keeps one code path in both feature
    // configurations rather than two that could drift.
    policy::notes_browser_source_limits_for_entries(policy::NOTES_BROWSER_SOURCE_ENTRY_LIMIT)
}
