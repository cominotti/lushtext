// SPDX-License-Identifier: GPL-3.0-or-later

//! Filesystem watcher lifecycle for one workspace section.
//!
//! The section owns watcher setup and GTK-side polling, while the service layer
//! owns the backend debouncer and path extraction. Restarting the watcher when
//! roots or drill-down scope change keeps refresh work proportional to what the
//! section is actually showing.

use std::rc::Rc;
use std::time::Duration;

use gtk4::gio::prelude::ListModelExt;
use gtk4::glib;
use gtk4::prelude::Cast;
use gtk4::prelude::ObjectExt;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use crate::services::notifications::NotificationSeverity;
use crate::services::workspace_watch::{
    WorkspaceWatchError, WorkspaceWatchPoll, WorkspaceWatchTarget, WorkspaceWatcher,
};

use super::LushtextWorkspaceSection;

/// Poll cadence for draining debounced watcher results on the GTK thread.
const WATCH_POLL_MS: u64 = 100;
/// Delay watcher startup by one main-loop tick so the window can present the
/// restored workspaces and session tabs before recursive watch setup begins.
const WATCH_START_DELAY_MS: u64 = 1;

impl LushtextWorkspaceSection {
    /// Restart automatic watching for the current visible roots.
    pub(super) fn restart_workspace_watch(&self) {
        self.stop_workspace_watch();

        let targets = self.current_watch_targets();

        if targets.is_empty() {
            return;
        }

        let generation = self
            .imp()
            .watch_runtime
            .start_generation
            .get()
            .wrapping_add(1);
        self.imp().watch_runtime.start_generation.set(generation);

        let section_weak = ObjectExt::downgrade(self);
        let targets = Rc::new(targets);
        let source_id =
            glib::timeout_add_local_once(Duration::from_millis(WATCH_START_DELAY_MS), move || {
                let Some(section) = section_weak.upgrade() else {
                    return;
                };
                section
                    .imp()
                    .watch_runtime
                    .start_source_id
                    .borrow_mut()
                    .take();
                if section.imp().watch_runtime.start_generation.get() != generation {
                    return;
                }

                match WorkspaceWatcher::start(targets.as_slice()) {
                    Ok(watcher) => {
                        section
                            .imp()
                            .watch_runtime
                            .last_reported_error
                            .borrow_mut()
                            .take();
                        *section.imp().watch_runtime.watcher.borrow_mut() = Some(watcher);
                        section.install_watch_poll_source();
                    }
                    Err(error) => section.report_watch_error(&start_error_message(&error)),
                }
            });
        *self.imp().watch_runtime.start_source_id.borrow_mut() = Some(source_id);
    }

    /// Stop automatic watching and remove the GTK poll source.
    pub(super) fn stop_workspace_watch(&self) {
        if let Some(source_id) = self.imp().watch_runtime.start_source_id.borrow_mut().take() {
            source_id.remove();
        }
        if let Some(source_id) = self.imp().watch_runtime.poll_source_id.borrow_mut().take() {
            source_id.remove();
        }
        self.imp().watch_runtime.watcher.borrow_mut().take();
    }

    fn install_watch_poll_source(&self) {
        if let Some(source_id) = self.imp().watch_runtime.poll_source_id.borrow_mut().take() {
            source_id.remove();
        }
        let section_weak = ObjectExt::downgrade(self);
        let source_id = glib::timeout_add_local(Duration::from_millis(WATCH_POLL_MS), move || {
            let Some(section) = section_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            section.drain_watch_polls();
            glib::ControlFlow::Continue
        });
        *self.imp().watch_runtime.poll_source_id.borrow_mut() = Some(source_id);
    }

    fn drain_watch_polls(&self) {
        loop {
            let poll = {
                let watcher = self.imp().watch_runtime.watcher.borrow();
                watcher.as_ref().and_then(WorkspaceWatcher::try_poll)
            };

            let Some(poll) = poll else {
                break;
            };

            match poll {
                WorkspaceWatchPoll::Update(update) => {
                    self.queue_auto_refresh(update.changed_paths);
                }
                WorkspaceWatchPoll::Error(message) => {
                    self.report_watch_error(&message);
                }
            }
        }
    }

    fn report_watch_error(&self, message: &str) {
        let mut last_error = self.imp().watch_runtime.last_reported_error.borrow_mut();
        if last_error.as_deref() == Some(message) {
            return;
        }
        *last_error = Some(message.to_string());
        self.emit_message(message, NotificationSeverity::Warning);
    }

    fn current_watch_targets(&self) -> Vec<WorkspaceWatchTarget> {
        let mut targets = Vec::new();

        let tree_model = self
            .imp()
            .tree_model
            .try_borrow()
            .ok()
            .and_then(|tree_model| tree_model.as_ref().cloned());

        if let Some(tree_model) = tree_model {
            for index in 0..tree_model.n_items() {
                let Some(row): Option<gtk4::TreeListRow> = tree_model
                    .item(index)
                    .and_then(|obj| obj.downcast::<gtk4::TreeListRow>().ok())
                else {
                    continue;
                };
                let Some(item): Option<crate::ui::sidebar::file_tree_item::FileTreeItem> =
                    row.item().and_then(|obj| {
                        obj.downcast::<crate::ui::sidebar::file_tree_item::FileTreeItem>()
                            .ok()
                    })
                else {
                    continue;
                };
                let Some(path) = item.path() else {
                    continue;
                };

                if item.is_dir() {
                    if row.depth() == 0 || row.is_expanded() {
                        targets.push(WorkspaceWatchTarget::directory(path));
                    }
                } else if row.depth() == 0 {
                    targets.push(WorkspaceWatchTarget::file(path));
                }
            }
        }

        if targets.is_empty() {
            targets.extend(
                self.current_visible_roots()
                    .into_iter()
                    .map(|entry| match entry {
                        crate::model::workspace::WorkspaceEntry::Directory { path } => {
                            WorkspaceWatchTarget::directory(path)
                        }
                        crate::model::workspace::WorkspaceEntry::File { path } => {
                            WorkspaceWatchTarget::file(path)
                        }
                    }),
            );
        }

        targets
    }
}

fn start_error_message(error: &WorkspaceWatchError) -> String {
    format!("Workspace auto-refresh unavailable: {error}")
}
