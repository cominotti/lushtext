// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination — execution. Projects bus state onto every live surface.
//!
//! One projection pass serves both surfaces: the window's status lane and every
//! open editor's inline alert. It is deliberately a full pass rather than a
//! targeted update, because the bus decides *which* notification wins each
//! surface and that answer can change without the surface being touched — a
//! resolved alert on tab 3 can promote a queued one on tab 3, and an expired
//! transient can uncover a progress message underneath it.
//!
//! The pass is **bounded by the open tab count** and skips any child that is not
//! an editor page, so a window with no tabs projects the status lane and nothing
//! else rather than failing.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::services::notifications::StatusMessage;
use crate::ui::editor_page::LushtextEditorPage;

use super::super::LushtextWindow;
use super::policy;

/// Render every notification surface, pulsing only a status update that is
/// actually visible.
///
/// `expected_status_update` is `Some` only on the publish/update path. Renders
/// caused by expiry sweeps, resolves, or progress heartbeats pass `None`, so
/// maintenance work never flashes the lane.
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
    }

    let tab_view = &imp.tab_view;
    for index in 0..tab_view.n_pages() {
        let page = tab_view.nth_page(index);
        // Tab children are generic GTK widgets, so the dynamic cast is what
        // unlocks editor-specific owner ids and inline alert rendering. A
        // non-editor child is skipped rather than treated as an error.
        if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
            let editor_view = imp
                .notification_bus
                .editor_info_bar_view(editor.notification_owner_id());
            editor.info_bar().render_notification(editor_view.as_ref());
        }
    }
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
