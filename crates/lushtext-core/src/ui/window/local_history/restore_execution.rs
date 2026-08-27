// SPDX-License-Identifier: GPL-3.0-or-later

//! Restoring one snapshot into the user's buffer, and undoing that restore.
//!
//! Stage-order-qualified `restore_execution` for the same reason as its sibling:
//! two execution-shaped jobs in one stage order, both new.
//!
//! The safety property this module exists to hold: **a restore never destroys
//! the buffer it replaces.** The current text is captured and persisted as a
//! `RestoreSafety` snapshot *before* the replacement starts, and the captured
//! body is handed to the editor as an undo affordance afterwards. A failure
//! anywhere before that point puts the selected snapshot back and leaves the
//! buffer alone.

use std::rc::Rc;

use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use libadwaita::prelude::AdwDialogExt;

use crate::model::local_history::LocalHistorySnapshot;
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::services::{json_store, local_history_service};
use crate::ui::buffer_snapshot;
use crate::ui::editor_page::{
    BufferReplacementOutcome, BufferReplacementRequest, BufferReplacementTicket,
    BufferReplacementWorkflow, LushtextEditorPage, PendingWarningAction,
};
use crate::ui::status_bar::MessageKind;

use super::policy;
use super::preview_execution::{
    GuardedLocalHistorySnapshot, LocalHistoryBrowserState, set_local_history_action_enabled,
};
use crate::ui::window::LushtextWindow;

/// Hand the loaded snapshot back to the browser and re-enable its actions.
///
/// Both abandoned-restore paths owe the user exactly this, and they owe it
/// identically: the safety capture failed, or the ticket went stale before the
/// replacement could start. Either way the browser must look the way it did
/// before the attempt — Restore reachable again, Copy reachable again only for a
/// non-empty snapshot, and the snapshot itself back in the slot the next attempt
/// reads. Consumes the state because the snapshot is moved into that slot.
fn return_snapshot_to_browser(state: RestoreWorkState) {
    set_local_history_action_enabled(&state.browser.restore_button, true);
    set_local_history_action_enabled(
        &state.browser.copy_button,
        !state.restore_snapshot.text.is_empty(),
    );
    state
        .browser
        .loaded_snapshot
        .replace(Some(state.restore_snapshot));
}

/// State passed through the restore-safety background capture.
struct RestoreWorkState {
    /// Browser widgets that should be updated when the safety snapshot finishes.
    browser: Rc<LocalHistoryBrowserState>,
    /// Historical snapshot whose body should replace the buffer on success.
    restore_snapshot: GuardedLocalHistorySnapshot,
    /// Editor/path/edit identity captured with the safety body.
    ticket: LocalHistoryReplacementTicket,
}

/// Editor identity a restore or undo must still describe when it publishes.
///
/// A `Ticket` with an `is_current(&editor)` inherent method rather than a
/// separate `Facts` value: the three generations are read from live state at
/// validation time, so there is nothing to capture separately.
#[derive(Clone, Copy)]
pub(super) struct LocalHistoryReplacementTicket {
    editor_generation: u64,
    path_generation: u64,
    pub(super) edit_generation: u64,
}

impl LocalHistoryReplacementTicket {
    pub(super) fn capture(editor: &LushtextEditorPage) -> Self {
        Self {
            editor_generation: editor.local_history_editor_generation(),
            path_generation: editor.local_history_path_generation(),
            edit_generation: editor.local_history_edit_generation(),
        }
    }

    pub(super) fn is_current(self, editor: &LushtextEditorPage) -> bool {
        editor.local_history_editor_generation() == self.editor_generation
            && editor.local_history_path_generation() == self.path_generation
            && editor.local_history_edit_generation() == self.edit_generation
    }
}

impl LushtextWindow {
    pub(in crate::ui::window) fn undo_local_history_restore(&self, editor: &LushtextEditorPage) {
        let Some(undo_text) = editor.take_local_history_restore_undo_text() else {
            self.publish_status_message(
                "There is no local-history restore to undo",
                MessageKind::Warning,
            );
            return;
        };
        let ticket = LocalHistoryReplacementTicket::capture(editor);
        let freshness_editor = editor.downgrade();
        let terminal_editor = editor.downgrade();
        let cancelled_editor = editor.downgrade();
        let window_weak = self.downgrade();
        let request = BufferReplacementRequest::new_guarded_returning_body_on_cancel(
            BufferReplacementTicket {
                workflow: BufferReplacementWorkflow::LocalHistoryUndo,
                generation: ticket.edit_generation,
            },
            undo_text,
            move |_| {
                freshness_editor
                    .upgrade()
                    .is_some_and(|editor| ticket.is_current(&editor))
            },
            move |outcome| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let Some(editor) = terminal_editor.upgrade() else {
                    return;
                };
                if !matches!(
                    outcome,
                    BufferReplacementOutcome::Complete {
                        ticket: BufferReplacementTicket {
                            workflow: BufferReplacementWorkflow::LocalHistoryUndo,
                            generation,
                        },
                        ..
                    } if generation == ticket.edit_generation && ticket.is_current(&editor)
                ) {
                    return;
                }
                editor.finish_local_history_buffer_replacement();
                if let Some(path) = editor.file_path() {
                    window.resolve_notes_for_editor(&editor, &path);
                }
                window.dismiss_editor_notifications(&editor);
                window.publish_status_message("Local-history restore undone", MessageKind::Info);
                window.refresh_status_bar();
            },
            move |body| {
                if let Some(editor) = cancelled_editor.upgrade()
                    && ticket.is_current(&editor)
                {
                    editor.set_local_history_restore_undo_text(Some(body));
                    editor.set_pending_warning_action(Some(
                        PendingWarningAction::UndoLocalHistoryRestore,
                    ));
                }
            },
        );
        editor.replace_buffer_bounded(request);
    }

    pub(super) fn restore_local_history_snapshot(
        browser: &Rc<LocalHistoryBrowserState>,
        snapshot: GuardedLocalHistorySnapshot,
    ) {
        let buffer = browser.editor.buffer();
        let observed_epoch = crate::ui::plain_disposal::progress_disposal_capacity_epoch();
        let Some(admission) = buffer_snapshot::try_admit_progress_snapshot(
            &buffer,
            policy::PREVIEW_RESERVATION_BYTES,
        ) else {
            browser.loaded_snapshot.replace(Some(snapshot));
            browser.restore_pending.set(true);
            let browser_weak = Rc::downgrade(browser);
            browser
                .restore_capacity_wakeup
                .arm(observed_epoch, move || {
                    let Some(browser) = browser_weak.upgrade() else {
                        return;
                    };
                    if !browser.restore_pending.replace(false) {
                        return;
                    }
                    let Some(snapshot) = browser.loaded_snapshot.take() else {
                        return;
                    };
                    LushtextWindow::restore_local_history_snapshot(&browser, snapshot);
                });
            set_local_history_action_enabled(&browser.copy_button, true);
            browser.window.publish_status_message(
                "Local-history restore is waiting for bounded memory capacity",
                MessageKind::Info,
            );
            return;
        };
        browser.restore_pending.set(false);
        browser.restore_capacity_wakeup.cancel();
        let browser_for_restore = Rc::clone(browser);
        let run_restore = move |outcome: buffer_snapshot::BufferSnapshotOutcome| {
            let browser = browser_for_restore;
            browser.restore_snapshot.take();
            let buffer_snapshot::BufferSnapshotOutcome::Captured(undo_payload) = outcome else {
                browser.loaded_snapshot.replace(Some(snapshot));
                set_local_history_action_enabled(&browser.restore_button, true);
                set_local_history_action_enabled(&browser.copy_button, true);
                return;
            };
            let path = browser.path.clone();
            let ticket = LocalHistoryReplacementTicket::capture(&browser.editor);
            spawn_blocking_then(
                RestoreWorkState {
                    browser,
                    restore_snapshot: snapshot,
                    ticket,
                },
                move || {
                    let undo_text = undo_payload.into_guarded_string_on_worker();
                    let data_dir = json_store::data_dir();
                    let result = local_history_service::capture_snapshot_for_path(
                        &data_dir,
                        &path,
                        undo_text.as_str(),
                        crate::model::local_history::LocalHistorySnapshotOrigin::RestoreSafety,
                        crate::services::local_history_service::LocalHistoryCapturePolicy::PreserveDuplicate,
                    );
                    (result, undo_text)
                },
                move |state, (result, undo_text)| {
                    if let Err(error) = result {
                        tracing::error!("Failed to capture local-history safety snapshot: {error}");
                        let window = state.browser.window.clone();
                        return_snapshot_to_browser(state);
                        window.publish_status_message(
                            "Local history restore could not be prepared safely",
                            MessageKind::Error,
                        );
                        return;
                    }
                    if !state.ticket.is_current(&state.browser.editor) {
                        return_snapshot_to_browser(state);
                        return;
                    }

                    let freshness_editor = state.browser.editor.downgrade();
                    let terminal_editor = state.browser.editor.downgrade();
                    let browser = Rc::clone(&state.browser);
                    let cancelled_browser = Rc::clone(&state.browser);
                    let ticket = state.ticket;
                    let restore_meta = state.restore_snapshot.meta.clone();
                    let restore_text = state
                        .restore_snapshot
                        .map_preserving_reservation(|snapshot| snapshot.text);
                    state.browser.editor.replace_buffer_bounded(
                        BufferReplacementRequest::new_guarded_returning_body_on_cancel(
                            BufferReplacementTicket {
                                workflow: BufferReplacementWorkflow::LocalHistoryRestore,
                                generation: ticket.edit_generation,
                            },
                            restore_text,
                            move |_| {
                                freshness_editor
                                    .upgrade()
                                    .is_some_and(|editor| ticket.is_current(&editor))
                            },
                            move |outcome| {
                                let Some(editor) = terminal_editor.upgrade() else {
                                    return;
                                };
                                if !matches!(
                                    outcome,
                                    BufferReplacementOutcome::Complete {
                                        ticket: BufferReplacementTicket {
                                            workflow: BufferReplacementWorkflow::LocalHistoryRestore,
                                            generation,
                                        },
                                        ..
                                    } if generation == ticket.edit_generation && ticket.is_current(&editor)
                                ) {
                                    set_local_history_action_enabled(&browser.restore_button, true);
                                    set_local_history_action_enabled(
                                        &browser.copy_button,
                                        browser
                                            .loaded_snapshot
                                            .borrow()
                                            .as_ref()
                                            .is_some_and(|snapshot| !snapshot.text.is_empty()),
                                    );
                                    return;
                                }
                                editor.set_local_history_restore_undo_text(Some(undo_text));
                                editor.finish_local_history_buffer_replacement();
                                browser.window.dismiss_editor_notifications(&editor);
                                browser
                                    .window
                                    .resolve_notes_for_editor(&editor, browser.path.as_path());
                                editor.emit_inline_notification_with_warning_action(
                                    InlineActionNotification {
                                        style: InlineNotificationStyle::Warning,
                                        title: "Restored from Local History".to_string(),
                                        body: "The previous buffer state was saved as a safety snapshot. Use Undo Restore to switch back immediately.".to_string(),
                                        primary_button: Some("Undo Restore".to_string()),
                                        secondary_button: None,
                                    },
                                    PendingWarningAction::UndoLocalHistoryRestore,
                                );
                                browser.window.publish_status_message(
                                    "Snapshot restored into the editor",
                                    MessageKind::Info,
                                );
                                browser.window.refresh_status_bar();
                                browser.dialog.close();
                            },
                            move |text| {
                                cancelled_browser.loaded_snapshot.replace(Some(
                                    text.map_preserving_reservation(|text| LocalHistorySnapshot {
                                        meta: restore_meta,
                                        text,
                                    }),
                                ));
                            },
                        ),
                    );
                },
            );
        };

        if buffer_snapshot::buffer_requires_chunked_snapshot(&buffer) {
            let snapshot = buffer_snapshot::snapshot_buffer_text_async_progress_budgeted_admitted(
                buffer,
                policy::PREVIEW_RESERVATION_BYTES,
                admission,
                run_restore,
            );
            browser.restore_snapshot.replace(Some(snapshot));
        } else {
            run_restore(buffer_snapshot::BufferSnapshotOutcome::Captured(
                admission.own_direct(buffer_snapshot::snapshot_buffer_text_direct(&buffer)),
            ));
        }
    }
}
