// SPDX-License-Identifier: GPL-3.0-or-later

//! Cross-cutting window callbacks wired by `LushtextWindow` onto each editor tab.

use std::cell::RefCell;
use std::rc::Rc;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use lushtext_services::coordination::notifications::InlineActionNotification;

use super::LushtextEditorPage;
use super::document_identity::InlineAlertLifetime;

type MemoryChangedCallback = Rc<dyn Fn()>;
type NotificationCallback = Rc<dyn Fn(InlineActionNotification)>;
type LoadSettledNotificationCallback = Rc<dyn Fn(bool) -> bool>;

#[derive(Default)]
pub(crate) struct EditorCallbacks {
    pub(crate) memory_changed: RefCell<Option<MemoryChangedCallback>>,
    pub(crate) notification: RefCell<Option<NotificationCallback>>,
    pub(crate) notification_load_settled: RefCell<Option<LoadSettledNotificationCallback>>,
}

impl LushtextEditorPage {
    /// Install the page-to-window callback for residency or safety changes.
    ///
    /// It runs on the GTK main thread after edits, modified-state changes,
    /// load/save transitions, eviction, or access updates. A new callback
    /// replaces the previous window observer.
    pub fn connect_memory_policy_changed<F: Fn() + 'static>(&self, f: F) {
        *self.imp().callbacks.memory_changed.borrow_mut() = Some(Rc::new(f));
    }

    pub fn connect_inline_notification<F: Fn(InlineActionNotification) + 'static>(&self, f: F) {
        *self.imp().callbacks.notification.borrow_mut() = Some(Rc::new(f));
    }

    /// Advance policy freshness and notify the window without copying text.
    pub(crate) fn notify_memory_policy_changed(&self) {
        self.imp()
            .residency
            .policy_generation
            .set(self.imp().residency.policy_generation.get().next());
        let callback = self.imp().callbacks.memory_changed.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
    }

    pub(super) fn invoke_notification_or_render(&self, notification: InlineActionNotification) {
        let callback = self.imp().callbacks.notification.borrow().clone();
        if let Some(callback) = callback {
            callback(notification);
        } else {
            let info_bar = self.info_bar();
            info_bar.render_notification(Some(&notification));
            info_bar.announce_throttled(&notification);
        }
    }

    /// Install the window callback that settles the current alert after a
    /// successful load.
    ///
    /// The callback receives whether the load superseded the alert, resolves
    /// it on the notification bus if so, and returns whether an alert is still
    /// shown for this editor afterwards.
    pub fn connect_inline_notification_load_settled<F: Fn(bool) -> bool + 'static>(&self, f: F) {
        *self.imp().callbacks.notification_load_settled.borrow_mut() = Some(Rc::new(f));
    }

    /// Settle the current alert for a successful load.
    pub(crate) fn settle_inline_notification_after_load(&self) -> bool {
        let superseded = self.imp().document_metadata.alert.get().lifetime
            == InlineAlertLifetime::SupersededByLoad;
        let callback = self
            .imp()
            .callbacks
            .notification_load_settled
            .borrow()
            .clone();
        if let Some(callback) = callback {
            return callback(superseded);
        }
        if superseded {
            self.info_bar().render_notification(None);
            return false;
        }
        self.info_bar().property::<bool>("visible")
    }
}
