// SPDX-License-Identifier: GPL-3.0-or-later

//! Bookmark and annotation workflows for the main window shell.
//!
//! This module keeps note-specific action handling, dialogs, persistence
//! scheduling, and workspace export logic out of the generic document shell.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, AlertDialogExt, AlertDialogExtManual};

use crate::model::annotation::{AnnotationRecord, AnnotationStyle};
use crate::services::{annotation_service, async_task, bookmark_service, editor_io, json_store};
use crate::ui::editor_page::{
    AnnotationEditSelection, BookmarkNavigationDirection, BookmarkToggleState, LushtextEditorPage,
};
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

/// Search scope title used by bookmark and annotation browse dialogs.
const WORKSPACE_SCOPE_TITLE: &str = "Current Workspace";

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
            }
        });

        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.connect_annotations_changed(move || {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
            {
                window.save_annotations_debounced(&editor);
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
                            "Bookmarks or annotations could not be loaded",
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
                Ok::<_, anyhow::Error>((bookmark_count, annotation_count))
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
        let Some(editor) = self.require_saved_editor("Annotations require a saved file") else {
            return;
        };
        self.present_annotation_editor(&editor, None);
    }

    /// Edit the annotation under the current cursor line.
    pub(super) fn edit_annotation(&self) {
        let Some(editor) = self.require_saved_editor("Annotations require a saved file") else {
            return;
        };
        let Some(annotation) = editor.current_annotation() else {
            self.publish_status_message(
                "Move the cursor onto an annotation first",
                MessageKind::Warning,
            );
            return;
        };
        self.present_annotation_editor(&editor, Some(&annotation));
    }

    /// Browse workspace annotations in a searchable dialog.
    pub(super) fn show_annotations_dialog(&self) {
        let scope_paths = self.workspace_note_scope_paths();
        if scope_paths.is_empty() {
            self.publish_status_message(
                "Add a workspace before browsing annotations",
                MessageKind::Warning,
            );
            return;
        }

        async_task::spawn_blocking_then(
            self.clone(),
            move || {
                let data_dir = json_store::data_dir();
                annotation_service::list_workspace_annotations(&data_dir, &scope_paths)
            },
            |window, result| match result {
                Ok(annotations) => {
                    if annotations.is_empty() {
                        window.publish_status_message(
                            "No annotations exist in the current workspace",
                            MessageKind::Info,
                        );
                        return;
                    }

                    window.present_annotation_browser(annotations);
                }
                Err(error) => {
                    tracing::error!("Failed to list workspace annotations: {error}");
                    window.publish_status_message(
                        "Annotations could not be listed",
                        MessageKind::Error,
                    );
                }
            },
        );
    }

    /// Export annotations for the current workspace scope to a markdown file.
    pub(super) fn export_annotations(&self) {
        let scope_paths = self.workspace_note_scope_paths();
        if scope_paths.is_empty() {
            self.publish_status_message(
                "Add a workspace before exporting annotations",
                MessageKind::Warning,
            );
            return;
        }

        let dialog = gtk4::FileDialog::builder()
            .title("Export Annotations")
            .modal(true)
            .build();
        dialog.set_initial_name(Some("annotations.md"));

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
                        &format!("Annotation report saved to {}", path.display()),
                        MessageKind::Info,
                    ),
                    Err(error) => {
                        tracing::error!("Failed to export annotations: {error}");
                        window
                            .publish_status_message("Annotation export failed", MessageKind::Error);
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
                            .publish_status_message("Annotation save failed", MessageKind::Warning);
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
        self.imp().sidebar.filtered_workspace_scope_paths()
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

    /// Present the searchable annotation browser dialog.
    fn present_annotation_browser(
        &self,
        annotations: Vec<annotation_service::WorkspaceAnnotation>,
    ) {
        let dialog = build_browser_dialog("Annotations");
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

        let annotations = Rc::new(annotations);
        rebuild_annotation_rows(self, &dialog, &rows_box, &annotations, "");

        let window = self.clone();
        let dialog_weak = dialog.downgrade();
        let rows_box = rows_box.clone();
        let annotations_for_search = annotations.clone();
        search_entry.connect_search_changed(move |entry| {
            let Some(dialog) = dialog_weak.upgrade() else {
                return;
            };
            rebuild_annotation_rows(
                &window,
                &dialog,
                &rows_box,
                &annotations_for_search,
                entry.text().as_str(),
            );
        });

        dialog.present(Some(self));
    }

    /// Present the create/edit dialog for a single annotation.
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
                    "Update the annotation for {}.",
                    annotation.line_range_label()
                )
            }
            AnnotationEditSelection::NewRange {
                start_line,
                end_line,
            } => format!(
                "Add a note for {}.",
                AnnotationRecord::new(*start_line, *end_line, "", AnnotationStyle::Note,)
                    .line_range_label()
            ),
        };

        let dialog = libadwaita::AlertDialog::new(
            Some(if existing.is_some() {
                "Edit Annotation"
            } else {
                "New Annotation"
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
        if let Some(annotation) = existing {
            note_view.buffer().set_text(&annotation.note_text);
        }
        let note_scroll = gtk4::ScrolledWindow::builder()
            .min_content_height(180)
            .vexpand(true)
            .child(&note_view)
            .build();
        content.append(&note_scroll);
        dialog.set_extra_child(Some(&content));

        let window = self.clone();
        let editor = editor.clone();
        let existing = existing.cloned();
        dialog.choose(Some(self), gio::Cancellable::NONE, move |response| {
            if response == RESPONSE_DELETE {
                if let Some(annotation) = existing.as_ref()
                    && editor.delete_annotation(&annotation.id)
                {
                    window.publish_status_message("Annotation deleted", MessageKind::Info);
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
                        .publish_status_message("Annotations need note text", MessageKind::Warning);
                    return;
                }

                let style = annotation_style_from_index(style_dropdown.selected());
                if let Some(annotation) = existing.as_ref() {
                    if editor
                        .update_annotation(&annotation.id, &note_text, style)
                        .is_some()
                    {
                        window.publish_status_message("Annotation updated", MessageKind::Info);
                    }
                } else {
                    let _ = editor.create_annotation_from_selection(&note_text, style);
                    window.publish_status_message("Annotation added", MessageKind::Info);
                }
            }
        });
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

/// Rebuild the annotation rows that match `query`.
fn rebuild_annotation_rows(
    window: &LushtextWindow,
    dialog: &libadwaita::Dialog,
    rows_box: &gtk4::Box,
    annotations: &[annotation_service::WorkspaceAnnotation],
    query: &str,
) {
    clear_box_children(rows_box);

    let filtered: Vec<_> = annotations
        .iter()
        .filter(|annotation| annotation_matches_query(annotation, query))
        .cloned()
        .collect();
    if filtered.is_empty() {
        rows_box.append(&empty_browser_label("No annotations match that search"));
        return;
    }

    for annotation in filtered {
        let preview = annotation
            .note_text
            .lines()
            .next()
            .unwrap_or("")
            .to_string();
        let button = gtk4::Button::new();
        button.add_css_class("flat");
        button.set_hexpand(true);
        button.set_halign(gtk4::Align::Fill);
        button.set_child(Some(&browser_row_content(
            &format!(
                "{} · {}",
                annotation.style.label(),
                annotation.line_range_label()
            ),
            &format!("{} · {}", annotation.path.display(), preview),
            Some(&annotation.note_text),
        )));

        let window = window.clone();
        let dialog_weak = dialog.downgrade();
        button.connect_clicked(move |_| {
            let line = annotation.start_line.saturating_add(1);
            if let Some(dialog) = dialog_weak.upgrade() {
                dialog.close();
            }
            open_editor_at_line(&window, &annotation.path, line);
            if let Some(editor) = window.active_editor() {
                editor.set_pending_annotation_focus(Some(annotation.annotation_id.clone()));
                window.open_pending_annotation_if_ready(&editor);
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

/// Filter annotation rows by style, path, note text, or 1-based line range.
#[must_use]
fn annotation_matches_query(
    annotation: &annotation_service::WorkspaceAnnotation,
    query: &str,
) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }

    annotation.style.label().to_lowercase().contains(&query)
        || annotation
            .path
            .display()
            .to_string()
            .to_lowercase()
            .contains(&query)
        || annotation.note_text.to_lowercase().contains(&query)
        || annotation
            .line_range_label()
            .to_lowercase()
            .contains(&query)
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
