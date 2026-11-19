// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace All undo and cancellation rollback: the shell of
//! [`ReplaceUndoProtocol`] (design D6).
//!
//! Each entry runs U0–U3 under its target guard, through
//! `protocol_drive::drive`: take facts, stream-hash the
//! current file, admit and stream the original back from its body (or, for a
//! legacy entry, from the entry JSON re-read on this worker), then retire the
//! entry before its body. Nothing here holds a whole file.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::admission::disk_admits;
use super::apply::digest_file;
use super::protocol::{CurrentContent, ReplaceUndoProtocol, UndoAction, UndoEnd, UndoOutcome};
use super::stream::{DigestSink, Sha256Sink, effective_stream_chunk_bytes};
use super::{
    ContentDigest, ReplaceUndoBackup, ReplaceUndoEntry, UndoOriginal, UndoReplaceOutcome,
    effective_main_replace_undo_bytes, effective_max_replace_file_bytes,
    run_undo_after_metadata_hook_for_test,
};
use crate::model::content_search::BoundedDiagnosticSample;
use crate::services::filesystem::backend_tap::ProtocolKind;
use crate::services::filesystem::leftovers::{LaunchId, sweep_journal_owned_leftovers};
use crate::services::filesystem::read::{BoundedStreamError, stream_bounded};
use crate::services::filesystem::types::FileFacts;
use crate::services::filesystem::{WriteLabel, metadata as fs_metadata, write as fs_write};
use crate::services::kill_point::{self, KillWindow};
use crate::services::protocol_drive::{Protocol, drive};
use crate::services::search_backup;

/// Restore files from backup (undo Replace All).
///
/// Writes each file atomically, but only when the file's current bytes still
/// hash to what Replace All wrote. Per-file failures stay in
/// `remaining_backup` so the UI can keep undo available for retry.
#[must_use]
pub fn undo_replacements(backup: &ReplaceUndoBackup) -> UndoReplaceOutcome {
    undo_replacements_for_open_identities(backup, &HashSet::new())
}

/// Restore a backup and return only restored paths that intersect open tabs.
#[must_use]
pub fn undo_replacements_for_open_identities(
    backup: &ReplaceUndoBackup,
    open_canonical_identities: &HashSet<PathBuf>,
) -> UndoReplaceOutcome {
    // The journal guard serializes Undo with a run and the journal disk jobs:
    // Undo shrinks the journal and may re-arm it (design D3a, D6).
    let guard = search_backup::acquire_journal_guard();
    let mut restored_count = 0usize;
    let mut skipped_paths = BoundedDiagnosticSample::default();
    let mut failed_paths = BoundedDiagnosticSample::default();
    let mut restored_open_paths = Vec::new();
    let mut remaining_backup = ReplaceUndoBackup::new();
    if guard.is_err() {
        for (path, entry) in backup {
            failed_paths.record_path(path);
            remaining_backup.insert(path.clone(), entry.clone());
        }
    } else {
        let mut session = UndoSession::new(backup);
        for (path, entry) in backup {
            let (end, facts) = session.undo_entry(path, entry, &mut |_| {});
            match end {
                UndoEnd::Restored { .. } => {
                    restored_count = restored_count.saturating_add(1);
                    record_open_path_intersection(
                        &mut restored_open_paths,
                        path,
                        facts
                            .as_ref()
                            .and_then(|facts| facts.canonical_path.as_deref()),
                        open_canonical_identities,
                    );
                }
                UndoEnd::Diverged => {
                    skipped_paths.record_path(path);
                    remaining_backup.insert(path.clone(), entry.clone());
                }
                UndoEnd::Failed => {
                    failed_paths.record_path(path);
                    remaining_backup.insert(path.clone(), entry.clone());
                }
            }
        }
    }
    drop(guard);

    UndoReplaceOutcome {
        restored_count,
        skipped_count: skipped_paths.total_count(),
        failed_count: failed_paths.total_count(),
        restored_open_paths,
        skipped_sample: skipped_paths,
        failed_sample: failed_paths,
        remaining_backup,
    }
}

pub(super) fn record_open_path_intersection(
    out: &mut Vec<PathBuf>,
    path: &Path,
    canonical_path: Option<&Path>,
    open_canonical_identities: &HashSet<PathBuf>,
) {
    let identity = canonical_path.unwrap_or(path);
    if open_canonical_identities.contains(identity) {
        out.push(path.to_path_buf());
    }
}

/// What a cancelled run's rollback left (design D8).
#[derive(Debug, Default)]
pub(super) struct RollbackResult {
    /// Entries rollback could not restore; the journal keeps exactly these.
    pub(super) remaining: ReplaceUndoBackup,
    /// Why each was not restored.
    pub(super) errors: BoundedDiagnosticSample,
}

/// Restore the files a cancelled run wrote, in reverse order (design D8).
///
/// The caller holds the journal guard. Each file is validated by its digest
/// first: the per-file target guard is released as the loop moves on, so an
/// editor save of a later target can land while rollback is working, and a
/// file that no longer holds this run's output is kept, with its entry.
pub(super) fn rollback_applied_files(backup: &ReplaceUndoBackup) -> RollbackResult {
    let mut result = RollbackResult::default();
    let mut session = UndoSession::new(backup);
    for (path, entry) in backup.iter().rev() {
        let (end, _) = session.undo_entry(path, entry, &mut |_| {});
        match end {
            UndoEnd::Restored { .. } => {}
            UndoEnd::Diverged => {
                result.errors.push(format!(
                    "Skipped rollback of {}: changed after Replace All wrote it",
                    path.display()
                ));
                result.remaining.insert(path.clone(), entry.clone());
            }
            UndoEnd::Failed => {
                result
                    .errors
                    .push(format!("Failed to restore {}", path.display()));
                result.remaining.insert(path.clone(), entry.clone());
            }
        }
    }
    result
}

/// One entry's undo protocol as [`drive`] runs it.
impl Protocol for ReplaceUndoProtocol {
    const KIND: ProtocolKind = ProtocolKind::ReplaceUndo;
    type Action = UndoAction;
    type Outcome = UndoOutcome;
    type End = UndoEnd;

    fn end(action: UndoAction) -> Option<UndoEnd> {
        match action {
            UndoAction::Finish(end) => Some(end),
            _ => None,
        }
    }

    fn advance(&mut self, outcome: UndoOutcome) -> UndoAction {
        self.step(outcome)
    }
}

/// State one Undo carries across its entries: whether the manifest still
/// needs re-arming for this launch (design D6, D9).
struct UndoSession<'a> {
    backup: &'a ReplaceUndoBackup,
    rearm_needed: bool,
}

impl<'a> UndoSession<'a> {
    fn new(backup: &'a ReplaceUndoBackup) -> Self {
        let current = LaunchId::current();
        Self {
            backup,
            rearm_needed: backup.values().any(|entry| entry.launch != current),
        }
    }

    /// Run U0–U3 for one entry. `bytes` sees each restored chunk's length.
    fn undo_entry(
        &mut self,
        path: &Path,
        entry: &ReplaceUndoEntry,
        bytes: &mut dyn FnMut(usize),
    ) -> (UndoEnd, Option<FileFacts>) {
        // A legacy entry belongs to a manifest that lists it, so it is retired
        // by the manifest rewrite of the caller's shrink, never here.
        let retire = matches!(entry.original_source, UndoOriginal::Body(_));
        let (mut protocol, first) = ReplaceUndoProtocol::start(self.rearm_needed, retire);
        let mut guard = None;
        let mut facts: Option<FileFacts> = None;
        let end = drive(&mut protocol, first, |action| match action {
            UndoAction::AcquireTarget => match fs_write::TargetWriteGuard::acquire(path) {
                Err(_) => UndoOutcome::Failed,
                Ok(acquired) => {
                    guard = Some(acquired);
                    match fs_metadata::file_facts(path) {
                        Err(_) => UndoOutcome::Failed,
                        Ok(found) => {
                            facts = Some(found);
                            run_undo_after_metadata_hook_for_test(path);
                            UndoOutcome::Done
                        }
                    }
                }
            },
            UndoAction::HashCurrent => UndoOutcome::Current(current_content(
                path,
                entry,
                facts.as_ref().expect("facts precede the hash"),
            )),
            UndoAction::RearmJournal => {
                if self.rearm() {
                    UndoOutcome::Done
                } else {
                    UndoOutcome::Failed
                }
            }
            UndoAction::AdmitRestore => {
                // The raw need only, never the reserve: `main`'s undo wrote
                // the same temp with none (design D6).
                let free = fs_metadata::free_space(fs_write::parent_or_current(path)).ok();
                if disk_admits(free, entry.original.len, false) {
                    UndoOutcome::Done
                } else {
                    UndoOutcome::Failed
                }
            }
            UndoAction::WriteRestore => {
                match write_restore(
                    path,
                    entry,
                    facts.as_ref().expect("facts precede the restore"),
                    bytes,
                ) {
                    // A rename whose directory sync failed holds the
                    // original bytes: it is restored, and the next hash
                    // of a retry sees them.
                    Ok(()) | Err(fs_write::DurableWriteError::AfterRename(_)) => UndoOutcome::Done,
                    Err(fs_write::DurableWriteError::BeforeRename(_)) => UndoOutcome::Failed,
                }
            }
            UndoAction::RemoveEntry => {
                match crate::services::filesystem::mutate::remove_file_if_exists(&entry.entry_file)
                {
                    Ok(_) => UndoOutcome::Done,
                    Err(_) => UndoOutcome::Failed,
                }
            }
            UndoAction::RemoveBody => match &entry.original_source {
                UndoOriginal::Body(body) => {
                    match crate::services::filesystem::mutate::remove_file_if_exists(body) {
                        Ok(_) => UndoOutcome::Done,
                        Err(_) => UndoOutcome::Failed,
                    }
                }
                UndoOriginal::LegacyEntry => UndoOutcome::Done,
            },
            UndoAction::Finish(_) => unreachable!("`drive` never executes the terminal action"),
        });
        self.rearm_needed = protocol.rearm_needed();
        drop(guard);
        (end, facts)
    }

    /// Sweep the recorded launch's journal-owned temps, then re-arm the
    /// manifest for this launch (design D6, D9).
    fn rearm(&self) -> bool {
        let Some(journal_dir) = self
            .backup
            .values()
            .next()
            .and_then(|entry| entry.entry_file.parent())
        else {
            return false;
        };
        let current = LaunchId::current();
        let mut recorded: Vec<LaunchId> = self
            .backup
            .values()
            .map(|entry| entry.launch)
            .filter(|launch| *launch != current)
            .collect();
        recorded.sort_unstable_by_key(|launch| (launch.pid, launch.nonce));
        recorded.dedup();
        for launch in recorded {
            let report =
                sweep_journal_owned_leftovers(self.backup.keys().map(PathBuf::as_path), launch);
            if report.failures > 0 {
                tracing::warn!(
                    "Replace All undo could not remove {} interrupted temp files",
                    report.failures
                );
            }
        }
        search_backup::rearm_journal(journal_dir, current).is_ok()
    }
}

/// U1: what the file holds now, by length and digest. Anything unreadable
/// or past the file limit reads as diverged, so it is never written.
pub(super) fn current_content(
    path: &Path,
    entry: &ReplaceUndoEntry,
    facts: &FileFacts,
) -> CurrentContent {
    let size = facts.byte_size;
    if size > effective_max_replace_file_bytes()
        || (size != entry.original.len && size != entry.replaced.len)
    {
        return CurrentContent::Diverged;
    }
    let Ok(digest) = digest_file(path, size) else {
        return CurrentContent::Diverged;
    };
    if digest == entry.original {
        CurrentContent::Original
    } else if digest == entry.replaced {
        CurrentContent::Replaced
    } else {
        CurrentContent::Diverged
    }
}

/// U2: stream the original back into the target. The rename happens only if
/// what was streamed hashes to the original and the target's facts are U0's.
fn write_restore(
    path: &Path,
    entry: &ReplaceUndoEntry,
    facts: &FileFacts,
    bytes: &mut dyn FnMut(usize),
) -> Result<(), fs_write::DurableWriteError> {
    let write_path = fs_write::resolve_target_identity(path)
        .map_err(fs_write::DurableWriteError::BeforeRename)?
        .into_path_buf();
    // A legacy original is re-read from its entry, under `main`'s bound, on
    // this worker; at most one such entry is in memory (design D6).
    let legacy = match &entry.original_source {
        UndoOriginal::Body(_) => None,
        UndoOriginal::LegacyEntry => Some(
            search_backup::load_legacy_original(
                &entry.entry_file,
                effective_main_replace_undo_bytes(),
            )
            .map_err(|e| fs_write::DurableWriteError::BeforeRename(std::io::Error::other(e)))?,
        ),
    };
    fs_write::atomic_replace_workspace_file_stream(&write_path, WriteLabel::REPLACE, |target| {
        let mut sink = Sha256Sink::default();
        let len = match (&entry.original_source, &legacy) {
            (_, Some(original)) => {
                for chunk in original.chunks(effective_stream_chunk_bytes()) {
                    target.write_all(chunk)?;
                    sink.update(chunk);
                    bytes(chunk.len());
                }
                u64::try_from(original.len()).unwrap_or(u64::MAX)
            }
            (UndoOriginal::Body(body), None) => stream_bounded(
                body,
                entry.original.len,
                effective_stream_chunk_bytes(),
                || false,
                |chunk| {
                    target.write_all(chunk)?;
                    sink.update(chunk);
                    bytes(chunk.len());
                    Ok::<(), std::io::Error>(())
                },
            )
            .map_err(|error| match error {
                BoundedStreamError::Io(e) | BoundedStreamError::Consumer(e) => e,
                BoundedStreamError::LimitExceeded { .. } => {
                    std::io::Error::other("undo body is larger than its recorded original")
                }
                BoundedStreamError::Cancelled => std::io::Error::other("cancelled"),
            })?,
            (UndoOriginal::LegacyEntry, None) => {
                return Err(std::io::Error::other("legacy original was not loaded"));
            }
        };
        let streamed = ContentDigest {
            len,
            sha256: sink.finish(),
        };
        if streamed != entry.original {
            return Err(std::io::Error::other(
                "undo body does not hold the recorded original",
            ));
        }
        let unchanged = fs_metadata::file_stat(path).is_ok_and(|now| {
            now.byte_size == facts.byte_size
                && now.modified_at_nanos == facts.modified_at_nanos
                && now.identity == facts.identity
        });
        if !unchanged {
            return Err(std::io::Error::other(
                "the file changed while Undo was restoring it",
            ));
        }
        kill_point::reach(
            KillWindow::ReplaceUndoBeforeTargetRename,
            WriteLabel::REPLACE.as_str(),
        );
        Ok(())
    })
}
