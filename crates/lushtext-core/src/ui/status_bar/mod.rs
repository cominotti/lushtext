// SPDX-License-Identifier: GPL-3.0-or-later

//! Bottom status bar widget — feedback messages and file metadata.

mod imp;

use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::Object;
use gtk4::glib;
use gtk4::prelude::*;
use std::time::Duration;

const MESSAGE_DISMISS_SECS: u64 = 5;

/// Visual severity of a status bar message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Warning,
    Error,
}

glib::wrapper! {
    pub struct LushtextStatusBar(ObjectSubclass<imp::LushtextStatusBar>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextStatusBar {
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Display a feedback message, replacing any current message.
    /// The message auto-dismisses after 5 seconds unless superseded.
    pub fn push_message(&self, text: &str, kind: MessageKind) {
        let imp = self.imp();
        let label = &*imp.message_label;

        clear_message_classes(label);

        let css_class = match kind {
            MessageKind::Info => "status-info",
            MessageKind::Warning => "status-warning",
            MessageKind::Error => "status-error",
        };
        label.add_css_class(css_class);
        label.set_label(text);

        // Bump generation and schedule auto-dismiss. Stale timers (from
        // previous messages) will see a mismatched generation and no-op.
        let gen = imp.message_generation.get().wrapping_add(1);
        imp.message_generation.set(gen);

        let weak = self.downgrade();
        glib::timeout_add_local_once(Duration::from_secs(MESSAGE_DISMISS_SECS), move || {
            if let Some(bar) = weak.upgrade() {
                if bar.imp().message_generation.get() == gen {
                    bar.clear_message();
                }
            }
        });
    }

    /// Clear the message area immediately.
    pub fn clear_message(&self) {
        let label = &*self.imp().message_label;
        clear_message_classes(label);
        label.set_label("");
    }

    /// Update the file size display. Pass `None` for untitled tabs
    /// or when no size is available.
    pub fn set_file_size(&self, bytes: Option<u64>) {
        match bytes {
            Some(b) => self.imp().file_size_label.set_label(&format_file_size(b)),
            None => self.imp().file_size_label.set_label(""),
        }
    }

    /// Show or hide the right-side metadata section (encoding + file size).
    /// Hidden when no tabs are open.
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
