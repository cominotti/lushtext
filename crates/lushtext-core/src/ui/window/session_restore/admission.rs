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

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::session::SessionData;

use super::policy::{SessionRestorePlanPermit, SessionRestorePolicy};
use super::{evidence, execution};
use crate::ui::window::LushtextWindow;

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
    pub(super) selection_generation: u64,
}

impl SessionRestoreRuntime {
    fn new(
        policy: SessionRestorePolicy,
        preserve_existing_selection: bool,
        selected_before: Option<glib::WeakRef<libadwaita::TabPage>>,
        cleanup_allowed_on_terminal: bool,
        selection_generation: u64,
    ) -> Self {
        Self {
            policy,
            scheduled_source: None,
            preserve_existing_selection,
            selected_before,
            requested_page: None,
            projection_batch_owned: true,
            cleanup_allowed_on_terminal,
            selection_generation,
        }
    }
}

impl LushtextWindow {
    /// Start one bounded multi-turn restore generation from compact descriptors.
    pub(super) fn begin_session_restore(&self, session: SessionData, cleanup_allowed: bool) {
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

    pub(super) fn schedule_session_restore_turn(&self, generation: u64) {
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
            if let Some(permit) = self.mount_restored_page(generation, admission) {
                inline_releases.push(permit);
            }
        }
        if let Some(page) = newer_selection {
            execution::apply_restore_selection(self, &page);
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

    /// Release one planning permit and advance the sequencer.
    ///
    /// Called from the load workflow's planning terminal. The generation check
    /// before `release_permit` is not redundant with the one inside it: it keeps
    /// a stale generation's terminal from being *counted* at all.
    pub(super) fn release_session_restore_plan_permit(&self, permit: SessionRestorePlanPermit) {
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
            execution::apply_restore_selection(self, &page);
        }
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

        execution::settle_restore_selection(self, &mut runtime);

        let first_terminal_projection = runtime.policy.note_terminal_projection_publication();
        debug_assert!(
            first_terminal_projection,
            "current restore generation publishes one terminal projection"
        );
        let metrics = runtime.policy.metrics();
        let cleanup_allowed = runtime.cleanup_allowed_on_terminal;
        self.imp().session.restoring.set(false);
        if runtime.projection_batch_owned {
            runtime.projection_batch_owned = false;
            self.end_tab_projection_refresh_batch();
        }
        evidence::record_restore_outcome(self, metrics);
        self.schedule_orphan_cleanup(cleanup_allowed);
    }

    pub(super) fn cancel_session_restore_runtime(&self, publish_projection: bool) {
        let Some(mut runtime) = self.imp().session.restore_runtime.take() else {
            return;
        };
        if let Some(source_id) = runtime.scheduled_source.take() {
            source_id.remove();
        }
        runtime.policy.cancel();
        let metrics = runtime.policy.metrics();
        self.imp().session.restoring.set(false);
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
