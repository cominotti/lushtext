// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination: the document-load workflow's retirement job.
//!
//! Retirement is what happens when a load is asked to stop — by the user, by a
//! newer request, by a save that pre-empts it, or by the page leaving the widget
//! tree. Its job is to give back everything the workflow holds:
//!
//! - the **decoded payload**, which is `DisposalOwned<String>` and is dropped
//!   here so document-sized memory is freed on a disposal-lane worker rather
//!   than on the GTK thread;
//! - the **admission charge**, released exactly once through the permit's
//!   `Drop`, whichever path reached the terminal;
//! - the **partially installed buffer**, cleared in bounded slices that obey the
//!   same paragraph-boundary contract the forward install does;
//! - the **load identity**, advanced so a worker already in flight can never
//!   publish against the request it was dispatched under.
//!
//! ## Cancellation and disposal are different, deliberately
//!
//! Cancellation is a user-visible outcome: the buffer is cleared, the tab is
//! marked failed, and an inline "Loading Cancelled" notice offers a retry.
//! Disposal is silent: the page is leaving, so a retry notification would be
//! invisible and stale, and nothing is published at all.
//!
//! The one thing both must do is leave `installation_incomplete` set until a
//! retry installs one exact payload. The buffer is intentionally emptied on the
//! cancelled path, so a save allowed to run against it would write a truncated
//! file over the user's document.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use gtk4::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::ui::editor_page::{EditorLoadState, LushtextEditorPage};

use super::execution::{self, ChunkedLoadInstall};
use super::policy::{self, AbortAction, AbortDisposition, LoadInstallPhase, LoadOutcome};
use super::{admission, evidence};

/// Cancel any in-progress file load. Safe to call even if no load is active.
pub(crate) fn cancel_load(editor: &LushtextEditorPage) {
    if editor.imp().load.finalizing.get() {
        // Final projection owns the main thread and has no cancellable payload
        // work left. A cancel can still withdraw a reload queued reentrantly by
        // an earlier callback.
        admission::discard_pending_request(editor);
        return;
    }
    let was_loading = editor.imp().load_state.get() == EditorLoadState::Loading;
    let installation_active = editor.imp().load.installation.borrow().is_some();
    admission::discard_pending_request(editor);
    editor
        .imp()
        .load
        .user_cancel_pending
        .set(policy::publishes_user_cancellation(
            was_loading,
            installation_active,
        ));
    cancel_current_load_resources(editor, AbortDisposition::Cancel);
    admission::retire_load_identity(editor);
    if was_loading && !installation_active {
        finish_user_cancelled_load(editor);
    }
}

/// Publish the user-visible terminal state for a cancelled load.
pub(super) fn finish_user_cancelled_load(editor: &LushtextEditorPage) {
    editor.imp().load_state.set(EditorLoadState::Failed);
    editor.imp().latest_load_failed.set(true);
    evidence::record_outcome(editor, LoadOutcome::Cancelled);
    editor.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Error,
        title: "Loading Cancelled".to_string(),
        body: "The document was not fully loaded. Retry when you are ready.".to_string(),
        primary_button: Some("_Retry".to_string()),
        secondary_button: None,
    });
    editor.notify_memory_policy_changed();
    editor.refresh_accessibility_metadata();
}

/// Tear down queued, admitted, or installing load state without UI feedback.
///
/// Disposal is not a user cancellation surface: the page is leaving the widget
/// hierarchy, so a retry notification would be invisible and stale.
pub(crate) fn dispose_load_resources(editor: &LushtextEditorPage) {
    if editor.imp().load.finalizing.get() {
        editor.imp().load.dispose_during_finalization.set(true);
    }
    admission::discard_pending_request(editor);
    editor.imp().load.user_cancel_pending.set(false);
    cancel_current_load_resources(editor, AbortDisposition::Dispose);
    admission::retire_load_identity(editor);
}

/// Stop the queued and planning halves of a load without touching an install.
pub(super) fn cancel_noninstall_load_resources(editor: &LushtextEditorPage) {
    editor
        .imp()
        .load_tracking
        .cancel_token
        .borrow()
        .store(true, Ordering::Release);
    admission::cancel_for_editor(editor);
    admission::finish_load_planning(editor);
}

fn cancel_current_load_resources(editor: &LushtextEditorPage, disposition: AbortDisposition) {
    cancel_noninstall_load_resources(editor);
    let installation = editor.imp().load.installation.borrow().clone();
    if let Some(session) = installation {
        abort_installation(&session, disposition);
    }
}

/// Stop one bounded installation, retiring what it holds.
pub(super) fn abort_installation(
    session: &Rc<RefCell<ChunkedLoadInstall>>,
    disposition: AbortDisposition,
) {
    let (editor, buffer, mark, source, loaded, permit) = {
        let mut state = session.borrow_mut();
        match policy::abort_action(disposition, state.phase(), state.terminal) {
            AbortAction::Ignore => return,
            AbortAction::Dispose => state.terminal = true,
            AbortAction::BeginCancelledClear => {
                state.phase = LoadInstallPhase::ClearingCancelled;
            }
        }
        let permit = if matches!(disposition, AbortDisposition::Dispose) {
            state.permit.take()
        } else {
            None
        };
        (
            state.editor.upgrade(),
            state.buffer.clone(),
            state.end_mark.take(),
            state.source_id.take(),
            state.loaded.take(),
            permit,
        )
    };
    if let Some(source) = source {
        source.remove();
    }
    if let Some(mark) = mark {
        buffer.delete_mark(&mark);
    }
    // Release decoded text before either disposal releases admission or
    // cancellation begins bounded cleanup of the partial GTK buffer.
    drop(loaded);
    if matches!(disposition, AbortDisposition::Dispose) {
        if let Some(editor) = editor {
            execution::clear_installation_owner(&editor, session);
        }
        drop(buffer);
        drop(permit);
        return;
    }
    let Some(editor) = editor else {
        abort_installation(session, AbortDisposition::Dispose);
        return;
    };
    editor.imp().load.installation_incomplete.set(true);
    execution::schedule_install_slice(session);
}

/// Clear one bounded slice of a cancelled installation's partial content.
pub(super) fn run_cancelled_clear_slice(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    let (editor, buffer) = {
        let state = session.borrow();
        (state.editor.upgrade(), state.buffer.clone())
    };
    let Some(editor) = editor else {
        abort_installation(session, AbortDisposition::Dispose);
        return;
    };
    if !execution::delete_buffer_slice(&buffer) {
        execution::schedule_install_slice(session);
        return;
    }

    let (restore, permit, slice_count) = {
        let mut state = session.borrow_mut();
        if state.terminal || state.phase() != LoadInstallPhase::ClearingCancelled {
            return;
        }
        state.terminal = true;
        (state.restore, state.permit.take(), state.slice_count)
    };
    execution::clear_installation_owner(&editor, session);
    buffer.end_irreversible_action();
    buffer.set_modified(false);
    editor.imp().load.installation_slice_count.set(slice_count);
    execution::restore_load_installation_state(&editor, restore);
    if editor.imp().load.user_cancel_pending.replace(false) {
        finish_user_cancelled_load(&editor);
    } else {
        editor.refresh_accessibility_metadata();
    }
    let pending = admission::take_pending_request(&editor);
    drop(permit);
    if let Some(pending) = pending {
        admission::resume_pending_request(&editor, pending);
    }
}
