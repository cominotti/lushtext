// SPDX-License-Identifier: GPL-3.0-or-later

//! Bottom status bar widget for the workspace toggle, feedback messages,
//! and compact metadata for the active document.

// GTK custom widgets split the public wrapper (`mod.rs`) from private subclass
// state (`imp.rs`) because the GObject type system constructs the implementation.
mod imp;

use std::time::Duration;

use crate::services::notifications::{NotificationSeverity, StatusMessage};
use crate::ui::accessibility::{self, AnnouncementLane};
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

pub use crate::services::notifications::NotificationSeverity as MessageKind;

/// Duration of the visible status-message acknowledgement pulse.
///
/// This mirrors the CSS animation length: shorter values can be missed when
/// users repeat an action quickly, while much longer values make routine
/// messages feel noisy instead of confirmatory.
const STATUS_MESSAGE_PULSE_DURATION: Duration = Duration::from_millis(420);

// `glib::wrapper!` exposes the Rust-facing widget type and records the GTK
// inheritance/interfaces that templates, CSS, accessibility, and layout use.
glib::wrapper! {
    /// Bottom status-bar widget for window feedback, the workspace toggle, and
    /// compact metadata for the active document.
    pub struct LushtextStatusBar(ObjectSubclass<imp::LushtextStatusBar>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextStatusBar {
    /// Create a status bar from its GTK composite template.
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Render the current status-bar notification view from the notification store.
    pub fn render_message(&self, message: Option<&StatusMessage>) {
        let label = &*self.imp().message_label;
        clear_message_classes(label);
        let Some(message) = message else {
            self.clear_message_area_pulse();
            label.set_label("");
            accessibility::set_description(label, "Current editor status and feedback");
            return;
        };

        label.add_css_class(match message.severity {
            NotificationSeverity::Info => "status-info",
            NotificationSeverity::Warning => "status-warning",
            NotificationSeverity::Error => "status-error",
        });
        label.set_label(&message.text);
        accessibility::set_description(label, &message.text);
        self.announce_visible_status(message);
    }

    /// Briefly flash the full status-message lane using the given severity.
    ///
    /// The visual state lives on `message_area_box`, not on the label, so short
    /// messages still acknowledge the whole horizontal area reserved for
    /// feedback between the sidebar toggle and document metadata controls.
    pub fn pulse_message_area(&self, severity: NotificationSeverity) {
        let imp = self.imp();
        let area = &*imp.message_area_box;
        clear_message_area_pulse_classes(area);

        area.add_css_class(match severity {
            NotificationSeverity::Info => "status-pulse-info",
            NotificationSeverity::Warning => "status-pulse-warning",
            NotificationSeverity::Error => "status-pulse-error",
        });

        let alt_class = if imp.pulse_alt.get() {
            "status-pulse-b"
        } else {
            "status-pulse-a"
        };
        imp.pulse_alt.set(!imp.pulse_alt.get());
        area.add_css_class(alt_class);

        // CSS animation timing is visual, but cleanup is widget state. The
        // superseding timer lets a later pulse keep its classes if an older
        // cleanup would fire after rapid repeated notifications.
        imp.pulse_cleanup_timer
            .arm(self, STATUS_MESSAGE_PULSE_DURATION, move |bar, _| {
                let imp = bar.imp();
                clear_message_area_pulse_classes(&imp.message_area_box);
            });
    }

    /// Remove any in-flight message-area pulse and invalidate pending cleanup.
    pub fn clear_message_area_pulse(&self) {
        let imp = self.imp();
        let _ = imp.pulse_cleanup_timer.invalidate();
        clear_message_area_pulse_classes(&imp.message_area_box);
    }

    /// Show or hide the "EditorConfig" indicator in the status bar.
    pub fn set_editorconfig_active(&self, active: bool) {
        self.imp().editorconfig_label.set_visible(active);
        self.imp().editorconfig_separator.set_visible(active);
    }

    /// Update the encoding control label for the active tab.
    pub fn set_encoding_label(&self, label: &str) {
        let button = &*self.imp().encoding_button;
        button.set_label(label);
        accessibility::set_labelled_description(
            button,
            "Choose text encoding",
            &format!("Current text encoding: {label}"),
        );
        accessibility::set_value_text(button, label);
    }

    /// Update the line-ending control label for the active tab.
    pub fn set_line_ending_label(&self, label: &str) {
        let button = &*self.imp().line_ending_button;
        button.set_label(label);
        accessibility::set_labelled_description(
            button,
            "Choose line endings",
            &format!("Current line endings: {label}"),
        );
        accessibility::set_value_text(button, label);
    }

    /// Show or hide the metadata section between the message area and the
    /// workspace toggle. Hidden when no tabs are open.
    pub fn set_metadata_visible(&self, visible: bool) {
        self.imp().metadata_box.set_visible(visible);
        accessibility::set_hidden(&*self.imp().metadata_box, !visible);
    }

    /// Mirror the workspace-sidebar state into the visible toggle button.
    pub fn set_workspace_sidebar_toggle_pressed(&self, pressed: bool) {
        let button = &*self.imp().sidebar_toggle_button;
        if button.is_active() != pressed {
            button.set_active(pressed);
        }
        accessibility::set_pressed(button, pressed);
    }

    /// Announce visible status updates only when they carry user-actionable weight.
    fn announce_visible_status(&self, message: &StatusMessage) {
        let Some(lane) = status_announcement_lane(message.severity) else {
            return;
        };

        let key = format!(
            "status:{}:{}",
            status_announcement_key(message.severity),
            message.text
        );
        self.imp()
            .status_announcement_throttler
            .announce_if_allowed(&*self.imp().message_label, lane, &key, &message.text);
    }

    /// Announce an informational workflow event through the status-bar speech target.
    ///
    /// Routine info messages stay visually quiet by default. Callers use this
    /// explicit path for user-initiated completions and long-running workflow
    /// milestones that should be spoken without making every info status noisy.
    /// Returns whether the announcement was accepted by the throttling policy.
    #[must_use]
    pub fn announce_workflow_update(
        &self,
        lane: AnnouncementLane,
        key: &str,
        message: &str,
    ) -> bool {
        let key = format!("workflow:{key}");
        self.imp()
            .status_announcement_throttler
            .announce_if_allowed(&*self.imp().message_label, lane, &key, message)
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

fn status_announcement_lane(severity: NotificationSeverity) -> Option<AnnouncementLane> {
    match severity {
        NotificationSeverity::Info => None,
        NotificationSeverity::Warning => Some(AnnouncementLane::StatusUpdate),
        NotificationSeverity::Error => Some(AnnouncementLane::Alert),
    }
}

fn status_announcement_key(severity: NotificationSeverity) -> &'static str {
    match severity {
        NotificationSeverity::Info => "info",
        NotificationSeverity::Warning => "warning",
        NotificationSeverity::Error => "error",
    }
}

/// Remove both severity and alternating animation classes before a pulse resets.
fn clear_message_area_pulse_classes(area: &gtk4::Box) {
    area.remove_css_class("status-pulse-info");
    area.remove_css_class("status-pulse-warning");
    area.remove_css_class("status-pulse-error");
    area.remove_css_class("status-pulse-a");
    area.remove_css_class("status-pulse-b");
}
