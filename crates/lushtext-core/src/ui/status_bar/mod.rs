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

    /// Update the file size display. Pass `None` for untitled tabs
    /// or when no size is available.
    pub fn set_file_size(&self, bytes: Option<u64>) {
        match bytes {
            Some(b) => self.imp().file_size_label.set_label(&format_file_size(b)),
            None => self.imp().file_size_label.set_label(""),
        }
    }

    /// Show or hide the "EditorConfig" indicator in the status bar.
    pub fn set_editorconfig_active(&self, active: bool) {
        self.imp().editorconfig_label.set_visible(active);
    }

    /// Show or hide the metadata section between the message area and the
    /// right-side properties toggle. Hidden when no tabs are open.
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

/// Format a byte count into a human-readable string using SI units,
/// matching GNOME Files / `g_format_size()` convention.
fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1_000.0;
    const MB: f64 = 1_000_000.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        let kb = b / KB;
        // Promote to MB when rounding would produce "1000.0 KB"
        if kb >= 999.95 {
            format!("{:.1} MB", b / MB)
        } else {
            format!("{:.1} KB", kb)
        }
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_file_size_bytes() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(999), "999 B");
    }

    #[test]
    fn test_format_file_size_kb() {
        assert_eq!(format_file_size(1_000), "1.0 KB");
        assert_eq!(format_file_size(1_500), "1.5 KB");
        assert_eq!(format_file_size(500_000), "500.0 KB");
    }

    #[test]
    fn test_format_file_size_kb_to_mb_boundary() {
        // Values near the KB/MB boundary should promote to MB
        // when rounding would produce "1000.0 KB"
        assert_eq!(format_file_size(999_949), "999.9 KB");
        assert_eq!(format_file_size(999_999), "1.0 MB");
    }

    #[test]
    fn test_format_file_size_mb() {
        assert_eq!(format_file_size(1_000_000), "1.0 MB");
        assert_eq!(format_file_size(2_500_000), "2.5 MB");
    }
}
