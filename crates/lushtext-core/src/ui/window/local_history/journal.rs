// SPDX-License-Identifier: GPL-3.0-or-later

//! The local-history sidecars: the durable record restore reads back.
//!
//! `journal` on slot 3a's reusable test — *does a later stage of the same
//! workflow restore from the record* — which the sidecars pass directly: capture
//! writes them, the browser lists them, and restore installs one back into the
//! user's buffer.
//!
//! This module owns reading the record and keeping it consistent with the files
//! it describes: listing a lineage (with recovery diagnostics), migrating a
//! lineage after an in-app rename, and deciding whether the browse action is
//! even reachable. Writing new snapshots is the capture half's job, in
//! `ui/editor_page/local_history.rs`, which this workflow **calls and records
//! rather than owns**.

use std::path::{Path, PathBuf};

use gtk_lush_tasks::spawn_blocking_then;
use gtk4::gio;
use gtk4::prelude::*;

use crate::model::local_history::LocalHistorySnapshotMeta;
use crate::model::migration_ledger::MigrationKind;
use crate::services::recovery_metadata::RecoveryDiagnostic;
use crate::services::{
    filesystem::metadata as fs_metadata, json_store, local_history_service, migration_ledger,
};
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;

use crate::ui::window::LushtextWindow;

/// Shown when a document is too large for local history to browse at all.
///
/// The size restates `local_history_service`'s browsing threshold in user-facing
/// prose, and both eligibility checks reach it: the already-open editor's and the
/// explicit-path worker's. One constant is what keeps them from disagreeing about
/// the limit the user is being told about.
const BROWSE_UNAVAILABLE_MESSAGE: &str = "Local history is unavailable for files above 50 MB";

/// Shown when an eligible file's snapshot metadata cannot be read.
///
/// Reported from both listing paths, each of which logs the underlying error
/// first; the user-facing wording deliberately stays the same for both.
const LISTING_FAILED_MESSAGE: &str = "Local history could not be loaded";

/// Background outcome for opening local history from a path rather than an already-loaded tab.
enum LocalHistoryPathLoadOutcome {
    /// The target exceeds the editor's local-history size policy.
    Unavailable,
    /// Snapshot metadata was loaded and can be presented once the tab is selected.
    Loaded {
        /// Saved file path whose lineage was loaded.
        path: PathBuf,
        /// Snapshot metadata for the browser sidebar.
        snapshots: Vec<LocalHistorySnapshotMeta>,
        /// Recovery diagnostics found while loading the lineage.
        diagnostics: Vec<RecoveryDiagnostic>,
    },
    /// Snapshot metadata could not be read.
    Failed(String),
}

impl LushtextWindow {
    pub(super) fn list_local_history_for_active_editor(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        let Some(path) = editor.file_path() else {
            self.publish_status_message(
                "Local history requires a saved file",
                MessageKind::Warning,
            );
            return;
        };
        if !editor.local_history_availability().allows_browsing() {
            self.publish_status_message(BROWSE_UNAVAILABLE_MESSAGE, MessageKind::Warning);
            return;
        }
        self.load_local_history_for_editor(editor, path);
    }

    /// Open local history for an explicit saved file path, selecting or opening its tab first.
    pub(super) fn list_local_history_for_path(&self, path: &Path) {
        let path = path.to_path_buf();
        spawn_blocking_then(
            self.clone(),
            move || {
                let availability = fs_metadata::file_facts(&path).ok().map_or(
                    local_history_service::LocalHistoryAvailability::Unavailable,
                    |facts| {
                        local_history_service::availability_for_size_check(
                            crate::services::file_limits::FileSizeCheck::classify(facts.byte_size),
                        )
                    },
                );
                if !availability.allows_browsing() {
                    return LocalHistoryPathLoadOutcome::Unavailable;
                }
                let data_dir = json_store::data_dir();
                match local_history_service::list_snapshots_for_path_recovering(&data_dir, &path) {
                    Ok(listing) => LocalHistoryPathLoadOutcome::Loaded {
                        path,
                        snapshots: listing.snapshots,
                        diagnostics: listing.diagnostics,
                    },
                    Err(error) => LocalHistoryPathLoadOutcome::Failed(error.to_string()),
                }
            },
            |window, result| match result {
                LocalHistoryPathLoadOutcome::Unavailable => {
                    window.publish_status_message(BROWSE_UNAVAILABLE_MESSAGE, MessageKind::Warning);
                }
                LocalHistoryPathLoadOutcome::Loaded {
                    path,
                    snapshots,
                    diagnostics,
                } => {
                    window.open_document(&path);
                    let Some(editor) = window.active_editor() else {
                        window.publish_status_message(
                            "Local history could not find an editor for that file",
                            MessageKind::Warning,
                        );
                        return;
                    };
                    let editor_path = editor.file_path().unwrap_or(path);
                    window.present_local_history_browser(editor, editor_path, snapshots);
                    window.publish_local_history_recovery_diagnostics(&diagnostics);
                }
                LocalHistoryPathLoadOutcome::Failed(error) => {
                    tracing::error!("Failed to list local-history snapshots: {error}");
                    window.publish_status_message(LISTING_FAILED_MESSAGE, MessageKind::Error);
                }
            },
        );
    }

    /// Load snapshot metadata for an already-open eligible editor.
    fn load_local_history_for_editor(&self, editor: LushtextEditorPage, path: PathBuf) {
        spawn_blocking_then(
            (self.clone(), editor, path.clone()),
            move || {
                let data_dir = json_store::data_dir();
                local_history_service::list_snapshots_for_path_recovering(&data_dir, &path)
            },
            |(window, editor, path), result| match result {
                Ok(listing) => {
                    window.present_local_history_browser(editor, path, listing.snapshots);
                    window.publish_local_history_recovery_diagnostics(&listing.diagnostics);
                }
                Err(error) => {
                    tracing::error!("Failed to list local-history snapshots: {error}");
                    window.publish_status_message(LISTING_FAILED_MESSAGE, MessageKind::Error);
                }
            },
        );
    }

    fn publish_local_history_recovery_diagnostics(&self, diagnostics: &[RecoveryDiagnostic]) {
        if diagnostics.is_empty() {
            return;
        }
        for diagnostic in diagnostics {
            tracing::warn!("{}", diagnostic.summary());
        }
        self.publish_status_message(
            "Some local-history metadata needed recovery",
            MessageKind::Warning,
        );
    }

    /// Recompute whether the local-history action should be enabled.
    pub(in crate::ui::window) fn update_local_history_action(&self) {
        if let Some(action) = self.lookup_action("show-local-history")
            && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
        {
            let enabled = self.active_editor().is_some_and(|editor| {
                editor.file_path().is_some()
                    && editor.local_history_availability().allows_browsing()
            });
            simple.set_enabled(enabled);
        }
    }

    /// Migrate local-history lineages after an in-app sidebar rename.
    pub(in crate::ui::window) fn migrate_local_history_after_rename(
        &self,
        old_path: &Path,
        new_path: &Path,
    ) {
        let old_path = old_path.to_path_buf();
        let new_path = new_path.to_path_buf();
        let old_for_move = old_path.clone();
        let new_for_move = new_path.clone();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                // One kind, but the same one-critical-section contract: the
                // ledger entry and the move share a lock, so a concurrent notes
                // or local-history rename of the same tree cannot observe this
                // one half-applied.
                migration_ledger::run_tracked_rename(
                    &data_dir,
                    &old_for_move,
                    &new_for_move,
                    &[MigrationKind::LocalHistory],
                    |rename| {
                        rename.run_kind(&data_dir, MigrationKind::LocalHistory, || {
                            local_history_service::move_path_tree(
                                &data_dir,
                                &old_for_move,
                                &new_for_move,
                            )
                        })
                    },
                )
            },
            move |(), result| {
                if let Err(error) = result {
                    tracing::error!(
                        "Failed to migrate local history for {} -> {}: {error}",
                        old_path.display(),
                        new_path.display()
                    );
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message(
                            "Rename succeeded, but local history could not be moved",
                            MessageKind::Warning,
                        );
                    }
                }
            },
        );
    }
}
