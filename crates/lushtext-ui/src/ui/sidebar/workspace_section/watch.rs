// SPDX-License-Identifier: GPL-3.0-or-later

//! `watch` role for the workspace tree workflow: watcher install, mailbox reconcile,
//! and the materialized target mirror for one workspace section.
//!
//! # Role
//!
//! Coordination, `watch`, nested under the workflow's canonical role home in
//! `ui/sidebar/`. This module **keeps its pre-convention name** because that name is
//! already a correct bounded role name; renaming a stable correct module for symmetry
//! with newly named siblings is what the convention forbids. It owns `watch_targets.rs`
//! as a plain data structure rather than as a second role module.
//!
//! # Inversion to be aware of
//!
//! Watcher installation runs off the GTK thread against a captured target generation,
//! so control resumes in the install completion. A completion whose generation is no
//! longer the effective one must neither adopt its handle nor record unavailability
//! against the newer targets.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use gtk_lush_settle::Debounce;
use gtk4::gio::prelude::ListModelExt;
use gtk4::glib;
use gtk4::prelude::{Cast, ObjectExt};
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use crate::ui::sidebar::file_tree_item::FileTreeItem;
use crate::ui::sidebar::seams::{
    WatchLifetimeGeneration, WatchTargetGeneration, WorkspaceWatchDisposition, WorkspaceWatchFacts,
    WorkspaceWatchTicket,
};
use lushtext_model::values::TargetGeneration;
use lushtext_model::workspace::FolderTreeEntry;
use lushtext_services::coordination::notifications::NotificationSeverity;
use lushtext_services::watch::workspace_watch::{
    WorkspaceWatchChange, WorkspaceWatchError, WorkspaceWatchMailboxSnapshot, WorkspaceWatchTarget,
    WorkspaceWatcher,
};

use super::LushtextWorkspaceSection;
use super::watch_targets::{MaterializedWatchTargets, RowWatchContribution};

/// Lifecycle phase of the section's background watcher installation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum WatchInstallPhase {
    /// No watcher installed and no install worker in flight.
    #[default]
    Idle,
    /// One lifecycle worker currently owns watcher creation/teardown.
    Installing,
    /// A watcher is installed for `generation`.
    Installed(WatchTargetGeneration),
    /// Watcher setup or polling failed terminally for `generation`.
    Unavailable(WatchTargetGeneration),
}

/// Deduplicated watcher-error state for [`WatchRuntimeState`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) enum WatchErrorState {
    /// No active watcher error.
    #[default]
    Clear,
    /// Most recent watcher error already surfaced to the user.
    Reported(String),
}

#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum WatchTestMode {
    #[default]
    Enabled,
    Disabled,
}

/// Live filesystem-watch wiring for one workspace section.
#[derive(Default)]
pub struct WatchRuntimeState {
    /// Backend watcher for the current materialized folder scopes, if active.
    pub watcher: RefCell<Option<WorkspaceWatcher>>,
    /// Incremental mirror of flattened rows and their effective target set.
    pub(super) targets: RefCell<MaterializedWatchTargets>,
    /// Coalesces rapid model signals before a worker owns watcher replacement.
    pub restart_debounce: Debounce,
    /// Invalidates every completion created before this section is disposed.
    pub(super) lifetime_generation: Cell<WatchLifetimeGeneration>,
    /// Current watcher installation/availability phase.
    pub(super) install_phase: Cell<WatchInstallPhase>,
    /// GTK main-loop source that takes one mailbox notice without blocking.
    pub poll_source_id: RefCell<Option<glib::SourceId>>,
    /// Last watcher error shown to the user so repeated backend failures do not
    /// spam the status bar every poll tick.
    pub(super) last_reported_error: RefCell<WatchErrorState>,
    #[cfg(feature = "test-utils")]
    /// Section-local worker delay before backend creation in responsiveness tests.
    pub(super) test_start_delay: Cell<Duration>,
    #[cfg(feature = "test-utils")]
    /// Section-local worker delay before old-handle teardown in responsiveness tests.
    pub(super) test_drop_delay: Cell<Duration>,
    #[cfg(feature = "test-utils")]
    /// Worker starts observed by this section for latest-only handoff tests.
    pub(super) test_worker_starts: Cell<usize>,
    #[cfg(feature = "test-utils")]
    /// Notices consumed by the most recent GTK poll callback (always zero or one).
    pub(super) test_last_poll_notices: Cell<usize>,
    #[cfg(feature = "test-utils")]
    /// Permanently suppresses watcher restarts for deterministic manual-refresh tests.
    pub(super) test_mode: Cell<WatchTestMode>,
}

impl WatchRuntimeState {
    /// Whether a watcher error has been surfaced to the user.
    pub(super) fn has_reported_error(&self) -> bool {
        !matches!(*self.last_reported_error.borrow(), WatchErrorState::Clear)
    }

    /// Every watch fact the evidence surface reports, read in one pass.
    pub(crate) fn watch_evidence(
        &self,
    ) -> (
        Vec<WorkspaceWatchTarget>,
        TargetGeneration,
        usize,
        bool,
        bool,
        Option<WorkspaceWatchMailboxSnapshot>,
    ) {
        let (targets, generation, touched, current) = {
            let mirror = self.targets.borrow();
            (
                mirror.effective_targets(),
                TargetGeneration::new(mirror.generation().value_for_evidence()),
                {
                    #[cfg(feature = "test-utils")]
                    {
                        mirror.touched_rows_for_evidence()
                    }
                    #[cfg(not(feature = "test-utils"))]
                    {
                        0
                    }
                },
                mirror.generation(),
            )
        };
        let phase = self.install_phase.get();
        let is_current = phase == WatchInstallPhase::Installed(current);
        let unavailability_is_current = phase == WatchInstallPhase::Unavailable(current);
        let mailbox = self
            .watcher
            .borrow()
            .as_ref()
            .map(WorkspaceWatcher::mailbox_snapshot);
        (
            targets,
            generation,
            touched,
            is_current,
            unavailability_is_current,
            mailbox,
        )
    }

    /// Reset the mirror's touched-row counter. A drive, not an observation.
    #[cfg(feature = "test-utils")]
    pub(crate) fn reset_watch_target_rows_touched(&self) {
        self.targets.borrow_mut().reset_touched_rows();
    }

    /// Install workers started, for restart-churn assertions in the evidence surface.
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(
            clippy::unused_self,
            reason = "the counter this reports is recorded only under `test-utils`, so \
                      the receiver is genuinely unused in a default-feature build. It \
                      stays a method because the evidence surface reaches it uniformly \
                      in both configurations. Gated with `cfg_attr` so the expectation \
                      is self-policing: it fires exactly where the receiver is unused, \
                      and fails the build if that stops being true."
        )
    )]
    pub(crate) fn worker_starts_for_evidence(&self) -> usize {
        #[cfg(feature = "test-utils")]
        {
            self.test_worker_starts.get()
        }
        #[cfg(not(feature = "test-utils"))]
        {
            0
        }
    }

    /// Notices GTK consumed on the most recent poll, for the evidence surface.
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(
            clippy::unused_self,
            reason = "the counter this reports is recorded only under `test-utils`, so \
                      the receiver is genuinely unused in a default-feature build. It \
                      stays a method because the evidence surface reaches it uniformly \
                      in both configurations. Gated with `cfg_attr` so the expectation \
                      is self-policing: it fires exactly where the receiver is unused, \
                      and fails the build if that stops being true."
        )
    )]
    pub(crate) fn last_poll_notices_for_evidence(&self) -> usize {
        #[cfg(feature = "test-utils")]
        {
            self.test_last_poll_notices.get()
        }
        #[cfg(not(feature = "test-utils"))]
        {
            0
        }
    }

    /// Whether the watch-install worker is in flight, for the evidence surface.
    pub(crate) fn watch_worker_inflight_for_evidence(&self) -> bool {
        matches!(self.install_phase.get(), WatchInstallPhase::Installing)
    }
}

/// Poll cadence for taking one coalesced watcher notice on the GTK thread.
const WATCH_POLL_MS: u64 = 100;
/// Quiet window that folds one GTK model mutation burst into one replacement.
const WATCH_RESTART_SETTLE_MS: u64 = 25;
/// Private row marker proving the model-level expansion hook was installed.
const WATCH_EXPANDED_HOOK_KEY: &str = "workspace-watch-model-expanded-hook";

enum WatchWorkerResult {
    Retired,
    Started(Result<WorkspaceWatcher, WorkspaceWatchError>),
}

impl LushtextWorkspaceSection {
    /// Whether watcher transport, lifecycle, or refresh planning is still unsettled.
    pub(crate) fn workspace_refresh_blocks_readiness(&self) -> bool {
        let refresh = &self.imp().refresh_runtime;
        if refresh.has_pending_work() {
            return true;
        }
        if super::scan_execution::child_scan_blocks_readiness(self) {
            return true;
        }

        #[cfg(feature = "test-utils")]
        if self.imp().watch_runtime.test_mode.get() == WatchTestMode::Disabled {
            return false;
        }

        let watch = &self.imp().watch_runtime;
        let phase = watch.install_phase.get();
        if matches!(phase, WatchInstallPhase::Installing) {
            return true;
        }
        let targets = watch.targets.borrow();
        let target_install_pending = if targets.is_empty() {
            watch.watcher.borrow().is_some()
        } else {
            let generation = targets.generation();
            !matches!(
                phase,
                WatchInstallPhase::Installed(installed) | WatchInstallPhase::Unavailable(installed)
                    if installed == generation
            )
        };
        drop(targets);
        if target_install_pending {
            return true;
        }
        watch.watcher.borrow().as_ref().is_some_and(|watcher| {
            let snapshot = watcher.mailbox_snapshot();
            snapshot.retained_paths > 0
                || snapshot.full_refresh
                || snapshot.has_error
                || snapshot.disconnected
                || snapshot.busy
        })
    }

    /// Switch to configured-folder fallback while a flattened model is replaced.
    pub(super) fn prepare_workspace_watch_model(&self, folders: &[FolderTreeEntry]) {
        let fallback = folders.iter().map(watch_target_for_folder).collect();
        let changed = {
            let mut targets = self.imp().watch_runtime.targets.borrow_mut();
            let fallback_changed = targets.set_fallback(fallback);
            targets.unmount() || fallback_changed
        };
        if changed {
            self.queue_workspace_watch_restart();
        }
    }

    /// Install the initial mirror and incremental signals for one flattened model.
    pub(super) fn install_workspace_watch_model(&self, model: &gtk4::TreeListModel) {
        let rows = (0..model.n_items())
            .map(|position| row_at(model, position).and_then(|row| watch_contribution(&row)))
            .collect::<Vec<_>>();
        let changed = self.imp().watch_runtime.targets.borrow_mut().mount(rows);

        for position in 0..model.n_items() {
            if let Some(row) = row_at(model, position) {
                self.install_expanded_watch_hook(&row);
            }
        }

        let section_weak = self.downgrade();
        model.connect_items_changed(move |model, position, removed, added| {
            let Some(section) = section_weak.upgrade() else {
                return;
            };
            section.apply_watch_model_splice(model, position, removed, added);
        });

        if changed {
            self.queue_workspace_watch_restart();
        }
    }

    fn apply_watch_model_splice(
        &self,
        model: &gtk4::TreeListModel,
        position: u32,
        removed: u32,
        added: u32,
    ) {
        let added_rows = (position..position.saturating_add(added))
            .filter_map(|index| row_at(model, index))
            .collect::<Vec<_>>();
        let contributions = added_rows
            .iter()
            .map(watch_contribution)
            .collect::<Vec<_>>();
        let changed = self.imp().watch_runtime.targets.borrow_mut().splice(
            position as usize,
            removed as usize,
            &contributions,
        );
        for row in added_rows {
            self.install_expanded_watch_hook(&row);
        }
        if changed {
            self.queue_workspace_watch_restart();
        }
    }

    /// Install the expansion hook on `row` once. Called at model install, at
    /// splices, and at factory bind so every realized `TreeListRow` keeps the
    /// hook even after GTK replaced an unreferenced row object (ledger A114,
    /// design R4/D20).
    pub(super) fn install_expanded_watch_hook(&self, row: &gtk4::TreeListRow) {
        // SAFETY: the private marker stores only `true` on this row. The row
        // owns its signal handler, whose closure captures the section weakly.
        if unsafe { row.data::<bool>(WATCH_EXPANDED_HOOK_KEY) }.is_some() {
            return;
        }
        // SAFETY: this private row key stores only the marker described above.
        unsafe {
            row.set_data(WATCH_EXPANDED_HOOK_KEY, true);
        }

        let section_weak = self.downgrade();
        row.connect_notify_local(Some("expanded"), move |row, _| {
            // The authoritative expansion set follows every live transition,
            // even during a reorder drag, matching what a whole-model snapshot
            // would capture at the next refresh.
            if let Some(section) = section_weak.upgrade() {
                section.record_row_expansion_transition(row);
            }
            if super::reorder_execution::expanded_watch_should_be_suppressed(row) {
                return;
            }
            let row_weak = row.downgrade();
            let section_weak = section_weak.clone();
            glib::idle_add_local_once(move || {
                let (Some(section), Some(row)) = (section_weak.upgrade(), row_weak.upgrade())
                else {
                    return;
                };
                section.refresh_workspace_watch_row(&row);
            });
        });
    }

    /// Refresh one row contribution after an in-place model mutation.
    pub(super) fn refresh_workspace_watch_row(&self, row: &gtk4::TreeListRow) {
        let position = row.position();
        if position == gtk4::INVALID_LIST_POSITION {
            return;
        }
        let changed = self
            .imp()
            .watch_runtime
            .targets
            .borrow_mut()
            .update_row(position as usize, watch_contribution(row));
        if changed {
            self.queue_workspace_watch_restart();
        }
    }

    /// Request a watcher restart after an out-of-band path change.
    ///
    /// Used when a completed file operation's row is gone or has been recycled,
    /// so the incremental row-level mirror cannot be repaired through
    /// `refresh_workspace_watch_row`. Without this the mirror silently keeps the
    /// old path and external-change detection for the renamed file stops.
    pub(super) fn request_workspace_watch_restart(&self) {
        self.queue_workspace_watch_restart();
    }

    fn queue_workspace_watch_restart(&self) {
        #[cfg(feature = "test-utils")]
        if self.imp().watch_runtime.test_mode.get() == WatchTestMode::Disabled {
            return;
        }

        if self.imp().watch_runtime.targets.borrow().is_empty() {
            self.imp()
                .watch_runtime
                .last_reported_error
                .replace(WatchErrorState::Clear);
            self.sync_file_tree_error_state();
        }
        self.imp().watch_runtime.restart_debounce.schedule(
            self,
            Duration::from_millis(WATCH_RESTART_SETTLE_MS),
            |section, _| section.start_current_workspace_watch(),
        );
    }

    fn start_current_workspace_watch(&self) {
        self.start_current_workspace_watch_retiring(None);
    }

    fn start_current_workspace_watch_retiring(&self, retiring: Option<WorkspaceWatcher>) {
        let runtime = &self.imp().watch_runtime;
        if matches!(runtime.install_phase.get(), WatchInstallPhase::Installing) {
            if let Some(watcher) = retiring {
                retire_watcher(watcher);
            }
            return;
        }

        let snapshot = self.imp().watch_runtime.targets.borrow().snapshot();
        let lifetime = self.imp().watch_runtime.lifetime_generation.get();
        if let Some(source_id) = self.imp().watch_runtime.poll_source_id.borrow_mut().take() {
            source_id.remove();
        }
        let old_watcher = retiring.or_else(|| self.imp().watch_runtime.watcher.borrow_mut().take());
        self.imp()
            .watch_runtime
            .install_phase
            .set(WatchInstallPhase::Installing);
        let generation = snapshot.generation;
        let targets = snapshot.targets;
        let section_weak = self.downgrade();
        #[cfg(feature = "test-utils")]
        let start_delay = self.imp().watch_runtime.test_start_delay.get();
        #[cfg(feature = "test-utils")]
        let drop_delay = self.imp().watch_runtime.test_drop_delay.get();
        #[cfg(feature = "test-utils")]
        self.imp()
            .watch_runtime
            .test_worker_starts
            .set(self.imp().watch_runtime.test_worker_starts.get() + 1);

        let ticket = WorkspaceWatchTicket::new(generation, lifetime);

        gtk_lush_tasks::spawn_blocking_then(
            (section_weak, ticket),
            move || {
                #[cfg(feature = "test-utils")]
                std::thread::sleep(drop_delay);
                drop(old_watcher);
                if targets.is_empty() {
                    WatchWorkerResult::Retired
                } else {
                    #[cfg(feature = "test-utils")]
                    std::thread::sleep(start_delay);
                    WatchWorkerResult::Started(WorkspaceWatcher::start(&targets))
                }
            },
            |(section_weak, ticket), result| {
                let Some(section) = section_weak.upgrade() else {
                    retire_worker_result(result);
                    return;
                };
                section
                    .imp()
                    .watch_runtime
                    .install_phase
                    .set(WatchInstallPhase::Idle);
                // One seam value validated as a unit, so the two stale cases stay
                // distinguishable: a stale lifetime retires, a stale target
                // generation re-enters the install.
                let facts = WorkspaceWatchFacts::new(
                    section.imp().watch_runtime.targets.borrow().generation(),
                    section.imp().watch_runtime.lifetime_generation.get(),
                );
                match ticket.disposition(&facts) {
                    WorkspaceWatchDisposition::Retire => {
                        retire_worker_result(result);
                        return;
                    }
                    WorkspaceWatchDisposition::Restart => {
                        let _ = section.imp().watch_runtime.restart_debounce.invalidate();
                        section.start_current_workspace_watch_retiring(stale_watcher(result));
                        return;
                    }
                    WorkspaceWatchDisposition::Install => {}
                }
                let generation = ticket.targets_generation();

                match result {
                    WatchWorkerResult::Retired => {
                        section
                            .imp()
                            .watch_runtime
                            .install_phase
                            .set(WatchInstallPhase::Idle);
                        section
                            .imp()
                            .watch_runtime
                            .last_reported_error
                            .replace(WatchErrorState::Clear);
                        section.sync_file_tree_error_state();
                    }
                    WatchWorkerResult::Started(Ok(watcher)) => {
                        section
                            .imp()
                            .watch_runtime
                            .last_reported_error
                            .replace(WatchErrorState::Clear);
                        section.sync_file_tree_error_state();
                        *section.imp().watch_runtime.watcher.borrow_mut() = Some(watcher);
                        section
                            .imp()
                            .watch_runtime
                            .install_phase
                            .set(WatchInstallPhase::Installed(generation));
                        section.install_watch_poll_source();
                    }
                    WatchWorkerResult::Started(Err(error)) => {
                        section
                            .imp()
                            .watch_runtime
                            .install_phase
                            .set(WatchInstallPhase::Unavailable(generation));
                        section.report_watch_error(&start_error_message(&error));
                    }
                }
            },
        );
    }

    /// Stop automatic watching without dropping backend resources on GTK.
    pub(in crate::ui::sidebar) fn stop_workspace_watch(&self) {
        let runtime = &self.imp().watch_runtime;
        let _ = runtime.restart_debounce.invalidate();
        runtime
            .lifetime_generation
            .set(runtime.lifetime_generation.get().next());
        if let Some(source_id) = runtime.poll_source_id.borrow_mut().take() {
            source_id.remove();
        }
        runtime.install_phase.set(WatchInstallPhase::Idle);
        if let Some(watcher) = runtime.watcher.borrow_mut().take() {
            retire_watcher(watcher);
        }
    }

    fn install_watch_poll_source(&self) {
        if let Some(source_id) = self.imp().watch_runtime.poll_source_id.borrow_mut().take() {
            source_id.remove();
        }
        let section_weak = self.downgrade();
        let source_id = glib::timeout_add_local(Duration::from_millis(WATCH_POLL_MS), move || {
            let Some(section) = section_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            section.poll_workspace_watch()
        });
        *self.imp().watch_runtime.poll_source_id.borrow_mut() = Some(source_id);
    }

    fn poll_workspace_watch(&self) -> glib::ControlFlow {
        let notice = {
            let watcher = self.imp().watch_runtime.watcher.borrow();
            watcher.as_ref().and_then(WorkspaceWatcher::try_poll)
        };
        #[cfg(feature = "test-utils")]
        self.imp()
            .watch_runtime
            .test_last_poll_notices
            .set(usize::from(notice.is_some()));
        let Some(notice) = notice else {
            return glib::ControlFlow::Continue;
        };

        match notice.change {
            Some(WorkspaceWatchChange::Paths(paths)) => self.queue_auto_refresh(paths),
            Some(WorkspaceWatchChange::FullRefresh) => self.queue_auto_full_refresh(),
            None => {}
        }
        if let Some(message) = notice.error {
            self.report_watch_error(&message);
        }
        if notice.disconnected {
            self.report_watch_error("Workspace auto-refresh disconnected.");
            let next_phase = match self.imp().watch_runtime.install_phase.get() {
                WatchInstallPhase::Installed(generation) => {
                    WatchInstallPhase::Unavailable(generation)
                }
                _ => WatchInstallPhase::Idle,
            };
            self.imp().watch_runtime.install_phase.set(next_phase);
            self.imp().watch_runtime.poll_source_id.borrow_mut().take();
            if let Some(watcher) = self.imp().watch_runtime.watcher.borrow_mut().take() {
                retire_watcher(watcher);
            }
            return glib::ControlFlow::Break;
        }

        glib::ControlFlow::Continue
    }

    fn report_watch_error(&self, message: &str) {
        {
            let mut last_error = self.imp().watch_runtime.last_reported_error.borrow_mut();
            if matches!(&*last_error, WatchErrorState::Reported(existing) if existing == message) {
                return;
            }
            *last_error = WatchErrorState::Reported(message.to_string());
        }
        self.sync_file_tree_error_state();
        self.emit_message(message, NotificationSeverity::Warning);
    }

    /// Reset the count of rows touched by target bookkeeping.
    ///
    /// A **drive**, not an observation: the count itself is read from the evidence
    /// surface. The pre-convention seam was a `take`, so counting mutated.
    #[cfg(feature = "test-utils")]
    pub fn reset_watch_target_rows_touched_for_test(&self) {
        self.imp().watch_runtime.reset_watch_target_rows_touched();
    }

    /// Configure section-local worker delays for lifecycle responsiveness tests.
    #[cfg(feature = "test-utils")]
    pub fn set_workspace_watcher_delays_for_test(&self, start: Duration, drop: Duration) {
        self.imp().watch_runtime.test_start_delay.set(start);
        self.imp().watch_runtime.test_drop_delay.set(drop);
    }

    /// Merge a path batch into the installed handle without touching the filesystem.
    #[cfg(feature = "test-utils")]
    pub fn merge_workspace_watch_paths_for_test(&self, paths: Vec<std::path::PathBuf>) {
        if let Some(watcher) = self.imp().watch_runtime.watcher.borrow().as_ref() {
            watcher.merge_paths_for_test(paths);
        }
    }

    /// Merge a backend diagnostic beside any pending change notice.
    #[cfg(feature = "test-utils")]
    pub fn merge_workspace_watch_error_for_test(&self, message: &str) {
        if let Some(watcher) = self.imp().watch_runtime.watcher.borrow().as_ref() {
            watcher.merge_error_for_test(message);
        }
    }

    /// Mark the installed watcher disconnected so lifecycle recovery can be asserted.
    #[cfg(feature = "test-utils")]
    pub fn disconnect_workspace_watch_for_test(&self) {
        if let Some(watcher) = self.imp().watch_runtime.watcher.borrow().as_ref() {
            watcher.mark_disconnected_for_test();
        }
    }

    /// Pause the timer so one poll callback can be asserted deterministically.
    #[cfg(feature = "test-utils")]
    pub fn pause_workspace_watch_polling_for_test(&self) {
        if let Some(source_id) = self.imp().watch_runtime.poll_source_id.borrow_mut().take() {
            source_id.remove();
        }
    }

    /// Run exactly one poll callback and report whether it consumed a notice.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn poll_workspace_watch_once_for_test(&self) -> usize {
        let _ = self.poll_workspace_watch();
        self.imp().watch_runtime.test_last_poll_notices.get()
    }

    /// Test helper for isolating manual refresh from automatic watcher events.
    #[cfg(feature = "test-utils")]
    pub fn stop_workspace_watch_for_test(&self) {
        self.imp()
            .watch_runtime
            .test_mode
            .set(WatchTestMode::Disabled);
        self.stop_workspace_watch();
    }
}

fn row_at(model: &gtk4::TreeListModel, position: u32) -> Option<gtk4::TreeListRow> {
    model.item(position)?.downcast::<gtk4::TreeListRow>().ok()
}

fn watch_contribution(row: &gtk4::TreeListRow) -> RowWatchContribution {
    let item = row.item()?.downcast::<FileTreeItem>().ok()?;
    let path = item.path()?;
    if item.is_dir() && (row.depth() == 0 || row.is_expanded()) {
        Some(WorkspaceWatchTarget::directory(path))
    } else if !item.is_dir() && row.depth() == 0 {
        Some(WorkspaceWatchTarget::file(path))
    } else {
        None
    }
}

fn watch_target_for_folder(entry: &FolderTreeEntry) -> WorkspaceWatchTarget {
    match entry {
        FolderTreeEntry::Directory { path } => WorkspaceWatchTarget::directory(path.clone()),
        FolderTreeEntry::File { path } => WorkspaceWatchTarget::file(path.clone()),
    }
}

fn start_error_message(error: &WorkspaceWatchError) -> String {
    format!("Workspace auto-refresh unavailable: {error}")
}

fn retire_worker_result(result: WatchWorkerResult) {
    if let WatchWorkerResult::Started(Ok(watcher)) = result {
        retire_watcher(watcher);
    }
}

fn stale_watcher(result: WatchWorkerResult) -> Option<WorkspaceWatcher> {
    match result {
        WatchWorkerResult::Started(Ok(watcher)) => Some(watcher),
        WatchWorkerResult::Retired | WatchWorkerResult::Started(Err(_)) => None,
    }
}

fn retire_watcher(watcher: WorkspaceWatcher) {
    gtk_lush_tasks::spawn_blocking_then((), move || drop(watcher), |(), ()| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}

    #[test]
    fn concrete_watcher_and_worker_result_cross_the_worker_boundary() {
        assert_send::<WorkspaceWatcher>();
        assert_send::<WatchWorkerResult>();
        assert_send::<super::super::watch_targets::WatchTargetSnapshot>();
    }
}
