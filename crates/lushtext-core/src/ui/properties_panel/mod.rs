// SPDX-License-Identifier: GPL-3.0-or-later

//! Right-side properties panel for the main window.
//!
//! The panel is a UI-only companion to the window shell: it shows metadata for
//! the currently selected editor and reuses the existing GSettings-backed
//! formatting controls so users can adjust editor behavior without opening the
//! full preferences dialog.

mod imp;

use crate::config::keys;
use crate::ui::editor_page::LushtextEditorPage;
use gio::prelude::SettingsExt;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use libadwaita::prelude::ActionRowExt;

glib::wrapper! {
    pub struct LushtextPropertiesPanel(ObjectSubclass<imp::LushtextPropertiesPanel>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextPropertiesPanel {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Refresh the document-specific rows from the currently selected editor.
    ///
    /// The formatting controls stay active even when no editor is selected
    /// because they are global preferences, but file-backed metadata needs a
    /// graceful empty state for untitled tabs and an empty window.
    pub fn set_active_editor(&self, editor: Option<&LushtextEditorPage>) {
        let settings = &self.imp().settings;

        if let Some(editor) = editor {
            if let Some(path) = editor.file_path() {
                self.imp()
                    .path_row
                    .set_subtitle(&path.display().to_string());
                let summary = editor.document_encoding_state().summary();
                self.imp().encoding_row.set_subtitle(&summary);
            } else {
                self.imp().path_row.set_subtitle("Untitled document");
                let summary = editor.document_encoding_state().summary();
                self.imp().encoding_row.set_subtitle(&summary);
            }
            self.imp()
                .file_size_row
                .set_subtitle(&format_file_size(editor.file_size()));

            let formatting_source = if editor.file_path().is_none() {
                "Not available for untitled tabs"
            } else if !settings.boolean(keys::USE_EDITORCONFIG) {
                "Preferences defaults"
            } else if editor.formatting_overrides().is_empty() {
                "Preferences defaults (no EditorConfig override)"
            } else {
                "EditorConfig override active"
            };
            self.imp()
                .formatting_source_row
                .set_subtitle(formatting_source);
        } else {
            self.imp().path_row.set_subtitle("No document selected");
            self.imp().encoding_row.set_subtitle("Not available");
            self.imp().file_size_row.set_subtitle("Not available");
            self.imp()
                .formatting_source_row
                .set_subtitle("Not available");
        }
    }
}

impl Default for LushtextPropertiesPanel {
    fn default() -> Self {
        Self::new()
    }
}

fn format_file_size(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "Not available".to_string();
    };

    const KB: f64 = 1_000.0;
    const MB: f64 = 1_000_000.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        let kb = b / KB;
        if kb >= 999.95 {
            format!("{:.1} MB", b / MB)
        } else {
            format!("{kb:.1} KB")
        }
    } else {
        format!("{bytes} B")
    }
}
