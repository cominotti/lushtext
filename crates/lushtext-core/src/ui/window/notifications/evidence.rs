// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence surface — the notification workflow's single observable state
//! outside the bus.
//!
//! The bus answers which notification wins each surface through its own
//! production views (`status_bar_view`, `editor_info_bar_view`), and tests keep
//! reading those. What it cannot answer lives in the window: which alert each
//! editor was already announced for (the announcement ledger), what the
//! preview copy was projected for (the projection ticket), and how many
//! announcements each surface emitted. Those facts are read here, through one
//! accessor, instead of through per-field counters on the presentation
//! surfaces, which must not own evidence.
//!
//! * **No field is read from inside a mutable borrow.** The ledger length is
//!   computed and its `Ref` dropped before the struct literal is built; the
//!   counts are a `Cell` copy.
//! * **A disposed window is a stage.** The preview copy's visibility is read
//!   through `try_get()`, and a disposed window reports it hidden; the counts,
//!   ledger, and ticket are plain state.
//! * **Materialization neutrality is vacuous**: the workflow owns no lazy GTK
//!   collection, and reading only inspects one realized widget's visibility.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::super::LushtextWindow;

/// How many inline-alert and status announcements each surface has emitted.
///
/// Counted by the projection pass when it calls a surface's announcement entry
/// point, so a test can tell "announced once" from "announced on every pass"
/// without a live accessibility bridge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InlineAlertAnnouncementCounts {
    /// Announcements from the selected editor's own inline alert.
    pub editor_info_bar: u64,
    /// Announcements from the window-hosted copy above the preview.
    pub preview_surface: u64,
    /// Announcements of a just-published status message that won the lane.
    pub status_lane: u64,
}

/// Everything a test or probe may observe about the notification workflow
/// beyond the bus's own views.
///
/// Read by widget tests and by the automation snapshot, whose
/// `surfaces.preview_document_alert_visible` projects from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationEvidence {
    /// Whether the preview copy is rendered visible: revealing, visible, and
    /// mapped. False for a disposed window.
    pub preview_document_alert_visible: bool,
    /// The editor owner id and alert key the preview copy was last projected
    /// for (the projection ticket), if it shows an alert.
    pub preview_document_alert_projection: Option<(usize, String)>,
    /// Announcements emitted so far, per surface.
    pub announcements: InlineAlertAnnouncementCounts,
    /// Editors whose current alert has already been announced. Never larger
    /// than the open tab count.
    pub announcement_ledger_len: usize,
}

/// Read the whole notification surface.
#[must_use]
pub fn notification_evidence(window: &LushtextWindow) -> NotificationEvidence {
    let imp = window.imp();
    let projection = &imp.notification_projection;
    let preview_document_alert_visible =
        imp.preview_document_alert.try_get().is_some_and(|alert| {
            alert.is_visible() && alert.is_mapped() && alert.imp().alert_revealer.reveals_child()
        });
    let preview_document_alert_projection = projection.preview_projection();
    let announcement_ledger_len = projection.announced_len();
    NotificationEvidence {
        preview_document_alert_visible,
        preview_document_alert_projection,
        announcements: projection.counts(),
        announcement_ledger_len,
    }
}
