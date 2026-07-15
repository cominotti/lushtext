// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for session and draft JSON persistence models.
//!
//! The tests stay at the model boundary: they verify serde round trips for the
//! persisted shapes without starting a real application session or touching the
//! draft-service filesystem workflow.

use std::path::{Path, PathBuf};

use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::model::session::{SessionData, SessionTab};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use crate::support;

/// Stable absolute folder for generated model paths.
const GENERATED_PATH_FOLDER: &str = "/workspace/property-folder";
/// Maximum cursor and scroll positions generated for session tabs.
///
/// The value is larger than normal examples but small enough to keep shrunk
/// failures readable.
const MAX_SESSION_POSITION: u32 = 20_000;
/// Maximum timestamp used by draft manifest properties.
///
/// Draft timestamps are seconds since epoch in production. The 32-bit ceiling
/// keeps generated JSON compact while covering a recognizable upper-bound
/// timestamp instead of an arbitrary small fixture value.
const MAX_DRAFT_TIMESTAMP: u64 = 0xFFFF_FFFF;

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn session_tabs_json_roundtrip(tab in session_tab()) {
        let json = serde_json::to_string(&tab)
            .map_err(|error| TestCaseError::fail(format!("session tab serialize failed: {error}")))?;
        let decoded: SessionTab = serde_json::from_str(&json)
            .map_err(|error| TestCaseError::fail(format!("session tab deserialize failed: {error}")))?;

        assert_session_tab_roundtripped(&tab, &decoded)?;
    }

    #[test]
    fn session_data_json_roundtrip(session in session_data()) {
        let json = serde_json::to_string(&session)
            .map_err(|error| TestCaseError::fail(format!("session serialize failed: {error}")))?;
        let decoded: SessionData = serde_json::from_str(&json)
            .map_err(|error| TestCaseError::fail(format!("session deserialize failed: {error}")))?;

        assert_session_data_roundtripped(&session, &decoded)?;
    }

    #[test]
    fn draft_entries_json_roundtrip(entry in draft_entry()) {
        let json = serde_json::to_string(&entry)
            .map_err(|error| TestCaseError::fail(format!("draft entry serialize failed: {error}")))?;
        let decoded: DraftEntry = serde_json::from_str(&json)
            .map_err(|error| TestCaseError::fail(format!("draft entry deserialize failed: {error}")))?;

        prop_assert_eq!(decoded, entry);
    }

    #[test]
    fn draft_manifests_json_roundtrip(manifest in draft_manifest()) {
        let json = serde_json::to_string(&manifest)
            .map_err(|error| TestCaseError::fail(format!("draft manifest serialize failed: {error}")))?;
        let decoded: DraftManifest = serde_json::from_str(&json)
            .map_err(|error| TestCaseError::fail(format!("draft manifest deserialize failed: {error}")))?;

        prop_assert_eq!(decoded, manifest);
    }
}

/// Generate a full session, including empty tab lists and out-of-range active indices.
fn session_data() -> impl Strategy<Value = SessionData> {
    (
        prop::collection::vec(session_tab(), 0..=support::MAX_VECTOR_LEN),
        prop::option::of(0usize..=support::MAX_VECTOR_LEN + 2),
    )
        .prop_map(|(tabs, active_tab_index)| SessionData {
            tabs,
            active_tab_index,
        })
}

/// Generate file-backed and untitled tabs with the persisted fields LushText restores.
fn session_tab() -> impl Strategy<Value = SessionTab> {
    (
        prop::option::of(generated_path()),
        prop::option::of(draft_id()),
        0u32..=MAX_SESSION_POSITION,
        0u32..=MAX_SESSION_POSITION,
        0u32..=MAX_SESSION_POSITION,
        any::<bool>(),
    )
        .prop_map(
            |(path, draft_id, cursor_line, cursor_col, scroll_line, pinned)| SessionTab {
                path,
                draft_id,
                cursor_line,
                cursor_col,
                scroll_line,
                pinned,
            },
        )
}

/// Generate a draft manifest while preserving the generated entry order.
fn draft_manifest() -> impl Strategy<Value = DraftManifest> {
    prop::collection::vec(draft_entry(), 0..=support::MAX_VECTOR_LEN).prop_map(|drafts| {
        DraftManifest {
            drafts,
            cleanup_continuation: None,
        }
    })
}

/// Generate one draft manifest entry with optional backing-file metadata.
fn draft_entry() -> impl Strategy<Value = DraftEntry> {
    (
        draft_id(),
        prop::option::of(generated_path()),
        prop::option::of(0u64..=MAX_DRAFT_TIMESTAMP),
        0u64..=MAX_DRAFT_TIMESTAMP,
    )
        .prop_map(
            |(draft_id, original_path, original_mtime_secs, saved_at_secs)| DraftEntry {
                draft_id,
                original_path,
                original_mtime_secs,
                saved_at_secs,
            },
        )
}

/// Generate a stable draft identifier that stays filename-safe.
fn draft_id() -> impl Strategy<Value = String> {
    (0u32..=999_999).prop_map(|id| format!("draft-{id:06}"))
}

/// Generate an absolute path under a synthetic workspace folder.
fn generated_path() -> impl Strategy<Value = PathBuf> {
    support::path_suffix()
        .prop_map(|suffix| append_suffix(Path::new(GENERATED_PATH_FOLDER), &suffix))
}

/// Append a generated relative suffix to a stable absolute folder.
fn append_suffix(folder: &Path, suffix: &Path) -> PathBuf {
    if suffix.as_os_str().is_empty() {
        folder.to_path_buf()
    } else {
        folder.join(suffix)
    }
}

/// Compare session data field-by-field because the production model does not derive `PartialEq`.
fn assert_session_data_roundtripped(
    expected: &SessionData,
    actual: &SessionData,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(actual.active_tab_index, expected.active_tab_index);
    prop_assert_eq!(actual.tabs.len(), expected.tabs.len());
    for (expected_tab, actual_tab) in expected.tabs.iter().zip(&actual.tabs) {
        assert_session_tab_roundtripped(expected_tab, actual_tab)?;
    }
    Ok(())
}

/// Compare one session tab field-by-field without changing the production derives.
fn assert_session_tab_roundtripped(
    expected: &SessionTab,
    actual: &SessionTab,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(&actual.path, &expected.path);
    prop_assert_eq!(&actual.draft_id, &expected.draft_id);
    prop_assert_eq!(actual.cursor_line, expected.cursor_line);
    prop_assert_eq!(actual.cursor_col, expected.cursor_col);
    prop_assert_eq!(actual.scroll_line, expected.scroll_line);
    prop_assert_eq!(actual.pinned, expected.pinned);
    Ok(())
}
