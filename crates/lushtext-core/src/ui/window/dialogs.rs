// SPDX-License-Identifier: GPL-3.0-or-later

//! File dialogs for the main window: open file, open folder, save as,
//! and save-changes confirmation on close.

use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_DISCARD: &str = "discard";
const RESPONSE_SAVE: &str = "save";

impl super::LushtextWindow {
    pub fn show_open_file_dialog(&self) {
        let dialog = gtk4::FileDialog::builder()
            .title("Open File")
            .modal(true)
            .build();

        let window = self.clone();
        dialog.open(Some(self), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result
                && let Some(path) = file.path()
            {
                window.handle_open_file_selection(&path);
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
            if let Ok(file) = result
                && let Some(path) = file.path()
            {
                window.handle_save_as_selection(&editor, path);
            }
        });
    }

    /// Complete the Open File chooser after GTK or a portal has produced a
    /// selected path. Cancellation intentionally does not call this helper, so
    /// no document state changes until a concrete file is selected.
    fn handle_open_file_selection(&self, path: &Path) {
        self.open_document(path);
    }

    /// Complete the Save As chooser after the user selects a destination.
    /// The editor adopts the new identity only inside `complete_save_as`, after
    /// the background durable write reports success.
    fn handle_save_as_selection(&self, editor: &LushtextEditorPage, path: PathBuf) {
        let old_path = editor.file_path();
        let old_draft_id = editor.draft_id();
        let editor = editor.clone();
        let editor_for_result = editor.clone();
        let window = self.clone();
        editor.save_file_async_to_path(path.clone(), move |save_result| {
            window.complete_save_as(
                &editor_for_result,
                old_path.as_deref(),
                old_draft_id.as_deref(),
                &path,
                save_result,
            );
        });
    }

    /// Test helper for the file chooser's successful Open File result.
    #[cfg(feature = "test-utils")]
    pub fn select_open_file_for_test(&self, path: &Path) {
        self.handle_open_file_selection(path);
    }

    /// Test helper for Open File cancellation. Kept explicit so chooser tests
    /// can prove that the cancel path is intentionally state-neutral.
    #[cfg(feature = "test-utils")]
    pub fn cancel_open_file_for_test(&self) {}

    /// Test helper for the file chooser's successful Save As result.
    #[cfg(feature = "test-utils")]
    pub fn select_save_as_destination_for_test(&self, path: &Path) {
        if let Some(editor) = self.active_editor() {
            self.handle_save_as_selection(&editor, path.to_path_buf());
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
        old_draft_id: Option<&str>,
        path: &Path,
        save_result: Result<(), crate::ui::editor_page::SaveError>,
    ) {
        let path_display = path.display().to_string();
        match save_result {
            Ok(()) => {
                let canonical_path = path.canonicalize().ok();
                {
                    let mut open_paths = self.imp().open_paths.borrow_mut();
                    if let Some(old) = old_path {
                        open_paths.remove(old);
                        open_paths.remove(&super::documents::open_path_key(old));
                        if let Ok(old_canonical) = old.canonicalize() {
                            open_paths.remove(&old_canonical);
                        }
                    }
                    open_paths.insert(super::documents::open_path_key(path));
                    if let Some(canonical_path) = canonical_path.clone() {
                        open_paths.insert(canonical_path);
                    }
                }
                editor.set_file_path_with_canonical(path, canonical_path);
                self.assign_draft_id(editor);
                self.resolve_editorconfig_for_editor(editor, path);
                self.reset_notes_after_save_as(editor, path);
                if let Some(old) = old_path {
                    self.delete_draft_for_path(old);
                } else if let Some(draft_id) = old_draft_id {
                    self.delete_draft_by_id(draft_id);
                }
                self.delete_draft_for_path(path);
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
            Err(crate::ui::editor_page::SaveError::LossyEncoding { preview, .. }) => {
                let window = self.clone();
                let editor = editor.clone();
                let editor_for_dialog = editor.clone();
                let retry_path = path.to_path_buf();
                let retry_old_path = old_path.map(std::path::Path::to_path_buf);
                let retry_old_draft_id = old_draft_id.map(ToOwned::to_owned);
                self.confirm_lossy_save(&editor_for_dialog, &preview, move || {
                    let editor_for_result = editor.clone();
                    let window_for_retry = window.clone();
                    let retry_old_path = retry_old_path.clone();
                    let retry_old_draft_id = retry_old_draft_id.clone();
                    editor.save_file_async_to_path(retry_path.clone(), move |retry_result| {
                        window_for_retry.complete_save_as(
                            &editor_for_result,
                            retry_old_path.as_deref(),
                            retry_old_draft_id.as_deref(),
                            &retry_path,
                            retry_result,
                        );
                    });
                });
            }
            Err(crate::ui::editor_page::SaveError::DurabilityUnconfirmed {
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
        group.set_accessible_role(gtk4::AccessibleRole::Group);
        group.update_property(&[
            gtk4::accessible::Property::Label("Documents with unsaved changes"),
            gtk4::accessible::Property::Description("Choose which modified documents to save"),
        ]);
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
            check.update_property(&[
                gtk4::accessible::Property::Label(&check_label),
                gtk4::accessible::Property::Description(
                    "Include this document when saving before close",
                ),
            ]);
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

        #[expect(
            clippy::cast_possible_truncation,
            reason = "The save-changes dialog only reflects the current tab set, which cannot approach u32::MAX entries"
        )]
        let pending = Rc::new(std::cell::Cell::new(selected_file_backed.len() as u32));
        let any_failed = Rc::new(std::cell::Cell::new(false));
        let saved_editors: Rc<RefCell<Vec<LushtextEditorPage>>> = Rc::new(RefCell::new(Vec::new()));
        let discarded_editors = Rc::new(discarded_editors);
        let on_done = Rc::new(on_done);
        let window = self.clone();

        for editor in selected_file_backed {
            let editor_for_callback = editor.clone();
            let pending = pending.clone();
            let on_done = on_done.clone();
            let window = window.clone();
            let any_failed = any_failed.clone();
            let saved_editors = saved_editors.clone();
            let discarded_editors = discarded_editors.clone();
            editor.save_file_async(move |result| {
                if let Err(e) = result {
                    // Abort the close on any failure. A durability-unconfirmed
                    // result wrote the bytes but could not prove them crash-safe,
                    // so warn rather than claim the save was lost — and still keep
                    // the tab open so the user can re-save to confirm.
                    any_failed.set(true);
                    if let crate::ui::editor_page::SaveError::DurabilityUnconfirmed { .. } = e {
                        tracing::warn!("Save during close not yet durable: {e}");
                        window.publish_status_message(
                            "Saved during close, but durability is unconfirmed — save again",
                            MessageKind::Warning,
                        );
                    } else {
                        tracing::error!("Save failed during close: {e}");
                        window.publish_status_message(
                            &format!("Save failed during close: {e}"),
                            MessageKind::Error,
                        );
                    }
                } else {
                    saved_editors.borrow_mut().push(editor_for_callback.clone());
                }
                let remaining = pending.get().saturating_sub(1);
                pending.set(remaining);
                if remaining == 0 {
                    if any_failed.get() {
                        on_done(false);
                        return;
                    }
                    let saved = saved_editors.borrow().clone();
                    if !saved.is_empty() {
                        window.cleanup_drafts_for_editors(&saved);
                    }
                    if !discarded_editors.is_empty() {
                        window.stage_close_discard_drafts(discarded_editors.as_ref());
                    }
                    on_done(true);
                }
            });
        }
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
            if let Some(ref path) = editor.file_path() {
                self.delete_draft_for_path(path);
            } else if let Some(draft_id) = editor.draft_id() {
                self.delete_draft_by_id(&draft_id);
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
