// SPDX-License-Identifier: GPL-3.0-or-later

//! Right-side properties panel for the main window.
//!
//! The panel is a UI-only companion to the window shell: it keeps slower
//! document inspection details, formatting-source explanation, and file-health
//! findings out of the bottom bar while still staying tied to the active editor.

// Private GObject implementation for the template-backed properties panel.
mod imp;

use crate::config::keys;
use crate::model::encoding::FileHealthFinding;
use crate::ui::accessibility;
use crate::ui::editor_page::LushtextEditorPage;
use gio::prelude::SettingsExt;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::{ListBoxRowExt, TextBufferExt, WidgetExt};
use libadwaita::prelude::ActionRowExt;
use libadwaita::prelude::PreferencesGroupExt;

glib::wrapper! {
    // Exposes the private ObjectSubclass as the public GTK widget used by the
    // adaptive window shell.
    /// Public document-properties panel shown as a side pane or bottom sheet.
    ///
    /// The wrapper exposes refresh methods for the active editor while the
    /// private implementation owns template rows and formatting controls.
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
    /// Untitled tabs and empty windows keep explicit copy so stale file-backed
    /// metadata never lingers after the active editor changes.
    pub fn set_active_editor(&self, editor: Option<&LushtextEditorPage>) {
        let settings = &self.imp().settings;

        if let Some(editor) = editor {
            if let Some(path) = editor.file_path() {
                self.imp()
                    .location_row
                    .set_subtitle(&path.display().to_string());
            } else {
                self.imp().location_row.set_subtitle("Untitled document");
            }
            self.imp()
                .file_size_row
                .set_subtitle(&format_file_size(editor.file_size()));
            self.imp()
                .statistics_row
                .set_subtitle(&format_document_statistics(editor));

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
            if editor.file_path().is_some() {
                self.set_health_details(
                    "No file-health issues recorded for this document.",
                    &editor.file_health(),
                );
            } else {
                self.set_health_details(
                    "Untitled documents do not have file-backed health details yet.",
                    &[],
                );
            }
        } else {
            self.imp().location_row.set_subtitle("No document selected");
            self.imp().file_size_row.set_subtitle("Not available");
            self.imp()
                .statistics_row
                .set_subtitle("Open a document to inspect statistics");
            self.imp()
                .formatting_source_row
                .set_subtitle("Not available");
            self.set_health_details("Open a document to inspect file health.", &[]);
        }
        self.refresh_accessibility_state();
    }

    /// Replace the dynamic file-health rows for the active document.
    fn set_health_details(&self, summary: &str, findings: &[FileHealthFinding]) {
        let imp = self.imp();
        imp.health_summary_row.set_subtitle(summary);
        imp.health_review_button.set_visible(!findings.is_empty());
        accessibility::set_hidden(&*imp.health_review_button, findings.is_empty());
        accessibility::set_disabled(&*imp.health_review_button, findings.is_empty());

        for row in imp.health_detail_rows.borrow_mut().drain(..) {
            imp.health_group.remove(&row);
        }

        let mut detail_rows = imp.health_detail_rows.borrow_mut();
        for finding in findings {
            let row = libadwaita::ActionRow::builder()
                .title(&finding.title)
                .subtitle(&finding.body)
                .build();
            row.set_activatable(false);
            row.set_subtitle_lines(0);
            accessibility::set_role(&row, gtk4::AccessibleRole::Group);
            accessibility::set_labelled_description(&row, &finding.title, &finding.body);
            imp.health_group.add(&row);
            detail_rows.push(row);
        }
    }

    /// Project the latest row subtitles into accessible value text.
    fn refresh_accessibility_state(&self) {
        let imp = self.imp();
        for row in [
            &*imp.location_row,
            &*imp.file_size_row,
            &*imp.statistics_row,
            &*imp.formatting_source_row,
            &*imp.health_summary_row,
        ] {
            if let Some(subtitle) = row.subtitle() {
                accessibility::set_value_text(row, subtitle.as_str());
            }
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

fn format_document_statistics(editor: &LushtextEditorPage) -> String {
    let buffer = editor.buffer();
    let line_count = buffer.line_count().max(1);
    let char_count = buffer.char_count();
    format!(
        "{} {}, {} {}",
        line_count,
        if line_count == 1 { "line" } else { "lines" },
        char_count,
        if char_count == 1 {
            "character"
        } else {
            "characters"
        }
    )
}
