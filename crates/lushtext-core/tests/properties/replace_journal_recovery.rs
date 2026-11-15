// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated invalid-state coverage for Replace All undo journals.
//!
//! These cases build tiny on-disk journals, streamed (bodies plus descriptor
//! entries) or legacy (inline entries, as a build before streamed entries
//! wrote them), and then damage one part of the commit protocol. The property
//! proves recovery never exposes an undo backup unless the manifest, every
//! referenced entry and every body validate together, and that the benign
//! leftovers a crash leaves (an orphan body) never cost Undo.

use std::path::{Path, PathBuf};

use lushtext_core::services::content_search::undo_replacements;
use lushtext_core::services::filesystem::fixture;
use lushtext_core::services::search_backup;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use tempfile::TempDir;

use crate::support;

const BACKUP_FILE: &str = "replace-backup.json";
const JOURNAL_DIR: &str = "replace-backup-journal";
const BODIES_DIR: &str = "bodies";
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
    MissingBody,
    TruncatedBody,
    MixedKinds,
}

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn invalid_journal_states_never_activate_undo(
        state in invalid_journal_state(),
        entries in generated_entries(),
        legacy in any::<bool>(),
    ) {
        let dir = TempDir::new()
            .map_err(|error| TestCaseError::fail(format!("tempdir creation failed: {error}")))?;

        write_invalid_state(dir.path(), state, &entries, legacy)
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

    #[test]
    fn orphan_journal_entries_do_not_block_valid_manifest(
        entries in generated_entries(),
    ) {
        let dir = TempDir::new()
            .map_err(|error| TestCaseError::fail(format!("tempdir creation failed: {error}")))?;

        write_invalid_state(dir.path(), InvalidJournalState::OrphanEntry, &entries, true)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;

        let recovery = search_backup::load_recovering(dir.path());

        prop_assert!(recovery.active);
        prop_assert!(!recovery.backup.is_empty());
        prop_assert!(!recovery.diagnostics.is_empty());
    }

    /// An orphan body (a crash between a body and its entry) is benign: the
    /// journal stays active with no diagnostic (design D9).
    #[test]
    fn an_orphan_body_never_costs_undo(entries in generated_entries()) {
        let dir = TempDir::new()
            .map_err(|error| TestCaseError::fail(format!("tempdir creation failed: {error}")))?;
        write_journal(dir.path(), &entries, false)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        fixture::write_text(
            &dir.path().join(JOURNAL_DIR).join(BODIES_DIR).join("0123abcd.body"),
            "a body whose entry was never written",
        );

        let recovery = search_backup::load_recovering(dir.path());

        prop_assert!(recovery.active);
        prop_assert_eq!(recovery.backup.len(), entries.len());
        prop_assert!(recovery.diagnostics.is_empty());
    }

    /// A legacy journal written before streamed entries loads as descriptors
    /// and its Undo restores every file (design D6), manifest-listed or
    /// incremental (as `main`'s Replace All runs wrote it).
    #[test]
    fn a_legacy_journal_loads_and_undoes(
        entries in generated_entries(),
        incremental in any::<bool>(),
    ) {
        let dir = TempDir::new()
            .map_err(|error| TestCaseError::fail(format!("tempdir creation failed: {error}")))?;
        let targets = TempDir::new()
            .map_err(|error| TestCaseError::fail(format!("tempdir creation failed: {error}")))?;
        let files: Vec<(PathBuf, &[u8], &[u8])> = entries
            .iter()
            .enumerate()
            .map(|(index, (original, replaced))| {
                (
                    targets.path().join(format!("legacy-{index}.txt")),
                    original.as_bytes(),
                    replaced.as_bytes(),
                )
            })
            .collect();
        for (path, _, replaced) in &files {
            fixture::write_bytes(path, replaced);
        }
        let borrowed: Vec<(&Path, &[u8], &[u8])> = files
            .iter()
            .map(|(path, original, replaced)| (path.as_path(), *original, *replaced))
            .collect();
        let backup = search_backup::fixture::write_legacy_journal(dir.path(), &borrowed, incremental)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(backup.len(), files.len());

        let outcome = undo_replacements(&backup);

        prop_assert_eq!(outcome.restored_count(), files.len());
        for (path, original, _) in &files {
            prop_assert_eq!(fixture::read_bytes(path), original.to_vec());
        }
    }
}

fn invalid_journal_state() -> impl Strategy<Value = InvalidJournalState> {
    prop_oneof![
        Just(InvalidJournalState::MissingManifest),
        Just(InvalidJournalState::MissingEntry),
        Just(InvalidJournalState::CorruptEntry),
        Just(InvalidJournalState::CleanupMarker),
        Just(InvalidJournalState::CorruptLegacyBackup),
        Just(InvalidJournalState::UnsupportedJournalPath),
        Just(InvalidJournalState::MissingBody),
        Just(InvalidJournalState::TruncatedBody),
        Just(InvalidJournalState::MixedKinds),
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
    legacy: bool,
) -> anyhow::Result<()> {
    match state {
        InvalidJournalState::MissingManifest => {
            write_journal(data_dir, entries, legacy)?;
            fixture::remove_file(&data_dir.join(JOURNAL_DIR).join(JOURNAL_MANIFEST_FILE));
            Ok(())
        }
        InvalidJournalState::MissingEntry if !legacy => {
            // An incremental journal has no list to miss an entry from; its
            // equivalent is an entry whose body is gone.
            write_journal(data_dir, entries, false)?;
            fixture::remove_file(&first_body_file(data_dir));
            Ok(())
        }
        InvalidJournalState::MissingEntry => {
            write_journal(data_dir, entries, true)?;
            fixture::remove_file(&first_entry_file(data_dir));
            Ok(())
        }
        InvalidJournalState::CorruptEntry => {
            write_journal(data_dir, entries, legacy)?;
            fixture::write_text(&first_entry_file(data_dir), "not valid json {{{");
            Ok(())
        }
        InvalidJournalState::OrphanEntry => {
            write_journal(data_dir, entries, true)?;
            let orphan = data_dir.join(JOURNAL_DIR).join("orphan.json");
            fixture::write_text(
                &orphan,
                r#"{"path":"/tmp/lushtext-orphan","original_content":"old","replaced_content":"new"}"#,
            );
            Ok(())
        }
        InvalidJournalState::CleanupMarker => {
            write_journal(data_dir, entries, legacy)?;
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
        InvalidJournalState::MissingBody => {
            write_journal(data_dir, entries, false)?;
            fixture::remove_file(&first_body_file(data_dir));
            Ok(())
        }
        InvalidJournalState::TruncatedBody => {
            write_journal(data_dir, entries, false)?;
            let body = first_body_file(data_dir);
            let mut bytes = fixture::read_bytes(&body);
            if bytes.pop().is_none() {
                bytes.extend_from_slice(b"grown");
            }
            fixture::write_bytes(&body, &bytes);
            Ok(())
        }
        InvalidJournalState::MixedKinds => {
            // A streamed journal with one legacy entry beside its entries.
            write_journal(data_dir, entries, false)?;
            let stray = TempDir::new()?;
            search_backup::fixture::write_legacy_journal(
                stray.path(),
                &[(Path::new("/tmp/lushtext-mixed-legacy.txt"), b"old", b"new")],
                true,
            )?;
            let legacy_entry = first_entry_file(stray.path());
            let name = legacy_entry
                .file_name()
                .expect("the legacy entry has a name")
                .to_owned();
            fixture::write_bytes(
                &data_dir.join(JOURNAL_DIR).join(name),
                fixture::read_bytes(&legacy_entry),
            );
            Ok(())
        }
    }
}

/// An active journal of the generated entries: streamed (incremental, as a
/// run writes one) or legacy (manifest-listed, as `main`'s `save` wrote one).
fn write_journal(
    data_dir: &Path,
    entries: &[(String, String)],
    legacy: bool,
) -> anyhow::Result<()> {
    let paths: Vec<PathBuf> = (0..entries.len()).map(generated_path).collect();
    let files: Vec<(&Path, &[u8], &[u8])> = paths
        .iter()
        .zip(entries)
        .map(|(path, (original, replaced))| {
            (path.as_path(), original.as_bytes(), replaced.as_bytes())
        })
        .collect();
    if legacy {
        search_backup::fixture::write_legacy_journal(data_dir, &files, false)?;
    } else {
        search_backup::fixture::write_stream_journal(data_dir, &files)?;
    }
    Ok(())
}

fn generated_path(index: usize) -> PathBuf {
    PathBuf::from(format!("/tmp/lushtext-generated-replace-{index}.txt"))
}

fn first_body_file(data_dir: &Path) -> PathBuf {
    let bodies = data_dir.join(JOURNAL_DIR).join(BODIES_DIR);
    let name = fixture::entry_names(&bodies)
        .into_iter()
        .next()
        .expect("a streamed journal has a body");
    bodies.join(name)
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
