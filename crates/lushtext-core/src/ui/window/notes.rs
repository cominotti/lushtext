// SPDX-License-Identifier: GPL-3.0-or-later

//! Bookmark and note workflows for the main window shell.
//!
//! This module keeps note-specific action handling, dialogs, persistence
//! scheduling, and workspace browse logic out of the generic document shell.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
#[cfg(feature = "test-utils")]
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::translate::IntoGlib;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use gtk4::{gio, pango};
use libadwaita::prelude::{
    AdwDialogExt, AlertDialogExt, AlertDialogExtManual, PreferencesGroupExt, SidebarItemExt,
};

use crate::model::bookmark::BookmarkRecord;
use crate::model::migration_ledger::MigrationKind;
use crate::model::note::{NoteEditorPresentation, NoteViewMode, RichNoteBody};
use crate::model::palette::{
    PaletteNoteCategory, PaletteNoteEntry, PaletteNoteTarget, PaletteOpenEditorNoteSnapshot,
};
use crate::model::workspace::{WorkspaceConfig, WorkspaceScope};
use crate::services::recovery_metadata::RecoveryDiagnostic;
use crate::services::{
    bookmark_excerpt, bookmark_service, document_note_service, folder_note_service, json_store,
    local_history_service, migration_ledger, palette as palette_service,
};
use crate::ui::editor_page::{
    BookmarkEditError, BookmarkNavigationDirection, BookmarkToggleState, LushtextEditorPage,
};
use crate::ui::markdown_preview::{LushtextMarkdownPreview, MarkdownPreviewRenderContext};
use crate::ui::status_bar::MessageKind;
use crate::ui::{accessibility, buffer_snapshot};
use gtk_lush_settle::Debounce;

use super::LushtextWindow;

/// Debounce interval for bookmark sidecar saves.
///
/// 200ms coalesces rapid line-shift edits into one filesystem write without
/// letting note state drift for long after the user pauses typing.
const NOTES_SAVE_DEBOUNCE_MS: u64 = 200;

/// Coalesce command-palette note-source reloads after live note/bookmark bursts.
const COMMAND_PALETTE_NOTES_REFRESH_DEBOUNCE_MS: u64 = 150;

/// Coalesce note-editor dirty-state checks after rapid typing.
///
/// Save sensitivity is helpful feedback, but recomputing it from the whole
/// buffer on every keystroke can steal time from text input on large notes.
const NOTE_SAVE_RESPONSE_REFRESH_DEBOUNCE_MS: u64 = 80;

/// Alert-dialog response IDs reused by the note workflows.
const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_SAVE: &str = "save";
const RESPONSE_CLEAR: &str = "clear";

/// Search scope title used by the standalone workspace bookmark browser.
const WORKSPACE_SCOPE_TITLE: &str = "Current Workspace";

/// Fixed notes browser width keeps the preview usable without turning the dialog
/// into a second full window.
const NOTES_BROWSER_WIDTH_SP: i32 = 980;
/// Fixed notes browser height leaves room for the list, preview, and action row.
const NOTES_BROWSER_HEIGHT_SP: i32 = 700;
/// Compact empty-browser width keeps status text readable without opening the
/// full split-view browser when there is nothing to list yet.
const EMPTY_NOTES_BROWSER_WIDTH_SP: i32 = 640;
/// Compact empty-browser height fits the normal status-page icon, title, and
/// description without introducing a scrollbar.
const EMPTY_NOTES_BROWSER_HEIGHT_SP: i32 = 480;
/// Maximum note rows materialized into the Adwaita sidebar at once.
///
/// The full notes set is still loaded and searched, but building thousands of
/// GTK sidebar items in one pass can stall the main loop. Search refinements
/// let users narrow beyond this first responsive slice.
const NOTES_BROWSER_RENDER_LIMIT: usize = 500;
/// Stack child name for Markdown/status bookmark and note previews.
const NOTES_PREVIEW_MARKDOWN_CHILD: &str = "markdown";
/// Stack child name for raw-text bookmark previews.
const NOTES_PREVIEW_RAW_CHILD: &str = "raw";
/// Text tag applied to the bookmarked row inside the raw preview surface.
const NOTES_RAW_BOOKMARK_TARGET_TAG: &str = "bookmark-target-line";
/// Horizontal inset inside raw bookmark previews.
///
/// This matches the note editor body margins so switching between note and
/// bookmark previews does not make the preview content jump sideways.
const NOTES_RAW_PREVIEW_TEXT_MARGIN_HORIZONTAL_SP: i32 = 12;
/// Vertical inset inside raw bookmark previews.
const NOTES_RAW_PREVIEW_TEXT_MARGIN_VERTICAL_SP: i32 = 10;

#[cfg(feature = "test-utils")]
static BOOKMARK_EXCERPT_PREVIEW_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Configure an artificial closed-file bookmark preview delay for widget tests.
#[cfg(feature = "test-utils")]
pub fn set_bookmark_excerpt_preview_delay_for_test(delay_ms: u64) {
    BOOKMARK_EXCERPT_PREVIEW_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Stable width for the shared edit/render note surface inside note dialogs.
const NOTE_EDITOR_SURFACE_WIDTH_SP: i32 = 520;
/// Stable height for the edit/render stack, matching the editable page's
/// measured request so toggling render mode does not shrink note dialogs.
const NOTE_EDITOR_SURFACE_HEIGHT_SP: i32 = 300;
/// Shared horizontal text inset for edit and rendered note bodies.
const NOTE_EDITOR_TEXT_MARGIN_HORIZONTAL_SP: i32 = 12;
/// Shared vertical text inset for edit and rendered note bodies.
const NOTE_EDITOR_TEXT_MARGIN_VERTICAL_SP: i32 = 10;

/// Result of loading the unified notes browser off the GTK main thread.
struct NotesBrowserLoadResult {
    entries: Vec<NotesBrowserEntry>,
    diagnostics: Vec<RecoveryDiagnostic>,
}

/// Decision for `Open Folder Note...` when the caller has not supplied an exact folder row.
///
/// Folder notes are attached to folders, not workspaces. Naming this decision
/// keeps the zero/one/many rules explicit so command actions and workspace
/// header actions cannot quietly fall back to the first configured folder.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FolderNoteOpenTarget {
    /// The current shared scope is `All workspaces`, so no single folder can be inferred.
    AggregateScope,
    /// A concrete workspace ID was requested but no restored workspace matched it.
    WorkspaceMissing,
    /// The concrete workspace exists but has no folders to attach a note to.
    EmptyWorkspace { workspace_name: String },
    /// The concrete workspace has exactly one folder and can open directly.
    SingleFolder {
        workspace_name: String,
        folder: PathBuf,
    },
    /// The concrete workspace has multiple folders and needs a visible choice.
    ChooseFolder {
        workspace_name: String,
        folders: Vec<PathBuf>,
    },
}

/// One entry shown in the unified notes browser.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NotesBrowserEntry {
    /// Shared note row produced by the GTK-free palette note source.
    note: PaletteNoteEntry,
}

/// Lowercased search query plus a small prefix table for allocation-light matching.
struct NotesBrowserQuery {
    /// Query represented as Unicode scalar values so note bodies do not need to
    /// allocate their own lowercased copies on every keystroke.
    needle: Vec<char>,
    /// Knuth-Morris-Pratt prefix table for streaming substring matching.
    prefix: Vec<usize>,
}

/// State for one open unified notes browser dialog.
struct NotesBrowserState {
    /// Window that owns the browser and receives follow-up actions.
    window: LushtextWindow,
    /// Dialog containing the browser widgets.
    dialog: libadwaita::Dialog,
    /// Adaptive split view used for wide and narrow layouts.
    split_view: libadwaita::NavigationSplitView,
    /// Search field driving the current filtered row set.
    search_entry: gtk4::SearchEntry,
    /// Adwaita browse rail for bookmarks, folder notes, and document notes.
    sidebar: libadwaita::Sidebar,
    /// Visible notice when the current result set is capped for responsiveness.
    limit_label: gtk4::Label,
    /// Header label for the selected note.
    preview_title: gtk4::Label,
    /// Secondary metadata label for the selected note.
    preview_meta: gtk4::Label,
    /// Stack switching between Markdown/status previews and raw bookmark excerpts.
    preview_stack: gtk4::Stack,
    /// Shared markdown preview widget reused for notes and Markdown bookmark excerpts.
    markdown_preview: LushtextMarkdownPreview,
    /// Backing buffer for raw bookmark excerpts.
    raw_preview_buffer: gtk4::TextBuffer,
    /// Open action for the selected note.
    open_button: gtk4::Button,
    /// Back button shown when the split view collapses.
    back_button: gtk4::Button,
    /// Complete set of notes covered by this browser session.
    all_entries: Vec<NotesBrowserEntry>,
    /// Entry indexes currently shown in the sidebar's grouped visual order.
    filtered_indices: RefCell<Vec<usize>>,
    /// Debounce used to rebuild browser search rows after typing settles.
    search_debounce: Debounce,
    /// Generation counter used to ignore stale closed-file bookmark preview loads.
    preview_generation: Cell<u32>,
}

/// Weak handle to the currently visible unified notes browser.
///
/// Window actions use this to drive the same search, selection, and Open button
/// behavior a user sees in the dialog without keeping a closed dialog alive.
#[derive(Clone)]
pub(super) struct ActiveNotesBrowser {
    state: Weak<NotesBrowserState>,
}

impl ActiveNotesBrowser {
    /// Track one newly presented notes browser dialog.
    fn new(state: &Rc<NotesBrowserState>) -> Self {
        Self {
            state: Rc::downgrade(state),
        }
    }

    /// Return whether this handle still points to the same browser state.
    fn same_target(&self, other: &Self) -> bool {
        self.state.ptr_eq(&other.state)
    }

    /// Return whether the dialog state still exists.
    fn is_alive(&self) -> bool {
        self.state.upgrade().is_some()
    }

    /// Filter the visible notes browser through its normal search entry.
    fn set_query(&self, query: &str) -> bool {
        let Some(state) = self.state.upgrade() else {
            return false;
        };
        state.search_entry.set_text(query);
        true
    }

    /// Select one visible row by zero-based sidebar index.
    fn select_visible_row(&self, index: u32) -> bool {
        let Some(state) = self.state.upgrade() else {
            return false;
        };
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        if index >= state.filtered_indices.borrow().len() {
            return false;
        }
        let selected = u32::try_from(index).expect("usize originated from u32");
        state.sidebar.set_selected(selected);
        true
    }

    /// Activate the same Open workflow as the visible notes browser button.
    fn open_selected(&self) -> bool {
        let Some(state) = self.state.upgrade() else {
            return false;
        };
        if state.selected_entry_index().is_none() {
            return false;
        }
        state.open_selected();
        true
    }
}

impl LushtextWindow {
    /// Wire bookmark and note callbacks for a newly created editor page.
    pub(super) fn wire_note_callbacks(&self, editor: &LushtextEditorPage) {
        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.connect_file_loaded(move || {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
                && let Some(path) = editor.file_path()
            {
                window.resolve_notes_for_editor(&editor, &path);
            }
        });

        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.connect_bookmarks_changed(move || {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
            {
                window.save_bookmarks_debounced(&editor);
                window.refresh_command_palette_note_source_debounced();
                if window.is_active_editor(&editor) {
                    window.refresh_notes_menu_state();
                }
            }
        });

        // The editor owns the source-mark activation hook, but the window owns
        // dialogs and active-tab checks. Weak refs keep closed tabs/windows from
        // staying alive just because a signal connection still exists.
        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.connect_bookmark_activated(move |bookmark| {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
                && window.is_active_editor(&editor)
                && editor.file_path().is_some()
            {
                window.present_bookmark_edit_dialog(&editor, &bookmark);
            }
        });

        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.buffer().connect_mark_set(move |_, _, _| {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
                && window.is_active_editor(&editor)
            {
                window.refresh_notes_menu_state();
            }
        });
    }

    /// Reload sidecar notes for the editor after a successful file load or reload.
    pub(super) fn resolve_notes_for_editor(&self, editor: &LushtextEditorPage, path: &Path) {
        let path = path.to_path_buf();
        let path_for_load = path.clone();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            editor.clone(),
            move || {
                let data_dir = json_store::data_dir();
                bookmark_service::load_for_path(&data_dir, &path_for_load)
                    .map(|document| document.bookmarks)
            },
            move |editor, result| {
                if editor.file_path().as_deref() != Some(path.as_path()) {
                    return;
                }
                match result {
                    Ok(bookmarks) => {
                        editor.load_bookmarks(&bookmarks);
                        if let Some(window) = window_weak.upgrade() {
                            window.refresh_command_palette_note_source_debounced();
                            window.refresh_status_bar();
                        }
                    }
                    Err(error) => {
                        tracing::error!("Failed to load notes for {}: {error}", path.display());
                        editor.clear_bookmarks();
                        if let Some(window) = window_weak.upgrade() {
                            window.publish_status_message(
                                "Bookmarks could not be loaded",
                                MessageKind::Warning,
                            );
                        }
                    }
                }
            },
        );
    }

    /// Reset live note state after Save As so the new path starts from its own identity.
    pub(super) fn reset_notes_after_save_as(&self, editor: &LushtextEditorPage, path: &Path) {
        editor.clear_bookmarks();
        self.resolve_notes_for_editor(editor, path);
        self.refresh_command_palette_note_source_debounced();
    }

    /// Migrate sidecar documents after an in-app sidebar rename.
    ///
    /// Pending ledger state is recorded before sidecar moves begin so interrupted
    /// partial work can retry on startup by generation.
    pub(super) fn migrate_note_sidecars_after_rename(&self, old_path: &Path, new_path: &Path) {
        let old_path = old_path.to_path_buf();
        let new_path = new_path.to_path_buf();
        let old_path_for_move = old_path.clone();
        let new_path_for_move = new_path.clone();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                let generation = migration_ledger::record_pending(
                    &data_dir,
                    &old_path_for_move,
                    &new_path_for_move,
                    &[
                        MigrationKind::Bookmarks,
                        MigrationKind::DocumentNotes,
                        MigrationKind::FolderNotes,
                    ],
                )?;
                let bookmark_count = migration_ledger::run_tracked_kind(
                    &data_dir,
                    generation,
                    MigrationKind::Bookmarks,
                    || {
                        bookmark_service::move_path_tree(
                            &data_dir,
                            &old_path_for_move,
                            &new_path_for_move,
                        )
                    },
                )?;
                let document_note_count = migration_ledger::run_tracked_kind(
                    &data_dir,
                    generation,
                    MigrationKind::DocumentNotes,
                    || {
                        document_note_service::move_path_tree(
                            &data_dir,
                            &old_path_for_move,
                            &new_path_for_move,
                        )
                    },
                )?;
                let folder_note_count = migration_ledger::run_tracked_kind(
                    &data_dir,
                    generation,
                    MigrationKind::FolderNotes,
                    || {
                        folder_note_service::move_folder_tree(
                            &data_dir,
                            &old_path_for_move,
                            &new_path_for_move,
                        )
                    },
                )?;
                Ok::<_, anyhow::Error>((bookmark_count, document_note_count, folder_note_count))
            },
            move |(), result| {
                if let Err(error) = result {
                    tracing::error!(
                        "Failed to migrate note sidecars for {} -> {}: {error}",
                        old_path.display(),
                        new_path.display()
                    );
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message(
                            "Rename succeeded, but note sidecars could not be moved",
                            MessageKind::Warning,
                        );
                    }
                } else if let Some(window) = window_weak.upgrade() {
                    window.refresh_command_palette_note_source_debounced();
                }
            },
        );
    }

    /// Retry persisted sidecar or local-history migrations left by an
    /// interrupted rename flow.
    pub(super) fn reconcile_pending_migrations_on_startup(&self) {
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                let migration_report = migration_ledger::reconcile_pending(&data_dir)?;
                let local_history_report = local_history_service::reconcile_lineages(&data_dir)?;
                Ok::<_, anyhow::Error>((migration_report, local_history_report))
            },
            move |(), result| match result {
                Ok((migration_report, local_history_report)) => {
                    if migration_report.completed > 0 {
                        tracing::info!(
                            "Recovered {} pending migration kind(s)",
                            migration_report.completed
                        );
                    }
                    if local_history_report.reconciled_lineages > 0 {
                        tracing::info!(
                            "Reconciled {} local-history lineage(s)",
                            local_history_report.reconciled_lineages
                        );
                    }
                    if local_history_report.has_deferred_work() {
                        tracing::warn!(
                            "Deferred local-history reconciliation after scanning {} lineage(s)",
                            local_history_report.scanned_lineages
                        );
                    }
                    if !migration_report.diagnostics.is_empty()
                        || !local_history_report.diagnostics.is_empty()
                        || local_history_report.has_deferred_work()
                    {
                        for diagnostic in &migration_report.diagnostics {
                            tracing::warn!(
                                "Migration recovery {} generation {}: {}",
                                diagnostic.kind.label(),
                                diagnostic.generation,
                                diagnostic.message
                            );
                        }
                        for diagnostic in &local_history_report.diagnostics {
                            tracing::warn!(
                                "Local-history recovery diagnostic: {}",
                                diagnostic.summary()
                            );
                        }
                        if let Some(window) = window_weak.upgrade() {
                            window.publish_status_message(
                                "Some rename recovery work still needs attention",
                                MessageKind::Warning,
                            );
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!("Failed to reconcile pending migrations: {error}");
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message(
                            "Rename recovery state could not be checked",
                            MessageKind::Warning,
                        );
                    }
                }
            },
        );
    }

    /// Toggle the bookmark on the current cursor line.
    pub(super) fn toggle_bookmark(&self) {
        let Some(editor) = self.require_saved_editor("Bookmarks require a saved file") else {
            return;
        };

        match editor.toggle_bookmark_at_cursor() {
            BookmarkToggleState::Added(line) => self.publish_status_message(
                &format!("Bookmark added at line {}", line.saturating_add(1)),
                MessageKind::Info,
            ),
            BookmarkToggleState::Removed(line) => self.publish_status_message(
                &format!("Bookmark removed from line {}", line.saturating_add(1)),
                MessageKind::Info,
            ),
        }
    }

    /// Edit the bookmark on the current cursor line.
    pub(super) fn edit_bookmark(&self) {
        let Some(editor) = self.require_saved_editor("Bookmarks require a saved file") else {
            return;
        };
        let Some(bookmark) = editor.current_bookmark() else {
            self.publish_status_message(
                "Move the cursor to a bookmarked line first",
                MessageKind::Warning,
            );
            return;
        };

        self.present_bookmark_edit_dialog(&editor, &bookmark);
    }

    /// Build and present the modal editor for one existing bookmark.
    ///
    /// The window layer owns modal UI and status feedback, while accepted edits
    /// are delegated back to `LushtextEditorPage` so live mark movement, minimap
    /// refresh, and debounced sidecar persistence stay on the existing path.
    fn present_bookmark_edit_dialog(&self, editor: &LushtextEditorPage, bookmark: &BookmarkRecord) {
        // A custom `AdwDialog` gives this form two fields, custom actions, and
        // inline validation feedback without closing on invalid input.
        let dialog = libadwaita::Dialog::builder()
            .title("Edit Bookmark")
            .content_width(420)
            .build();

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.set_margin_top(18);
        content.set_margin_bottom(18);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let title_label = gtk4::Label::new(Some("Edit Bookmark"));
        title_label.set_halign(gtk4::Align::Start);
        title_label.set_hexpand(true);
        title_label.add_css_class("title-4");
        header.append(&title_label);

        header.append(&build_dialog_close_button(&dialog));
        content.append(&header);

        let group = libadwaita::PreferencesGroup::new();
        accessibility::set_role(&group, gtk4::AccessibleRole::Group);
        accessibility::set_labelled_description(
            &group,
            "Bookmark fields",
            "Edit the bookmark label and one-based line number",
        );

        // Adwaita preference rows provide standard GNOME labeled form controls
        // here; the explicit accessible group keeps the modal form meaningful
        // outside a preferences window.
        let label_row = libadwaita::EntryRow::builder().title("Label").build();
        accessibility::set_labelled_description(
            &label_row,
            "Bookmark label",
            "Optional bookmark name shown in lists, gutter tooltips, and note browsers",
        );
        if let Some(label) = bookmark.label.as_deref() {
            label_row.set_text(label);
        }
        group.add(&label_row);

        let line_row = libadwaita::EntryRow::builder()
            .title("Line")
            .text(bookmark.line.saturating_add(1).to_string())
            .build();
        accessibility::set_labelled_description(
            &line_row,
            "Bookmark line",
            "One-based document line number for this bookmark",
        );
        group.add(&line_row);
        content.append(&group);

        let error_label = gtk4::Label::new(None);
        error_label.set_halign(gtk4::Align::Start);
        error_label.set_xalign(0.0);
        error_label.set_wrap(true);
        error_label.add_css_class("error");
        error_label.set_visible(false);
        accessibility::set_role(&error_label, gtk4::AccessibleRole::Status);
        accessibility::set_label(&error_label, "Bookmark edit feedback");
        accessibility::set_hidden(&error_label, true);
        content.append(&error_label);

        let button_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        button_box.set_halign(gtk4::Align::End);
        let cancel_button = gtk4::Button::with_label("Cancel");
        accessibility::set_labelled_description(
            &cancel_button,
            "Cancel",
            "Close bookmark editor without saving changes",
        );
        let dialog_weak = dialog.downgrade();
        cancel_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog_weak.upgrade() {
                dialog.close();
            }
        });
        button_box.append(&cancel_button);

        let save_button = gtk4::Button::with_label("Save");
        save_button.add_css_class("suggested-action");
        accessibility::set_labelled_description(
            &save_button,
            "Save bookmark",
            "Save the bookmark label and line number",
        );
        button_box.append(&save_button);
        content.append(&button_box);

        let error_weak = error_label.downgrade();
        let line_row_weak = line_row.downgrade();
        line_row.connect_changed(move |_| {
            if let Some(error_label) = error_weak.upgrade() {
                clear_bookmark_edit_error(&error_label);
            }
            if let Some(line_row) = line_row_weak.upgrade() {
                accessibility::set_invalid(&line_row, false);
            }
        });

        let error_weak = error_label.downgrade();
        let line_row_weak = line_row.downgrade();
        label_row.connect_changed(move |_| {
            if let Some(error_label) = error_weak.upgrade() {
                clear_bookmark_edit_error(&error_label);
            }
            if let Some(line_row) = line_row_weak.upgrade() {
                accessibility::set_invalid(&line_row, false);
            }
        });

        let bookmark_id = bookmark.id.clone();
        let editor_weak = editor.downgrade();
        let window_weak = self.downgrade();
        let dialog_weak = dialog.downgrade();
        save_button.connect_clicked(move |_| {
            let label = (!label_row.text().trim().is_empty()).then(|| label_row.text().to_string());
            let target_line = match parse_bookmark_target_line(&line_row.text()) {
                Ok(line) => line,
                Err(message) => {
                    show_bookmark_edit_error(&error_label, Some(&line_row), &message);
                    return;
                }
            };

            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            // The window only parses dialog input. The editor validates line
            // range and collisions because it owns live marks and buffer state.
            match editor.update_bookmark(&bookmark_id, label, target_line) {
                Ok(outcome) => {
                    window.publish_status_message(
                        &format!("Bookmark saved at line {}", outcome.line.saturating_add(1)),
                        MessageKind::Info,
                    );
                    if let Some(dialog) = dialog_weak.upgrade() {
                        dialog.close();
                    }
                }
                Err(error) => {
                    let message = bookmark_edit_error_message(&error);
                    let line_row = if matches!(error, BookmarkEditError::NotFound) {
                        None
                    } else {
                        Some(&line_row)
                    };
                    show_bookmark_edit_error(&error_label, line_row, &message);
                }
            }
        });

        dialog.set_child(Some(&content));
        dialog.present(Some(self));
    }

    /// Jump to the next or previous bookmark in the active file.
    pub(super) fn navigate_bookmark_action(&self, direction: BookmarkNavigationDirection) {
        let Some(editor) = self.require_saved_editor("Bookmarks require a saved file") else {
            return;
        };

        let Some(bookmark) = editor.navigate_bookmark(direction) else {
            self.publish_status_message(
                "No bookmarks exist in the active file",
                MessageKind::Warning,
            );
            return;
        };

        self.publish_status_message(
            &format!("Jumped to {}", bookmark.display_label()),
            MessageKind::Info,
        );
    }

    /// Browse workspace bookmarks in a searchable dialog.
    pub(super) fn show_bookmarks_dialog(&self) {
        let workspace_folders = self.workspace_folder_paths_for_notes();
        if workspace_folders.is_empty() {
            self.publish_status_message(
                "Add a workspace before browsing bookmarks",
                MessageKind::Warning,
            );
            return;
        }

        spawn_blocking_then(
            self.clone(),
            move || {
                let data_dir = json_store::data_dir();
                bookmark_service::list_workspace_bookmarks_recovering(&data_dir, &workspace_folders)
            },
            |window, result| match result {
                Ok(listing) => {
                    Self::trace_browse_recovery_diagnostics(&listing.diagnostics);
                    if listing.bookmarks.is_empty() {
                        if listing.diagnostics.is_empty() {
                            window.publish_status_message(
                                "No bookmarks exist in the current workspace",
                                MessageKind::Info,
                            );
                        } else {
                            window.publish_status_message(
                                "Some bookmark data could not be loaded",
                                MessageKind::Warning,
                            );
                        }
                        return;
                    }

                    window.present_bookmark_browser(listing.bookmarks);
                    if !listing.diagnostics.is_empty() {
                        window.publish_status_message(
                            "Some bookmark data could not be loaded",
                            MessageKind::Warning,
                        );
                    }
                }
                Err(error) => {
                    tracing::error!("Failed to list workspace bookmarks: {error}");
                    window.publish_status_message(
                        "Bookmarks could not be listed",
                        MessageKind::Error,
                    );
                }
            },
        );
    }

    /// Open the document note for the active saved file.
    pub(super) fn open_document_note(&self) {
        let Some(editor) = self.require_saved_editor("Document notes require a saved file") else {
            return;
        };
        let Some(path) = editor.file_path() else {
            return;
        };
        self.open_document_note_for_path(&path);
    }

    /// Open the document note for a concrete saved file path.
    pub(super) fn open_document_note_for_path(&self, path: &Path) {
        self.open_document_note_for_path_with_folders(path, self.current_workspace_folder_paths());
    }

    /// Open the folder note for the current concrete workspace scope.
    pub(super) fn open_folder_note(&self) {
        self.open_folder_note_target(self.current_folder_note_open_target());
    }

    /// Open the folder note for a concrete workspace selected from the sidebar.
    pub(super) fn open_folder_note_for_id(
        &self,
        workspace_id: &crate::model::workspace::WorkspaceId,
    ) {
        let target = self
            .imp()
            .sidebar
            .workspaces_file()
            .workspaces
            .into_iter()
            .find(|workspace| &workspace.id == workspace_id)
            .map_or(
                FolderNoteOpenTarget::WorkspaceMissing,
                folder_note_target_for_workspace,
            );
        self.open_folder_note_target(target);
    }

    /// Open the folder note for an exact workspace folder row target.
    pub(super) fn open_folder_note_for_workspace_folder(
        &self,
        workspace_id: &crate::model::workspace::WorkspaceId,
        folder: &Path,
    ) {
        let Some(workspace) = self
            .imp()
            .sidebar
            .workspaces_file()
            .workspaces
            .into_iter()
            .find(|workspace| &workspace.id == workspace_id)
        else {
            self.publish_status_message("Folder note target was not found", MessageKind::Warning);
            return;
        };
        if !workspace
            .folders
            .iter()
            .any(|workspace_folder| workspace_folder.path() == folder)
        {
            self.publish_status_message("Folder note target was not found", MessageKind::Warning);
            return;
        }
        self.open_folder_note_for_folder(&workspace.name, folder);
    }

    /// Apply the already-decided folder-note target by warning, choosing, or opening directly.
    fn open_folder_note_target(&self, target: FolderNoteOpenTarget) {
        match target {
            FolderNoteOpenTarget::AggregateScope => {
                self.publish_status_message(
                    "Select one workspace before opening a folder note",
                    MessageKind::Warning,
                );
            }
            FolderNoteOpenTarget::WorkspaceMissing => {
                self.publish_status_message(
                    "Folder note target was not found",
                    MessageKind::Warning,
                );
            }
            FolderNoteOpenTarget::EmptyWorkspace { workspace_name } => {
                self.publish_status_message(
                    &format!("Add a folder to {workspace_name} before opening a folder note"),
                    MessageKind::Warning,
                );
            }
            FolderNoteOpenTarget::SingleFolder {
                workspace_name,
                folder,
            } => self.open_folder_note_for_folder(&workspace_name, &folder),
            FolderNoteOpenTarget::ChooseFolder {
                workspace_name,
                folders,
            } => self.present_folder_note_target_chooser(&workspace_name, folders),
        }
    }

    /// Browse notes across the current workspace scope.
    pub(super) fn show_notes_dialog(&self) {
        let workspaces_file = self.imp().sidebar.workspaces_file();
        let scope = workspaces_file.current_scope();
        let all_workspaces = workspaces_file.workspaces;
        // Snapshot the current scope before background I/O; the sidebar can
        // change while notes are being listed.
        let visible_workspaces: Vec<WorkspaceConfig> = match &scope {
            WorkspaceScope::All => all_workspaces.clone(),
            WorkspaceScope::Workspace(workspace_id) => all_workspaces
                .iter()
                .filter(|workspace| &workspace.id == workspace_id)
                .cloned()
                .collect(),
        };
        let scope_folders: Vec<PathBuf> = visible_workspaces
            .iter()
            .flat_map(WorkspaceConfig::folder_paths)
            .collect();
        let open_editor_snapshots =
            self.open_editor_note_snapshots(&scope_folders, &all_workspaces);

        spawn_blocking_then(
            self.clone(),
            move || {
                let data_dir = json_store::data_dir();
                let load = palette_service::load_note_entries_for_scope(
                    &data_dir,
                    &visible_workspaces,
                    &scope,
                    &scope_folders,
                    open_editor_snapshots,
                )?;
                Ok::<_, anyhow::Error>(NotesBrowserLoadResult {
                    entries: load
                        .entries
                        .into_iter()
                        .map(NotesBrowserEntry::from)
                        .collect(),
                    diagnostics: load.diagnostics,
                })
            },
            |window, result| match result {
                Ok(result) => {
                    Self::trace_browse_recovery_diagnostics(&result.diagnostics);
                    if result.entries.is_empty() {
                        window.present_notes_browser(result.entries);
                        if !result.diagnostics.is_empty() {
                            window.publish_status_message(
                                "Some note data could not be loaded",
                                MessageKind::Warning,
                            );
                        }
                        return;
                    }

                    window.present_notes_browser(result.entries);
                    if !result.diagnostics.is_empty() {
                        window.publish_status_message(
                            "Some note data could not be loaded",
                            MessageKind::Warning,
                        );
                    }
                }
                Err(error) => {
                    tracing::error!("Failed to list notes: {error}");
                    window.publish_status_message("Notes could not be listed", MessageKind::Error);
                }
            },
        );
    }

    fn trace_browse_recovery_diagnostics(diagnostics: &[RecoveryDiagnostic]) {
        for diagnostic in diagnostics {
            tracing::warn!("{}", diagnostic.summary());
        }
    }

    /// Load and present the document note attached to one saved file.
    fn open_document_note_for_path_with_folders(
        &self,
        path: &Path,
        workspace_folders: Vec<PathBuf>,
    ) {
        let path = path.to_path_buf();
        let path_for_load = path.clone();
        let path_for_dialog = path;
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                document_note_service::load_for_path(&data_dir, &path_for_load)
            },
            move |(), result| match result {
                Ok(note) => {
                    if let Some(window) = window_weak.upgrade() {
                        window.present_document_note_dialog(
                            &path_for_dialog,
                            workspace_folders,
                            note.as_ref().map(|document| &document.note),
                        );
                    }
                }
                Err(error) => {
                    tracing::error!(
                        "Failed to load document note for {}: {error}",
                        path_for_dialog.display()
                    );
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message(
                            "Document note could not be loaded",
                            MessageKind::Error,
                        );
                    }
                }
            },
        );
    }

    /// Present a folder choice when a workspace has more than one folder-note target.
    fn present_folder_note_target_chooser(&self, workspace_name: &str, folders: Vec<PathBuf>) {
        let dialog = libadwaita::AlertDialog::new(
            Some("Open Folder Note"),
            Some("Choose which workspace folder to open a note for."),
        );
        dialog.add_response(RESPONSE_CANCEL, "Cancel");
        dialog.set_close_response(RESPONSE_CANCEL);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        content.set_margin_start(6);
        content.set_margin_end(6);
        content.set_margin_top(6);
        content.set_margin_bottom(6);

        let rows_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        for folder in folders {
            let button = gtk4::Button::new();
            button.set_hexpand(true);
            button.set_halign(gtk4::Align::Fill);
            button.set_tooltip_text(Some(&folder.display().to_string()));

            let label = gtk4::Label::new(Some(&folder.display().to_string()));
            label.set_xalign(0.0);
            label.set_wrap(true);
            label.set_halign(gtk4::Align::Fill);
            label.set_hexpand(true);
            button.set_child(Some(&label));

            let workspace_name = workspace_name.to_string();
            let folder_for_button = folder;
            let window_weak = self.downgrade();
            let dialog_weak = dialog.downgrade();
            button.connect_clicked(move |_| {
                if let Some(dialog) = dialog_weak.upgrade() {
                    dialog.close();
                }
                if let Some(window) = window_weak.upgrade() {
                    window.open_folder_note_for_folder(&workspace_name, &folder_for_button);
                }
            });
            rows_box.append(&button);
        }

        let scroll = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .min_content_width(420)
            .max_content_height(280)
            .propagate_natural_height(true)
            .child(&rows_box)
            .build();
        content.append(&scroll);
        dialog.set_extra_child(Some(&content));
        dialog.present(Some(self));
    }

    /// Load and present the folder note attached to one workspace folder.
    fn open_folder_note_for_folder(&self, workspace_name: &str, folder: &Path) {
        let workspace_name = workspace_name.to_string();
        let folder = folder.to_path_buf();
        let folder_for_load = folder.clone();
        let folder_for_dialog = folder;
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                folder_note_service::load_for_folder_recovering(&data_dir, &folder_for_load)
            },
            move |(), result| match result {
                Ok(load) => {
                    let has_diagnostics = !load.diagnostics.is_empty();
                    Self::trace_browse_recovery_diagnostics(&load.diagnostics);
                    if let Some(window) = window_weak.upgrade() {
                        window.present_folder_note_dialog(
                            &workspace_name,
                            &folder_for_dialog,
                            load.document.as_ref().map(|document| &document.note),
                        );
                        if has_diagnostics {
                            window.publish_status_message(
                                "Some folder note data could not be loaded",
                                MessageKind::Warning,
                            );
                        }
                    }
                }
                Err(error) => {
                    tracing::error!(
                        "Failed to load folder note for {}: {error}",
                        folder_for_dialog.display()
                    );
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message(
                            "Folder note could not be loaded",
                            MessageKind::Error,
                        );
                    }
                }
            },
        );
    }

    /// Present the document note editor for one saved file.
    fn present_document_note_dialog(
        &self,
        path: &Path,
        workspace_folders: Vec<PathBuf>,
        existing_note: Option<&RichNoteBody>,
    ) {
        let dialog = libadwaita::AlertDialog::new(
            Some("Document Note"),
            Some("Keep a richer note for this file without changing its source bytes."),
        );
        dialog.add_response(RESPONSE_CANCEL, "Cancel");
        if existing_note.is_some() {
            dialog.add_response(RESPONSE_CLEAR, "Clear");
            dialog.set_response_appearance(
                RESPONSE_CLEAR,
                libadwaita::ResponseAppearance::Destructive,
            );
        }
        dialog.add_response(RESPONSE_SAVE, "Save");
        dialog.set_response_appearance(RESPONSE_SAVE, libadwaita::ResponseAppearance::Suggested);
        dialog.set_default_response(Some(RESPONSE_SAVE));
        dialog.set_close_response(RESPONSE_CANCEL);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content.set_margin_start(6);
        content.set_margin_end(6);
        content.set_margin_top(6);
        content.set_margin_bottom(6);

        let path_label = gtk4::Label::new(Some(&path.display().to_string()));
        path_label.set_halign(gtk4::Align::Start);
        path_label.set_xalign(0.0);
        path_label.set_wrap(true);
        path_label.add_css_class("dim-label");
        content.append(&path_label);

        let loaded_text = existing_note.as_ref().map(|note| note.text.as_str());
        let presentation = NoteEditorPresentation::from_loaded_text(loaded_text);
        let initial_text = loaded_text.unwrap_or("");
        let (surface, note_view) = build_note_editor_surface(
            initial_text,
            &MarkdownPreviewRenderContext::new(Some(path.to_path_buf()), workspace_folders),
            presentation.initial_mode(),
            "Write some note text to preview rendered markdown.",
        );
        install_note_save_response_state(&dialog, &note_view.buffer(), presentation, initial_text);
        content.append(&surface);
        dialog.set_extra_child(Some(&content));

        let path = path.to_path_buf();
        let existing_note = existing_note.cloned();
        let window = self.clone();
        dialog.choose(Some(self), gio::Cancellable::NONE, move |response| {
            if response == RESPONSE_CLEAR {
                let path_for_delete = path.clone();
                spawn_blocking_then(
                    window,
                    move || {
                        let data_dir = json_store::data_dir();
                        document_note_service::delete_for_path(&data_dir, &path_for_delete)
                    },
                    |window, result| match result {
                        Ok(()) => {
                            window.refresh_command_palette_note_source_debounced();
                            window
                                .publish_status_message("Document note cleared", MessageKind::Info);
                        }
                        Err(error) => {
                            tracing::error!("Failed to clear document note: {error}");
                            window.publish_status_message(
                                "Document note could not be cleared",
                                MessageKind::Error,
                            );
                        }
                    },
                );
            } else if response == RESPONSE_SAVE {
                let buffer = note_view.buffer();
                let path_for_save = path.clone();
                let existing_note_for_save = existing_note.clone();
                let window_for_save = window;
                snapshot_note_buffer_text(buffer, move |note_text| {
                    if note_text.trim().is_empty() {
                        window_for_save.publish_status_message(
                            "Document notes need note text",
                            MessageKind::Warning,
                        );
                        return;
                    }

                    let mut note = existing_note_for_save
                        .clone()
                        .unwrap_or_else(|| RichNoteBody::new(""));
                    if existing_note_for_save.is_some() {
                        let _ = note.update_text(&note_text);
                    } else {
                        note = RichNoteBody::new(&note_text);
                    }

                    spawn_blocking_then(
                        window_for_save,
                        move || {
                            let data_dir = json_store::data_dir();
                            document_note_service::save_for_path(&data_dir, &path_for_save, &note)
                                .map(|_| ())
                        },
                        |window, result| match result {
                            Ok(()) => {
                                window.refresh_command_palette_note_source_debounced();
                                window.publish_status_message(
                                    "Document note saved",
                                    MessageKind::Info,
                                );
                            }
                            Err(error) => {
                                tracing::error!("Failed to save document note: {error}");
                                window.publish_status_message(
                                    "Document note could not be saved",
                                    MessageKind::Error,
                                );
                            }
                        },
                    );
                });
            }
        });
    }

    /// Present the folder note editor for one concrete workspace folder.
    fn present_folder_note_dialog(
        &self,
        workspace_name: &str,
        folder: &Path,
        existing_note: Option<&RichNoteBody>,
    ) {
        let dialog = libadwaita::AlertDialog::new(
            Some("Folder Note"),
            Some("Keep one project-scoped note for this workspace folder."),
        );
        dialog.add_response(RESPONSE_CANCEL, "Cancel");
        if existing_note.is_some() {
            dialog.add_response(RESPONSE_CLEAR, "Clear");
            dialog.set_response_appearance(
                RESPONSE_CLEAR,
                libadwaita::ResponseAppearance::Destructive,
            );
        }
        dialog.add_response(RESPONSE_SAVE, "Save");
        dialog.set_response_appearance(RESPONSE_SAVE, libadwaita::ResponseAppearance::Suggested);
        dialog.set_default_response(Some(RESPONSE_SAVE));
        dialog.set_close_response(RESPONSE_CANCEL);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content.set_margin_start(6);
        content.set_margin_end(6);
        content.set_margin_top(6);
        content.set_margin_bottom(6);

        let title_label = gtk4::Label::new(Some(workspace_name));
        title_label.set_halign(gtk4::Align::Start);
        title_label.set_xalign(0.0);
        title_label.add_css_class("heading");
        content.append(&title_label);

        let folder_label = gtk4::Label::new(Some(&folder.display().to_string()));
        folder_label.set_halign(gtk4::Align::Start);
        folder_label.set_xalign(0.0);
        folder_label.set_wrap(true);
        folder_label.add_css_class("dim-label");
        content.append(&folder_label);

        let loaded_text = existing_note.as_ref().map(|note| note.text.as_str());
        let presentation = NoteEditorPresentation::from_loaded_text(loaded_text);
        let initial_text = loaded_text.unwrap_or("");
        let (surface, note_view) = build_note_editor_surface(
            initial_text,
            &MarkdownPreviewRenderContext::new(None, vec![folder.to_path_buf()]),
            presentation.initial_mode(),
            "Write some note text to preview rendered markdown.",
        );
        install_note_save_response_state(&dialog, &note_view.buffer(), presentation, initial_text);
        content.append(&surface);
        dialog.set_extra_child(Some(&content));

        let folder = folder.to_path_buf();
        let existing_note = existing_note.cloned();
        let window = self.clone();
        dialog.choose(Some(self), gio::Cancellable::NONE, move |response| {
            if response == RESPONSE_CLEAR {
                let folder_for_delete = folder.clone();
                spawn_blocking_then(
                    window,
                    move || {
                        let data_dir = json_store::data_dir();
                        folder_note_service::delete_for_folder(&data_dir, &folder_for_delete)
                    },
                    |window, result| match result {
                        Ok(()) => {
                            window.refresh_command_palette_note_source_debounced();
                            window.publish_status_message("Folder note cleared", MessageKind::Info);
                        }
                        Err(error) => {
                            tracing::error!("Failed to clear folder note: {error}");
                            window.publish_status_message(
                                "Folder note could not be cleared",
                                MessageKind::Error,
                            );
                        }
                    },
                );
            } else if response == RESPONSE_SAVE {
                let buffer = note_view.buffer();
                let folder_for_save = folder.clone();
                let existing_note_for_save = existing_note.clone();
                let window_for_save = window;
                snapshot_note_buffer_text(buffer, move |note_text| {
                    if note_text.trim().is_empty() {
                        window_for_save.publish_status_message(
                            "Folder notes need note text",
                            MessageKind::Warning,
                        );
                        return;
                    }

                    let mut note = existing_note_for_save
                        .clone()
                        .unwrap_or_else(|| RichNoteBody::new(""));
                    if existing_note_for_save.is_some() {
                        let _ = note.update_text(&note_text);
                    } else {
                        note = RichNoteBody::new(&note_text);
                    }

                    spawn_blocking_then(
                        window_for_save,
                        move || {
                            let data_dir = json_store::data_dir();
                            folder_note_service::save_for_folder(&data_dir, &folder_for_save, &note)
                                .map(|_| ())
                        },
                        |window, result| match result {
                            Ok(()) => {
                                window.refresh_command_palette_note_source_debounced();
                                window
                                    .publish_status_message("Folder note saved", MessageKind::Info);
                            }
                            Err(error) => {
                                tracing::error!("Failed to save folder note: {error}");
                                window.publish_status_message(
                                    "Folder note could not be saved",
                                    MessageKind::Error,
                                );
                            }
                        },
                    );
                });
            }
        });
    }

    /// Debounce bookmark persistence so one burst of edits produces one sidecar write.
    fn save_bookmarks_debounced(&self, editor: &LushtextEditorPage) {
        let window_weak = self.downgrade();
        editor.imp().bookmarks.persistence.save_debounce.schedule(
            editor,
            Duration::from_millis(NOTES_SAVE_DEBOUNCE_MS),
            move |editor, _| {
                if editor.imp().bookmarks.persistence.save_inflight.get() {
                    editor.imp().bookmarks.persistence.save_dirty.set(true);
                    return;
                }

                if let Some(window) = window_weak.upgrade() {
                    window.persist_bookmarks_now(&editor);
                }
            },
        );
    }

    /// Write the current bookmark snapshot to disk.
    fn persist_bookmarks_now(&self, editor: &LushtextEditorPage) {
        let Some(path) = editor.file_path() else {
            return;
        };
        let bookmarks = editor.bookmark_records();
        let data_dir = json_store::data_dir();
        editor.imp().bookmarks.persistence.save_inflight.set(true);
        editor.imp().bookmarks.persistence.save_dirty.set(false);

        let window_weak = self.downgrade();
        spawn_blocking_then(
            editor.clone(),
            move || bookmark_service::save_for_path(&data_dir, &path, &bookmarks).map(|_| ()),
            move |editor, result| {
                editor.imp().bookmarks.persistence.save_inflight.set(false);
                if let Err(error) = result {
                    tracing::error!("Failed to save bookmarks: {error}");
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message("Bookmark save failed", MessageKind::Warning);
                    }
                }
                if editor.imp().bookmarks.persistence.save_dirty.replace(false)
                    && let Some(window) = window_weak.upgrade()
                {
                    window.persist_bookmarks_now(&editor);
                }
            },
        );
    }

    /// Snapshot open saved-editor note state without touching the filesystem.
    ///
    /// This runs on the GTK main thread because `bookmark_records()` reads the
    /// live `GtkSourceMark` projection. Sidecar loading and identity
    /// deduplication stay in the existing background browse task.
    fn open_editor_note_snapshots(
        &self,
        scope_folders: &[PathBuf],
        all_workspaces: &[WorkspaceConfig],
    ) -> Vec<PaletteOpenEditorNoteSnapshot> {
        let tab_view = &self.imp().tab_view;
        let mut snapshots = Vec::new();
        for index in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(index);
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            let Some(path) = editor.file_path() else {
                continue;
            };
            let open_tab_source = (!palette_service::path_is_in_folders(&path, scope_folders))
                .then(|| palette_service::open_tab_source_for_path(all_workspaces, &path));
            snapshots.push(PaletteOpenEditorNoteSnapshot {
                path: path.clone(),
                bookmarks: editor.bookmark_records(),
                open_tab_source,
            });
        }
        snapshots
    }

    /// Coalesce cached note-row refreshes after bursty note and bookmark edits.
    pub(super) fn refresh_command_palette_note_source_debounced(&self) {
        if !self.imp().palette_revealer.reveals_child() {
            self.invalidate_command_palette_note_source();
            return;
        }

        self.imp().command_palette_notes_refresh_debounce.schedule(
            self,
            Duration::from_millis(COMMAND_PALETTE_NOTES_REFRESH_DEBOUNCE_MS),
            |window, _| {
                window.refresh_command_palette_note_source();
            },
        );
    }

    /// Refresh cached command-palette note rows from the current workspace scope.
    ///
    /// The GTK thread only snapshots open-editor bookmark metadata here. Sidecar
    /// listing and document identity work stay in the background task, and the
    /// generation guard prevents stale completions from replacing newer rows.
    pub(super) fn refresh_command_palette_note_source(&self) {
        let generation = self.next_command_palette_notes_generation();
        if !self.imp().palette_revealer.reveals_child() {
            self.imp().command_palette.set_note_entries(Vec::new());
            return;
        }

        let workspaces_file = self.imp().sidebar.workspaces_file();
        let scope = workspaces_file.current_scope();
        let all_workspaces = workspaces_file.workspaces;
        let visible_workspaces: Vec<WorkspaceConfig> = match &scope {
            WorkspaceScope::All => all_workspaces.clone(),
            WorkspaceScope::Workspace(workspace_id) => all_workspaces
                .iter()
                .filter(|workspace| &workspace.id == workspace_id)
                .cloned()
                .collect(),
        };
        let scope_folders: Vec<PathBuf> = visible_workspaces
            .iter()
            .flat_map(WorkspaceConfig::folder_paths)
            .collect();
        let open_editor_snapshots =
            self.open_editor_note_snapshots(&scope_folders, &all_workspaces);

        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                palette_service::load_note_entries_for_scope(
                    &data_dir,
                    &visible_workspaces,
                    &scope,
                    &scope_folders,
                    open_editor_snapshots,
                )
            },
            move |(), result| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                if window.imp().command_palette_notes_generation.get() != generation {
                    return;
                }
                match result {
                    Ok(load) => {
                        Self::trace_browse_recovery_diagnostics(&load.diagnostics);
                        let has_diagnostics = !load.diagnostics.is_empty();
                        window.imp().command_palette.set_note_entries(load.entries);
                        if has_diagnostics && window.imp().palette_revealer.reveals_child() {
                            window.publish_status_message(
                                "Some note data could not be loaded for the palette",
                                MessageKind::Warning,
                            );
                        }
                    }
                    Err(error) => {
                        tracing::warn!("Failed to refresh command-palette notes: {error}");
                        if window.imp().palette_revealer.reveals_child() {
                            window.publish_status_message(
                                "Notes could not be loaded for the palette",
                                MessageKind::Warning,
                            );
                        }
                    }
                }
            },
        );
    }

    fn invalidate_command_palette_note_source(&self) {
        self.next_command_palette_notes_generation();
        self.imp().command_palette.set_note_entries(Vec::new());
    }

    fn next_command_palette_notes_generation(&self) -> u32 {
        let generation = self
            .imp()
            .command_palette_notes_generation
            .get()
            .wrapping_add(1);
        self.imp().command_palette_notes_generation.set(generation);
        generation
    }

    /// Find an already-open saved editor for a concrete path.
    fn open_editor_for_path(&self, path: &Path) -> Option<LushtextEditorPage> {
        let tab_view = &self.imp().tab_view;
        for index in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(index);
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            if editor.file_path().as_deref() == Some(path) {
                return Some(editor.clone());
            }
        }
        None
    }

    /// Return the active editor only when it has a stable saved file path.
    fn require_saved_editor(&self, missing_path_message: &str) -> Option<LushtextEditorPage> {
        let Some(editor) = self.active_editor() else {
            self.publish_status_message(missing_path_message, MessageKind::Warning);
            return None;
        };
        if editor.file_path().is_some() {
            return Some(editor);
        }

        self.publish_status_message(missing_path_message, MessageKind::Warning);
        None
    }

    /// Collect current workspace folders for bookmark and note workflows.
    fn workspace_folder_paths_for_notes(&self) -> Vec<PathBuf> {
        self.current_workspace_folder_paths()
    }

    /// Decide what `Open Folder Note...` can do in the current shared scope.
    fn current_folder_note_open_target(&self) -> FolderNoteOpenTarget {
        let workspaces_file = self.imp().sidebar.workspaces_file();
        let WorkspaceScope::Workspace(workspace_id) = workspaces_file.current_scope() else {
            return FolderNoteOpenTarget::AggregateScope;
        };
        workspaces_file
            .workspaces
            .into_iter()
            .find(|workspace| workspace.id == workspace_id)
            .map_or(
                FolderNoteOpenTarget::WorkspaceMissing,
                folder_note_target_for_workspace,
            )
    }

    /// Return whether the header menu can start a folder-note workflow immediately.
    fn current_folder_note_action_available(&self) -> bool {
        matches!(
            self.current_folder_note_open_target(),
            FolderNoteOpenTarget::SingleFolder { .. } | FolderNoteOpenTarget::ChooseFolder { .. }
        )
    }

    /// Refresh the window-scoped Notes menu label and menu-only action state.
    ///
    /// The header button and `Browse Notes…` stay window-scoped so the browser
    /// can show workspace rows, open-tab rows, or its empty state even when no
    /// editor tab is active. Target-specific rows still use sensitivity below.
    ///
    /// The dedicated menu uses its own `notes-*` actions so it can become
    /// insensitive without disabling the existing shortcuts or command-palette
    /// commands that still rely on the workflow guards below.
    pub(super) fn refresh_notes_menu_state(&self) {
        let active_editor = self.active_editor();
        let saved_editor = active_editor
            .as_ref()
            .filter(|editor| editor.file_path().is_some());
        let bookmark_label = if saved_editor
            .as_ref()
            .is_some_and(|editor| editor.current_bookmark().is_some())
        {
            "Remove Bookmark"
        } else {
            "Add Bookmark"
        };

        if !self.notes_menu_uses_bookmark_label(bookmark_label) {
            self.rebuild_notes_menu(bookmark_label);
        }

        self.imp().notes_menu_button.set_visible(true);

        self.set_notes_menu_action_enabled("notes-toggle-bookmark", saved_editor.is_some());
        self.set_notes_menu_action_enabled("notes-open-document-note", saved_editor.is_some());
        self.set_notes_menu_action_enabled(
            "notes-open-folder-note",
            self.current_folder_note_action_available(),
        );
        self.set_notes_menu_action_enabled("notes-show-notes", true);
    }

    /// Check the existing menu model before replacing it during ordinary state refreshes.
    ///
    /// The menu is small, and avoiding no-op replacements keeps GTK's popup
    /// lifecycle stable if a refresh races with user activation.
    fn notes_menu_uses_bookmark_label(&self, bookmark_label: &'static str) -> bool {
        let Some(menu) = self.imp().notes_menu_button.menu_model() else {
            return false;
        };

        Self::menu_label_for_action(&menu, "win.notes-toggle-bookmark")
            .is_some_and(|label| label == bookmark_label)
    }

    /// Find the label for one action in a possibly sectioned menu model.
    ///
    /// Searching by action keeps the bookmark-label guard independent from the
    /// visual section order, which is allowed to change as the menu evolves.
    fn menu_label_for_action(model: &gio::MenuModel, action_name: &str) -> Option<String> {
        for index in 0..model.n_items() {
            let action = model
                .item_attribute_value(index, "action", Some(glib::VariantTy::STRING))
                .and_then(|variant| variant.get::<String>());
            if action.as_deref() == Some(action_name) {
                return model
                    .item_attribute_value(index, "label", Some(glib::VariantTy::STRING))
                    .and_then(|variant| variant.get::<String>());
            }

            for link_name in ["section", "submenu"] {
                if let Some(link) = model.item_link(index, link_name)
                    && let Some(label) = Self::menu_label_for_action(&link, action_name)
                {
                    return Some(label);
                }
            }
        }
        None
    }

    /// Rebuild the small header-bar Notes menu so its bookmark row can use
    /// the active cursor context without disabling the expert command actions.
    fn rebuild_notes_menu(&self, bookmark_label: &'static str) {
        let menu = gio::Menu::new();

        let browse_section = gio::Menu::new();
        browse_section.append(Some("Browse Notes…"), Some("win.notes-show-notes"));
        menu.append_section(None, &browse_section);

        let document_section = gio::Menu::new();
        document_section.append(Some(bookmark_label), Some("win.notes-toggle-bookmark"));
        document_section.append(
            Some("Open Document Note…"),
            Some("win.notes-open-document-note"),
        );
        menu.append_section(None, &document_section);

        let workspace_section = gio::Menu::new();
        workspace_section.append(
            Some("Open Folder Note…"),
            Some("win.notes-open-folder-note"),
        );
        menu.append_section(None, &workspace_section);

        self.imp().notes_menu_button.set_menu_model(Some(&menu));
    }

    /// Update one Notes-menu-only action without affecting shortcut actions.
    fn set_notes_menu_action_enabled(&self, action_name: &str, enabled: bool) {
        if let Some(action) = self.lookup_action(action_name)
            && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
        {
            simple.set_enabled(enabled);
        }
    }

    /// Present the searchable bookmark browser dialog.
    fn present_bookmark_browser(&self, bookmarks: Vec<bookmark_service::WorkspaceBookmark>) {
        let dialog = build_browser_dialog("Bookmarks");
        let content = browser_content_box(&dialog);
        let search_entry = gtk4::SearchEntry::new();
        search_entry.set_placeholder_text(Some(&format!("Search {WORKSPACE_SCOPE_TITLE}…")));
        accessibility::set_labelled_description(
            &search_entry,
            "Search bookmarks",
            "Filter bookmarks in the current workspace",
        );
        content.append(&search_entry);

        let rows_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        accessibility::set_role(&rows_box, gtk4::AccessibleRole::List);
        accessibility::set_labelled_description(
            &rows_box,
            "Bookmark results",
            "Bookmarks in the current workspace matching the search",
        );
        let scroll = gtk4::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .min_content_height(320)
            .child(&rows_box)
            .build();
        content.append(&scroll);

        let bookmarks = Rc::new(bookmarks);
        rebuild_bookmark_rows(self, &dialog, &rows_box, &bookmarks, "");

        let window_weak = self.downgrade();
        let dialog_weak = dialog.downgrade();
        let rows_box_weak = rows_box.downgrade();
        let bookmarks_for_search = bookmarks;
        let search_debounce = Debounce::default();
        search_entry.connect_search_changed(move |entry| {
            let query = entry.text().to_string();
            if query.is_empty() {
                let _ = search_debounce.invalidate();
                if let (Some(window), Some(dialog), Some(rows_box)) = (
                    window_weak.upgrade(),
                    dialog_weak.upgrade(),
                    rows_box_weak.upgrade(),
                ) {
                    rebuild_bookmark_rows(&window, &dialog, &rows_box, &bookmarks_for_search, "");
                }
                return;
            }
            let window_weak = window_weak.clone();
            let dialog_weak = dialog_weak.clone();
            let rows_box_weak = rows_box_weak.clone();
            let bookmarks_for_search = bookmarks_for_search.clone();
            search_debounce.schedule(entry, Duration::from_millis(150), move |_, _| {
                let (Some(window), Some(dialog), Some(rows_box)) = (
                    window_weak.upgrade(),
                    dialog_weak.upgrade(),
                    rows_box_weak.upgrade(),
                ) else {
                    return;
                };
                rebuild_bookmark_rows(&window, &dialog, &rows_box, &bookmarks_for_search, &query);
            });
        });

        dialog.present(Some(self));
    }

    /// Present the unified notes browser for the current workspace scope.
    fn present_notes_browser(&self, entries: Vec<NotesBrowserEntry>) {
        if entries.is_empty() {
            build_empty_notes_dialog().present(Some(self));
            return;
        }

        let dialog = libadwaita::Dialog::builder()
            .title("Notes")
            .content_width(NOTES_BROWSER_WIDTH_SP)
            .content_height(NOTES_BROWSER_HEIGHT_SP)
            .follows_content_size(false)
            .build();

        let search_entry = gtk4::SearchEntry::new();
        install_dialog_escape_close(&dialog, &search_entry);
        search_entry.set_placeholder_text(Some("Search Notes..."));
        accessibility::set_labelled_description(
            &search_entry,
            "Search notes",
            "Filter bookmarks, document notes, and folder notes",
        );

        let sidebar = libadwaita::Sidebar::new();
        accessibility::set_role(&sidebar, gtk4::AccessibleRole::List);
        sidebar.set_mode(libadwaita::SidebarMode::Sidebar);
        sidebar.set_vexpand(true);
        sidebar.set_placeholder(Some(&empty_browser_label("No notes match that search")));
        accessibility::set_labelled_description(
            &sidebar,
            "Notes results",
            "Choose a bookmark, document note, or folder note",
        );
        let limit_label = gtk4::Label::new(None);
        limit_label.set_halign(gtk4::Align::Start);
        limit_label.set_xalign(0.0);
        limit_label.set_wrap(true);
        limit_label.add_css_class("caption");
        limit_label.add_css_class("dim-label");
        limit_label.set_visible(false);
        accessibility::set_role(&limit_label, gtk4::AccessibleRole::Status);
        accessibility::set_labelled_description(
            &limit_label,
            "Notes result limit",
            "Shown when the notes browser limits a large result set",
        );

        let preview_title = gtk4::Label::new(Some("Select a note"));
        preview_title.set_halign(gtk4::Align::Start);
        preview_title.set_xalign(0.0);
        preview_title.add_css_class("title-4");

        let preview_meta = gtk4::Label::new(Some(
            "Choose a bookmark, folder note, or document note to preview it here.",
        ));
        preview_meta.set_halign(gtk4::Align::Start);
        preview_meta.set_xalign(0.0);
        preview_meta.set_wrap(true);
        preview_meta.add_css_class("dim-label");

        let markdown_preview = LushtextMarkdownPreview::new();
        markdown_preview.set_hexpand(true);
        markdown_preview.set_vexpand(true);
        markdown_preview.show_placeholder("Select a note to preview its details.");

        let raw_preview_buffer = gtk4::TextBuffer::new(None);
        ensure_raw_preview_target_tag(&raw_preview_buffer);
        let raw_preview_view = gtk4::TextView::with_buffer(&raw_preview_buffer);
        raw_preview_view.set_editable(false);
        raw_preview_view.set_cursor_visible(false);
        raw_preview_view.set_monospace(true);
        raw_preview_view.set_wrap_mode(gtk4::WrapMode::None);
        raw_preview_view.set_left_margin(NOTES_RAW_PREVIEW_TEXT_MARGIN_HORIZONTAL_SP);
        raw_preview_view.set_right_margin(NOTES_RAW_PREVIEW_TEXT_MARGIN_HORIZONTAL_SP);
        raw_preview_view.set_top_margin(NOTES_RAW_PREVIEW_TEXT_MARGIN_VERTICAL_SP);
        raw_preview_view.set_bottom_margin(NOTES_RAW_PREVIEW_TEXT_MARGIN_VERTICAL_SP);
        accessibility::set_labelled_description(
            &raw_preview_view,
            "Bookmark source preview",
            "Read-only source excerpt around the selected bookmark",
        );
        accessibility::set_read_only(&raw_preview_view, true);
        accessibility::set_multi_line(&raw_preview_view, true);

        let raw_preview_scroll = gtk4::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .propagate_natural_width(false)
            .propagate_natural_height(false)
            .child(&raw_preview_view)
            .build();

        let preview_stack = gtk4::Stack::new();
        preview_stack.set_hexpand(true);
        preview_stack.set_vexpand(true);
        preview_stack.set_hhomogeneous(true);
        preview_stack.set_vhomogeneous(true);
        preview_stack.add_named(&markdown_preview, Some(NOTES_PREVIEW_MARKDOWN_CHILD));
        preview_stack.add_named(&raw_preview_scroll, Some(NOTES_PREVIEW_RAW_CHILD));
        preview_stack.set_visible_child_name(NOTES_PREVIEW_MARKDOWN_CHILD);
        accessibility::set_role(&preview_stack, gtk4::AccessibleRole::Group);
        accessibility::set_labelled_description(
            &preview_stack,
            "Notes preview",
            "Read-only preview for the selected bookmark, document note, or folder note",
        );
        accessibility::set_value_text(&preview_stack, "No note selected");

        let open_button = gtk4::Button::with_label("Open");
        open_button.add_css_class("suggested-action");
        open_button.set_sensitive(false);
        accessibility::set_labelled_description(
            &open_button,
            "Open selected note",
            "Open the selected bookmark, document note, or folder note",
        );
        accessibility::set_disabled(&open_button, true);
        accessibility::set_value_text(&open_button, "No note selected");

        let back_button = gtk4::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back to Notes")
            .visible(false)
            .build();
        accessibility::set_labelled_description(
            &back_button,
            "Back to notes",
            "Return to the notes result list in compact layouts",
        );

        let split_view = libadwaita::NavigationSplitView::new();
        split_view.set_hexpand(true);
        split_view.set_vexpand(true);
        split_view.set_min_sidebar_width(260.0);
        split_view.set_max_sidebar_width(340.0);
        split_view.set_sidebar(Some(&libadwaita::NavigationPage::new(
            &build_notes_sidebar(&dialog, &search_entry, &sidebar, &limit_label),
            "Notes",
        )));
        split_view.set_content(Some(&libadwaita::NavigationPage::new(
            &build_notes_preview_page(
                &dialog,
                &back_button,
                &preview_title,
                &preview_meta,
                &preview_stack,
                &open_button,
            ),
            "Preview",
        )));
        split_view.set_show_content(false);
        dialog.set_child(Some(&build_notes_browser_shell(&dialog, &split_view)));

        let state = Rc::new(NotesBrowserState {
            window: self.clone(),
            dialog,
            split_view,
            search_entry,
            sidebar,
            limit_label,
            preview_title,
            preview_meta,
            preview_stack,
            markdown_preview,
            raw_preview_buffer,
            open_button,
            back_button,
            filtered_indices: RefCell::new(Vec::new()),
            search_debounce: Debounce::default(),
            preview_generation: Cell::new(0),
            all_entries: entries,
        });

        rebuild_notes_browser_sidebar(&state, "");
        state.search_entry.connect_search_changed({
            let state = Rc::downgrade(&state);
            move |entry| {
                if let Some(state) = state.upgrade() {
                    schedule_notes_browser_search(&state, entry.text().to_string());
                }
            }
        });
        state.sidebar.connect_selected_item_notify({
            let state = Rc::downgrade(&state);
            move |sidebar| {
                if let Some(state) = state.upgrade() {
                    NotesBrowserState::refresh_preview(
                        &state,
                        sidebar_item_index(sidebar.selected_item()),
                        true,
                    );
                }
            }
        });
        state.sidebar.connect_activated({
            let state = Rc::downgrade(&state);
            move |sidebar, index| {
                if let Some(state) = state.upgrade() {
                    sidebar.set_selected(index);
                    NotesBrowserState::refresh_preview(&state, usize::try_from(index).ok(), true);
                }
            }
        });
        state.open_button.connect_clicked({
            let state = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = state.upgrade() {
                    state.open_selected();
                }
            }
        });
        state.back_button.connect_clicked({
            let state = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = state.upgrade() {
                    state.split_view.set_show_content(false);
                }
            }
        });
        // Collapsed adaptive navigation owns back-button visibility. The
        // binding seeds the initial dialog layout and stays live as breakpoints
        // change without storing a signal handler ID.
        state
            .split_view
            .bind_property("collapsed", &state.back_button, "visible")
            .sync_create()
            .build();

        let active_browser = ActiveNotesBrowser::new(&state);
        *self.imp().active_notes_browser.borrow_mut() = Some(active_browser.clone());
        self.set_notes_browser_actions_enabled(true);

        // The dialog owns this holder while it is visible, keeping browser
        // state alive without child-widget signal closures strongly owning the
        // whole dialog subtree. The `closed` signal drops the state and breaks
        // the temporary dialog -> holder -> state -> dialog cycle.
        let state_holder = Rc::new(RefCell::new(Some(Rc::clone(&state))));
        state.dialog.connect_closed({
            let state_holder = Rc::clone(&state_holder);
            let window_weak = self.downgrade();
            move |_| {
                state_holder.borrow_mut().take();
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let should_disable = {
                    let mut slot = window.imp().active_notes_browser.borrow_mut();
                    if slot
                        .as_ref()
                        .is_some_and(|current| current.same_target(&active_browser))
                    {
                        slot.take();
                        true
                    } else {
                        false
                    }
                };
                if should_disable {
                    window.set_notes_browser_actions_enabled(false);
                }
            }
        });

        state.dialog.present(Some(self));
        focus_after_present(&state.search_entry);
    }

    /// Enable or disable actions that require a visible unified notes browser.
    pub(super) fn set_notes_browser_actions_enabled(&self, enabled: bool) {
        for name in [
            "set-notes-browser-query",
            "select-notes-browser-row",
            "open-notes-browser-selection",
        ] {
            if let Some(action) = self.lookup_action(name)
                && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
            {
                simple.set_enabled(enabled);
            }
        }
    }

    /// Set the visible notes-browser filter text through the dialog search entry.
    pub(super) fn set_notes_browser_query(&self, query: &str) {
        let Some(browser) = self.current_notes_browser() else {
            self.publish_status_message(
                "Open Browse Notes before filtering notes",
                MessageKind::Warning,
            );
            return;
        };
        if !browser.set_query(query) {
            self.set_notes_browser_actions_enabled(false);
        }
    }

    /// Select one visible notes-browser row without relying on pointer coordinates.
    pub(super) fn select_notes_browser_row(&self, index: u32) {
        let Some(browser) = self.current_notes_browser() else {
            self.publish_status_message(
                "Open Browse Notes before selecting a notes row",
                MessageKind::Warning,
            );
            return;
        };
        if !browser.select_visible_row(index) {
            self.publish_status_message("That notes row is not visible", MessageKind::Warning);
        }
    }

    /// Open the currently selected notes-browser row through the visible workflow.
    pub(super) fn open_notes_browser_selection(&self) {
        let Some(browser) = self.current_notes_browser() else {
            self.publish_status_message(
                "Open Browse Notes before opening a note",
                MessageKind::Warning,
            );
            return;
        };
        if !browser.open_selected() {
            self.publish_status_message(
                "Select a notes row before opening it",
                MessageKind::Warning,
            );
        }
    }

    /// Activate one note-search target through the existing note workflows.
    pub(super) fn activate_palette_note_target(&self, target: &PaletteNoteTarget) {
        match target {
            PaletteNoteTarget::Bookmark { path, line, .. } => {
                open_editor_at_line(self, path, line.saturating_add(1));
            }
            PaletteNoteTarget::FolderNote {
                workspace_name,
                folder,
            } => self.open_folder_note_for_folder(workspace_name, folder),
            PaletteNoteTarget::DocumentNote {
                path,
                workspace_folders,
            } => {
                self.open_document(path);
                self.open_document_note_for_path_with_folders(path, workspace_folders.clone());
            }
        }
    }

    /// Return the current browser handle, clearing stale state left by a closed dialog.
    fn current_notes_browser(&self) -> Option<ActiveNotesBrowser> {
        let browser = self.imp().active_notes_browser.borrow().clone();
        if browser.as_ref().is_some_and(ActiveNotesBrowser::is_alive) {
            return browser;
        }
        self.imp().active_notes_browser.borrow_mut().take();
        self.set_notes_browser_actions_enabled(false);
        None
    }
}

/// Build the populated notes-browser chrome around the adaptive split view.
fn build_notes_browser_shell(
    dialog: &libadwaita::Dialog,
    split_view: &libadwaita::NavigationSplitView,
) -> gtk4::Box {
    let shell = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    install_dialog_escape_close(dialog, &shell);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header.set_margin_start(18);
    header.set_margin_end(18);
    header.set_margin_top(18);
    let title = gtk4::Label::new(Some("Notes"));
    title.set_halign(gtk4::Align::Start);
    title.set_hexpand(true);
    title.set_xalign(0.0);
    title.add_css_class("title-4");
    header.append(&title);

    let close_button = build_dialog_close_button(dialog);
    install_dialog_escape_close(dialog, &close_button);
    header.append(&close_button);
    shell.append(&header);
    shell.append(split_view);

    shell
}

/// Build the shared edit/render note surface used by document and folder notes.
#[must_use]
fn build_note_editor_surface(
    initial_text: &str,
    render_context: &MarkdownPreviewRenderContext,
    initial_mode: NoteViewMode,
    empty_preview_description: &'static str,
) -> (gtk4::Box, gtk4::TextView) {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);

    let stack = gtk4::Stack::new();
    stack.set_size_request(NOTE_EDITOR_SURFACE_WIDTH_SP, NOTE_EDITOR_SURFACE_HEIGHT_SP);
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_hhomogeneous(true);
    stack.set_vhomogeneous(true);
    accessibility::set_role(&stack, gtk4::AccessibleRole::Group);
    accessibility::set_labelled_description(
        &stack,
        "Note body view",
        "Switch between editing the note body and reading the rendered preview",
    );

    let switcher = gtk4::StackSwitcher::new();
    switcher.set_stack(Some(&stack));
    switcher.set_hexpand(true);
    switcher.set_halign(gtk4::Align::Fill);
    accessibility::set_labelled_description(
        &switcher,
        "Note view mode",
        "Choose whether to edit the note or read the rendered preview",
    );
    content.append(&switcher);

    let note_view = gtk4::TextView::new();
    note_view.set_size_request(NOTE_EDITOR_SURFACE_WIDTH_SP, NOTE_EDITOR_SURFACE_HEIGHT_SP);
    note_view.set_wrap_mode(gtk4::WrapMode::WordChar);
    note_view.set_vexpand(true);
    // TextView margin properties act as inner padding for the editable
    // document, so this gives the note body breathing room inside the box
    // instead of just pushing the whole widget farther from neighboring rows.
    note_view.set_left_margin(NOTE_EDITOR_TEXT_MARGIN_HORIZONTAL_SP);
    note_view.set_right_margin(NOTE_EDITOR_TEXT_MARGIN_HORIZONTAL_SP);
    note_view.set_top_margin(NOTE_EDITOR_TEXT_MARGIN_VERTICAL_SP);
    note_view.set_bottom_margin(NOTE_EDITOR_TEXT_MARGIN_VERTICAL_SP);
    note_view.buffer().set_text(initial_text);
    accessibility::set_labelled_description(
        &note_view,
        "Note body editor",
        "Editable note body text",
    );
    accessibility::set_multi_line(&note_view, true);

    let note_scroll = gtk4::ScrolledWindow::builder()
        .width_request(NOTE_EDITOR_SURFACE_WIDTH_SP)
        .height_request(NOTE_EDITOR_SURFACE_HEIGHT_SP)
        .min_content_width(NOTE_EDITOR_SURFACE_WIDTH_SP)
        .max_content_width(NOTE_EDITOR_SURFACE_WIDTH_SP)
        .min_content_height(NOTE_EDITOR_SURFACE_HEIGHT_SP)
        .max_content_height(NOTE_EDITOR_SURFACE_HEIGHT_SP)
        .propagate_natural_width(false)
        .propagate_natural_height(false)
        .vexpand(true)
        .child(&note_view)
        .build();
    stack.add_titled(&note_scroll, Some("edit"), "Edit");

    let preview = LushtextMarkdownPreview::new();
    preview.set_size_request(NOTE_EDITOR_SURFACE_WIDTH_SP, NOTE_EDITOR_SURFACE_HEIGHT_SP);
    preview.set_hexpand(true);
    preview.set_vexpand(true);
    preview.set_scroller_content_size(NOTE_EDITOR_SURFACE_WIDTH_SP, NOTE_EDITOR_SURFACE_HEIGHT_SP);
    preview
        .text_view()
        .set_size_request(NOTE_EDITOR_SURFACE_WIDTH_SP, NOTE_EDITOR_SURFACE_HEIGHT_SP);
    preview
        .text_view()
        .set_left_margin(NOTE_EDITOR_TEXT_MARGIN_HORIZONTAL_SP);
    preview
        .text_view()
        .set_right_margin(NOTE_EDITOR_TEXT_MARGIN_HORIZONTAL_SP);
    preview
        .text_view()
        .set_top_margin(NOTE_EDITOR_TEXT_MARGIN_VERTICAL_SP);
    preview
        .text_view()
        .set_bottom_margin(NOTE_EDITOR_TEXT_MARGIN_VERTICAL_SP);
    let rendered_note_view = preview.text_view();
    accessibility::set_labelled_description(
        &rendered_note_view,
        "Rendered note preview",
        "Read-only rendered view of the note body",
    );
    accessibility::set_read_only(&rendered_note_view, true);
    accessibility::set_multi_line(&rendered_note_view, true);
    preview.show_content_placeholder(empty_preview_description);
    // Non-empty notes may open directly in Render, and the hidden Render page
    // still participates in stack measurement when Edit starts first. Render
    // once up front so either initial mode has stable geometry without doing a
    // second markdown pass during setup.
    if !initial_text.trim().is_empty() {
        render_note_preview(
            &preview,
            &note_view.buffer(),
            render_context,
            empty_preview_description,
        );
    }
    stack.add_titled(&preview, Some("render"), "Render");

    match initial_mode {
        NoteViewMode::Edit => {
            stack.set_visible_child_name("edit");
            accessibility::set_value_text(&stack, "Edit mode");
        }
        NoteViewMode::Render => {
            stack.set_visible_child_name("render");
            accessibility::set_value_text(&stack, "Render mode");
        }
    }

    let buffer = note_view.buffer();
    let preview_for_render = preview;
    let render_context_clone = render_context.clone();
    stack.connect_notify_local(Some("visible-child-name"), move |stack, _| {
        if stack.visible_child_name().as_deref() == Some("render") {
            accessibility::set_value_text(stack, "Render mode");
            let preview = preview_for_render.clone();
            let buffer = buffer.clone();
            let render_context = render_context_clone.clone();
            glib::idle_add_local_once(move || {
                render_note_preview(
                    &preview,
                    &buffer,
                    &render_context,
                    empty_preview_description,
                );
            });
        } else {
            accessibility::set_value_text(stack, "Edit mode");
        }
    });

    content.append(&stack);
    (content, note_view)
}

/// Wires the Save response to the note buffer's normalized dirty-state policy.
///
/// Runs on the GTK main thread and installs a `TextBuffer::changed` handler; the
/// handler weakly references the dialog so late changes cannot keep it alive.
fn install_note_save_response_state(
    dialog: &libadwaita::AlertDialog,
    buffer: &gtk4::TextBuffer,
    presentation: NoteEditorPresentation,
    initial_text: &str,
) {
    dialog.set_response_enabled(RESPONSE_SAVE, presentation.save_enabled_for(initial_text));

    let refresh = NoteSaveResponseRefresh::new(dialog, presentation);
    buffer.connect_changed(move |buffer| {
        refresh.schedule(buffer);
    });
}

/// Main-loop state for coalescing Save sensitivity refreshes.
#[derive(Clone)]
struct NoteSaveResponseRefresh {
    dialog_weak: glib::WeakRef<libadwaita::AlertDialog>,
    presentation: Rc<NoteEditorPresentation>,
    debounce: Debounce,
    in_flight: Rc<Cell<bool>>,
    rerun_requested: Rc<Cell<bool>>,
}

impl NoteSaveResponseRefresh {
    /// Create a refresh state object tied to one note editor dialog.
    fn new(dialog: &libadwaita::AlertDialog, presentation: NoteEditorPresentation) -> Self {
        Self {
            dialog_weak: dialog.downgrade(),
            presentation: Rc::new(presentation),
            debounce: Debounce::new(),
            in_flight: Rc::new(Cell::new(false)),
            rerun_requested: Rc::new(Cell::new(false)),
        }
    }

    /// Schedule a trailing Save sensitivity refresh after live text edits.
    fn schedule(&self, buffer: &gtk4::TextBuffer) {
        let refresh = self.clone();
        self.debounce.schedule(
            buffer,
            Duration::from_millis(NOTE_SAVE_RESPONSE_REFRESH_DEBOUNCE_MS),
            move |buffer, _| refresh.queue(buffer),
        );
    }

    /// Recompute Save sensitivity from the newest buffer snapshot without blocking input.
    ///
    /// At most one large-buffer snapshot runs at a time. If rapid typing changes
    /// the buffer while a snapshot is collecting chunks, the stale result is
    /// ignored and exactly one follow-up snapshot reads the latest buffer text.
    fn queue(&self, buffer: gtk4::TextBuffer) {
        if self.dialog_weak.upgrade().is_none() {
            return;
        }

        if self.in_flight.replace(true) {
            self.rerun_requested.set(true);
            return;
        }

        let refresh = self.clone();
        let buffer_for_rerun = buffer.clone();
        snapshot_note_buffer_text(buffer, move |text| {
            let rerun_requested = refresh.rerun_requested.replace(false);
            refresh.in_flight.set(false);

            if rerun_requested {
                if refresh.dialog_weak.upgrade().is_some() {
                    refresh.queue(buffer_for_rerun);
                }
                return;
            }

            let Some(dialog) = refresh.dialog_weak.upgrade() else {
                return;
            };
            dialog
                .set_response_enabled(RESPONSE_SAVE, refresh.presentation.save_enabled_for(&text));
        });
    }
}

/// Render one note body into the shared markdown preview widget.
fn render_note_preview(
    preview: &LushtextMarkdownPreview,
    buffer: &gtk4::TextBuffer,
    render_context: &MarkdownPreviewRenderContext,
    empty_preview_description: &'static str,
) {
    let preview = preview.clone();
    let render_context = render_context.clone();
    snapshot_note_buffer_text(buffer.clone(), move |text| {
        if text.trim().is_empty() {
            preview.show_content_placeholder(empty_preview_description);
        } else {
            preview.render_markdown_with_context(&text, &render_context);
        }
    });
}

/// Snapshot note editor text without monopolizing the GTK main loop.
fn snapshot_note_buffer_text<F: FnOnce(String) + 'static>(buffer: gtk4::TextBuffer, callback: F) {
    if buffer_snapshot::buffer_requires_chunked_snapshot(&buffer) {
        buffer_snapshot::snapshot_buffer_text_async(buffer, callback);
    } else {
        callback(buffer_snapshot::snapshot_buffer_text_direct(&buffer));
    }
}

/// Build an explicit empty state when the current scope has no notes yet.
fn build_empty_notes_dialog() -> libadwaita::Dialog {
    let dialog = libadwaita::Dialog::builder()
        .title("Notes")
        .content_width(EMPTY_NOTES_BROWSER_WIDTH_SP)
        .content_height(EMPTY_NOTES_BROWSER_HEIGHT_SP)
        // `AdwStatusPage` has a narrow natural request; following content size
        // recreates the cramped empty-state column instead of this readable target.
        .follows_content_size(false)
        .build();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    install_dialog_escape_close(&dialog, &content);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let title = gtk4::Label::new(Some("Notes"));
    title.set_halign(gtk4::Align::Start);
    title.set_hexpand(true);
    title.add_css_class("title-4");
    header.append(&title);
    let close_button = build_dialog_close_button(&dialog);
    install_dialog_escape_close(&dialog, &close_button);
    header.append(&close_button);
    content.append(&header);

    let status = libadwaita::StatusPage::builder()
        .icon_name("text-x-generic-symbolic")
        .title("No notes yet")
        .description(
            "Bookmarks, document notes, and folder notes will appear here once you save one.",
        )
        .build();
    accessibility::set_role(&status, gtk4::AccessibleRole::Status);
    accessibility::set_labelled_description(
        &status,
        "No notes yet",
        "Bookmarks, document notes, and folder notes will appear here once you save one.",
    );
    status.set_hexpand(true);
    status.set_vexpand(true);
    content.append(&status);
    dialog.set_child(Some(&content));
    focus_after_present(&close_button);
    dialog
}

/// Build the browse rail used by the unified notes browser.
fn build_notes_sidebar(
    dialog: &libadwaita::Dialog,
    search_entry: &gtk4::SearchEntry,
    sidebar: &libadwaita::Sidebar,
    limit_label: &gtk4::Label,
) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    install_dialog_escape_close(dialog, &content);

    content.append(search_entry);

    let scroll = gtk4::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(sidebar)
        .build();
    content.append(&scroll);
    content.append(limit_label);

    content
}

/// Build the preview page used by the unified notes browser.
fn build_notes_preview_page(
    dialog: &libadwaita::Dialog,
    back_button: &gtk4::Button,
    preview_title: &gtk4::Label,
    preview_meta: &gtk4::Label,
    preview_stack: &gtk4::Stack,
    open_button: &gtk4::Button,
) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    install_dialog_escape_close(dialog, &content);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header.append(back_button);

    let title_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    title_box.set_hexpand(true);
    title_box.append(preview_title);
    title_box.append(preview_meta);
    header.append(&title_box);
    content.append(&header);

    content.append(preview_stack);

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    actions.set_halign(gtk4::Align::End);
    actions.append(open_button);
    content.append(&actions);

    content
}

impl NotesBrowserEntry {
    /// Wrap one shared note source row for browser-specific preview behavior.
    fn from(note: PaletteNoteEntry) -> Self {
        Self { note }
    }

    /// User-facing row title used in the browser list.
    #[must_use]
    fn row_title(&self) -> String {
        self.note.title.clone()
    }

    /// Secondary row text used for scope and location metadata.
    #[must_use]
    fn row_subtitle(&self) -> String {
        self.note.subtitle.clone()
    }

    /// Optional row detail showing the first meaningful line of note text.
    #[must_use]
    fn row_detail(&self) -> Option<String> {
        self.note.detail.clone()
    }

    /// Title shown in the preview header for the selected note.
    #[must_use]
    fn preview_title(&self) -> String {
        self.row_title()
    }

    /// Secondary preview metadata shown under the selected note title.
    #[must_use]
    fn preview_meta(&self) -> String {
        self.row_subtitle()
    }

    /// Note text rendered into the preview widget.
    #[must_use]
    fn note_text(&self) -> &str {
        self.note.note_text()
    }

    /// Render context used by the shared markdown preview widget.
    #[must_use]
    fn render_context(&self) -> MarkdownPreviewRenderContext {
        match &self.note.target {
            PaletteNoteTarget::FolderNote { folder, .. } => {
                MarkdownPreviewRenderContext::new(None, vec![folder.clone()])
            }
            PaletteNoteTarget::Bookmark {
                path,
                workspace_folders,
                ..
            }
            | PaletteNoteTarget::DocumentNote {
                path,
                workspace_folders,
            } => MarkdownPreviewRenderContext::new(Some(path.clone()), workspace_folders.clone()),
        }
    }

    /// Symbolic icon used by the grouped Adwaita sidebar item.
    #[must_use]
    fn sidebar_icon_name(&self) -> &'static str {
        match self.note.target {
            PaletteNoteTarget::Bookmark { .. } => "bookmark-new-symbolic",
            PaletteNoteTarget::FolderNote { .. } => "folder-symbolic",
            PaletteNoteTarget::DocumentNote { .. } => "text-x-generic-symbolic",
        }
    }

    /// Return whether this row belongs in the supplemental open-tab section.
    #[must_use]
    fn is_open_tab(&self) -> bool {
        self.note.category == PaletteNoteCategory::OpenTabs
    }

    /// Return whether this entry matches one prepared search query.
    fn matches_query(&self, query: &NotesBrowserQuery) -> bool {
        query.matches(&self.row_title())
            || query.matches(&self.row_subtitle())
            || query.matches(self.note_text())
    }
}

impl NotesBrowserQuery {
    /// Prepare one non-empty query for repeated row checks.
    #[must_use]
    fn new(query: &str) -> Option<Self> {
        let lower_text = query.trim().to_lowercase();
        if lower_text.is_empty() {
            return None;
        }

        let needle: Vec<_> = lower_text.chars().collect();
        let prefix = Self::prefix_table(&needle);
        Some(Self { needle, prefix })
    }

    /// Build the KMP prefix table once per query instead of once per note body.
    fn prefix_table(needle: &[char]) -> Vec<usize> {
        let mut prefix = vec![0; needle.len()];
        let mut matched = 0;
        for index in 1..needle.len() {
            while matched > 0 && needle[index] != needle[matched] {
                matched = prefix[matched - 1];
            }
            if needle[index] == needle[matched] {
                matched += 1;
                prefix[index] = matched;
            }
        }
        prefix
    }

    /// Match without allocating a lowercased copy of large note bodies.
    fn matches(&self, haystack: &str) -> bool {
        if haystack.is_empty() {
            return false;
        }

        let mut matched = 0;
        for character in haystack.chars().flat_map(char::to_lowercase) {
            while matched > 0 && character != self.needle[matched] {
                matched = self.prefix[matched - 1];
            }
            if character == self.needle[matched] {
                matched += 1;
                if matched == self.needle.len() {
                    return true;
                }
            }
        }
        false
    }
}

impl NotesBrowserState {
    /// Refresh the preview pane for one selected sidebar item.
    fn refresh_preview(state: &Rc<Self>, index: Option<usize>, user_selected: bool) {
        let generation = state.advance_preview_generation();
        let Some(index) = index else {
            state.show_unselected_preview();
            return;
        };

        let Some(entry_index) = state.filtered_indices.borrow().get(index).copied() else {
            state.show_unselected_preview();
            return;
        };
        let Some(entry) = state.all_entries.get(entry_index) else {
            state.show_unselected_preview();
            return;
        };

        state.preview_title.set_label(&entry.preview_title());
        state.preview_meta.set_label(&entry.preview_meta());
        if matches!(&entry.note.target, PaletteNoteTarget::Bookmark { .. }) {
            Self::refresh_bookmark_preview(state, entry, generation);
        } else if entry.note_text().trim().is_empty() {
            state.show_markdown_placeholder("This note is empty.");
        } else {
            state.show_markdown_preview();
            state
                .markdown_preview
                .render_markdown_with_context(entry.note_text(), &entry.render_context());
        }
        state.open_button.set_sensitive(true);
        accessibility::set_disabled(&state.open_button, false);
        accessibility::set_value_text(
            &state.open_button,
            &format!("Open {}", entry.preview_title()),
        );
        accessibility::set_value_text(&state.preview_stack, &entry.preview_title());

        if user_selected {
            // `show-content` is only visible while collapsed, but setting it
            // before the adaptive layout settles preserves the user's
            // navigation request during resize and widget-test transitions.
            state.split_view.set_show_content(true);
        }
    }

    /// Advance the preview token that async bookmark loads must match.
    fn advance_preview_generation(&self) -> u32 {
        let generation = self.preview_generation.get().wrapping_add(1);
        self.preview_generation.set(generation);
        generation
    }

    /// Reset the preview pane to the initial no-selection state.
    fn show_unselected_preview(&self) {
        self.preview_title.set_label("Select a note");
        self.preview_meta
            .set_label("Choose a bookmark, folder note, or document note to preview it here.");
        self.show_markdown_placeholder("Select a note to preview its details.");
        self.open_button.set_sensitive(false);
        accessibility::set_disabled(&self.open_button, true);
        accessibility::set_value_text(&self.open_button, "No note selected");
        accessibility::set_value_text(&self.preview_stack, "No note selected");
    }

    /// Switch to the Markdown/status preview child and clear hidden raw state.
    fn show_markdown_preview(&self) {
        self.raw_preview_buffer.set_text("");
        self.preview_stack
            .set_visible_child_name(NOTES_PREVIEW_MARKDOWN_CHILD);
    }

    /// Show a status-style placeholder in the Markdown child.
    fn show_markdown_placeholder(&self, description: &str) {
        self.show_markdown_preview();
        self.markdown_preview.show_placeholder(description);
    }

    /// Show plain text inside the Markdown child to preserve preview allocation.
    fn show_markdown_content_placeholder(&self, description: &str) {
        self.show_markdown_preview();
        self.markdown_preview.show_content_placeholder(description);
    }

    /// Resolve and render a bookmark preview for the selected row.
    fn refresh_bookmark_preview(state: &Rc<Self>, entry: &NotesBrowserEntry, generation: u32) {
        let PaletteNoteTarget::Bookmark { path, line, .. } = &entry.note.target else {
            return;
        };

        let presentation = bookmark_excerpt::presentation_for_path(path);
        if let Some(editor) = state.window.open_editor_for_path(path) {
            state.render_bookmark_excerpt_state(
                entry,
                live_bookmark_excerpt_for_editor(&editor, *line, presentation),
            );
            return;
        }

        state.render_bookmark_excerpt_state(
            entry,
            bookmark_excerpt::BookmarkExcerptState::Loading { presentation },
        );

        let path = path.clone();
        let path_for_load = path.clone();
        let line = *line;
        let state_weak = Rc::downgrade(state);
        spawn_blocking_then(
            (),
            move || {
                delay_bookmark_excerpt_preview_for_test();
                bookmark_excerpt::load_from_path(&path_for_load, line)
            },
            move |(), result| {
                let Some(state) = state_weak.upgrade() else {
                    return;
                };
                state.apply_bookmark_preview_completion(generation, &path, line, result);
            },
        );
    }

    /// Apply a closed-file preview only if it still belongs to the selected row.
    fn apply_bookmark_preview_completion(
        &self,
        generation: u32,
        path: &Path,
        line: u32,
        result: bookmark_excerpt::BookmarkExcerptState,
    ) {
        if self.preview_generation.get() != generation
            || !self.selected_bookmark_matches(path, line)
        {
            return;
        }

        let Some(entry_index) = self.selected_entry_index() else {
            return;
        };
        let Some(entry) = self.all_entries.get(entry_index) else {
            return;
        };
        self.render_bookmark_excerpt_state(entry, result);
    }

    /// Render one resolved bookmark preview state into the active preview child.
    fn render_bookmark_excerpt_state(
        &self,
        entry: &NotesBrowserEntry,
        state: bookmark_excerpt::BookmarkExcerptState,
    ) {
        match state {
            bookmark_excerpt::BookmarkExcerptState::Loading { .. } => {
                self.show_markdown_content_placeholder("Loading bookmark preview...");
            }
            bookmark_excerpt::BookmarkExcerptState::Unavailable(unavailable) => {
                self.show_markdown_content_placeholder(bookmark_unavailable_description(
                    unavailable.reason,
                ));
            }
            bookmark_excerpt::BookmarkExcerptState::Ready(excerpt) => match excerpt.presentation {
                bookmark_excerpt::BookmarkExcerptPresentation::Markdown => {
                    self.show_markdown_preview();
                    self.markdown_preview.render_markdown_with_context(
                        &excerpt.body_text_with_markers(),
                        &entry.render_context(),
                    );
                }
                bookmark_excerpt::BookmarkExcerptPresentation::PlainText => {
                    self.render_raw_bookmark_excerpt(&excerpt);
                }
            },
        }
    }

    /// Render a plain-text bookmark excerpt into the raw preview surface.
    fn render_raw_bookmark_excerpt(&self, excerpt: &bookmark_excerpt::BookmarkExcerpt) {
        self.markdown_preview.clear();
        let formatted = format_raw_bookmark_excerpt(excerpt);
        self.raw_preview_buffer.set_text(&formatted.text);
        let tag = ensure_raw_preview_target_tag(&self.raw_preview_buffer);
        let start = self
            .raw_preview_buffer
            .iter_at_offset(formatted.target_start);
        let end = self.raw_preview_buffer.iter_at_offset(formatted.target_end);
        self.raw_preview_buffer.apply_tag(&tag, &start, &end);
        self.preview_stack
            .set_visible_child_name(NOTES_PREVIEW_RAW_CHILD);
    }

    /// Return the backing entry index for the currently selected sidebar item.
    fn selected_entry_index(&self) -> Option<usize> {
        let selected = sidebar_item_index(self.sidebar.selected_item())?;
        self.filtered_indices.borrow().get(selected).copied()
    }

    /// Check that an async bookmark completion still belongs to the selected row.
    fn selected_bookmark_matches(&self, path: &Path, line: u32) -> bool {
        let Some(entry_index) = self.selected_entry_index() else {
            return false;
        };
        matches!(
            self.all_entries.get(entry_index).map(|entry| &entry.note.target),
            Some(PaletteNoteTarget::Bookmark {
                path: selected_path,
                line: selected_line,
                ..
            }) if selected_path == path && *selected_line == line
        )
    }

    /// Open the currently selected note through the same window workflows used elsewhere.
    fn open_selected(&self) {
        let Some(entry_index) = self.selected_entry_index() else {
            return;
        };
        let Some(entry) = self.all_entries.get(entry_index) else {
            return;
        };

        self.dialog.close();
        activate_notes_browser_entry(&self.window, entry);
    }
}

/// Extract live source context from an open editor without snapshotting the full buffer.
fn live_bookmark_excerpt_for_editor(
    editor: &LushtextEditorPage,
    target_line: u32,
    presentation: bookmark_excerpt::BookmarkExcerptPresentation,
) -> bookmark_excerpt::BookmarkExcerptState {
    let buffer = editor.buffer();
    let line_count = u32::try_from(buffer.line_count().max(1)).unwrap_or(u32::MAX);
    if target_line >= line_count {
        return bookmark_excerpt::BookmarkExcerptState::Unavailable(
            bookmark_excerpt::BookmarkExcerptUnavailable {
                presentation,
                reason: bookmark_excerpt::BookmarkExcerptUnavailableReason::LineOutOfRange,
            },
        );
    }

    let before =
        u32::try_from(bookmark_excerpt::BOOKMARK_EXCERPT_CONTEXT_BEFORE_LINES).unwrap_or(u32::MAX);
    let after =
        u32::try_from(bookmark_excerpt::BOOKMARK_EXCERPT_CONTEXT_AFTER_LINES).unwrap_or(u32::MAX);
    let first_line = target_line.saturating_sub(before);
    let last_line = target_line
        .saturating_add(after)
        .min(line_count.saturating_sub(1));

    let mut lines = Vec::new();
    for line in first_line..=last_line {
        lines.push(buffer_line_text(&buffer, line, line_count));
    }

    bookmark_excerpt::extract_from_context_lines(
        presentation,
        first_line,
        target_line,
        lines,
        first_line > 0,
        last_line.saturating_add(1) < line_count,
    )
}

/// Copy one bounded line from a `GtkTextBuffer`.
fn buffer_line_text(buffer: &sourceview5::Buffer, line: u32, line_count: u32) -> String {
    let start = buffer
        .iter_at_line(i32::try_from(line).unwrap_or(i32::MAX))
        .unwrap_or_else(|| buffer.end_iter());
    let line_end = if line.saturating_add(1) < line_count {
        buffer
            .iter_at_line(i32::try_from(line.saturating_add(1)).unwrap_or(i32::MAX))
            .unwrap_or_else(|| buffer.end_iter())
    } else {
        buffer.end_iter()
    };
    let mut capped_end = start;
    capped_end.forward_chars(
        i32::try_from(bookmark_excerpt::BOOKMARK_EXCERPT_LINE_CHAR_LIMIT.saturating_add(1))
            .unwrap_or(i32::MAX),
    );
    let end = if capped_end.offset() < line_end.offset() {
        capped_end
    } else {
        line_end
    };
    buffer
        .text(&start, &end, true)
        .trim_end_matches('\n')
        .trim_end_matches('\r')
        .to_string()
}

/// Ensure the raw bookmark target-line tag exists in the given buffer.
fn ensure_raw_preview_target_tag(buffer: &gtk4::TextBuffer) -> gtk4::TextTag {
    let table = buffer.tag_table();
    if let Some(tag) = table.lookup(NOTES_RAW_BOOKMARK_TARGET_TAG) {
        return tag;
    }

    let tag = gtk4::TextTag::new(Some(NOTES_RAW_BOOKMARK_TARGET_TAG));
    tag.set_weight(pango::Weight::Bold.into_glib());
    table.add(&tag);
    tag
}

/// Formatted raw bookmark body plus text-buffer offsets for target emphasis.
struct RawBookmarkExcerptText {
    /// Text inserted into the raw preview buffer.
    text: String,
    /// Character offset where the target line starts.
    target_start: i32,
    /// Character offset immediately after the target line.
    target_end: i32,
}

/// Render raw source context with line numbers and a target-line marker.
fn format_raw_bookmark_excerpt(
    excerpt: &bookmark_excerpt::BookmarkExcerpt,
) -> RawBookmarkExcerptText {
    let line_number_width = excerpt
        .lines
        .last()
        .map_or(1, |line| line.number.saturating_add(1).to_string().len())
        .max(2);
    let mut text = String::new();
    let mut target_start = 0;
    let mut target_end = 0;

    if excerpt.window.truncation.before {
        push_raw_preview_line(&mut text, "... earlier bookmark context omitted ...");
    }

    for (index, line) in excerpt.lines.iter().enumerate() {
        if index == excerpt.window.target_line_index {
            target_start = raw_preview_offset(&text);
        }

        let marker = if index == excerpt.window.target_line_index {
            ">"
        } else {
            " "
        };
        let line_number = line.number.saturating_add(1);
        push_raw_preview_line(
            &mut text,
            &format!("{marker} {line_number:>line_number_width$} | {}", line.text),
        );

        if index == excerpt.window.target_line_index {
            target_end = raw_preview_offset(&text).saturating_sub(1);
        }
    }

    if excerpt.window.truncation.after {
        push_raw_preview_line(&mut text, "... later bookmark context omitted ...");
    }

    RawBookmarkExcerptText {
        text,
        target_start,
        target_end,
    }
}

fn push_raw_preview_line(text: &mut String, line: &str) {
    text.push_str(line);
    text.push('\n');
}

fn raw_preview_offset(text: &str) -> i32 {
    i32::try_from(text.chars().count()).unwrap_or(i32::MAX)
}

fn bookmark_unavailable_description(
    reason: bookmark_excerpt::BookmarkExcerptUnavailableReason,
) -> &'static str {
    match reason {
        bookmark_excerpt::BookmarkExcerptUnavailableReason::MissingOrUnreadable => {
            "Bookmark preview unavailable: the file is missing or cannot be read."
        }
        bookmark_excerpt::BookmarkExcerptUnavailableReason::BinaryOrUnsupported => {
            "Bookmark preview unavailable: this file is not UTF-8 text."
        }
        bookmark_excerpt::BookmarkExcerptUnavailableReason::TooLargeToPreview => {
            "Bookmark preview unavailable: this file is too large to preview."
        }
        bookmark_excerpt::BookmarkExcerptUnavailableReason::LineBeyondPreviewBudget => {
            "Bookmark preview unavailable: the bookmarked line is beyond the preview budget."
        }
        bookmark_excerpt::BookmarkExcerptUnavailableReason::LineOutOfRange => {
            "Bookmark preview unavailable: the bookmarked line is no longer in this file."
        }
    }
}

/// Sleep only under `test-utils` so widget tests can exercise stale completions.
fn delay_bookmark_excerpt_preview_for_test() {
    #[cfg(feature = "test-utils")]
    {
        let delay_ms = BOOKMARK_EXCERPT_PREVIEW_DELAY_MS.load(Ordering::Acquire);
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }
}

/// Debounce browser search so large note sets do not rebuild on every keystroke.
fn schedule_notes_browser_search(state: &Rc<NotesBrowserState>, query: String) {
    if query.is_empty() {
        let _ = state.search_debounce.invalidate();
        rebuild_notes_browser_sidebar(state, "");
        return;
    }
    let state_weak = Rc::downgrade(state);
    state.search_debounce.schedule(
        &state.search_entry,
        Duration::from_millis(150),
        move |_, _| {
            let Some(state) = state_weak.upgrade() else {
                return;
            };
            rebuild_notes_browser_sidebar(&state, &query);
        },
    );
}

/// Rebuild the sectioned sidebar items shown by the unified notes browser.
fn rebuild_notes_browser_sidebar(state: &Rc<NotesBrowserState>, query: &str) {
    state.sidebar.remove_all();

    let prepared_query = NotesBrowserQuery::new(query);
    let mut matching_indices =
        Vec::with_capacity(state.all_entries.len().min(NOTES_BROWSER_RENDER_LIMIT));
    let mut truncated = false;
    for (index, entry) in state.all_entries.iter().enumerate() {
        if prepared_query
            .as_ref()
            .is_some_and(|query| !entry.matches_query(query))
        {
            continue;
        }
        if matching_indices.len() == NOTES_BROWSER_RENDER_LIMIT {
            truncated = true;
            break;
        }
        matching_indices.push(index);
    }

    if matching_indices.is_empty() {
        state.limit_label.set_visible(false);
        *state.filtered_indices.borrow_mut() = Vec::new();
        NotesBrowserState::refresh_preview(state, None, false);
        return;
    }
    if truncated {
        let message = format!(
            "Showing first {NOTES_BROWSER_RENDER_LIMIT} matches. Refine search to narrow results."
        );
        state.limit_label.set_label(&message);
        accessibility::set_label(&state.limit_label, &message);
    }
    state.limit_label.set_visible(truncated);

    let grouped_indices =
        append_notes_sidebar_sections(&state.sidebar, &state.all_entries, &matching_indices);
    *state.filtered_indices.borrow_mut() = grouped_indices;

    state.sidebar.set_selected(0);
    NotesBrowserState::refresh_preview(state, Some(0), false);
}

/// Append note entries as semantic Adwaita sidebar sections and return the
/// exact flat order used for selection lookup.
fn append_notes_sidebar_sections(
    sidebar: &libadwaita::Sidebar,
    all_entries: &[NotesBrowserEntry],
    matching_indices: &[usize],
) -> Vec<usize> {
    let mut ordered_indices = Vec::with_capacity(matching_indices.len());
    append_note_sidebar_section(
        sidebar,
        "Bookmarks",
        matching_indices.iter().copied().filter(|index| {
            all_entries.get(*index).is_some_and(|entry| {
                entry.note.category == PaletteNoteCategory::Bookmarks && !entry.is_open_tab()
            })
        }),
        all_entries,
        &mut ordered_indices,
    );
    append_note_sidebar_section(
        sidebar,
        "Folder Notes",
        matching_indices.iter().copied().filter(|index| {
            all_entries
                .get(*index)
                .is_some_and(|entry| entry.note.category == PaletteNoteCategory::FolderNotes)
        }),
        all_entries,
        &mut ordered_indices,
    );
    append_note_sidebar_section(
        sidebar,
        "Document Notes",
        matching_indices.iter().copied().filter(|index| {
            all_entries.get(*index).is_some_and(|entry| {
                entry.note.category == PaletteNoteCategory::DocumentNotes && !entry.is_open_tab()
            })
        }),
        all_entries,
        &mut ordered_indices,
    );
    append_note_sidebar_section(
        sidebar,
        "Open Tabs",
        matching_indices.iter().copied().filter(|index| {
            all_entries
                .get(*index)
                .is_some_and(NotesBrowserEntry::is_open_tab)
        }),
        all_entries,
        &mut ordered_indices,
    );
    ordered_indices
}

/// Add one non-empty Notes browser section to the sidebar.
fn append_note_sidebar_section(
    sidebar: &libadwaita::Sidebar,
    title: &str,
    indices: impl Iterator<Item = usize>,
    all_entries: &[NotesBrowserEntry],
    ordered_indices: &mut Vec<usize>,
) {
    let section = libadwaita::SidebarSection::new();
    section.set_title(Some(title));

    let start_len = ordered_indices.len();
    for index in indices {
        let Some(entry) = all_entries.get(index) else {
            continue;
        };
        section.append(build_notes_sidebar_item(entry));
        ordered_indices.push(index);
    }

    if ordered_indices.len() > start_len {
        sidebar.append(section);
    }
}

/// Build one Adwaita sidebar item while preserving the old row's searchable
/// metadata and preview line in the visible subtitle/tooltip.
fn build_notes_sidebar_item(entry: &NotesBrowserEntry) -> libadwaita::SidebarItem {
    let subtitle = entry.row_detail().map_or_else(
        || entry.row_subtitle(),
        |detail| format!("{} · {detail}", entry.row_subtitle()),
    );
    libadwaita::SidebarItem::builder()
        .title(entry.row_title())
        .subtitle(subtitle.clone())
        .tooltip(subtitle)
        .icon_name(entry.sidebar_icon_name())
        .build()
}

/// Resolve an Adwaita sidebar item back to the flat backing vector index.
fn sidebar_item_index(item: Option<libadwaita::SidebarItem>) -> Option<usize> {
    item.and_then(|item| usize::try_from(item.index()).ok())
}

/// Route one browser row back through the appropriate window workflow.
fn activate_notes_browser_entry(window: &LushtextWindow, entry: &NotesBrowserEntry) {
    window.activate_palette_note_target(&entry.note.target);
}

/// Convert one concrete workspace into the only valid folder-note action shape.
fn folder_note_target_for_workspace(workspace: WorkspaceConfig) -> FolderNoteOpenTarget {
    let workspace_name = workspace.name;
    let folders = workspace
        .folders
        .into_iter()
        .map(|folder| folder.path().to_path_buf())
        .collect::<Vec<_>>();

    match folders.as_slice() {
        [] => FolderNoteOpenTarget::EmptyWorkspace { workspace_name },
        [folder] => FolderNoteOpenTarget::SingleFolder {
            workspace_name,
            folder: folder.clone(),
        },
        _ => FolderNoteOpenTarget::ChooseFolder {
            workspace_name,
            folders,
        },
    }
}

/// Parse only the syntax of a user-facing 1-based bookmark line.
///
/// Range and collision checks stay in the editor layer so failed edits leave the
/// live bookmark projection unchanged.
fn parse_bookmark_target_line(text: &str) -> Result<u32, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("Enter a line number.".to_string());
    }

    trimmed
        .parse::<u32>()
        .map_err(|_| "Line must be a whole number.".to_string())
}

/// Convert editor validation failures into dialog feedback.
fn bookmark_edit_error_message(error: &BookmarkEditError) -> String {
    match error {
        BookmarkEditError::NotFound => "That bookmark is no longer available.".to_string(),
        BookmarkEditError::LineOutOfRange {
            requested_line,
            max_line,
        } => format!("Line {requested_line} is outside this document. Use 1 through {max_line}."),
        BookmarkEditError::LineOccupied { line } => {
            format!("Line {line} already has another bookmark.")
        }
    }
}

/// Show bookmark-edit validation feedback and expose the failed field to assistive tech.
fn show_bookmark_edit_error(
    error_label: &gtk4::Label,
    invalid_line_row: Option<&libadwaita::EntryRow>,
    message: &str,
) {
    error_label.set_label(message);
    error_label.set_visible(true);
    accessibility::set_label(error_label, message);
    accessibility::set_hidden(error_label, false);
    accessibility::set_invalid(error_label, true);
    accessibility::announce_with_lane(error_label, message, accessibility::AnnouncementLane::Alert);
    if let Some(line_row) = invalid_line_row {
        accessibility::set_invalid(line_row, true);
    }
}

/// Hide bookmark-edit validation feedback and clear stale accessible error state.
fn clear_bookmark_edit_error(error_label: &gtk4::Label) {
    error_label.set_visible(false);
    error_label.set_label("");
    accessibility::set_label(error_label, "Bookmark edit feedback");
    accessibility::set_hidden(error_label, true);
    accessibility::set_invalid(error_label, false);
}

/// Build the base dialog used by bookmark browsers.
fn build_browser_dialog(title: &str) -> libadwaita::Dialog {
    let dialog = libadwaita::Dialog::builder()
        .title(title)
        .content_width(720)
        .content_height(480)
        .build();

    let content = browser_content_box(&dialog);
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let title_label = gtk4::Label::new(Some(title));
    title_label.set_halign(gtk4::Align::Start);
    title_label.set_hexpand(true);
    title_label.add_css_class("title-4");
    header.append(&title_label);

    header.append(&build_dialog_close_button(&dialog));
    content.prepend(&header);
    dialog
}

/// Build one compact close affordance for browser-style dialogs.
fn build_dialog_close_button(dialog: &libadwaita::Dialog) -> gtk4::Button {
    let close_button = gtk4::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Close")
        .build();
    accessibility::set_labelled_description(
        &close_button,
        "Close",
        "Close this dialog and return to the editor",
    );
    let dialog_weak = dialog.downgrade();
    close_button.connect_clicked(move |_| {
        if let Some(dialog) = dialog_weak.upgrade() {
            dialog.close();
        }
    });
    close_button
}

/// Close dialog content on Escape even when the focused child owns key handling.
fn install_dialog_escape_close(dialog: &libadwaita::Dialog, widget: &impl IsA<gtk4::Widget>) {
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let dialog_weak = dialog.downgrade();
    controller.connect_key_pressed(move |_, key, _, _| {
        if key != gtk4::gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        if let Some(dialog) = dialog_weak.upgrade() {
            dialog.close();
        }
        glib::Propagation::Stop
    });
    widget.as_ref().add_controller(controller);
}

/// Defer focus until after `AdwDialog::present()` realizes its child tree.
fn focus_after_present(widget: &impl IsA<gtk4::Widget>) {
    let widget_weak = widget.as_ref().downgrade();
    glib::idle_add_local_once(move || {
        if let Some(widget) = widget_weak.upgrade() {
            widget.grab_focus();
        }
    });
}

/// Return the vertical content box attached to a browse dialog.
#[must_use]
fn browser_content_box(dialog: &libadwaita::Dialog) -> gtk4::Box {
    if let Some(child) = dialog.child()
        && let Ok(content) = child.downcast::<gtk4::Box>()
    {
        return content;
    }

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    dialog.set_child(Some(&content));
    content
}

/// Rebuild the bookmark rows that match `query`.
fn rebuild_bookmark_rows(
    window: &LushtextWindow,
    dialog: &libadwaita::Dialog,
    rows_box: &gtk4::Box,
    bookmarks: &[bookmark_service::WorkspaceBookmark],
    query: &str,
) {
    clear_box_children(rows_box);

    let mut rendered = 0usize;
    let mut truncated = false;

    for bookmark in bookmarks
        .iter()
        .filter(|bookmark| bookmark_matches_query(bookmark, query))
    {
        if rendered >= NOTES_BROWSER_RENDER_LIMIT {
            truncated = true;
            break;
        }
        rendered = rendered.saturating_add(1);
        append_bookmark_browser_row(window, dialog, rows_box, bookmark.clone());
    }

    if rendered == 0 {
        rows_box.append(&empty_browser_label("No bookmarks match that search"));
    }

    if truncated {
        rows_box.append(&empty_browser_label(
            "Showing first 500 bookmark matches. Refine the search to narrow results.",
        ));
    }
}

fn append_bookmark_browser_row(
    window: &LushtextWindow,
    dialog: &libadwaita::Dialog,
    rows_box: &gtk4::Box,
    bookmark: bookmark_service::WorkspaceBookmark,
) {
    let button = gtk4::Button::new();
    button.add_css_class("flat");
    button.set_hexpand(true);
    button.set_halign(gtk4::Align::Fill);
    let bookmark_label = bookmark.display_label();
    let bookmark_location = format!(
        "{} · Line {}",
        bookmark.path.display(),
        bookmark.line.saturating_add(1)
    );
    button.set_child(Some(&browser_row_content(
        &bookmark_label,
        &bookmark_location,
        None,
    )));
    button.set_tooltip_text(Some(&bookmark_location));
    accessibility::set_labelled_description(
        &button,
        &format!("Open bookmark {bookmark_label}"),
        &bookmark_location,
    );

    let window = window.clone();
    let dialog_weak = dialog.downgrade();
    button.connect_clicked(move |_| {
        open_editor_at_line(&window, &bookmark.path, bookmark.line.saturating_add(1));
        if let Some(dialog) = dialog_weak.upgrade() {
            dialog.close();
        }
    });
    rows_box.append(&button);
}

/// Build the content widget used inside bookmark browser rows.
#[must_use]
fn browser_row_content(title: &str, subtitle: &str, detail: Option<&str>) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    content.set_margin_start(8);
    content.set_margin_end(8);
    content.set_margin_top(8);
    content.set_margin_bottom(8);

    let title_label = gtk4::Label::new(Some(title));
    title_label.set_halign(gtk4::Align::Start);
    title_label.set_xalign(0.0);
    title_label.add_css_class("heading");
    content.append(&title_label);

    let subtitle_label = gtk4::Label::new(Some(subtitle));
    subtitle_label.set_halign(gtk4::Align::Start);
    subtitle_label.set_xalign(0.0);
    subtitle_label.set_wrap(true);
    subtitle_label.add_css_class("dim-label");
    content.append(&subtitle_label);

    if let Some(detail) = detail {
        let detail_label = gtk4::Label::new(Some(detail));
        detail_label.set_halign(gtk4::Align::Start);
        detail_label.set_xalign(0.0);
        detail_label.set_wrap(true);
        detail_label.add_css_class("caption");
        content.append(&detail_label);
    }

    content
}

/// Remove every child from a vertical browser rows box before rebuilding it.
fn clear_box_children(rows_box: &gtk4::Box) {
    while let Some(child) = rows_box.first_child() {
        rows_box.remove(&child);
    }
}

/// Build the empty-state label shown when a browser search has no matches.
#[must_use]
fn empty_browser_label(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_halign(gtk4::Align::Center);
    label.add_css_class("dim-label");
    accessibility::set_role(&label, gtk4::AccessibleRole::Status);
    accessibility::set_label(&label, text);
    label
}

/// Filter bookmark rows by label, path, or 1-based line number.
#[must_use]
fn bookmark_matches_query(bookmark: &bookmark_service::WorkspaceBookmark, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }

    bookmark.display_label().to_lowercase().contains(&query)
        || bookmark
            .path
            .display()
            .to_string()
            .to_lowercase()
            .contains(&query)
        || bookmark.line.saturating_add(1).to_string().contains(&query)
}

/// Open a file at a specific 1-based line number and focus the editor.
fn open_editor_at_line(window: &LushtextWindow, path: &Path, line: u32) {
    window.open_document(path);

    let Some(editor) = window.active_editor() else {
        return;
    };

    let line_zero_based = line.saturating_sub(1);
    if editor.is_evicted() {
        editor.set_restore_position(line_zero_based, 0, line_zero_based.saturating_sub(3));
        window.reload_if_evicted();
    } else if editor.buffer().char_count() > 0 {
        let buffer = editor.buffer();
        let iter = buffer
            .iter_at_line(i32::try_from(line_zero_based).unwrap_or(i32::MAX))
            .unwrap_or_else(|| buffer.end_iter());
        buffer.place_cursor(&iter);
        let mark = buffer.create_mark(None, &iter, true);
        editor
            .source_view()
            .scroll_to_mark(&mark, 0.2, true, 0.0, 0.0);
        buffer.delete_mark(&mark);
    } else {
        editor.set_restore_position(line_zero_based, 0, line_zero_based.saturating_sub(3));
    }
    editor.source_view().grab_focus();
}
