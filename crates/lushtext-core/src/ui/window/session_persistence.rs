// SPDX-License-Identifier: GPL-3.0-or-later

//! Session save/restore flows for the main window.
//!
//! This slice owns tab-state collection, debounced session persistence, and
//! startup restore orchestration. Draft-specific lifecycle work stays in
//! `drafts.rs`, even when restore needs to hand draft state across the split.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use crate::model::draft::PreloadedDraftRestore;
use crate::model::session::{SessionData, SessionTab};
use crate::services::notifications::NotificationSeverity;
use crate::services::recovery_metadata::{
    RecoveryDiagnostic, RecoveryPreservation, RecoveryProblem,
};
use crate::services::{draft_service, json_store, session_service};
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::plain_disposal::PROGRESS_DISPOSAL_RETAINED_BYTE_CAPACITY as STARTUP_PRELOAD_RESERVATION_BYTES;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;

use super::session_restore::{
    SessionRestoreAdmission, SessionRestorePlanPermit, SessionRestorePolicy, SessionRestoreRuntime,
};

struct GuardedStartupRestore {
    loaded: draft_service::RestoreState,
    preloaded: crate::ui::plain_disposal::DisposalOwned<HashMap<String, PreloadedDraftRestore>>,
}

fn startup_preloads_retained_bytes(preloaded: &HashMap<String, PreloadedDraftRestore>) -> u64 {
    let preload_bytes = preloaded
        .iter()
        .fold(0usize, |total, (id, restore)| {
            total
                .saturating_add(id.capacity())
                .saturating_add(match restore {
                    PreloadedDraftRestore::Content(content) => content.capacity(),
                    PreloadedDraftRestore::SkipStaleFile
                    | PreloadedDraftRestore::SkipOversized
                    | PreloadedDraftRestore::LazyAggregateBudget => 0,
                })
        })
        .saturating_add(preloaded.capacity().saturating_mul(
            std::mem::size_of::<(String, PreloadedDraftRestore)>().saturating_add(1),
        ));
    u64::try_from(
        std::mem::size_of::<HashMap<String, PreloadedDraftRestore>>().saturating_add(preload_bytes),
    )
    .unwrap_or(u64::MAX)
}

/// Demote eager bodies until the complete retained preload graph fits its permit.
///
/// A missing preload entry already falls back to the serialized lazy reader, so
/// clearing an unusually metadata-heavy map is safe and keeps release builds
/// from silently owning more memory than the progress lane accounted for.
fn fit_startup_preloads_to_reservation(
    preloaded: &mut HashMap<String, PreloadedDraftRestore>,
    retained_byte_limit: u64,
) -> u64 {
    let mut retained_bytes = startup_preloads_retained_bytes(preloaded);
    if retained_bytes <= retained_byte_limit {
        return retained_bytes;
    }

    for restore in preloaded.values_mut() {
        let PreloadedDraftRestore::Content(_) = restore else {
            continue;
        };
        let PreloadedDraftRestore::Content(content) =
            std::mem::replace(restore, PreloadedDraftRestore::LazyAggregateBudget)
        else {
            unreachable!("content match was checked before replacement");
        };
        retained_bytes =
            retained_bytes.saturating_sub(u64::try_from(content.capacity()).unwrap_or(u64::MAX));
        if retained_bytes <= retained_byte_limit {
            return retained_bytes;
        }
    }

    // A pathological count/key payload can outweigh the lane even after every
    // body became lazy. Dropping only the hints preserves the manifest and lets
    // each restored page take the normal bounded lazy path.
    *preloaded = HashMap::new();
    startup_preloads_retained_bytes(preloaded)
}

/// Scalar boundedness and lifecycle evidence exposed to the widget harness.
#[cfg(feature = "test-utils")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionRestoreRuntimeSnapshot {
    pub active: bool,
    pub scheduled_source: bool,
    pub projection_deferred: bool,
    pub generation: u64,
    pub total_descriptors: usize,
    pub pages_created: usize,
    pub gtk_turns: usize,
    pub max_pages_in_one_turn: usize,
    pub max_inflight_file_plans: usize,
    pub planning_terminals: usize,
    pub pending_descriptors: usize,
    pub active_file_plans: usize,
    pub terminal_projection_publications: usize,
    pub aggregate_projection_publications: u64,
    pub cancelled: bool,
}

impl super::LushtextWindow {
    /// Whether persisted descriptors are not yet available to a close snapshot.
    fn startup_session_descriptors_pending(&self) -> bool {
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
        let mut identity_indices = index_session_tabs(&session.tabs);
        let mut pending_indices = HashMap::new();
        session.tabs.reserve(pending.len());
        for (ordinal, descriptor) in pending {
            let index =
                merge_session_tab(&mut session.tabs, &mut identity_indices, descriptor, false);
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

    /// Load the session file plus draft restore state in one background task.
    pub fn load_session_and_drafts(&self) {
        self.imp().session.restoring.set(true);
        self.imp().session.restore_capacity_wakeup.cancel();
        if let Some(previous) = self.imp().session.restore_cancel.take() {
            previous.store(true, Ordering::Release);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *self.imp().session.restore_cancel.borrow_mut() = Some(cancel.clone());
        self.start_session_and_drafts_load(cancel);
    }

    fn start_session_and_drafts_load(&self, cancel: Arc<AtomicBool>) {
        if cancel.load(Ordering::Acquire)
            || !self
                .imp()
                .session
                .restore_cancel
                .borrow()
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &cancel))
        {
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
                        window.start_session_and_drafts_load(retry_cancel);
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
                let retained_bytes = fit_startup_preloads_to_reservation(
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
                if cancel.load(Ordering::Acquire)
                    || !window
                        .imp()
                        .session
                        .restore_cancel
                        .borrow()
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, &cancel))
                {
                    return;
                }
                window.imp().session.restore_cancel.borrow_mut().take();
                let GuardedStartupRestore { loaded, preloaded } = guarded;
                *window.imp().drafts.manifest.borrow_mut() = loaded.manifest;
                window
                    .imp()
                    .drafts
                    .manifest_authority
                    .set(loaded.manifest_authority);
                *window.imp().drafts.preloaded.borrow_mut() = preloaded;
                for diagnostic in &loaded.diagnostics {
                    tracing::warn!("{}", diagnostic.summary());
                }
                window
                    .start_session_restore(loaded.session, loaded.manifest_authority.is_trusted());
                window.publish_startup_recovery_diagnostics(&loaded.diagnostics);
            },
        );
    }

    /// Start one bounded multi-turn restore generation from compact descriptors.
    fn start_session_restore(&self, session: SessionData, cleanup_allowed: bool) {
        self.cancel_session_restore_runtime(false);
        if session.tabs.is_empty() {
            self.imp().session.restoring.set(false);
            self.schedule_orphan_cleanup(cleanup_allowed);
            return;
        }

        let state = &self.imp().session;
        let generation = state.next_restore_generation.get().wrapping_add(1);
        state.next_restore_generation.set(generation);
        let preserve_existing_selection = self.imp().tab_view.n_pages() > 0;
        let selected_before = self
            .imp()
            .tab_view
            .selected_page()
            .map(|page| page.downgrade());
        let policy = SessionRestorePolicy::new(generation, session.tabs, session.active_tab_index);
        let selection_generation = state.selection_generation.get();

        state.restoring.set(true);
        self.begin_tab_projection_refresh_batch();
        state
            .restore_runtime
            .replace(Some(SessionRestoreRuntime::new(
                policy,
                preserve_existing_selection,
                selected_before,
                cleanup_allowed,
                selection_generation,
            )));
        self.schedule_session_restore_turn(generation);
    }

    fn schedule_session_restore_turn(&self, generation: u64) {
        let mut runtime = self.imp().session.restore_runtime.borrow_mut();
        let Some(runtime) = runtime.as_mut() else {
            return;
        };
        if runtime.policy.generation() != generation
            || runtime.policy.is_terminal()
            || runtime.scheduled_source.is_some()
        {
            return;
        }

        let window_weak = self.downgrade();
        let source_id = glib::idle_add_local_once(move || {
            if let Some(window) = window_weak.upgrade() {
                window.run_scheduled_session_restore_turn(generation);
            }
        });
        runtime.scheduled_source = Some(source_id);
    }

    fn run_scheduled_session_restore_turn(&self, generation: u64) {
        let newer_selection = {
            let runtime = self.imp().session.restore_runtime.borrow();
            runtime.as_ref().and_then(|runtime| {
                (runtime.policy.generation() == generation
                    && runtime.selection_generation
                        != self.imp().session.selection_generation.get())
                .then(|| self.imp().tab_view.selected_page())
                .flatten()
            })
        };
        let turn = {
            let mut runtime = self.imp().session.restore_runtime.borrow_mut();
            let Some(runtime) = runtime.as_mut() else {
                return;
            };
            if runtime.policy.generation() != generation {
                return;
            }
            // A fired one-shot source no longer exists; take its ID without
            // calling `remove()` so cancellation cannot double-remove it.
            runtime.scheduled_source.take();
            runtime.policy.plan_turn()
        };

        let mut inline_releases = Vec::new();
        for admission in turn.admissions {
            if let Some(permit) = self.create_session_restore_page(generation, admission) {
                inline_releases.push(permit);
            }
        }
        if let Some(page) = newer_selection {
            self.set_restore_selected_page(&page);
        } else {
            self.restore_preexisting_selection(generation);
        }

        let (terminal, needs_next_turn) = {
            let mut runtime = self.imp().session.restore_runtime.borrow_mut();
            let Some(runtime) = runtime.as_mut() else {
                return;
            };
            if runtime.policy.generation() != generation {
                return;
            }
            for permit in inline_releases {
                let released = runtime.policy.release_permit(permit);
                debug_assert!(released, "inline restore permit must release once");
            }
            (
                runtime.policy.is_terminal(),
                runtime.policy.needs_next_turn(),
            )
        };
        if terminal {
            self.finish_session_restore(generation);
        } else if needs_next_turn {
            self.schedule_session_restore_turn(generation);
        }
    }

    /// Create one restored page. Returning a permit means no file planning was
    /// started (for example, the path already had a live page), so the caller
    /// can release it after the current bounded turn finishes.
    fn create_session_restore_page(
        &self,
        generation: u64,
        admission: SessionRestoreAdmission,
    ) -> Option<SessionRestorePlanPermit> {
        let SessionRestoreAdmission {
            ordinal,
            tab,
            permit,
        } = admission;
        let mut inline_release = None;
        self.imp().session.applying_restore_selection.set(true);
        let page = if let Some(path) = tab.path.as_deref() {
            let permit = permit.expect("file-backed restore admission owns a planning permit");
            let window_weak = self.downgrade();
            let opened = self.open_document_from_session_restore(path, move || {
                if let Some(window) = window_weak.upgrade() {
                    window.release_session_restore_plan_permit(permit);
                }
            });
            match opened {
                Some((page, true)) => Some(page),
                Some((page, false)) => {
                    inline_release = Some(permit);
                    Some(page)
                }
                None => {
                    inline_release = Some(permit);
                    None
                }
            }
        } else {
            debug_assert!(permit.is_none());
            self.new_tab();
            self.imp().tab_view.selected_page()
        };
        self.imp().session.applying_restore_selection.set(false);

        if let Some(page) = page {
            self.restore_tab_pinned_state(&page, tab.pinned);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                editor.set_restore_position(tab.cursor_line, tab.cursor_col, tab.scroll_line);
                if tab.path.is_none()
                    && let Some(draft_id) = tab.draft_id.as_deref()
                {
                    editor.set_draft_id(draft_id.to_string());
                    self.check_draft_by_id(editor, draft_id);
                }
            }
            let mut runtime = self.imp().session.restore_runtime.borrow_mut();
            if let Some(runtime) = runtime.as_mut()
                && runtime.policy.generation() == generation
                && runtime.policy.requested_active_ordinal() == Some(ordinal)
            {
                runtime.requested_page = Some(page.downgrade());
            }
        }

        inline_release
    }

    fn release_session_restore_plan_permit(&self, permit: SessionRestorePlanPermit) {
        let (terminal, needs_next_turn) = {
            let mut runtime = self.imp().session.restore_runtime.borrow_mut();
            let Some(runtime) = runtime.as_mut() else {
                return;
            };
            if runtime.policy.generation() != permit.generation()
                || !runtime.policy.release_permit(permit)
            {
                return;
            }
            (
                runtime.policy.is_terminal(),
                runtime.policy.needs_next_turn(),
            )
        };
        if terminal {
            self.finish_session_restore(permit.generation());
        } else if needs_next_turn {
            self.schedule_session_restore_turn(permit.generation());
        }
    }

    fn restore_preexisting_selection(&self, generation: u64) {
        let selected = {
            let runtime = self.imp().session.restore_runtime.borrow();
            runtime.as_ref().and_then(|runtime| {
                (runtime.policy.generation() == generation
                    && runtime.preserve_existing_selection
                    && runtime.selection_generation
                        == self.imp().session.selection_generation.get())
                .then(|| {
                    runtime
                        .selected_before
                        .as_ref()
                        .and_then(glib::WeakRef::upgrade)
                })
                .flatten()
            })
        };
        if let Some(page) = selected {
            self.set_restore_selected_page(&page);
        }
    }

    fn set_restore_selected_page(&self, page: &libadwaita::TabPage) {
        self.imp().session.applying_restore_selection.set(true);
        self.imp().tab_view.set_selected_page(page);
        self.imp().session.applying_restore_selection.set(false);
    }

    fn finish_session_restore(&self, generation: u64) {
        let Some(mut runtime) = self.imp().session.restore_runtime.take() else {
            return;
        };
        if runtime.policy.generation() != generation {
            self.imp().session.restore_runtime.replace(Some(runtime));
            return;
        }
        if let Some(source_id) = runtime.scheduled_source.take() {
            source_id.remove();
        }

        let selection_intent_is_current =
            runtime.selection_generation == self.imp().session.selection_generation.get();
        if selection_intent_is_current && runtime.preserve_existing_selection {
            if let Some(page) = runtime
                .selected_before
                .as_ref()
                .and_then(glib::WeakRef::upgrade)
            {
                self.set_restore_selected_page(&page);
            }
        } else if selection_intent_is_current
            && let Some(ordinal) = runtime.policy.requested_active_ordinal()
        {
            let requested = runtime
                .requested_page
                .as_ref()
                .and_then(glib::WeakRef::upgrade);
            if let Some(page) = requested {
                self.set_restore_selected_page(&page);
            } else if self.imp().tab_view.n_pages() > 0 {
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "AdwTabView page counts are non-negative"
                )]
                let ordinal = ordinal.min(self.imp().tab_view.n_pages() as usize - 1);
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "Persisted tab counts remain well below i32::MAX"
                )]
                let page = self.imp().tab_view.nth_page(ordinal as i32);
                self.set_restore_selected_page(&page);
            }
        }

        let first_terminal_projection = runtime.policy.note_terminal_projection_publication();
        debug_assert!(
            first_terminal_projection,
            "current restore generation publishes one terminal projection"
        );
        let evidence = runtime.policy.evidence();
        let cleanup_allowed = runtime.cleanup_allowed_on_terminal;
        self.imp().session.restoring.set(false);
        if runtime.projection_batch_owned {
            runtime.projection_batch_owned = false;
            self.end_tab_projection_refresh_batch();
        }
        self.imp().session.last_restore_evidence.set(Some(evidence));
        self.schedule_orphan_cleanup(cleanup_allowed);
    }

    fn cancel_session_restore_runtime(&self, publish_projection: bool) {
        let Some(mut runtime) = self.imp().session.restore_runtime.take() else {
            return;
        };
        if let Some(source_id) = runtime.scheduled_source.take() {
            source_id.remove();
        }
        runtime.policy.cancel();
        let evidence = runtime.policy.evidence();
        self.imp().session.restoring.set(false);
        if runtime.projection_batch_owned {
            runtime.projection_batch_owned = false;
            if publish_projection {
                self.end_tab_projection_refresh_batch();
            } else {
                self.cancel_tab_projection_refresh_batch();
            }
        }
        self.imp().session.last_restore_evidence.set(Some(evidence));
    }

    /// Cancel scheduled restore ownership without publishing widgets during dispose.
    pub(super) fn cancel_session_restore_for_dispose(&self) {
        self.cancel_session_restore_runtime(false);
    }

    /// Start a supplied restore generation through production admission policy.
    #[cfg(feature = "test-utils")]
    pub fn restore_session_for_test(&self, session: SessionData) {
        self.start_session_restore(session, false);
    }

    /// Whether close persistence must preserve the prior session file because
    /// startup has not yet published compact restore descriptors.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn startup_session_descriptors_pending_for_test(&self) -> bool {
        self.startup_session_descriptors_pending()
    }

    /// Cancel the active generation through the production ownership path.
    #[cfg(feature = "test-utils")]
    pub fn cancel_session_restore_for_test(&self) {
        self.cancel_session_restore_runtime(false);
    }

    /// Return content-free policy and source ownership evidence.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn session_restore_runtime_snapshot_for_test(&self) -> SessionRestoreRuntimeSnapshot {
        let runtime = self.imp().session.restore_runtime.borrow();
        let (active, scheduled_source, evidence) = runtime.as_ref().map_or_else(
            || {
                (
                    false,
                    false,
                    self.imp()
                        .session
                        .last_restore_evidence
                        .get()
                        .unwrap_or_default(),
                )
            },
            |runtime| {
                (
                    true,
                    runtime.scheduled_source.is_some(),
                    runtime.policy.evidence(),
                )
            },
        );
        SessionRestoreRuntimeSnapshot {
            active,
            scheduled_source,
            projection_deferred: self.tab_projection_refresh_deferred(),
            generation: evidence.generation,
            total_descriptors: evidence.total_descriptors,
            pages_created: evidence.pages_created,
            gtk_turns: evidence.gtk_turns,
            max_pages_in_one_turn: evidence.max_pages_in_one_turn,
            max_inflight_file_plans: evidence.max_inflight_file_plans,
            planning_terminals: evidence.planning_terminals,
            pending_descriptors: evidence.pending_descriptors,
            active_file_plans: evidence.active_file_plans,
            terminal_projection_publications: evidence.terminal_projection_publications,
            aggregate_projection_publications: self.imp().session.tab_projection_publications.get(),
            cancelled: evidence.cancelled,
        }
    }

    fn publish_startup_recovery_diagnostics(&self, diagnostics: &[RecoveryDiagnostic]) {
        if diagnostics.is_empty() {
            return;
        }
        let message = startup_recovery_status_message(diagnostics);
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum SessionTabIdentity {
    Path(PathBuf),
    Draft(String),
}

fn session_tab_identity(tab: &SessionTab) -> Option<SessionTabIdentity> {
    tab.path.clone().map(SessionTabIdentity::Path).or_else(|| {
        tab.draft_id
            .as_ref()
            .filter(|draft_id| !draft_id.is_empty())
            .cloned()
            .map(SessionTabIdentity::Draft)
    })
}

fn index_session_tabs(tabs: &[SessionTab]) -> HashMap<SessionTabIdentity, usize> {
    let mut indices = HashMap::with_capacity(tabs.len());
    for (index, tab) in tabs.iter().enumerate() {
        if let Some(identity) = session_tab_identity(tab) {
            indices.entry(identity).or_insert(index);
        }
    }
    indices
}

fn merge_session_tab(
    tabs: &mut Vec<SessionTab>,
    indices: &mut HashMap<SessionTabIdentity, usize>,
    tab: SessionTab,
    replace_existing: bool,
) -> usize {
    let identity = session_tab_identity(&tab);
    if let Some(index) = identity
        .as_ref()
        .and_then(|identity| indices.get(identity).copied())
    {
        if replace_existing {
            tabs[index] = tab;
        }
        return index;
    }

    let index = tabs.len();
    tabs.push(tab);
    if let Some(identity) = identity {
        indices.insert(identity, index);
    }
    index
}

/// Preserve not-yet-loaded descriptors while layering current pages over them.
fn merge_persisted_session_with_current(
    mut persisted: SessionData,
    current: SessionData,
) -> SessionData {
    let mut indices = index_session_tabs(&persisted.tabs);
    persisted.tabs.reserve(current.tabs.len());
    let mut current_active_index = None;
    for (current_index, tab) in current.tabs.into_iter().enumerate() {
        let merged_index = merge_session_tab(&mut persisted.tabs, &mut indices, tab, true);
        if current.active_tab_index == Some(current_index) {
            current_active_index = Some(merged_index);
        }
    }
    persisted.active_tab_index = current_active_index.or_else(|| {
        persisted
            .active_tab_index
            .filter(|index| *index < persisted.tabs.len())
    });
    persisted
}

/// Load not-yet-published startup descriptors without discarding preservation authority.
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
    Ok(merge_persisted_session_with_current(
        persisted.value,
        current,
    ))
}

fn startup_recovery_status_message(diagnostics: &[RecoveryDiagnostic]) -> String {
    let damaged = diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.problem,
                RecoveryProblem::Malformed { .. }
                    | RecoveryProblem::UnsupportedFormat { .. }
                    | RecoveryProblem::UnsupportedVersion { .. }
                    | RecoveryProblem::Unreadable { .. }
                    | RecoveryProblem::UnsupportedFileKind { .. }
                    | RecoveryProblem::Oversized { .. }
            )
        })
        .count();
    let repaired = diagnostics
        .iter()
        .filter(|diagnostic| matches!(diagnostic.problem, RecoveryProblem::Repaired { .. }))
        .count();
    let skipped = diagnostics
        .iter()
        .filter(|diagnostic| matches!(diagnostic.problem, RecoveryProblem::RepairSkipped { .. }))
        .count();
    let preserved = diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.preservation,
                RecoveryPreservation::Quarantined { .. }
                    | RecoveryPreservation::CopiedToQuarantine { .. }
                    | RecoveryPreservation::PreservedInPlace
            )
        })
        .count();

    match (damaged > 0, repaired > 0, skipped > 0, preserved > 0) {
        (true, true, _, true) => format!(
            "Some recovery data was repaired; {damaged} issue(s) were preserved for inspection"
        ),
        (true, false, true, true) => format!(
            "Some recovery data could not be loaded; {damaged} issue(s) were preserved for inspection"
        ),
        (true, _, _, _) => {
            format!("Some recovery data could not be loaded ({damaged} issue(s))")
        }
        (false, true, true, _) => {
            "Some recovery data was partially repaired; other items were preserved".to_string()
        }
        (false, true, false, _) => "Some recovery data was repaired".to_string(),
        (false, false, true, _) => {
            "Some recovery data could not be repaired automatically".to_string()
        }
        (false, false, false, _) => "Recovery data changed during startup".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::services::recovery_metadata::{RecoveryMetadataClass, RecoveryPreservation};

    fn session_tab(
        path: Option<PathBuf>,
        draft_id: Option<String>,
        cursor_line: u32,
    ) -> SessionTab {
        SessionTab {
            path,
            draft_id,
            cursor_line,
            cursor_col: 0,
            scroll_line: 0,
            pinned: false,
        }
    }

    #[test]
    fn preload_graph_demotes_bodies_and_clears_metadata_when_needed() {
        let mut preloaded = HashMap::from([
            (
                "first".to_string(),
                PreloadedDraftRestore::Content(String::with_capacity(2_048)),
            ),
            (
                "second".to_string(),
                PreloadedDraftRestore::Content(String::with_capacity(2_048)),
            ),
        ]);
        let original = startup_preloads_retained_bytes(&preloaded);
        let body_limited = original.saturating_sub(2_048);

        let retained = fit_startup_preloads_to_reservation(&mut preloaded, body_limited);

        assert!(retained <= body_limited);
        assert!(
            preloaded
                .values()
                .any(|restore| { matches!(restore, PreloadedDraftRestore::LazyAggregateBudget) })
        );

        let metadata_only_limit =
            u64::try_from(std::mem::size_of::<HashMap<String, PreloadedDraftRestore>>())
                .expect("HashMap shell fits u64");
        let retained = fit_startup_preloads_to_reservation(&mut preloaded, metadata_only_limit);
        assert!(preloaded.is_empty());
        assert!(retained <= metadata_only_limit);
    }

    #[test]
    fn session_merge_indexes_large_descriptor_sets_and_overlays_current_pages() {
        let persisted_tabs = (0..20_000)
            .map(|index| {
                session_tab(
                    Some(PathBuf::from(format!("/persisted/{index}.txt"))),
                    None,
                    0,
                )
            })
            .collect::<Vec<_>>();
        let mut current_tabs = (0..10_000)
            .map(|index| {
                session_tab(
                    Some(PathBuf::from(format!("/persisted/{index}.txt"))),
                    None,
                    7,
                )
            })
            .collect::<Vec<_>>();
        current_tabs.push(session_tab(None, Some("new-untitled".to_string()), 11));

        let merged = merge_persisted_session_with_current(
            SessionData {
                tabs: persisted_tabs,
                active_tab_index: Some(19_999),
            },
            SessionData {
                tabs: current_tabs,
                active_tab_index: Some(10_000),
            },
        );

        assert_eq!(merged.tabs.len(), 20_001);
        assert_eq!(merged.tabs[9_999].cursor_line, 7);
        assert_eq!(merged.tabs[19_999].cursor_line, 0);
        assert_eq!(
            merged.tabs[20_000].draft_id.as_deref(),
            Some("new-untitled")
        );
        assert_eq!(merged.active_tab_index, Some(20_000));
    }

    #[test]
    fn startup_recovery_status_groups_damage_and_repair() {
        let diagnostics = vec![
            RecoveryDiagnostic::with_preservation(
                RecoveryMetadataClass::DraftManifest,
                PathBuf::from("/tmp/manifest.json"),
                RecoveryProblem::Malformed {
                    detail: "bad JSON".to_string(),
                },
                RecoveryPreservation::Quarantined {
                    path: PathBuf::from("/tmp/quarantine/manifest.json"),
                },
            ),
            RecoveryDiagnostic::repaired(
                RecoveryMetadataClass::DraftManifest,
                PathBuf::from("/tmp/manifest.json"),
                "rebuilt one draft",
            ),
        ];

        let message = startup_recovery_status_message(&diagnostics);

        assert!(message.contains("repaired"));
        assert!(message.contains("preserved"));
    }

    #[test]
    fn startup_recovery_status_mentions_unrepaired_items() {
        let diagnostics = vec![RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::DraftManifest,
            PathBuf::from("/tmp/manifest.json"),
            "ambiguous draft",
        )];

        let message = startup_recovery_status_message(&diagnostics);

        assert!(message.contains("could not be repaired"));
    }
}
