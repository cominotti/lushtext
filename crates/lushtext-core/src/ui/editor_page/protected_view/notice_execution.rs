// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`notice_execution`, the notice stage of the Protected
//! View workflow; `protected-view-inspector-and-surfaces` task 2.3).
//!
//! - **The alert.** One bounded inline alert per protected document
//!   (`policy::protected_alert`): published from an idle after an install
//!   terminal or an edit entry, so the load terminal's own alert settling runs
//!   first, and merged with the restored-draft alert of a recovered draft. Its
//!   primary button opens the Line Inspector, runs the entry undo, or (for a
//!   recovered draft) discards the draft; the window wires them through
//!   `PendingWarningAction`.
//! - **The notice.** A pre-empted request (paste, Ctrl+H, cycling invisibles,
//!   toggling a bookmark, an unavailable Ctrl+Z) publishes its
//!   `policy::notice_text` as one status message, rate-limited by
//!   `policy::notice_due`, so ten Ctrl+V presses read as one message.

use std::cell::Cell;

use gtk4::prelude::*;

use super::policy::{
    ProtectedAction, ProtectedAlertFacts, ProtectedAlertPrimary, ProtectedNotice, notice_due,
    notice_text, preemption_notice, protected_alert,
};
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::ui::editor_page::{InlineAlertLifetime, LushtextEditorPage, PendingWarningAction};
use crate::ui::status_bar::MessageKind;
use crate::ui::window::LushtextWindow;

/// The editor's notice state.
#[derive(Debug, Default)]
pub(crate) struct ProtectedNoticeState {
    /// The last notice published, and when (monotonic µs).
    last_notice: Cell<Option<(ProtectedNotice, i64)>>,
    /// Notices published (evidence).
    notices_published: Cell<u64>,
    /// Protected View alerts published (evidence).
    alerts_published: Cell<u64>,
    /// An alert publication is queued on an idle.
    alert_queued: Cell<bool>,
}

impl LushtextEditorPage {
    /// A request Protected View pre-empted: publish its notice, rate-limited.
    pub(crate) fn notify_protected_preemption(&self, action: ProtectedAction) {
        if let Some(notice) = preemption_notice(action) {
            self.publish_protected_notice(notice);
        }
    }

    /// Publish `notice` as one status message, unless the same notice was
    /// published within the quiet interval.
    pub(crate) fn publish_protected_notice(&self, notice: ProtectedNotice) {
        let state = self.protected_notice_state();
        let now = glib::monotonic_time();
        if !notice_due(notice, state.last_notice.get(), now) {
            return;
        }
        let Some(window) = self.root().and_downcast::<LushtextWindow>() else {
            return;
        };
        state.last_notice.set(Some((notice, now)));
        state
            .notices_published
            .set(state.notices_published.get().saturating_add(1));
        window.publish_status_message(notice_text(notice), MessageKind::Warning);
    }

    /// Queue the Protected View alert for the next idle: after an install
    /// terminal has settled its own alerts, or after the edit-entry emission.
    pub(crate) fn request_protected_alert(&self) {
        let state = self.protected_notice_state();
        if state.alert_queued.replace(true) {
            return;
        }
        let page = self.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(page) = page.upgrade() {
                page.protected_notice_state().alert_queued.set(false);
                page.publish_protected_alert();
            }
        });
    }

    /// Show the Protected View alert for the current session (replacing the
    /// restored-draft alert of a recovered draft with the merged one).
    pub(crate) fn publish_protected_alert(&self) {
        if !self.is_protected_view() || self.alert_outranks_protected_view() {
            return;
        }
        let has_backing_file = self.file_path().is_some();
        let copy = protected_alert(ProtectedAlertFacts {
            folded_paragraphs: self.protected_fold_summary().folded_paragraphs,
            entry_undo_available: self.protected_entry_undo_available(),
            restored_draft: self.is_draft_restored().then_some(has_backing_file),
        });
        let notification = InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: copy.title.to_owned(),
            body: copy.body,
            primary_button: Some(copy.primary_label.to_owned()),
            secondary_button: copy.secondary_label.map(str::to_owned),
        };
        // A recovered draft's alert describes the recovered buffer, which a
        // reload from disk replaces; so does the plain alert, which the next
        // install re-derives.
        match copy.primary {
            ProtectedAlertPrimary::DiscardDraft => {
                self.emit_inline_notification(notification, InlineAlertLifetime::SupersededByLoad);
            }
            ProtectedAlertPrimary::InspectLine => self
                .emit_inline_notification_with_warning_action(
                    notification,
                    PendingWarningAction::InspectLine,
                    InlineAlertLifetime::SupersededByLoad,
                ),
            ProtectedAlertPrimary::EntryUndo => self.emit_inline_notification_with_warning_action(
                notification,
                PendingWarningAction::UndoProtectedEntry,
                InlineAlertLifetime::SupersededByLoad,
            ),
        }
        let state = self.protected_notice_state();
        state
            .alerts_published
            .set(state.alerts_published.get().saturating_add(1));
    }

    /// Whether the editor shows an alert the Protected View alert must not
    /// replace: one whose action protects the user's text (Undo Restore after
    /// a local-history restore, Show in Local History for a stale draft's
    /// edits) or one that outlives a load ("Draft Not Restored", "Draft
    /// Restore Failed"). The bus keeps one alert per editor, so replacing
    /// either would take its action away; the view-only state is still in
    /// the editor's description, the menu and the notice.
    fn alert_outranks_protected_view(&self) -> bool {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        let shown = self.info_bar().property::<bool>("visible");
        let protective = matches!(
            self.pending_warning_action(),
            Some(
                PendingWarningAction::UndoLocalHistoryRestore
                    | PendingWarningAction::ShowLocalHistory
            )
        );
        let outlives_load =
            self.imp().document_metadata.alert_lifetime.get() == InlineAlertLifetime::OutlivesLoad;
        shown && (protective || outlives_load)
    }

    /// `(notices published, alerts published)`, for the evidence.
    pub(crate) fn protected_notice_counts(&self) -> (u64, u64) {
        let state = self.protected_notice_state();
        (state.notices_published.get(), state.alerts_published.get())
    }

    fn protected_notice_state(&self) -> &ProtectedNoticeState {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        &self.imp().protected_notice
    }
}
