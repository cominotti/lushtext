// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for local-history recovery and migration retry behavior.

use lushtext_core::model::local_history::LocalHistorySnapshotOrigin;
use lushtext_core::model::migration_ledger::MigrationKind;
use lushtext_core::services::{
    filesystem::{fixture, metadata as fs_metadata, mutate as fs_mutate},
    local_history_service, migration_ledger,
};

use crate::common::TestContext;

#[test]
fn local_history_migration_failure_survives_restart_and_retry_preserves_snapshots() {
    let ctx = TestContext::new();
    let old_file = ctx.write_file("workspace/src/old.rs", "fn old_name() {}\n");
    let new_file = ctx.path().join("workspace/src/new.rs");
    local_history_service::capture_snapshot_for_path(
        ctx.data_dir(),
        &old_file,
        "old history\n",
        LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("capture old local history");
    let old_identity =
        local_history_service::resolve_document_identity(&old_file).expect("old identity");
    let old_history_dir =
        local_history_service::local_history_dir(ctx.data_dir()).join(old_identity.sidecar_id);

    fixture::rename(&old_file, &new_file);
    let new_identity =
        local_history_service::resolve_document_identity(&new_file).expect("new identity");
    let new_history_dir =
        local_history_service::local_history_dir(ctx.data_dir()).join(new_identity.sidecar_id);
    fixture::create_dir_all(new_history_dir.parent().expect("history parent"));
    fixture::write_text(&new_history_dir, "blocks first retry\n");
    migration_ledger::record_pending(
        ctx.data_dir(),
        &old_file,
        &new_file,
        &[MigrationKind::LocalHistory],
    )
    .expect("record pending local-history migration");

    let failed_retry = migration_ledger::reconcile_pending(ctx.data_dir())
        .expect("blocked local-history migration should stay diagnostic");

    assert_eq!(failed_retry.attempted, 1);
    assert_eq!(failed_retry.completed, 0);
    assert_eq!(failed_retry.diagnostics.len(), 1);
    assert!(fs_metadata::exists(&old_history_dir));
    assert_eq!(
        migration_ledger::load_recovering(ctx.data_dir())
            .value
            .entries
            .len(),
        1
    );

    fs_mutate::remove_file_if_exists(&new_history_dir).expect("remove blocking file");
    let successful_retry =
        migration_ledger::reconcile_pending(ctx.data_dir()).expect("retry migration");

    assert_eq!(successful_retry.attempted, 1);
    assert_eq!(successful_retry.completed, 1);
    assert!(successful_retry.diagnostics.is_empty());
    assert!(!fs_metadata::exists(&old_history_dir));
    let snapshots = local_history_service::list_snapshots_for_path(ctx.data_dir(), &new_file)
        .expect("list migrated local history");
    assert_eq!(snapshots.len(), 1);
    let loaded = local_history_service::load_snapshot_for_path(
        ctx.data_dir(),
        &new_file,
        &snapshots[0].snapshot_id,
    )
    .expect("load migrated snapshot")
    .expect("migrated snapshot exists");
    assert_eq!(loaded.text, "old history\n");
    assert!(
        migration_ledger::load_recovering(ctx.data_dir())
            .value
            .entries
            .is_empty()
    );
}

#[test]
fn local_history_duplicate_lineages_merge_through_startup_retry() {
    let ctx = TestContext::new();
    let old_file = ctx.write_file("workspace/src/old.rs", "fn old_name() {}\n");
    let new_file = ctx.write_file("workspace/src/new.rs", "fn new_name() {}\n");
    local_history_service::capture_snapshot_for_path(
        ctx.data_dir(),
        &old_file,
        "old lineage\n",
        LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::PreserveDuplicate,
    )
    .expect("capture old local history");
    local_history_service::capture_snapshot_for_path(
        ctx.data_dir(),
        &new_file,
        "new lineage\n",
        LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::PreserveDuplicate,
    )
    .expect("capture new local history");
    let old_identity =
        local_history_service::resolve_document_identity(&old_file).expect("old identity");
    let old_history_dir =
        local_history_service::local_history_dir(ctx.data_dir()).join(old_identity.sidecar_id);
    migration_ledger::record_pending(
        ctx.data_dir(),
        &old_file,
        &new_file,
        &[MigrationKind::LocalHistory],
    )
    .expect("record pending local-history migration");

    let report = migration_ledger::reconcile_pending(ctx.data_dir()).expect("startup retry");

    assert_eq!(report.attempted, 1);
    assert_eq!(report.completed, 1);
    assert!(!fs_metadata::exists(&old_history_dir));
    let bodies = snapshot_bodies(ctx.data_dir(), &new_file);
    assert!(bodies.iter().any(|body| body == "old lineage\n"));
    assert!(bodies.iter().any(|body| body == "new lineage\n"));
}

#[test]
fn save_as_lineage_stays_separate_from_pending_rename_migration() {
    let ctx = TestContext::new();
    let old_file = ctx.write_file("workspace/src/old.rs", "fn old_name() {}\n");
    local_history_service::capture_snapshot_for_path(
        ctx.data_dir(),
        &old_file,
        "rename lineage\n",
        LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::PreserveDuplicate,
    )
    .expect("capture old local history");
    let renamed_file = ctx.path().join("workspace/src/renamed.rs");
    fixture::rename(&old_file, &renamed_file);
    migration_ledger::record_pending(
        ctx.data_dir(),
        &old_file,
        &renamed_file,
        &[MigrationKind::LocalHistory],
    )
    .expect("record pending rename");

    let save_as_file = ctx.write_file("workspace/src/save-as.rs", "fn save_as() {}\n");
    local_history_service::capture_snapshot_for_path(
        ctx.data_dir(),
        &save_as_file,
        "save as lineage\n",
        LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::PreserveDuplicate,
    )
    .expect("capture Save As local history");

    let report = migration_ledger::reconcile_pending(ctx.data_dir()).expect("startup retry");

    assert_eq!(report.completed, 1);
    let renamed_bodies = snapshot_bodies(ctx.data_dir(), &renamed_file);
    let save_as_bodies = snapshot_bodies(ctx.data_dir(), &save_as_file);
    assert!(renamed_bodies.iter().any(|body| body == "rename lineage\n"));
    assert!(
        !renamed_bodies
            .iter()
            .any(|body| body == "save as lineage\n"),
        "Save As history must not be consumed by rename migration"
    );
    assert!(
        save_as_bodies
            .iter()
            .any(|body| body == "save as lineage\n")
    );
    assert!(
        !save_as_bodies.iter().any(|body| body == "rename lineage\n"),
        "pending rename history must remain tied to the original rename"
    );
}

#[test]
fn local_history_cleanup_failure_remains_diagnostic_and_retryable() {
    let ctx = TestContext::new();
    let old_file = ctx.write_file("workspace/src/old.rs", "fn old_name() {}\n");
    let new_file = ctx.write_file("workspace/src/new.rs", "fn new_name() {}\n");
    local_history_service::capture_snapshot_for_path(
        ctx.data_dir(),
        &old_file,
        "old lineage\n",
        LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::PreserveDuplicate,
    )
    .expect("capture old local history");
    local_history_service::capture_snapshot_for_path(
        ctx.data_dir(),
        &new_file,
        "new lineage\n",
        LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::PreserveDuplicate,
    )
    .expect("capture new local history");
    let old_identity =
        local_history_service::resolve_document_identity(&old_file).expect("old identity");
    let old_history_dir =
        local_history_service::local_history_dir(ctx.data_dir()).join(old_identity.sidecar_id);
    migration_ledger::record_pending(
        ctx.data_dir(),
        &old_file,
        &new_file,
        &[MigrationKind::LocalHistory],
    )
    .expect("record pending local-history migration");

    local_history_service::fail_next_obsolete_lineage_cleanup_for_path_for_test(&old_history_dir);
    let failed_report = migration_ledger::reconcile_pending(ctx.data_dir())
        .expect("cleanup failure should stay diagnostic");

    assert_eq!(failed_report.attempted, 1);
    assert_eq!(failed_report.completed, 0);
    assert_eq!(failed_report.diagnostics.len(), 1);
    assert!(
        failed_report.diagnostics[0]
            .message
            .contains("failed to remove obsolete local-history lineage"),
        "unexpected diagnostic: {:?}",
        failed_report.diagnostics
    );
    assert!(fs_metadata::exists(&old_history_dir));
    assert_eq!(
        migration_ledger::load_recovering(ctx.data_dir())
            .value
            .entries
            .len(),
        1,
        "cleanup failure should keep retry state"
    );

    let successful_report =
        migration_ledger::reconcile_pending(ctx.data_dir()).expect("retry cleanup");

    assert_eq!(successful_report.completed, 1);
    assert!(!fs_metadata::exists(&old_history_dir));
    assert!(
        migration_ledger::load_recovering(ctx.data_dir())
            .value
            .entries
            .is_empty()
    );
}

fn snapshot_bodies(data_dir: &std::path::Path, path: &std::path::Path) -> Vec<String> {
    local_history_service::list_snapshots_for_path(data_dir, path)
        .expect("list snapshots")
        .into_iter()
        .map(|meta| {
            local_history_service::load_snapshot_for_path(data_dir, path, &meta.snapshot_id)
                .expect("load snapshot")
                .expect("snapshot exists")
                .text
        })
        .collect()
}
