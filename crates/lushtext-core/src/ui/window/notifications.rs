// SPDX-License-Identifier: GPL-3.0-or-later

//! Window-scoped notification rendering and publication helpers.
//!
//! This window adapter bridges `NotificationBus` state to the GTK status bar
//! and per-editor info bars, keeping callers from touching notification widgets directly.

use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::services::notifications::{
    InlineActionNotification, NotificationOwner, NotificationPayload, NotificationSeverity,
    NotificationSurface, StatusMessage,
};
use crate::ui::editor_page::LushtextEditorPage;

use super::LushtextWindow;

impl LushtextWindow {
    /// Refresh the status bar and every open editor info bar from the notification bus.
    ///
    /// This render-only GTK path does not publish messages or start acknowledgement pulses.
    pub fn render_notifications(&self) {
        self.render_notifications_with_status_update(None);
    }

    /// Render every notification surface and optionally pulse a visible status update.
    ///
    /// The optional expected message keeps visual acknowledgement tied to the
    /// publish/update path. Generic renders from expiry sweeps, resolves, or
    /// progress heartbeats pass `None` so maintenance work does not flash.
    fn render_notifications_with_status_update(
        &self,
        expected_status_update: Option<&StatusMessage>,
    ) {
        let imp = self.imp();
        let status_view = imp.notification_bus.status_bar_view();
        imp.status_bar.render_message(status_view.as_ref());
        if let Some(expected) = expected_status_update
            && status_update_is_visible(status_view.as_ref(), expected)
        {
            imp.status_bar.pulse_message_area(expected.severity);
        }

        let tab_view = &imp.tab_view;
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            // Tab children are generic GTK widgets, so the dynamic cast unlocks
            // editor-specific notification owner ids and inline alert rendering.
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let editor_view = imp
                    .notification_bus
                    .editor_info_bar_view(editor.notification_owner_id());
                editor.info_bar().render_notification(editor_view.as_ref());
            }
        }
    }

    /// Render notifications after a status-bar publish/update and pulse if it is visible.
    ///
    /// Progress notifications can update underneath a higher-priority transient
    /// message, so callers pass the expected update and this method confirms it
    /// actually won the status-bar view before flashing the message area.
    pub(crate) fn render_notifications_for_status_update(&self, expected: &StatusMessage) {
        self.render_notifications_with_status_update(Some(expected));
    }

    /// Publish a transient window-owned status message and refresh the status bar.
    ///
    /// If this message becomes the visible status view, the message lane pulses as acknowledgement.
    pub fn publish_status_message(&self, text: &str, severity: NotificationSeverity) {
        let message = StatusMessage {
            text: text.to_string(),
            severity,
        };
        if self.imp().notification_bus.publish(
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            NotificationPayload::Transient(message.clone()),
        ) {
            self.render_notifications_for_status_update(&message);
        }
    }

    /// Publish an editor-owned inline notification and re-render notification surfaces.
    ///
    /// The notification is scoped to the editor's stable owner id so other tabs keep their alerts.
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

    /// Dismiss all notifications owned by an editor and refresh visible notification surfaces.
    pub fn dismiss_editor_notifications(&self, editor: &LushtextEditorPage) {
        if self
            .imp()
            .notification_bus
            .dismiss_owner(NotificationOwner::Editor(editor.notification_owner_id()))
        {
            self.render_notifications();
        }
    }

    /// Resolve this editor's info bar without dismissing its other surfaces.
    pub(super) fn resolve_editor_inline_notification(&self, editor: &LushtextEditorPage) {
        let owner = NotificationOwner::Editor(editor.notification_owner_id());
        let surface = NotificationSurface::EditorInfoBar(editor.notification_owner_id());
        if self.imp().notification_bus.resolve(owner, surface) {
            self.render_notifications();
        }
    }

    /// Start the window-owned expiry sweep on GTK's local main loop.
    pub(super) fn start_notification_sweep_timer(&self) {
        let window_weak = self.downgrade();
        // The weak reference keeps this recurring source from retaining a
        // destroyed window; `Break` removes it when upgrade fails.
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

/// Return whether a status update is the message currently occupying the bar.
///
/// This tiny query keeps hidden progress updates from producing an unrelated
/// visual acknowledgement while a transient message is still on top.
fn status_update_is_visible(status_view: Option<&StatusMessage>, expected: &StatusMessage) -> bool {
    status_view.is_some_and(|message| message == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(text: &str, severity: NotificationSeverity) -> StatusMessage {
        StatusMessage {
            text: text.to_string(),
            severity,
        }
    }

    #[test]
    fn status_update_pulses_when_expected_message_is_visible() {
        let expected = message("File saved", NotificationSeverity::Info);
        assert!(status_update_is_visible(Some(&expected), &expected));
    }

    #[test]
    fn status_update_does_not_pulse_when_different_message_is_visible() {
        let expected = message("Searching 10 files…", NotificationSeverity::Info);
        let visible = message("File saved", NotificationSeverity::Info);
        assert!(!status_update_is_visible(Some(&visible), &expected));
    }

    #[test]
    fn status_update_does_not_pulse_when_no_status_message_is_visible() {
        let expected = message("File saved", NotificationSeverity::Info);
        assert!(!status_update_is_visible(None, &expected));
    }
}
