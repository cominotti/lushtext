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
                {
                    let mut open_paths = window.imp().open_paths.borrow_mut();
                    if let Some(ref old) = editor.file_path() {
                        open_paths.remove(old.as_path());
                    }
                    open_paths.insert(path.clone());
                }
                editor.set_file_path(&path);
                let path_display = path.display().to_string();
                let window_clone = window.clone();
                editor.save_file_async(move |save_result| match save_result {
                    Ok(()) => {
                        if let Some(page) = window_clone.imp().tab_view.selected_page() {
                            page.set_title(
                                &page
                                    .child()
                                    .downcast_ref::<crate::ui::editor_page::LushtextEditorPage>()
                                    .map(|e| e.title())
                                    .unwrap_or_default(),
                            );
                        }
                        window_clone
                            .imp()
                            .status_bar
                            .push_message(&format!("Saved as {path_display}"), MessageKind::Info);
                        window_clone.refresh_status_bar();
                    }
                    Err(e) => {
                        tracing::error!("Save As failed: {}", e);
                        window_clone
                            .imp()
                            .status_bar
                            .push_message(&format!("Save failed: {e}"), MessageKind::Error);
                    }
                });
            }
        });
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
        // user can select which files to save. Unchecked files are not saved on
        // "Save" but their drafts are still deleted (same as discard).
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
                let mut pending_saves = 0u32;
                let pending = Rc::new(std::cell::Cell::new(0u32));

                for (check, editor) in checks.iter() {
                    if check.is_active() && editor.file_path().is_some() {
                        pending_saves += 1;
                    }
                }

                if pending_saves == 0 {
                    let all: Vec<_> = checks.iter().map(|(_, e)| e.clone()).collect();
                    window.cleanup_drafts_for_editors(&all);
                    on_done(true);
                    return;
                }

                pending.set(pending_saves);
                let on_done = on_done.clone();
                let window_c = window.clone();
                let all_editors: Rc<Vec<LushtextEditorPage>> =
                    Rc::new(checks.iter().map(|(_, e)| e.clone()).collect());

                for (check, editor) in checks.iter() {
                    if !check.is_active() || editor.file_path().is_none() {
                        continue;
                    }
                    let pending = pending.clone();
                    let on_done = on_done.clone();
                    let window_c = window_c.clone();
                    let all_editors = all_editors.clone();
                    editor.save_file_async(move |result| {
                        if let Err(e) = result {
                            tracing::error!("Save failed during close: {e}");
                        }
                        let remaining = pending.get().saturating_sub(1);
                        pending.set(remaining);
                        if remaining == 0 {
                            window_c.cleanup_drafts_for_editors(&all_editors);
                            on_done(true);
                        }
                    });
                }
            }
            RESPONSE_DISCARD => {
                let checks = checks.borrow();
                let all: Vec<_> = checks.iter().map(|(_, e)| e.clone()).collect();
                window.cleanup_drafts_for_editors(&all);
                on_done(true);
            }
            _ => {
                on_done(false);
            }
        });

        dialog.present(Some(self));
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
