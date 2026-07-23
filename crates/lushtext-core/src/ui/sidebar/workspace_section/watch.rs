// SPDX-License-Identifier: GPL-3.0-or-later

//! Incremental filesystem watcher lifecycle for one workspace section.

use std::time::Duration;

use gtk4::gio::prelude::ListModelExt;
use gtk4::glib;
use gtk4::prelude::{Cast, ObjectExt};
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use crate::model::workspace::FolderTreeEntry;
use crate::services::notifications::NotificationSeverity;
#[cfg(feature = "test-utils")]
use crate::services::workspace_watch::WorkspaceWatchMailboxSnapshot;
use crate::services::workspace_watch::{
    WorkspaceWatchChange, WorkspaceWatchError, WorkspaceWatchTarget, WorkspaceWatcher,
};
use crate::ui::sidebar::file_tree_item::FileTreeItem;

use super::LushtextWorkspaceSection;
use super::watch_targets::RowWatchContribution;

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
        if refresh.pending_full_reload.get() || !refresh.pending_paths.borrow().is_empty() {
            return true;
        }
        if super::tree_loading::child_scan_blocks_readiness(self) {
            return true;
        }

        #[cfg(feature = "test-utils")]
        if self.imp().watch_runtime.test_disabled.get() {
            return false;
        }

        let watch = &self.imp().watch_runtime;
        if watch.worker_inflight.get() {
            return true;
        }
        let targets = watch.targets.borrow();
        let target_install_pending = if targets.is_empty() {
            watch.watcher.borrow().is_some()
        } else {
            let generation = targets.generation();
            watch.installed_generation.get() != Some(generation)
                && watch.unavailable_generation.get() != Some(generation)
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

    /// Test seam for the same scalar state used by automation readiness.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn workspace_refresh_blocks_readiness_for_test(&self) -> bool {
        self.workspace_refresh_blocks_readiness()
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

    fn install_expanded_watch_hook(&self, row: &gtk4::TreeListRow) {
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
            if super::dnd::expanded_watch_should_be_suppressed(row) {
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

    fn queue_workspace_watch_restart(&self) {
        #[cfg(feature = "test-utils")]
        if self.imp().watch_runtime.test_disabled.get() {
            return;
        }

        if self.imp().watch_runtime.targets.borrow().is_empty() {
            self.imp()
                .watch_runtime
                .last_reported_error
                .borrow_mut()
                .take();
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
        if runtime.worker_inflight.get() {
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
        self.imp().watch_runtime.installed_generation.set(None);
        self.imp().watch_runtime.unavailable_generation.set(None);
        let generation = snapshot.generation;
        let targets = snapshot.targets;
        let section_weak = self.downgrade();
        #[cfg(feature = "test-utils")]
        let start_delay = self.imp().watch_runtime.test_start_delay.get();
        #[cfg(feature = "test-utils")]
        let drop_delay = self.imp().watch_runtime.test_drop_delay.get();
        self.imp().watch_runtime.worker_inflight.set(true);
        #[cfg(feature = "test-utils")]
        self.imp()
            .watch_runtime
            .test_worker_starts
            .set(self.imp().watch_runtime.test_worker_starts.get() + 1);

        gtk_lush_tasks::spawn_blocking_then(
            (section_weak, generation, lifetime),
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
            |(section_weak, generation, lifetime), result| {
                let Some(section) = section_weak.upgrade() else {
                    retire_worker_result(result);
                    return;
                };
                section.imp().watch_runtime.worker_inflight.set(false);
                if section.imp().watch_runtime.lifetime_generation.get() != lifetime {
                    retire_worker_result(result);
                    return;
                }
                if section.imp().watch_runtime.targets.borrow().generation() != generation {
                    let _ = section.imp().watch_runtime.restart_debounce.invalidate();
                    section.start_current_workspace_watch_retiring(stale_watcher(result));
                    return;
                }

                match result {
                    WatchWorkerResult::Retired => {
                        section.imp().watch_runtime.unavailable_generation.set(None);
                        section
                            .imp()
                            .watch_runtime
                            .last_reported_error
                            .borrow_mut()
                            .take();
                        section.sync_file_tree_error_state();
                    }
                    WatchWorkerResult::Started(Ok(watcher)) => {
                        section.imp().watch_runtime.unavailable_generation.set(None);
                        section
                            .imp()
                            .watch_runtime
                            .last_reported_error
                            .borrow_mut()
                            .take();
                        section.sync_file_tree_error_state();
                        *section.imp().watch_runtime.watcher.borrow_mut() = Some(watcher);
                        section
                            .imp()
                            .watch_runtime
                            .installed_generation
                            .set(Some(generation));
                        section.install_watch_poll_source();
                    }
                    WatchWorkerResult::Started(Err(error)) => {
                        section
                            .imp()
                            .watch_runtime
                            .unavailable_generation
                            .set(Some(generation));
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
        runtime.installed_generation.set(None);
        runtime.unavailable_generation.set(None);
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
            let generation = self.imp().watch_runtime.installed_generation.replace(None);
            self.imp()
                .watch_runtime
                .unavailable_generation
                .set(generation);
            self.imp().watch_runtime.poll_source_id.borrow_mut().take();
            if let Some(watcher) = self.imp().watch_runtime.watcher.borrow_mut().take() {
                retire_watcher(watcher);
            }
            return glib::ControlFlow::Break;
        }

        glib::ControlFlow::Continue
    }

    fn report_watch_error(&self, message: &str) {
        let mut last_error = self.imp().watch_runtime.last_reported_error.borrow_mut();
        if last_error.as_deref() == Some(message) {
            return;
        }
        *last_error = Some(message.to_string());
        drop(last_error);
        self.sync_file_tree_error_state();
        self.emit_message(message, NotificationSeverity::Warning);
    }

    /// Test helper for verifying incremental watcher target selection.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn watch_targets_for_test(&self) -> Vec<WorkspaceWatchTarget> {
        self.imp().watch_runtime.targets.borrow().snapshot().targets
    }

    /// Return and reset the count of rows touched by target bookkeeping.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn take_watch_target_rows_touched_for_test(&self) -> usize {
        self.imp()
            .watch_runtime
            .targets
            .borrow_mut()
            .take_touched_rows()
    }

    /// Return the effective target generation for restart-churn assertions.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn watch_target_generation_for_test(&self) -> u64 {
        self.imp()
            .watch_runtime
            .targets
            .borrow()
            .generation()
            .value()
    }

    /// Whether the installed backend belongs to the latest effective targets.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn workspace_watcher_is_current_for_test(&self) -> bool {
        let current = self.imp().watch_runtime.targets.borrow().generation();
        self.imp().watch_runtime.installed_generation.get() == Some(current)
    }

    /// Whether terminal unavailability belongs to the latest effective targets.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn workspace_watcher_unavailability_is_current_for_test(&self) -> bool {
        let current = self.imp().watch_runtime.targets.borrow().generation();
        self.imp().watch_runtime.unavailable_generation.get() == Some(current)
    }

    /// Configure section-local worker delays for lifecycle responsiveness tests.
    #[cfg(feature = "test-utils")]
    pub fn set_workspace_watcher_delays_for_test(&self, start: Duration, drop: Duration) {
        self.imp().watch_runtime.test_start_delay.set(start);
        self.imp().watch_runtime.test_drop_delay.set(drop);
    }

    /// Return section-local lifecycle worker starts for latest-only assertions.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn workspace_watcher_worker_starts_for_test(&self) -> usize {
        self.imp().watch_runtime.test_worker_starts.get()
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

    /// Scalar mailbox, refresh-plan, and per-poll evidence without retained paths.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn workspace_watch_pressure_for_test(
        &self,
    ) -> (Option<WorkspaceWatchMailboxSnapshot>, usize, bool, usize) {
        let mailbox = self
            .imp()
            .watch_runtime
            .watcher
            .borrow()
            .as_ref()
            .map(WorkspaceWatcher::mailbox_snapshot_for_test);
        let (refresh_paths, refresh_full) = self.refresh_pressure_for_test();
        (
            mailbox,
            refresh_paths,
            refresh_full,
            self.imp().watch_runtime.test_last_poll_notices.get(),
        )
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
        self.imp().watch_runtime.test_disabled.set(true);
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
