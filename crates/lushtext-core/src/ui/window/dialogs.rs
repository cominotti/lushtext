// SPDX-License-Identifier: GPL-3.0-or-later

//! File dialogs for the main window: open file, open folder, save as,
//! and save-changes confirmation on close.

use crate::ui::accessibility::{self, AnnouncementLane};
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;
#[cfg(feature = "test-utils")]
use std::sync::atomic::{AtomicU64, Ordering};

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_DISCARD: &str = "discard";
const RESPONSE_SAVE: &str = "save";
#[cfg(feature = "test-utils")]
static CLOSE_SAFETY_COMPLETION_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Delay final close publication so widget tests can force freshness races.
#[cfg(feature = "test-utils")]
pub fn set_close_safety_completion_delay_for_test(delay_ms: u64) {
    CLOSE_SAFETY_COMPLETION_DELAY_MS.store(delay_ms, Ordering::Release);
}

impl super::LushtextWindow {
    pub fn show_open_file_dialog(&self) {
        self.imp().open_popover.popdown();

        let dialog = gtk4::FileDialog::builder()
            .title("Open File")
            .modal(true)
            .build();

        let window = self.clone();
        dialog.open(Some(self), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                window.handle_open_file_selection(&file);
            }
        });
    }

    pub fn show_save_as_dialog(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };

        let dialog = gtk4::FileDialog::builder()
            .title("Save As")
            .modal(true)
            .build();

        // Pre-populate with current filename if available
        if let Some(name) = editor.file_path().as_deref().and_then(|p| p.file_name()) {
            dialog.set_initial_name(Some(&name.to_string_lossy()));
        }

        let window = self.clone();
        dialog.save(Some(self), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                window.handle_save_as_selection(&editor, &file);
            }
        });
    }

    /// Complete the Open File chooser after GTK or a portal has produced a file.
    ///
    /// Cancellation intentionally does not call this helper, so no document
    /// state changes until a concrete selection is available.
    fn handle_open_file_selection(&self, file: &gio::File) {
        if let Some(path) = file.path() {
            self.open_document(&path);
        } else {
            self.report_unsupported_open_file(file);
        }
    }

    /// Complete the Save As chooser after the user selects a destination.
    /// The editor adopts the new identity only inside `complete_save_as`, after
    /// the background durable write reports success.
    fn handle_save_as_selection(&self, editor: &LushtextEditorPage, file: &gio::File) {
        let Some(path) = file.path() else {
            let uri = file.uri();
            self.publish_status_message(
                &format!("Could not save to {uri}: only local files are supported"),
                MessageKind::Error,
            );
            self.refresh_status_bar();
            return;
        };
        let old_path = editor.file_path();
        let old_canonical_path = editor.canonical_file_path();
        let old_draft_id = editor.draft_id();
        let editor = editor.clone();
        let editor_for_result = editor.clone();
        let window = self.clone();
        editor.save_file_async_to_path(path.clone(), move |save_result| {
            window.complete_save_as(
                &editor_for_result,
                old_path.as_deref(),
                old_canonical_path.as_deref(),
                old_draft_id.as_deref(),
                &path,
                save_result,
            );
        });
    }

    /// Test helper for the file chooser's successful Open File result.
    #[cfg(feature = "test-utils")]
    pub fn select_open_file_for_test(&self, path: &Path) {
        self.handle_open_file_selection(&gio::File::for_path(path));
    }

    /// Test helper for the file chooser selecting an unsupported non-local file.
    #[cfg(feature = "test-utils")]
    pub fn select_open_file_uri_for_test(&self, uri: &str) {
        self.handle_open_file_selection(&gio::File::for_uri(uri));
    }

    /// Test helper for Open File cancellation. Kept explicit so chooser tests
    /// can prove that the cancel path is intentionally state-neutral.
    #[cfg(feature = "test-utils")]
    pub fn cancel_open_file_for_test(&self) {}

    /// Test helper for the file chooser's successful Save As result.
    #[cfg(feature = "test-utils")]
    pub fn select_save_as_destination_for_test(&self, path: &Path) {
        if let Some(editor) = self.active_editor() {
            self.handle_save_as_selection(&editor, &gio::File::for_path(path));
        }
    }

    /// Test helper for the Save As chooser selecting an unsupported non-local file.
    #[cfg(feature = "test-utils")]
    pub fn select_save_as_uri_for_test(&self, uri: &str) {
        if let Some(editor) = self.active_editor() {
            self.handle_save_as_selection(&editor, &gio::File::for_uri(uri));
        }
    }

    /// Test helper for Save As cancellation. Cancellation preserves the active
    /// editor's existing path, modified flag, and draft identity.
    #[cfg(feature = "test-utils")]
    pub fn cancel_save_as_destination_for_test(&self) {}

    /// Complete the Save As state transition after the background write
    /// resolves. State only switches to the new path on success.
    #[doc(hidden)]
    pub fn complete_save_as(
        &self,
        editor: &LushtextEditorPage,
        old_path: Option<&Path>,
        old_canonical_path: Option<&Path>,
        old_draft_id: Option<&str>,
        path: &Path,
        save_result: Result<(), crate::ui::editor_page::EditorSaveError>,
    ) {
        let path_display = path.display().to_string();
        match save_result {
            Ok(()) => {
                let canonical_path = editor.canonical_file_path();
                {
                    let mut open_paths = self.imp().open_paths.borrow_mut();
                    if let Some(old) = old_path {
                        open_paths.remove(old);
                        open_paths.remove(&super::documents::open_path_key(old));
                    }
                    if let Some(old_canonical) = old_canonical_path {
                        open_paths.remove(old_canonical);
                    }
                    open_paths.insert(super::documents::open_path_key(path));
                    if let Some(canonical_path) = canonical_path.clone() {
                        open_paths.insert(canonical_path);
                    }
                }
                editor.set_file_path_with_canonical(path, canonical_path);
                self.reconcile_open_paths_from_tabs();
                self.refresh_canonical_path_after_rename(editor, path);
                self.refresh_sidebar_file_row_states();
                self.refresh_open_popover_rows();
                self.assign_draft_id(editor);
                let new_draft_id = editor.draft_id();
                self.resolve_editorconfig_for_editor(editor, path);
                self.reset_notes_after_save_as(editor, path);
                if let Some(draft_id) = old_draft_id {
                    self.delete_draft_by_id(draft_id);
                } else if let Some(old) = old_path {
                    self.delete_draft_for_path(old);
                }
                if let Some(draft_id) = new_draft_id {
                    self.delete_draft_by_id(&draft_id);
                } else {
                    self.delete_draft_for_path(path);
                }
                editor.set_draft_restored(false);
                self.dismiss_editor_notifications(editor);

                let tab_view = &self.imp().tab_view;
                for i in 0..tab_view.n_pages() {
                    let page = tab_view.nth_page(i);
                    if let Some(candidate) = page.child().downcast_ref::<LushtextEditorPage>()
                        && candidate.as_ptr() == editor.as_ptr()
                    {
                        page.set_title(&editor.title());
                        break;
                    }
                }
                self.publish_status_message(&format!("Saved as {path_display}"), MessageKind::Info);
                self.refresh_command_palette_sources();
                self.refresh_status_bar();
            }
            Err(crate::ui::editor_page::EditorSaveError::LossyEncoding { preview, .. }) => {
                let window = self.clone();
                let editor = editor.clone();
                let editor_for_dialog = editor.clone();
                let retry_path = path.to_path_buf();
                let retry_old_path = old_path.map(std::path::Path::to_path_buf);
                let retry_old_canonical_path = old_canonical_path.map(std::path::Path::to_path_buf);
                let retry_old_draft_id = old_draft_id.map(ToOwned::to_owned);
                self.confirm_lossy_save(&editor_for_dialog, &preview, move || {
                    let editor_for_result = editor.clone();
                    let window_for_retry = window.clone();
                    let retry_old_path = retry_old_path.clone();
                    let retry_old_canonical_path = retry_old_canonical_path.clone();
                    let retry_old_draft_id = retry_old_draft_id.clone();
                    editor.save_file_async_to_path(retry_path.clone(), move |retry_result| {
                        window_for_retry.complete_save_as(
                            &editor_for_result,
                            retry_old_path.as_deref(),
                            retry_old_canonical_path.as_deref(),
                            retry_old_draft_id.as_deref(),
                            &retry_path,
                            retry_result,
                        );
                    });
                });
            }
            Err(crate::ui::editor_page::EditorSaveError::DurabilityUnconfirmed {
                path: written,
                source,
            }) => {
                // Bytes reached the destination but the directory fsync failed.
                // Stay conservative for Save As: do not adopt the new identity
                // while durability is unconfirmed, so the user re-saves to commit.
                tracing::warn!(
                    "Wrote {}, but durability sync failed: {source}",
                    written.display()
                );
                self.publish_status_message(
                    &format!(
                        "Wrote {path_display}, but durability is unconfirmed — save again to confirm"
                    ),
                    MessageKind::Warning,
                );
                self.refresh_status_bar();
            }
            Err(e) => {
                tracing::error!("Save As failed: {}", e);
                self.publish_status_message(&format!("Save failed: {e}"), MessageKind::Error);
            }
        }
    }

    // --- Discard changes confirmation ---

    /// Show the "Discard Changes?" confirmation dialog matching GNOME Text
    /// Editor's UX. Calls `on_done(true)` if the user confirms, `on_done(false)`
    /// if cancelled.
    pub fn show_discard_changes_dialog<F: Fn(bool) + 'static>(&self, title: &str, on_done: F) {
        let dialog = libadwaita::AlertDialog::builder()
            .heading(format!("Discard Changes to \u{201C}{title}\u{201D}?"))
            .body("Unsaved changes will be permanently lost.")
            .build();

        dialog.add_response(RESPONSE_CANCEL, "_Cancel");
        dialog.add_response(RESPONSE_DISCARD, "_Discard");

        dialog.set_response_appearance(
            RESPONSE_DISCARD,
            libadwaita::ResponseAppearance::Destructive,
        );
        dialog.set_default_response(Some(RESPONSE_CANCEL));
        dialog.set_close_response(RESPONSE_CANCEL);

        dialog.connect_response(None::<&str>, move |_, response| {
            on_done(response == RESPONSE_DISCARD);
        });

        accessibility::announce_with_lane(
            self,
            &format!("Discard changes to {title}? Unsaved changes will be permanently lost."),
            AnnouncementLane::Alert,
        );
        dialog.present(Some(self));
    }

    // --- Save changes confirmation ---

    /// Collect all modified editor pages in the tab view.
    #[must_use]
    pub fn modified_editors(&self) -> Vec<(libadwaita::TabPage, LushtextEditorPage)> {
        let tab_view = &self.imp().tab_view;
        let mut result = Vec::new();
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            let child = page.child();
            if let Some(editor) = child.downcast_ref::<LushtextEditorPage>()
                && editor.is_modified()
            {
                result.push((page, editor.clone()));
            }
        }
        result
    }

    /// Return whether any open editor is currently saving on a background thread.
    #[must_use]
    pub fn has_saving_editors(&self) -> bool {
        let tab_view = &self.imp().tab_view;
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            let child = page.child();
            if let Some(editor) = child.downcast_ref::<LushtextEditorPage>()
                && editor.is_saving()
            {
                return true;
            }
        }
        false
    }

    /// Publish the shared close-flow warning for tabs whose save is still running.
    pub(crate) fn publish_save_in_progress_warning(&self) {
        self.publish_status_message(
            "Save is still in progress. Wait for it to finish before closing.",
            MessageKind::Warning,
        );
    }

    /// Show the "Save Changes?" dialog for the given modified editors.
    /// Calls `on_done(confirmed)` after the user responds — `true` means
    /// proceed with close (save or discard completed), `false` means cancel.
    pub fn show_save_changes_dialog<F: Fn(bool) + 'static>(
        &self,
        modified: &[(libadwaita::TabPage, LushtextEditorPage)],
        on_done: F,
    ) {
        if modified.is_empty() {
            on_done(true);
            return;
        }

        let dialog = libadwaita::AlertDialog::builder()
            .heading("Save Changes?")
            .body(
                "Open documents contain unsaved changes. \
                 Changes which are not saved will be permanently lost.",
            )
            .build();

        dialog.add_response(RESPONSE_CANCEL, "_Cancel");
        let discard_label = if modified.len() > 1 {
            "_Discard All"
        } else {
            "_Discard"
        };
        dialog.add_response(RESPONSE_DISCARD, discard_label);
        dialog.add_response(RESPONSE_SAVE, "_Save");

        dialog.set_response_appearance(
            RESPONSE_DISCARD,
            libadwaita::ResponseAppearance::Destructive,
        );
        dialog.set_response_appearance(RESPONSE_SAVE, libadwaita::ResponseAppearance::Suggested);
        dialog.set_default_response(Some(RESPONSE_CANCEL));
        dialog.set_close_response(RESPONSE_CANCEL);

        // Per-file checklist: each row has a checkbox (default: checked) so the
        // user can select which files to save. Untitled tabs stay visible here
        // so the close flow can block and ask the user to Save As explicitly
        // instead of silently treating them as already saved.
        let group = libadwaita::PreferencesGroup::new();
        accessibility::set_role(&group, gtk4::AccessibleRole::Group);
        accessibility::set_labelled_description(
            &group,
            "Documents with unsaved changes",
            "Choose which modified documents to save",
        );
        let checks: Rc<RefCell<Vec<(gtk4::CheckButton, LushtextEditorPage)>>> =
            Rc::new(RefCell::new(Vec::new()));

        for (_page, editor) in modified {
            let title = editor.title();
            let path = editor.file_path();
            let subtitle = path
                .as_deref()
                .and_then(|p| p.parent().map(|d| d.display().to_string()))
                .unwrap_or_default();
            let row_title = if path.is_none() {
                format!("{title} (new)")
            } else {
                title
            };

            let row = libadwaita::ActionRow::builder()
                .title(&row_title)
                .subtitle(&subtitle)
                .build();

            let check = gtk4::CheckButton::builder()
                .active(true)
                .valign(gtk4::Align::Center)
                .build();
            let check_label = format!("Save {row_title}");
            accessibility::set_labelled_description(
                &check,
                &check_label,
                "Include this document when saving before close",
            );
            row.add_prefix(&check);
            let check_weak = check.downgrade();
            row.set_activatable(true);
            row.connect_activated(move |_| {
                if let Some(check) = check_weak.upgrade() {
                    check.set_active(!check.is_active());
                }
            });

            group.add(&row);
            checks.borrow_mut().push((check, editor.clone()));
        }
        dialog.set_extra_child(Some(&group));

        let window = self.clone();
        let on_done = Rc::new(on_done);

        dialog.connect_response(None::<&str>, move |_, response| match response {
            RESPONSE_SAVE => {
                let checks = checks.borrow();
                let selected: Vec<_> = checks
                    .iter()
                    .filter(|(check, _)| check.is_active())
                    .map(|(_, editor)| editor.clone())
                    .collect();
                let discarded: Vec<_> = checks
                    .iter()
                    .filter(|(check, _)| !check.is_active())
                    .map(|(_, editor)| editor.clone())
                    .collect();
                let on_done = on_done.clone();
                window.save_editors_for_close(selected, discarded, move |confirmed| {
                    on_done(confirmed);
                });
            }
            RESPONSE_DISCARD => {
                let checks = checks.borrow();
                let all: Vec<_> = checks.iter().map(|(_, e)| e.clone()).collect();
                window.stage_close_discard_drafts(&all);
                on_done(true);
            }
            _ => {
                on_done(false);
            }
        });

        accessibility::announce_with_lane(
            self,
            "Save changes? Open documents contain unsaved changes. Discarding changes is permanent.",
            AnnouncementLane::Alert,
        );
        dialog.present(Some(self));
    }

    /// Save the selected editors during a close flow. Drafts for unchecked
    /// editors are only removed after every requested save succeeds so a
    /// failed close attempt never drops recovery data prematurely.
    pub fn save_editors_for_close<F: Fn(bool) + 'static>(
        &self,
        editors: Vec<LushtextEditorPage>,
        discarded_editors: Vec<LushtextEditorPage>,
        on_done: F,
    ) {
        if editors.iter().any(|editor| editor.file_path().is_none()) {
            self.publish_status_message(
                "Untitled documents must be saved with Save As or discarded before closing",
                MessageKind::Warning,
            );
            on_done(false);
            return;
        }

        let selected_file_backed: Vec<_> = editors
            .into_iter()
            .filter(|editor| editor.file_path().is_some())
            .collect();
        if selected_file_backed.is_empty() {
            if !discarded_editors.is_empty() {
                self.stage_close_discard_drafts(&discarded_editors);
            }
            on_done(true);
            return;
        }

        let saved_editors: Rc<RefCell<Vec<LushtextEditorPage>>> = Rc::new(RefCell::new(Vec::new()));
        let discarded_fingerprints = Rc::new(
            discarded_editors
                .iter()
                .map(close_safety_editor_fingerprint_for)
                .collect(),
        );
        let discarded_editors = Rc::new(discarded_editors);
        let on_done: Rc<dyn Fn(bool)> = Rc::new(on_done);
        let was_sensitive = self.is_sensitive();
        // The user's checked/unchecked choices are consent for the exact editor
        // generations visible at confirmation, not for edits made during a slow save.
        self.set_sensitive(false);
        let close_session_identity = self.begin_close_save_session();
        self.drive_close_save_pipeline(CloseSavePipeline {
            identity: close_session_identity,
            remaining: VecDeque::from(selected_file_backed),
            saved_editors,
            discarded_editors,
            discarded_fingerprints,
            was_sensitive,
            on_done,
        });
    }

    fn begin_close_save_session(&self) -> u64 {
        let state = &self.imp().session;
        let identity = state.next_close_save_identity.get().wrapping_add(1);
        state.next_close_save_identity.set(identity);
        state.active_close_save_identity.set(Some(identity));
        identity
    }

    pub(crate) fn close_save_session_is_current(&self, identity: u64) -> bool {
        self.imp().session.active_close_save_identity.get() == Some(identity)
    }

    fn finish_close_save_session(&self, identity: u64) {
        if self.close_save_session_is_current(identity) {
            self.imp().session.active_close_save_identity.set(None);
        }
    }

    fn drive_close_save_pipeline(&self, mut pipeline: CloseSavePipeline) {
        if !self.close_save_session_is_current(pipeline.identity) {
            (pipeline.on_done)(false);
            return;
        }

        let Some(editor) = pipeline.remaining.pop_front() else {
            self.finish_close_save_session(pipeline.identity);
            if !close_discard_fingerprints_are_current(self, &pipeline.discarded_fingerprints) {
                self.set_sensitive(pipeline.was_sensitive);
                self.publish_status_message(
                    "Documents changed while saving; review them and close again",
                    MessageKind::Warning,
                );
                for editor in pipeline
                    .discarded_editors
                    .iter()
                    .filter(|editor| editor.is_modified())
                {
                    editor.set_draft_dirty(true);
                }
                self.schedule_first_dirty_draft_autosave();
                (pipeline.on_done)(false);
                return;
            }
            let saved = pipeline.saved_editors.borrow().clone();
            if !saved.is_empty() {
                self.cleanup_drafts_for_editors(&saved);
            }
            if !pipeline.discarded_editors.is_empty() {
                self.stage_close_discard_drafts(pipeline.discarded_editors.as_ref());
            }
            // `on_done(true)` synchronously enters close safety and freezes the
            // window again; restore the pre-dialog state for a correct abort path.
            self.set_sensitive(pipeline.was_sensitive);
            (pipeline.on_done)(true);
            return;
        };

        let window = self.clone();
        let saved_editor = editor.clone();
        editor.save_file_async_for_close(pipeline.identity, move |result| match result {
            Ok(()) => {
                pipeline.saved_editors.borrow_mut().push(saved_editor);
                window.drive_close_save_pipeline(pipeline);
            }
            Err(error) => {
                window.finish_close_save_session(pipeline.identity);
                // A durability-unconfirmed result wrote bytes but could not
                // prove them crash-safe. Keep every tab and draft recoverable.
                if matches!(
                    error,
                    crate::ui::editor_page::EditorSaveError::DurabilityUnconfirmed { .. }
                ) {
                    tracing::warn!("Save during close not yet durable: {error}");
                    window.publish_status_message(
                        "Saved during close, but durability is unconfirmed — save again",
                        MessageKind::Warning,
                    );
                } else {
                    tracing::error!("Save failed during close: {error}");
                    window.publish_status_message(
                        &format!("Save failed during close: {error}"),
                        MessageKind::Error,
                    );
                }
                window.set_sensitive(pipeline.was_sensitive);
                (pipeline.on_done)(false);
            }
        });
    }

    fn stage_close_discard_drafts(&self, editors: &[LushtextEditorPage]) {
        let mut discarded_ids = self.imp().drafts.close_discard_ids.borrow_mut();
        discarded_ids.extend(editors.iter().filter_map(LushtextEditorPage::draft_id));
        drop(discarded_ids);
        self.cleanup_drafts_for_editors(editors);
    }

    pub(crate) fn clear_close_discard_drafts(&self) {
        self.imp().drafts.close_discard_ids.borrow_mut().clear();
    }

    /// Delete drafts for the given editors. Handles both path-backed files
    /// (by path lookup) and untitled tabs (by draft_id).
    fn cleanup_drafts_for_editors(&self, editors: &[LushtextEditorPage]) {
        for editor in editors {
            if let Some(draft_id) = editor.draft_id() {
                self.delete_draft_by_id(&draft_id);
            } else if let Some(ref path) = editor.file_path() {
                self.delete_draft_for_path(path);
            }
        }
    }

    /// Show a save-changes dialog for a single tab being closed.
    /// `confirm_close` is called with `true` to proceed or `false` to cancel.
    pub fn confirm_close_tab<F: Fn(bool) + 'static>(
        &self,
        page: &libadwaita::TabPage,
        editor: &LushtextEditorPage,
        confirm_close: F,
    ) {
        if editor.is_saving() {
            self.publish_save_in_progress_warning();
            confirm_close(false);
            return;
        }
        if !editor.is_modified() {
            confirm_close(true);
            return;
        }
        self.show_save_changes_dialog(&[(page.clone(), editor.clone())], confirm_close);
    }
}

impl super::LushtextWindow {
    /// Freeze a confirmed window-close transaction across asynchronous safety work.
    pub(super) fn begin_async_close_safety(&self) {
        // Keep the close transaction single-flight: duplicate close requests report
        // progress while the background draft flush and ordered session save finish.
        if self.imp().session.close_safety_inflight.get() {
            self.publish_status_message("Finishing close safety checks…", MessageKind::Info);
            return;
        }
        let fingerprint = close_safety_editor_fingerprint(self);
        let was_sensitive = self.is_sensitive();
        self.imp().session.close_safety_inflight.set(true);
        self.set_sensitive(false);
        self.imp().search_panel.close();
        let window_for_draft = self.clone();
        self.flush_dirty_drafts_async(move |draft_result| match draft_result {
            Ok(()) => {
                let sidebar = window_for_draft.imp().sidebar.clone();
                let window_for_workspace = window_for_draft;
                sidebar.flush_workspace_persistence(move |workspace_result| {
                    if let Err(error) = workspace_result {
                        abort_async_close_safety(&window_for_workspace, was_sensitive);
                        window_for_workspace.publish_status_message(
                            &format!(
                                "Close cancelled because workspace changes could not be saved: {error}"
                            ),
                            MessageKind::Error,
                        );
                        return;
                    }

                    let window_for_session = window_for_workspace.clone();
                    let window_for_destroy = window_for_workspace;
                    window_for_session.save_session_for_close_async(move |result| {
                        if let Err(error) = result {
                            abort_async_close_safety(&window_for_destroy, was_sensitive);
                            window_for_destroy.publish_status_message(
                                &format!("Close cancelled because session recovery state could not be saved: {error}"),
                                MessageKind::Error,
                            );
                            return;
                        }
                        #[cfg(feature = "test-utils")]
                        {
                            let delay_ms =
                                CLOSE_SAFETY_COMPLETION_DELAY_MS.load(Ordering::Acquire);
                            if delay_ms > 0 {
                                glib::timeout_add_local_once(
                                    std::time::Duration::from_millis(delay_ms),
                                    move || {
                                        finish_async_close_safety(
                                            &window_for_destroy,
                                            &fingerprint,
                                            was_sensitive,
                                        );
                                    },
                                );
                                return;
                            }
                        }
                        finish_async_close_safety(
                            &window_for_destroy,
                            &fingerprint,
                            was_sensitive,
                        );
                    });
                });
            }
            Err(error) => {
                abort_async_close_safety(&window_for_draft, was_sensitive);
                window_for_draft.publish_status_message(
                    &format!("Draft save failed: {error}"),
                    MessageKind::Error,
                );
            }
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CloseSafetyEditorFingerprint {
    owner_id: usize,
    draft_generation: u64,
    modified: bool,
    path: Option<PathBuf>,
}

struct CloseSavePipeline {
    identity: u64,
    remaining: VecDeque<LushtextEditorPage>,
    saved_editors: Rc<RefCell<Vec<LushtextEditorPage>>>,
    discarded_editors: Rc<Vec<LushtextEditorPage>>,
    discarded_fingerprints: Rc<Vec<CloseSafetyEditorFingerprint>>,
    was_sensitive: bool,
    on_done: Rc<dyn Fn(bool)>,
}

fn close_safety_editor_fingerprint(
    window: &super::LushtextWindow,
) -> Vec<CloseSafetyEditorFingerprint> {
    let tab_view = &window.imp().tab_view;
    let mut fingerprint = Vec::with_capacity(usize::try_from(tab_view.n_pages()).unwrap_or(0));
    for index in 0..tab_view.n_pages() {
        let page = tab_view.nth_page(index);
        if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
            fingerprint.push(close_safety_editor_fingerprint_for(editor));
        }
    }
    fingerprint
}

fn close_safety_editor_fingerprint_for(
    editor: &LushtextEditorPage,
) -> CloseSafetyEditorFingerprint {
    CloseSafetyEditorFingerprint {
        owner_id: editor.notification_owner_id(),
        draft_generation: editor.draft_dirty_generation(),
        modified: editor.is_modified(),
        path: editor.file_path(),
    }
}

fn close_discard_fingerprints_are_current(
    window: &super::LushtextWindow,
    expected: &[CloseSafetyEditorFingerprint],
) -> bool {
    let current: HashMap<_, _> = close_safety_editor_fingerprint(window)
        .into_iter()
        .map(|fingerprint| (fingerprint.owner_id, fingerprint))
        .collect();
    expected
        .iter()
        .all(|fingerprint| current.get(&fingerprint.owner_id) == Some(fingerprint))
}

fn finish_async_close_safety(
    window: &super::LushtextWindow,
    expected: &[CloseSafetyEditorFingerprint],
    was_sensitive: bool,
) {
    if window.has_saving_editors() || close_safety_editor_fingerprint(window) != expected {
        abort_async_close_safety(window, was_sensitive);
        window.publish_status_message(
            "Documents changed while closing; review them and close again",
            MessageKind::Warning,
        );
        return;
    }
    window.imp().session.close_safety_inflight.set(false);
    window.imp().session.close_safety_bypass.set(true);
    window.destroy();
}

fn abort_async_close_safety(window: &super::LushtextWindow, was_sensitive: bool) {
    window.imp().session.close_safety_inflight.set(false);
    window.imp().session.close_safety_bypass.set(false);
    window.set_sensitive(was_sensitive);
    window.clear_close_discard_drafts();
    let modified = window.modified_editors();
    if !modified.is_empty() {
        for (_, editor) in &modified {
            editor.set_draft_dirty(true);
        }
        window.schedule_first_dirty_draft_autosave();
    }
}
