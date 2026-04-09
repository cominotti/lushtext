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

pub const NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineNotificationStyle {
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationOwner {
    Window,
    Search,
    Editor(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationSurface {
    StatusBar,
    EditorInfoBar(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusMessage {
    pub text: String,
    pub severity: NotificationSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineActionNotification {
    pub style: InlineNotificationStyle,
    pub title: String,
    pub body: String,
    pub primary_button: Option<String>,
    pub secondary_button: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationPayload {
    Transient(StatusMessage),
    Progress(StatusMessage),
    InlineAction(InlineActionNotification),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationEvent {
    Publish,
    Update,
    Heartbeat,
    Resolve,
    DismissOwner,
    SweepExpired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRecord {
    pub owner: NotificationOwner,
    pub surface: NotificationSurface,
    pub payload: NotificationPayload,
    pub updated_at: Instant,
    pub expires_at: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct NotificationBus {
    records: RefCell<Vec<NotificationRecord>>,
    generation: Cell<u64>,
}

impl NotificationBus {
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

    pub fn heartbeat(&self, owner: NotificationOwner, surface: NotificationSurface) -> bool {
        self.reduce_at(
            NotificationEvent::Heartbeat,
            Instant::now(),
            owner,
            surface,
            None,
        )
    }

    pub fn resolve(&self, owner: NotificationOwner, surface: NotificationSurface) -> bool {
        self.reduce_at(
            NotificationEvent::Resolve,
            Instant::now(),
            owner,
            surface,
            None,
        )
    }

    pub fn dismiss_owner(&self, owner: NotificationOwner) -> bool {
        self.reduce_at(
            NotificationEvent::DismissOwner,
            Instant::now(),
            owner,
            NotificationSurface::StatusBar,
            None,
        )
    }

    pub fn sweep_expired(&self) -> bool {
        self.reduce_at(
            NotificationEvent::SweepExpired,
            Instant::now(),
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            None,
        )
    }

    pub fn status_bar_view(&self) -> Option<StatusMessage> {
        self.status_bar_view_at(Instant::now())
    }

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

    pub fn editor_info_bar_view(&self, editor_id: usize) -> Option<InlineActionNotification> {
        self.editor_info_bar_view_at(editor_id, Instant::now())
    }

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
        self.prune_expired(now);

        let mut records = self.records.borrow_mut();
        let changed = match event {
            NotificationEvent::Publish => {
                let Some(payload) = payload else {
                    return false;
                };
                publish_record(&mut records, owner, surface, payload, now)
            }
            NotificationEvent::Update => {
                let Some(payload) = payload else {
                    return false;
                };
                update_progress_record(&mut records, owner, surface, payload, now)
            }
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

        if changed {
            self.bump_generation();
        }

        changed
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
    records: &mut Vec<NotificationRecord>,
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
}
