// SPDX-License-Identifier: GPL-3.0-or-later

//! Bookmark and annotation workflows for the main window shell.
//!
//! This module keeps note-specific action handling, dialogs, persistence
//! scheduling, and workspace export logic out of the generic document shell.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, AlertDialogExt, AlertDialogExtManual};

use crate::model::annotation::{AnnotationId, AnnotationRecord, AnnotationStyle};
use crate::model::note::{NoteViewMode, RichNoteBody, note_preview_line};
use crate::model::workspace::{WorkspaceConfig, WorkspaceScope};
use crate::services::{
    annotation_service, async_task, bookmark_service, document_note_service, editor_io, json_store,
    workspace_note_service,
};
use crate::ui::editor_page::{
    AnnotationEditSelection, BookmarkNavigationDirection, BookmarkToggleState, LushtextEditorPage,
};
use crate::ui::markdown_preview::{LushtextMarkdownPreview, MarkdownPreviewRenderContext};
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

/// Debounce interval for bookmark and annotation sidecar saves.
///
/// 200ms coalesces rapid line-shift edits into one filesystem write without
/// letting note state drift for long after the user pauses typing.
const NOTES_SAVE_DEBOUNCE_MS: u64 = 200;

/// Alert-dialog response IDs reused by the note workflows.
const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_SAVE: &str = "save";
const RESPONSE_DELETE: &str = "delete";
const RESPONSE_CLEAR: &str = "clear";

/// Search scope title used by bookmark and annotation browse dialogs.
const WORKSPACE_SCOPE_TITLE: &str = "Current Workspace";

/// Fixed notes browser width keeps the preview usable without turning the dialog
/// into a second full window.
const NOTES_BROWSER_WIDTH_SP: i32 = 980;
/// Fixed notes browser height leaves room for the list, preview, and action row.
const NOTES_BROWSER_HEIGHT_SP: i32 = 700;

/// One entry shown in the unified notes browser.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NotesBrowserEntry {
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
    Range {
        workspace_name: String,
        workspace_root: PathBuf,
        path: PathBuf,
        annotation_id: AnnotationId,
        start_line: u32,
        end_line: u32,
        note_text: String,
        style: AnnotationStyle,
    },
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
    /// Notes list shown in the sidebar rail.
    list_box: gtk4::ListBox,
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
    /// Current filtered entries in the same order as the list box rows.
    filtered_entries: RefCell<Vec<NotesBrowserEntry>>,
}

impl LushtextWindow {
    /// Wire bookmark and annotation callbacks for a newly created editor page.
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
        editor.connect_annotations_changed(move || {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
            {
                window.save_annotations_debounced(&editor);
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
                let bookmarks =
                    bookmark_service::load_for_path(&data_dir, &path_for_load)?.bookmarks;
                let annotations =
                    annotation_service::load_for_path(&data_dir, &path_for_load)?.annotations;
                Ok::<_, anyhow::Error>((bookmarks, annotations))
            },
            move |editor, result| match result {
                Ok((bookmarks, annotations)) => {
                    editor.load_bookmarks(&bookmarks);
                    editor.load_annotations(&annotations);
                    if let Some(window) = window_weak.upgrade() {
                        window.open_pending_annotation_if_ready(&editor);
                        window.refresh_status_bar();
                    }
                }
                Err(error) => {
                    tracing::error!("Failed to load notes for {}: {error}", path.display());
                    editor.clear_bookmarks();
                    editor.clear_annotations();
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message(
                            "Bookmarks or range notes could not be loaded",
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
        editor.clear_annotations();
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
                let annotation_count = annotation_service::move_path_tree(
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
                Ok::<_, anyhow::Error>((
                    bookmark_count,
                    annotation_count,
                    document_note_count,
                    workspace_note_count,
                ))
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
                            "Rename succeeded, but bookmarks/annotations could not be moved",
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

    /// Create a new annotation for the current selection (or current line).
    pub(super) fn add_annotation(&self) {
        let Some(editor) = self.require_saved_editor("Range notes require a saved file") else {
            return;
        };
        self.present_annotation_editor(&editor, None);
    }

    /// Edit the annotation under the current cursor line.
    pub(super) fn edit_annotation(&self) {
        let Some(editor) = self.require_saved_editor("Range notes require a saved file") else {
            return;
        };
        let Some(annotation) = editor.current_annotation() else {
            self.publish_status_message(
                "Move the cursor onto a range note first",
                MessageKind::Warning,
            );
            return;
        };
        self.present_annotation_editor(&editor, Some(&annotation));
    }

    /// Open the document note for the active saved file.
    pub(super) fn open_document_note(&self) {
        let Some(editor) = self.require_saved_editor("Document notes require a saved file") else {
            return;
        };
        let Some(path) = editor.file_path() else {
            return;
        };
        self.open_document_note_for_path_with_roots(
            &path,
            self.current_workspace_directory_roots(),
        );
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

    /// Browse notes across the current workspace scope.
    pub(super) fn show_annotations_dialog(&self) {
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
                let document_notes =
                    document_note_service::list_workspace_document_notes(&data_dir, &scope_roots)?;
                let range_notes =
                    annotation_service::list_workspace_annotations(&data_dir, &scope_roots)?;
                Ok::<_, anyhow::Error>(build_notes_browser_entries(
                    &visible_workspaces,
                    workspace_notes,
                    document_notes,
                    range_notes,
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

    /// Export range notes for the current workspace scope to a markdown file.
    pub(super) fn export_annotations(&self) {
        let scope_paths = self.workspace_note_scope_paths();
        if scope_paths.is_empty() {
            self.publish_status_message(
                "Add a workspace before exporting range notes",
                MessageKind::Warning,
            );
            return;
        }

        let dialog = gtk4::FileDialog::builder()
            .title("Export Range Notes")
            .modal(true)
            .build();
        dialog.set_initial_name(Some("range-notes.md"));

        let window = self.clone();
        dialog.save(Some(self), gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else {
                return;
            };
            let Some(path) = file.path() else {
                return;
            };

            let save_path = path.clone();
            let scope_paths = scope_paths.clone();
            async_task::spawn_blocking_then(
                window.clone(),
                move || {
                    let data_dir = json_store::data_dir();
                    let markdown =
                        annotation_service::export_workspace_markdown(&data_dir, &scope_paths)?;
                    editor_io::write_snapshot_to_path(&save_path, &markdown)
                        .map(|_| ())
                        .map_err(anyhow::Error::from)
                },
                move |window, result| match result {
                    Ok(()) => window.publish_status_message(
                        &format!("Range-note report saved to {}", path.display()),
                        MessageKind::Info,
                    ),
                    Err(error) => {
                        tracing::error!("Failed to export range notes: {error}");
                        window
                            .publish_status_message("Range-note export failed", MessageKind::Error);
                    }
                },
            );
        });
    }

    /// If a pending annotation was requested from a browse surface, open it now.
    pub(super) fn open_pending_annotation_if_ready(&self, editor: &LushtextEditorPage) {
        let Some(annotation_id) = editor.imp().annotations.pending_focus_id.borrow().clone() else {
            return;
        };
        let Some(annotation) = editor.annotation_by_id(&annotation_id) else {
            return;
        };

        editor.set_pending_annotation_focus(None);
        self.present_annotation_editor(editor, Some(&annotation));
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

    /// Debounce annotation persistence so range-tracking edits do not spam the filesystem.
    fn save_annotations_debounced(&self, editor: &LushtextEditorPage) {
        let generation = editor
            .imp()
            .annotations
            .persistence
            .save_generation
            .get()
            .wrapping_add(1);
        editor
            .imp()
            .annotations
            .persistence
            .save_generation
            .set(generation);

        let editor_weak = editor.downgrade();
        let window_weak = self.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(NOTES_SAVE_DEBOUNCE_MS), move || {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            if editor.imp().annotations.persistence.save_generation.get() != generation {
                return;
            }

            if editor.imp().annotations.persistence.save_inflight.get() {
                editor.imp().annotations.persistence.save_dirty.set(true);
                return;
            }

            if let Some(window) = window_weak.upgrade() {
                window.persist_annotations_now(&editor);
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

    /// Write the current annotation snapshot to disk.
    fn persist_annotations_now(&self, editor: &LushtextEditorPage) {
        let Some(path) = editor.file_path() else {
            return;
        };
        let annotations = editor.annotation_records();
        let data_dir = json_store::data_dir();
        editor.imp().annotations.persistence.save_inflight.set(true);
        editor.imp().annotations.persistence.save_dirty.set(false);

        let window_weak = self.downgrade();
        async_task::spawn_blocking_then(
            editor.clone(),
            move || annotation_service::save_for_path(&data_dir, &path, &annotations).map(|_| ()),
            move |editor, result| {
                editor
                    .imp()
                    .annotations
                    .persistence
                    .save_inflight
                    .set(false);
                if let Err(error) = result {
                    tracing::error!("Failed to save annotations: {error}");
                    if let Some(window) = window_weak.upgrade() {
                        window
                            .publish_status_message("Range-note save failed", MessageKind::Warning);
                    }
                }
                if editor
                    .imp()
                    .annotations
                    .persistence
                    .save_dirty
                    .replace(false)
                    && let Some(window) = window_weak.upgrade()
                {
                    window.persist_annotations_now(&editor);
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

    /// Collect the current workspace scope for bookmark, annotation, and export workflows.
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

        self.imp()
            .notes_menu_button
            .set_visible(active_editor.is_some() || workspace_actions_available);

        self.set_notes_menu_action_enabled("notes-toggle-bookmark", saved_editor.is_some());
        self.set_notes_menu_action_enabled(
            "notes-edit-bookmark-label",
            saved_editor.is_some_and(|editor| editor.current_bookmark().is_some()),
        );
        self.set_notes_menu_action_enabled("notes-add-annotation", saved_editor.is_some());
        self.set_notes_menu_action_enabled(
            "notes-edit-annotation",
            saved_editor.is_some_and(|editor| editor.current_annotation().is_some()),
        );
        self.set_notes_menu_action_enabled("notes-open-document-note", saved_editor.is_some());
        self.set_notes_menu_action_enabled(
            "notes-open-workspace-note",
            self.current_workspace_note_target().is_some(),
        );
        self.set_notes_menu_action_enabled("notes-show-bookmarks", workspace_actions_available);
        self.set_notes_menu_action_enabled("notes-show-annotations", workspace_actions_available);
        self.set_notes_menu_action_enabled("notes-export-annotations", workspace_actions_available);
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

    /// Present the create/edit dialog for a single saved-file range note.
    fn present_annotation_editor(
        &self,
        editor: &LushtextEditorPage,
        existing: Option<&AnnotationRecord>,
    ) {
        let selection = existing.cloned().map_or_else(
            || editor.annotation_edit_selection(),
            AnnotationEditSelection::Existing,
        );
        let body = match &selection {
            AnnotationEditSelection::Existing(annotation) => {
                format!(
                    "Update the range note for {}.",
                    annotation.line_range_label()
                )
            }
            AnnotationEditSelection::NewRange {
                start_line,
                end_line,
            } => format!(
                "Add a range note for {}.",
                AnnotationRecord::new(*start_line, *end_line, "", AnnotationStyle::Note,)
                    .line_range_label()
            ),
        };

        let dialog = libadwaita::AlertDialog::new(
            Some(if existing.is_some() {
                "Edit Range Note"
            } else {
                "New Range Note"
            }),
            Some(&body),
        );
        dialog.add_response(RESPONSE_CANCEL, "Cancel");
        if existing.is_some() {
            dialog.add_response(RESPONSE_DELETE, "Delete");
            dialog.set_response_appearance(
                RESPONSE_DELETE,
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

        let style_label = gtk4::Label::new(Some("Style"));
        style_label.set_halign(gtk4::Align::Start);
        content.append(&style_label);

        let style_dropdown = gtk4::DropDown::from_strings(&["Note", "Todo", "Warning", "Question"]);
        style_dropdown.set_selected(annotation_style_index(
            existing.map_or(AnnotationStyle::Note, |annotation| annotation.style),
        ));
        content.append(&style_dropdown);

        let note_label = gtk4::Label::new(Some("Note"));
        note_label.set_halign(gtk4::Align::Start);
        content.append(&note_label);

        let initial_text = existing.map_or("", |annotation| annotation.note_text.as_str());
        let render_context = MarkdownPreviewRenderContext::new(
            editor.file_path(),
            self.current_workspace_directory_roots(),
        );
        let (surface, note_view) = build_note_editor_surface(
            initial_text,
            &render_context,
            NoteViewMode::Edit,
            "Write some note text to preview rendered markdown.",
        );
        content.append(&surface);
        dialog.set_extra_child(Some(&content));

        let window = self.clone();
        let editor = editor.clone();
        let existing = existing.cloned();
        dialog.choose(Some(self), gio::Cancellable::NONE, move |response| {
            if response == RESPONSE_DELETE {
                if let Some(annotation) = existing.as_ref()
                    && editor.delete_annotation(&annotation.id)
                {
                    window.publish_status_message("Range note deleted", MessageKind::Info);
                }
                return;
            }

            if response == RESPONSE_SAVE {
                let buffer = note_view.buffer();
                let note_text = buffer
                    .text(&buffer.start_iter(), &buffer.end_iter(), true)
                    .to_string();
                if note_text.trim().is_empty() {
                    window
                        .publish_status_message("Range notes need note text", MessageKind::Warning);
                    return;
                }

                let style = annotation_style_from_index(style_dropdown.selected());
                if let Some(annotation) = existing.as_ref() {
                    if editor
                        .update_annotation(&annotation.id, &note_text, style)
                        .is_some()
                    {
                        window.publish_status_message("Range note updated", MessageKind::Info);
                    }
                } else {
                    let _ = editor.create_annotation_from_selection(&note_text, style);
                    window.publish_status_message("Range note added", MessageKind::Info);
                }
            }
        });
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

        let list_box = gtk4::ListBox::new();
        list_box.set_selection_mode(gtk4::SelectionMode::Single);
        list_box.add_css_class("boxed-list");

        let search_entry = gtk4::SearchEntry::new();
        search_entry.set_placeholder_text(Some(&format!("Search {WORKSPACE_SCOPE_TITLE}…")));

        let preview_title = gtk4::Label::new(Some("Select a note"));
        preview_title.set_halign(gtk4::Align::Start);
        preview_title.set_xalign(0.0);
        preview_title.add_css_class("title-4");

        let preview_meta = gtk4::Label::new(Some(
            "Choose a workspace, document, or range note to preview it here.",
        ));
        preview_meta.set_halign(gtk4::Align::Start);
        preview_meta.set_xalign(0.0);
        preview_meta.set_wrap(true);
        preview_meta.add_css_class("dim-label");

        let preview = LushtextMarkdownPreview::new();
        preview.set_hexpand(true);
        preview.set_vexpand(true);
        preview.show_placeholder("Select a note to preview its rendered markdown.");

        let open_button = gtk4::Button::with_label("Open");
        open_button.add_css_class("suggested-action");
        open_button.set_sensitive(false);

        let back_button = gtk4::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back to Notes")
            .visible(false)
            .build();

        let split_view = libadwaita::NavigationSplitView::new();
        split_view.set_min_sidebar_width(260.0);
        split_view.set_max_sidebar_width(340.0);
        split_view.set_sidebar(Some(&libadwaita::NavigationPage::new(
            &build_notes_sidebar(&search_entry, &list_box),
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
            list_box,
            preview_title,
            preview_meta,
            preview,
            open_button,
            back_button,
            filtered_entries: RefCell::new(entries.clone()),
            all_entries: entries,
        });

        rebuild_notes_browser_rows(&state, "");
        state.search_entry.connect_search_changed({
            let state = Rc::clone(&state);
            move |entry| {
                rebuild_notes_browser_rows(&state, entry.text().as_str());
            }
        });
        state.list_box.connect_row_selected({
            let state = Rc::clone(&state);
            move |_list, row| {
                state.refresh_preview(row, true);
            }
        });
        state.list_box.connect_row_activated({
            let state = Rc::clone(&state);
            move |_list, row| {
                state.refresh_preview(Some(row), true);
                state.open_selected();
            }
        });
        state.open_button.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                state.open_selected();
            }
        });
        state.back_button.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                state.split_view.set_show_content(false);
            }
        });
        state.split_view.connect_collapsed_notify({
            let state = Rc::clone(&state);
            move |split| {
                state.back_button.set_visible(split.is_collapsed());
            }
        });

        state.dialog.present(Some(self));
    }
}

/// Build the shared edit/render note surface used by range, document, and workspace notes.
#[must_use]
fn build_note_editor_surface(
    initial_text: &str,
    render_context: &MarkdownPreviewRenderContext,
    initial_mode: NoteViewMode,
    empty_preview_description: &'static str,
) -> (gtk4::Box, gtk4::TextView) {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);

    let stack = gtk4::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);

    let switcher = gtk4::StackSwitcher::new();
    switcher.set_stack(Some(&stack));
    switcher.set_hexpand(true);
    switcher.set_halign(gtk4::Align::Fill);
    content.append(&switcher);

    let note_view = gtk4::TextView::new();
    note_view.set_wrap_mode(gtk4::WrapMode::WordChar);
    note_view.set_vexpand(true);
    // TextView margin properties act as inner padding for the editable
    // document, so this gives the note body breathing room inside the box
    // instead of just pushing the whole widget farther from neighboring rows.
    note_view.set_left_margin(12);
    note_view.set_right_margin(12);
    note_view.set_top_margin(10);
    note_view.set_bottom_margin(10);
    note_view.buffer().set_text(initial_text);

    let note_scroll = gtk4::ScrolledWindow::builder()
        .min_content_height(180)
        .vexpand(true)
        .child(&note_view)
        .build();
    stack.add_titled(&note_scroll, Some("edit"), "Edit");

    let preview = LushtextMarkdownPreview::new();
    preview.set_hexpand(true);
    preview.set_vexpand(true);
    preview.show_placeholder(empty_preview_description);
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
        preview.show_placeholder(empty_preview_description);
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
            "Range notes, document notes, and workspace notes will appear here once you save one.",
        )
        .build();
    dialog.set_child(Some(&status));
    dialog
}

/// Build the browse rail used by the unified notes browser.
fn build_notes_sidebar(search_entry: &gtk4::SearchEntry, list_box: &gtk4::ListBox) -> gtk4::Box {
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
        .child(list_box)
        .build();
    content.append(&scroll);

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
            Self::Workspace { workspace_name, .. } => format!("Workspace Note · {workspace_name}"),
            Self::Document { path, .. } => {
                let file_name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                format!("Document Note · {file_name}")
            }
            Self::Range {
                start_line,
                end_line,
                style,
                ..
            } => format!(
                "{} · {}",
                style.label(),
                format_range_label(*start_line, *end_line)
            ),
        }
    }

    /// Secondary row text used for scope and location metadata.
    #[must_use]
    fn row_subtitle(&self) -> String {
        match self {
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
            Self::Range {
                workspace_name,
                path,
                start_line,
                end_line,
                ..
            } => format!(
                "{workspace_name} · {} · {}",
                path.display(),
                format_range_label(*start_line, *end_line)
            ),
        }
    }

    /// Optional row detail showing the first meaningful line of note text.
    #[must_use]
    fn row_detail(&self) -> Option<String> {
        let preview = note_preview_line(self.note_text());
        (!preview.is_empty()).then_some(preview)
    }

    /// Title shown in the preview header for the selected note.
    #[must_use]
    fn preview_title(&self) -> String {
        match self {
            Self::Workspace { workspace_name, .. } => format!("Workspace Note · {workspace_name}"),
            Self::Document { path, .. } => {
                let file_name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                format!("Document Note · {file_name}")
            }
            Self::Range {
                start_line,
                end_line,
                style,
                ..
            } => format!(
                "Range Note · {} · {}",
                style.label(),
                format_range_label(*start_line, *end_line)
            ),
        }
    }

    /// Secondary preview metadata shown under the selected note title.
    #[must_use]
    fn preview_meta(&self) -> String {
        match self {
            Self::Workspace {
                workspace_name,
                root,
                ..
            } => format!("{workspace_name} · {}", root.display()),
            Self::Document {
                workspace_name,
                path,
                ..
            }
            | Self::Range {
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
            Self::Workspace { note, .. } | Self::Document { note, .. } => &note.text,
            Self::Range { note_text, .. } => note_text,
        }
    }

    /// Render context used by the shared markdown preview widget.
    #[must_use]
    fn render_context(&self) -> MarkdownPreviewRenderContext {
        match self {
            Self::Workspace { root, .. } => {
                MarkdownPreviewRenderContext::new(None, vec![root.clone()])
            }
            Self::Document {
                path,
                workspace_root,
                ..
            }
            | Self::Range {
                path,
                workspace_root,
                ..
            } => {
                MarkdownPreviewRenderContext::new(Some(path.clone()), vec![workspace_root.clone()])
            }
        }
    }

    /// Return whether this entry matches one search query.
    #[must_use]
    fn matches_query(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }

        self.row_title().to_lowercase().contains(&query)
            || self.row_subtitle().to_lowercase().contains(&query)
            || self.note_text().to_lowercase().contains(&query)
    }
}

impl NotesBrowserState {
    /// Refresh the preview pane for one selected browser row.
    fn refresh_preview(&self, row: Option<&gtk4::ListBoxRow>, user_selected: bool) {
        let Some(row) = row else {
            self.preview_title.set_label("Select a note");
            self.preview_meta
                .set_label("Choose a workspace, document, or range note to preview it here.");
            self.preview
                .show_placeholder("Select a note to preview its rendered markdown.");
            self.open_button.set_sensitive(false);
            return;
        };

        let Ok(index) = usize::try_from(row.index()) else {
            return;
        };
        let Some(entry) = self.filtered_entries.borrow().get(index).cloned() else {
            self.preview_title.set_label("Select a note");
            self.preview_meta
                .set_label("Choose a workspace, document, or range note to preview it here.");
            self.preview
                .show_placeholder("Select a note to preview its rendered markdown.");
            self.open_button.set_sensitive(false);
            return;
        };

        self.preview_title.set_label(&entry.preview_title());
        self.preview_meta.set_label(&entry.preview_meta());
        if entry.note_text().trim().is_empty() {
            self.preview.show_placeholder("This note is empty.");
        } else {
            self.preview
                .render_markdown_with_context(entry.note_text(), &entry.render_context());
        }
        self.open_button.set_sensitive(true);

        if user_selected && self.split_view.is_collapsed() {
            self.split_view.set_show_content(true);
        }
    }

    /// Open the currently selected note through the same window workflows used elsewhere.
    fn open_selected(&self) {
        let Some(row) = self.list_box.selected_row() else {
            return;
        };
        let Ok(index) = usize::try_from(row.index()) else {
            return;
        };
        let Some(entry) = self.filtered_entries.borrow().get(index).cloned() else {
            return;
        };

        self.dialog.close();
        activate_notes_browser_entry(&self.window, &entry);
    }
}

/// Rebuild the list rows shown by the unified notes browser.
fn rebuild_notes_browser_rows(state: &Rc<NotesBrowserState>, query: &str) {
    clear_list_box_rows(&state.list_box);

    let filtered: Vec<_> = state
        .all_entries
        .iter()
        .filter(|entry| entry.matches_query(query))
        .cloned()
        .collect();
    *state.filtered_entries.borrow_mut() = filtered.clone();

    if filtered.is_empty() {
        let row = gtk4::ListBoxRow::new();
        row.set_selectable(false);
        row.set_activatable(false);
        row.set_child(Some(&empty_browser_label("No notes match that search")));
        state.list_box.append(&row);
        state.refresh_preview(None, false);
        return;
    }

    for entry in filtered {
        let row = gtk4::ListBoxRow::new();
        row.set_selectable(true);
        row.set_activatable(true);
        let detail = entry.row_detail();
        row.set_child(Some(&browser_row_content(
            &entry.row_title(),
            &entry.row_subtitle(),
            detail.as_deref(),
        )));
        state.list_box.append(&row);
    }

    if let Some(first_row) = state.list_box.row_at_index(0) {
        state.list_box.select_row(Some(&first_row));
        state.refresh_preview(Some(&first_row), false);
    }
}

/// Route one browser row back through the appropriate window workflow.
fn activate_notes_browser_entry(window: &LushtextWindow, entry: &NotesBrowserEntry) {
    match entry {
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
        NotesBrowserEntry::Range {
            path,
            annotation_id,
            start_line,
            ..
        } => {
            open_editor_at_line(window, path, start_line.saturating_add(1));
            if let Some(editor) = window.active_editor() {
                editor.set_pending_annotation_focus(Some(annotation_id.clone()));
                window.open_pending_annotation_if_ready(&editor);
            }
        }
    }
}

/// Merge workspace, document, and range notes into one browser entry list.
fn build_notes_browser_entries(
    visible_workspaces: &[WorkspaceConfig],
    workspace_notes: Vec<workspace_note_service::ListedWorkspaceNote>,
    document_notes: Vec<document_note_service::WorkspaceDocumentNote>,
    range_notes: Vec<annotation_service::WorkspaceAnnotation>,
) -> Vec<NotesBrowserEntry> {
    let mut entries = Vec::new();

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

    for note in range_notes {
        if let Some(workspace) = workspace_for_path(visible_workspaces, &note.path) {
            entries.push(NotesBrowserEntry::Range {
                workspace_name: workspace.name.clone(),
                workspace_root: workspace.root.clone(),
                path: note.path,
                annotation_id: note.annotation_id,
                start_line: note.start_line,
                end_line: note.end_line,
                note_text: note.note_text,
                style: note.style,
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

/// Display one inclusive zero-based line range in the 1-based form users expect.
#[must_use]
fn format_range_label(start_line: u32, end_line: u32) -> String {
    let start = start_line.saturating_add(1);
    let end = end_line.saturating_add(1);
    if start == end {
        format!("Line {start}")
    } else {
        format!("Lines {start}-{end}")
    }
}

/// Build the base dialog used by the bookmark and annotation browsers.
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

/// Build the content widget used inside bookmark and annotation browser rows.
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

/// Remove every row from a list box before rebuilding its filtered contents.
fn clear_list_box_rows(list_box: &gtk4::ListBox) {
    while let Some(row) = list_box.row_at_index(0) {
        list_box.remove(&row);
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

/// Map an annotation style to the matching dropdown row.
#[must_use]
fn annotation_style_index(style: AnnotationStyle) -> u32 {
    match style {
        AnnotationStyle::Note => 0,
        AnnotationStyle::Todo => 1,
        AnnotationStyle::Warning => 2,
        AnnotationStyle::Question => 3,
    }
}

/// Map the selected dropdown row back to a persisted annotation style.
#[must_use]
fn annotation_style_from_index(index: u32) -> AnnotationStyle {
    match index {
        1 => AnnotationStyle::Todo,
        2 => AnnotationStyle::Warning,
        3 => AnnotationStyle::Question,
        _ => AnnotationStyle::Note,
    }
}

/// Open a file at a specific 1-based line number and focus the editor.
fn open_editor_at_line(window: &LushtextWindow, path: &Path, line: u32) {
    window.open_document(path);

    let Some(editor) = window.active_editor() else {
        return;
    };

    let line_zero_based = line.saturating_sub(1);
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
    editor.source_view().grab_focus();
}
