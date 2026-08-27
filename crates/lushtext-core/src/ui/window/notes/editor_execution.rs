// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination role `execution`: the document-note and folder-note editors.
//!
//! One ordered stage order with two entry surfaces. Resolve the target, present
//! the shared Edit/Render editor, then save, clear, or discard.
//!
//! # Inversions
//!
//! 1. **Target resolution runs on a worker**; the dialog is presented from its
//!    completion, so the editor never blocks on sidecar I/O.
//! 2. **The folder chooser and both dialogs resume in `AdwAlertDialog::choose`
//!    after the dialog has already closed.** That is why a failed write must hand
//!    the typed prose *back*: at that point the editor holding the only copy is
//!    gone. See `NoteWriteOutcome`.
//! 3. **Large bodies are captured in bounded chunks**, resuming in the buffer
//!    snapshot's completion before the durable write is dispatched.
//! 4. **The write resumes in its own completion**, which on failure re-presents
//!    the editor pre-filled with the recovered text rather than reporting a
//!    transient message over lost work.
//!
//! Geometry note preserved from `modal-geometry-stability`: the hidden Render
//! page is pre-rendered for an existing non-empty note so the first mode switch
//! reveals already-measured content.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_settle::Debounce;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, AlertDialogExt, AlertDialogExtManual};

use crate::model::note::{NoteEditorPresentation, NoteViewMode, RichNoteBody};
use crate::services::{document_note_service, folder_note_service, json_store};
use crate::ui::markdown_preview::{LushtextMarkdownPreview, MarkdownPreviewRenderContext};
use crate::ui::status_bar::MessageKind;
use crate::ui::{accessibility, buffer_snapshot};

use super::LushtextWindow;
use super::policy::FolderNoteOpenTarget;
use super::policy::folder_note_target_for_workspace;

/// Outcome of one note write, carrying the user's prose back on failure.
///
/// This is deliberately **not** a `Result<_, anyhow::Error>`. An
/// `AdwAlertDialog::choose` callback fires after the dialog has already closed,
/// so by the time a write fails the editor holding the only copy of the typed
/// text is gone. A plain error arm therefore loses the prose to a transient
/// status message — reachable without exotic timing, because
/// `note_storage::resolve_document_identity` fails whenever another program
/// renamed or deleted the file while the dialog was open. Handing the owned text
/// back is what lets the completion re-present the editor with it.
enum NoteWriteOutcome {
    /// The note was written.
    Saved,
    /// The buffer was empty after trimming, so nothing was written.
    EmptyText,
    /// The write failed; the user's prose is handed back for recovery.
    Failed {
        /// The exact text the user typed, moved back off the worker.
        text: String,
        /// Why the write failed, for the log and the user-facing message.
        error: anyhow::Error,
    },
}

/// Alert-dialog response IDs reused by note editors.
const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_SAVE: &str = "save";
const RESPONSE_CLEAR: &str = "clear";
/// Stable width for the shared edit/render note surface inside note dialogs.
const NOTE_EDITOR_SURFACE_WIDTH_SP: i32 = 520;
/// Stable height for the shared edit/render note surface.
const NOTE_EDITOR_SURFACE_HEIGHT_SP: i32 = 300;
/// Shared horizontal text inset for edit and rendered note bodies.
const NOTE_EDITOR_TEXT_MARGIN_HORIZONTAL_SP: i32 = 12;
/// Shared vertical text inset for edit and rendered note bodies.
const NOTE_EDITOR_TEXT_MARGIN_VERTICAL_SP: i32 = 10;
/// Render-mode placeholder shown while a note has no text to render yet.
const NOTE_EDITOR_RENDER_PLACEHOLDER: &str = "Write some note text to preview rendered markdown.";

/// How one note editor should open: sensitivity source, text, and starting mode.
///
/// Both note dialogs derive this identically, so it is derived once here.
struct NoteEditorStart<'a> {
    /// Save sensitivity and the pre-render decision, derived from the **stored**
    /// note so a recovery re-open keeps the first open's dirty semantics.
    presentation: NoteEditorPresentation,
    /// The text the editor opens holding: recovered prose when a write failed,
    /// otherwise the stored note.
    initial_text: &'a str,
    /// The mode the editor opens in.
    initial_mode: NoteViewMode,
}

impl<'a> NoteEditorStart<'a> {
    /// Derive how the editor opens for one stored note and optional recovery.
    ///
    /// Only the visible text and the starting mode change on a recovery re-open;
    /// `presentation` still describes the stored note.
    fn derive(existing_note: Option<&'a RichNoteBody>, recovered_text: Option<&'a str>) -> Self {
        let loaded_text = existing_note.map(|note| note.text.as_str());
        let presentation = NoteEditorPresentation::from_loaded_text(loaded_text);
        let initial_mode = if recovered_text.is_some() {
            // The user is fixing a failed save; show them their prose, not a
            // rendered view of it.
            NoteViewMode::Edit
        } else {
            presentation.initial_mode()
        };
        Self {
            presentation,
            initial_text: recovered_text.or(loaded_text).unwrap_or(""),
            initial_mode,
        }
    }
}

/// Build the note body one save should persist.
///
/// Editing an existing note keeps its `created_at_secs` and lets `update_text`
/// refresh `updated_at_secs`; a first note is created outright. Both note
/// dialogs' workers need exactly this, so neither reconstructs it.
fn note_body_for_save(existing_note: Option<RichNoteBody>, note_text: &str) -> RichNoteBody {
    match existing_note {
        Some(mut note) => {
            let _ = note.update_text(note_text);
            note
        }
        None => RichNoteBody::new(note_text),
    }
}

impl LushtextWindow {
    /// Open the document note for the active saved file.
    pub(in crate::ui::window) fn open_document_note(&self) {
        let Some(editor) = self.require_saved_editor("Document notes require a saved file") else {
            return;
        };
        let Some(path) = editor.file_path() else {
            return;
        };
        self.open_document_note_for_path(&path);
    }

    /// Open the document note for a concrete saved file path.
    pub(in crate::ui::window) fn open_document_note_for_path(&self, path: &Path) {
        self.open_document_note_for_path_with_folders(path, self.current_workspace_folder_paths());
    }

    /// Open the folder note for the current concrete workspace scope.
    pub(in crate::ui::window) fn open_folder_note(&self) {
        self.open_folder_note_target(self.current_folder_note_open_target());
    }

    /// Open the folder note for a concrete workspace selected from the sidebar.
    pub(in crate::ui::window) fn open_folder_note_for_id(
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
    pub(in crate::ui::window) fn open_folder_note_for_workspace_folder(
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
    /// Load and present the document note attached to one saved file.
    pub(super) fn open_document_note_for_path_with_folders(
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
                            None,
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
    pub(super) fn open_folder_note_for_folder(&self, workspace_name: &str, folder: &Path) {
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
                            None,
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

    /// Present the document-note editor, optionally pre-filled with recovered prose.
    ///
    /// `recovered_text` is `Some` only when a previous write failed: the editor
    /// re-opens holding exactly what the user typed, in Edit mode, so a failed
    /// save is a retry rather than a loss.
    fn present_document_note_dialog(
        &self,
        path: &Path,
        workspace_folders: Vec<PathBuf>,
        existing_note: Option<&RichNoteBody>,
        recovered_text: Option<&str>,
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

        let start = NoteEditorStart::derive(existing_note, recovered_text);
        let (surface, note_view) = build_note_editor_surface(
            start.initial_text,
            &MarkdownPreviewRenderContext::new(Some(path.to_path_buf()), workspace_folders.clone()),
            start.initial_mode,
            NOTE_EDITOR_RENDER_PLACEHOLDER,
        );
        install_note_save_response_state(
            &dialog,
            &note_view.buffer(),
            start.presentation,
            start.initial_text,
        );
        content.append(&surface);
        dialog.set_extra_child(Some(&content));

        let path = path.to_path_buf();
        let existing_note = existing_note.cloned();
        let folders_for_save = workspace_folders;
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
                let window_weak = window.downgrade();
                let snapshot = snapshot_note_buffer_text(buffer, move |outcome| {
                    let Some(window_for_save) = window_weak.upgrade() else {
                        return;
                    };
                    window_for_save.prune_note_save_snapshots();
                    let buffer_snapshot::BufferSnapshotOutcome::Captured(note_text) = outcome
                    else {
                        window_for_save.publish_status_message(
                            "Document note changed while preparing the save",
                            MessageKind::Warning,
                        );
                        return;
                    };
                    let path_for_recovery = path_for_save.clone();
                    let folders_for_recovery = folders_for_save.clone();
                    let existing_for_recovery = existing_note_for_save.clone();
                    spawn_blocking_then(
                        window_for_save,
                        move || {
                            let note_text = note_text.into_string_on_worker();
                            if note_text.trim().is_empty() {
                                return NoteWriteOutcome::EmptyText;
                            }
                            let note = note_body_for_save(existing_note_for_save, &note_text);
                            let data_dir = json_store::data_dir();
                            match document_note_service::save_for_path(
                                &data_dir,
                                &path_for_save,
                                &note,
                            ) {
                                Ok(_) => NoteWriteOutcome::Saved,
                                // The prose travels back rather than being
                                // dropped with the error.
                                Err(error) => NoteWriteOutcome::Failed {
                                    text: note_text,
                                    error,
                                },
                            }
                        },
                        move |window, outcome| match outcome {
                            NoteWriteOutcome::Saved => {
                                window.refresh_command_palette_note_source_debounced();
                                window.publish_status_message(
                                    "Document note saved",
                                    MessageKind::Info,
                                );
                            }
                            NoteWriteOutcome::EmptyText => window.publish_status_message(
                                "Document notes need note text",
                                MessageKind::Warning,
                            ),
                            NoteWriteOutcome::Failed { text, error } => {
                                tracing::error!("Failed to save document note: {error}");
                                window.publish_status_message(
                                    "Document note could not be saved — reopened with your text",
                                    MessageKind::Error,
                                );
                                window.present_document_note_dialog(
                                    &path_for_recovery,
                                    folders_for_recovery,
                                    existing_for_recovery.as_ref(),
                                    Some(text.as_str()),
                                );
                            }
                        },
                    );
                });
                window.track_note_save_snapshot(snapshot);
            }
        });
    }

    /// Present the folder note editor, optionally pre-filled with recovered prose.
    ///
    /// `recovered_text` is `Some` only when a previous write failed; see
    /// [`Self::present_document_note_dialog`] for why the text travels back.
    fn present_folder_note_dialog(
        &self,
        workspace_name: &str,
        folder: &Path,
        existing_note: Option<&RichNoteBody>,
        recovered_text: Option<&str>,
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

        let start = NoteEditorStart::derive(existing_note, recovered_text);
        let (surface, note_view) = build_note_editor_surface(
            start.initial_text,
            &MarkdownPreviewRenderContext::new(None, vec![folder.to_path_buf()]),
            start.initial_mode,
            NOTE_EDITOR_RENDER_PLACEHOLDER,
        );
        install_note_save_response_state(
            &dialog,
            &note_view.buffer(),
            start.presentation,
            start.initial_text,
        );
        content.append(&surface);
        dialog.set_extra_child(Some(&content));

        let folder = folder.to_path_buf();
        let existing_note = existing_note.cloned();
        let workspace_name_for_recovery = workspace_name.to_string();
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
                let window_weak = window.downgrade();
                let snapshot = snapshot_note_buffer_text(buffer, move |outcome| {
                    let Some(window_for_save) = window_weak.upgrade() else {
                        return;
                    };
                    window_for_save.prune_note_save_snapshots();
                    let buffer_snapshot::BufferSnapshotOutcome::Captured(note_text) = outcome
                    else {
                        window_for_save.publish_status_message(
                            "Folder note changed while preparing the save",
                            MessageKind::Warning,
                        );
                        return;
                    };
                    let folder_for_recovery = folder_for_save.clone();
                    let name_for_recovery = workspace_name_for_recovery.clone();
                    let existing_for_recovery = existing_note_for_save.clone();
                    spawn_blocking_then(
                        window_for_save,
                        move || {
                            let note_text = note_text.into_string_on_worker();
                            if note_text.trim().is_empty() {
                                return NoteWriteOutcome::EmptyText;
                            }
                            let note = note_body_for_save(existing_note_for_save, &note_text);
                            let data_dir = json_store::data_dir();
                            match folder_note_service::save_for_folder(
                                &data_dir,
                                &folder_for_save,
                                &note,
                            ) {
                                Ok(_) => NoteWriteOutcome::Saved,
                                Err(error) => NoteWriteOutcome::Failed {
                                    text: note_text,
                                    error,
                                },
                            }
                        },
                        move |window, outcome| match outcome {
                            NoteWriteOutcome::Saved => {
                                window.refresh_command_palette_note_source_debounced();
                                window
                                    .publish_status_message("Folder note saved", MessageKind::Info);
                            }
                            NoteWriteOutcome::EmptyText => window.publish_status_message(
                                "Folder notes need note text",
                                MessageKind::Warning,
                            ),
                            NoteWriteOutcome::Failed { text, error } => {
                                tracing::error!("Failed to save folder note: {error}");
                                window.publish_status_message(
                                    "Folder note could not be saved — reopened with your text",
                                    MessageKind::Error,
                                );
                                window.present_folder_note_dialog(
                                    &name_for_recovery,
                                    &folder_for_recovery,
                                    existing_for_recovery.as_ref(),
                                    Some(text.as_str()),
                                );
                            }
                        },
                    );
                });
                window.track_note_save_snapshot(snapshot);
            }
        });
    }

    /// Retain a chunked note-save capture until it completes or the window is disposed.
    fn track_note_save_snapshot(&self, snapshot: Option<buffer_snapshot::BufferSnapshotHandle>) {
        self.prune_note_save_snapshots();
        if let Some(snapshot) = snapshot {
            self.imp().note_save_snapshots.borrow_mut().push(snapshot);
        }
    }

    /// Drop terminal weak handles without disturbing other concurrent note saves.
    fn prune_note_save_snapshots(&self) {
        self.imp()
            .note_save_snapshots
            .borrow_mut()
            .retain(buffer_snapshot::BufferSnapshotHandle::is_active);
    }
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
    dialog.connect_closed({
        let refresh = refresh.clone();
        move |_| {
            if let Some(snapshot) = refresh.snapshot.take() {
                snapshot.dispose();
            }
        }
    });
    buffer.connect_changed(move |buffer| {
        refresh.schedule(buffer);
    });
}

/// Main-loop state for coalescing Save sensitivity refreshes.
#[derive(Clone)]
struct NoteSaveResponseRefresh {
    dialog_weak: glib::WeakRef<libadwaita::AlertDialog>,
    presentation: Arc<NoteEditorPresentation>,
    debounce: Debounce,
    in_flight: Rc<Cell<bool>>,
    rerun_requested: Rc<Cell<bool>>,
    snapshot: Rc<RefCell<Option<buffer_snapshot::BufferSnapshotHandle>>>,
}

impl NoteSaveResponseRefresh {
    /// Create a refresh state object tied to one note editor dialog.
    fn new(dialog: &libadwaita::AlertDialog, presentation: NoteEditorPresentation) -> Self {
        Self {
            dialog_weak: dialog.downgrade(),
            presentation: Arc::new(presentation),
            debounce: Debounce::new(),
            in_flight: Rc::new(Cell::new(false)),
            rerun_requested: Rc::new(Cell::new(false)),
            snapshot: Rc::new(RefCell::new(None)),
        }
    }

    /// Schedule a trailing Save sensitivity refresh after live text edits.
    fn schedule(&self, buffer: &gtk4::TextBuffer) {
        let refresh = self.clone();
        self.debounce.schedule(
            buffer,
            Duration::from_millis(super::policy::NOTE_SAVE_RESPONSE_REFRESH_DEBOUNCE_MS),
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
        let snapshot_slot = Rc::clone(&self.snapshot);
        let snapshot = snapshot_note_buffer_text(buffer, move |outcome| {
            snapshot_slot.borrow_mut().take();
            let rerun_requested = refresh.rerun_requested.replace(false);

            if rerun_requested {
                refresh.in_flight.set(false);
                if refresh.dialog_weak.upgrade().is_some() {
                    refresh.queue(buffer_for_rerun);
                }
                return;
            }

            let buffer_snapshot::BufferSnapshotOutcome::Captured(text) = outcome else {
                refresh.in_flight.set(false);
                return;
            };
            let presentation = Arc::clone(&refresh.presentation);
            spawn_blocking_then(
                (refresh, buffer_for_rerun),
                move || {
                    let text = text.into_string_on_worker();
                    presentation.save_enabled_for(&text)
                },
                move |(refresh, buffer_for_rerun), save_enabled| {
                    let rerun_requested = refresh.rerun_requested.replace(false);
                    refresh.in_flight.set(false);
                    if rerun_requested {
                        if refresh.dialog_weak.upgrade().is_some() {
                            refresh.queue(buffer_for_rerun);
                        }
                        return;
                    }
                    if let Some(dialog) = refresh.dialog_weak.upgrade() {
                        dialog.set_response_enabled(RESPONSE_SAVE, save_enabled);
                    }
                },
            );
        });
        self.snapshot.replace(snapshot);
    }
}

/// Render one note body into the shared markdown preview widget.
fn render_note_preview(
    preview: &LushtextMarkdownPreview,
    buffer: &gtk4::TextBuffer,
    render_context: &MarkdownPreviewRenderContext,
    empty_preview_description: &'static str,
) {
    preview.replace_source_snapshot(None);
    let preview_weak = preview.downgrade();
    let render_context = render_context.clone();
    let snapshot = snapshot_note_buffer_text(buffer.clone(), move |outcome| {
        let Some(preview) = preview_weak.upgrade() else {
            return;
        };
        preview.clear_source_snapshot();
        let buffer_snapshot::BufferSnapshotOutcome::Captured(text) = outcome else {
            return;
        };
        preview.render_snapshot_with_context_or_placeholder(
            text,
            render_context,
            Some(empty_preview_description),
        );
    });
    preview.replace_source_snapshot(snapshot);
}

/// Snapshot note editor text without monopolizing the GTK main loop.
fn snapshot_note_buffer_text<F: FnOnce(buffer_snapshot::BufferSnapshotOutcome) + 'static>(
    buffer: gtk4::TextBuffer,
    callback: F,
) -> Option<buffer_snapshot::BufferSnapshotHandle> {
    if buffer_snapshot::buffer_requires_chunked_snapshot(&buffer) {
        Some(buffer_snapshot::snapshot_buffer_text_async(
            buffer, callback,
        ))
    } else {
        callback(buffer_snapshot::BufferSnapshotOutcome::Captured(
            buffer_snapshot::BufferSnapshotPayload::direct(
                buffer_snapshot::snapshot_buffer_text_direct(&buffer),
            ),
        ));
        None
    }
}
