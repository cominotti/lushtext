// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination — execution. Projects bus state onto every live surface
//! and decides, once per alert, when it is spoken.
//!
//! One projection pass serves every surface: the window's status lane and every
//! open editor's inline alert. It is deliberately a full pass rather than a
//! targeted update, because the bus decides *which* notification wins each
//! surface and that answer can change without the surface being touched — a
//! resolved alert on tab 3 can promote a queued one on tab 3, and an expired
//! transient can uncover a progress message underneath it.
//!
//! The pass is **bounded by the open tab count** and skips any child that is not
//! an editor page, so a window with no tabs projects the status lane and nothing
//! else rather than failing.
//!
//! Rendering is silent. The pass runs from every publish, dismiss, resolve,
//! expiry sweep, selected-tab change, and preview-mode change, so announcing
//! from the render made every unrelated pass speak again. Speech is decided
//! here instead: an inline alert is announced once, on the one surface where it
//! is perceivable, the first time it is (`announcement ledger`), and a status
//! message is announced at acknowledgement, when it was just published and won
//! the lane.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::services::notifications::{InlineActionNotification, StatusMessage};
use crate::ui::editor_page::LushtextEditorPage;

use super::super::LushtextWindow;
use super::evidence::InlineAlertAnnouncementCounts;
use super::policy::{self, InlineAlertSurface};

/// Window-owned coordination state of the projection pass.
///
/// Grouped with the window's `notification_bus`. The ledger maps an editor's
/// notification owner id to the key of the alert it was last announced for; it
/// is reconciled against the open tabs on every pass, so it never outgrows the
/// tab count.
#[derive(Debug, Default)]
pub(crate) struct NotificationProjectionState {
    announced: RefCell<HashMap<usize, String>>,
    counts: Cell<InlineAlertAnnouncementCounts>,
}

impl NotificationProjectionState {
    /// How many editors have their current alert recorded as announced.
    #[cfg(feature = "test-utils")]
    pub(super) fn announced_len(&self) -> usize {
        self.announced.borrow().len()
    }

    /// The announcements emitted so far, per surface.
    #[cfg(feature = "test-utils")]
    pub(super) fn counts(&self) -> InlineAlertAnnouncementCounts {
        self.counts.get()
    }

    /// Forget one editor's entry, for example when its tab closes.
    ///
    /// The owner id is an object address that a later editor can reuse, so the
    /// entry is dropped explicitly rather than left for the next pass.
    pub(super) fn forget(&self, owner_id: usize) {
        self.announced.borrow_mut().remove(&owner_id);
    }

    fn count(&self, update: impl FnOnce(&mut InlineAlertAnnouncementCounts)) {
        let mut counts = self.counts.get();
        update(&mut counts);
        self.counts.set(counts);
    }
}

/// Render every notification surface, pulsing and announcing only a status
/// update that is actually visible.
///
/// `expected_status_update` is `Some` only on the publish/update path. Renders
/// caused by expiry sweeps, resolves, progress heartbeats, inline alerts, tab
/// switches, and preview-mode changes pass `None`, so maintenance work never
/// flashes the lane or speaks it again.
pub(super) fn project_notification_surfaces(
    window: &LushtextWindow,
    expected_status_update: Option<&StatusMessage>,
) {
    let imp = window.imp();
    let status_view = imp.notification_bus.status_bar_view();
    imp.status_bar.render_message(status_view.as_ref());

    if let Some(expected) = expected_status_update
        && policy::status_update_is_visible(status_view.as_ref(), expected)
    {
        imp.status_bar.pulse_message_area(expected.severity);
        if imp.status_bar.announce_visible_status(expected) {
            imp.notification_projection
                .count(|counts| counts.status_lane += 1);
        }
    }

    let preview_only = imp.preview_mode.get();
    let tab_view = &imp.tab_view;
    let selected_page = tab_view.selected_page();
    let mut owners_with_alerts = HashSet::new();
    let mut perceivable = None;
    for index in 0..tab_view.n_pages() {
        let page = tab_view.nth_page(index);
        // Tab children are generic GTK widgets, so the dynamic cast is what
        // unlocks editor-specific owner ids and inline alert rendering. A
        // non-editor child is skipped rather than treated as an error.
        let Some(editor) = page.child().downcast::<LushtextEditorPage>().ok() else {
            continue;
        };
        let owner_id = editor.notification_owner_id();
        let editor_view = imp.notification_bus.editor_info_bar_view(owner_id);
        editor.info_bar().render_notification(editor_view.as_ref());
        let Some(notification) = editor_view else {
            continue;
        };
        owners_with_alerts.insert(owner_id);
        let is_selected = selected_page.as_ref() == Some(&page);
        if let Some(surface) = policy::inline_alert_perceivable_on(preview_only, is_selected) {
            perceivable = Some((editor, notification, surface));
        }
    }

    // An editor with no alert, or no longer open, loses its entry, so an alert
    // that comes back after being cleared is announced as a new one.
    imp.notification_projection
        .announced
        .borrow_mut()
        .retain(|owner_id, _| owners_with_alerts.contains(owner_id));

    if let Some((editor, notification, surface)) = perceivable {
        announce_once(window, &editor, &notification, surface);
    }
}

/// Announce `notification` on `surface` unless it was already announced.
///
/// A background tab's alert is not perceivable, so the ledger keeps no entry
/// for it and it stays due until its tab is selected.
fn announce_once(
    window: &LushtextWindow,
    editor: &LushtextEditorPage,
    notification: &InlineActionNotification,
    surface: InlineAlertSurface,
) {
    let projection = &window.imp().notification_projection;
    let owner_id = editor.notification_owner_id();
    let key = policy::inline_alert_announcement_key(notification);
    let due = policy::inline_alert_announcement_due(
        projection
            .announced
            .borrow()
            .get(&owner_id)
            .map(String::as_str),
        &key,
    );
    if !due {
        return;
    }
    match surface {
        InlineAlertSurface::EditorInfoBar => {
            editor.info_bar().announce_alert(notification);
            projection.count(|counts| counts.editor_info_bar += 1);
        }
    }
    projection.announced.borrow_mut().insert(owner_id, key);
}

/// Install the recurring expiry sweep on GTK's local main loop.
///
/// The sweep is the workflow's only out-of-band resumption: the clock, not a
/// user action, decides when a timed notification retires, and control resumes
/// in this closure. It is a recurring heartbeat rather than a settle timer —
/// there is no newest request to supersede — so it stays an explicit
/// `timeout_add_local` per the settle-helper boundary in `.agents/rules/rust.md`.
pub(super) fn arm_expiry_sweep(window: &LushtextWindow) {
    let window_weak = window.downgrade();
    // Weak so this recurring source cannot retain a destroyed window; `Break`
    // removes the source once the upgrade fails.
    let source_id = glib::timeout_add_local(policy::NOTIFICATION_SWEEP_INTERVAL, move || {
        let Some(window) = window_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        if window.imp().notification_bus.sweep_expired() {
            window.render_notifications();
        }
        glib::ControlFlow::Continue
    });
    window
        .imp()
        .notification_sweep_source_id
        .replace(Some(source_id));
}
