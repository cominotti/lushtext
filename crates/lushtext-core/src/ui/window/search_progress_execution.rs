// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`execution`) for `WFR-SEARCH-REPLACE`'s **search
//! progress** stage order.
//!
//! Stage-order-qualified because this row already owns
//! `ui/search_panel/execution.rs` and `ui/search_panel/replace_execution.rs` at
//! its canonical role home, and this is a third ordered sequence: **arm a
//! delayed visibility -> publish bounded progress -> heartbeat the
//! notification alive -> retire it at the terminal**.
//!
//! It lives in `ui/window/` rather than beside its siblings because what it
//! coordinates is the **status lane**, which the window owns and the search
//! panel does not. That is the one coordination job the window-side search
//! module held; the rest of `ui/window/search.rs` is a called presentation
//! surface.
//!
//! # Why the heartbeat exists
//!
//! A long workspace search publishes one notification and then goes quiet
//! while the walker runs. The notification lane retires silent owners, so
//! without a heartbeat a search that is still running would have its progress
//! message reaped mid-search and the user would see the status lane go blank
//! while work continued. The heartbeat is the liveness signal, not a redraw.
//!
//! Slot 7b extracted this from `ui/window/search.rs`, whose matrix cell claimed
//! this row's files were "all under `ui/search_panel/**`" — false by 928
//! production lines at the time.

use std::time::Duration;

use glib::object::ObjectExt;
use glib::subclass::prelude::ObjectSubclassIsExt;

use crate::services::notifications::{
    NotificationOwner, NotificationSeverity, NotificationSurface, StatusMessage,
};

use super::LushtextWindow;

/// Compose the bounded progress message for a running workspace search.
///
/// Deliberately reports only the count of files searched. It must not report a
/// total: the palette's file-index total is a different population, and using
/// it here once made the status lane claim a denominator the search was not
/// working against.
pub(super) fn format_search_progress_message(files_searched: usize) -> String {
    format!("Searching {files_searched} files\u{2026}")
}

impl LushtextWindow {
    /// Start delayed status-bar progress tracking for a new workspace search.
    ///
    /// Clears stale progress, arms the 500 ms visibility delay, and starts the
    /// heartbeat timer that keeps active progress notifications alive.
    pub(crate) fn prepare_search_progress_tracking(&self) {
        self.finish_search_progress_tracking();
        let imp = self.imp();
        imp.search_progress.visible.set(false);
        self.start_search_progress_heartbeat();

        imp.search_progress.visibility_timer.arm(
            self,
            Duration::from_millis(500),
            move |window, _| {
                let imp = window.imp();
                if !imp.search_panel.imp().runtime.searching.get()
                    || !imp.search_panel_revealer.reveals_child()
                {
                    return;
                }
                imp.search_progress.visible.set(true);
            },
        );
    }

    /// Publish an informational search-progress update through the notification bus.
    pub(crate) fn update_search_progress_message(&self, message: &str) {
        self.update_search_progress_status_message(message, NotificationSeverity::Info);
    }

    /// Route progress updates through the visible-status pulse gate.
    ///
    /// The expected `StatusMessage` lets rendering pulse only when this progress
    /// update actually occupies the status bar instead of sitting below a transient.
    fn update_search_progress_status_message(&self, message: &str, severity: NotificationSeverity) {
        let status_message = StatusMessage {
            text: message.to_string(),
            severity,
        };
        if self.imp().notification_bus.update_progress(
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            status_message.text.clone(),
            status_message.severity,
        ) {
            self.render_notifications_for_status_update(&status_message);
        }
    }

    /// Publish a search-progress status message through the production routing path.
    ///
    /// Widget tests use this to exercise visible and hidden progress updates
    /// without starting a real workspace search.
    #[cfg(feature = "test-utils")]
    pub fn update_search_progress_message_for_test(
        &self,
        message: &str,
        severity: NotificationSeverity,
    ) {
        self.update_search_progress_status_message(message, severity);
    }

    pub(crate) fn finish_search_progress_tracking(&self) {
        self.imp().search_progress.visible.set(false);
        let _ = self.imp().search_progress.visibility_timer.invalidate();
        self.stop_search_progress_heartbeat();
        if self
            .imp()
            .notification_bus
            .resolve(NotificationOwner::Search, NotificationSurface::StatusBar)
        {
            self.render_notifications();
        }
    }

    fn start_search_progress_heartbeat(&self) {
        self.stop_search_progress_heartbeat();
        let window_weak = self.downgrade();
        let source_id = glib::timeout_add_local(Duration::from_secs(1), move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let imp = window.imp();
            if !imp.search_panel.imp().runtime.searching.get() {
                window.finish_search_progress_tracking();
                return glib::ControlFlow::Break;
            }

            if imp.search_panel_revealer.reveals_child()
                && imp.search_progress.visible.get()
                && imp
                    .notification_bus
                    .heartbeat(NotificationOwner::Search, NotificationSurface::StatusBar)
            {
                window.render_notifications();
            }
            glib::ControlFlow::Continue
        });
        self.imp()
            .search_progress
            .heartbeat_source_id
            .replace(Some(source_id));
    }

    fn stop_search_progress_heartbeat(&self) {
        if let Some(source_id) = self.imp().search_progress.heartbeat_source_id.take() {
            source_id.remove();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::format_search_progress_message;

    #[test]
    fn search_progress_message_does_not_use_palette_index_total() {
        assert_eq!(
            format_search_progress_message(14_100),
            "Searching 14100 files\u{2026}"
        );
    }
}
