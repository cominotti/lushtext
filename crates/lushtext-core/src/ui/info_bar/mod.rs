// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor inline alert widget — contextual warning/error messages above the editor.
//!
//! The widget keeps the old editor-scoped recovery surface but renders it with
//! GTK5-safe building blocks: one `GtkRevealer`, labels, explicit action
//! buttons, and app CSS classes for warning or error styling.

mod imp;

use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::ui::accessibility;
// This widget is a **called presentation surface** of the notification
// workflow, so what a screen reader hears is the workflow's decision, not this
// widget's. Both are imported from the canonical role home rather than
// re-implemented here, which is the honesty test for the split.
use crate::ui::window::notifications::policy::{
    inline_alert_announcement_key, inline_alert_announcement_lane, inline_alert_announcement_text,
};
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

glib::wrapper! {
    // glib::wrapper! exposes the private ObjectSubclass as the public GTK
    // widget type used in templates and by other UI adapters.
    /// Public inline alert widget rendered above an editor tab.
    ///
    /// The wrapper keeps callers on the GTK-facing API while the private
    /// implementation owns template children and reveal/style state.
    pub struct LushtextInfoBar(ObjectSubclass<imp::LushtextInfoBar>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextInfoBar {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Render or clear the editor-scoped inline alert.
    ///
    /// Callers pass the already-selected notification for this editor; this
    /// widget only updates GTK template children, style classes, and reveal
    /// state on the main thread.
    pub fn render_notification(&self, notification: Option<&InlineActionNotification>) {
        let imp = self.imp();
        let Some(notification) = notification else {
            self.hide_alert();
            return;
        };

        imp.alert_title.set_label(&notification.title);
        imp.alert_body.set_label(&notification.body);
        imp.retry_button.set_visible(false);
        imp.discard_button.set_visible(false);
        imp.save_button.set_visible(false);

        match notification.style {
            InlineNotificationStyle::Error => {
                imp.alert_box.remove_css_class("warning");
                imp.alert_box.add_css_class("error");
                imp.retry_button
                    .set_visible(notification.primary_button.is_some());
                if let Some(label) = &notification.primary_button {
                    imp.retry_button.set_label(label);
                }
            }
            InlineNotificationStyle::Warning => {
                imp.alert_box.remove_css_class("error");
                imp.alert_box.add_css_class("warning");
                imp.discard_button
                    .set_visible(notification.primary_button.is_some());
                if let Some(label) = &notification.primary_button {
                    imp.discard_button.set_label(label);
                }
                imp.save_button
                    .set_visible(notification.secondary_button.is_some());
                if let Some(label) = &notification.secondary_button {
                    imp.save_button.set_label(label);
                }
            }
        }

        imp.actions_box.set_visible(true);
        self.set_visible(true);
        imp.alert_revealer.set_visible(true);
        imp.alert_revealer.set_reveal_child(true);
        self.queue_resize();
        if let Some(parent) = self.parent() {
            parent.queue_resize();
        }

        let announcement = inline_alert_announcement_text(notification);
        accessibility::set_labelled_description(
            &*imp.alert_title,
            &announcement,
            &notification.body,
        );
        imp.alert_announcement_throttler.announce_if_allowed(
            &*imp.alert_title,
            inline_alert_announcement_lane(notification.style),
            &inline_alert_announcement_key(notification),
            &announcement,
        );
    }

    fn hide_alert(&self) {
        let imp = self.imp();
        imp.alert_revealer.set_reveal_child(false);
        imp.alert_revealer.set_visible(false);
        imp.actions_box.set_visible(false);
        self.set_visible(false);
    }

    /// Set the callback invoked when the retry action is clicked.
    pub fn connect_retry<F: Fn() + 'static>(&self, f: F) {
        *self.imp().retry_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Set the callback invoked when the save action is clicked.
    pub fn connect_save<F: Fn() + 'static>(&self, f: F) {
        *self.imp().save_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Set the callback invoked by the warning primary action.
    ///
    /// Depending on the active warning, this button can discard restored
    /// drafts, reload external file changes, normalize line endings, or undo a
    /// local-history restore.
    pub fn connect_discard<F: Fn() + 'static>(&self, f: F) {
        *self.imp().discard_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Set the callback invoked when the user explicitly dismisses the alert.
    pub fn connect_dismissed<F: Fn() + 'static>(&self, f: F) {
        *self.imp().dismissed_callback.borrow_mut() = Some(Box::new(f));
    }
}

impl Default for LushtextInfoBar {
    fn default() -> Self {
        Self::new()
    }
}
