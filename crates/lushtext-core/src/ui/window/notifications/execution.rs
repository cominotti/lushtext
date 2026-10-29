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
//!
//! The pass also projects the selected tab's alert onto the window-hosted copy
//! above the preview, only while preview-only mode hides the editor column, and
//! records what it projected as a `DocumentAlertProjectionTicket`. A click on
//! the copy resumes in `forward_preview_document_alert`, which forwards it to
//! the selected editor's own bar only while that ticket is still current.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::services::notifications::{InlineActionNotification, InlineAlertAction, StatusMessage};
use crate::ui::editor_page::LushtextEditorPage;

use super::super::LushtextWindow;
use super::super::geometry::execution::focus_is_within;
use super::evidence::InlineAlertAnnouncementCounts;
use super::policy::{self, DocumentAlertFacts, DocumentAlertProjectionTicket, InlineAlertSurface};
use crate::ui::window::StagedSurface;

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
    /// What the preview copy currently shows, if anything.
    preview_ticket: RefCell<Option<DocumentAlertProjectionTicket>>,
}

impl NotificationProjectionState {
    /// How many editors have their current alert recorded as announced.
    pub(super) fn announced_len(&self) -> usize {
        self.announced.borrow().len()
    }

    /// The announcements emitted so far, per surface.
    pub(super) fn counts(&self) -> InlineAlertAnnouncementCounts {
        self.counts.get()
    }

    /// The owner id and alert key the preview copy was last projected for.
    pub(super) fn preview_projection(&self) -> Option<(usize, String)> {
        self.preview_ticket
            .borrow()
            .as_ref()
            .map(|ticket| (ticket.owner_id(), ticket.alert_key().to_string()))
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
    let mut selected_editor = None;
    let mut selected_view = None;
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
        let is_selected = selected_page.as_ref() == Some(&page);
        let focus_on_bar = is_selected && focus_is_on(window, editor.info_bar());
        editor.info_bar().render_notification(editor_view.as_ref());
        if focus_on_bar && editor_view.is_none() {
            // A focused Dismiss hides its own button, and GTK leaves focus on
            // the hidden widget; give it back to the document.
            window.ensure_composed(StagedSurface::Content);
            editor.source_view().grab_focus();
        }
        if is_selected {
            selected_editor = Some(editor.clone());
        }
        let Some(notification) = editor_view else {
            continue;
        };
        owners_with_alerts.insert(owner_id);
        if is_selected {
            selected_view = Some(notification.clone());
        }
        if let Some(surface) = policy::inline_alert_perceivable_on(preview_only, is_selected) {
            perceivable = Some((editor, notification, surface));
        }
    }

    // An editor with no alert, or no longer open, loses its entry, so an alert
    // that comes back after being cleared is announced as a new one. A closing
    // tab needs nothing more: its teardown dismisses the editor's alerts, and
    // that pass no longer visits it, so the entry is gone before the owner id
    // (an object address) can be reused.
    imp.notification_projection
        .announced
        .borrow_mut()
        .retain(|owner_id, _| owners_with_alerts.contains(owner_id));

    project_preview_document_alert(
        window,
        preview_only,
        selected_editor.as_ref(),
        selected_view.as_ref(),
    );

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
        InlineAlertSurface::PreviewSurface => {
            window
                .imp()
                .preview_document_alert
                .announce_alert(notification);
            projection.count(|counts| counts.preview_surface += 1);
        }
    }
    projection.announced.borrow_mut().insert(owner_id, key);
}

/// Render the selected tab's alert onto the preview copy, or hide the copy.
///
/// Shown only in preview-only mode; in the other layouts the editor's own bar
/// is on screen, and one banner per document is the rule. When the copy hides
/// while preview-only stays active and keyboard focus was on it, focus moves to
/// the preview text if it is shown, and otherwise to nothing — never to GTK's
/// first focusable fallback, which is a sidebar button. When it hides because
/// preview-only ended, focus on it moves to the selected editor, whose own bar
/// now shows the alert.
fn project_preview_document_alert(
    window: &LushtextWindow,
    preview_only: bool,
    selected_editor: Option<&LushtextEditorPage>,
    selected_view: Option<&InlineActionNotification>,
) {
    let imp = window.imp();
    let alert = &*imp.preview_document_alert;
    let selected_owner = selected_editor.map(LushtextEditorPage::notification_owner_id);
    let plan = policy::plan_preview_document_alert(preview_only, selected_owner, selected_view);
    // GTK leaves focus on a button that hides itself, so focus on the copy is
    // stranded once it hides. (The copy's own Dismiss can instead leave the
    // window with no focus; the forward handles that, because only a click on
    // the copy may move focus from nothing.)
    let focus_on_copy = focus_is_on(window, alert);
    alert.render_notification(plan.as_ref().and(selected_view));
    let hidden_now = plan.is_none();
    imp.notification_projection.preview_ticket.replace(plan);

    if !hidden_now || !focus_on_copy {
        return;
    }
    if preview_only {
        hand_focus_to_preview(window);
        return;
    }
    // Preview-only ended under a focused copy button: the editor column is
    // back, so the document takes focus rather than a hidden button.
    match selected_editor {
        Some(editor) => {
            window.ensure_composed(StagedSurface::Content);
            editor.source_view().grab_focus();
        }
        None => GtkWindowExt::set_focus(window, None::<&gtk4::Widget>),
    }
}

/// Whether the window's focus widget is `bar` or inside it.
fn focus_is_on(window: &LushtextWindow, bar: &impl IsA<gtk4::Widget>) -> bool {
    focus_is_within(window, bar.upcast_ref())
}

/// Move focus off a hidden preview copy, per the overlay focus-restoration rule.
fn hand_focus_to_preview(window: &LushtextWindow) {
    let text_view = window.imp().markdown_preview.text_view();
    if text_view.is_mapped() && text_view.is_focusable() {
        text_view.grab_focus();
    } else {
        GtkWindowExt::set_focus(window, None::<&gtk4::Widget>);
    }
}

/// A button on the preview copy was activated: forward it to the selected
/// editor's own bar, which runs the handlers `wire_info_bar` installed.
///
/// This is the forward's resumption point. The click acts only while the
/// ticket from the last projection still matches the live state; otherwise —
/// the selection moved while tab-derived projections were deferred, or the
/// alert changed — it runs no handler and re-projects instead.
pub(super) fn forward_preview_document_alert(window: &LushtextWindow, action: InlineAlertAction) {
    let imp = window.imp();
    let selected = window.active_editor();
    let facts = DocumentAlertFacts {
        preview_only: imp.preview_mode.get(),
        selected_owner: selected
            .as_ref()
            .map(LushtextEditorPage::notification_owner_id),
        current_alert_key: selected
            .as_ref()
            .and_then(|editor| {
                imp.notification_bus
                    .editor_info_bar_view(editor.notification_owner_id())
            })
            .map(|notification| policy::inline_alert_announcement_key(&notification)),
    };
    let current = imp
        .notification_projection
        .preview_ticket
        .borrow()
        .as_ref()
        .is_some_and(|ticket| policy::document_alert_projection_is_current(ticket, &facts));
    match selected {
        Some(editor) if current => {
            editor.info_bar().activate(action);
            // The copy's own Dismiss hides it before forwarding, which can
            // leave the window with no focus widget at all. That is stranded
            // too, but only here: a resolve the user did not start from the
            // copy must not move focus.
            if imp.preview_mode.get()
                && imp
                    .notification_projection
                    .preview_ticket
                    .borrow()
                    .is_none()
                && GtkWindowExt::focus(window).is_none()
            {
                hand_focus_to_preview(window);
            }
        }
        _ => window.render_notifications(),
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
