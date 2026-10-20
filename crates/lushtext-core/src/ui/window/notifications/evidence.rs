// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence surface — the notification workflow's single observable state
//! outside the bus.
//!
//! The bus answers which notification wins each surface through its own
//! production views (`status_bar_view`, `editor_info_bar_view`), and tests keep
//! reading those. What it cannot answer lives in the window: which alert each
//! editor was already announced for (the announcement ledger) and how many
//! announcements each surface emitted. Those facts are read here, through one
//! accessor, instead of through per-field counters on the presentation
//! surfaces, which must not own evidence.
//!
//! * **No field is read from inside a mutable borrow.** The ledger length is
//!   computed and its `Ref` dropped before the struct literal is built; the
//!   counts are a `Cell` copy.
//! * **A disposed window is a stage.** Nothing here derives from a template
//!   child, so a disposed window reads its last counts and ledger honestly.
//! * **Materialization neutrality is vacuous**: the workflow owns no lazy GTK
//!   collection, and reading touches no widget at all.

#[cfg(feature = "test-utils")]
use glib::subclass::prelude::ObjectSubclassIsExt;

#[cfg(feature = "test-utils")]
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
    /// Announcements of a just-published status message that won the lane.
    pub status_lane: u64,
}

/// Everything a test or probe may observe about the notification workflow
/// beyond the bus's own views.
///
/// Gated on `test-utils` while widget tests are its only reader.
#[cfg(feature = "test-utils")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationEvidence {
    /// Announcements emitted so far, per surface.
    pub announcements: InlineAlertAnnouncementCounts,
    /// Editors whose current alert has already been announced. Never larger
    /// than the open tab count.
    pub announcement_ledger_len: usize,
}

/// Read the whole notification surface.
#[cfg(feature = "test-utils")]
#[must_use]
pub fn notification_evidence(window: &LushtextWindow) -> NotificationEvidence {
    let projection = &window.imp().notification_projection;
    let announcement_ledger_len = projection.announced_len();
    NotificationEvidence {
        announcements: projection.counts(),
        announcement_ledger_len,
    }
}
