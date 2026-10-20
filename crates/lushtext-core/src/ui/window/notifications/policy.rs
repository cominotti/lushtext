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

/// The one surface on which an inline alert can currently be perceived.
///
/// An alert is perceivable on at most one surface: the editor's own bar while
/// its editor is the window's selected tab and preview-only mode is off, the
/// window-hosted copy above the preview while its editor is selected and
/// preview-only mode is on, and on no surface while its editor is a background
/// tab. Side-by-side needs no case of its own: its preview pane never collapses
/// into an overlay, so the editor's bar stays on screen beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineAlertSurface {
    /// The selected editor's own `LushtextInfoBar`.
    EditorInfoBar,
    /// The window-hosted copy shown above the preview in preview-only mode.
    PreviewSurface,
}

/// Where an editor's alert is perceivable, or `None` where nobody can see it.
///
/// Whether the window itself is the active one is deliberately not an input:
/// each window announces for its own selected tab.
#[must_use]
pub fn inline_alert_perceivable_on(
    preview_only: bool,
    is_selected: bool,
) -> Option<InlineAlertSurface> {
    match (is_selected, preview_only) {
        (false, _) => None,
        (true, false) => Some(InlineAlertSurface::EditorInfoBar),
        (true, true) => Some(InlineAlertSurface::PreviewSurface),
    }
}

/// Whether an alert whose key is `current` still owes its one announcement.
///
/// `announced` is the key the window last announced for that editor. An
/// unchanged key was already spoken; a changed key, or an alert that came back
/// after its record was cleared (which drops the ledger entry), is a new alert.
#[must_use]
pub fn inline_alert_announcement_due(announced: Option<&str>, current: &str) -> bool {
    announced != Some(current)
}

/// What the preview copy was projected for: one editor and one alert.
///
/// Built once, by the projection pass, when it renders an alert onto the
/// window-hosted preview copy. It crosses into the window's state and then into
/// the copy's click closures (two boundaries) and is validated there as a unit
/// against `DocumentAlertFacts`, so a click acts only on the alert the user saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentAlertProjectionTicket {
    owner_id: usize,
    alert_key: String,
}

impl DocumentAlertProjectionTicket {
    /// The notification owner id of the editor whose alert was projected.
    #[must_use]
    pub const fn owner_id(&self) -> usize {
        self.owner_id
    }

    /// The projected alert's `inline_alert_announcement_key`.
    #[must_use]
    pub fn alert_key(&self) -> &str {
        &self.alert_key
    }
}

/// The live state a forwarded preview-copy click is checked against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentAlertFacts {
    /// Whether preview-only mode is active now.
    pub preview_only: bool,
    /// The notification owner id of the selected editor now, if any.
    pub selected_owner: Option<usize>,
    /// The key of the selected editor's current alert, if any.
    pub current_alert_key: Option<String>,
}

/// Plan the preview copy: `Some` only while preview-only mode is active and the
/// selected tab has an alert.
///
/// In editor-only and side-by-side layouts the editor's own bar is on screen,
/// so the copy stays hidden and the user never sees two banners.
#[must_use]
pub fn plan_preview_document_alert(
    preview_only: bool,
    selected_owner: Option<usize>,
    view: Option<&InlineActionNotification>,
) -> Option<DocumentAlertProjectionTicket> {
    if !preview_only {
        return None;
    }
    let owner_id = selected_owner?;
    let notification = view?;
    Some(DocumentAlertProjectionTicket {
        owner_id,
        alert_key: inline_alert_announcement_key(notification),
    })
}

/// Whether a click on the preview copy still refers to what is projected.
///
/// Stale when preview-only was left, the selection moved to another editor, or
/// that editor's alert changed or went away. The selection can move without a
/// re-projection while tab-derived projections are deferred (session-restore
/// batches and bulk close), which is the case this guards.
#[must_use]
pub fn document_alert_projection_is_current(
    ticket: &DocumentAlertProjectionTicket,
    facts: &DocumentAlertFacts,
) -> bool {
    facts.preview_only
        && facts.selected_owner == Some(ticket.owner_id)
        && facts.current_alert_key.as_deref() == Some(ticket.alert_key.as_str())
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

    #[test]
    fn an_alert_is_perceivable_only_on_the_selected_editor_outside_preview_only() {
        assert_eq!(
            inline_alert_perceivable_on(false, true),
            Some(InlineAlertSurface::EditorInfoBar)
        );
        assert_eq!(inline_alert_perceivable_on(false, false), None);
        assert_eq!(inline_alert_perceivable_on(true, false), None);
        assert_eq!(
            inline_alert_perceivable_on(true, true),
            Some(InlineAlertSurface::PreviewSurface),
            "preview-only hides the editor's bar, so the preview copy carries it"
        );
    }

    #[test]
    fn an_announcement_is_due_on_first_appearance_and_on_a_changed_key_only() {
        assert!(inline_alert_announcement_due(
            None,
            "inline-alert:Warning:A:b"
        ));
        assert!(inline_alert_announcement_due(
            Some("inline-alert:Warning:A:b"),
            "inline-alert:Warning:A:c"
        ));
        assert!(!inline_alert_announcement_due(
            Some("inline-alert:Warning:A:b"),
            "inline-alert:Warning:A:b"
        ));
    }

    fn facts_for(ticket: &DocumentAlertProjectionTicket) -> DocumentAlertFacts {
        DocumentAlertFacts {
            preview_only: true,
            selected_owner: Some(ticket.owner_id()),
            current_alert_key: Some(ticket.alert_key().to_string()),
        }
    }

    #[test]
    fn the_preview_copy_is_planned_only_for_preview_only_with_an_alert() {
        let warning = alert(InlineNotificationStyle::Warning, "Changed", "On disk");
        let ticket = plan_preview_document_alert(true, Some(3), Some(&warning))
            .expect("preview-only with an alert projects the copy");
        assert_eq!(ticket.owner_id(), 3);
        assert_eq!(ticket.alert_key(), inline_alert_announcement_key(&warning));

        assert_eq!(
            plan_preview_document_alert(false, Some(3), Some(&warning)),
            None
        );
        assert_eq!(
            plan_preview_document_alert(true, None, Some(&warning)),
            None
        );
        assert_eq!(plan_preview_document_alert(true, Some(3), None), None);
    }

    #[test]
    fn a_matching_ticket_and_facts_pair_is_current() {
        let warning = alert(InlineNotificationStyle::Warning, "Changed", "On disk");
        let ticket = plan_preview_document_alert(true, Some(3), Some(&warning)).expect("ticket");
        assert!(document_alert_projection_is_current(
            &ticket,
            &facts_for(&ticket)
        ));
    }

    #[test]
    fn every_ticket_facts_mismatch_is_stale() {
        let warning = alert(InlineNotificationStyle::Warning, "Changed", "On disk");
        let ticket = plan_preview_document_alert(true, Some(3), Some(&warning)).expect("ticket");

        let left_preview_only = DocumentAlertFacts {
            preview_only: false,
            ..facts_for(&ticket)
        };
        let other_tab = DocumentAlertFacts {
            selected_owner: Some(4),
            ..facts_for(&ticket)
        };
        let no_tab = DocumentAlertFacts {
            selected_owner: None,
            ..facts_for(&ticket)
        };
        let changed_alert = DocumentAlertFacts {
            current_alert_key: Some("inline-alert:Error:Other:Body".to_string()),
            ..facts_for(&ticket)
        };
        let resolved_alert = DocumentAlertFacts {
            current_alert_key: None,
            ..facts_for(&ticket)
        };
        for facts in [
            left_preview_only,
            other_tab,
            no_tab,
            changed_alert,
            resolved_alert,
        ] {
            assert!(
                !document_alert_projection_is_current(&ticket, &facts),
                "{facts:?} must be stale"
            );
        }
    }
}
