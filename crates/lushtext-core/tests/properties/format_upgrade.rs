// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated malformed-byte coverage for format-upgrade inventory.
//!
//! Classification is intentionally read-only. Arbitrary bytes may be damaged or
//! unsupported, but they must never panic and must not create backup state.

use lushtext_core::services::filesystem::{fixture, metadata as fs_metadata};
use lushtext_core::services::format_upgrade::{
    FORMAT_UPGRADE_BACKUP_DIR, FormatClassification, build_plan, scan,
};
use proptest::prelude::*;
use tempfile::TempDir;

use crate::support;

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn generated_session_bytes_classify_without_writes(
        bytes in proptest::collection::vec(any::<u8>(), 0..=support::MAX_BYTE_VECTOR_LEN),
    ) {
        let dir = TempDir::new().map_err(|error| TestCaseError::fail(error.to_string()))?;
        let path = dir.path().join("session.json");
        fixture::write_bytes(&path, &bytes);

        let inventory = scan(dir.path());
        let plan = build_plan(&inventory);

        let session = inventory
            .items
            .iter()
            .find(|item| item.path.relative() == std::path::Path::new("session.json"))
            .ok_or_else(|| TestCaseError::fail("session item missing"))?;
        let classification_is_known = matches!(
            session.classification,
            FormatClassification::Current { .. }
                | FormatClassification::Upgradeable { .. }
                | FormatClassification::FutureVersion { .. }
                | FormatClassification::UnsupportedOld { .. }
                | FormatClassification::Damaged { .. }
                | FormatClassification::UnsafeToReplace { .. }
        );
        prop_assert!(classification_is_known);
        prop_assert_eq!(fixture::read_bytes(&path), bytes);
        prop_assert!(!fs_metadata::exists(&dir.path().join(FORMAT_UPGRADE_BACKUP_DIR)));
        let _ = plan;
    }
}
