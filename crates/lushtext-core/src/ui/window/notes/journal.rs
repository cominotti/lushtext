// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination role `journal`: the note-sidecar migration ledger.
//!
//! This is the workflow's durable, generation-guarded record, and it carries the
//! **longest-lived inversion in the codebase**. When a rename completes,
//! [`LushtextWindow::migrate_note_sidecars_after_rename`] records pending work
//! for all three sidecar kinds *before* moving anything, then runs bookmarks,
//! document notes, and folder notes in that fixed order. If the process dies
//! mid-run, control resumes in
//! [`LushtextWindow::reconcile_pending_migrations_on_startup`] **on a later app
//! launch**, bounded by the ledger's own attempt cap.
//!
//! It is a `journal` rather than an `execution` on the role test a later stage of
//! the same workflow reads the record back: startup reconcile is that stage. The
//! mutual-exclusion gate that serializes ledger writes lives inside
//! `services::migration_ledger` with the record it protects, not in a separate
//! `admission` module.
//!
//! Ordering that must not change: the rename's own cache, watch-row, and
//! expansion updates settle first — that guarantee belongs to
//! `WFR-WORKSPACE-TREE`, which calls this operation through
//! `ui/window/documents.rs` — and once called, `record_pending` precedes every
//! `run_tracked_kind`.

use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use std::path::Path;

use crate::model::migration_ledger::MigrationKind;
use crate::services::{
    bookmark_service, document_note_service, folder_note_service, json_store,
    local_history_service, migration_ledger,
};
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

impl LushtextWindow {
    /// Migrate sidecar documents after an in-app sidebar rename.
    ///
    /// Pending ledger state is recorded before sidecar moves begin so interrupted
    /// partial work can retry on startup by generation.
    pub(in crate::ui::window) fn migrate_note_sidecars_after_rename(
        &self,
        old_path: &Path,
        new_path: &Path,
    ) {
        let old_path = old_path.to_path_buf();
        let new_path = new_path.to_path_buf();
        let old_path_for_move = old_path.clone();
        let new_path_for_move = new_path.clone();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                // One critical section for the whole rename: the ledger entry and
                // all three kinds. Recording pending work and then running the
                // kinds under *separate* locks would let a second rename of the
                // same tree slip between two kinds, complete a kind with `Ok(0)`
                // because the sidecar had not moved yet, and retire both ledger
                // entries — stranding the note with nothing left to retry. See
                // `migration_ledger::migration_operation_lock`.
                migration_ledger::run_tracked_rename(
                    &data_dir,
                    &old_path_for_move,
                    &new_path_for_move,
                    &[
                        MigrationKind::Bookmarks,
                        MigrationKind::DocumentNotes,
                        MigrationKind::FolderNotes,
                    ],
                    |rename| {
                        let bookmark_count =
                            rename.run_kind(&data_dir, MigrationKind::Bookmarks, || {
                                bookmark_service::move_path_tree(
                                    &data_dir,
                                    &old_path_for_move,
                                    &new_path_for_move,
                                )
                            })?;
                        let document_note_count =
                            rename.run_kind(&data_dir, MigrationKind::DocumentNotes, || {
                                document_note_service::move_path_tree(
                                    &data_dir,
                                    &old_path_for_move,
                                    &new_path_for_move,
                                )
                            })?;
                        let folder_note_count =
                            rename.run_kind(&data_dir, MigrationKind::FolderNotes, || {
                                folder_note_service::move_folder_tree(
                                    &data_dir,
                                    &old_path_for_move,
                                    &new_path_for_move,
                                )
                            })?;
                        Ok((bookmark_count, document_note_count, folder_note_count))
                    },
                )
            },
            move |(), result| {
                if let Err(error) = result {
                    tracing::error!(
                        "Failed to migrate note sidecars for {} -> {}: {error}",
                        old_path.display(),
                        new_path.display()
                    );
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message(
                            "Rename succeeded, but note sidecars could not be moved",
                            MessageKind::Warning,
                        );
                    }
                } else if let Some(window) = window_weak.upgrade() {
                    window.refresh_command_palette_note_source_debounced();
                }
            },
        );
    }

    /// Retry persisted sidecar or local-history migrations left by an
    /// interrupted rename flow.
    ///
    /// Visibility note: `pub` rather than `pub(in crate::ui::window)` because the
    /// external widget harness drives this operation directly to prove that its
    /// completion re-resolves open editors' note sidecars. It is an intent-named
    /// workflow operation, not a test seam, so it is widened rather than
    /// duplicated behind a `*_for_test` name.
    pub fn reconcile_pending_migrations_on_startup(&self) {
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                let migration_report = migration_ledger::reconcile_pending(&data_dir)?;
                let local_history_report = local_history_service::reconcile_lineages(&data_dir)?;
                Ok::<_, anyhow::Error>((migration_report, local_history_report))
            },
            move |(), result| match result {
                Ok((migration_report, local_history_report)) => {
                    if migration_report.completed > 0 {
                        tracing::info!(
                            "Recovered {} pending migration kind(s)",
                            migration_report.completed
                        );
                        // Restored tabs may already have read a sidecar this
                        // reconcile has just moved, so they are showing stale or
                        // empty note state. Re-resolve them now rather than
                        // waiting for the user's next edit — which, before the
                        // unread-sidecar guard in `bookmark_execution`, would
                        // have written the stale set back over the migrated one.
                        if let Some(window) = window_weak.upgrade() {
                            window.resolve_notes_for_open_editors();
                        }
                    }
                    if local_history_report.reconciled_lineages > 0 {
                        tracing::info!(
                            "Reconciled {} local-history lineage(s)",
                            local_history_report.reconciled_lineages
                        );
                    }
                    let deferred_local_history_work = local_history_report.has_deferred_work();
                    if deferred_local_history_work {
                        tracing::warn!(
                            "Deferred local-history reconciliation after scanning {} lineage(s)",
                            local_history_report.scanned_lineages
                        );
                    }
                    for diagnostic in &migration_report.diagnostics {
                        tracing::warn!(
                            "Migration recovery {} generation {}: {}",
                            diagnostic.kind.label(),
                            diagnostic.generation,
                            diagnostic.message
                        );
                    }
                    for diagnostic in &local_history_report.diagnostics {
                        tracing::warn!(
                            "Local-history recovery diagnostic: {}",
                            diagnostic.summary()
                        );
                    }
                    // Only the user-facing message is conditional; the warnings
                    // above are no-ops when their diagnostic lists are empty.
                    if (deferred_local_history_work
                        || !migration_report.diagnostics.is_empty()
                        || !local_history_report.diagnostics.is_empty())
                        && let Some(window) = window_weak.upgrade()
                    {
                        window.publish_status_message(
                            "Some rename recovery work still needs attention",
                            MessageKind::Warning,
                        );
                    }
                }
                Err(error) => {
                    tracing::warn!("Failed to reconcile pending migrations: {error}");
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message(
                            "Rename recovery state could not be checked",
                            MessageKind::Warning,
                        );
                    }
                }
            },
        );
    }
}
