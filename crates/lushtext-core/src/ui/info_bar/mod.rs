// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor info bar widget — contextual warning/error bars above the editor.
//!
//! Matches GNOME Text Editor's info bar design: `GtkInfoBar` with `message-type`
//! set to `warning` (yellow/amber) or `error` (red) for Adwaita theme styling.
//! Three scenarios: file access errors, draft changes restored, and external
//! file modification. Each bar starts hidden and is revealed via `show_*()`.

mod imp;

use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
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
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    pub fn render_notification(&self, notification: Option<&InlineActionNotification>) {
        let imp = self.imp();
        imp.access_infobar.set_revealed(false);
        imp.discard_infobar.set_revealed(false);
        let Some(notification) = notification else {
            return;
        };

        match notification.style {
            InlineNotificationStyle::Error => {
                imp.access_title.set_label(&notification.title);
                imp.access_subtitle.set_label(&notification.body);
                imp.retry_button
                    .set_visible(notification.primary_button.is_some());
                if let Some(label) = &notification.primary_button {
                    imp.retry_button.set_label(label);
                }
                imp.access_infobar.set_revealed(true);
            }
            InlineNotificationStyle::Warning => {
                imp.discard_title.set_label(&notification.title);
                imp.discard_subtitle.set_label(&notification.body);
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
                imp.discard_infobar.set_revealed(true);
            }
        }
    }

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

    pub fn connect_dismissed<F: Fn() + 'static>(&self, f: F) {
        *self.imp().dismissed_callback.borrow_mut() = Some(Box::new(f));
    }
}

impl Default for LushtextInfoBar {
    fn default() -> Self {
        Self::new()
    }
}
