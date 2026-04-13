// SPDX-License-Identifier: GPL-3.0-or-later

//! Window-scoped notification rendering and publication helpers.

use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::services::notifications::{
    InlineActionNotification, NotificationOwner, NotificationPayload, NotificationSeverity,
    NotificationSurface,
};
use crate::ui::editor_page::LushtextEditorPage;

use super::LushtextWindow;

impl LushtextWindow {
    pub fn render_notifications(&self) {
        let imp = self.imp();
        let status_view = imp.notification_bus.status_bar_view();
        imp.status_bar.render_message(status_view.as_ref());

        let tab_view = &imp.tab_view;
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let editor_view = imp
                    .notification_bus
                    .editor_info_bar_view(editor.notification_owner_id());
                editor.info_bar().render_notification(editor_view.as_ref());
            }
        }
    }

    pub fn publish_status_message(&self, text: &str, severity: NotificationSeverity) {
        if self.imp().notification_bus.publish(
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            NotificationPayload::Transient(crate::services::notifications::StatusMessage {
                text: text.to_string(),
                severity,
            }),
        ) {
            self.render_notifications();
        }
    }

    pub fn publish_editor_inline_notification(
        &self,
        editor: &LushtextEditorPage,
        notification: InlineActionNotification,
    ) {
        let owner = NotificationOwner::Editor(editor.notification_owner_id());
        let surface = NotificationSurface::EditorInfoBar(editor.notification_owner_id());
        if self.imp().notification_bus.publish(
            owner,
            surface,
            NotificationPayload::InlineAction(notification),
        ) {
            self.render_notifications();
        }
    }

    pub fn dismiss_editor_notifications(&self, editor: &LushtextEditorPage) {
        if self
            .imp()
            .notification_bus
            .dismiss_owner(NotificationOwner::Editor(editor.notification_owner_id()))
        {
            self.render_notifications();
        }
    }

    pub(super) fn start_notification_sweep_timer(&self) {
        let window_weak = self.downgrade();
        let source_id = glib::timeout_add_local(Duration::from_secs(1), move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if window.imp().notification_bus.sweep_expired() {
                window.render_notifications();
            }
            glib::ControlFlow::Continue
        });
        self.imp()
            .notification_sweep_source_id
            .replace(Some(source_id));
    }
}
