// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated invalid-state coverage for Replace All undo journals.
//!
//! These cases build tiny on-disk journals and then damage one part of the
//! commit protocol. The property proves recovery never exposes an undo backup
//! unless the active manifest and every referenced entry validate together.

use std::path::{Path, PathBuf};

use lushtext_core::services::content_search::{ReplaceUndoBackup, ReplaceUndoEntry};
use lushtext_core::services::filesystem::fixture;
use lushtext_core::services::search_backup;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use tempfile::TempDir;

use crate::support;

const BACKUP_FILE: &str = "replace-backup.json";
const JOURNAL_DIR: &str = "replace-backup-journal";
const JOURNAL_MANIFEST_FILE: &str = "manifest.json";
const CLEANUP_MARKER_FILE: &str = "cleanup-in-progress.json";

#[derive(Debug, Clone, Copy)]
enum InvalidJournalState {
    MissingManifest,
    MissingEntry,
    CorruptEntry,
    OrphanEntry,
    CleanupMarker,
    CorruptLegacyBackup,
    UnsupportedJournalPath,
}

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn invalid_journal_states_never_activate_undo(
        state in invalid_journal_state(),
        entries in generated_entries(),
    ) {
        let dir = TempDir::new()
            .map_err(|error| TestCaseError::fail(format!("tempdir creation failed: {error}")))?;

        write_invalid_state(dir.path(), state, &entries)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;

        let recovery = search_backup::load_recovering(dir.path());

        prop_assert!(!recovery.active);
        prop_assert!(recovery.backup.is_empty());
        prop_assert!(!recovery.diagnostics.is_empty());
        prop_assert!(
            search_backup::load(dir.path())
                .map_err(|error| TestCaseError::fail(error.to_string()))?
                .is_empty()
        );
    }
}

fn invalid_journal_state() -> impl Strategy<Value = InvalidJournalState> {
    prop_oneof![
        Just(InvalidJournalState::MissingManifest),
        Just(InvalidJournalState::MissingEntry),
        Just(InvalidJournalState::CorruptEntry),
        Just(InvalidJournalState::OrphanEntry),
        Just(InvalidJournalState::CleanupMarker),
        Just(InvalidJournalState::CorruptLegacyBackup),
        Just(InvalidJournalState::UnsupportedJournalPath),
    ]
}

fn generated_entries() -> impl Strategy<Value = Vec<(String, String)>> {
    prop::collection::vec(
        (
            support::optional_text_fragment(),
            support::optional_text_fragment(),
        ),
        1..=3,
    )
}

fn write_invalid_state(
    data_dir: &Path,
    state: InvalidJournalState,
    entries: &[(String, String)],
) -> anyhow::Result<()> {
    match state {
        InvalidJournalState::MissingManifest => write_entries_without_manifest(data_dir, entries),
        InvalidJournalState::MissingEntry => {
            write_active_journal(data_dir, entries)?;
            fixture::remove_file(&first_entry_file(data_dir));
            Ok(())
        }
        InvalidJournalState::CorruptEntry => {
            write_active_journal(data_dir, entries)?;
            fixture::write_text(&first_entry_file(data_dir), "not valid json {{{");
            Ok(())
        }
        InvalidJournalState::OrphanEntry => {
            write_active_journal(data_dir, entries)?;
            let orphan = data_dir.join(JOURNAL_DIR).join("orphan.json");
            fixture::write_text(
                &orphan,
                r#"{"path":"/tmp/lushtext-orphan","original_content":"old","replaced_content":"new"}"#,
            );
            Ok(())
        }
        InvalidJournalState::CleanupMarker => {
            write_active_journal(data_dir, entries)?;
            fixture::write_text(
                &data_dir.join(JOURNAL_DIR).join(CLEANUP_MARKER_FILE),
                r#"{"reason":"property interrupted cleanup"}"#,
            );
            Ok(())
        }
        InvalidJournalState::CorruptLegacyBackup => {
            fixture::write_text(&data_dir.join(BACKUP_FILE), "not valid json {{{");
            Ok(())
        }
        InvalidJournalState::UnsupportedJournalPath => {
            fixture::write_text(&data_dir.join(JOURNAL_DIR), "not a directory");
            Ok(())
        }
    }
}

fn write_entries_without_manifest(
    data_dir: &Path,
    entries: &[(String, String)],
) -> anyhow::Result<()> {
    for (index, (original, replaced)) in entries.iter().enumerate() {
        let path = generated_path(index);
        let entry =
            ReplaceUndoEntry::new(original.clone().into_bytes(), replaced.clone().into_bytes());
        search_backup::save_entry(data_dir, &path, &entry)?;
    }
    Ok(())
}

fn write_active_journal(data_dir: &Path, entries: &[(String, String)]) -> anyhow::Result<()> {
    let mut backup = ReplaceUndoBackup::new();
    for (index, (original, replaced)) in entries.iter().enumerate() {
        backup.insert(
            generated_path(index),
            ReplaceUndoEntry::new(original.clone().into_bytes(), replaced.clone().into_bytes()),
        );
    }
    search_backup::save(data_dir, &backup)
}

fn generated_path(index: usize) -> PathBuf {
    PathBuf::from(format!("/tmp/lushtext-generated-replace-{index}.txt"))
}

fn first_entry_file(data_dir: &Path) -> PathBuf {
    let journal_dir = data_dir.join(JOURNAL_DIR);
    let entry = fixture::entry_names(&journal_dir)
        .into_iter()
        .find(|name| {
            Path::new(name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
                && name != JOURNAL_MANIFEST_FILE
                && name != CLEANUP_MARKER_FILE
        })
        .expect("generated journal should contain at least one entry file");
    journal_dir.join(entry)
}
