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
//! # Closing during a Replace All run (design D8)
//!
//! A close while a run is in flight asks first ("Replace All Is Still
//! Running"): Keep Running closes nothing; Close When Finished lets the run
//! complete, keeps its journal active, and then re-enters the ordinary close
//! path; Stop and Close cancels the run, which rolls back every file it
//! rewrote, and then closes. Either way each file is original or replaced with
//! an active journal entry. The run's completion resumes here
//! (`close_if_requested_after_replace_run`), after the panel has finished its
//! transaction.
//!
//! Slot 7b extracted this from `ui/window/search.rs`, whose matrix cell claimed
//! this row's files were "all under `ui/search_panel/**`" — false by 928
//! production lines at the time.

use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use crate::services::notifications::{
    NotificationOwner, NotificationSeverity, NotificationSurface, StatusMessage,
};

use super::LushtextWindow;

/// Response ids of the close-during-run dialog.
const RESPONSE_KEEP_RUNNING: &str = "keep-running";
const RESPONSE_STOP_AND_CLOSE: &str = "stop-and-close";
const RESPONSE_CLOSE_WHEN_FINISHED: &str = "close-when-finished";

/// Compose the bounded progress message for a running workspace search.
///
/// Deliberately reports only the count of files searched. It must not report a
/// total: the palette's file-index total is a different population, and using
/// it here once made the status lane claim a denominator the search was not
/// working against.
pub(super) fn format_search_progress_message(files_searched: usize) -> String {
    format!("Searching {files_searched} files\u{2026}")
}

/// Compose the progress message of a running Replace All (design D8): files
/// finished of the run's files, or the rollback after Cancel.
pub(super) fn format_replace_progress_message(
    files_done: usize,
    files_total: usize,
    rolling_back: bool,
) -> String {
    if rolling_back {
        "Cancelling Replace All: restoring files\u{2026}".to_string()
    } else {
        format!("Replacing in {files_done} of {files_total} files\u{2026}")
    }
}

/// Period of the Replace All progress poll; its first publication waits one
/// period, the search progress delay, so a short run never flashes it.
const REPLACE_PROGRESS_POLL: Duration = Duration::from_millis(500);

impl LushtextWindow {
    /// Stage order **replace progress**: poll the run's atomics every 500 ms,
    /// publish the progress (the publication renews its lease, as the search
    /// heartbeat does), announce it once, and retire it once the run ends.
    pub(crate) fn start_replace_progress_tracking(&self) {
        self.stop_replace_progress_tracking();
        let window_weak = self.downgrade();
        let announced = std::cell::Cell::new(false);
        // Each poll dispatch is one span in the measurement build
        // (`stream-replace-all` task 9.1, bar (c): `B_INTERACT_US`).
        let source_id = glib::timeout_add_local(REPLACE_PROGRESS_POLL, move || {
            crate::ui::responsiveness_probe::span(
                crate::ui::responsiveness_probe::REPLACE_PROGRESS_HEARTBEAT_SPAN,
                || poll_replace_progress(&window_weak, &announced),
            )
        });
        self.imp()
            .search_progress
            .replace_poll_source_id
            .replace(Some(source_id));
    }

    /// Retire the Replace All progress poll and its message.
    pub(crate) fn stop_replace_progress_tracking(&self) {
        if let Some(source_id) = self.imp().search_progress.replace_poll_source_id.take() {
            source_id.remove();
            if self
                .imp()
                .notification_bus
                .resolve(NotificationOwner::Search, NotificationSurface::StatusBar)
            {
                self.render_notifications();
            }
        }
    }

    /// A close arrived while a Replace All run is in flight: ask what to do.
    pub(crate) fn request_close_during_replace_run(&self) {
        if self.imp().search_progress.close_after_replace_run.get() {
            self.publish_status_message(
                "The window will close when Replace All finishes",
                crate::ui::status_bar::MessageKind::Info,
            );
            return;
        }
        let dialog = libadwaita::AlertDialog::new(
            Some("Replace All Is Still Running"),
            Some("Closing now would stop it in the middle of its files."),
        );
        dialog.add_response(RESPONSE_KEEP_RUNNING, "_Keep Running");
        dialog.add_response(RESPONSE_STOP_AND_CLOSE, "_Stop and Close");
        dialog.add_response(RESPONSE_CLOSE_WHEN_FINISHED, "Close When _Finished");
        dialog.set_response_appearance(
            RESPONSE_STOP_AND_CLOSE,
            libadwaita::ResponseAppearance::Destructive,
        );
        dialog.set_response_appearance(
            RESPONSE_CLOSE_WHEN_FINISHED,
            libadwaita::ResponseAppearance::Suggested,
        );
        dialog.set_default_response(Some(RESPONSE_KEEP_RUNNING));
        dialog.set_close_response(RESPONSE_KEEP_RUNNING);

        let group = libadwaita::PreferencesGroup::new();
        group.set_title("Options");
        for (title, subtitle) in [
            (
                "Keep Running",
                "Replace All continues; the window stays open.",
            ),
            (
                "Stop and Close",
                "Replace All stops and restores every file it already rewrote, then the window closes.",
            ),
            (
                "Close When Finished",
                "Replace All completes and keeps its Undo for the next launch, then the window closes.",
            ),
        ] {
            let row = libadwaita::ActionRow::new();
            row.set_title(title);
            row.set_subtitle(subtitle);
            row.set_activatable(false);
            group.add(&row);
        }
        dialog.set_extra_child(Some(&group));

        let window = self.downgrade();
        dialog.connect_response(None, move |_, response| {
            let Some(window) = window.upgrade() else {
                return;
            };
            match response {
                RESPONSE_CLOSE_WHEN_FINISHED => window.close_after_replace_run(false),
                RESPONSE_STOP_AND_CLOSE => window.close_after_replace_run(true),
                _ => {}
            }
        });
        dialog.present(Some(self));
    }

    /// Close once the run finishes, stopping it first when `stop` is set.
    fn close_after_replace_run(&self, stop: bool) {
        let imp = self.imp();
        if !imp.search_panel.replace_run_in_flight() {
            // The run finished while the dialog was open.
            self.close();
            return;
        }
        imp.search_progress.close_after_replace_run.set(true);
        if stop {
            imp.search_panel.request_replace_cancel();
        }
    }

    /// Resumption point: the run's completion, after the panel finished its
    /// transaction. Re-enters the ordinary close path when a close waited.
    pub(crate) fn close_if_requested_after_replace_run(&self) {
        if self
            .imp()
            .search_progress
            .close_after_replace_run
            .replace(false)
        {
            self.close();
        }
    }

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

/// One dispatch of the Replace All progress poll: publish the run's progress
/// and announce it once; `Break` once the run has ended.
fn poll_replace_progress(
    window_weak: &glib::WeakRef<LushtextWindow>,
    announced: &std::cell::Cell<bool>,
) -> glib::ControlFlow {
    let Some(window) = window_weak.upgrade() else {
        return glib::ControlFlow::Break;
    };
    let Some(progress) = window.imp().search_panel.replace_run_progress() else {
        window.imp().search_progress.replace_poll_source_id.take();
        return glib::ControlFlow::Break;
    };
    let (done, total, _, phase) = progress.snapshot();
    let message = format_replace_progress_message(
        done,
        total,
        phase == crate::services::content_search::ReplacePhase::RollingBack,
    );
    window.update_search_progress_message(&message);
    if !announced.replace(true) {
        window.announce_workflow_update(
            crate::ui::accessibility::AnnouncementLane::StatusUpdate,
            "replace-all-progress",
            "Replace All is running",
        );
    }
    glib::ControlFlow::Continue
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
