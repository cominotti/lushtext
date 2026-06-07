// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated malformed-byte coverage for recovery metadata loaders.
//!
//! The property stays display-free and uses tiny fixtures so the PR-friendly
//! property lane can prove that damaged app-owned JSON never panics or gets
//! overwritten before preservation.

use lushtext_core::services::filesystem::fixture;
use lushtext_core::services::recovery_metadata::{
    RecoveryLoad, RecoveryLoadConfig, RecoveryLoadOutcome, RecoveryMetadataClass,
    load_json_or_default,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

use crate::support;

#[derive(Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
struct GeneratedMetadata {
    value: String,
}

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn malformed_metadata_bytes_never_panic_or_lose_evidence(
        bytes in proptest::collection::vec(any::<u8>(), 0..=support::MAX_BYTE_VECTOR_LEN),
    ) {
        let dir = TempDir::new().map_err(|error| TestCaseError::fail(error.to_string()))?;
        let path = dir.path().join("session.json");
        fixture::write_bytes(&path, &bytes);

        let result: RecoveryLoad<GeneratedMetadata> = load_json_or_default(
            &RecoveryLoadConfig::new(dir.path(), &path, RecoveryMetadataClass::Session),
        );

        match result.outcome {
            RecoveryLoadOutcome::Loaded => {
                prop_assert!(result.diagnostics.is_empty());
            }
            RecoveryLoadOutcome::QuarantinedDefault | RecoveryLoadOutcome::PreservedDefault => {
                prop_assert!(!result.diagnostics.is_empty());
                let preserved = result.diagnostics.iter().any(|diagnostic| {
                    diagnostic
                        .preservation
                        .quarantine_path()
                        .is_some_and(|quarantine_path| fixture::read_bytes(quarantine_path) == bytes)
                        || fixture::exists(&path) && fixture::read_bytes(&path) == bytes
                });
                prop_assert!(preserved);
            }
            RecoveryLoadOutcome::MissingDefault | RecoveryLoadOutcome::Partial => {
                prop_assert!(false, "generated existing file should not produce {:?}", result.outcome);
            }
        }
    }
}
