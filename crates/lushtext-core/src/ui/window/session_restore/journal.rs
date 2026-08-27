// SPDX-License-Identifier: GPL-3.0-or-later

//! The session file: the durable record the next startup restores from.
//!
//! `journal` rather than `admission` or `execution`, on slot 3a's reusable test —
//! *does a later stage of the same workflow restore from the record* — which the
//! session file passes twice over: the next launch reads it back through
//! `load_session_and_drafts`, and **within one run** a close reads it back so a
//! restore that never finished cannot delete the descriptors it had not reached.
//!
//! Per slot 2b's definition, the record's own mutual-exclusion gate lives here
//! rather than in a separate role: that gate is the `save_debounce` generation,
//! which decides both which write wins and whether a late success may clear a
//! newer failure.

use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::session::{SessionData, SessionTab};
use crate::services::notifications::NotificationSeverity;
use crate::services::recovery_metadata::RecoveryDiagnostic;
use crate::services::{draft_service, json_store, session_service};
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::plain_disposal::PROGRESS_DISPOSAL_RETAINED_BYTE_CAPACITY as STARTUP_PRELOAD_RESERVATION_BYTES;

use super::policy;
use crate::ui::window::LushtextWindow;

/// The startup read's two payloads, kept together so the guarded preload graph
/// cannot be separated from the manifest it belongs to.
struct GuardedStartupRestore {
    loaded: draft_service::RestoreState,
    preloaded: crate::ui::plain_disposal::DisposalOwned<
        std::collections::HashMap<String, crate::model::draft::PreloadedDraftRestore>,
    >,
}

impl LushtextWindow {
    /// Whether persisted descriptors are not yet available to a close snapshot.
    ///
    /// True until the startup gate has run **and** a restore generation has
    /// published its compact descriptors. While it holds, a close must merge the
    /// persisted file rather than overwrite it.
    pub(super) fn startup_session_descriptors_pending(&self) -> bool {
        !self.imp().startup_data_flow.completed.get()
            || (self.imp().session.restoring.get()
                && self.imp().session.restore_runtime.borrow().is_none())
    }

    /// Snapshot current tab state into one persisted `SessionData` value object.
    #[must_use]
    #[expect(
        clippy::cast_sign_loss,
        reason = "AdwTabView page indices are non-negative when a tab exists"
    )]
    pub fn collect_session(&self) -> SessionData {
        let tab_view = &self.imp().tab_view;
        let mut tabs = Vec::with_capacity(tab_view.n_pages() as usize);

        let selected = tab_view.selected_page();
        let mut active_tab_index = None;

        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let (cursor_line, cursor_col) = editor.cursor_position();
                // Failed file-backed placeholders retain their original path so
                // temporary mount or permission failures remain retryable across
                // another session instead of being serialized as untitled tabs.
                let path = editor.file_path();
                let draft_id = if path.is_none() {
                    editor.draft_id()
                } else {
                    None
                };
                tabs.push(SessionTab {
                    path,
                    draft_id,
                    cursor_line,
                    cursor_col,
                    scroll_line: editor.visible_top_line(),
                    pinned: page.is_pinned(),
                });
                if selected.as_ref() == Some(&page) {
                    active_tab_index = Some(i as usize);
                }
            }
        }

        SessionData {
            tabs,
            active_tab_index,
        }
    }

    /// Snapshot mounted pages plus descriptors not yet admitted by progressive restore.
    fn collect_session_for_close(&self) -> SessionData {
        let mut session = self.collect_session();
        let (pending, requested_ordinal, requested_page, preserve_existing, intent_changed) = {
            let runtime = self.imp().session.restore_runtime.borrow();
            let Some(runtime) = runtime.as_ref() else {
                return session;
            };
            (
                runtime.policy.pending_descriptors(),
                runtime.policy.requested_active_ordinal(),
                runtime
                    .requested_page
                    .as_ref()
                    .and_then(glib::WeakRef::upgrade),
                runtime.preserve_existing_selection,
                runtime.selection_generation != self.imp().session.selection_generation.get(),
            )
        };

        let requested_page_index = requested_page.as_ref().and_then(|requested| {
            (0..self.imp().tab_view.n_pages())
                .find(|index| self.imp().tab_view.nth_page(*index) == *requested)
                .and_then(|index| usize::try_from(index).ok())
        });
        let mut identity_indices = policy::index_session_tabs(&session.tabs);
        let mut pending_indices = std::collections::HashMap::new();
        session.tabs.reserve(pending.len());
        for (ordinal, descriptor) in pending {
            let index = policy::merge_session_tab(
                &mut session.tabs,
                &mut identity_indices,
                descriptor,
                false,
            );
            pending_indices.insert(ordinal, index);
        }
        if !intent_changed && !preserve_existing {
            session.active_tab_index = requested_page_index.or_else(|| {
                requested_ordinal.and_then(|ordinal| pending_indices.get(&ordinal).copied())
            });
        }
        session
    }

    /// Save session with a 500ms debounce. No-op during session restore.
    pub fn save_session_debounced(&self) {
        if self.imp().session.restoring.get() {
            return;
        }

        self.imp().session.save_debounce.schedule(
            self,
            Duration::from_millis(500),
            move |window, token| {
                if window.imp().session.restoring.get()
                    || window.startup_session_descriptors_pending()
                {
                    return;
                }
                let generation = token.value();
                let session = window.collect_session();
                let data_dir = json_store::data_dir();
                let ordered_generation = u64::from(generation);
                spawn_blocking_then(
                    window,
                    move || session_service::save_ordered(&data_dir, &session, ordered_generation),
                    move |window, result| match result {
                        Ok(true) => window.clear_session_save_failure(generation),
                        Ok(false) => {}
                        Err(error) => {
                            tracing::error!("Failed to save session: {error}");
                            let detail = error.to_string();
                            window.record_session_save_failure(generation, &detail, true);
                        }
                    },
                );
            },
        );
    }

    /// Synchronous session save for the close-request path.
    pub fn save_session_sync(&self) {
        let generation = self.imp().session.save_debounce.advance().value();
        let data_dir = json_store::data_dir();
        let session = if self.startup_session_descriptors_pending() {
            load_and_merge_persisted_session_for_close(&data_dir, self.collect_session())
        } else {
            Ok(self.collect_session_for_close())
        };
        match session.and_then(|session| {
            session_service::save_ordered(&data_dir, &session, u64::from(generation))
        }) {
            Ok(true) => self.clear_session_save_failure(generation),
            Ok(false) => {}
            Err(error) => {
                tracing::error!("Failed to save session on close: {error}");
                let detail = error.to_string();
                self.record_session_save_failure(generation, &detail, true);
            }
        }
    }

    /// Save session for close on a background worker, then report whether close may continue.
    pub fn save_session_for_close_async<F: FnOnce(anyhow::Result<()>) + 'static>(
        &self,
        on_done: F,
    ) {
        let generation = self.imp().session.save_debounce.advance().value();
        let descriptors_pending = self.startup_session_descriptors_pending();
        let session = if descriptors_pending {
            self.collect_session()
        } else {
            self.collect_session_for_close()
        };
        let data_dir = json_store::data_dir();
        spawn_blocking_then(
            self.clone(),
            move || {
                let session = if descriptors_pending {
                    load_and_merge_persisted_session_for_close(&data_dir, session)?
                } else {
                    session
                };
                session_service::save_ordered(&data_dir, &session, u64::from(generation))
            },
            move |window, result| {
                let close_result = match result {
                    Ok(true) => {
                        window.clear_session_save_failure(generation);
                        Ok(())
                    }
                    Ok(false) => Ok(()),
                    Err(error) => {
                        tracing::error!("Failed to save session on close: {error}");
                        let detail = error.to_string();
                        window.record_session_save_failure(generation, &detail, true);
                        Err(error)
                    }
                };
                on_done(close_result);
            },
        );
    }

    /// Read the journal back at startup, together with the draft records.
    ///
    /// The two are read in one worker pass because the session descriptors and
    /// the draft manifest have to agree: a descriptor whose draft is gone must
    /// not restore as if its content still existed.
    pub fn load_session_and_drafts(&self) {
        self.imp().session.restoring.set(true);
        self.imp().session.restore_capacity_wakeup.cancel();
        if let Some(previous) = self.imp().session.restore_cancel.take() {
            previous.store(true, Ordering::Release);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *self.imp().session.restore_cancel.borrow_mut() = Some(cancel.clone());
        self.start_startup_journal_read(cancel);
    }

    /// Whether one startup journal read has been cancelled or replaced.
    ///
    /// The read is guarded twice — once before it dispatches and once when its
    /// worker returns — and both ask exactly this question, so the identity
    /// comparison lives in one place. Two conditions, in this order: the token
    /// itself was cancelled, or the window's live token is no longer the same
    /// `Arc`, meaning a newer read took ownership.
    fn startup_journal_read_superseded(&self, cancel: &Arc<AtomicBool>) -> bool {
        cancel.load(Ordering::Acquire)
            || !self
                .imp()
                .session
                .restore_cancel
                .borrow()
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, cancel))
    }

    fn start_startup_journal_read(&self, cancel: Arc<AtomicBool>) {
        if self.startup_journal_read_superseded(&cancel) {
            return;
        }
        let observed_epoch = crate::ui::plain_disposal::progress_disposal_capacity_epoch();
        let Some(reservation) = crate::ui::plain_disposal::try_reserve_progress_for_gtk(
            STARTUP_PRELOAD_RESERVATION_BYTES,
        ) else {
            let window_weak = self.downgrade();
            let retry_cancel = Arc::clone(&cancel);
            self.imp()
                .session
                .restore_capacity_wakeup
                .arm(observed_epoch, move || {
                    if let Some(window) = window_weak.upgrade() {
                        window.start_startup_journal_read(retry_cancel);
                    }
                });
            return;
        };
        let data_dir = json_store::data_dir();
        let worker_cancel = cancel.clone();
        spawn_blocking_then(
            self.clone(),
            move || {
                let mut loaded =
                    draft_service::load_restore_state_cancellable(&data_dir, &worker_cancel);
                let mut preloaded = std::mem::take(&mut loaded.preloaded_drafts);
                let retained_bytes = policy::fit_startup_preloads_to_reservation(
                    &mut preloaded,
                    STARTUP_PRELOAD_RESERVATION_BYTES,
                );
                debug_assert!(retained_bytes <= STARTUP_PRELOAD_RESERVATION_BYTES);
                let mut reservation = reservation;
                reservation.shrink_to(retained_bytes);
                GuardedStartupRestore {
                    loaded,
                    preloaded: reservation.own(preloaded),
                }
            },
            move |window, guarded| {
                if window.startup_journal_read_superseded(&cancel) {
                    return;
                }
                window.imp().session.restore_cancel.borrow_mut().take();
                let GuardedStartupRestore { loaded, preloaded } = guarded;
                let cleanup_allowed = loaded.manifest_authority.is_trusted();
                // The draft records belong to the draft-recovery workflow, so
                // they are handed over through its own named operation rather
                // than written field by field from here.
                window.adopt_startup_draft_records(
                    loaded.manifest,
                    loaded.manifest_authority,
                    preloaded,
                );
                for diagnostic in &loaded.diagnostics {
                    tracing::warn!("{}", diagnostic.summary());
                }
                window.begin_session_restore(loaded.session, cleanup_allowed);
                window.publish_startup_recovery_diagnostics(&loaded.diagnostics);
            },
        );
    }

    fn publish_startup_recovery_diagnostics(&self, diagnostics: &[RecoveryDiagnostic]) {
        if diagnostics.is_empty() {
            return;
        }
        let message = policy::startup_recovery_status_message(diagnostics);
        self.publish_status_message(&message, NotificationSeverity::Warning);
    }

    fn record_session_save_failure(&self, generation: u32, detail: &str, visible: bool) {
        let session = &self.imp().session;
        session.save_failed.set(true);
        session.failed_generation.set(generation);
        *session.failure_detail.borrow_mut() = Some(detail.to_string());
        if visible {
            self.publish_status_message(
                &format!("Session layout may not restore: {detail}"),
                NotificationSeverity::Warning,
            );
        }
    }

    fn clear_session_save_failure(&self, generation: u32) {
        let session = &self.imp().session;
        // A late successful save must not clear a newer failure banner, so only
        // the same or newer generation may mark the session state healthy again.
        if session.save_failed.get() && generation >= session.failed_generation.get() {
            session.save_failed.set(false);
            session.failed_generation.set(0);
            session.failure_detail.borrow_mut().take();
        }
    }
}

/// Load not-yet-published startup descriptors without discarding preservation authority.
///
/// Refusing the close outright when recovery evidence cannot be preserved is
/// deliberate: overwriting a session file whose damage has not been quarantined
/// would destroy the only copy of the user's layout.
fn load_and_merge_persisted_session_for_close(
    data_dir: &Path,
    current: SessionData,
) -> anyhow::Result<SessionData> {
    let persisted = session_service::load_recovering(data_dir);
    if !persisted.replacement_allowed() {
        let detail = persisted
            .diagnostics
            .iter()
            .map(RecoveryDiagnostic::summary)
            .collect::<Vec<_>>()
            .join("; ");
        anyhow::bail!(
            "session recovery evidence could not be preserved safely; close was cancelled: {detail}"
        );
    }
    Ok(policy::merge_persisted_session_with_current(
        persisted.value,
        current,
    ))
}
