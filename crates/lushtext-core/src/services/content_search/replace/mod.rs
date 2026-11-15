// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace-all and undo flows for workspace content search.
//!
//! This is the command side of the content-search service. It performs file
//! locking, atomic writes, rollback on cancellation, and undo backup handling
//! without depending on any GTK types.

#[cfg(any(test, feature = "test-utils"))]
use std::cell::Cell;
use std::collections::BTreeMap;
// `BTreeSet` backs only the test/test-utils before-rename failure-seam registry,
// so gate the import to match its sole consumer and stay clean in default builds.
#[cfg(any(test, feature = "test-utils"))]
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(any(test, feature = "test-utils"))]
use std::sync::{Mutex, OnceLock};

use crate::model::content_search::{BoundedDiagnosticSample, ReplaceResult};

/// Largest single file Replace All will read and rewrite.
///
/// Sixteen MiB keeps the whole-file validation and undo snapshot path bounded
/// on ordinary laptops while still covering typical source and notes files.
///
/// Kind: presentation cap; 10 MiB rounded up to a power of two
/// (`power-of-two-thresholds` D12; its bound measured in task 9.5 M9).
/// The aggregate undo payload `MAX_REPLACE_UNDO_BYTES` (64 MiB, unchanged)
/// still bounds a run.
pub const MAX_REPLACE_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// The per-file limit before `power-of-two-thresholds` raised it: a run
/// replaces, in a first pass, every file whose source and output both fit it,
/// and only then the larger files.
///
/// Kind: presentation cap; recorded exception (`power-of-two-thresholds` D12):
/// it must stay `main`'s 10 MiB exactly. The first pass then charges the undo
/// payload exactly as `main` did, so a 10–16 MiB file early in path order can
/// never take the budget of a later file `main` replaced; the second pass only
/// adds files `main` skipped.
const MAIN_REPLACE_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// Largest total undo payload retained for one Replace All run.
///
/// The payload stores before and after bytes for each touched file. Sixty-four
/// megabytes keeps rollback useful without letting one operation consume the
/// editor's broader buffer-memory budget.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const MAX_REPLACE_UNDO_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum complete in-memory undo owner, including paths and table capacity.
///
/// Kind: cap; a power of two, rounded down.
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
static UNDO_AFTER_METADATA_HOOKS: OnceLock<Mutex<BTreeMap<PathBuf, UndoAfterMetadataHook>>> =
    OnceLock::new();

#[cfg(any(test, feature = "test-utils"))]
static FAIL_REPLACE_BEFORE_RENAME_PATHS: OnceLock<Mutex<BTreeSet<PathBuf>>> = OnceLock::new();

/// Override the Replace All undo-payload ceiling on the current test thread.
///
/// Gated to match its only consumer, the `test-utils` re-export in
/// `content_search/mod.rs`. Under plain `cfg(test)` this would compile with
/// zero users — the gate mismatch that `--all-features` cannot see.
#[cfg(feature = "test-utils")]
pub fn set_max_replace_undo_bytes_for_test(limit: Option<u64>) {
    TEST_MAX_REPLACE_UNDO_BYTES.with(|slot| slot.set(limit));
}

/// Cleanup ownership for one armed before-rename failure seam.
///
/// Dropping the guard disarms a still-unconsumed seam so a failed assertion or
/// early test exit cannot leak it into a later operation on the same target.
#[cfg(any(test, feature = "test-utils"))]
#[must_use = "dropping the guard immediately would disarm the failure seam"]
pub struct ReplaceBeforeRenameFailureGuard {
    target: PathBuf,
}

#[cfg(any(test, feature = "test-utils"))]
impl Drop for ReplaceBeforeRenameFailureGuard {
    fn drop(&mut self) {
        if let Some(registry) = FAIL_REPLACE_BEFORE_RENAME_PATHS.get() {
            registry
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&self.target);
        }
    }
}

/// Fail the next Replace All target write for `target` before its rename.
///
/// The registry is keyed by the exact target path, so parallel operations on
/// distinct targets cannot arm or consume one another's seams. The returned
/// guard disarms an unconsumed seam on drop.
#[cfg(any(test, feature = "test-utils"))]
#[must_use = "the guard owns cleanup for an unconsumed before-rename seam"]
pub fn fail_next_replace_before_rename_for_path_for_test(
    target: &Path,
) -> ReplaceBeforeRenameFailureGuard {
    let registry = FAIL_REPLACE_BEFORE_RENAME_PATHS.get_or_init(|| Mutex::new(BTreeSet::new()));
    registry
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(target.to_path_buf());
    ReplaceBeforeRenameFailureGuard {
        target: target.to_path_buf(),
    }
}

/// Report whether a before-rename failure seam is still armed for `target`.
#[cfg(any(test, feature = "test-utils"))]
#[must_use]
pub fn replace_before_rename_failure_is_armed_for_test(target: &Path) -> bool {
    FAIL_REPLACE_BEFORE_RENAME_PATHS
        .get()
        .is_some_and(|registry| {
            registry
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains(target)
        })
}

#[cfg(any(test, feature = "test-utils"))]
fn take_replace_before_rename_failure_for_test(path: &Path) -> bool {
    FAIL_REPLACE_BEFORE_RENAME_PATHS
        .get()
        .is_some_and(|registry| {
            registry
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(path)
        })
}

#[cfg(not(any(test, feature = "test-utils")))]
fn take_replace_before_rename_failure_for_test(_path: &Path) -> bool {
    false
}

/// Cleanup ownership for one registered Undo after-metadata hook.
///
/// Dropping the guard removes a still-unconsumed registration so a failed
/// assertion or early test exit cannot leak its seam into a later operation.
#[cfg(any(test, feature = "test-utils"))]
#[must_use = "dropping the guard immediately would unregister the hook"]
pub struct UndoAfterMetadataHookGuard {
    target: PathBuf,
}

#[cfg(any(test, feature = "test-utils"))]
impl Drop for UndoAfterMetadataHookGuard {
    fn drop(&mut self) {
        if let Some(registry) = UNDO_AFTER_METADATA_HOOKS.get() {
            registry
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&self.target);
        }
    }
}

/// Install a one-shot Undo race seam for `target`, firing after metadata but
/// before bounded ingestion.
///
/// The registry is keyed by the exact owned path the Undo entry carries, so
/// parallel operations on distinct targets cannot consume one another's seams.
/// Registering a second hook for the same target replaces the first.
#[cfg(any(test, feature = "test-utils"))]
#[must_use = "the guard owns cleanup for an unconsumed registration"]
pub fn register_undo_after_metadata_hook_for_test(
    target: &Path,
    hook: impl FnOnce(&Path) + Send + 'static,
) -> UndoAfterMetadataHookGuard {
    let registry = UNDO_AFTER_METADATA_HOOKS.get_or_init(|| Mutex::new(BTreeMap::new()));
    registry
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(target.to_path_buf(), Box::new(hook));
    UndoAfterMetadataHookGuard {
        target: target.to_path_buf(),
    }
}

/// Report whether no Undo after-metadata hooks remain registered.
///
/// This whole-registry assertion is only race-free when the caller owns every
/// registration in the process (for example under nextest's process-per-test
/// isolation). Parallel tests inside one process should use the target-scoped
/// [`undo_after_metadata_hook_is_registered_for_test`] instead.
///
/// Gated to match its only consumer, the `test-utils` re-export in
/// `content_search/mod.rs`.
#[cfg(feature = "test-utils")]
#[must_use]
pub fn undo_after_metadata_hook_registry_is_empty_for_test() -> bool {
    UNDO_AFTER_METADATA_HOOKS.get().is_none_or(|registry| {
        registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    })
}

/// Report whether an Undo after-metadata hook is still registered for `target`.
#[cfg(any(test, feature = "test-utils"))]
#[must_use]
pub fn undo_after_metadata_hook_is_registered_for_test(target: &Path) -> bool {
    UNDO_AFTER_METADATA_HOOKS.get().is_some_and(|registry| {
        registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(target)
    })
}

#[cfg(any(test, feature = "test-utils"))]
fn run_undo_after_metadata_hook_for_test(path: &Path) {
    let hook = UNDO_AFTER_METADATA_HOOKS.get().and_then(|registry| {
        registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(path)
    });
    // Invoke after the registry lock is released so hooks for distinct targets
    // can execute concurrently and a hook driving another Undo cannot deadlock.
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

mod apply;
mod stream;
#[cfg(test)]
mod tests;
mod undo;

pub use apply::apply_replacements;
pub(crate) use apply::apply_replacements_if_current;
#[cfg(feature = "property-tests")]
pub use stream::{
    apply_replacements_to_text_for_property_test,
    apply_replacements_to_text_reference_for_property_test,
};
pub use undo::{undo_replacements, undo_replacements_for_open_identities};
