// SPDX-License-Identifier: GPL-3.0-or-later

//! Bookmark and note workflows for the main window shell.
//!
//! This module keeps note-specific action handling, dialogs, persistence
//! scheduling, and workspace browse logic out of the generic document shell.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, AlertDialogExt, AlertDialogExtManual, SidebarItemExt};

use crate::model::note::{NoteViewMode, RichNoteBody, note_preview_line};
use crate::model::workspace::{WorkspaceConfig, WorkspaceScope};
use crate::services::{
    async_task, bookmark_service, document_note_service, json_store, workspace_note_service,
};
use crate::ui::editor_page::{
    BookmarkNavigationDirection, BookmarkToggleState, LushtextEditorPage,
};
use crate::ui::markdown_preview::{LushtextMarkdownPreview, MarkdownPreviewRenderContext};
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

/// Debounce interval for bookmark sidecar saves.
///
/// 200ms coalesces rapid line-shift edits into one filesystem write without
/// letting note state drift for long after the user pauses typing.
const NOTES_SAVE_DEBOUNCE_MS: u64 = 200;

/// Alert-dialog response IDs reused by the note workflows.
const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_SAVE: &str = "save";
const RESPONSE_CLEAR: &str = "clear";

/// Search scope title used by bookmark and notes browse dialogs.
const WORKSPACE_SCOPE_TITLE: &str = "Current Workspace";

/// Fixed notes browser width keeps the preview usable without turning the dialog
/// into a second full window.
const NOTES_BROWSER_WIDTH_SP: i32 = 980;
/// Fixed notes browser height leaves room for the list, preview, and action row.
const NOTES_BROWSER_HEIGHT_SP: i32 = 700;
/// Maximum note rows materialized into the Adwaita sidebar at once.
///
/// The full notes set is still loaded and searched, but building thousands of
/// GTK sidebar items in one pass can stall the main loop. Search refinements
/// let users narrow beyond this first responsive slice.
const NOTES_BROWSER_RENDER_LIMIT: usize = 500;

/// Stable width for the shared edit/render note surface inside note dialogs.
const NOTE_EDITOR_SURFACE_WIDTH_SP: i32 = 520;
/// Stable height for the edit/render stack, matching the editable page's
/// measured request so toggling render mode does not shrink note dialogs.
const NOTE_EDITOR_SURFACE_HEIGHT_SP: i32 = 300;
/// Shared horizontal text inset for edit and rendered note bodies.
const NOTE_EDITOR_TEXT_MARGIN_HORIZONTAL_SP: i32 = 12;
/// Shared vertical text inset for edit and rendered note bodies.
const NOTE_EDITOR_TEXT_MARGIN_VERTICAL_SP: i32 = 10;

/// One entry shown in the unified notes browser.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NotesBrowserEntry {
    Bookmark {
        workspace_name: String,
        workspace_root: PathBuf,
        path: PathBuf,
        line: u32,
        label: Option<String>,
    },
    Workspace {
        workspace_name: String,
        root: PathBuf,
        note: RichNoteBody,
    },
    Document {
        workspace_name: String,
        workspace_root: PathBuf,
        path: PathBuf,
        note: RichNoteBody,
    },
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
    /// Adwaita browse rail for bookmarks, workspace notes, and document notes.
    sidebar: libadwaita::Sidebar,
    /// Visible notice when the current result set is capped for responsiveness.
    limit_label: gtk4::Label,
    /// Header label for the selected note.
    preview_title: gtk4::Label,
    /// Secondary metadata label for the selected note.
    preview_meta: gtk4::Label,
    /// Shared markdown preview widget reused for every selected note.
    preview: LushtextMarkdownPreview,
    /// Open action for the selected note.
    open_button: gtk4::Button,
    /// Back button shown when the split view collapses.
    back_button: gtk4::Button,
    /// Complete set of notes covered by this browser session.
    all_entries: Vec<NotesBrowserEntry>,
    /// Entry indexes currently shown in the sidebar's grouped visual order.
    filtered_indices: RefCell<Vec<usize>>,
    /// Generation counter used to debounce search rebuilds without stale work.
    search_generation: Cell<u32>,
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
                if window.is_active_editor(&editor) {
                    window.refresh_notes_menu_state();
                }
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
        async_task::spawn_blocking_then(
            editor.clone(),
            move || {
                let data_dir = json_store::data_dir();
                bookmark_service::load_for_path(&data_dir, &path_for_load)
                    .map(|document| document.bookmarks)
            },
            move |editor, result| match result {
                Ok(bookmarks) => {
                    editor.load_bookmarks(&bookmarks);
                    if let Some(window) = window_weak.upgrade() {
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
            },
        );
    }

    /// Reset live note state after Save As so the new path starts from its own identity.
    pub(super) fn reset_notes_after_save_as(&self, editor: &LushtextEditorPage, path: &Path) {
        editor.clear_bookmarks();
        self.resolve_notes_for_editor(editor, path);
    }

    /// Migrate sidecar documents after an in-app sidebar rename.
    pub(super) fn migrate_note_sidecars_after_rename(&self, old_path: &Path, new_path: &Path) {
        let old_path = old_path.to_path_buf();
        let new_path = new_path.to_path_buf();
        let old_path_for_move = old_path.clone();
        let new_path_for_move = new_path.clone();
        let window_weak = self.downgrade();
        async_task::spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                let bookmark_count = bookmark_service::move_path_tree(
                    &data_dir,
                    &old_path_for_move,
                    &new_path_for_move,
                )?;
                let document_note_count = document_note_service::move_path_tree(
                    &data_dir,
                    &old_path_for_move,
                    &new_path_for_move,
                )?;
                let workspace_note_count = workspace_note_service::move_root_tree(
                    &data_dir,
                    &old_path_for_move,
                    &new_path_for_move,
                )?;
                Ok::<_, anyhow::Error>((bookmark_count, document_note_count, workspace_note_count))
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

    /// Edit the bookmark label on the current cursor line.
    pub(super) fn edit_bookmark_label(&self) {
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

        let dialog = libadwaita::AlertDialog::new(
            Some("Edit Bookmark Label"),
            Some("Update the label shown in bookmark lists and tooltips."),
        );
        dialog.add_response(RESPONSE_CANCEL, "Cancel");
        dialog.add_response(RESPONSE_SAVE, "Save");
        dialog.set_response_appearance(RESPONSE_SAVE, libadwaita::ResponseAppearance::Suggested);
        dialog.set_default_response(Some(RESPONSE_SAVE));
        dialog.set_close_response(RESPONSE_CANCEL);

        let entry = gtk4::Entry::new();
        entry.set_activates_default(true);
        if let Some(label) = bookmark.label.as_deref() {
            entry.set_text(label);
        }
        dialog.set_extra_child(Some(&entry));

        let editor = editor.clone();
        let window = self.clone();
        dialog.choose(Some(self), gio::Cancellable::NONE, move |response| {
            if response != RESPONSE_SAVE {
                return;
            }

            let label = (!entry.text().trim().is_empty()).then(|| entry.text().to_string());
            if editor.set_bookmark_label_at_cursor(label).is_some() {
                window.publish_status_message("Bookmark label saved", MessageKind::Info);
            }
        });
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
        let scope_paths = self.workspace_note_scope_paths();
        if scope_paths.is_empty() {
            self.publish_status_message(
                "Add a workspace before browsing bookmarks",
                MessageKind::Warning,
            );
            return;
        }

        async_task::spawn_blocking_then(
            self.clone(),
            move || {
                let data_dir = json_store::data_dir();
                bookmark_service::list_workspace_bookmarks(&data_dir, &scope_paths)
            },
            |window, result| match result {
                Ok(bookmarks) => {
                    if bookmarks.is_empty() {
                        window.publish_status_message(
                            "No bookmarks exist in the current workspace",
                            MessageKind::Info,
                        );
                        return;
                    }

                    window.present_bookmark_browser(bookmarks);
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
        self.open_document_note_for_path_with_roots(path, self.current_workspace_directory_roots());
    }

    /// Open the workspace note for the current concrete workspace scope.
    pub(super) fn open_workspace_note(&self) {
        let Some(workspace) = self.current_workspace_note_target() else {
            self.publish_status_message(
                "Select one workspace before opening a workspace note",
                MessageKind::Warning,
            );
            return;
        };
        self.open_workspace_note_for_root(&workspace.name, &workspace.root);
    }

    /// Open the workspace note for a concrete workspace selected from the sidebar.
    pub(super) fn open_workspace_note_for_id(
        &self,
        workspace_id: &crate::model::workspace::WorkspaceId,
    ) {
        let Some(workspace) = self
            .imp()
            .sidebar
            .workspaces_file()
            .workspaces
            .into_iter()
            .find(|workspace| &workspace.id == workspace_id)
        else {
            self.publish_status_message(
                "Workspace note target was not found",
                MessageKind::Warning,
            );
            return;
        };
        self.open_workspace_note_for_root(&workspace.name, &workspace.root);
    }

    /// Browse notes across the current workspace scope.
    pub(super) fn show_notes_dialog(&self) {
        let workspaces_file = self.imp().sidebar.workspaces_file();
        let scope = workspaces_file.current_scope();
        let visible_workspaces: Vec<WorkspaceConfig> = match &scope {
            WorkspaceScope::All => workspaces_file.workspaces.clone(),
            WorkspaceScope::Workspace(workspace_id) => workspaces_file
                .workspaces
                .into_iter()
                .filter(|workspace| &workspace.id == workspace_id)
                .collect(),
        };
        let scope_roots: Vec<PathBuf> = visible_workspaces
            .iter()
            .map(|workspace| workspace.root.clone())
            .collect();
        if scope_roots.is_empty() {
            self.publish_status_message(
                "Add a workspace before browsing notes",
                MessageKind::Warning,
            );
            return;
        }

        async_task::spawn_blocking_then(
            self.clone(),
            move || {
                let data_dir = json_store::data_dir();
                let workspace_notes = workspace_note_service::list_workspace_notes_for_scope(
                    &data_dir,
                    &visible_workspaces,
                    &scope,
                )?;
                let bookmarks =
                    bookmark_service::list_workspace_bookmarks(&data_dir, &scope_roots)?;
                let document_notes =
                    document_note_service::list_workspace_document_notes(&data_dir, &scope_roots)?;
                Ok::<_, anyhow::Error>(build_notes_browser_entries(
                    &visible_workspaces,
                    bookmarks,
                    workspace_notes,
                    document_notes,
                ))
            },
            |window, result| match result {
                Ok(entries) => {
                    if entries.is_empty() {
                        window.publish_status_message(
                            "No notes exist in the current workspace",
                            MessageKind::Info,
                        );
                        return;
                    }

                    window.present_notes_browser(entries);
                }
                Err(error) => {
                    tracing::error!("Failed to list notes: {error}");
                    window.publish_status_message("Notes could not be listed", MessageKind::Error);
                }
            },
        );
    }

    /// Load and present the document note attached to one saved file.
    fn open_document_note_for_path_with_roots(&self, path: &Path, workspace_roots: Vec<PathBuf>) {
        let path = path.to_path_buf();
        let path_for_load = path.clone();
        let path_for_dialog = path.clone();
        let window_weak = self.downgrade();
        async_task::spawn_blocking_then(
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
                            workspace_roots,
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

    /// Load and present the workspace note attached to one workspace root.
    fn open_workspace_note_for_root(&self, workspace_name: &str, root: &Path) {
        let workspace_name = workspace_name.to_string();
        let root = root.to_path_buf();
        let root_for_load = root.clone();
        let root_for_dialog = root.clone();
        let window_weak = self.downgrade();
        async_task::spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                workspace_note_service::load_for_root(&data_dir, &root_for_load)
            },
            move |(), result| match result {
                Ok(note) => {
                    if let Some(window) = window_weak.upgrade() {
                        window.present_workspace_note_dialog(
                            &workspace_name,
                            &root_for_dialog,
                            note.as_ref().map(|document| &document.note),
                        );
                    }
                }
                Err(error) => {
                    tracing::error!(
                        "Failed to load workspace note for {}: {error}",
                        root_for_dialog.display()
                    );
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message(
                            "Workspace note could not be loaded",
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
        workspace_roots: Vec<PathBuf>,
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

        let initial_text = existing_note.as_ref().map_or("", |note| note.text.as_str());
        let (surface, note_view) = build_note_editor_surface(
            initial_text,
            &MarkdownPreviewRenderContext::new(Some(path.to_path_buf()), workspace_roots),
            NoteViewMode::Edit,
            "Write some note text to preview rendered markdown.",
        );
        content.append(&surface);
        dialog.set_extra_child(Some(&content));

        let path = path.to_path_buf();
        let existing_note = existing_note.cloned();
        let window = self.clone();
        dialog.choose(Some(self), gio::Cancellable::NONE, move |response| {
            if response == RESPONSE_CLEAR {
                let path_for_delete = path.clone();
                async_task::spawn_blocking_then(
                    window.clone(),
                    move || {
                        let data_dir = json_store::data_dir();
                        document_note_service::delete_for_path(&data_dir, &path_for_delete)
                    },
                    |window, result| match result {
                        Ok(()) => window
                            .publish_status_message("Document note cleared", MessageKind::Info),
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
                let note_text = buffer
                    .text(&buffer.start_iter(), &buffer.end_iter(), true)
                    .to_string();
                if note_text.trim().is_empty() {
                    window.publish_status_message(
                        "Document notes need note text",
                        MessageKind::Warning,
                    );
                    return;
                }

                let mut note = existing_note
                    .clone()
                    .unwrap_or_else(|| RichNoteBody::new(""));
                if existing_note.is_some() {
                    let _ = note.update_text(&note_text);
                } else {
                    note = RichNoteBody::new(&note_text);
                }

                let path_for_save = path.clone();
                async_task::spawn_blocking_then(
                    window.clone(),
                    move || {
                        let data_dir = json_store::data_dir();
                        document_note_service::save_for_path(&data_dir, &path_for_save, &note)
                            .map(|_| ())
                    },
                    |window, result| match result {
                        Ok(()) => {
                            window.publish_status_message("Document note saved", MessageKind::Info);
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
            }
        });
    }

    /// Present the workspace note editor for one concrete workspace root.
    fn present_workspace_note_dialog(
        &self,
        workspace_name: &str,
        root: &Path,
        existing_note: Option<&RichNoteBody>,
    ) {
        let dialog = libadwaita::AlertDialog::new(
            Some("Workspace Note"),
            Some("Keep one project-scoped note for this workspace root."),
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

        let root_label = gtk4::Label::new(Some(&root.display().to_string()));
        root_label.set_halign(gtk4::Align::Start);
        root_label.set_xalign(0.0);
        root_label.set_wrap(true);
        root_label.add_css_class("dim-label");
        content.append(&root_label);

        let initial_text = existing_note.as_ref().map_or("", |note| note.text.as_str());
        let (surface, note_view) = build_note_editor_surface(
            initial_text,
            &MarkdownPreviewRenderContext::new(None, vec![root.to_path_buf()]),
            NoteViewMode::Edit,
            "Write some note text to preview rendered markdown.",
        );
        content.append(&surface);
        dialog.set_extra_child(Some(&content));

        let root = root.to_path_buf();
        let existing_note = existing_note.cloned();
        let window = self.clone();
        dialog.choose(Some(self), gio::Cancellable::NONE, move |response| {
            if response == RESPONSE_CLEAR {
                let root_for_delete = root.clone();
                async_task::spawn_blocking_then(
                    window.clone(),
                    move || {
                        let data_dir = json_store::data_dir();
                        workspace_note_service::delete_for_root(&data_dir, &root_for_delete)
                    },
                    |window, result| match result {
                        Ok(()) => window
                            .publish_status_message("Workspace note cleared", MessageKind::Info),
                        Err(error) => {
                            tracing::error!("Failed to clear workspace note: {error}");
                            window.publish_status_message(
                                "Workspace note could not be cleared",
                                MessageKind::Error,
                            );
                        }
                    },
                );
            } else if response == RESPONSE_SAVE {
                let buffer = note_view.buffer();
                let note_text = buffer
                    .text(&buffer.start_iter(), &buffer.end_iter(), true)
                    .to_string();
                if note_text.trim().is_empty() {
                    window.publish_status_message(
                        "Workspace notes need note text",
                        MessageKind::Warning,
                    );
                    return;
                }

                let mut note = existing_note
                    .clone()
                    .unwrap_or_else(|| RichNoteBody::new(""));
                if existing_note.is_some() {
                    let _ = note.update_text(&note_text);
                } else {
                    note = RichNoteBody::new(&note_text);
                }

                let root_for_save = root.clone();
                async_task::spawn_blocking_then(
                    window.clone(),
                    move || {
                        let data_dir = json_store::data_dir();
                        workspace_note_service::save_for_root(&data_dir, &root_for_save, &note)
                            .map(|_| ())
                    },
                    |window, result| match result {
                        Ok(()) => {
                            window
                                .publish_status_message("Workspace note saved", MessageKind::Info);
                        }
                        Err(error) => {
                            tracing::error!("Failed to save workspace note: {error}");
                            window.publish_status_message(
                                "Workspace note could not be saved",
                                MessageKind::Error,
                            );
                        }
                    },
                );
            }
        });
    }

    /// Debounce bookmark persistence so one burst of edits produces one sidecar write.
    fn save_bookmarks_debounced(&self, editor: &LushtextEditorPage) {
        let generation = editor
            .imp()
            .bookmarks
            .persistence
            .save_generation
            .get()
            .wrapping_add(1);
        editor
            .imp()
            .bookmarks
            .persistence
            .save_generation
            .set(generation);

        let editor_weak = editor.downgrade();
        let window_weak = self.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(NOTES_SAVE_DEBOUNCE_MS), move || {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            if editor.imp().bookmarks.persistence.save_generation.get() != generation {
                return;
            }

            if editor.imp().bookmarks.persistence.save_inflight.get() {
                editor.imp().bookmarks.persistence.save_dirty.set(true);
                return;
            }

            if let Some(window) = window_weak.upgrade() {
                window.persist_bookmarks_now(&editor);
            }
        });
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
        async_task::spawn_blocking_then(
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

    /// Return the active editor only when it has a stable saved file path.
    fn require_saved_editor(&self, missing_path_message: &str) -> Option<LushtextEditorPage> {
        let editor = self.active_editor()?;
        if editor.file_path().is_some() {
            return Some(editor);
        }

        self.publish_status_message(missing_path_message, MessageKind::Warning);
        None
    }

    /// Collect the current workspace scope for bookmark and note workflows.
    fn workspace_note_scope_paths(&self) -> Vec<PathBuf> {
        self.current_workspace_scope_paths()
    }

    /// Return the currently selected concrete workspace, if one exists.
    fn current_workspace_note_target(&self) -> Option<WorkspaceConfig> {
        let workspaces_file = self.imp().sidebar.workspaces_file();
        let WorkspaceScope::Workspace(workspace_id) = workspaces_file.current_scope() else {
            return None;
        };
        workspaces_file
            .workspaces
            .into_iter()
            .find(|workspace| workspace.id == workspace_id)
    }

    /// Recompute the Notes menu button visibility and menu-only action state.
    ///
    /// The dedicated menu uses its own `notes-*` actions so it can become
    /// insensitive without disabling the existing shortcuts or command-palette
    /// commands that still rely on the workflow guards below.
    pub(super) fn refresh_notes_menu_state(&self) {
        let workspace_actions_available = self.notes_workspace_actions_available();
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

        self.imp()
            .notes_menu_button
            .set_visible(active_editor.is_some() || workspace_actions_available);

        self.set_notes_menu_action_enabled("notes-toggle-bookmark", saved_editor.is_some());
        self.set_notes_menu_action_enabled("notes-open-document-note", saved_editor.is_some());
        self.set_notes_menu_action_enabled(
            "notes-open-workspace-note",
            self.current_workspace_note_target().is_some(),
        );
        self.set_notes_menu_action_enabled("notes-show-notes", workspace_actions_available);
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
            Some("Open Workspace Note…"),
            Some("win.notes-open-workspace-note"),
        );
        menu.append_section(None, &workspace_section);

        self.imp().notes_menu_button.set_menu_model(Some(&menu));
    }

    /// Return whether the current shared workspace scope exposes any roots.
    fn notes_workspace_actions_available(&self) -> bool {
        !self.workspace_note_scope_paths().is_empty()
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
        search_entry.update_property(&[
            gtk4::accessible::Property::Label("Search bookmarks"),
            gtk4::accessible::Property::Description("Filter bookmarks in the current workspace"),
        ]);
        content.append(&search_entry);

        let rows_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let scroll = gtk4::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .min_content_height(320)
            .child(&rows_box)
            .build();
        content.append(&scroll);

        let bookmarks = Rc::new(bookmarks);
        rebuild_bookmark_rows(self, &dialog, &rows_box, &bookmarks, "");

        let window = self.clone();
        let dialog_weak = dialog.downgrade();
        let rows_box = rows_box.clone();
        let bookmarks_for_search = bookmarks.clone();
        search_entry.connect_search_changed(move |entry| {
            let Some(dialog) = dialog_weak.upgrade() else {
                return;
            };
            rebuild_bookmark_rows(
                &window,
                &dialog,
                &rows_box,
                &bookmarks_for_search,
                entry.text().as_str(),
            );
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
        search_entry.set_placeholder_text(Some(&format!("Search {WORKSPACE_SCOPE_TITLE}…")));
        search_entry.update_property(&[
            gtk4::accessible::Property::Label("Search notes"),
            gtk4::accessible::Property::Description(
                "Filter bookmarks, document notes, and workspace notes",
            ),
        ]);

        let sidebar = libadwaita::Sidebar::new();
        sidebar.set_accessible_role(gtk4::AccessibleRole::List);
        sidebar.set_mode(libadwaita::SidebarMode::Sidebar);
        sidebar.set_vexpand(true);
        sidebar.set_placeholder(Some(&empty_browser_label("No notes match that search")));
        sidebar.update_property(&[
            gtk4::accessible::Property::Label("Notes results"),
            gtk4::accessible::Property::Description(
                "Choose a bookmark, document note, or workspace note",
            ),
        ]);
        let limit_label = gtk4::Label::new(None);
        limit_label.set_halign(gtk4::Align::Start);
        limit_label.set_xalign(0.0);
        limit_label.set_wrap(true);
        limit_label.add_css_class("caption");
        limit_label.add_css_class("dim-label");
        limit_label.set_visible(false);

        let preview_title = gtk4::Label::new(Some("Select a note"));
        preview_title.set_halign(gtk4::Align::Start);
        preview_title.set_xalign(0.0);
        preview_title.add_css_class("title-4");

        let preview_meta = gtk4::Label::new(Some(
            "Choose a bookmark, workspace note, or document note to preview it here.",
        ));
        preview_meta.set_halign(gtk4::Align::Start);
        preview_meta.set_xalign(0.0);
        preview_meta.set_wrap(true);
        preview_meta.add_css_class("dim-label");

        let preview = LushtextMarkdownPreview::new();
        preview.set_hexpand(true);
        preview.set_vexpand(true);
        preview.show_placeholder("Select a note to preview its details.");

        let open_button = gtk4::Button::with_label("Open");
        open_button.add_css_class("suggested-action");
        open_button.set_sensitive(false);
        open_button.update_property(&[gtk4::accessible::Property::Label("Open selected note")]);

        let back_button = gtk4::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back to Notes")
            .visible(false)
            .build();
        back_button.update_property(&[gtk4::accessible::Property::Label("Back to notes")]);

        let split_view = libadwaita::NavigationSplitView::new();
        split_view.set_min_sidebar_width(260.0);
        split_view.set_max_sidebar_width(340.0);
        split_view.set_sidebar(Some(&libadwaita::NavigationPage::new(
            &build_notes_sidebar(&search_entry, &sidebar, &limit_label),
            "Notes",
        )));
        split_view.set_content(Some(&libadwaita::NavigationPage::new(
            &build_notes_preview_page(
                &back_button,
                &preview_title,
                &preview_meta,
                &preview,
                &open_button,
            ),
            "Preview",
        )));
        split_view.set_show_content(false);
        dialog.set_child(Some(&split_view));

        let state = Rc::new(NotesBrowserState {
            window: self.clone(),
            dialog,
            split_view,
            search_entry,
            sidebar,
            limit_label,
            preview_title,
            preview_meta,
            preview,
            open_button,
            back_button,
            filtered_indices: RefCell::new(Vec::new()),
            search_generation: Cell::new(0),
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
                    state.refresh_preview(sidebar_item_index(sidebar.selected_item()), true);
                }
            }
        });
        state.sidebar.connect_activated({
            let state = Rc::downgrade(&state);
            move |sidebar, index| {
                if let Some(state) = state.upgrade() {
                    sidebar.set_selected(index);
                    state.refresh_preview(usize::try_from(index).ok(), true);
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
        state.split_view.connect_collapsed_notify({
            let state = Rc::downgrade(&state);
            move |split| {
                if let Some(state) = state.upgrade() {
                    state.back_button.set_visible(split.is_collapsed());
                }
            }
        });

        // The dialog owns this holder while it is visible, keeping browser
        // state alive without child-widget signal closures strongly owning the
        // whole dialog subtree. The `closed` signal drops the state and breaks
        // the temporary dialog -> holder -> state -> dialog cycle.
        let state_holder = Rc::new(RefCell::new(Some(Rc::clone(&state))));
        state.dialog.connect_closed({
            let state_holder = Rc::clone(&state_holder);
            move |_| {
                state_holder.borrow_mut().take();
            }
        });

        state.dialog.present(Some(self));
    }
}

/// Build the shared edit/render note surface used by document and workspace notes.
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

    let switcher = gtk4::StackSwitcher::new();
    switcher.set_stack(Some(&stack));
    switcher.set_hexpand(true);
    switcher.set_halign(gtk4::Align::Fill);
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
    preview.show_content_placeholder(empty_preview_description);
    // Existing notes open in Edit mode, but the hidden Render page still
    // participates in stack measurement. Render once up front so the first
    // click on Render does not swap placeholder geometry into content geometry.
    if !initial_text.trim().is_empty() {
        render_note_preview(
            &preview,
            &note_view.buffer(),
            render_context,
            empty_preview_description,
        );
    }
    stack.add_titled(&preview, Some("render"), "Render");

    let buffer = note_view.buffer();
    let preview_clone = preview.clone();
    let render_context_clone = render_context.clone();
    stack.connect_notify_local(Some("visible-child-name"), move |stack, _| {
        if stack.visible_child_name().as_deref() == Some("render") {
            render_note_preview(
                &preview_clone,
                &buffer,
                &render_context_clone,
                empty_preview_description,
            );
        }
    });

    match initial_mode {
        NoteViewMode::Edit => stack.set_visible_child_name("edit"),
        NoteViewMode::Render => {
            stack.set_visible_child_name("render");
            render_note_preview(
                &preview,
                &note_view.buffer(),
                render_context,
                empty_preview_description,
            );
        }
    }

    content.append(&stack);
    (content, note_view)
}

/// Render one note body into the shared markdown preview widget.
fn render_note_preview(
    preview: &LushtextMarkdownPreview,
    buffer: &gtk4::TextBuffer,
    render_context: &MarkdownPreviewRenderContext,
    empty_preview_description: &str,
) {
    let text = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string();
    if text.trim().is_empty() {
        preview.show_content_placeholder(empty_preview_description);
    } else {
        preview.render_markdown_with_context(&text, render_context);
    }
}

/// Build an explicit empty state when the current scope has no notes yet.
fn build_empty_notes_dialog() -> libadwaita::Dialog {
    let dialog = libadwaita::Dialog::builder()
        .title("Notes")
        .content_width(560)
        .content_height(360)
        .follows_content_size(true)
        .build();

    let status = libadwaita::StatusPage::builder()
        .icon_name("text-x-generic-symbolic")
        .title("No notes yet")
        .description(
            "Bookmarks, document notes, and workspace notes will appear here once you save one.",
        )
        .build();
    dialog.set_child(Some(&status));
    dialog
}

/// Build the browse rail used by the unified notes browser.
fn build_notes_sidebar(
    search_entry: &gtk4::SearchEntry,
    sidebar: &libadwaita::Sidebar,
    limit_label: &gtk4::Label,
) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);

    let title = gtk4::Label::new(Some("Notes"));
    title.set_halign(gtk4::Align::Start);
    title.set_xalign(0.0);
    title.add_css_class("title-4");
    content.append(&title);
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
    back_button: &gtk4::Button,
    preview_title: &gtk4::Label,
    preview_meta: &gtk4::Label,
    preview: &LushtextMarkdownPreview,
    open_button: &gtk4::Button,
) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header.append(back_button);

    let title_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    title_box.set_hexpand(true);
    title_box.append(preview_title);
    title_box.append(preview_meta);
    header.append(&title_box);
    content.append(&header);

    content.append(preview);

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    actions.set_halign(gtk4::Align::End);
    actions.append(open_button);
    content.append(&actions);

    content
}

impl NotesBrowserEntry {
    /// User-facing row title used in the browser list.
    #[must_use]
    fn row_title(&self) -> String {
        match self {
            Self::Bookmark { line, label, .. } => {
                format!(
                    "Bookmark · {}",
                    bookmark_display_label(label.as_deref(), *line)
                )
            }
            Self::Workspace { workspace_name, .. } => format!("Workspace Note · {workspace_name}"),
            Self::Document { path, .. } => {
                let file_name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                format!("Document Note · {file_name}")
            }
        }
    }

    /// Secondary row text used for scope and location metadata.
    #[must_use]
    fn row_subtitle(&self) -> String {
        match self {
            Self::Bookmark {
                workspace_name,
                path,
                line,
                ..
            } => format!(
                "{workspace_name} · {} · {}",
                path.display(),
                format_line_label(*line)
            ),
            Self::Workspace {
                workspace_name,
                root,
                ..
            } => format!("{workspace_name} · {}", root.display()),
            Self::Document {
                workspace_name,
                path,
                ..
            } => format!("{workspace_name} · {}", path.display()),
        }
    }

    /// Optional row detail showing the first meaningful line of note text.
    #[must_use]
    fn row_detail(&self) -> Option<String> {
        if let Self::Bookmark { .. } = self {
            None
        } else {
            let preview = note_preview_line(self.note_text());
            (!preview.is_empty()).then_some(preview)
        }
    }

    /// Title shown in the preview header for the selected note.
    #[must_use]
    fn preview_title(&self) -> String {
        match self {
            Self::Bookmark { line, label, .. } => {
                format!(
                    "Bookmark · {}",
                    bookmark_display_label(label.as_deref(), *line)
                )
            }
            Self::Workspace { workspace_name, .. } => format!("Workspace Note · {workspace_name}"),
            Self::Document { path, .. } => {
                let file_name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                format!("Document Note · {file_name}")
            }
        }
    }

    /// Secondary preview metadata shown under the selected note title.
    #[must_use]
    fn preview_meta(&self) -> String {
        match self {
            Self::Bookmark {
                workspace_name,
                path,
                line,
                ..
            } => format!(
                "{workspace_name} · {} · {}",
                path.display(),
                format_line_label(*line)
            ),
            Self::Workspace {
                workspace_name,
                root,
                ..
            } => format!("{workspace_name} · {}", root.display()),
            Self::Document {
                workspace_name,
                path,
                ..
            } => format!("{workspace_name} · {}", path.display()),
        }
    }

    /// Note text rendered into the preview widget.
    #[must_use]
    fn note_text(&self) -> &str {
        match self {
            Self::Bookmark { .. } => "",
            Self::Workspace { note, .. } | Self::Document { note, .. } => &note.text,
        }
    }

    /// Placeholder copy shown in the preview body for bookmark entries.
    #[must_use]
    fn bookmark_preview_text(&self) -> String {
        match self {
            Self::Bookmark {
                workspace_name,
                path,
                line,
                label,
                ..
            } => format!(
                "{} in {workspace_name} at {} · {}",
                bookmark_display_label(label.as_deref(), *line),
                format_line_label(*line),
                path.display()
            ),
            _ => String::new(),
        }
    }

    /// Render context used by the shared markdown preview widget.
    #[must_use]
    fn render_context(&self) -> MarkdownPreviewRenderContext {
        match self {
            Self::Workspace { root, .. } => {
                MarkdownPreviewRenderContext::new(None, vec![root.clone()])
            }
            Self::Bookmark {
                path,
                workspace_root,
                ..
            }
            | Self::Document {
                path,
                workspace_root,
                ..
            } => {
                MarkdownPreviewRenderContext::new(Some(path.clone()), vec![workspace_root.clone()])
            }
        }
    }

    /// Symbolic icon used by the grouped Adwaita sidebar item.
    #[must_use]
    fn sidebar_icon_name(&self) -> &'static str {
        match self {
            Self::Bookmark { .. } => "bookmark-new-symbolic",
            Self::Workspace { .. } => "folder-symbolic",
            Self::Document { .. } => "text-x-generic-symbolic",
        }
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
    fn refresh_preview(&self, index: Option<usize>, user_selected: bool) {
        let Some(index) = index else {
            self.preview_title.set_label("Select a note");
            self.preview_meta.set_label(
                "Choose a bookmark, workspace note, or document note to preview it here.",
            );
            self.preview
                .show_placeholder("Select a note to preview its details.");
            self.open_button.set_sensitive(false);
            return;
        };

        let Some(entry_index) = self.filtered_indices.borrow().get(index).copied() else {
            self.preview_title.set_label("Select a note");
            self.preview_meta.set_label(
                "Choose a bookmark, workspace note, or document note to preview it here.",
            );
            self.preview
                .show_placeholder("Select a note to preview its details.");
            self.open_button.set_sensitive(false);
            return;
        };
        let Some(entry) = self.all_entries.get(entry_index) else {
            self.preview_title.set_label("Select a note");
            self.preview_meta.set_label(
                "Choose a bookmark, workspace note, or document note to preview it here.",
            );
            self.preview
                .show_placeholder("Select a note to preview its details.");
            self.open_button.set_sensitive(false);
            return;
        };

        self.preview_title.set_label(&entry.preview_title());
        self.preview_meta.set_label(&entry.preview_meta());
        if matches!(entry, &NotesBrowserEntry::Bookmark { .. }) {
            self.preview
                .show_placeholder(&entry.bookmark_preview_text());
        } else if entry.note_text().trim().is_empty() {
            self.preview.show_placeholder("This note is empty.");
        } else {
            self.preview
                .render_markdown_with_context(entry.note_text(), &entry.render_context());
        }
        self.open_button.set_sensitive(true);

        if user_selected {
            // `show-content` is only visible while collapsed, but setting it
            // before the adaptive layout settles preserves the user's
            // navigation request during resize and widget-test transitions.
            self.split_view.set_show_content(true);
        }
    }

    /// Open the currently selected note through the same window workflows used elsewhere.
    fn open_selected(&self) {
        let Some(index) = sidebar_item_index(self.sidebar.selected_item()) else {
            return;
        };
        let Some(entry_index) = self.filtered_indices.borrow().get(index).copied() else {
            return;
        };
        let Some(entry) = self.all_entries.get(entry_index) else {
            return;
        };

        self.dialog.close();
        activate_notes_browser_entry(&self.window, entry);
    }
}

/// Debounce browser search so large note sets do not rebuild on every keystroke.
fn schedule_notes_browser_search(state: &Rc<NotesBrowserState>, query: String) {
    let generation = state.search_generation.get().wrapping_add(1);
    state.search_generation.set(generation);
    let state = Rc::downgrade(state);
    glib::timeout_add_local_once(Duration::from_millis(150), move || {
        let Some(state) = state.upgrade() else {
            return;
        };
        if state.search_generation.get() != generation {
            return;
        }
        rebuild_notes_browser_sidebar(&state, &query);
    });
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
        state.refresh_preview(None, false);
        return;
    }
    if truncated {
        state.limit_label.set_label(&format!(
            "Showing first {NOTES_BROWSER_RENDER_LIMIT} matches. Refine search to narrow results."
        ));
    }
    state.limit_label.set_visible(truncated);

    let grouped_indices =
        append_notes_sidebar_sections(&state.sidebar, &state.all_entries, &matching_indices);
    *state.filtered_indices.borrow_mut() = grouped_indices;

    state.sidebar.set_selected(0);
    state.refresh_preview(Some(0), false);
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
            all_entries
                .get(*index)
                .is_some_and(|entry| matches!(entry, NotesBrowserEntry::Bookmark { .. }))
        }),
        all_entries,
        &mut ordered_indices,
    );
    append_note_sidebar_section(
        sidebar,
        "Workspace Notes",
        matching_indices.iter().copied().filter(|index| {
            all_entries
                .get(*index)
                .is_some_and(|entry| matches!(entry, NotesBrowserEntry::Workspace { .. }))
        }),
        all_entries,
        &mut ordered_indices,
    );
    append_note_sidebar_section(
        sidebar,
        "Document Notes",
        matching_indices.iter().copied().filter(|index| {
            all_entries
                .get(*index)
                .is_some_and(|entry| matches!(entry, NotesBrowserEntry::Document { .. }))
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
    match entry {
        NotesBrowserEntry::Bookmark { path, line, .. } => {
            open_editor_at_line(window, path, line.saturating_add(1));
        }
        NotesBrowserEntry::Workspace {
            workspace_name,
            root,
            ..
        } => window.open_workspace_note_for_root(workspace_name, root),
        NotesBrowserEntry::Document {
            path,
            workspace_root,
            ..
        } => {
            window.open_document(path);
            window.open_document_note_for_path_with_roots(path, vec![workspace_root.clone()]);
        }
    }
}

/// Merge bookmarks plus workspace and document notes into one browser entry list.
fn build_notes_browser_entries(
    visible_workspaces: &[WorkspaceConfig],
    bookmarks: Vec<bookmark_service::WorkspaceBookmark>,
    workspace_notes: Vec<workspace_note_service::ListedWorkspaceNote>,
    document_notes: Vec<document_note_service::WorkspaceDocumentNote>,
) -> Vec<NotesBrowserEntry> {
    let mut entries = Vec::new();

    for bookmark in bookmarks {
        if let Some(workspace) = workspace_for_path(visible_workspaces, &bookmark.path) {
            entries.push(NotesBrowserEntry::Bookmark {
                workspace_name: workspace.name.clone(),
                workspace_root: workspace.root.clone(),
                path: bookmark.path,
                line: bookmark.line,
                label: bookmark.label,
            });
        }
    }

    entries.extend(
        workspace_notes
            .into_iter()
            .map(|note| NotesBrowserEntry::Workspace {
                workspace_name: note.workspace_name,
                root: note.root,
                note: note.note,
            }),
    );

    for note in document_notes {
        if let Some(workspace) = workspace_for_path(visible_workspaces, &note.path) {
            entries.push(NotesBrowserEntry::Document {
                workspace_name: workspace.name.clone(),
                workspace_root: workspace.root.clone(),
                path: note.path,
                note: note.note,
            });
        }
    }

    entries.sort_by(|left, right| {
        left.row_title()
            .cmp(&right.row_title())
            .then_with(|| left.row_subtitle().cmp(&right.row_subtitle()))
    });
    entries
}

/// Find the most specific workspace that owns one saved path.
fn workspace_for_path<'a>(
    workspaces: &'a [WorkspaceConfig],
    path: &Path,
) -> Option<&'a WorkspaceConfig> {
    workspaces
        .iter()
        .filter(|workspace| path.starts_with(&workspace.root))
        .max_by_key(|workspace| workspace.root.components().count())
}

/// Display one zero-based bookmark line in the 1-based form users expect.
#[must_use]
fn format_line_label(line: u32) -> String {
    format!("Line {}", line.saturating_add(1))
}

/// Return the bookmark's explicit label or its stable line fallback.
#[must_use]
fn bookmark_display_label(label: Option<&str>, line: u32) -> String {
    label
        .filter(|label| !label.trim().is_empty())
        .map_or_else(|| format_line_label(line), ToOwned::to_owned)
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

    let close_button = gtk4::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Close")
        .build();
    let dialog_weak = dialog.downgrade();
    close_button.connect_clicked(move |_| {
        if let Some(dialog) = dialog_weak.upgrade() {
            dialog.close();
        }
    });
    header.append(&close_button);
    content.prepend(&header);
    dialog
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

    let filtered: Vec<_> = bookmarks
        .iter()
        .filter(|bookmark| bookmark_matches_query(bookmark, query))
        .cloned()
        .collect();
    if filtered.is_empty() {
        rows_box.append(&empty_browser_label("No bookmarks match that search"));
        return;
    }

    for bookmark in filtered {
        let button = gtk4::Button::new();
        button.add_css_class("flat");
        button.set_hexpand(true);
        button.set_halign(gtk4::Align::Fill);
        button.set_child(Some(&browser_row_content(
            &bookmark.display_label(),
            &format!(
                "{} · Line {}",
                bookmark.path.display(),
                bookmark.line.saturating_add(1)
            ),
            None,
        )));

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
