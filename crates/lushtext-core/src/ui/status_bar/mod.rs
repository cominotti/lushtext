// SPDX-License-Identifier: GPL-3.0-or-later

//! Bottom status bar widget — mirrored pane toggles, feedback messages,
//! and file metadata.

mod imp;

use crate::services::notifications::StatusMessage;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

pub use crate::services::notifications::NotificationSeverity as MessageKind;

glib::wrapper! {
    pub struct LushtextStatusBar(ObjectSubclass<imp::LushtextStatusBar>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextStatusBar {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Render the current status-bar notification view from the notification store.
    pub fn render_message(&self, message: Option<&StatusMessage>) {
        let label = &*self.imp().message_label;
        clear_message_classes(label);
        let Some(message) = message else {
            label.set_label("");
            return;
        };

        label.add_css_class(match message.severity {
            crate::services::notifications::NotificationSeverity::Info => "status-info",
            crate::services::notifications::NotificationSeverity::Warning => "status-warning",
            crate::services::notifications::NotificationSeverity::Error => "status-error",
        });
        label.set_label(&message.text);
    }

    /// Show or hide the "EditorConfig" indicator in the status bar.
    pub fn set_editorconfig_active(&self, active: bool) {
        self.imp().editorconfig_label.set_visible(active);
        self.imp().editorconfig_separator.set_visible(active);
    }

    /// Update the encoding control label for the active tab.
    pub fn set_encoding_label(&self, label: &str) {
        self.imp().encoding_button.set_label(label);
    }

    /// Update the line-ending control label for the active tab.
    pub fn set_line_ending_label(&self, label: &str) {
        self.imp().line_ending_button.set_label(label);
    }

    /// Show or hide the metadata section between the message area and the
    /// workspace toggle. Hidden when no tabs are open.
    pub fn set_metadata_visible(&self, visible: bool) {
        self.imp().metadata_box.set_visible(visible);
    }
}

impl Default for LushtextStatusBar {
    fn default() -> Self {
        Self::new()
    }
}

fn clear_message_classes(label: &gtk4::Label) {
    label.remove_css_class("status-info");
    label.remove_css_class("status-warning");
    label.remove_css_class("status-error");
}
