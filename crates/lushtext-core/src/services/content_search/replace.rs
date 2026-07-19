// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace-all and undo flows for workspace content search.
//!
//! This is the command side of the content-search service. It performs file
//! locking, atomic writes, rollback on cancellation, and undo backup handling
//! without depending on any GTK types.

#[cfg(any(test, feature = "test-utils"))]
use std::cell::Cell;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
#[cfg(any(test, feature = "test-utils"))]
use std::sync::{Mutex, OnceLock};

use crate::model::content_search::{
    BoundedDiagnosticSample, MAX_REPLACE_PREVIEW_ROWS, ReplaceResult, Replacement,
    UndoPayloadLedger,
};
use crate::services::{
    filesystem::{WriteLabel, metadata as fs_metadata, read as fs_read, write as fs_write},
    search_backup,
};

/// Largest single file Replace All will read and rewrite.
///
/// Ten megabytes keeps the whole-file validation and undo snapshot path bounded
/// on ordinary laptops while still covering typical source and notes files.
pub const MAX_REPLACE_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// Largest total undo payload retained for one Replace All run.
///
/// The payload stores before and after bytes for each touched file. Sixty-four
/// megabytes keeps rollback useful without letting one operation consume the
/// editor's broader buffer-memory budget.
pub const MAX_REPLACE_UNDO_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum complete in-memory undo owner, including paths and table capacity.
pub const MAX_REPLACE_UNDO_RETAINED_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static TEST_MAX_REPLACE_UNDO_BYTES: Cell<Option<u64>> = const { Cell::new(None) };
}

#[cfg(test)]
thread_local! {
    static TEST_REQUIRE_ACTIVE_JOURNAL_BEFORE_WRITE: Cell<bool> = const { Cell::new(false) };
}

#[cfg(any(test, feature = "test-utils"))]
type UndoAfterMetadataHook = Box<dyn FnOnce(&Path) + Send + 'static>;
#[cfg(any(test, feature = "test-utils"))]
static UNDO_AFTER_METADATA_HOOK: OnceLock<Mutex<Option<UndoAfterMetadataHook>>> = OnceLock::new();

#[cfg(any(test, feature = "test-utils"))]
static FAIL_REPLACE_BEFORE_RENAME_PATH: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

/// Override the Replace All undo-payload ceiling on the current test thread.
#[cfg(any(test, feature = "test-utils"))]
pub fn set_max_replace_undo_bytes_for_test(limit: Option<u64>) {
    TEST_MAX_REPLACE_UNDO_BYTES.with(|slot| slot.set(limit));
}

/// Fail the next Replace All target write for `path` before its rename.
#[cfg(any(test, feature = "test-utils"))]
pub fn fail_next_replace_before_rename_for_path_for_test(path: &Path) {
    let slot = FAIL_REPLACE_BEFORE_RENAME_PATH.get_or_init(|| Mutex::new(None));
    *slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(path.to_path_buf());
}

#[cfg(any(test, feature = "test-utils"))]
fn take_replace_before_rename_failure_for_test(path: &Path) -> bool {
    let slot = FAIL_REPLACE_BEFORE_RENAME_PATH.get_or_init(|| Mutex::new(None));
    let mut pending = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if pending.as_deref() == Some(path) {
        pending.take();
        true
    } else {
        false
    }
}

#[cfg(not(any(test, feature = "test-utils")))]
fn take_replace_before_rename_failure_for_test(_path: &Path) -> bool {
    false
}

/// Install a one-shot Undo race seam after metadata but before bounded ingestion.
#[cfg(any(test, feature = "test-utils"))]
pub fn set_undo_after_metadata_hook_for_test(hook: impl FnOnce(&Path) + Send + 'static) {
    let slot = UNDO_AFTER_METADATA_HOOK.get_or_init(|| Mutex::new(None));
    *slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Box::new(hook));
}

#[cfg(any(test, feature = "test-utils"))]
fn run_undo_after_metadata_hook_for_test(path: &Path) {
    let slot = UNDO_AFTER_METADATA_HOOK.get_or_init(|| Mutex::new(None));
    let hook = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    if let Some(hook) = hook {
        hook(path);
    }
}

#[cfg(not(any(test, feature = "test-utils")))]
fn run_undo_after_metadata_hook_for_test(_path: &Path) {}

/// Per-file bytes needed to safely undo a Replace All.
///
/// The undo path compares `replaced_bytes` with the file's current contents
/// before restoring `original_bytes`, so edits made after Replace All are not
/// overwritten by a stale undo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceUndoEntry {
    /// File bytes before Replace All changed this file.
    pub original_bytes: Vec<u8>,
    /// File bytes immediately after Replace All changed this file.
    pub replaced_bytes: Vec<u8>,
}

impl ReplaceUndoEntry {
    /// Build one undo entry from the before/after byte snapshots.
    #[must_use]
    pub fn new(original_bytes: Vec<u8>, replaced_bytes: Vec<u8>) -> Self {
        Self {
            original_bytes,
            replaced_bytes,
        }
    }
}

/// In-memory Replace All undo backup keyed by absolute file path.
pub type ReplaceUndoBackup = BTreeMap<PathBuf, ReplaceUndoEntry>;

/// Return every heap byte retained by an in-memory Replace All undo owner.
#[must_use]
pub fn replace_undo_retained_byte_weight(backup: &ReplaceUndoBackup) -> u64 {
    backup.iter().fold(0u64, |total, (path, entry)| {
        total.saturating_add(replace_undo_entry_retained_byte_weight(path, entry))
    })
}

fn replace_undo_entry_retained_byte_weight(path: &PathBuf, entry: &ReplaceUndoEntry) -> u64 {
    // BTreeMap gives rollback and diagnostic sampling stable path order. Charge
    // one conservative node/link allowance per entry in addition to its graph.
    let node_bytes = std::mem::size_of::<(PathBuf, ReplaceUndoEntry)>()
        .saturating_add(std::mem::size_of::<usize>().saturating_mul(4));
    u64::try_from(node_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(u64::try_from(path.capacity()).unwrap_or(u64::MAX))
        .saturating_add(u64::try_from(entry.original_bytes.capacity()).unwrap_or(u64::MAX))
        .saturating_add(u64::try_from(entry.replaced_bytes.capacity()).unwrap_or(u64::MAX))
}

/// Direct boundedness evidence collected while Replace All constructs output.
///
/// Totals cover every file whose text construction ran. Metadata and undo
/// fields are high-water marks because those are the ownership bounds that
/// matter while one operation is live.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReplaceConstructionMetrics {
    /// Source lines visited by the monotonic line-boundary cursor.
    pub source_lines: u64,
    /// Replacement records accepted into successfully constructed output.
    pub accepted_replacements: usize,
    /// Largest number of retained edit records for any one file.
    pub retained_edit_records: usize,
    /// Largest retained edit-vector allocation, in bytes, for any one file.
    pub retained_edit_bytes: usize,
    /// Total output bytes successfully constructed during this operation.
    pub output_bytes: u64,
    /// Largest aggregate before-and-after undo payload admitted at one time.
    pub undo_bytes: u64,
    /// Reversible undo bytes still live at terminal publication.
    pub undo_live_bytes: u64,
}

impl ReplaceConstructionMetrics {
    fn absorb_construction(&mut self, other: Self) {
        self.source_lines = self.source_lines.saturating_add(other.source_lines);
        self.accepted_replacements = self
            .accepted_replacements
            .saturating_add(other.accepted_replacements);
        self.retained_edit_records = self.retained_edit_records.max(other.retained_edit_records);
        self.retained_edit_bytes = self.retained_edit_bytes.max(other.retained_edit_bytes);
        self.output_bytes = self.output_bytes.saturating_add(other.output_bytes);
    }
}

/// GTK-free freshness token for one serialized Replace All journal transaction.
#[derive(Clone)]
pub(crate) struct ReplaceJournalFreshness {
    generation: Arc<AtomicU32>,
    expected: u32,
}

impl ReplaceJournalFreshness {
    pub(crate) fn new(generation: Arc<AtomicU32>, expected: u32) -> Self {
        Self {
            generation,
            expected,
        }
    }

    #[must_use]
    pub(crate) fn expected(&self) -> u32 {
        self.expected
    }

    #[must_use]
    fn is_current(&self) -> bool {
        self.generation.load(Ordering::Acquire) == self.expected
    }
}

/// Result of applying Replace All plus the undo payload needed by the caller.
#[derive(Debug)]
pub struct ApplyReplacementsOutcome {
    /// User-facing replacement counts, skips, and recoverable errors.
    pub result: ReplaceResult,
    /// Per-file before/after bytes retained for the active undo window.
    pub undo_backup: ReplaceUndoBackup,
    /// Direct construction and undo ownership evidence for this operation.
    pub metrics: ReplaceConstructionMetrics,
}

impl ApplyReplacementsOutcome {
    /// Split the outcome into its two public payloads for callers that need both.
    #[must_use]
    pub fn into_parts(self) -> (ReplaceResult, ReplaceUndoBackup) {
        (self.result, self.undo_backup)
    }

    /// Return direct boundedness evidence without consuming the result.
    #[must_use]
    pub fn metrics(&self) -> ReplaceConstructionMetrics {
        self.metrics
    }
}

/// Outcome of one undo attempt across a Replace All backup.
///
/// `remaining_backup` contains only entries that were not restored, letting the
/// UI persist a smaller retryable backup instead of clearing undo state after a
/// partial success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoReplaceOutcome {
    /// Exact number of paths restored to their pre-replace bytes.
    pub restored_count: usize,
    /// Exact number of paths left untouched because current bytes diverged.
    pub skipped_count: usize,
    /// Exact number of paths that could not be read, locked, or written.
    pub failed_count: usize,
    /// Restored paths intersected with the caller's open canonical identities.
    pub restored_open_paths: Vec<PathBuf>,
    /// Bounded deterministic skipped-path evidence.
    pub skipped_sample: BoundedDiagnosticSample,
    /// Bounded deterministic failure-path evidence.
    pub failed_sample: BoundedDiagnosticSample,
    /// Retryable backup entries for skipped or failed paths.
    pub remaining_backup: ReplaceUndoBackup,
}

impl UndoReplaceOutcome {
    /// Number of files restored by this undo attempt.
    #[must_use]
    pub fn restored_count(&self) -> usize {
        self.restored_count
    }

    /// Number of files still retained for a future undo attempt.
    #[must_use]
    pub fn remaining_count(&self) -> usize {
        self.remaining_backup.len()
    }
}

/// Apply replacements to files on disk.
///
/// Groups replacements by file, streams sorted source ranges into changed output,
/// and writes atomically (temp file + rename). Returns the replacement summary
/// and a backup mapping file paths to their before/after content snapshots for undo.
///
/// Per-file errors are collected (not early-returned) so that already-replaced files
/// remain in the backup for undo. Only returns `Err` if zero files could be processed.
///
/// `skip_paths` lists files that should NOT be replaced (e.g., open tabs with unsaved changes).
/// Skipped files are excluded from the result count but included in the exact
/// count and bounded `ReplaceResult::skipped_sample` diagnostic projection.
///
/// # Errors
///
/// Returns an error if every candidate file fails to process or if the replace
/// operation is cancelled and rollback cannot complete cleanly.
pub fn apply_replacements(
    replacements: &[Replacement],
    skip_paths: &HashSet<PathBuf>,
    cancel: &AtomicBool,
    journal_data_dir: Option<&Path>,
) -> anyhow::Result<ApplyReplacementsOutcome> {
    apply_replacements_inner(
        replacements,
        skip_paths,
        &HashSet::new(),
        cancel,
        journal_data_dir,
        None,
    )
    .and_then(|outcome| {
        outcome.ok_or_else(|| anyhow::anyhow!("unguarded Replace All became stale"))
    })
}

/// Apply only if the UI reservation is still current after acquiring the journal lock.
pub(crate) fn apply_replacements_if_current(
    replacements: &[Replacement],
    skip_paths: &HashSet<PathBuf>,
    open_canonical_identities: &HashSet<PathBuf>,
    cancel: &AtomicBool,
    journal_data_dir: &Path,
    freshness: &ReplaceJournalFreshness,
) -> anyhow::Result<Option<ApplyReplacementsOutcome>> {
    apply_replacements_inner(
        replacements,
        skip_paths,
        open_canonical_identities,
        cancel,
        Some(journal_data_dir),
        Some(freshness),
    )
}

fn apply_replacements_inner(
    replacements: &[Replacement],
    skip_paths: &HashSet<PathBuf>,
    open_canonical_identities: &HashSet<PathBuf>,
    cancel: &AtomicBool,
    journal_data_dir: Option<&Path>,
    freshness: Option<&ReplaceJournalFreshness>,
) -> anyhow::Result<Option<ApplyReplacementsOutcome>> {
    if replacements.len() > MAX_REPLACE_PREVIEW_ROWS {
        anyhow::bail!(
            "Replace All selection exceeds the {MAX_REPLACE_PREVIEW_ROWS}-replacement limit"
        );
    }
    let _journal_guard = journal_data_dir
        .map(|_| search_backup::acquire_journal_guard())
        .transpose()?;
    // The journal lock serializes this decision with startup recovery and all
    // journal commits. A stale UI generation must exit before preparing a new
    // journal or mutating any target file.
    if freshness.is_some_and(|freshness| !freshness.is_current()) {
        return Ok(None);
    }
    let mut by_file: BTreeMap<PathBuf, Vec<&Replacement>> = BTreeMap::new();
    for r in replacements {
        by_file.entry(r.path.clone()).or_default().push(r);
    }

    let mut backup = ReplaceUndoBackup::new();
    let mut replaced_count = 0usize;
    let mut files_affected = 0usize;
    let mut skipped_sample = BoundedDiagnosticSample::default();
    let mut errors = BoundedDiagnosticSample::default();
    let mut affected_open_paths = Vec::new();
    let mut cancelled = false;
    let mut undo_ledger = UndoPayloadLedger::new(effective_max_replace_undo_bytes());
    let mut retained_undo_ledger = UndoPayloadLedger::new(MAX_REPLACE_UNDO_RETAINED_BYTES);
    let mut metrics = ReplaceConstructionMetrics::default();
    let mut journal_prepared = false;
    let mut journal_armed = false;

    for (path, mut file_replacements) in by_file {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        if skip_paths.contains(&path) {
            skipped_sample.record_path(&path);
            continue;
        }

        let _guard = match fs_write::TargetWriteGuard::acquire(&path) {
            Ok(guard) => guard,
            Err(e) => {
                errors.push(format!("Failed to lock {}: {e}", path.display()));
                continue;
            }
        };
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        let facts = match fs_metadata::file_facts(&path) {
            Ok(facts) => facts,
            Err(e) => {
                errors.push(format!("Failed to stat {}: {e}", path.display()));
                continue;
            }
        };
        if facts.byte_size > MAX_REPLACE_FILE_BYTES {
            skipped_sample.record_path(&path);
            errors.push(format!(
                "Skipped {}: file is larger than the 10 MB Replace All limit",
                path.display()
            ));
            continue;
        }

        let original_bytes =
            match fs_read::bounded_bytes(&path, MAX_REPLACE_FILE_BYTES, facts.byte_size, || {
                cancel.load(Ordering::Relaxed)
            }) {
                Ok(bytes) => bytes,
                Err(fs_read::BoundedFileReadError::Cancelled) => {
                    cancelled = true;
                    break;
                }
                Err(fs_read::BoundedFileReadError::LimitExceeded { .. }) => {
                    skipped_sample.record_path(&path);
                    errors.push(format!(
                        "Skipped {}: file is larger than the 10 MB Replace All limit",
                        path.display()
                    ));
                    continue;
                }
                Err(fs_read::BoundedFileReadError::Io(e)) => {
                    errors.push(format!("Failed to read {}: {e}", path.display()));
                    continue;
                }
            };

        let original_text = match simdutf8::basic::from_utf8(&original_bytes) {
            Ok(text) => text,
            Err(e) => {
                errors.push(format!("Non-UTF8 file {}: {e}", path.display()));
                continue;
            }
        };

        let remaining_undo_bytes = undo_ledger
            .remaining_bytes()
            .saturating_sub(u64::try_from(original_bytes.len()).unwrap_or(u64::MAX));
        let undo_output_limit = usize::try_from(remaining_undo_bytes).unwrap_or(usize::MAX);
        let file_output_limit = usize::try_from(MAX_REPLACE_FILE_BYTES).unwrap_or(usize::MAX);
        let output_limit = undo_output_limit.min(file_output_limit);
        let output_limited_by_file_size = file_output_limit <= undo_output_limit;
        let text_build =
            build_replaced_text(original_text, &mut file_replacements, output_limit, || {
                cancel.load(Ordering::Relaxed)
            });
        metrics.absorb_construction(text_build.metrics);
        let (new_content, file_replaced) = match text_build.outcome {
            ReplacementTextOutcome::Replaced {
                new_content,
                replacement_count,
            } => (new_content, replacement_count),
            ReplacementTextOutcome::StaleLine { line_number } => {
                errors.push(format!(
                    "Skipped {}: line {} changed since search",
                    path.display(),
                    line_number,
                ));
                continue;
            }
            ReplacementTextOutcome::InvalidPreview { reason } => {
                errors.push(format!(
                    "Skipped {}: invalid replacement preview ({reason})",
                    path.display(),
                ));
                continue;
            }
            ReplacementTextOutcome::OutputLimitExceeded => {
                skipped_sample.record_path(&path);
                if output_limited_by_file_size {
                    errors.push(format!(
                        "Skipped {}: replacement output would exceed the 10 MB per-file limit",
                        path.display()
                    ));
                } else {
                    errors.push(format!(
                        "Skipped {}: undo data would exceed the 64 MB Replace All limit",
                        path.display()
                    ));
                }
                continue;
            }
            ReplacementTextOutcome::Cancelled => {
                cancelled = true;
                break;
            }
            ReplacementTextOutcome::Unchanged => continue,
        };
        let replaced_bytes = new_content.into_bytes();
        let entry_payload_bytes = u64::try_from(original_bytes.len())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(replaced_bytes.len()).unwrap_or(u64::MAX));
        if !undo_ledger.try_charge(entry_payload_bytes) {
            skipped_sample.record_path(&path);
            errors.push(format!(
                "Skipped {}: undo data would exceed the 64 MB Replace All limit",
                path.display()
            ));
            continue;
        }

        let entry = ReplaceUndoEntry::new(original_bytes, replaced_bytes);
        let backup_path = path.clone();
        let retained_entry_bytes = replace_undo_entry_retained_byte_weight(&backup_path, &entry);
        if !retained_undo_ledger.try_charge(retained_entry_bytes) {
            let reclaimed = undo_ledger.reclaim(entry_payload_bytes);
            debug_assert_eq!(reclaimed, entry_payload_bytes);
            skipped_sample.record_path(&path);
            errors.push(format!(
                "Skipped {}: complete undo state would exceed the 64 MB Replace All limit",
                path.display()
            ));
            continue;
        }
        if let Some(data_dir) = journal_data_dir {
            if !journal_prepared {
                if let Err(e) = search_backup::begin_incremental_journal(data_dir) {
                    errors.push(format!(
                        "Failed to prepare undo journal before replacing {}: {e}",
                        path.display()
                    ));
                    let reclaimed = undo_ledger.reclaim(entry_payload_bytes);
                    debug_assert_eq!(reclaimed, entry_payload_bytes);
                    let retained_reclaimed = retained_undo_ledger.reclaim(retained_entry_bytes);
                    debug_assert_eq!(retained_reclaimed, retained_entry_bytes);
                    continue;
                }
                journal_prepared = true;
            }
            if let Err(e) = search_backup::save_entry(data_dir, &path, &entry) {
                errors.push(format!(
                    "Failed to persist undo journal before replacing {}: {e}",
                    path.display()
                ));
                let reclaimed = undo_ledger.reclaim(entry_payload_bytes);
                debug_assert_eq!(reclaimed, entry_payload_bytes);
                let retained_reclaimed = retained_undo_ledger.reclaim(retained_entry_bytes);
                debug_assert_eq!(retained_reclaimed, retained_entry_bytes);
                if let Err(cleanup_error) = search_backup::delete_entry(data_dir, &path) {
                    errors.push(format!(
                        "Failed to remove incomplete undo entry for {}: {cleanup_error}",
                        path.display()
                    ));
                }
                continue;
            }
            if !journal_armed {
                if let Err(e) = search_backup::mark_incremental_journal_active(data_dir) {
                    errors.push(format!(
                        "Failed to activate undo journal before replacing {}: {e}",
                        path.display()
                    ));
                    if let Err(cleanup_error) = search_backup::delete_entry(data_dir, &path) {
                        errors.push(format!(
                            "Failed to remove inactive undo entry for {}: {cleanup_error}",
                            path.display()
                        ));
                    }
                    let reclaimed = undo_ledger.reclaim(entry_payload_bytes);
                    debug_assert_eq!(reclaimed, entry_payload_bytes);
                    let retained_reclaimed = retained_undo_ledger.reclaim(retained_entry_bytes);
                    debug_assert_eq!(retained_reclaimed, retained_entry_bytes);
                    continue;
                }
                journal_armed = true;
            }
        }
        backup.insert(backup_path, entry);
        metrics.undo_bytes = undo_ledger.high_water_bytes();

        assert_active_journal_before_write_for_test(journal_data_dir, &path);

        match atomic_write(&path, &backup[&path].replaced_bytes) {
            Ok(()) => {
                let identity = facts.canonical_path.as_deref().unwrap_or(&path);
                if open_canonical_identities.contains(identity) {
                    affected_open_paths.push(path.clone());
                }
                record_replacement_success_counts(
                    &mut replaced_count,
                    &mut files_affected,
                    file_replaced,
                );
            }
            Err(ReplaceWriteError::BeforeRename(e)) => {
                errors.push(format!("Failed to write {}: {e}", path.display()));
                backup.remove(&path);
                let reclaimed = undo_ledger.reclaim(entry_payload_bytes);
                debug_assert_eq!(reclaimed, entry_payload_bytes);
                let retained_reclaimed = retained_undo_ledger.reclaim(retained_entry_bytes);
                debug_assert_eq!(retained_reclaimed, retained_entry_bytes);
                if let Some(data_dir) = journal_data_dir
                    && let Err(journal_error) = search_backup::delete_entry(data_dir, &path)
                {
                    errors.push(format!(
                        "Failed to remove undo journal entry after write failure for {}: {journal_error}",
                        path.display()
                    ));
                }
                continue;
            }
            Err(ReplaceWriteError::AfterRename(e)) => {
                errors.push(format!(
                    "Replaced {}, but durability sync failed: {e}",
                    path.display()
                ));
                let identity = facts.canonical_path.as_deref().unwrap_or(&path);
                if open_canonical_identities.contains(identity) {
                    affected_open_paths.push(path.clone());
                }
                record_replacement_success_counts(
                    &mut replaced_count,
                    &mut files_affected,
                    file_replaced,
                );
            }
        }
    }

    if cancelled {
        let rollback_errors = rollback_applied_files(&backup);
        if rollback_errors.total_count() == 0 {
            if let Some(data_dir) = journal_data_dir
                && let Err(e) = persist_undo_backup(data_dir, &ReplaceUndoBackup::new())
            {
                return Err(anyhow::anyhow!(
                    "Replace cancelled; undo backup cleanup failed: {e}"
                ));
            }
            return Err(anyhow::anyhow!("Replace cancelled"));
        }
        return Err(anyhow::anyhow!(
            rollback_errors.summary("Replace cancelled; rollback failed")
        ));
    }

    if backup.is_empty()
        && journal_prepared
        && let Some(data_dir) = journal_data_dir
        && let Err(e) = search_backup::delete(data_dir)
    {
        errors.push(format!("Failed to clean empty undo journal: {e}"));
    }

    if files_affected == 0 && skipped_sample.total_count() == 0 && errors.total_count() > 0 {
        return Err(anyhow::anyhow!(errors.summary("Replace All failed")));
    }

    debug_assert_eq!(
        retained_undo_ledger.live_bytes(),
        replace_undo_retained_byte_weight(&backup)
    );
    debug_assert!(retained_undo_ledger.live_bytes() <= MAX_REPLACE_UNDO_RETAINED_BYTES);
    metrics.undo_live_bytes = undo_ledger.live_bytes();

    let result = ReplaceResult {
        replaced_count,
        files_affected,
        skipped_count: skipped_sample.total_count(),
        error_count: errors.total_count(),
        skipped_sample,
        error_sample: errors,
        affected_open_paths,
    };
    Ok(Some(ApplyReplacementsOutcome {
        result,
        undo_backup: backup,
        metrics,
    }))
}

fn assert_active_journal_before_write_for_test(journal_data_dir: Option<&Path>, path: &Path) {
    #[cfg(test)]
    TEST_REQUIRE_ACTIVE_JOURNAL_BEFORE_WRITE.with(|required| {
        if required.get() {
            let data_dir = journal_data_dir.expect("journal data dir required by test");
            let persisted = search_backup::load(data_dir)
                .expect("active undo journal must be readable before target write");
            assert!(
                persisted.contains_key(path),
                "target undo entry must be active before target write"
            );
        }
    });
    #[cfg(not(test))]
    let _ = (journal_data_dir, path);
}

fn effective_max_replace_undo_bytes() -> u64 {
    #[cfg(any(test, feature = "test-utils"))]
    {
        if let Some(override_value) = TEST_MAX_REPLACE_UNDO_BYTES.with(Cell::get) {
            return override_value;
        }
    }
    MAX_REPLACE_UNDO_BYTES
}

/// Fold one successfully written file into the public Replace All counters.
fn record_replacement_success_counts(
    replaced_count: &mut usize,
    files_affected: &mut usize,
    file_replaced: usize,
) {
    *replaced_count += file_replaced;
    *files_affected += 1;
}

/// Pure result of applying one file's replacement preview data to text.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReplacementTextOutcome {
    /// At least one bounded replacement changed the file text.
    Replaced {
        /// Full file text after replacements are applied.
        new_content: String,
        /// Number of individual replacement previews consumed.
        replacement_count: usize,
    },
    /// No generated replacement targeted an existing line or valid range.
    Unchanged,
    /// The line text no longer matches the preview captured during search.
    StaleLine {
        /// Original 1-based line number reported by the stale search result.
        line_number: u64,
    },
    /// Preview metadata violated an invariant guaranteed by preview generation.
    InvalidPreview {
        /// Content-free invariant description suitable for diagnostics.
        reason: &'static str,
    },
    /// Constructed output would exceed the remaining durable undo allowance.
    OutputLimitExceeded,
    /// The owning Replace All operation was cancelled during construction.
    Cancelled,
}

#[derive(Debug)]
struct ReplacementTextBuild {
    outcome: ReplacementTextOutcome,
    metrics: ReplaceConstructionMetrics,
}

#[derive(Debug)]
struct PendingEdit<'a> {
    start: usize,
    end: usize,
    replacement: &'a str,
}

/// Monotonic source-line cursor that never retains a whole-file line index.
struct StreamingLineCursor<'a> {
    bytes: &'a [u8],
    next_line_start: usize,
    next_line_number: u64,
    current_line_number: Option<u64>,
    current_span: Option<std::ops::Range<usize>>,
    source_lines: u64,
}

impl<'a> StreamingLineCursor<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            next_line_start: 0,
            next_line_number: 1,
            current_line_number: None,
            current_span: None,
            source_lines: 0,
        }
    }

    fn advance_to(
        &mut self,
        target_line: u64,
        is_cancelled: &mut impl FnMut() -> bool,
    ) -> Result<Option<std::ops::Range<usize>>, ()> {
        if self.current_line_number == Some(target_line) {
            return Ok(self.current_span.clone());
        }
        if self
            .current_line_number
            .is_some_and(|line| target_line < line)
        {
            return Ok(None);
        }

        while self.next_line_start < self.bytes.len() {
            if self.source_lines.is_multiple_of(1_024) && is_cancelled() {
                return Err(());
            }
            let line_start = self.next_line_start;
            let newline = memchr::memchr(b'\n', &self.bytes[line_start..])
                .map(|relative| line_start + relative);
            let line_end = match newline {
                Some(index) if index > line_start && self.bytes[index - 1] == b'\r' => index - 1,
                Some(index) => index,
                None => self.bytes.len(),
            };
            self.next_line_start = newline.map_or(self.bytes.len(), |index| index + 1);
            let line_number = self.next_line_number;
            self.next_line_number = self.next_line_number.saturating_add(1);
            self.current_line_number = Some(line_number);
            self.current_span = Some(line_start..line_end);
            self.source_lines = self.source_lines.saturating_add(1);

            if line_number == target_line {
                return Ok(self.current_span.clone());
            }
            if line_number > target_line {
                return Ok(None);
            }
        }

        Ok(None)
    }
}

/// Apply one file's replacement previews to already-loaded text.
///
/// The helper owns deterministic range clipping and source-order construction
/// used by the I/O command above, making that behavior property-testable
/// without opening files or touching undo journals.
fn build_replaced_text(
    original_text: &str,
    file_replacements: &mut [&Replacement],
    max_output_bytes: usize,
    mut is_cancelled: impl FnMut() -> bool,
) -> ReplacementTextBuild {
    let mut metrics = ReplaceConstructionMetrics::default();
    let file_byte_limit = usize::try_from(MAX_REPLACE_FILE_BYTES).unwrap_or(usize::MAX);
    if original_text.len() > file_byte_limit {
        return ReplacementTextBuild {
            outcome: ReplacementTextOutcome::InvalidPreview {
                reason: "source bytes exceed the per-file limit",
            },
            metrics,
        };
    }
    if file_replacements.len() > MAX_REPLACE_PREVIEW_ROWS {
        return ReplacementTextBuild {
            outcome: ReplacementTextOutcome::InvalidPreview {
                reason: "replacement count exceeds the preview limit",
            },
            metrics,
        };
    }
    if file_replacements.is_empty() {
        return ReplacementTextBuild {
            outcome: ReplacementTextOutcome::Unchanged,
            metrics,
        };
    }

    file_replacements.sort_by(|a, b| {
        a.line_number
            .cmp(&b.line_number)
            .then(a.match_range.start.cmp(&b.match_range.start))
            .then(a.match_range.end.cmp(&b.match_range.end))
    });

    // Preview generation guarantees positive line numbers plus ordered,
    // non-overlapping clipped ranges. Recheck those invariants before line
    // discovery or output allocation because this command is a data-mutation
    // boundary and may also be called by non-UI clients.
    let mut previous_line = None;
    let mut previous_end = 0usize;
    for (index, replacement) in file_replacements.iter().enumerate() {
        if index.is_multiple_of(1_024) && is_cancelled() {
            return ReplacementTextBuild {
                outcome: ReplacementTextOutcome::Cancelled,
                metrics,
            };
        }
        if replacement.line_number == 0 {
            return ReplacementTextBuild {
                outcome: ReplacementTextOutcome::InvalidPreview {
                    reason: "line numbers must be one-based",
                },
                metrics,
            };
        }
        if replacement.match_range.start > replacement.match_range.end {
            return ReplacementTextBuild {
                outcome: ReplacementTextOutcome::InvalidPreview {
                    reason: "replacement range endpoints are reversed",
                },
                metrics,
            };
        }
        let snapshot = replacement.original_line.as_ref();
        let start = snapshot.floor_char_boundary(replacement.match_range.start.min(snapshot.len()));
        let end = snapshot.ceil_char_boundary(replacement.match_range.end.min(snapshot.len()));
        if previous_line == Some(replacement.line_number) && start < previous_end {
            return ReplacementTextBuild {
                outcome: ReplacementTextOutcome::InvalidPreview {
                    reason: "replacement ranges overlap",
                },
                metrics,
            };
        }
        previous_line = Some(replacement.line_number);
        previous_end = end;
    }

    let mut edits = Vec::with_capacity(file_replacements.len());
    metrics.retained_edit_records = edits.capacity();
    metrics.retained_edit_bytes = edits
        .capacity()
        .saturating_mul(std::mem::size_of::<PendingEdit<'static>>());
    let mut line_cursor = StreamingLineCursor::new(original_text);

    // Validate against the original line snapshot before mutating anything so
    // stale search results skip the whole file instead of partially applying.
    for replacement in file_replacements.iter() {
        let line_span = match line_cursor.advance_to(replacement.line_number, &mut is_cancelled) {
            Ok(Some(span)) => span,
            Ok(None) => {
                metrics.source_lines = line_cursor.source_lines;
                return ReplacementTextBuild {
                    outcome: ReplacementTextOutcome::StaleLine {
                        line_number: replacement.line_number,
                    },
                    metrics,
                };
            }
            Err(()) => {
                metrics.source_lines = line_cursor.source_lines;
                return ReplacementTextBuild {
                    outcome: ReplacementTextOutcome::Cancelled,
                    metrics,
                };
            }
        };
        let line = &original_text[line_span.clone()];
        if line != replacement.original_line.as_ref() {
            metrics.source_lines = line_cursor.source_lines;
            return ReplacementTextBuild {
                outcome: ReplacementTextOutcome::StaleLine {
                    line_number: replacement.line_number,
                },
                metrics,
            };
        }
        let start = line.floor_char_boundary(replacement.match_range.start.min(line.len()));
        let end = line.ceil_char_boundary(replacement.match_range.end.min(line.len()));
        edits.push(PendingEdit {
            start: line_span.start + start,
            end: line_span.start + end,
            replacement: replacement.replacement.as_ref(),
        });
    }
    metrics.source_lines = line_cursor.source_lines;

    if edits.is_empty() {
        return ReplacementTextBuild {
            outcome: ReplacementTextOutcome::Unchanged,
            metrics,
        };
    }

    let output_len = replaced_capacity(original_text.len(), &edits);
    if output_len > max_output_bytes {
        return ReplacementTextBuild {
            outcome: ReplacementTextOutcome::OutputLimitExceeded,
            metrics,
        };
    }

    let mut new_content = String::with_capacity(output_len);
    let mut cursor = 0usize;
    for (index, edit) in edits.iter().enumerate() {
        if index.is_multiple_of(1_024) && is_cancelled() {
            return ReplacementTextBuild {
                outcome: ReplacementTextOutcome::Cancelled,
                metrics,
            };
        }
        new_content.push_str(&original_text[cursor..edit.start]);
        new_content.push_str(edit.replacement);
        cursor = edit.end;
    }
    new_content.push_str(&original_text[cursor..]);
    metrics.accepted_replacements = edits.len();
    metrics.output_bytes = u64::try_from(new_content.len()).unwrap_or(u64::MAX);

    ReplacementTextBuild {
        outcome: ReplacementTextOutcome::Replaced {
            new_content,
            replacement_count: edits.len(),
        },
        metrics,
    }
}

/// Reference-only whole-file line index retained for equivalence tests.
#[cfg(any(test, feature = "property-tests"))]
fn line_spans_reference(text: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut line_start = 0usize;

    for index in memchr::memchr_iter(b'\n', bytes) {
        let line_end = if index > line_start && bytes[index - 1] == b'\r' {
            index - 1
        } else {
            index
        };
        spans.push(line_start..line_end);
        line_start = index + 1;
    }

    if line_start < bytes.len() {
        spans.push(line_start..bytes.len());
    }
    spans
}

/// Estimate final output capacity from the exact replacement ranges.
fn replaced_capacity(original_len: usize, edits: &[PendingEdit<'_>]) -> usize {
    let mut capacity = original_len;
    for edit in edits {
        capacity = capacity
            .saturating_sub(edit.end.saturating_sub(edit.start))
            .saturating_add(edit.replacement.len());
    }
    capacity
}

/// Apply replacement preview data to text through the production pure helper.
///
/// This feature-only hook lets the property target exercise clipping and
/// ordering without touching files, locks, or undo-backup persistence.
#[cfg(feature = "property-tests")]
#[must_use]
pub fn apply_replacements_to_text_for_property_test(
    original_text: &str,
    replacements: &[Replacement],
) -> Option<(String, usize)> {
    let mut file_replacements: Vec<&Replacement> = replacements.iter().collect();
    match build_replaced_text(original_text, &mut file_replacements, usize::MAX, || false).outcome {
        ReplacementTextOutcome::Replaced {
            new_content,
            replacement_count,
        } => Some((new_content, replacement_count)),
        ReplacementTextOutcome::Unchanged
        | ReplacementTextOutcome::StaleLine { .. }
        | ReplacementTextOutcome::InvalidPreview { .. }
        | ReplacementTextOutcome::OutputLimitExceeded
        | ReplacementTextOutcome::Cancelled => None,
    }
}

/// Apply preview data through the former whole-file line-index algorithm.
///
/// This reference is intentionally feature-only: production must never retain
/// metadata for every source line, while property tests need an independent
/// implementation for visible-result equivalence.
#[cfg(feature = "property-tests")]
#[must_use]
pub fn apply_replacements_to_text_reference_for_property_test(
    original_text: &str,
    replacements: &[Replacement],
) -> Option<(String, usize)> {
    let mut file_replacements: Vec<&Replacement> = replacements.iter().collect();
    match build_replaced_text_reference(original_text, &mut file_replacements) {
        ReplacementTextOutcome::Replaced {
            new_content,
            replacement_count,
        } => Some((new_content, replacement_count)),
        ReplacementTextOutcome::Unchanged
        | ReplacementTextOutcome::StaleLine { .. }
        | ReplacementTextOutcome::InvalidPreview { .. }
        | ReplacementTextOutcome::OutputLimitExceeded
        | ReplacementTextOutcome::Cancelled => None,
    }
}

#[cfg(any(test, feature = "property-tests"))]
fn build_replaced_text_reference(
    original_text: &str,
    file_replacements: &mut [&Replacement],
) -> ReplacementTextOutcome {
    file_replacements.sort_by(|a, b| {
        a.line_number
            .cmp(&b.line_number)
            .then(a.match_range.start.cmp(&b.match_range.start))
            .then(a.match_range.end.cmp(&b.match_range.end))
    });

    let line_spans = line_spans_reference(original_text);
    let mut edits = Vec::with_capacity(file_replacements.len());
    for replacement in file_replacements.iter() {
        let Some(line_index) = replacement.line_number.checked_sub(1) else {
            return ReplacementTextOutcome::InvalidPreview {
                reason: "line numbers must be one-based",
            };
        };
        let Ok(line_index) = usize::try_from(line_index) else {
            return ReplacementTextOutcome::StaleLine {
                line_number: replacement.line_number,
            };
        };
        let Some(line_span) = line_spans.get(line_index).cloned() else {
            return ReplacementTextOutcome::StaleLine {
                line_number: replacement.line_number,
            };
        };
        let line = &original_text[line_span.clone()];
        if line != replacement.original_line.as_ref() {
            return ReplacementTextOutcome::StaleLine {
                line_number: replacement.line_number,
            };
        }
        let start = line.floor_char_boundary(replacement.match_range.start.min(line.len()));
        let end = line.ceil_char_boundary(replacement.match_range.end.min(line.len()));
        if start <= end {
            edits.push(PendingEdit {
                start: line_span.start + start,
                end: line_span.start + end,
                replacement: replacement.replacement.as_ref(),
            });
        }
    }

    if edits.is_empty() {
        return ReplacementTextOutcome::Unchanged;
    }

    let mut new_content = String::with_capacity(replaced_capacity(original_text.len(), &edits));
    let mut cursor = 0usize;
    for edit in &edits {
        new_content.push_str(&original_text[cursor..edit.start]);
        new_content.push_str(edit.replacement);
        cursor = edit.end;
    }
    new_content.push_str(&original_text[cursor..]);
    ReplacementTextOutcome::Replaced {
        new_content,
        replacement_count: edits.len(),
    }
}

/// Restore files from backup (undo Replace All).
///
/// Writes each file atomically (temp file + rename), but only when the file's
/// current bytes still match the Replace All output snapshot. Per-file failures
/// stay in `remaining_backup` so the UI can keep undo available for retry.
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
    let mut restored_count = 0usize;
    let mut skipped_paths = BoundedDiagnosticSample::default();
    let mut failed_paths = BoundedDiagnosticSample::default();
    let mut restored_open_paths = Vec::new();
    let mut remaining_backup = ReplaceUndoBackup::new();

    for (path, entry) in backup {
        let Ok(_lock) = fs_write::TargetWriteGuard::acquire(path) else {
            failed_paths.record_path(path);
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        };

        let Ok(current_facts) = fs_metadata::file_facts(path) else {
            failed_paths.record_path(path);
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        };
        let original_len = u64::try_from(entry.original_bytes.len()).unwrap_or(u64::MAX);
        let replaced_len = u64::try_from(entry.replaced_bytes.len()).unwrap_or(u64::MAX);
        if current_facts.byte_size != original_len && current_facts.byte_size != replaced_len {
            skipped_paths.record_path(path);
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        }
        if current_facts.byte_size > MAX_REPLACE_FILE_BYTES {
            skipped_paths.record_path(path);
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        }

        run_undo_after_metadata_hook_for_test(path);
        let current_bytes = match fs_read::bounded_bytes(
            path,
            MAX_REPLACE_FILE_BYTES,
            current_facts.byte_size,
            || false,
        ) {
            Ok(bytes) => bytes,
            Err(fs_read::BoundedFileReadError::LimitExceeded { .. }) => {
                skipped_paths.record_path(path);
                remaining_backup.insert(path.clone(), entry.clone());
                continue;
            }
            Err(fs_read::BoundedFileReadError::Cancelled) => {
                unreachable!("Undo uses a non-cancelling bounded reader")
            }
            Err(fs_read::BoundedFileReadError::Io(_)) => {
                failed_paths.record_path(path);
                remaining_backup.insert(path.clone(), entry.clone());
                continue;
            }
        };

        if current_bytes == entry.original_bytes {
            restored_count = restored_count.saturating_add(1);
            record_open_path_intersection(
                &mut restored_open_paths,
                path,
                current_facts.canonical_path.as_deref(),
                open_canonical_identities,
            );
            continue;
        }

        if current_bytes != entry.replaced_bytes {
            skipped_paths.record_path(path);
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        }

        if atomic_write(path, &entry.original_bytes).is_err() {
            failed_paths.record_path(path);
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        }

        restored_count = restored_count.saturating_add(1);
        record_open_path_intersection(
            &mut restored_open_paths,
            path,
            current_facts.canonical_path.as_deref(),
            open_canonical_identities,
        );
    }

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

fn record_open_path_intersection(
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

/// Distinguishes write failures before and after the destination rename.
#[derive(Debug)]
enum ReplaceWriteError {
    /// The final path should still contain its previous bytes.
    BeforeRename(anyhow::Error),
    /// The rename already succeeded, but making the directory entry durable failed.
    AfterRename(anyhow::Error),
}

impl std::fmt::Display for ReplaceWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BeforeRename(error) | Self::AfterRename(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ReplaceWriteError {}

/// Atomically write bytes to a file through the shared durable-write helper.
///
/// Delegating to `durable_write` means Replace All inherits the same identity
/// metadata preservation (mode bits, ownership, ACLs, xattrs) and the same
/// before/after-rename failure classification as in-editor saves, instead of
/// re-implementing the contract here.
fn atomic_write(path: &Path, content: &[u8]) -> Result<(), ReplaceWriteError> {
    if take_replace_before_rename_failure_for_test(path) {
        return Err(ReplaceWriteError::BeforeRename(anyhow::anyhow!(
            "Injected pre-rename Replace All failure for {}",
            path.display()
        )));
    }
    let write_path = fs_write::resolve_target_identity(path)
        .map_err(|source| {
            ReplaceWriteError::BeforeRename(anyhow::anyhow!(
                "Failed to resolve write target {}: {source}",
                path.display()
            ))
        })?
        .into_path_buf();
    fs_write::atomic_replace(&write_path, WriteLabel::REPLACE, content).map_err(|error| match error
    {
        fs_write::DurableWriteError::BeforeRename(source) => ReplaceWriteError::BeforeRename(
            anyhow::anyhow!("Failed to write {}: {source}", path.display()),
        ),
        fs_write::DurableWriteError::AfterRename(source) => {
            ReplaceWriteError::AfterRename(anyhow::anyhow!(
                "Failed to sync parent directory for {}: {source}",
                path.display()
            ))
        }
    })
}

/// Persist the current undo backup snapshot or delete the journal when empty.
fn persist_undo_backup(data_dir: &Path, backup: &ReplaceUndoBackup) -> anyhow::Result<()> {
    if backup.is_empty() {
        search_backup::delete(data_dir)
    } else {
        search_backup::save(data_dir, backup)
    }
}

/// Restore already-written files in reverse order when cancellation interrupts a run.
fn rollback_applied_files(backup: &ReplaceUndoBackup) -> BoundedDiagnosticSample {
    let mut errors = BoundedDiagnosticSample::default();
    for (path, entry) in backup.iter().rev() {
        let Ok(_guard) = fs_write::TargetWriteGuard::acquire(path) else {
            errors.push(format!("Failed to lock {} for rollback", path.display()));
            continue;
        };
        if let Err(e) = atomic_write(path, &entry.original_bytes) {
            errors.push(format!("Failed to restore {}: {e}", path.display()));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use std::collections::HashSet;
    use std::sync::Arc;
    use tempfile::tempdir;

    /// Helper: create a Replacement struct for testing.
    fn make_replacement(
        path: &Path,
        line_number: u64,
        original_line: &str,
        replacement: &str,
        match_range: std::ops::Range<usize>,
    ) -> Replacement {
        let mut replaced_line = original_line.to_string();
        let start = match_range.start.min(replaced_line.len());
        let end = match_range.end.min(replaced_line.len());
        replaced_line.replace_range(start..end, replacement);
        Replacement {
            match_id: crate::model::content_search::SearchMatchId::from_index(0),
            path: path.to_path_buf(),
            line_number,
            original_line: original_line.to_string().into(),
            replaced_line,
            replacement: replacement.to_string().into(),
            match_range,
        }
    }

    #[test]
    fn replace_all_byte_budgets_match_documented_limits() {
        assert_eq!(MAX_REPLACE_FILE_BYTES, 10 * 1024 * 1024);
        assert_eq!(MAX_REPLACE_UNDO_BYTES, 64 * 1024 * 1024);
    }

    #[test]
    fn undo_retained_weight_charges_table_paths_and_vector_capacities() {
        let mut backup = ReplaceUndoBackup::new();
        let mut path = PathBuf::from("/workspace/retained.rs");
        path.reserve(8 * 1024);
        let path_capacity = path.capacity();
        let mut original_bytes = Vec::with_capacity(16 * 1024);
        original_bytes.extend_from_slice(b"before");
        let original_capacity = original_bytes.capacity();
        let mut replaced_bytes = Vec::with_capacity(32 * 1024);
        replaced_bytes.extend_from_slice(b"after");
        let replaced_capacity = replaced_bytes.capacity();
        backup.insert(path, ReplaceUndoEntry::new(original_bytes, replaced_bytes));

        let nested_capacity = path_capacity
            .saturating_add(original_capacity)
            .saturating_add(replaced_capacity);
        assert!(
            replace_undo_retained_byte_weight(&backup)
                > u64::try_from(nested_capacity).unwrap_or(u64::MAX),
            "the hash table allocation must be charged in addition to nested owners"
        );
    }

    #[test]
    fn replacement_count_cap_is_rejected_before_any_target_mutation() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("count-cap.txt");
        fixture::write_text(&file, "needle\n");
        let replacement = make_replacement(&file, 1, "needle", "thread", 0..6);
        let replacements = vec![replacement; MAX_REPLACE_PREVIEW_ROWS + 1];
        let cancel = AtomicBool::new(false);

        let error = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect_err("over-cap replacement selections must be rejected");

        assert!(error.to_string().contains("replacement limit"));
        assert_eq!(fixture::read_text(&file), "needle\n");
    }

    #[test]
    fn test_apply_replacements_literal() {
        let dir = tempdir().expect("expected operation to succeed");
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        fixture::write_text(&file_a, "let hello = 1;\nlet world = 2;\n");
        fixture::write_text(&file_b, "fn hello() {}\n");

        let replacements = vec![
            make_replacement(&file_a, 1, "let hello = 1;", "goodbye", 4..9),
            make_replacement(&file_b, 1, "fn hello() {}", "goodbye", 3..8),
        ];

        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("expected operation to succeed")
            .into_parts();

        assert_eq!(result.replaced_count, 2);
        assert_eq!(result.files_affected, 2);
        assert_eq!(result.skipped_count, 0);

        let content_a = fixture::read_text(&file_a);
        assert!(
            content_a.contains("goodbye"),
            "a.rs should have replacement"
        );
        assert!(
            !content_a.contains("hello"),
            "a.rs should not have original"
        );

        let content_b = fixture::read_text(&file_b);
        assert!(
            content_b.contains("goodbye"),
            "b.rs should have replacement"
        );

        assert_eq!(backup.len(), 2, "backup should contain both files");
    }

    #[test]
    fn test_apply_replacements_preserves_backup() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        let original = "let needle = 42;\n";
        fixture::write_text(&file, original);

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        let cancel = AtomicBool::new(false);
        let (_, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("expected operation to succeed")
            .into_parts();

        assert_eq!(backup[&file].original_bytes, original.as_bytes());
        assert_eq!(
            backup[&file].replaced_bytes,
            b"let haystack = 42;\n".as_slice()
        );
    }

    #[test]
    fn test_apply_replacements_persists_undo_journal_before_success() {
        let dir = tempdir().expect("expected operation to succeed");
        let journal_dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        fixture::write_text(&file, "let needle = 42;\n");

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        TEST_REQUIRE_ACTIVE_JOURNAL_BEFORE_WRITE.with(|required| required.set(true));
        let cancel = AtomicBool::new(false);
        let (_, backup) = apply_replacements(
            &replacements,
            &HashSet::new(),
            &cancel,
            Some(journal_dir.path()),
        )
        .expect("expected operation to succeed")
        .into_parts();
        TEST_REQUIRE_ACTIVE_JOURNAL_BEFORE_WRITE.with(|required| required.set(false));

        let persisted =
            search_backup::load(journal_dir.path()).expect("expected operation to succeed");
        assert_eq!(persisted, backup);
    }

    #[test]
    fn post_rename_durability_failure_keeps_changed_bytes_and_undo_evidence() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("durability-ambiguous.txt");
        fixture::write_text(&file, "needle\n");
        let replacements = vec![make_replacement(&file, 1, "needle", "thread", 0..6)];
        let cancel = AtomicBool::new(false);
        crate::services::filesystem::write::fail_next_parent_sync_for_test();

        let outcome = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("post-rename durability ambiguity still changed the target");
        let (result, backup) = outcome.into_parts();

        assert_eq!(fixture::read_text(&file), "thread\n");
        assert_eq!(result.replaced_count, 1);
        assert_eq!(result.files_affected, 1);
        assert_eq!(result.error_count, 1);
        assert!(result.error_sample[0].contains("durability sync failed"));
        assert_eq!(backup[&file].original_bytes, b"needle\n");
        assert_eq!(backup[&file].replaced_bytes, b"thread\n");
    }

    #[test]
    fn stale_reserved_apply_cannot_overwrite_newer_committed_journal_or_file() {
        let dir = tempdir().expect("replace target tempdir");
        let journal_dir = tempdir().expect("replace journal tempdir");
        let file = dir.path().join("test.rs");
        fixture::write_text(&file, "needle\n");
        let replacements = vec![make_replacement(&file, 1, "needle", "stale", 0..6)];

        let newer_path = dir.path().join("newer.rs");
        let mut newer_backup = ReplaceUndoBackup::new();
        newer_backup.insert(
            newer_path,
            ReplaceUndoEntry::new(b"before".to_vec(), b"after".to_vec()),
        );
        search_backup::save(journal_dir.path(), &newer_backup)
            .expect("commit newer replacement journal");

        let generation = Arc::new(AtomicU32::new(2));
        let stale = ReplaceJournalFreshness::new(generation, 1);
        let cancel = AtomicBool::new(false);
        let outcome = apply_replacements_if_current(
            &replacements,
            &HashSet::new(),
            &HashSet::new(),
            &cancel,
            journal_dir.path(),
            &stale,
        )
        .expect("stale apply freshness check");

        assert!(outcome.is_none());
        assert_eq!(fixture::read_text(&file), "needle\n");
        assert_eq!(
            search_backup::load(journal_dir.path()).expect("load newer journal"),
            newer_backup
        );
    }

    #[test]
    fn test_apply_replacements_waits_for_existing_save_guard_on_same_target() {
        use std::sync::mpsc;
        use std::time::Duration;

        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        fixture::write_text(&file, "needle\n");
        let replacements = vec![make_replacement(&file, 1, "needle", "replaced", 0..6)];
        let save_guard =
            fs_write::TargetWriteGuard::acquire(&file).expect("simulate in-flight save guard");
        let (tx, rx) = mpsc::channel();

        let worker = std::thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);
            tx.send(result).expect("send replace result");
        });

        assert!(
            rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "Replace All should wait while an editor save holds the stable target guard"
        );
        assert_eq!(
            fixture::read_text(&file),
            "needle\n",
            "Replace All must not read/write through the held save guard"
        );

        drop(save_guard);
        let result = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("replace should finish after save guard drops")
            .expect("replace should succeed");
        worker.join().expect("replace worker should join");

        assert_eq!(result.result.files_affected, 1);
        assert_eq!(fixture::read_text(&file), "replaced\n");
    }

    #[test]
    fn test_apply_replacements_waits_for_startup_journal_recovery_guard() {
        use std::sync::mpsc;
        use std::time::Duration;

        let dir = tempdir().expect("replace target tempdir");
        let journal_dir = tempdir().expect("replace journal tempdir");
        let file = dir.path().join("test.rs");
        fixture::write_text(&file, "needle\n");
        let replacements = vec![make_replacement(&file, 1, "needle", "replaced", 0..6)];
        let recovery_guard =
            search_backup::acquire_journal_guard().expect("simulate startup journal recovery");
        let journal_path = journal_dir.path().to_path_buf();
        let (tx, rx) = mpsc::channel();

        let worker = std::thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            let result =
                apply_replacements(&replacements, &HashSet::new(), &cancel, Some(&journal_path));
            tx.send(result).expect("send replace result");
        });

        assert!(
            rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "Replace All must wait while startup owns journal recovery"
        );
        assert_eq!(fixture::read_text(&file), "needle\n");

        drop(recovery_guard);
        let result = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("replace should finish after recovery guard drops")
            .expect("replace should succeed");
        worker.join().expect("replace worker should join");

        assert_eq!(result.result.files_affected, 1);
        assert_eq!(fixture::read_text(&file), "replaced\n");
        assert!(
            search_backup::load(journal_dir.path())
                .expect("load active journal")
                .contains_key(&file)
        );
    }

    #[test]
    fn test_apply_replacements_skips_over_file_size_cap_before_reading() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("huge.rs");
        fixture::create_sparse_file(&file, MAX_REPLACE_FILE_BYTES + 1);

        let replacements = vec![make_replacement(&file, 1, "needle", "replaced", 0..6)];
        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("oversized files should be reported as skipped, not fatal")
            .into_parts();

        assert_eq!(result.replaced_count, 0);
        assert_eq!(result.files_affected, 0);
        assert_eq!(result.skipped_count, 1);
        assert_eq!(result.skipped_sample[0], file.display().to_string());
        assert!(result.error_sample[0].contains("larger than the 10 MB"));
        assert!(backup.is_empty());
    }

    #[test]
    fn test_apply_replacements_allows_file_at_exact_size_cap() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("exact.rs");
        let original_line = "a".repeat(
            usize::try_from(MAX_REPLACE_FILE_BYTES)
                .expect("replace file byte cap should fit in usize"),
        );
        fixture::write_text(&file, &original_line);
        let replacements = vec![make_replacement(&file, 1, &original_line, "b", 0..1)];
        let cancel = AtomicBool::new(false);

        let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("exact file-size cap should still be replaceable")
            .into_parts();

        assert_eq!(result.files_affected, 1);
        assert_eq!(result.replaced_count, 1);
        assert_eq!(result.skipped_count, 0);
        assert_eq!(result.error_count, 0);
        assert_eq!(
            fs_metadata::file_facts(&file)
                .expect("stat replaced file")
                .byte_size,
            MAX_REPLACE_FILE_BYTES
        );
        assert!(backup.contains_key(&file));
    }

    #[test]
    fn test_apply_replacements_skips_file_that_would_exceed_undo_cap() {
        let dir = tempdir().expect("expected operation to succeed");
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        fixture::write_text(&file_a, "needle-a\n");
        fixture::write_text(&file_b, "needle-b\n");

        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(Some(24)));
        let replacements = vec![
            make_replacement(&file_a, 1, "needle-a", "done-a", 0..8),
            make_replacement(&file_b, 1, "needle-b", "done-b", 0..8),
        ];
        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("undo cap should skip later files without failing prior writes")
            .into_parts();
        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(None));

        assert_eq!(result.files_affected, 1);
        assert_eq!(result.skipped_count, 1);
        assert_eq!(result.skipped_sample[0], file_b.display().to_string());
        assert!(result.error_sample[0].contains("undo data would exceed"));
        assert_eq!(fixture::read_text(&file_a), "done-a\n");
        assert_eq!(fixture::read_text(&file_b), "needle-b\n");
        assert!(backup.contains_key(&file_a));
        assert!(!backup.contains_key(&file_b));
    }

    #[test]
    fn test_apply_replacements_allows_entry_at_exact_undo_cap() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("exact-undo.rs");
        fixture::write_text(&file, "needle\n");
        let replacements = vec![make_replacement(&file, 1, "needle", "done", 0..6)];
        let exact_payload = u64::try_from("needle\n".len() + "done\n".len())
            .expect("tiny undo payload should fit in u64");

        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(Some(exact_payload)));
        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("exact undo cap should still be replaceable")
            .into_parts();
        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(None));

        assert_eq!(result.files_affected, 1);
        assert_eq!(result.replaced_count, 1);
        assert_eq!(result.skipped_count, 0);
        assert_eq!(result.error_count, 0);
        assert_eq!(fixture::read_text(&file), "done\n");
        assert!(backup.contains_key(&file));
    }

    #[test]
    fn test_apply_replacements_rejects_entry_one_byte_over_undo_cap_without_write() {
        let dir = tempdir().expect("replace tempdir");
        let file = dir.path().join("one-over-undo.rs");
        fixture::write_text(&file, "needle\n");
        let replacements = vec![make_replacement(&file, 1, "needle", "done", 0..6)];
        let exact_payload = u64::try_from("needle\n".len() + "done\n".len())
            .expect("tiny undo payload should fit in u64");

        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(Some(exact_payload - 1)));
        let outcome = apply_replacements(
            &replacements,
            &HashSet::new(),
            &AtomicBool::new(false),
            None,
        )
        .expect("one-byte-over undo payload should be skipped safely");
        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(None));

        assert_eq!(outcome.result.files_affected, 0);
        assert_eq!(outcome.result.replaced_count, 0);
        assert_eq!(outcome.result.skipped_count, 1);
        assert_eq!(outcome.result.error_count, 1);
        assert!(outcome.result.error_sample[0].contains("undo data would exceed"));
        assert!(outcome.undo_backup.is_empty());
        assert_eq!(outcome.metrics.undo_live_bytes, 0);
        assert_eq!(fixture::read_text(&file), "needle\n");
    }

    #[test]
    fn pre_rename_failure_reclaims_live_charge_and_allows_later_sorted_target() {
        let dir = tempdir().expect("replace tempdir");
        let journal_dir = tempdir().expect("journal tempdir");
        let first = dir.path().join("a-first.txt");
        let later = dir.path().join("b-later.txt");
        fixture::write_text(&first, "needle\n");
        fixture::write_text(&later, "needle\n");
        let replacements = vec![
            make_replacement(&first, 1, "needle", "done", 0..6),
            make_replacement(&later, 1, "needle", "done", 0..6),
        ];
        let one_entry =
            u64::try_from("needle\n".len() + "done\n".len()).expect("fixture payload fits u64");
        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(Some(one_entry)));
        fail_next_replace_before_rename_for_path_for_test(&first);

        let outcome = apply_replacements(
            &replacements,
            &HashSet::new(),
            &AtomicBool::new(false),
            Some(journal_dir.path()),
        )
        .expect("later target should proceed after reclaim");
        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(None));

        assert_eq!(fixture::read_text(&first), "needle\n");
        assert_eq!(fixture::read_text(&later), "done\n");
        assert_eq!(outcome.result.files_affected, 1);
        assert_eq!(outcome.result.error_count, 1);
        assert_eq!(outcome.metrics.undo_live_bytes, one_entry);
        assert_eq!(outcome.metrics.undo_bytes, one_entry);
        assert!(!outcome.undo_backup.contains_key(&first));
        assert!(outcome.undo_backup.contains_key(&later));
        let persisted = search_backup::load(journal_dir.path()).expect("active journal");
        assert!(!persisted.contains_key(&first));
        assert!(persisted.contains_key(&later));
    }

    #[test]
    fn test_build_replaced_text_preserves_original_line_endings() {
        let path = PathBuf::from("/mixed.txt");
        let original = "alpha needle\r\nbeta needle\n";
        let replacements = [
            make_replacement(&path, 1, "alpha needle", "hay", 6..12),
            make_replacement(&path, 2, "beta needle", "stack", 5..11),
        ];
        let mut refs: Vec<&Replacement> = replacements.iter().collect();

        let build = build_replaced_text(original, &mut refs, usize::MAX, || false);

        assert_eq!(
            build.outcome,
            ReplacementTextOutcome::Replaced {
                new_content: "alpha hay\r\nbeta stack\n".to_string(),
                replacement_count: 2,
            }
        );
        assert_eq!(build.metrics.source_lines, 2);
        assert_eq!(build.metrics.accepted_replacements, 2);
    }

    #[test]
    fn streaming_builder_matches_reference_for_empty_unicode_and_unterminated_lines() {
        let path = PathBuf::from("/semantic-fixture.txt");
        let original = "\nα needle\r\nlast🙂needle";
        let replacements = [
            make_replacement(&path, 1, "", "empty", 0..0),
            make_replacement(&path, 2, "α needle", "thread", 3..9),
            make_replacement(&path, 3, "last🙂needle", "done", 8..14),
        ];
        let mut streaming_refs: Vec<&Replacement> = replacements.iter().collect();
        let mut reference_refs: Vec<&Replacement> = replacements.iter().collect();

        let streaming = build_replaced_text(original, &mut streaming_refs, usize::MAX, || false);
        let reference = build_replaced_text_reference(original, &mut reference_refs);

        assert_eq!(streaming.outcome, reference);
        assert_eq!(
            streaming.outcome,
            ReplacementTextOutcome::Replaced {
                new_content: "empty\nα thread\r\nlast🙂done".to_string(),
                replacement_count: 3,
            }
        );
        assert_eq!(streaming.metrics.source_lines, 3);
    }

    #[test]
    fn dense_short_line_fixture_retains_only_replacement_sized_metadata() {
        let byte_limit = usize::try_from(MAX_REPLACE_FILE_BYTES)
            .expect("Replace All file-byte limit should fit usize");
        let source_line_count = byte_limit / 2;
        let original = "x\n".repeat(source_line_count);
        let path = PathBuf::from("/dense-short-lines.txt");
        let original_line: Arc<str> = Arc::from("x");
        let replacement_text: Arc<str> = Arc::from("y");
        let replacements: Vec<_> = (0..MAX_REPLACE_PREVIEW_ROWS)
            .map(|index| {
                let line_index = index.saturating_mul(source_line_count.saturating_sub(1))
                    / MAX_REPLACE_PREVIEW_ROWS.saturating_sub(1);
                Replacement {
                    match_id: crate::model::content_search::SearchMatchId::from_index(index),
                    path: path.clone(),
                    line_number: u64::try_from(line_index + 1)
                        .expect("dense fixture line number should fit u64"),
                    original_line: original_line.clone(),
                    replaced_line: "y".to_string(),
                    replacement: replacement_text.clone(),
                    match_range: 0..1,
                }
            })
            .collect();
        let mut refs: Vec<&Replacement> = replacements.iter().collect();

        let build = build_replaced_text(&original, &mut refs, byte_limit, || false);

        let ReplacementTextOutcome::Replaced {
            new_content,
            replacement_count,
        } = build.outcome
        else {
            panic!("near-cap dense-line fixture should construct output");
        };
        assert_eq!(replacement_count, MAX_REPLACE_PREVIEW_ROWS);
        assert_eq!(new_content.len(), original.len());
        assert_eq!(
            build.metrics.source_lines,
            u64::try_from(source_line_count).expect("source-line fixture count should fit u64")
        );
        assert_eq!(
            build.metrics.retained_edit_records,
            MAX_REPLACE_PREVIEW_ROWS
        );
        assert!(
            build.metrics.retained_edit_records < source_line_count,
            "retained metadata must follow replacements, not source lines"
        );
        assert_eq!(
            build.metrics.retained_edit_bytes,
            MAX_REPLACE_PREVIEW_ROWS * std::mem::size_of::<PendingEdit<'static>>()
        );
        assert_eq!(
            build.metrics.output_bytes,
            u64::try_from(original.len()).expect("output fixture bytes should fit u64")
        );
    }

    #[test]
    fn malformed_ranges_are_rejected_before_line_discovery_or_output_allocation() {
        let path = PathBuf::from("/malformed.txt");
        let mut reversed = make_replacement(&path, 1, "abcdef", "x", 2..4);
        reversed.match_range = std::ops::Range { start: 4, end: 2 };
        let mut reversed_refs = vec![&reversed];

        let reversed_build =
            build_replaced_text("abcdef\n", &mut reversed_refs, usize::MAX, || false);

        assert_eq!(
            reversed_build.outcome,
            ReplacementTextOutcome::InvalidPreview {
                reason: "replacement range endpoints are reversed",
            }
        );
        assert_eq!(reversed_build.metrics.source_lines, 0);
        assert_eq!(reversed_build.metrics.retained_edit_records, 0);

        let overlapping = [
            make_replacement(&path, 1, "abcdef", "x", 0..3),
            make_replacement(&path, 1, "abcdef", "y", 2..5),
        ];
        let mut overlapping_refs: Vec<&Replacement> = overlapping.iter().collect();
        let overlapping_build =
            build_replaced_text("abcdef\n", &mut overlapping_refs, usize::MAX, || false);

        assert_eq!(
            overlapping_build.outcome,
            ReplacementTextOutcome::InvalidPreview {
                reason: "replacement ranges overlap",
            }
        );
        assert_eq!(overlapping_build.metrics.source_lines, 0);
        assert_eq!(overlapping_build.metrics.retained_edit_records, 0);
    }

    #[test]
    fn output_limit_is_checked_before_output_allocation() {
        let path = PathBuf::from("/output-limit.txt");
        let replacement = make_replacement(&path, 1, "x", "expanded", 0..1);
        let mut refs = vec![&replacement];

        let build = build_replaced_text("x\n", &mut refs, 2, || false);

        assert_eq!(build.outcome, ReplacementTextOutcome::OutputLimitExceeded);
        assert_eq!(build.metrics.source_lines, 1);
        assert_eq!(build.metrics.output_bytes, 0);
    }

    #[test]
    fn streaming_line_discovery_observes_cancellation_between_dense_chunks() {
        let path = PathBuf::from("/cancel-dense.txt");
        let original = "x\n".repeat(5_000);
        let replacement = make_replacement(&path, 5_000, "x", "y", 0..1);
        let mut refs = vec![&replacement];
        let cancellation_checks = Cell::new(0usize);

        let build = build_replaced_text(&original, &mut refs, usize::MAX, || {
            let next = cancellation_checks.get().saturating_add(1);
            cancellation_checks.set(next);
            next >= 4
        });

        assert_eq!(build.outcome, ReplacementTextOutcome::Cancelled);
        assert!(build.metrics.source_lines < 5_000);
        assert_eq!(build.metrics.output_bytes, 0);
    }

    #[test]
    fn test_line_spans_handles_leading_newline_without_underflow() {
        assert_eq!(line_spans_reference("\nnext"), vec![0..0, 1..5]);
        assert_eq!(line_spans_reference("\r\nnext"), vec![0..0, 2..6]);
    }

    #[test]
    fn test_line_spans_does_not_add_empty_trailing_line() {
        assert_eq!(line_spans_reference("first\n"), vec![0..5]);
        assert_eq!(line_spans_reference("first\r\n"), vec![0..5]);
        assert_eq!(line_spans_reference("\n"), vec![0..0]);
    }

    #[test]
    fn test_replaced_capacity_tracks_exact_growth_and_shrink() {
        let edits = [
            PendingEdit {
                start: 0,
                end: 5,
                replacement: "hi",
            },
            PendingEdit {
                start: 8,
                end: 10,
                replacement: "there!",
            },
        ];

        assert_eq!(replaced_capacity(12, &edits), 13);
    }

    #[test]
    fn test_apply_replacements_aborts_when_undo_journal_cannot_be_persisted() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        let journal_path = dir.path().join("journal-is-a-file");
        fixture::write_text(&file, "let needle = 42;\n");
        fixture::write_text(&journal_path, "not a directory");

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        let cancel = AtomicBool::new(false);
        let result =
            apply_replacements(&replacements, &HashSet::new(), &cancel, Some(&journal_path));

        assert!(
            result
                .expect_err("journal failure should abort before file mutation")
                .to_string()
                .contains("undo journal"),
        );
        assert_eq!(
            fixture::read_text(&file),
            "let needle = 42;\n",
            "file bytes must stay unchanged when the undo journal cannot be saved",
        );
    }

    #[test]
    fn test_apply_replacements_treats_line_numbers_past_eof_as_stale() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        fixture::write_text(&file, "needle\n");

        let replacements = vec![make_replacement(&file, 2, "needle", "replaced", 0..6)];

        let cancel = AtomicBool::new(false);
        let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);

        assert!(
            result
                .expect_err("out-of-range search row should make the preview stale")
                .to_string()
                .contains("line 2 changed since search")
        );
        assert_eq!(fixture::read_text(&file), "needle\n");
    }

    #[test]
    fn test_apply_replacements_keeps_partial_success_errors_in_result() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        let missing = dir.path().join("missing.rs");
        fixture::write_text(&file, "needle\n");

        let replacements = vec![
            make_replacement(&file, 1, "needle", "replaced", 0..6),
            make_replacement(&missing, 1, "needle", "replaced", 0..6),
        ];

        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("successful files should keep the Replace All operation successful")
            .into_parts();

        assert_eq!(result.replaced_count, 1);
        assert_eq!(result.files_affected, 1);
        assert_eq!(result.error_count, 1);
        assert!(result.error_sample[0].contains("Failed to stat"));
        assert_eq!(fixture::read_text(&file), "replaced\n");
        assert!(backup.contains_key(&file));
        assert!(!backup.contains_key(&missing));
    }

    #[test]
    fn record_replacement_success_counts_adds_file_and_replacement_totals() {
        let mut replaced_count = 3;
        let mut files_affected = 2;

        record_replacement_success_counts(&mut replaced_count, &mut files_affected, 4);

        assert_eq!(replaced_count, 7);
        assert_eq!(files_affected, 3);
    }

    #[test]
    fn atomic_write_error_display_includes_inner_error() {
        let error = ReplaceWriteError::AfterRename(anyhow::anyhow!("directory sync failed"));

        assert_eq!(error.to_string(), "directory sync failed");
    }

    #[test]
    fn remaining_count_reports_actual_backup_len() {
        let dir = tempdir().expect("expected operation to succeed");
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        let mut remaining_backup = ReplaceUndoBackup::new();
        remaining_backup.insert(
            file_a,
            ReplaceUndoEntry::new(b"before-a".to_vec(), b"after-a".to_vec()),
        );
        remaining_backup.insert(
            file_b,
            ReplaceUndoEntry::new(b"before-b".to_vec(), b"after-b".to_vec()),
        );
        let outcome = UndoReplaceOutcome {
            restored_count: 0,
            skipped_count: 0,
            failed_count: 0,
            restored_open_paths: Vec::new(),
            skipped_sample: BoundedDiagnosticSample::default(),
            failed_sample: BoundedDiagnosticSample::default(),
            remaining_backup,
        };

        assert_eq!(outcome.remaining_count(), 2);
    }

    #[test]
    fn test_undo_replacements_restores_content() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        let original = "let needle = 42;\n";
        fixture::write_text(&file, original);

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        let cancel = AtomicBool::new(false);
        let (_, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("expected operation to succeed")
            .into_parts();

        assert!(fixture::read_text(&file).contains("haystack"));

        let outcome = undo_replacements(&backup);
        assert_eq!(outcome.restored_count(), 1);
        assert!(outcome.remaining_backup.is_empty());
        assert_eq!(fixture::read_text(&file), original);
    }

    #[test]
    fn test_undo_replacements_drops_entry_when_file_is_already_original() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        fixture::write_text(&file, "before\n");

        let mut backup = ReplaceUndoBackup::new();
        backup.insert(
            file,
            ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
        );

        let outcome = undo_replacements(&backup);

        assert_eq!(outcome.restored_count(), 1);
        assert_eq!(outcome.skipped_count, 0);
        assert_eq!(outcome.failed_count, 0);
        assert!(outcome.remaining_backup.is_empty());
    }

    #[test]
    fn test_undo_replacements_skips_diverged_file_and_keeps_backup() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        fixture::write_text(&file, "let needle = 42;\n");

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        let cancel = AtomicBool::new(false);
        let (_, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("expected operation to succeed")
            .into_parts();
        fixture::write_text(&file, "let user_edit = 42;\n");

        let outcome = undo_replacements(&backup);

        assert_eq!(outcome.restored_count(), 0);
        assert_eq!(outcome.skipped_count, 1);
        assert_eq!(outcome.skipped_sample[0], file.display().to_string());
        assert_eq!(outcome.failed_count, 0);
        assert_eq!(outcome.remaining_backup, backup);
        assert_eq!(
            fixture::read_text(&file),
            "let user_edit = 42;\n",
            "undo must not overwrite edits made after Replace All",
        );
    }

    #[test]
    fn test_undo_replacements_skips_externally_grown_file_before_reading() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        fixture::write_text(&file, "after\n");
        let mut backup = ReplaceUndoBackup::new();
        backup.insert(
            file.clone(),
            ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
        );
        fixture::write_text(&file, "after plus a large external edit\n");

        let outcome = undo_replacements(&backup);

        assert_eq!(outcome.restored_count(), 0);
        assert_eq!(outcome.skipped_count, 1);
        assert_eq!(outcome.skipped_sample[0], file.display().to_string());
        assert_eq!(outcome.failed_count, 0);
        assert_eq!(outcome.remaining_backup, backup);
        assert_eq!(
            fixture::read_text(&file),
            "after plus a large external edit\n"
        );
    }

    #[test]
    fn test_undo_replacements_allows_current_file_at_exact_size_cap() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("exact-cap.txt");
        let replaced_bytes = vec![
            b'x';
            usize::try_from(MAX_REPLACE_FILE_BYTES)
                .expect("replace file byte cap should fit in usize")
        ];
        fixture::write_bytes(&file, &replaced_bytes);
        let mut backup = ReplaceUndoBackup::new();
        backup.insert(
            file.clone(),
            ReplaceUndoEntry::new(b"before\n".to_vec(), replaced_bytes),
        );

        let outcome = undo_replacements(&backup);

        assert_eq!(outcome.restored_count(), 1);
        assert_eq!(outcome.skipped_count, 0);
        assert_eq!(outcome.failed_count, 0);
        assert!(outcome.remaining_backup.is_empty());
        assert_eq!(fixture::read_text(&file), "before\n");
    }

    #[test]
    fn undo_growth_after_metadata_is_skipped_without_unbounded_allocation() {
        let dir = tempdir().expect("undo growth tempdir");
        let file = dir.path().join("grown.txt");
        fixture::write_text(&file, "after\n");
        let mut backup = ReplaceUndoBackup::new();
        backup.insert(
            file.clone(),
            ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
        );
        set_undo_after_metadata_hook_for_test(|path| {
            fixture::write_repeated_bytes(path, b"x", MAX_REPLACE_FILE_BYTES + 1);
        });

        let outcome = undo_replacements(&backup);

        assert_eq!(outcome.restored_count(), 0);
        assert_eq!(outcome.skipped_count, 1);
        assert_eq!(outcome.failed_count, 0);
        assert_eq!(outcome.remaining_backup, backup);
        assert_eq!(
            fs_metadata::file_facts(&file)
                .expect("grown target facts")
                .byte_size,
            MAX_REPLACE_FILE_BYTES + 1
        );
    }

    #[test]
    fn undo_failure_heavy_terminal_keeps_exact_totals_and_bounded_ordered_samples() {
        let dir = tempdir().expect("undo failures tempdir");
        let mut backup = ReplaceUndoBackup::new();
        for index in 0..10_000 {
            let path = dir.path().join(format!("missing-{index:05}.txt"));
            backup.insert(
                path,
                ReplaceUndoEntry::new(b"before".to_vec(), b"after".to_vec()),
            );
        }

        let outcome = undo_replacements(&backup);

        assert_eq!(outcome.failed_count, 10_000);
        assert_eq!(outcome.failed_sample.entries().len(), 32);
        assert!(outcome.failed_sample.retained_bytes() <= 32 * 1024);
        assert!(outcome.failed_sample[0].contains("missing-00000.txt"));
        assert!(outcome.failed_sample[31].contains("missing-00031.txt"));
        assert_eq!(outcome.remaining_count(), 10_000);
    }

    #[test]
    fn undo_returns_only_the_restored_open_identity_intersection() {
        let dir = tempdir().expect("undo open intersection tempdir");
        let open = dir.path().join("open.txt");
        let closed = dir.path().join("closed.txt");
        fixture::write_text(&open, "after\n");
        fixture::write_text(&closed, "after\n");
        let mut backup = ReplaceUndoBackup::new();
        for path in [&open, &closed] {
            backup.insert(
                path.clone(),
                ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
            );
        }
        let open_identity = fs_metadata::canonical_path(&open).expect("open canonical path");

        let outcome =
            undo_replacements_for_open_identities(&backup, &HashSet::from([open_identity]));

        assert_eq!(outcome.restored_count(), 2);
        assert_eq!(outcome.restored_open_paths, vec![open]);
    }

    #[test]
    fn persist_undo_backup_saves_nonempty_journals_and_deletes_empty_ones() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        let mut backup = ReplaceUndoBackup::new();
        backup.insert(
            file,
            ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
        );

        persist_undo_backup(dir.path(), &backup).expect("persist nonempty undo backup");
        assert_eq!(
            search_backup::load(dir.path()).expect("load persisted undo backup"),
            backup
        );

        persist_undo_backup(dir.path(), &ReplaceUndoBackup::new())
            .expect("delete empty undo backup");
        assert!(
            search_backup::load(dir.path())
                .expect("empty persisted undo backup should load")
                .is_empty()
        );
    }

    #[test]
    fn test_undo_replacements_keeps_failed_entries_after_partial_restore() {
        let dir = tempdir().expect("expected operation to succeed");
        let restored_file = dir.path().join("restored.rs");
        let missing_file = dir.path().join("missing.rs");
        fixture::write_text(&restored_file, "after\n");

        let mut backup = ReplaceUndoBackup::new();
        backup.insert(
            restored_file.clone(),
            ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
        );
        backup.insert(
            missing_file.clone(),
            ReplaceUndoEntry::new(b"missing-before\n".to_vec(), b"missing-after\n".to_vec()),
        );

        let outcome = undo_replacements(&backup);

        assert_eq!(outcome.restored_count(), 1);
        assert_eq!(outcome.failed_count, 1);
        assert_eq!(outcome.failed_sample[0], missing_file.display().to_string());
        assert_eq!(outcome.skipped_count, 0);
        assert_eq!(outcome.remaining_count(), 1);
        assert_eq!(
            outcome.remaining_backup.get(&missing_file),
            backup.get(&missing_file),
        );
        assert_eq!(fixture::read_text(&restored_file), "before\n");
    }

    #[test]
    fn test_apply_replacements_reverse_order() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        fixture::write_text(&file, "ab cd\n");

        let replacements = vec![
            make_replacement(&file, 1, "ab cd", "XY", 0..2),
            make_replacement(&file, 1, "ab cd", "ZW", 3..5),
        ];

        let cancel = AtomicBool::new(false);
        let (result, _) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("expected operation to succeed")
            .into_parts();
        assert_eq!(result.replaced_count, 2);

        let content = fixture::read_text(&file);
        assert_eq!(
            content, "XY ZW\n",
            "both replacements should apply correctly"
        );
    }

    #[test]
    fn test_apply_replacements_cancel() {
        let dir = tempdir().expect("expected operation to succeed");
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        fixture::write_text(&file_a, "needle\n");
        fixture::write_text(&file_b, "needle\n");

        let replacements = vec![
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ];

        let cancel = AtomicBool::new(true);
        let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);
        assert!(result.is_err(), "cancelled replace should abort");
        assert_eq!(fixture::read_text(&file_a), "needle\n");
        assert_eq!(fixture::read_text(&file_b), "needle\n");
    }

    #[test]
    #[cfg(unix)]
    fn test_apply_replacements_cancel_rolls_back_applied_files() {
        let dir = tempdir().expect("expected operation to succeed");
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        fixture::write_text(&file_a, "needle\n");
        fixture::write_text(&file_b, "needle\n");
        let replacements = vec![
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ];

        let cancel = Arc::new(AtomicBool::new(false));
        let lock_b =
            fs_write::TargetWriteGuard::acquire(&file_b).expect("expected operation to succeed");
        let cancel_for_worker = cancel.clone();
        let worker = std::thread::spawn(move || {
            apply_replacements(
                &replacements,
                &HashSet::new(),
                cancel_for_worker.as_ref(),
                None,
            )
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if fs_read::text(&file_a).is_ok_and(|content| content.contains("replaced")) {
                cancel.store(true, Ordering::Relaxed);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        drop(lock_b);

        let result = worker.join().expect("expected operation to succeed");

        assert!(result.is_err(), "cancelled replace should roll back");
        assert_eq!(fixture::read_text(&file_a), "needle\n");
        assert_eq!(fixture::read_text(&file_b), "needle\n");
    }

    #[test]
    fn test_apply_replacements_nonexistent_file() {
        let dir = tempdir().expect("expected operation to succeed");
        let missing_file = dir.path().join("does_not_exist.rs");

        let replacements = vec![make_replacement(
            &missing_file,
            1,
            "phantom line",
            "replacement",
            0..7,
        )];

        let cancel = AtomicBool::new(false);
        let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);
        assert!(result.is_err(), "should fail when only file is nonexistent");
    }

    #[test]
    fn replace_all_failure_heavy_summary_is_exact_bounded_and_content_free() {
        let dir = tempdir().expect("replace failure tempdir");
        let private_line: Arc<str> = Arc::from("private-document-sentinel");
        let replacement: Arc<str> = Arc::from("done");
        let replacements = (0..MAX_REPLACE_PREVIEW_ROWS)
            .map(|index| Replacement {
                match_id: crate::model::content_search::SearchMatchId::from_index(index),
                path: dir.path().join(format!("missing-{index:05}.txt")),
                line_number: 1,
                original_line: private_line.clone(),
                replaced_line: "done".to_string(),
                replacement: replacement.clone(),
                match_range: 0..private_line.len(),
            })
            .collect::<Vec<_>>();

        let error = apply_replacements(
            &replacements,
            &HashSet::new(),
            &AtomicBool::new(false),
            None,
        )
        .expect_err("all missing targets should produce one bounded terminal error")
        .to_string();

        assert!(error.contains("Replace All failed: 10000 issue(s)"));
        assert!(error.contains("missing-00000.txt"));
        assert!(error.contains("9999 more omitted"));
        assert!(!error.contains("missing-09999.txt"));
        assert!(!error.contains(private_line.as_ref()));
        assert!(error.len() < 1_024);
    }

    #[test]
    fn test_apply_replacements_skip_paths() {
        let dir = tempdir().expect("expected operation to succeed");
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        fixture::write_text(&file_a, "needle\n");
        fixture::write_text(&file_b, "needle\n");

        let replacements = vec![
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ];

        let mut skip = HashSet::new();
        skip.insert(file_b.clone());

        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &skip, &cancel, None)
            .expect("expected operation to succeed")
            .into_parts();

        assert_eq!(result.replaced_count, 1, "only a.rs should be replaced");
        assert_eq!(result.files_affected, 1);
        assert_eq!(result.skipped_count, 1);
        assert_eq!(result.skipped_sample[0], file_b.display().to_string());
        assert!(backup.contains_key(&file_a));
        assert!(!backup.contains_key(&file_b));
        assert_eq!(fixture::read_text(&file_b), "needle\n");
    }

    #[test]
    fn test_apply_replacements_skips_stale_search_result() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("stale.rs");
        fixture::write_text(&file, "needle changed\n");

        let replacements = vec![make_replacement(&file, 1, "needle", "replaced", 0..6)];

        let cancel = AtomicBool::new(false);
        let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);

        assert!(
            result
                .expect_err("all-stale replace should report why nothing was written")
                .to_string()
                .contains("changed since search")
        );
        assert_eq!(fixture::read_text(&file), "needle changed\n");
    }

    #[test]
    fn test_apply_replacements_skips_whole_file_when_preview_line_is_missing() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("stale-missing-line.rs");
        fixture::write_text(&file, "needle one\n");

        let replacements = vec![
            make_replacement(&file, 1, "needle one", "replaced one", 0..6),
            make_replacement(&file, 2, "needle two", "replaced two", 0..6),
        ];

        let cancel = AtomicBool::new(false);
        let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);

        assert!(
            result
                .expect_err("missing preview line should make the file stale")
                .to_string()
                .contains("line 2 changed since search")
        );
        assert_eq!(
            fixture::read_text(&file),
            "needle one\n",
            "stale preview data must not partially replace earlier lines"
        );
    }

    #[cfg(unix)]
    #[test]
    fn replace_all_and_undo_preserve_file_mode() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("private.rs");
        fixture::write_text(&file, "let needle = 42;\n");
        fixture::set_mode(&file, 0o600);

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "found",
            4..10,
        )];
        let cancel = AtomicBool::new(false);
        let (_result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("replace should succeed")
            .into_parts();

        let mode_after_replace = fixture::mode(&file) & 0o777;
        assert_eq!(
            mode_after_replace, 0o600,
            "Replace All must keep the rewritten file's restrictive mode"
        );

        let outcome = undo_replacements(&backup);
        assert_eq!(outcome.restored_count(), 1, "undo should restore the file");
        let mode_after_undo = fixture::mode(&file) & 0o777;
        assert_eq!(
            mode_after_undo, 0o600,
            "undoing Replace All must also keep the file's restrictive mode"
        );
    }
}
