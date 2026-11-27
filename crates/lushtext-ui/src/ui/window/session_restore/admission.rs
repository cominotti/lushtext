// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded admission for one session-restore generation.
//!
//! *Reserve then settle*, exactly: each turn admits at most `pages_per_turn`
//! descriptors, a file-backed one reserves a `SessionRestorePlanPermit` up front,
//! and the permit is released when that document's background planning reaches a
//! terminal. `release_permit` counts exactly those releases to decide when the
//! next document may open — which is why **every** load terminal must either
//! carry a parked request's planning owner into a restart or release it, and why
//! no path may drop one.
//!
//! This module owns the GTK half only: the source IDs, the weak tab-page
//! handles, and the turn re-arming. Every decision about *how many* and *which*
//! belongs to [`super::policy`].

use std::cell::{Cell, RefCell};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_settle::Debounce;
use gtk4::glib;
use gtk4::prelude::*;

use lushtext_model::persistence::session::SessionData;
use lushtext_model::values::{FailedRestoreGeneration, RestoreGeneration, SelectionGeneration};

use super::policy::{SessionRestorePlanPermit, SessionRestorePolicy, SessionRestoreTurnMetrics};
use super::{evidence, execution};
use crate::ui::window::LushtextWindow;

/// Algebraic lifecycle phase for startup session read and bounded restore.
#[derive(Default)]
pub(in crate::ui::window) enum SessionRestorePhase {
    #[default]
    Idle,
    /// Startup manifest/session recovery read is running off GTK.
    ReadingStartup { cancel: Arc<AtomicBool> },
    /// Startup read finished and is waiting for queued activation opens.
    AwaitingActivationOpens,
    /// Bounded multi-turn GTK page mounting is active.
    Restoring { runtime: Box<SessionRestoreRuntime> },
}

/// Session-file persistence failure state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::ui::window) enum SessionSaveFailurePhase {
    #[default]
    Healthy,
    Failed {
        generation: FailedRestoreGeneration,
        detail: String,
    },
}

/// Window-close safety-check lifecycle shared with draft flush.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui::window) enum CloseSafetyPhase {
    #[default]
    Idle,
    Inflight,
    BypassPending,
}

/// Active sequential multi-editor close-save session identity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui::window) enum CloseSaveSessionPhase {
    #[default]
    Idle,
    Active(u64),
}

/// Session-persistence state for the main window shell.
#[derive(Default)]
pub struct SessionState {
    /// Debounce for session saves (500ms) and ordered-save freshness.
    pub(super) save_debounce: Debounce,
    /// Lifecycle of startup read and bounded GTK restore.
    pub(super) restore_phase: RefCell<SessionRestorePhase>,
    /// One paced admission wakeup before startup recovery I/O begins.
    pub(super) restore_capacity_wakeup: crate::ui::plain_disposal::ProgressDisposalCapacityWakeup,
    /// Monotonic identity allocated before each bounded restore generation.
    pub(super) next_restore_generation: Cell<RestoreGeneration>,
    /// Last generation's terminal or cancellation counters, retained after the
    /// runtime that owned them is taken.
    pub(super) last_restore_outcome: Cell<Option<SessionRestoreTurnMetrics>>,
    /// Aggregate tab-derived projection rebuilds, retained for boundedness proof.
    pub(in crate::ui::window) tab_projection_publications: Cell<u64>,
    /// User/CLI tab-selection intent accepted since window construction.
    pub(in crate::ui::window) selection_generation: Cell<SelectionGeneration>,
    /// Suppresses generation changes for restore-owned transient selections.
    pub(in crate::ui::window) applying_restore_selection: Cell<bool>,
    /// Session-file save failure state (`Healthy` or `Failed`).
    pub(super) save_failure: RefCell<SessionSaveFailurePhase>,
    /// Close-safety gate (`Idle`, `Inflight`, or `BypassPending`).
    pub(in crate::ui::window) close_safety: Cell<CloseSafetyPhase>,
    /// Monotonic identity source for sequential multi-editor close saves.
    next_close_save_identity: Cell<u64>,
    /// Current close-save batch allowed to admit and publish completion.
    close_save_session: Cell<CloseSaveSessionPhase>,
}

impl SessionState {
    pub(in crate::ui::window) fn is_restoring(&self) -> bool {
        !matches!(*self.restore_phase.borrow(), SessionRestorePhase::Idle)
    }

    pub(in crate::ui::window) fn has_active_restore_runtime(&self) -> bool {
        matches!(
            *self.restore_phase.borrow(),
            SessionRestorePhase::Restoring { .. }
        )
    }

    pub(in crate::ui::window) fn startup_descriptors_pending(&self) -> bool {
        matches!(
            *self.restore_phase.borrow(),
            SessionRestorePhase::ReadingStartup { .. }
                | SessionRestorePhase::AwaitingActivationOpens
        )
    }

    pub(in crate::ui::window) fn is_close_safety_inflight(&self) -> bool {
        self.close_safety.get() == CloseSafetyPhase::Inflight
    }

    /// Return the latest session-save failure message, if any.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn failure_detail(&self) -> Option<String> {
        match &*self.save_failure.borrow() {
            SessionSaveFailurePhase::Healthy => None,
            SessionSaveFailurePhase::Failed { detail, .. } => Some(detail.clone()),
        }
    }

    pub(in crate::ui::window) fn take_close_safety_bypass(&self) -> bool {
        if self.close_safety.get() == CloseSafetyPhase::BypassPending {
            self.close_safety.set(CloseSafetyPhase::Idle);
            true
        } else {
            false
        }
    }

    pub(in crate::ui::window) fn begin_close_save_session(&self) -> u64 {
        let identity = self.next_close_save_identity.get().wrapping_add(1);
        self.next_close_save_identity.set(identity);
        self.close_save_session
            .set(CloseSaveSessionPhase::Active(identity));
        identity
    }

    pub(in crate::ui::window) fn close_save_session_is_current(&self, identity: u64) -> bool {
        self.close_save_session.get() == CloseSaveSessionPhase::Active(identity)
    }

    pub(in crate::ui::window) fn finish_close_save_session(&self, identity: u64) {
        if self.close_save_session_is_current(identity) {
            self.close_save_session.set(CloseSaveSessionPhase::Idle);
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::ui::window) fn active_close_save_identity(&self) -> Option<u64> {
        match self.close_save_session.get() {
            CloseSaveSessionPhase::Idle => None,
            CloseSaveSessionPhase::Active(id) => Some(id),
        }
    }

    pub(super) fn take_restore_runtime(&self) -> Option<Box<SessionRestoreRuntime>> {
        let mut phase = self.restore_phase.borrow_mut();
        if matches!(*phase, SessionRestorePhase::Restoring { .. }) {
            match std::mem::take(&mut *phase) {
                SessionRestorePhase::Restoring { runtime } => Some(runtime),
                _ => None,
            }
        } else {
            None
        }
    }

    pub(super) fn with_restore_runtime<R>(
        &self,
        f: impl FnOnce(&SessionRestoreRuntime) -> R,
    ) -> Option<R> {
        let phase = self.restore_phase.borrow();
        match &*phase {
            SessionRestorePhase::Restoring { runtime } => Some(f(runtime)),
            _ => None,
        }
    }

    pub(super) fn with_restore_runtime_mut<R>(
        &self,
        f: impl FnOnce(&mut SessionRestoreRuntime) -> R,
    ) -> Option<R> {
        let mut phase = self.restore_phase.borrow_mut();
        match &mut *phase {
            SessionRestorePhase::Restoring { runtime } => Some(f(runtime)),
            _ => None,
        }
    }

    pub(in crate::ui::window) fn cancel_startup_read_for_dispose(&self) {
        let prev = {
            let mut phase = self.restore_phase.borrow_mut();
            if matches!(*phase, SessionRestorePhase::ReadingStartup { .. }) {
                match std::mem::take(&mut *phase) {
                    SessionRestorePhase::ReadingStartup { cancel } => Some(cancel),
                    _ => None,
                }
            } else {
                None
            }
        };
        if let Some(cancel) = prev {
            cancel.store(true, Ordering::Release);
        }
        self.restore_capacity_wakeup.cancel();
    }
}

/// GTK-owned state layered around the pure policy for one active generation.
pub(crate) struct SessionRestoreRuntime {
    pub(super) policy: SessionRestorePolicy,
    pub(super) scheduled_source: Option<glib::SourceId>,
    pub(super) preserve_existing_selection: bool,
    pub(super) selected_before: Option<glib::WeakRef<libadwaita::TabPage>>,
    pub(super) requested_page: Option<glib::WeakRef<libadwaita::TabPage>>,
    pub(super) projection_batch_owned: bool,
    pub(super) cleanup_allowed_on_terminal: bool,
    /// Selection intent generation captured before restore-owned tab mutations begin.
    pub(super) selection_generation: SelectionGeneration,
}

#[derive(Clone, Copy)]
enum RestoreSelectionPolicy {
    PreserveExisting,
    SelectRestored,
}

#[derive(Clone, Copy)]
enum OrphanCleanupPolicy {
    Allowed,
    Deferred,
}

impl SessionRestoreRuntime {
    fn new(
        policy: SessionRestorePolicy,
        selection: RestoreSelectionPolicy,
        selected_before: Option<glib::WeakRef<libadwaita::TabPage>>,
        cleanup: OrphanCleanupPolicy,
        selection_generation: SelectionGeneration,
    ) -> Self {
        Self {
            policy,
            scheduled_source: None,
            preserve_existing_selection: matches!(
                selection,
                RestoreSelectionPolicy::PreserveExisting
            ),
            selected_before,
            requested_page: None,
            projection_batch_owned: true,
            cleanup_allowed_on_terminal: matches!(cleanup, OrphanCleanupPolicy::Allowed),
            selection_generation,
        }
    }
}

impl LushtextWindow {
    /// Start one bounded multi-turn restore generation from compact descriptors.
    pub(super) fn begin_session_restore(&self, session: SessionData, cleanup_allowed: bool) {
        self.cancel_session_restore_runtime(false);
        if session.tabs.is_empty() {
            self.imp()
                .session
                .restore_phase
                .replace(SessionRestorePhase::Idle);
            // Tabs opened while the startup read ran (a CLI open, admitted in
            // the window-composition content stage) had their session saves
            // suppressed by `restoring`; persist them now.
            if self.imp().tab_view.n_pages() > 0 {
                self.save_session_debounced();
            }
            self.schedule_orphan_cleanup(cleanup_allowed);
            self.evaluate_preserved_drafts_after_startup();
            return;
        }

        let state = &self.imp().session;
        let generation = state.next_restore_generation.get().next();
        state.next_restore_generation.set(generation);
        let selection_policy = if self.imp().tab_view.n_pages() > 0 {
            RestoreSelectionPolicy::PreserveExisting
        } else {
            RestoreSelectionPolicy::SelectRestored
        };
        let cleanup_policy = if cleanup_allowed {
            OrphanCleanupPolicy::Allowed
        } else {
            OrphanCleanupPolicy::Deferred
        };
        let selected_before = self
            .imp()
            .tab_view
            .selected_page()
            .map(|page| page.downgrade());
        let policy = SessionRestorePolicy::new(generation, session.tabs, session.active_tab_index);
        let selection_generation = state.selection_generation.get();

        self.begin_tab_projection_refresh_batch();
        state.restore_phase.replace(SessionRestorePhase::Restoring {
            runtime: Box::new(SessionRestoreRuntime::new(
                policy,
                selection_policy,
                selected_before,
                cleanup_policy,
                selection_generation,
            )),
        });
        self.schedule_session_restore_turn(generation);
    }

    pub(super) fn schedule_session_restore_turn(&self, generation: RestoreGeneration) {
        let mut phase = self.imp().session.restore_phase.borrow_mut();
        let SessionRestorePhase::Restoring { runtime } = &mut *phase else {
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

    fn run_scheduled_session_restore_turn(&self, generation: RestoreGeneration) {
        let newer_selection = self
            .imp()
            .session
            .with_restore_runtime(|runtime| {
                (runtime.policy.generation() == generation
                    && runtime.selection_generation
                        != self.imp().session.selection_generation.get())
                .then(|| self.imp().tab_view.selected_page())
                .flatten()
            })
            .flatten();
        let Some(turn) = self.imp().session.with_restore_runtime_mut(|runtime| {
            if runtime.policy.generation() != generation {
                return None;
            }
            // A fired one-shot source no longer exists; take its ID without
            // calling `remove()` so cancellation cannot double-remove it.
            runtime.scheduled_source.take();
            Some(runtime.policy.plan_turn())
        }) else {
            return;
        };
        let Some(turn) = turn else {
            return;
        };

        let mut inline_releases = Vec::new();
        for admission in turn.admissions {
            if let Some(permit) = self.mount_restored_page(generation, admission) {
                inline_releases.push(permit);
            }
        }
        if let Some(page) = newer_selection {
            execution::apply_restore_selection(self, &page);
        } else {
            self.restore_preexisting_selection(generation);
        }

        let Some(outcome) = self.imp().session.with_restore_runtime_mut(|runtime| {
            if runtime.policy.generation() != generation {
                return None;
            }
            for permit in inline_releases {
                let released = runtime.policy.release_permit(permit);
                debug_assert!(released, "inline restore permit must release once");
            }
            Some((
                runtime.policy.is_terminal(),
                runtime.policy.needs_next_turn(),
            ))
        }) else {
            return;
        };
        let Some((terminal, needs_next_turn)) = outcome else {
            return;
        };
        if terminal {
            self.finish_session_restore(generation);
        } else if needs_next_turn {
            self.schedule_session_restore_turn(generation);
        }
    }

    /// Release one planning permit and advance the sequencer.
    ///
    /// Called from the load workflow's planning terminal. The generation check
    /// before `release_permit` is not redundant with the one inside it: it keeps
    /// a stale generation's terminal from being *counted* at all.
    pub(super) fn release_session_restore_plan_permit(&self, permit: SessionRestorePlanPermit) {
        let Some(outcome) = self.imp().session.with_restore_runtime_mut(|runtime| {
            if runtime.policy.generation() != permit.generation()
                || !runtime.policy.release_permit(permit)
            {
                return None;
            }
            Some((
                runtime.policy.is_terminal(),
                runtime.policy.needs_next_turn(),
            ))
        }) else {
            return;
        };
        let Some((terminal, needs_next_turn)) = outcome else {
            return;
        };
        if terminal {
            self.finish_session_restore(permit.generation());
        } else if needs_next_turn {
            self.schedule_session_restore_turn(permit.generation());
        }
    }

    fn restore_preexisting_selection(&self, generation: RestoreGeneration) {
        let selected = self
            .imp()
            .session
            .with_restore_runtime(|runtime| {
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
            .flatten();
        if let Some(page) = selected {
            execution::apply_restore_selection(self, &page);
        }
    }

    fn finish_session_restore(&self, generation: RestoreGeneration) {
        let Some(mut runtime) = self.imp().session.take_restore_runtime() else {
            return;
        };
        if runtime.policy.generation() != generation {
            self.imp()
                .session
                .restore_phase
                .replace(SessionRestorePhase::Restoring { runtime });
            return;
        }
        if let Some(source_id) = runtime.scheduled_source.take() {
            source_id.remove();
        }

        execution::settle_restore_selection(self, &mut runtime);
        // The restored document takes the keyboard from GTK's initial focus
        // (`investigate-first-keys-without-frames`) when the restore created a
        // page in this window; a focus the user chose, or any key or button
        // the user pressed, while the restore ran is kept.
        if runtime.policy.metrics().pages_created > 0 {
            self.hand_focus_to_opened_document();
        } else {
            // No handoff follows: an alert held back while the restore ran
            // is announced now (`opened_document_focus_pending`).
            self.render_notifications();
        }

        let first_terminal_projection = runtime.policy.note_terminal_projection_publication();
        debug_assert!(
            first_terminal_projection,
            "current restore generation publishes one terminal projection"
        );
        let metrics = runtime.policy.metrics();
        let cleanup_allowed = runtime.cleanup_allowed_on_terminal;
        if runtime.projection_batch_owned {
            runtime.projection_batch_owned = false;
            self.end_tab_projection_refresh_batch();
        }
        // Each page attached after the selection scrolled the strip to its new
        // tab (ledger A132), so the selected tab can be out of view: let the
        // strip bring it back once the terminal publish has shown the strip.
        self.reveal_selected_tab();
        evidence::record_restore_outcome(self, metrics);
        self.schedule_orphan_cleanup(cleanup_allowed);
        self.evaluate_preserved_drafts_after_startup();
    }

    pub(super) fn cancel_session_restore_runtime(&self, publish_projection: bool) {
        let Some(mut runtime) = self.imp().session.take_restore_runtime() else {
            return;
        };
        if let Some(source_id) = runtime.scheduled_source.take() {
            source_id.remove();
        }
        runtime.policy.cancel();
        let metrics = runtime.policy.metrics();
        if runtime.projection_batch_owned {
            runtime.projection_batch_owned = false;
            if publish_projection {
                self.end_tab_projection_refresh_batch();
            } else {
                self.cancel_tab_projection_refresh_batch();
            }
        }
        evidence::record_restore_outcome(self, metrics);
    }
}
