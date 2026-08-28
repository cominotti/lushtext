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
//! 2. **Project.** `execution` renders the winning status message into the lane
//!    and each editor's winning alert into its own info bar, bounded by the open
//!    tab count. A full pass, because the bus's answer for one surface can change
//!    without that surface being touched.
//! 3. **Acknowledge.** Only a status update that actually won the lane pulses it.
//!    A progress heartbeat published underneath a higher-priority transient
//!    message must not flash for a message the user cannot see, which is
//!    `policy::status_update_is_visible`'s whole job.
//! 4. *(resume)* **Retire.** The expiry sweep is the workflow's only out-of-band
//!    resumption: the **clock** decides when a timed notification retires, and
//!    control resumes in `execution::arm_expiry_sweep`'s closure, which
//!    re-projects if anything actually expired. A recurring heartbeat, not a
//!    settle timer — there is no newest request to supersede.
//!
//! ## Module roles
//!
//! | Module | Role |
//! | --- | --- |
//! | `mod.rs` (this file) | narrative facade |
//! | `policy` | pure policy — the pulse predicate, the sweep interval, the editor target pair, and the inline-alert announcement key, text, and lane |
//! | `execution` | coordination — the bounded projection pass and the sweep source |
//!
//! **Called presentation surfaces**, recorded here and in the matrix row rather
//! than given role names: `ui/status_bar/**` projects one status message onto the
//! lane and owns the pulse animation; `ui/info_bar/**` projects one notification
//! onto an editor's inline alert row. Both **import their view types from
//! `services/notifications.rs`** and define no private copies, which is the
//! honesty test for this split. `services/notifications.rs` is the shared,
//! GTK-free bus — the reducer that decides which notification wins a surface. It
//! is shared with every workflow that reports, so it is not this row's and is not
//! a role.
//!
//! ## What a test reads
//!
//! Production API. This row had exactly **one** gated declaration
//! (`inline_alert_announcement_key_for_test`), and it was a test-only wrapper
//! around a **pure function** rather than an inspection of live state. Its
//! disposition was therefore **retirement onto production pure policy**, not
//! consolidation into an evidence surface: the function moved here as
//! `policy::inline_alert_announcement_key` and the wrapper was deleted, taking
//! the row's seam count to **zero**. A surface would have been a surface over
//! nothing.

pub mod policy;

mod execution;

use glib::subclass::prelude::ObjectSubclassIsExt;

use crate::services::notifications::{
    InlineActionNotification, NotificationOwner, NotificationPayload, NotificationSeverity,
    NotificationSurface, StatusMessage,
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

    /// Stage 4 — arm the recurring expiry sweep.
    pub(super) fn start_notification_sweep_timer(&self) {
        execution::arm_expiry_sweep(self);
    }
}

/// Derive both bus halves for one editor once, rather than at each call site.
fn editor_target(editor: &LushtextEditorPage) -> EditorNotificationTarget {
    EditorNotificationTarget::for_editor(editor.notification_owner_id())
}
