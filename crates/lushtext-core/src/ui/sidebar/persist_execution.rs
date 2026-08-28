// SPDX-License-Identifier: GPL-3.0-or-later

//! `execution` role for the workspace tree workflow's **persistence** stage order.
//!
//! # Role, and why this is `execution` rather than `journal`
//!
//! Coordination, `execution`, qualified by the stage order it serves, with
//! latest-generation supersession over a single worker slot.
//!
//! It is deliberately **not** a `journal`. The `journal` role names a durable,
//! generation-guarded record that a later stage of the same workflow reads back to
//! recover from a failure. `workspaces.json` is not that: no generation is written to
//! the file, there is no stale-record cleanup, a failed write leaves the previous
//! file intact, and the read-back is an ordinary next-launch load rather than a
//! recovery. The retry ladder awaits explicit user intent, not a recovered record.
//!
//! # Inversions to be aware of
//!
//! The debounce defers a requested write; the worker defers the write itself; and the
//! bounded retry ladder defers a failed write. Close-time flush bypasses the debounce
//! and resolves its waiters from the terminal, and **a flush failure aborts the close**
//! rather than letting the window close over an unwritten workspace list.

use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::glib;

use crate::services::notifications::NotificationSeverity;
use crate::services::{json_store, workspace_manager};
use crate::ui::sidebar::policy::{
    WorkspacePersistenceCloseDecision, WorkspacePersistenceStartReason,
    WorkspacePersistenceTerminalEffect,
};

use super::LushtextSidebar;

/// Typed user-safe failure returned by an asynchronous workspace close flush.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspacePersistenceFlushError {
    message: String,
}

impl WorkspacePersistenceFlushError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for WorkspacePersistenceFlushError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for WorkspacePersistenceFlushError {}

impl LushtextSidebar {
    /// Save the current workspace state to disk on a background thread.
    pub(super) fn persist(&self) {
        let should_schedule = {
            let imp = self.imp();
            let mut state = imp.persistence.borrow_mut();
            state.request_mutation();
            state.in_flight_generation().is_none()
        };
        if !should_schedule {
            return;
        }

        self.schedule_persist(
            Duration::from_millis(crate::ui::sidebar::policy::PERSIST_DEBOUNCE_MS),
            WorkspacePersistenceStartReason::Debounce,
        );
    }

    fn schedule_persist(&self, delay: Duration, reason: WorkspacePersistenceStartReason) {
        self.imp()
            .persist_debounce
            .schedule(self, delay, move |sidebar, _| {
                sidebar.start_persist_worker(reason);
            });
    }

    fn start_persist_worker(&self, reason: WorkspacePersistenceStartReason) {
        let Some(generation) = self.imp().persistence.borrow_mut().start(reason) else {
            return;
        };
        let data_dir = json_store::data_dir();
        let workspaces_file = self.imp().workspaces_file.borrow().clone();

        spawn_blocking_then(
            self.clone(),
            move || workspace_manager::save(&data_dir, &workspaces_file),
            move |sidebar, result| {
                let had_failure = sidebar.imp().persistence.borrow().is_failed();
                let effect = match result {
                    Ok(()) => sidebar
                        .imp()
                        .persistence
                        .borrow_mut()
                        .apply_success(generation),
                    Err(error) => {
                        tracing::error!("Failed to save workspaces: {error}");
                        sidebar
                            .imp()
                            .persistence
                            .borrow_mut()
                            .apply_failure(generation, "Workspace changes could not be saved.")
                    }
                };
                let close_waiting = !sidebar.imp().persistence_flush_waiters.borrow().is_empty();

                match effect {
                    WorkspacePersistenceTerminalEffect::StartNewest => {
                        sidebar.start_persist_worker(if close_waiting {
                            WorkspacePersistenceStartReason::CloseFlush
                        } else {
                            WorkspacePersistenceStartReason::Debounce
                        });
                    }
                    WorkspacePersistenceTerminalEffect::RetryAfter(_)
                    | WorkspacePersistenceTerminalEffect::AwaitExplicitRetry
                        if close_waiting =>
                    {
                        sidebar.resolve_workspace_flush_waiters(&Err(
                            WorkspacePersistenceFlushError::new(
                                "the newest workspace snapshot could not be saved",
                            ),
                        ));
                    }
                    WorkspacePersistenceTerminalEffect::RetryAfter(delay) => {
                        sidebar.publish_workspace_persistence_message(
                            "Workspace changes could not be saved. LushText will retry them.",
                            NotificationSeverity::Warning,
                        );
                        sidebar
                            .schedule_persist(delay, WorkspacePersistenceStartReason::RetryWakeup);
                    }
                    WorkspacePersistenceTerminalEffect::AwaitExplicitRetry => {
                        sidebar.publish_workspace_persistence_message(
                            "Workspace changes could not be saved. They remain pending for the next change or close attempt.",
                            NotificationSeverity::Warning,
                        );
                    }
                    WorkspacePersistenceTerminalEffect::Settled => {
                        if had_failure {
                            sidebar.publish_workspace_persistence_message(
                                "Workspace changes were saved.",
                                NotificationSeverity::Info,
                            );
                        }
                        if close_waiting {
                            sidebar.resolve_workspace_flush_waiters(&Ok(()));
                        }
                    }
                    WorkspacePersistenceTerminalEffect::IgnoredStale if close_waiting => {
                        sidebar.resolve_workspace_flush_waiters(&Err(
                            WorkspacePersistenceFlushError::new(
                                "workspace persistence returned an obsolete terminal",
                            ),
                        ));
                    }
                    WorkspacePersistenceTerminalEffect::IgnoredStale => {}
                }
            },
        );
    }

    /// Flush the newest requested workspace snapshot without waiting for debounce.
    pub(crate) fn flush_workspace_persistence(
        &self,
        callback: impl FnOnce(Result<(), WorkspacePersistenceFlushError>) + 'static,
    ) {
        let close_decision = {
            let persistence = self.imp().persistence.borrow();
            persistence.close_decision()
        };
        match close_decision {
            WorkspacePersistenceCloseDecision::Durable => {
                glib::idle_add_local_once(move || callback(Ok(())));
            }
            WorkspacePersistenceCloseDecision::WaitForInFlight(_) => {
                self.imp()
                    .persistence_flush_waiters
                    .borrow_mut()
                    .push(Box::new(callback));
            }
            WorkspacePersistenceCloseDecision::StartNow(_) => {
                self.imp()
                    .persistence_flush_waiters
                    .borrow_mut()
                    .push(Box::new(callback));
                let _ = self.imp().persist_debounce.invalidate();
                self.start_persist_worker(WorkspacePersistenceStartReason::CloseFlush);
            }
        }
    }

    fn resolve_workspace_flush_waiters(&self, result: &Result<(), WorkspacePersistenceFlushError>) {
        let waiters = self
            .imp()
            .persistence_flush_waiters
            .borrow_mut()
            .drain(..)
            .collect::<Vec<_>>();
        for waiter in waiters {
            waiter(result.clone());
        }
    }

    fn publish_workspace_persistence_message(&self, text: &str, severity: NotificationSeverity) {
        if let Some(ref callback) = *self.imp().message_callback.borrow() {
            callback(text, severity);
        }
    }
}
