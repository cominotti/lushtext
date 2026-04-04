// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor info bar widget — contextual warning/error bars above the editor.
//!
//! Matches GNOME Text Editor's info bar design: `GtkInfoBar` with `message-type`
//! set to `warning` (yellow/amber) or `error` (red) for Adwaita theme styling.
//! Three scenarios: file access errors, draft changes restored, and external
//! file modification. Each bar starts hidden and is revealed via `show_*()`.

mod imp;

use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

glib::wrapper! {
    pub struct LushtextInfoBar(ObjectSubclass<imp::LushtextInfoBar>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

// GtkInfoBar is deprecated since GTK 4.10 but has no multi-button replacement.
// GNOME Text Editor still uses GtkInfoBar in its latest code for the same reason.
#[allow(deprecated)]
impl LushtextInfoBar {
    pub fn new() -> Self {
        Object::builder().build()
    }

    // --- Show methods ---

    /// Show the error bar for a file that could not be opened.
    /// Displays the error message and a "Retry" button.
    pub fn show_load_error(&self, message: &str) {
        let imp = self.imp();
        imp.access_subtitle.set_label(message);
        imp.access_infobar.set_revealed(true);
    }

    /// Show the warning bar indicating draft changes were restored.
    /// If `has_backing_file` is true, shows "Draft Changes Restored" with
    /// Save and Discard buttons. If false, shows "Document Restored" with
    /// Save As and Discard buttons.
    pub fn show_draft_restored(&self, has_backing_file: bool) {
        let imp = self.imp();
        if has_backing_file {
            imp.discard_title.set_label("Draft Changes Restored");
            imp.discard_subtitle
                .set_label("Unsaved changes from a previous session have been restored.");
            imp.save_button.set_label("_Save\u{2026}");
        } else {
            imp.discard_title.set_label("Document Restored");
            imp.discard_subtitle
                .set_label("Unsaved document has been restored.");
            imp.save_button.set_label("Save _As\u{2026}");
        }
        imp.save_button.set_visible(true);
        imp.discard_infobar.set_revealed(true);
    }

    /// Show the warning bar indicating the file was modified externally.
    /// Displays "File Has Changed on Disk" with a "Discard Changes and Reload"
    /// button (reusing the discard button).
    pub fn show_externally_changed(&self) {
        let imp = self.imp();
        imp.discard_title.set_label("File Has Changed on Disk");
        imp.discard_subtitle
            .set_label("The file was modified by another program.");
        imp.discard_button.set_label("_Discard Changes and Reload");
        imp.save_button.set_visible(false);
        imp.discard_infobar.set_revealed(true);
    }

    /// Hide all info bars.
    pub fn dismiss_all(&self) {
        let imp = self.imp();
        imp.access_infobar.set_revealed(false);
        imp.discard_infobar.set_revealed(false);
    }

    // --- Query methods ---

    /// Whether the discard/draft info bar is currently revealed.
    pub fn is_discard_revealed(&self) -> bool {
        self.imp().discard_infobar.is_revealed()
    }

    /// Whether the access error info bar is currently revealed.
    pub fn is_access_revealed(&self) -> bool {
        self.imp().access_infobar.is_revealed()
    }

    // --- Callback connectors ---

    /// Set the callback invoked when the "Retry" button is clicked
    /// on the access error bar.
    pub fn connect_retry<F: Fn() + 'static>(&self, f: F) {
        *self.imp().retry_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Set the callback invoked when the "Save" button is clicked
    /// on the draft/discard bar.
    pub fn connect_save<F: Fn() + 'static>(&self, f: F) {
        *self.imp().save_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Set the callback invoked when the "Discard" button is clicked
    /// on the draft/discard bar. Also used as "Reload" for external changes.
    pub fn connect_discard<F: Fn() + 'static>(&self, f: F) {
        *self.imp().discard_callback.borrow_mut() = Some(Box::new(f));
    }
}

impl Default for LushtextInfoBar {
    fn default() -> Self {
        Self::new()
    }
}
