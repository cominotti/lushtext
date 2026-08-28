// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: pure policy — the notification workflow's `policy.rs`.
//!
//! The census recorded this row as owning no pure policy and needing no seam
//! value object. Probing found both, and neither was covered by mutation: the
//! pulse predicate sat inline in the window adapter, the sweep interval was a
//! bare literal, and the editor owner/surface pair was reconstructed at four
//! call sites. The announcement key and lane decisions moved here from the
//! inline-alert widget, which is a called presentation surface and should not be
//! deciding what a screen reader hears.

use std::time::Duration;

use crate::services::notifications::{
    InlineActionNotification, InlineNotificationStyle, NotificationOwner, NotificationSurface,
    StatusMessage,
};
use crate::ui::accessibility::AnnouncementLane;

/// How often the window-owned expiry sweep runs.
///
/// One second is the coarsest interval that still retires a timed notification
/// before a user reads it as stuck. It is a recurring heartbeat rather than a
/// settle timer: it must keep firing while notifications exist, so it is
/// deliberately not a `SupersedingTimer`.
pub const NOTIFICATION_SWEEP_INTERVAL: Duration = Duration::from_secs(1);

/// The bus owner and surface a single editor's notifications live under.
///
/// Reified because the pair was rebuilt at four call sites in the window
/// adapter, each time from the same editor owner id, and passing the two halves
/// separately is exactly the shape where an owner can be handed to a surface
/// parameter without the compiler noticing. Both halves are now derived once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorNotificationTarget {
    owner: NotificationOwner,
    surface: NotificationSurface,
}

impl EditorNotificationTarget {
    /// Derive both halves from one editor's stable notification owner id.
    #[must_use]
    pub fn for_editor(owner_id: usize) -> Self {
        Self {
            owner: NotificationOwner::Editor(owner_id),
            surface: NotificationSurface::EditorInfoBar(owner_id),
        }
    }

    /// The bus owner for this editor.
    #[must_use]
    pub const fn owner(self) -> NotificationOwner {
        self.owner
    }

    /// The bus surface for this editor's inline alert.
    #[must_use]
    pub const fn surface(self) -> NotificationSurface {
        self.surface
    }
}

/// Whether a status update is the message currently occupying the bar.
///
/// A progress update can be published underneath a higher-priority transient
/// message, so the pulse must be tied to the message that actually won the
/// view. Without this, a hidden progress heartbeat would flash the lane for a
/// message the user cannot see.
#[must_use]
pub fn status_update_is_visible(
    status_view: Option<&StatusMessage>,
    expected: &StatusMessage,
) -> bool {
    status_view.is_some_and(|message| message == expected)
}

/// What a screen reader is told when an inline alert is rendered.
#[must_use]
pub fn inline_alert_announcement_text(notification: &InlineActionNotification) -> String {
    format!("{}: {}", notification.title, notification.body)
}

/// The stable throttling key for one inline alert.
///
/// Semantic rather than time-based, so repeated renders of the same warning do
/// not chatter while changed alert content is still announced. This is the
/// behavior the row's only test seam existed to observe; it is production pure
/// policy now, so the seam retired instead of being consolidated into a surface
/// over nothing.
#[must_use]
pub fn inline_alert_announcement_key(notification: &InlineActionNotification) -> String {
    format!(
        "inline-alert:{:?}:{}:{}",
        notification.style, notification.title, notification.body
    )
}

/// Which announcement lane an inline alert belongs in.
///
/// Failed loads need immediate alert treatment; recovery warnings share the
/// status-update lane so repeated rendering stays calm.
#[must_use]
pub fn inline_alert_announcement_lane(style: InlineNotificationStyle) -> AnnouncementLane {
    match style {
        InlineNotificationStyle::Error => AnnouncementLane::Alert,
        InlineNotificationStyle::Warning => AnnouncementLane::StatusUpdate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::notifications::NotificationSeverity;

    fn message(text: &str, severity: NotificationSeverity) -> StatusMessage {
        StatusMessage {
            text: text.to_string(),
            severity,
        }
    }

    fn alert(style: InlineNotificationStyle, title: &str, body: &str) -> InlineActionNotification {
        InlineActionNotification {
            style,
            title: title.to_string(),
            body: body.to_string(),
            primary_button: None,
            secondary_button: None,
        }
    }

    #[test]
    fn status_update_pulses_only_for_the_message_that_won_the_bar() {
        let expected = message("File saved", NotificationSeverity::Info);
        assert!(status_update_is_visible(Some(&expected), &expected));

        let visible = message("File saved", NotificationSeverity::Info);
        let hidden = message("Searching 10 files…", NotificationSeverity::Info);
        assert!(!status_update_is_visible(Some(&visible), &hidden));
        assert!(!status_update_is_visible(None, &expected));
    }

    #[test]
    fn status_update_compares_severity_as_well_as_text() {
        let info = message("Saved", NotificationSeverity::Info);
        let warning = message("Saved", NotificationSeverity::Warning);
        assert!(
            !status_update_is_visible(Some(&info), &warning),
            "same text at a different severity is a different message"
        );
    }

    #[test]
    fn the_sweep_interval_is_one_second() {
        assert_eq!(NOTIFICATION_SWEEP_INTERVAL, Duration::from_secs(1));
        assert_eq!(NOTIFICATION_SWEEP_INTERVAL.as_millis(), 1_000);
    }

    #[test]
    fn an_editor_target_derives_both_halves_from_one_owner_id() {
        let target = EditorNotificationTarget::for_editor(7);
        assert_eq!(target.owner(), NotificationOwner::Editor(7));
        assert_eq!(target.surface(), NotificationSurface::EditorInfoBar(7));
    }

    #[test]
    fn editor_targets_for_different_editors_do_not_collide() {
        let first = EditorNotificationTarget::for_editor(1);
        let second = EditorNotificationTarget::for_editor(2);
        assert_ne!(first.owner(), second.owner());
        assert_ne!(first.surface(), second.surface());
    }

    #[test]
    fn the_announcement_key_is_semantic_and_distinguishes_every_field() {
        let warning = alert(InlineNotificationStyle::Warning, "Restored", "A draft");
        let key = inline_alert_announcement_key(&warning);
        assert_eq!(key, "inline-alert:Warning:Restored:A draft");

        // Each field participates, so a changed alert is announced again.
        assert_ne!(
            key,
            inline_alert_announcement_key(&alert(
                InlineNotificationStyle::Error,
                "Restored",
                "A draft"
            ))
        );
        assert_ne!(
            key,
            inline_alert_announcement_key(&alert(
                InlineNotificationStyle::Warning,
                "Changed",
                "A draft"
            ))
        );
        assert_ne!(
            key,
            inline_alert_announcement_key(&alert(
                InlineNotificationStyle::Warning,
                "Restored",
                "Other"
            ))
        );
    }

    #[test]
    fn the_announcement_key_is_stable_across_repeated_renders() {
        let warning = alert(InlineNotificationStyle::Warning, "Restored", "A draft");
        assert_eq!(
            inline_alert_announcement_key(&warning),
            inline_alert_announcement_key(&warning)
        );
    }

    #[test]
    fn announcement_text_reads_title_then_body() {
        let warning = alert(InlineNotificationStyle::Warning, "Restored", "A draft");
        assert_eq!(
            inline_alert_announcement_text(&warning),
            "Restored: A draft"
        );
    }

    #[test]
    fn errors_alert_immediately_and_warnings_share_the_status_lane() {
        assert_eq!(
            inline_alert_announcement_lane(InlineNotificationStyle::Error),
            AnnouncementLane::Alert
        );
        assert_eq!(
            inline_alert_announcement_lane(InlineNotificationStyle::Warning),
            AnnouncementLane::StatusUpdate
        );
    }
}
