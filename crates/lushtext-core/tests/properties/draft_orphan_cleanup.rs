// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated draft-orphan cleanup outcomes over deterministic fault fixtures.
//!
//! These cases exercise the public service contract without GTK: every reported
//! removal must be confirmed on disk, while status, deletion, and manifest-write
//! failures must preserve their evidence for a later trusted pass.

use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::services::draft_service::{
    self, DraftOrphanCleanupFailure, DraftOrphanCleanupFault, DraftOrphanCleanupRetentionReason,
};
use lushtext_core::services::filesystem::{fixture, metadata as fs_metadata};
use std::path::Path;
use tempfile::TempDir;

#[derive(Clone, Copy, Debug)]
enum GeneratedCleanupCase {
    ConfirmedDelete,
    StatusFailure,
    DeleteFailure,
    ManifestWriteFailure,
}

fn entry(id: &str) -> DraftEntry {
    DraftEntry {
        draft_id: id.to_string(),
        original_path: None,
        original_mtime_secs: None,
        saved_at_secs: 1,
    }
}

#[test]
#[cfg(unix)]
fn cleanup_never_reports_failed_evidence_as_removed() {
    for case in [
        GeneratedCleanupCase::ConfirmedDelete,
        GeneratedCleanupCase::StatusFailure,
        GeneratedCleanupCase::DeleteFailure,
        GeneratedCleanupCase::ManifestWriteFailure,
    ] {
        let temp = TempDir::new().expect("create property fixture");
        let data_dir = temp.path();
        let drafts = draft_service::drafts_dir(data_dir);

        let (plan, expected_failure, fault) = match case {
            GeneratedCleanupCase::ConfirmedDelete => {
                draft_service::write_draft(data_dir, "orphan", "body").expect("write orphan");
                (
                    draft_service::inspect_orphan_cleanup(data_dir, &DraftManifest::default())
                        .expect("inspect orphan"),
                    None,
                    None,
                )
            }
            GeneratedCleanupCase::StatusFailure => {
                fixture::create_dir_all(&drafts);
                fixture::symlink(Path::new("loop.draft"), &drafts.join("loop.draft"));
                let manifest = DraftManifest {
                    drafts: vec![entry("loop")],
                };
                draft_service::save_manifest(data_dir, &manifest).expect("save status manifest");
                (
                    draft_service::inspect_orphan_cleanup(data_dir, &manifest)
                        .expect("directory scan remains trusted"),
                    Some("status"),
                    None,
                )
            }
            GeneratedCleanupCase::DeleteFailure => {
                draft_service::write_draft(data_dir, "blocked", "body").expect("write body");
                let plan =
                    draft_service::inspect_orphan_cleanup(data_dir, &DraftManifest::default())
                        .expect("inspect blocked body");
                (plan, Some("delete"), Some(DraftOrphanCleanupFault::Delete))
            }
            GeneratedCleanupCase::ManifestWriteFailure => {
                let manifest = DraftManifest {
                    drafts: vec![entry("ghost")],
                };
                draft_service::save_manifest(data_dir, &manifest).expect("save manifest");
                let plan = draft_service::inspect_orphan_cleanup(data_dir, &manifest)
                    .expect("inspect missing body");
                (
                    plan,
                    Some("manifest"),
                    Some(DraftOrphanCleanupFault::Manifest),
                )
            }
        };

        let outcome = match fault {
            Some(fault) => {
                draft_service::execute_orphan_cleanup_with_fault_for_test(data_dir, plan, fault)
            }
            None => draft_service::execute_orphan_cleanup(data_dir, plan),
        };

        match expected_failure {
            None => {
                assert_eq!(outcome.confirmed_cleaned_count(), 1, "case: {case:?}");
                assert_eq!(outcome.deleted_files.len(), 1, "case: {case:?}");
            }
            Some(category) => {
                assert_eq!(outcome.confirmed_cleaned_count(), 0, "case: {case:?}");
                assert!(outcome.has_more_work, "case: {case:?}");
                assert!(!outcome.retained.is_empty(), "case: {case:?}");
                let category_present = outcome.failures.iter().any(|failure| {
                    matches!(
                        (category, failure),
                        ("status", DraftOrphanCleanupFailure::Status(_))
                            | ("delete", DraftOrphanCleanupFailure::Delete(_))
                            | ("manifest", DraftOrphanCleanupFailure::Manifest(_))
                    )
                });
                assert!(category_present, "case: {case:?}");
                assert!(
                    outcome.retained.iter().any(|retained| matches!(
                        retained.reason,
                        DraftOrphanCleanupRetentionReason::StatusUncertain
                            | DraftOrphanCleanupRetentionReason::DeleteFailed
                            | DraftOrphanCleanupRetentionReason::ManifestCommitFailed
                            | DraftOrphanCleanupRetentionReason::BodyNotRegularFile
                    )),
                    "case: {case:?}"
                );
            }
        }

        for deleted in &outcome.deleted_files {
            assert_eq!(
                fs_metadata::path_status(deleted).expect("deleted path status"),
                lushtext_core::services::filesystem::PathStatus::Missing,
                "case: {case:?}",
            );
        }
    }
}
