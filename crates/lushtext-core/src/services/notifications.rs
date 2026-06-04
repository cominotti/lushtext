// SPDX-License-Identifier: GPL-3.0-or-later

//! Window-scoped notification bus + store.
//!
//! The bus accepts typed notification events while the store owns lifecycle,
//! priority, and timeout behavior for the visible notification surfaces.
//! A 10-second timeout acts as the final guardrail for ephemeral status and
//! progress notifications; editor inline actions remain persistent until the
//! owning editor resolves or dismisses them.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

/// Lifetime for transient status and progress messages.
///
/// Ten seconds is long enough for slow search/save feedback to be noticed, but
/// short enough that stale progress cannot permanently occupy the status bar.
pub const NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(10);

/// User-facing severity used for status-bar styling and inline warning tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationSeverity {
    /// Informational feedback that does not require user intervention.
    Info,
    /// Recoverable warning, usually paired with an action or follow-up choice.
    Warning,
    /// Error state where the requested operation failed.
    Error,
}

/// Visual treatment for editor-scoped inline action notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineNotificationStyle {
    /// Warning-colored banner for recoverable or cautionary states.
    Warning,
    /// Error-colored banner for failed operations.
    Error,
}

/// Logical source that owns a notification record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationOwner {
    /// Window-level workflows such as open/save and layout state.
    Window,
    /// Workspace search progress and completion feedback.
    Search,
    /// One editor tab, keyed by its stable per-window editor ID.
    Editor(usize),
}

/// UI surface where a notification may be rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationSurface {
    /// The persistent bottom status bar for window-level feedback.
    StatusBar,
    /// An editor-owned inline alert row above the text view.
    EditorInfoBar(usize),
}

/// Text and severity rendered by the status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusMessage {
    /// Short user-facing message text.
    pub text: String,
    /// Styling and priority hint for this status message.
    pub severity: NotificationSeverity,
}

/// Persistent inline alert with optional workflow actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineActionNotification {
    /// Warning or error visual treatment for the inline row.
    pub style: InlineNotificationStyle,
    /// Short headline shown in the inline alert.
    pub title: String,
    /// Supporting copy explaining the state or decision.
    pub body: String,
    /// Primary action label, if the workflow exposes one.
    pub primary_button: Option<String>,
    /// Secondary action label, if the workflow exposes one.
    pub secondary_button: Option<String>,
}

/// Payload variants stored by the notification bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationPayload {
    /// Time-limited status-bar message that should win over progress updates.
    Transient(StatusMessage),
    /// Time-limited progress message that can be renewed by heartbeats.
    Progress(StatusMessage),
    /// Persistent editor inline alert with optional action buttons.
    InlineAction(InlineActionNotification),
}

/// Reducer event used to mutate the notification store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationEvent {
    /// Insert or replace a notification payload.
    Publish,
    /// Update an existing progress payload or publish it if missing.
    Update,
    /// Refresh an existing progress timeout without changing its text.
    Heartbeat,
    /// Remove one owner/surface notification.
    Resolve,
    /// Remove all notifications owned by one workflow.
    DismissOwner,
    /// Drop expired notifications without publishing a new payload.
    SweepExpired,
}

/// Stored notification plus ownership, surface, and lifetime metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRecord {
    /// Workflow that owns and may later resolve this record.
    pub owner: NotificationOwner,
    /// UI surface that may render this record.
    pub surface: NotificationSurface,
    /// Renderable payload for the status bar or editor inline row.
    pub payload: NotificationPayload,
    /// Last publish/update/heartbeat time used for priority and expiry.
    pub updated_at: Instant,
    /// Optional expiry time; inline action records stay persistent.
    pub expires_at: Option<Instant>,
}

/// Main-thread notification store for one window.
///
/// `RefCell` and `Cell` keep mutation possible through shared GTK callbacks,
/// while callers receive view snapshots instead of direct access to records.
#[derive(Debug, Default)]
pub struct NotificationBus {
    /// Ordered records for visible notification surfaces.
    records: RefCell<Vec<NotificationRecord>>,
    /// Monotonic counter bumped whenever a visible view may have changed.
    generation: Cell<u64>,
}

impl NotificationBus {
    /// Publish a notification payload and return whether the visible store changed.
    pub fn publish(
        &self,
        owner: NotificationOwner,
        surface: NotificationSurface,
        payload: NotificationPayload,
    ) -> bool {
        self.reduce_at(
            NotificationEvent::Publish,
            Instant::now(),
            owner,
            surface,
            Some(payload),
        )
    }

    /// Update or create a progress notification for one owner and surface.
    pub fn update_progress(
        &self,
        owner: NotificationOwner,
        surface: NotificationSurface,
        text: impl Into<String>,
        severity: NotificationSeverity,
    ) -> bool {
        self.reduce_at(
            NotificationEvent::Update,
            Instant::now(),
            owner,
            surface,
            Some(NotificationPayload::Progress(StatusMessage {
                text: text.into(),
                severity,
            })),
        )
    }

    /// Refresh the timeout for an existing progress notification.
    pub fn heartbeat(&self, owner: NotificationOwner, surface: NotificationSurface) -> bool {
        self.reduce_at(
            NotificationEvent::Heartbeat,
            Instant::now(),
            owner,
            surface,
            None,
        )
    }

    /// Remove one notification identified by owner and surface.
    pub fn resolve(&self, owner: NotificationOwner, surface: NotificationSurface) -> bool {
        self.reduce_at(
            NotificationEvent::Resolve,
            Instant::now(),
            owner,
            surface,
            None,
        )
    }

    /// Remove all notifications owned by one workflow.
    pub fn dismiss_owner(&self, owner: NotificationOwner) -> bool {
        self.reduce_at(
            NotificationEvent::DismissOwner,
            Instant::now(),
            owner,
            NotificationSurface::StatusBar,
            None,
        )
    }

    /// Drop expired notifications using the current clock.
    pub fn sweep_expired(&self) -> bool {
        self.sweep_expired_at(Instant::now())
    }

    /// Drop expired notifications using a caller-provided clock for tests.
    pub fn sweep_expired_at(&self, now: Instant) -> bool {
        self.reduce_at(
            NotificationEvent::SweepExpired,
            now,
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            None,
        )
    }

    /// Return the highest-priority message currently visible in the status bar.
    pub fn status_bar_view(&self) -> Option<StatusMessage> {
        self.status_bar_view_at(Instant::now())
    }

    /// Return the status-bar view at `now`, pruning expired records first.
    pub fn status_bar_view_at(&self, now: Instant) -> Option<StatusMessage> {
        self.prune_expired(now);
        let records = self.records.borrow();

        records
            .iter()
            .filter(|record| record.surface == NotificationSurface::StatusBar)
            .filter_map(|record| match &record.payload {
                NotificationPayload::Transient(message) => {
                    Some((2u8, record.updated_at, message.clone()))
                }
                NotificationPayload::Progress(message) => {
                    Some((1u8, record.updated_at, message.clone()))
                }
                NotificationPayload::InlineAction(_) => None,
            })
            .max_by_key(|(priority, updated_at, _)| (*priority, *updated_at))
            .map(|(_, _, message)| message)
    }

    /// Return the latest inline action notification for one editor.
    pub fn editor_info_bar_view(&self, editor_id: usize) -> Option<InlineActionNotification> {
        self.editor_info_bar_view_at(editor_id, Instant::now())
    }

    /// Return the editor inline view at `now`, pruning expired records first.
    pub fn editor_info_bar_view_at(
        &self,
        editor_id: usize,
        now: Instant,
    ) -> Option<InlineActionNotification> {
        self.prune_expired(now);
        let surface = NotificationSurface::EditorInfoBar(editor_id);
        let records = self.records.borrow();
        records
            .iter()
            .filter(|record| record.surface == surface)
            .filter_map(|record| match &record.payload {
                NotificationPayload::InlineAction(notification) => {
                    Some((record.updated_at, notification.clone()))
                }
                NotificationPayload::Transient(_) | NotificationPayload::Progress(_) => None,
            })
            .max_by_key(|(updated_at, _)| *updated_at)
            .map(|(_, notification)| notification)
    }

    /// Return the current store generation for view refresh de-duplication.
    pub fn generation(&self) -> u64 {
        self.generation.get()
    }

    fn reduce_at(
        &self,
        event: NotificationEvent,
        now: Instant,
        owner: NotificationOwner,
        surface: NotificationSurface,
        payload: Option<NotificationPayload>,
    ) -> bool {
        let expired_changed = self.prune_expired(now);

        let mut records = self.records.borrow_mut();
        let event_changed = match event {
            NotificationEvent::Publish => payload
                .is_some_and(|payload| publish_record(&mut records, owner, surface, payload, now)),
            NotificationEvent::Update => payload.is_some_and(|payload| {
                update_progress_record(&mut records, owner, surface, payload, now)
            }),
            NotificationEvent::Heartbeat => {
                renew_progress_record(&mut records, owner, surface, now)
            }
            NotificationEvent::Resolve => {
                let before = records.len();
                records.retain(|record| !(record.owner == owner && record.surface == surface));
                before != records.len()
            }
            NotificationEvent::DismissOwner => {
                let before = records.len();
                records.retain(|record| record.owner != owner);
                before != records.len()
            }
            NotificationEvent::SweepExpired => false,
        };

        if event_changed {
            self.bump_generation();
        }

        expired_changed || event_changed
    }

    fn prune_expired(&self, now: Instant) -> bool {
        let mut records = self.records.borrow_mut();
        let before = records.len();
        records.retain(|record| record.expires_at.is_none_or(|expires_at| expires_at > now));
        let changed = before != records.len();
        if changed {
            self.bump_generation();
        }
        changed
    }

    fn bump_generation(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
    }
}

fn publish_record(
    records: &mut Vec<NotificationRecord>,
    owner: NotificationOwner,
    surface: NotificationSurface,
    payload: NotificationPayload,
    now: Instant,
) -> bool {
    match &payload {
        NotificationPayload::Transient(_) => {
            records.retain(|record| {
                !(record.surface == surface
                    && matches!(record.payload, NotificationPayload::Transient(_)))
            });
        }
        NotificationPayload::Progress(_) | NotificationPayload::InlineAction(_) => {
            records.retain(|record| !(record.owner == owner && record.surface == surface));
        }
    }

    records.push(NotificationRecord {
        owner,
        surface,
        expires_at: expiry_for_payload(now, &payload),
        payload,
        updated_at: now,
    });
    true
}

fn update_progress_record(
    records: &mut Vec<NotificationRecord>,
    owner: NotificationOwner,
    surface: NotificationSurface,
    payload: NotificationPayload,
    now: Instant,
) -> bool {
    let NotificationPayload::Progress(message) = payload else {
        return false;
    };

    if let Some(existing) = records.iter_mut().find(|record| {
        record.owner == owner
            && record.surface == surface
            && matches!(record.payload, NotificationPayload::Progress(_))
    }) {
        let changed = match &existing.payload {
            NotificationPayload::Progress(current) => current != &message,
            NotificationPayload::Transient(_) | NotificationPayload::InlineAction(_) => true,
        };
        existing.payload = NotificationPayload::Progress(message);
        existing.updated_at = now;
        existing.expires_at = Some(now + NOTIFICATION_TIMEOUT);
        return changed;
    }

    records.push(NotificationRecord {
        owner,
        surface,
        payload: NotificationPayload::Progress(message),
        updated_at: now,
        expires_at: Some(now + NOTIFICATION_TIMEOUT),
    });
    true
}

fn renew_progress_record(
    records: &mut [NotificationRecord],
    owner: NotificationOwner,
    surface: NotificationSurface,
    now: Instant,
) -> bool {
    let Some(existing) = records.iter_mut().find(|record| {
        record.owner == owner
            && record.surface == surface
            && matches!(record.payload, NotificationPayload::Progress(_))
    }) else {
        return false;
    };

    existing.updated_at = now;
    existing.expires_at = Some(now + NOTIFICATION_TIMEOUT);
    true
}

fn expiry_for_payload(now: Instant, payload: &NotificationPayload) -> Option<Instant> {
    match payload {
        NotificationPayload::Transient(_) | NotificationPayload::Progress(_) => {
            Some(now + NOTIFICATION_TIMEOUT)
        }
        NotificationPayload::InlineAction(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transient(text: &str, severity: NotificationSeverity) -> NotificationPayload {
        NotificationPayload::Transient(StatusMessage {
            text: text.to_string(),
            severity,
        })
    }

    fn progress(text: &str) -> NotificationPayload {
        NotificationPayload::Progress(StatusMessage {
            text: text.to_string(),
            severity: NotificationSeverity::Info,
        })
    }

    fn inline(title: &str) -> NotificationPayload {
        NotificationPayload::InlineAction(InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: title.to_string(),
            body: "body".to_string(),
            primary_button: Some("_Discard".to_string()),
            secondary_button: Some("_Save…".to_string()),
        })
    }

    fn progress_with_severity(text: &str, severity: NotificationSeverity) -> NotificationPayload {
        NotificationPayload::Progress(StatusMessage {
            text: text.to_string(),
            severity,
        })
    }

    #[test]
    fn transient_overrides_progress_but_progress_remains_underneath() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            Some(progress("Searching 100 files…")),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now + Duration::from_secs(1),
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            Some(transient("File saved", NotificationSeverity::Info)),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::Heartbeat,
            now + Duration::from_secs(8),
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            None,
        ));

        assert_eq!(
            bus.status_bar_view_at(now + Duration::from_secs(2))
                .expect("transient visible")
                .text,
            "File saved"
        );

        assert_eq!(
            bus.status_bar_view_at(now + Duration::from_secs(12))
                .expect("progress resumes")
                .text,
            "Searching 100 files…"
        );
    }

    #[test]
    fn public_heartbeat_reports_only_matching_progress_records() {
        let bus = NotificationBus::default();

        assert!(!bus.heartbeat(NotificationOwner::Search, NotificationSurface::StatusBar));
        assert!(bus.publish(
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            progress("Searching…"),
        ));
        assert!(bus.heartbeat(NotificationOwner::Search, NotificationSurface::StatusBar));
        assert!(!bus.heartbeat(NotificationOwner::Editor(1), NotificationSurface::StatusBar));
    }

    #[test]
    fn heartbeat_matches_owner_surface_and_progress_payload_only() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Search,
            NotificationSurface::EditorInfoBar(1),
            Some(progress("Decoy same owner")),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now + Duration::from_millis(1),
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            Some(progress("Decoy same surface")),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now + Duration::from_millis(2),
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            Some(progress("Target")),
        ));

        assert!(bus.reduce_at(
            NotificationEvent::Heartbeat,
            now + Duration::from_secs(1),
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            None,
        ));

        let records = bus.records.borrow();
        let decoy_same_owner = records
            .iter()
            .find(|record| record.surface == NotificationSurface::EditorInfoBar(1))
            .expect("decoy same owner");
        let decoy_same_surface = records
            .iter()
            .find(|record| record.owner == NotificationOwner::Window)
            .expect("decoy same surface");
        let target = records
            .iter()
            .find(|record| {
                record.owner == NotificationOwner::Search
                    && record.surface == NotificationSurface::StatusBar
            })
            .expect("target");

        assert_eq!(decoy_same_owner.updated_at, now);
        assert_eq!(
            decoy_same_surface.updated_at,
            now + Duration::from_millis(1)
        );
        assert_eq!(target.updated_at, now + Duration::from_secs(1));
    }

    #[test]
    fn progress_expires_without_completion() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            Some(progress("Searching 8100 files…")),
        ));

        assert!(
            bus.status_bar_view_at(now + Duration::from_secs(9))
                .is_some()
        );
        assert!(
            bus.status_bar_view_at(now + Duration::from_secs(11))
                .is_none()
        );
    }

    #[test]
    fn notifications_expire_at_exact_timeout_boundary() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            Some(transient("Saved", NotificationSeverity::Info)),
        ));

        let just_before_expiry =
            now + NOTIFICATION_TIMEOUT.saturating_sub(Duration::from_millis(1));
        assert!(bus.status_bar_view_at(just_before_expiry).is_some());
        assert!(bus.sweep_expired_at(now + NOTIFICATION_TIMEOUT));
        assert!(bus.status_bar_view_at(now + NOTIFICATION_TIMEOUT).is_none());
    }

    #[test]
    fn heartbeat_renews_progress_lease() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            Some(progress("Searching 100 files…")),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::Heartbeat,
            now + Duration::from_secs(8),
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            None,
        ));

        assert!(
            bus.status_bar_view_at(now + Duration::from_secs(15))
                .is_some()
        );
        assert!(
            bus.status_bar_view_at(now + Duration::from_secs(19))
                .is_none()
        );
    }

    #[test]
    fn update_progress_creates_record_when_missing() {
        let bus = NotificationBus::default();

        assert!(bus.update_progress(
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            "Searching 42 files…",
            NotificationSeverity::Info,
        ));

        let view = bus.status_bar_view().expect("progress visible");
        assert_eq!(view.text, "Searching 42 files…");
        assert_eq!(view.severity, NotificationSeverity::Info);
    }

    #[test]
    fn update_progress_replaces_existing_text_and_severity() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            Some(progress("Searching 1 file…")),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::Update,
            now + Duration::from_secs(1),
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            Some(NotificationPayload::Progress(StatusMessage {
                text: "Search stalled".to_string(),
                severity: NotificationSeverity::Warning,
            })),
        ));

        let view = bus
            .status_bar_view_at(now + Duration::from_secs(2))
            .expect("updated progress visible");
        assert_eq!(view.text, "Search stalled");
        assert_eq!(view.severity, NotificationSeverity::Warning);
    }

    #[test]
    fn update_progress_matches_only_the_requested_progress_record() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Search,
            NotificationSurface::EditorInfoBar(1),
            Some(progress("Decoy same owner")),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now + Duration::from_millis(1),
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            Some(progress("Decoy same surface")),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now + Duration::from_millis(2),
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            Some(progress("Target")),
        ));

        assert!(bus.reduce_at(
            NotificationEvent::Update,
            now + Duration::from_secs(1),
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            Some(progress_with_severity(
                "Updated target",
                NotificationSeverity::Warning
            )),
        ));

        let records = bus.records.borrow();
        assert_eq!(records.len(), 3);
        assert!(
            records.iter().any(|record| {
                record.surface == NotificationSurface::EditorInfoBar(1)
                    && matches!(
                        &record.payload,
                        NotificationPayload::Progress(message)
                            if message.text == "Decoy same owner"
                    )
            }),
            "same-owner decoy should not be updated"
        );
        assert!(
            records.iter().any(|record| {
                record.owner == NotificationOwner::Window
                    && matches!(
                        &record.payload,
                        NotificationPayload::Progress(message)
                            if message.text == "Decoy same surface"
                    )
            }),
            "same-surface decoy should not be updated"
        );
        assert!(
            records.iter().any(|record| {
                record.owner == NotificationOwner::Search
                    && record.surface == NotificationSurface::StatusBar
                    && matches!(
                        &record.payload,
                        NotificationPayload::Progress(message)
                            if message.text == "Updated target"
                                && message.severity == NotificationSeverity::Warning
                    )
            }),
            "target should be updated"
        );
    }

    #[test]
    fn transient_expires_after_timeout() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            Some(transient("Saved", NotificationSeverity::Info)),
        ));

        assert!(
            bus.status_bar_view_at(now + Duration::from_secs(9))
                .is_some()
        );
        assert!(
            bus.status_bar_view_at(now + Duration::from_secs(11))
                .is_none()
        );
    }

    #[test]
    fn sweep_expired_reports_changes_and_advances_generation() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            Some(transient("Saved", NotificationSeverity::Info)),
        ));
        let generation_before_sweep = bus.generation();

        assert!(bus.sweep_expired_at(now + NOTIFICATION_TIMEOUT + Duration::from_secs(1)));
        assert!(
            bus.status_bar_view_at(now + NOTIFICATION_TIMEOUT + Duration::from_secs(1))
                .is_none()
        );
        assert!(bus.generation() > generation_before_sweep);
    }

    #[test]
    fn sweep_expired_reports_false_when_nothing_changed() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(!bus.sweep_expired());
        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            Some(transient("Saved", NotificationSeverity::Info)),
        ));
        assert!(!bus.sweep_expired_at(now + Duration::from_secs(1)));
    }

    #[test]
    fn public_sweep_expired_reports_true_for_expired_records() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        bus.records.borrow_mut().push(NotificationRecord {
            owner: NotificationOwner::Window,
            surface: NotificationSurface::StatusBar,
            payload: transient("Already expired", NotificationSeverity::Info),
            updated_at: now,
            expires_at: Some(now),
        });

        assert!(bus.sweep_expired());
        assert!(bus.records.borrow().is_empty());
    }

    #[test]
    fn resolve_clears_specific_notification_but_preserves_others() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            Some(progress("Searching…")),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Editor(9),
            NotificationSurface::EditorInfoBar(9),
            Some(inline("Document Restored")),
        ));

        assert!(bus.reduce_at(
            NotificationEvent::Resolve,
            now + Duration::from_secs(1),
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            None,
        ));

        assert!(
            bus.status_bar_view_at(now + Duration::from_secs(1))
                .is_none()
        );
        assert!(
            bus.editor_info_bar_view_at(9, now + Duration::from_secs(1))
                .is_some()
        );
    }

    #[test]
    fn resolve_matches_owner_and_surface_together() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            Some(transient("Window saved", NotificationSeverity::Info)),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now + Duration::from_millis(1),
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            Some(progress("Searching…")),
        ));

        assert!(bus.reduce_at(
            NotificationEvent::Resolve,
            now + Duration::from_secs(1),
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            None,
        ));

        let records = bus.records.borrow();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].owner, NotificationOwner::Window);
        assert_eq!(records[0].surface, NotificationSurface::StatusBar);
    }

    #[test]
    fn inline_notifications_do_not_expire_automatically() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Editor(4),
            NotificationSurface::EditorInfoBar(4),
            Some(inline("File Has Changed on Disk")),
        ));

        assert!(
            bus.editor_info_bar_view_at(4, now + Duration::from_secs(60))
                .is_some()
        );
    }

    #[test]
    fn generation_changes_when_records_change() {
        let bus = NotificationBus::default();
        let generation_before = bus.generation();

        assert!(bus.publish(
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            transient("Saved", NotificationSeverity::Info),
        ));
        let generation_after_publish = bus.generation();
        assert!(generation_after_publish > generation_before);

        assert!(bus.resolve(NotificationOwner::Window, NotificationSurface::StatusBar));
        assert!(bus.generation() > generation_after_publish);
    }

    #[test]
    fn inline_notifications_are_scoped_per_editor() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Editor(7),
            NotificationSurface::EditorInfoBar(7),
            Some(inline("Draft Changes Restored")),
        ));

        assert_eq!(
            bus.editor_info_bar_view_at(7, now)
                .expect("editor notification visible")
                .title,
            "Draft Changes Restored"
        );
        assert!(bus.editor_info_bar_view_at(8, now).is_none());
    }

    #[test]
    fn public_editor_info_bar_view_returns_latest_inline_notification() {
        let bus = NotificationBus::default();

        assert!(bus.publish(
            NotificationOwner::Editor(7),
            NotificationSurface::EditorInfoBar(7),
            inline("Draft Changes Restored"),
        ));

        assert_eq!(
            bus.editor_info_bar_view(7)
                .expect("editor notification visible")
                .title,
            "Draft Changes Restored"
        );
        assert!(bus.editor_info_bar_view(8).is_none());
    }

    #[test]
    fn publish_replaces_only_conflicting_records() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            Some(progress("Search progress")),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now + Duration::from_millis(1),
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            Some(transient("First transient", NotificationSeverity::Info)),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now + Duration::from_millis(2),
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            Some(transient("Second transient", NotificationSeverity::Warning)),
        ));

        {
            let records = bus.records.borrow();
            assert_eq!(
                records.len(),
                2,
                "new transient should replace old transient"
            );
            assert!(
                records
                    .iter()
                    .any(|record| matches!(record.payload, NotificationPayload::Progress(_))),
                "progress should remain under transient status"
            );
        }

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now + Duration::from_millis(3),
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            Some(progress("Window progress")),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now + Duration::from_millis(4),
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            Some(progress("Window progress updated")),
        ));

        let records = bus.records.borrow();
        assert_eq!(
            records.len(),
            2,
            "same owner/surface progress should replace"
        );
        assert!(
            records.iter().any(|record| {
                record.owner == NotificationOwner::Search
                    && matches!(
                        &record.payload,
                        NotificationPayload::Progress(message)
                            if message.text == "Search progress"
                    )
            }),
            "different owner progress on the same surface should remain"
        );
        assert!(
            records.iter().any(|record| {
                record.owner == NotificationOwner::Window
                    && matches!(
                        &record.payload,
                        NotificationPayload::Progress(message)
                            if message.text == "Window progress updated"
                    )
            }),
            "same owner/surface progress should be replaced by newest payload"
        );
    }

    #[test]
    fn dismiss_owner_clears_all_owned_notifications() {
        let bus = NotificationBus::default();
        let now = Instant::now();

        assert!(bus.reduce_at(
            NotificationEvent::Publish,
            now,
            NotificationOwner::Editor(3),
            NotificationSurface::EditorInfoBar(3),
            Some(inline("File Has Changed on Disk")),
        ));
        assert!(bus.reduce_at(
            NotificationEvent::DismissOwner,
            now + Duration::from_secs(1),
            NotificationOwner::Editor(3),
            NotificationSurface::StatusBar,
            None,
        ));

        assert!(
            bus.editor_info_bar_view_at(3, now + Duration::from_secs(1))
                .is_none()
        );
    }

    #[test]
    fn public_dismiss_owner_reports_whether_records_changed() {
        let bus = NotificationBus::default();

        assert!(!bus.dismiss_owner(NotificationOwner::Editor(3)));
        assert!(bus.publish(
            NotificationOwner::Editor(3),
            NotificationSurface::EditorInfoBar(3),
            inline("File Has Changed on Disk"),
        ));
        assert!(bus.dismiss_owner(NotificationOwner::Editor(3)));
        assert!(!bus.dismiss_owner(NotificationOwner::Editor(3)));
    }
}
