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
                    editor.set_file_path(&path);
                    match editor.save_file() {
                        Ok(()) => {
                            // Update tab title to reflect new filename
                            if let Some(page) = window.imp().tab_view.selected_page() {
                                page.set_title(&editor.title());
                            }
                            window.imp().status_bar.push_message(
                                &format!("Saved as {}", path.display()),
                                MessageKind::Info,
                            );
                            window.refresh_status_bar();
                        }
                        Err(e) => {
                            tracing::error!("Save As failed: {}", e);
                            window
                                .imp()
                                .status_bar
                                .push_message(&format!("Save failed: {}", e), MessageKind::Error);
                        }
                    }
                }
            }
        });
    }
}
