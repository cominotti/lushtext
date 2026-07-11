// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated draft-orphan cleanup outcomes over deterministic fault fixtures.
//!
//! These cases exercise the public service contract without GTK: every reported
//! removal must be confirmed on disk, while status, deletion, and manifest-write
//! failures must preserve their evidence for a later trusted pass.

use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::services::draft_service::{
    self, DraftOrphanCleanupFailure, DraftOrphanCleanupRetentionReason,
};
use lushtext_core::services::filesystem::{fixture, metadata as fs_metadata};
use proptest::prelude::*;
use std::path::Path;
use tempfile::TempDir;

use crate::support::property_config;

#[derive(Clone, Copy, Debug)]
enum GeneratedCleanupCase {
    ConfirmedDelete,
    StatusFailure,
    DeleteFailure,
    ManifestWriteFailure,
}

/// Generate success or one failure category that must remain retryable.
fn generated_case() -> impl Strategy<Value = GeneratedCleanupCase> {
    prop_oneof![
        Just(GeneratedCleanupCase::ConfirmedDelete),
        Just(GeneratedCleanupCase::StatusFailure),
        Just(GeneratedCleanupCase::DeleteFailure),
        Just(GeneratedCleanupCase::ManifestWriteFailure),
    ]
}

fn entry(id: &str) -> DraftEntry {
    DraftEntry {
        draft_id: id.to_string(),
        original_path: None,
        original_mtime_secs: None,
        saved_at_secs: 1,
    }
}

proptest! {
    #![proptest_config(property_config())]

    #[test]
    #[cfg(unix)]
    fn cleanup_never_reports_failed_evidence_as_removed(case in generated_case()) {
        let temp = TempDir::new().expect("create property fixture");
        let data_dir = temp.path();
        let drafts = draft_service::drafts_dir(data_dir);

        let (plan, expected_failure) = match case {
            GeneratedCleanupCase::ConfirmedDelete => {
                draft_service::write_draft(data_dir, "orphan", "body").expect("write orphan");
                (
                    draft_service::inspect_orphan_cleanup(data_dir, &DraftManifest::default())
                        .expect("inspect orphan"),
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
                )
            }
            GeneratedCleanupCase::DeleteFailure => {
                draft_service::write_draft(data_dir, "blocked", "body").expect("write body");
                let plan = draft_service::inspect_orphan_cleanup(
                    data_dir,
                    &DraftManifest::default(),
                )
                .expect("inspect blocked body");
                // Remove directory write permission to force the mutation
                // failure; owner access is restored immediately after execution.
                fixture::set_mode(&drafts, 0o555);
                (plan, Some("delete"))
            }
            GeneratedCleanupCase::ManifestWriteFailure => {
                let manifest = DraftManifest {
                    drafts: vec![entry("ghost")],
                };
                draft_service::save_manifest(data_dir, &manifest).expect("save manifest");
                let plan = draft_service::inspect_orphan_cleanup(data_dir, &manifest)
                    .expect("inspect missing body");
                // The same permission guard makes durable manifest replacement
                // fail without making the existing manifest unreadable.
                fixture::set_mode(&drafts, 0o555);
                (plan, Some("manifest"))
            }
        };

        let outcome = draft_service::execute_orphan_cleanup(data_dir, plan);
        fixture::set_mode(&drafts, 0o700);

        match expected_failure {
            None => {
                prop_assert_eq!(outcome.confirmed_cleaned_count(), 1);
                prop_assert_eq!(outcome.deleted_files.len(), 1);
            }
            Some(category) => {
                prop_assert_eq!(outcome.confirmed_cleaned_count(), 0);
                prop_assert!(outcome.has_more_work);
                prop_assert!(!outcome.retained.is_empty());
                let category_present = outcome.failures.iter().any(|failure| matches!(
                    (category, failure),
                    ("status", DraftOrphanCleanupFailure::Status(_))
                        | ("delete", DraftOrphanCleanupFailure::Delete(_))
                        | ("manifest", DraftOrphanCleanupFailure::Manifest(_))
                ));
                prop_assert!(category_present);
                prop_assert!(outcome.retained.iter().any(|retained| matches!(
                    retained.reason,
                    DraftOrphanCleanupRetentionReason::StatusUncertain
                        | DraftOrphanCleanupRetentionReason::DeleteFailed
                        | DraftOrphanCleanupRetentionReason::ManifestCommitFailed
                        | DraftOrphanCleanupRetentionReason::BodyNotRegularFile
                )));
            }
        }

        for deleted in &outcome.deleted_files {
            prop_assert_eq!(
                fs_metadata::path_status(deleted).expect("deleted path status"),
                lushtext_core::services::filesystem::PathStatus::Missing
            );
        }
    }
}
