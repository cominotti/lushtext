// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination role `execution`: the bookmark lifecycle and editor note resolution.
//!
//! Two of `WFR-NOTES-BOOKMARKS`'s ordered stage orders live here because they
//! share the editor's live `GtkSourceMark` projection and its sidecar identity:
//!
//! - **Editor note resolution** — a file finishes loading, `resolve_notes_for_editor`
//!   reads the bookmark sidecar on a worker, and the result installs only if the
//!   editor still owns the same path and the same bookmark generation.
//! - **Bookmark lifecycle** — toggle and label edit, debounced sidecar
//!   persistence with bounded failure retry and a synchronous close-time flush,
//!   and the closed-file excerpt preview.
//!
//! # Inversions
//!
//! 1. **Sidecar load resumes on a worker completion**, which re-checks path and
//!    bookmark generation, then sets `sidecar_resolved` and re-drives any write
//!    the unread-sidecar guard deferred.
//! 2. **Persistence resumes after the debounce quiet window**, and again in the
//!    write completion — which on failure re-arms the dirty flag and reschedules,
//!    bounded by `policy::MAX_BOOKMARK_SAVE_ATTEMPTS`.
//! 3. **The closed-file excerpt load resumes in a completion** validated against
//!    a `seams::NotesBrowserTicket<PreviewFlight>`, then starts the one retained
//!    latest request.
//!
//! Pure decisions — the target-line parse, the edit-error and
//! preview-unavailable messages, and the raw-excerpt formatter — are in `policy`.

use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::translate::IntoGlib;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::pango;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, PreferencesGroupExt};

use crate::model::bookmark::BookmarkRecord;
use crate::model::palette::PaletteNoteTarget;
use crate::services::{bookmark_excerpt, bookmark_service, json_store};
use crate::ui::accessibility;
use crate::ui::editor_page::{
    BookmarkEditError, BookmarkNavigationDirection, BookmarkToggleState, LushtextEditorPage,
};
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;
use super::browser::NotesBrowserEntryExt;
use super::browser::{NOTES_PREVIEW_RAW_CHILD, NotesBrowserEntry, NotesBrowserState};
use super::chrome::build_dialog_close_button;
use super::policy::{
    bookmark_edit_error_message, bookmark_unavailable_description, format_raw_bookmark_excerpt,
    parse_bookmark_target_line,
};
use super::seams::{NotesBrowserFacts, NotesBrowserTicket, PreviewFlight};

/// Text tag applied to the bookmarked row inside the raw preview surface.
const NOTES_RAW_BOOKMARK_TARGET_TAG: &str = "bookmark-target-line";

impl LushtextWindow {
    /// Wire bookmark and note callbacks for a newly created editor page.
    pub(in crate::ui::window) fn wire_note_callbacks(&self, editor: &LushtextEditorPage) {
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
    pub(in crate::ui::window) fn resolve_notes_for_editor(
        &self,
        editor: &LushtextEditorPage,
        path: &Path,
    ) {
        let path = path.to_path_buf();
        let path_for_load = path.clone();
        let started_at_generation = editor.bookmark_change_generation();
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
                // Only a **successful** read opens the guard. A failed read tells
                // us nothing about what the sidecar contains, so treating it as
                // resolved would let the very next empty write delete a sidecar
                // that may well be intact — the error arm clears the live
                // projection, which is exactly an empty set. The failure path
                // below reports and retries instead.
                if result.is_ok() {
                    editor
                        .imp()
                        .bookmarks
                        .persistence
                        .sidecar_resolved
                        .set(true);
                }
                // A write deferred by that guard has no other way back: nothing
                // else consults `save_dirty` outside a live flight's completion,
                // so without this the deferral would wait for the user's next
                // edit or, worse, for the close-time flush.
                //
                // But only re-drive a write that would *do* something. The common
                // case by far is a document with no bookmarks at all: the live set
                // is empty and the sidecar just loaded as empty, so there is
                // nothing to persist and nothing to delete. Re-arming the debounce
                // there would add a timer and a worker to **every** file load and
                // every Save As, which is latency spent to write nothing.
                let deferred_write_matters = editor.imp().bookmarks.persistence.save_dirty.get()
                    && !(editor.bookmark_records().is_empty()
                        && result.as_ref().is_ok_and(Vec::is_empty));
                if deferred_write_matters {
                    if let Some(window) = window_weak.upgrade() {
                        window.save_bookmarks_debounced(&editor);
                    }
                } else {
                    editor.imp().bookmarks.persistence.save_dirty.set(false);
                }
                match result {
                    Ok(bookmarks) => {
                        if !editor
                            .load_bookmarks_if_generation_matches(&bookmarks, started_at_generation)
                        {
                            return;
                        }
                        if let Some(window) = window_weak.upgrade() {
                            window.refresh_command_palette_note_source_debounced();
                            window.refresh_status_bar();
                        }
                    }
                    Err(error) => {
                        if editor.bookmark_change_generation() != started_at_generation {
                            return;
                        }
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

    /// Re-read every open saved editor's note sidecars.
    ///
    /// Used after a startup migration reconcile moves sidecars a restored tab has
    /// already read.
    pub(super) fn resolve_notes_for_open_editors(&self) {
        let Some(tab_view) = self.imp().tab_view.try_get() else {
            return;
        };
        for index in 0..tab_view.n_pages() {
            if let Some(editor) = tab_view
                .nth_page(index)
                .child()
                .downcast_ref::<LushtextEditorPage>()
                && let Some(path) = editor.file_path()
            {
                self.resolve_notes_for_editor(editor, &path);
            }
        }
    }

    /// Reset live note state after Save As so the new path starts from its own identity.
    pub(in crate::ui::window) fn reset_notes_after_save_as(
        &self,
        editor: &LushtextEditorPage,
        path: &Path,
    ) {
        editor.clear_bookmarks();
        // The new path has its own sidecar, which has not been read back yet.
        // Leaving the flag set from the old identity would disarm the
        // unread-sidecar guard for the whole resolve window, so a Save As onto a
        // path that already has bookmarks could delete them.
        editor
            .imp()
            .bookmarks
            .persistence
            .sidecar_resolved
            .set(false);
        self.resolve_notes_for_editor(editor, path);
        self.refresh_command_palette_note_source_debounced();
    }

    /// Toggle the bookmark on the current cursor line.
    pub(in crate::ui::window) fn toggle_bookmark(&self) {
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
    pub(in crate::ui::window) fn edit_bookmark(&self) {
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
    pub(super) fn present_bookmark_edit_dialog(
        &self,
        editor: &LushtextEditorPage,
        bookmark: &BookmarkRecord,
    ) {
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

        // Editing either field clears the same validation feedback, so both rows
        // connect one handler rather than two copies of it.
        let clear_feedback = {
            let error_weak = error_label.downgrade();
            let line_row_weak = line_row.downgrade();
            move || {
                if let Some(error_label) = error_weak.upgrade() {
                    clear_bookmark_edit_error(&error_label);
                }
                if let Some(line_row) = line_row_weak.upgrade() {
                    accessibility::set_invalid(&line_row, false);
                }
            }
        };
        let clear_feedback_for_label = clear_feedback.clone();
        line_row.connect_changed(move |_| clear_feedback());
        label_row.connect_changed(move |_| clear_feedback_for_label());

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
    pub(in crate::ui::window) fn navigate_bookmark_action(
        &self,
        direction: BookmarkNavigationDirection,
    ) {
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
    pub(in crate::ui::window) fn show_bookmarks_dialog(&self) {
        let workspace_folders = self.workspace_folder_paths_for_notes();
        if workspace_folders.is_empty() {
            self.publish_status_message(
                "Add a workspace before browsing bookmarks",
                MessageKind::Warning,
            );
            return;
        }
        self.show_notes_browser_mode(crate::services::palette::NotesBrowserMode::Bookmarks);
    }
    /// Debounce bookmark persistence so one burst of edits produces one sidecar write.
    pub(super) fn save_bookmarks_debounced(&self, editor: &LushtextEditorPage) {
        // `save_dirty` now means "the live bookmark set is not on disk", set the
        // moment a write is requested rather than only when one is already in
        // flight. That is what lets `flush_bookmarks_for_editor` tell an
        // unpersisted editor from a clean one at close time, and it is why a
        // failed write can re-arm rather than silently dropping the change.
        editor.imp().bookmarks.persistence.save_dirty.set(true);
        let window_weak = self.downgrade();
        editor.imp().bookmarks.persistence.save_debounce.schedule(
            editor,
            Duration::from_millis(super::policy::NOTES_SAVE_DEBOUNCE_MS),
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
    ///
    /// A failed write **re-arms the dirty flag and reschedules**. Before that,
    /// the dirty flag was cleared before the write and the error arm restored
    /// nothing, so one transient failure left the bookmarks in memory
    /// indefinitely: nothing else consults `save_dirty` outside a live flight's
    /// completion, so the next toggle was the only thing that could ever retry.
    /// Workspace persistence has always had a retry path; this is the same
    /// contract for the sidecar the same window owns.
    fn persist_bookmarks_now(&self, editor: &LushtextEditorPage) {
        let Some(path) = editor.file_path() else {
            return;
        };
        let bookmarks = editor.bookmark_records();
        // Never let an unread sidecar be overwritten by an empty live set:
        // `save_for_path` deletes the sidecar when the set is empty, so this
        // would silently destroy every bookmark for a file whose sidecar has not
        // been read back yet. Keep the write outstanding instead.
        if bookmarks.is_empty() && !editor.imp().bookmarks.persistence.sidecar_resolved.get() {
            tracing::debug!("Deferring bookmark write until the sidecar has been read back");
            editor.imp().bookmarks.persistence.save_dirty.set(true);
            return;
        }
        let data_dir = json_store::data_dir();
        editor.imp().bookmarks.persistence.save_inflight.set(true);
        editor.imp().bookmarks.persistence.save_dirty.set(false);

        // Captured at dispatch and re-checked on the worker: if the close-time
        // flush supersedes this write, its older snapshot must not land after the
        // flush's newer one.
        let save_generation =
            std::sync::Arc::clone(&editor.imp().bookmarks.persistence.save_generation);
        let issued = save_generation.load(std::sync::atomic::Ordering::Acquire);
        let window_weak = self.downgrade();
        spawn_blocking_then(
            editor.clone(),
            move || {
                if save_generation.load(std::sync::atomic::Ordering::Acquire) != issued {
                    tracing::debug!("Skipping superseded bookmark write");
                    return Ok(());
                }
                bookmark_service::save_for_path(&data_dir, &path, &bookmarks).map(|_| ())
            },
            move |editor, result| {
                editor.imp().bookmarks.persistence.save_inflight.set(false);
                let write_succeeded = result.is_ok();
                if write_succeeded {
                    // A successful write makes this editor the sidecar's author,
                    // so a later empty set is a real removal rather than an
                    // unloaded projection and may delete the sidecar.
                    editor
                        .imp()
                        .bookmarks
                        .persistence
                        .sidecar_resolved
                        .set(true);
                }
                let persistence = &editor.imp().bookmarks.persistence;
                // Read **before** the failure arm sets it: this distinguishes "a
                // newer edit arrived while this write was in flight" from "this
                // write failed and is still outstanding". Conflating the two is
                // what made an earlier version of this retry unbounded — the
                // failure arm set the flag, the immediate re-persist path then saw
                // it, and the two fed each other forever.
                let newer_edit_arrived = persistence.save_dirty.get();
                let retry_after_failure = if let Err(error) = result {
                    let streak = persistence.save_failure_streak.get().saturating_add(1);
                    persistence.save_failure_streak.set(streak);
                    tracing::error!("Failed to save bookmarks (attempt {streak}): {error}");
                    // Report once per streak, not once per retry: a persistently
                    // unwritable sidecar must not pulse the status bar.
                    if streak == 1
                        && let Some(window) = window_weak.upgrade()
                    {
                        window.publish_status_message("Bookmark save failed", MessageKind::Warning);
                    }
                    // The live set is still not on disk, so say so. This is what
                    // makes the close-time flush try once more after a failure
                    // streak instead of treating the editor as clean.
                    persistence.save_dirty.set(true);
                    // Bounded retry: past the cap nothing reschedules on its own,
                    // and the outstanding write waits for the user's next bookmark
                    // edit or for the close flush. Unbounded retry would churn the
                    // worker pool for as long as the sidecar stays unwritable.
                    streak < super::policy::MAX_BOOKMARK_SAVE_ATTEMPTS
                } else {
                    persistence.save_failure_streak.set(0);
                    false
                };
                if let Some(window) = window_weak.upgrade() {
                    if retry_after_failure {
                        // Failures go through the debounce, which is what bounds
                        // the worker churn.
                        window.save_bookmarks_debounced(&editor);
                    } else if newer_edit_arrived && write_succeeded {
                        // A newer edit that arrived mid-flight is re-persisted
                        // **immediately**, as it always was. Routing the success
                        // path through the debounce too would widen the very
                        // window the close-time flush exists to close.
                        window.persist_bookmarks_now(&editor);
                    }
                }
            },
        );
    }

    /// Flush every open editor's pending bookmark write before the window closes.
    pub(in crate::ui::window) fn flush_all_pending_bookmarks(&self) {
        let Some(tab_view) = self.imp().tab_view.try_get() else {
            return;
        };
        for index in 0..tab_view.n_pages() {
            if let Some(editor) = tab_view
                .nth_page(index)
                .child()
                .downcast_ref::<LushtextEditorPage>()
            {
                self.flush_bookmarks_for_editor(editor);
            }
        }
    }

    /// Flush any pending bookmark write for one editor before it goes away.
    ///
    /// `Debounce::schedule` holds its target **weakly**, so a scheduled bookmark
    /// write is simply dropped when the tab or window is torn down. Nothing in
    /// the close chain or in tab detachment touched the bookmark persistence
    /// state, so a bookmark added within the quiet window before closing was
    /// silently lost. Workspace persistence already participates in the close
    /// chain; this gives the sidecar the same treatment, synchronously, because
    /// there is no later turn in which to finish.
    ///
    /// **Why a blocking write on the GTK thread is acceptable here, stated so it
    /// is not copied somewhere it is not.** A bookmark sidecar is a small JSON
    /// document of line numbers and short labels — never document text — so the
    /// write is bounded by a few kilobytes rather than by buffer size. If an
    /// in-flight worker write holds the same target's write guard, this call waits
    /// for it, which cannot deadlock: the worker needs no GTK turn to finish.
    /// This is a teardown path with no later turn, which is the only reason it is
    /// synchronous at all.
    pub(in crate::ui::window) fn flush_bookmarks_for_editor(&self, editor: &LushtextEditorPage) {
        // A disposed widget is a stage. `bookmark_records()` reads the live
        // `GtkSourceMark` projection through the editor's `source_view` template
        // child, whose panicking accessor is exactly what a teardown-path read
        // must not use. Answering "nothing to flush" is the honest response.
        if editor.imp().source_view.try_get().is_none() {
            return;
        }
        let persistence = &editor.imp().bookmarks.persistence;
        let _ = persistence.save_debounce.invalidate();
        if !persistence.save_dirty.get() {
            return;
        }
        // Supersede any in-flight worker write **before** writing. Both writes
        // replace the whole sidecar, so without this the worker's older snapshot
        // could land after this newer one and revert it — the flush would lose
        // exactly the bookmark it exists to save. The worker re-checks this token
        // inside the target write guard and skips when it is stale.
        persistence
            .save_generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let Some(path) = editor.file_path() else {
            return;
        };
        let bookmarks = editor.bookmark_records();
        // The same unread-sidecar guard `persist_bookmarks_now` applies. Without
        // it this flush is the one place that would write an empty set over a
        // sidecar the editor never read — and because `save_document` deletes on
        // empty, that is a deletion, not a stale write. The dirty flag is left
        // set so the state stays honest.
        if bookmarks.is_empty() && !persistence.sidecar_resolved.get() {
            tracing::debug!("Skipping bookmark flush until the sidecar has been read back");
            return;
        }
        persistence.save_dirty.set(false);
        // Synchronous on purpose: the editor page is about to be detached, so a
        // worker completion would arrive after its target is gone.
        if let Err(error) =
            bookmark_service::save_for_path(&json_store::data_dir(), &path, &bookmarks)
        {
            tracing::error!(
                "Failed to flush bookmarks for {} before close: {error}",
                path.display()
            );
            self.publish_status_message(
                "Bookmarks could not be saved before closing",
                MessageKind::Warning,
            );
        }
    }
}

impl NotesBrowserState {
    /// Resolve and render a bookmark preview for the selected row.
    pub(super) fn refresh_bookmark_preview(state: &Rc<Self>, entry: &NotesBrowserEntry) {
        let PaletteNoteTarget::Bookmark { path, line, .. } = &entry.target else {
            return;
        };

        let presentation = bookmark_excerpt::presentation_for_path(path);
        if let Some(editor) = state.window.open_editor_for_path(path) {
            // Live-editor previews bypass closed-file workers entirely; the
            // caller already invalidated older closed-file work.
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

        let start = state.preview_loads.borrow_mut().submit(
            bookmark_excerpt::BookmarkExcerptPreviewRequest {
                path: path.clone(),
                line: *line,
            },
        );
        if let Some(start) = start {
            Self::start_bookmark_preview_load(state, start);
        }
    }

    /// Launch the sole active closed-file excerpt worker for one admitted request.
    fn start_bookmark_preview_load(
        state: &Rc<Self>,
        start: bookmark_excerpt::BookmarkExcerptPreviewStart,
    ) {
        if start.cancellation.is_cancelled() {
            Self::finish_bookmark_preview_load(state, start.generation, None);
            return;
        }

        let bookmark_excerpt::BookmarkExcerptPreviewStart {
            generation,
            request,
            cancellation,
        } = start;
        let path = request.path.clone();
        let line = request.line;
        let state_weak = Rc::downgrade(state);
        spawn_blocking_then(
            (),
            move || {
                #[cfg(feature = "test-utils")]
                super::test_policy::delay_bookmark_excerpt_preview();
                bookmark_excerpt::load_from_path_cancellable(
                    &request.path,
                    request.line,
                    &cancellation,
                )
            },
            move |(), outcome| {
                let Some(state) = state_weak.upgrade() else {
                    return;
                };
                let completion = match outcome {
                    bookmark_excerpt::BookmarkExcerptLoadOutcome::Completed(result) => {
                        Some((path, line, result))
                    }
                    bookmark_excerpt::BookmarkExcerptLoadOutcome::Cancelled => None,
                };
                Self::finish_bookmark_preview_load(&state, generation, completion);
            },
        );
    }

    /// Retire one active excerpt terminal, publish if current, then start the latest request.
    ///
    /// Every terminal (success, unavailable, cancelled, and the pre-cancelled
    /// short circuit) passes through this single transition so active ownership
    /// clears exactly once and a retained pending request cannot stall.
    fn finish_bookmark_preview_load(
        state: &Rc<Self>,
        generation: u64,
        completion: Option<(
            std::path::PathBuf,
            u32,
            bookmark_excerpt::BookmarkExcerptState,
        )>,
    ) {
        let ticket = NotesBrowserTicket::<PreviewFlight>::new(generation, state.mode.get());
        let (accepted, next) = {
            let mut loads = state.preview_loads.borrow_mut();
            let accepted = ticket.may_publish(&NotesBrowserFacts::new(
                loads.is_current(ticket.generation()),
                state.mode.get(),
                state.disposed.get(),
            ));
            let next = loads.finish(ticket.generation());
            (accepted, next)
        };
        if accepted && let Some((path, line, result)) = completion {
            state.apply_bookmark_preview_completion(&path, line, result);
        }
        if let Some(next) = next {
            Self::start_bookmark_preview_load(state, next);
        }
    }

    /// Apply a closed-file preview only if it still belongs to the selected row.
    fn apply_bookmark_preview_completion(
        &self,
        path: &Path,
        line: u32,
        result: bookmark_excerpt::BookmarkExcerptState,
    ) {
        if !self.selected_bookmark_matches(path, line) {
            return;
        }

        let Some(entry_index) = self.selected_entry_index() else {
            return;
        };
        let all_entries = self.all_entries.borrow();
        let Some(entry) = all_entries.get(entry_index) else {
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
    /// Check that an async bookmark completion still belongs to the selected row.
    fn selected_bookmark_matches(&self, path: &Path, line: u32) -> bool {
        let Some(entry_index) = self.selected_entry_index() else {
            return false;
        };
        matches!(
            self.all_entries
                .borrow()
                .get(entry_index)
                .map(|entry| &entry.target),
            Some(PaletteNoteTarget::Bookmark {
                path: selected_path,
                line: selected_line,
                ..
            }) if selected_path == path && *selected_line == line
        )
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
pub(super) fn ensure_raw_preview_target_tag(buffer: &gtk4::TextBuffer) -> gtk4::TextTag {
    let table = buffer.tag_table();
    if let Some(tag) = table.lookup(NOTES_RAW_BOOKMARK_TARGET_TAG) {
        return tag;
    }

    let tag = gtk4::TextTag::new(Some(NOTES_RAW_BOOKMARK_TARGET_TAG));
    tag.set_weight(pango::Weight::Bold.into_glib());
    table.add(&tag);
    tag
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
/// Open a file at a specific 1-based line number and focus the editor.
pub(super) fn open_editor_at_line(window: &LushtextWindow, path: &Path, line: u32) {
    window.open_document(path);

    let Some(editor) = window.active_editor() else {
        return;
    };

    let line_zero_based = line.saturating_sub(1);
    // `is_evicted()` is read first so an evicted editor's buffer is not touched.
    let evicted = editor.is_evicted();
    if evicted || editor.buffer().char_count() == 0 {
        // There is no installed text to place a cursor in yet, so the target
        // line goes to the restore path instead of the buffer.
        editor.set_restore_position(line_zero_based, 0, line_zero_based.saturating_sub(3));
        if evicted {
            window.reload_if_evicted();
        }
    } else {
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
    }
    editor.source_view().grab_focus();
}
