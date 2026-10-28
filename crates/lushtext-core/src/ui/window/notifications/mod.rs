// SPDX-License-Identifier: GPL-3.0-or-later

//! Report workflow results to the user.
//!
//! Not a workflow the user starts — a workflow every *other* workflow finishes
//! in. Its entry points are results: a save completes, a search reports
//! progress, a load fails, a draft is restored. One ordered stage sequence
//! serves all of them, and the window side is the canonical role home because it
//! is the only layer that can see both surfaces at once.
//!
//! ## Stages
//!
//! 1. **Publish.** A caller hands the bus one payload against one owner and one
//!    surface — a transient status message, a persistent inline alert, a
//!    dismissal, or a resolve. The bus decides whether that changed anything and
//!    says so; a publish that changes nothing renders nothing.
//! 2. **Project.** `execution` renders the winning status message into the lane,
//!    each editor's winning alert into its own info bar, and — only while
//!    preview-only mode hides the editor column — the selected tab's alert into
//!    the window-hosted copy above the preview (`preview_document_alert`),
//!    bounded by the open tab count. A full pass, because the bus's answer for
//!    one surface can change without that surface being touched. It runs after
//!    every bus change, and also when the selected tab or the preview mode
//!    changes (`refresh_selected_tab_model_projections`,
//!    `apply_preview_shell_state`), because those decide where an alert can be
//!    seen. Rendering is silent. Exactly one banner shows the selected
//!    document's alert in every layout.
//!    - *(resume)* **Forward.** What the copy shows is recorded as a
//!      `policy::DocumentAlertProjectionTicket`. A click on the copy resumes in
//!      `execution::forward_preview_document_alert`, which checks the ticket
//!      against the live `policy::DocumentAlertFacts` and, while it is current,
//!      activates the same button on the selected editor's own bar, so both
//!      surfaces run the handlers `wire_info_bar` installed and the resulting
//!      bus change re-renders both. A stale ticket (the selection moved while
//!      tab-derived projections were deferred) runs nothing and re-projects.
//!    - **Announce.** An inline alert is spoken exactly once, from the one
//!      surface where it is perceivable, the first time it is
//!      (`policy::inline_alert_perceivable_on`): the selected editor's own bar
//!      outside preview-only mode, the preview copy in it, and nowhere for a
//!      background tab, which is spoken when its tab is first selected. The window's announcement
//!      ledger records the alert key each editor was announced for
//!      (`policy::inline_alert_announcement_due`); re-renders, tab switches,
//!      and other tabs' activity speak nothing, and a changed or re-raised
//!      alert is new.
//! 3. **Acknowledge.** Only a status update that actually won the lane pulses it
//!    and is announced. A progress heartbeat published underneath a
//!    higher-priority transient message must not flash or speak for a message
//!    the user cannot see, which is `policy::status_update_is_visible`'s whole
//!    job. Expiry sweeps, resolves, and every other pass speak nothing.
//! 4. *(resume)* **Retire.** The expiry sweep is the workflow's only out-of-band
//!    resumption: the **clock** decides when a timed notification retires, and
//!    control resumes in `execution::arm_expiry_sweep`'s closure, which
//!    re-projects if anything actually expired. A recurring heartbeat, not a
//!    settle timer — there is no newest request to supersede.
//!
//! ## What the editor info-bar surface carries
//!
//! `NotificationSurface::EditorInfoBar` carries **document-state** alerts only:
//! each one describes the document or its recovery, not the editor widget. A
//! future editor-state message must use a different surface.
//!
//! Each producer states whether the next successful load supersedes its alert
//! (`InlineAlertLifetime`), and the load terminal resolves a superseded alert
//! here, through the bus, so no later pass can bring it back:
//!
//! | Superseded by the next successful load | Outlives a load |
//! | --- | --- |
//! | "Could Not Open File", "Loading Cancelled" | "Draft Not Restored" (both variants) |
//! | "File Has Changed on Disk" | "Draft Restore Failed" |
//! | "Mixed Line Endings Detected" (re-derived from the new bytes) | |
//! | "Draft Changes Restored" / "Document Restored", "Restored from Local History", "Automatic Recovery Paused" | |
//!
//! The outliving alerts point at a preserved recovery body that is still on
//! disk after the reload. The bus keeps one alert per editor, so while one of
//! them is shown the load terminal does not publish "Mixed Line Endings
//! Detected" over it.
//!
//! ## Module roles
//!
//! | Module | Role |
//! | --- | --- |
//! | `mod.rs` (this file) | narrative facade |
//! | `policy` | pure policy — the pulse predicate, the sweep interval, the editor target pair, the inline-alert announcement key, text, and lane, perceivability, the announce-once rule, the preview-copy plan, and the `DocumentAlertProjectionTicket` + `DocumentAlertFacts` seam |
//! | `execution` | coordination — the bounded projection pass, the announcement ledger, the preview projection and its ticket, the forward and focus handoff, and the sweep source |
//! | `evidence` | evidence — `NotificationEvidence`: the preview copy's rendered visibility and ticket, the ledger, and the per-surface announcement counts |
//!
//! **Called presentation surfaces**, recorded here and in the matrix row rather
//! than given role names: `ui/status_bar/**` projects one status message onto the
//! lane and owns the pulse animation; `ui/info_bar/**` projects one notification
//! onto an inline alert row, both the one inside each editor page and the
//! window-hosted instance above the preview (`resources/ui/window.blp`). Both **import their view types from
//! `services/notifications.rs`** and define no private copies, which is the
//! honesty test for this split. `services/notifications.rs` is the shared,
//! GTK-free bus — the reducer that decides which notification wins a surface. It
//! is shared with every workflow that reports, so it is not this row's and is not
//! a role.
//!
//! ## What a test reads
//!
//! The bus's own production views for which notification wins each surface, and
//! `evidence::notification_evidence` for the facts the bus cannot answer: whether
//! the preview copy is rendered and for which alert, which alert each editor was
//! announced for, and how many announcements each surface emitted. The
//! automation snapshot's `surfaces.preview_document_alert_visible` projects
//! from it. The row's one former gated declaration
//! (`inline_alert_announcement_key_for_test`) stays retired onto production pure
//! policy, so its seam count stays zero.

pub mod evidence;
pub mod policy;

mod execution;

pub(crate) use execution::NotificationProjectionState;

use glib::prelude::ObjectExt;
use glib::subclass::prelude::ObjectSubclassIsExt;

use crate::services::notifications::{
    InlineActionNotification, InlineAlertAction, NotificationOwner, NotificationPayload,
    NotificationSeverity, NotificationSurface, StatusMessage,
};
use crate::ui::editor_page::LushtextEditorPage;

use super::LushtextWindow;
use policy::EditorNotificationTarget;

impl LushtextWindow {
    /// Stage 2 — refresh every notification surface from the bus.
    ///
    /// Render-only: it publishes nothing and starts no acknowledgement pulse.
    pub fn render_notifications(&self) {
        execution::project_notification_surfaces(self, None);
    }

    /// Stages 2 and 3 — render, then pulse if `expected` won the status lane.
    pub(crate) fn render_notifications_for_status_update(&self, expected: &StatusMessage) {
        execution::project_notification_surfaces(self, Some(expected));
    }

    /// Stage 1 — publish a transient window-owned status message.
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

    /// Stage 1 — publish an editor-owned inline notification.
    ///
    /// Scoped to the editor's stable owner id, so other tabs keep their alerts.
    pub fn publish_editor_inline_notification(
        &self,
        editor: &LushtextEditorPage,
        notification: InlineActionNotification,
    ) {
        let target = editor_target(editor);
        if self.imp().notification_bus.publish(
            target.owner(),
            target.surface(),
            NotificationPayload::InlineAction(notification),
        ) {
            self.render_notifications();
        }
    }

    /// Stage 1 — dismiss every notification owned by an editor.
    pub fn dismiss_editor_notifications(&self, editor: &LushtextEditorPage) {
        if self
            .imp()
            .notification_bus
            .dismiss_owner(editor_target(editor).owner())
        {
            self.render_notifications();
        }
    }

    /// Stage 1 — resolve this editor's info bar without touching its other surfaces.
    pub(super) fn resolve_editor_inline_notification(&self, editor: &LushtextEditorPage) {
        let target = editor_target(editor);
        if self
            .imp()
            .notification_bus
            .resolve(target.owner(), target.surface())
        {
            self.render_notifications();
        }
    }

    /// Stage 1 — settle `editor`'s alert after a successful load.
    ///
    /// Resolves it when the load `superseded` it, then reports whether the
    /// editor still has an alert (one that outlives the load), so the load
    /// terminal does not publish its own warning over it.
    pub(super) fn settle_editor_inline_notification_after_load(
        &self,
        editor: &LushtextEditorPage,
        superseded: bool,
    ) -> bool {
        if superseded {
            self.resolve_editor_inline_notification(editor);
        }
        self.imp()
            .notification_bus
            .editor_info_bar_view(editor.notification_owner_id())
            .is_some()
    }

    /// Wire the window-hosted preview copy's buttons, once, at construction.
    ///
    /// Each button forwards to the selected editor's own bar at click time, so
    /// both surfaces run the handlers `wire_info_bar` installed on that bar.
    /// Control resumes in `execution::forward_preview_document_alert`, which
    /// acts only while the projection ticket is still current.
    pub(super) fn wire_preview_document_alert(&self) {
        let alert = &*self.imp().preview_document_alert;
        let forward = |action: InlineAlertAction| {
            let window_weak = self.downgrade();
            move || {
                if let Some(window) = window_weak.upgrade() {
                    execution::forward_preview_document_alert(&window, action);
                }
            }
        };
        alert.connect_retry(forward(InlineAlertAction::Retry));
        alert.connect_discard(forward(InlineAlertAction::Primary));
        alert.connect_save(forward(InlineAlertAction::Secondary));
        alert.connect_dismissed(forward(InlineAlertAction::Dismiss));
    }

    /// Forget a closing editor's announcement ledger entry.
    ///
    /// The owner id is an object address a later editor can reuse, so the
    /// entry must not outlive the tab even until the next projection pass.
    pub(super) fn forget_inline_alert_announcement(&self, editor: &LushtextEditorPage) {
        self.imp()
            .notification_projection
            .forget(editor.notification_owner_id());
    }

    /// Stage 4 — arm the recurring expiry sweep.
    pub(super) fn start_notification_sweep_timer(&self) {
        execution::arm_expiry_sweep(self);
    }
}

/// Derive both bus halves for one editor once, rather than at each call site.
fn editor_target(editor: &LushtextEditorPage) -> EditorNotificationTarget {
    EditorNotificationTarget::for_editor(editor.notification_owner_id())
}
