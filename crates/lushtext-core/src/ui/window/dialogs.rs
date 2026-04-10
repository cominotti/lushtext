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
use std::path::PathBuf;
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
                window.open_document(&path);
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
                let old_path = editor.file_path();
                let old_draft_id = editor.draft_id();
                let editor = editor.clone();
                let editor_for_result = editor.clone();
                let window_clone = window.clone();
                editor.save_file_async_to_path(path.clone(), move |save_result| {
                    window_clone.complete_save_as(
                        &editor_for_result,
                        old_path.clone(),
                        old_draft_id.clone(),
                        path.clone(),
                        save_result,
                    );
                });
            }
        });
    }

    /// Complete the Save As state transition after the background write
    /// resolves. State only switches to the new path on success.
    #[doc(hidden)]
    pub fn complete_save_as(
        &self,
        editor: &LushtextEditorPage,
        old_path: Option<PathBuf>,
        old_draft_id: Option<String>,
        path: PathBuf,
        save_result: Result<(), crate::ui::editor_page::SaveError>,
    ) {
        let path_display = path.display().to_string();
        match save_result {
            Ok(()) => {
                {
                    let mut open_paths = self.imp().open_paths.borrow_mut();
                    if let Some(ref old) = old_path {
                        open_paths.remove(old.as_path());
                    }
                    open_paths.insert(path.clone());
                }
                editor.set_file_path(&path);
                self.assign_draft_id(editor);
                self.resolve_editorconfig_for_editor(editor, &path);
                if let Some(ref old) = old_path {
                    self.delete_draft_for_path(old);
                } else if let Some(ref draft_id) = old_draft_id {
                    self.delete_draft_by_id(draft_id);
                }
                self.delete_draft_for_path(&path);
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

    /// Show the "Save Changes?" dialog for the given modified editors.
    /// Calls `on_done(confirmed)` after the user responds — `true` means
    /// proceed with close (save or discard completed), `false` means cancel.
    pub fn show_save_changes_dialog<F: Fn(bool) + 'static>(
        &self,
        modified: Vec<(libadwaita::TabPage, LushtextEditorPage)>,
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
        let checks: Rc<RefCell<Vec<(gtk4::CheckButton, LushtextEditorPage)>>> =
            Rc::new(RefCell::new(Vec::new()));

        for (_page, editor) in &modified {
            let title = editor.title();
            let path = editor.file_path();
            let subtitle = path
                .as_deref()
                .and_then(|p| p.parent().map(|d| d.display().to_string()))
                .unwrap_or_default();

            let row = libadwaita::ActionRow::builder()
                .title(if path.is_none() {
                    format!("{title} (new)")
                } else {
                    title
                })
                .subtitle(&subtitle)
                .build();

            let check = gtk4::CheckButton::builder()
                .active(true)
                .valign(gtk4::Align::Center)
                .build();
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
                    any_failed.set(true);
                    tracing::error!("Save failed during close: {e}");
                    window.publish_status_message(
                        &format!("Save failed during close: {e}"),
                        MessageKind::Error,
                    );
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
        let mut discarded_ids = self.imp().close_discard_draft_ids.borrow_mut();
        discarded_ids.extend(editors.iter().filter_map(LushtextEditorPage::draft_id));
        drop(discarded_ids);
        self.cleanup_drafts_for_editors(editors);
    }

    pub(crate) fn clear_close_discard_drafts(&self) {
        self.imp().close_discard_draft_ids.borrow_mut().clear();
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
        if !editor.is_modified() {
            confirm_close(true);
            return;
        }
        self.show_save_changes_dialog(vec![(page.clone(), editor.clone())], confirm_close);
    }
}
