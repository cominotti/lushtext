// SPDX-License-Identifier: GPL-3.0-or-later

//! File dialogs for the main window: open file, open folder, save as.

use crate::ui::status_bar::MessageKind;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;

impl super::LushtextWindow {
    pub(super) fn show_open_file_dialog(&self) {
        let dialog = gtk4::FileDialog::builder()
            .title("Open File")
            .modal(true)
            .build();

        let window = self.clone();
        dialog.open(Some(self), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    window.open_document(&path);
                }
            }
        });
    }

    pub(super) fn show_save_as_dialog(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };

        let dialog = gtk4::FileDialog::builder()
            .title("Save As")
            .modal(true)
            .build();

        // Pre-populate with current filename if available
        if let Some(path) = editor.file_path() {
            if let Some(name) = path.file_name() {
                dialog.set_initial_name(Some(&name.to_string_lossy()));
            }
        }

        let window = self.clone();
        dialog.save(Some(self), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    {
                        let mut open_paths = window.imp().open_paths.borrow_mut();
                        if let Some(old) = editor.file_path() {
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
                                        .downcast_ref::<crate::ui::editor_page::LushtextEditorPage>(
                                        )
                                        .map(|e| e.title())
                                        .unwrap_or_default(),
                                );
                            }
                            window_clone.imp().status_bar.push_message(
                                &format!("Saved as {path_display}"),
                                MessageKind::Info,
                            );
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
            }
        });
    }
}
