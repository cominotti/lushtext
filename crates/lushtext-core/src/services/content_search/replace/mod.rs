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
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};
#[cfg(any(test, feature = "test-utils"))]
use std::sync::{Mutex, OnceLock};

use crate::model::content_search::{BoundedDiagnosticSample, ReplaceResult, ReplaceSkipReport};

/// Largest single file Replace All will read and rewrite (design D1).
///
/// The open tier, referenced: past it LushText cannot open the file, so no
/// "Open and Replace in Editor" fallback exists, and the skip reason says so.
/// A file whose replacement output would exceed it is skipped too, because
/// Undo's divergence check streams the current file under the same limit.
///
/// Kind: derived (`DocumentScale::REFUSE_TO_OPEN`).
pub const MAX_REPLACE_FILE_BYTES: u64 = crate::model::document_scale::REFUSE_TO_OPEN;
/// `main`'s first-pass per-file limit, replayed by the no-loss ledger
/// (design D4): a first pass, in path order, over the files whose source and
/// output both fit it.
///
/// Kind: presentation cap; recorded exception: it must stay `main`'s 10 MiB.
pub const MAIN_REPLACE_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// `main`'s per-file limit after `power-of-two-thresholds`, replayed by the
/// no-loss ledger's second pass (design D4).
///
/// Kind: presentation cap; recorded exception: it must stay `main`'s 16 MiB.
pub const MAIN_MAX_REPLACE_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// `main`'s aggregate undo payload (source plus output bytes of every admitted
/// file), replayed by the no-loss ledger, and the bound the loader keeps for a
/// legacy inline journal (design D4, D6).
///
/// Kind: presentation cap; recorded exception: it must stay `main`'s 64 MiB.
pub const MAIN_REPLACE_UNDO_BYTES: u64 = 64 * 1024 * 1024;
/// Free space Replace All leaves on the journal's and the target's filesystem
/// beyond a file's own writes, unless `main` would have admitted the file
/// (design D4). Without it an undo journal could fill a user's disk.
///
/// Kind: policy tier; a power of two.
pub const REPLACE_DISK_RESERVE_BYTES: u64 = 1024 * 1024 * 1024;
// The reserve leaves room for every file the no-loss waiver admits: their
// bodies need at most `main`'s undo payload, plus one output at a time.
const _: () =
    assert!(REPLACE_DISK_RESERVE_BYTES >= MAIN_REPLACE_UNDO_BYTES + MAIN_MAX_REPLACE_FILE_BYTES);
/// Maximum complete in-memory undo owner, including paths and table capacity.
///
/// It bounds the descriptor owner that crosses to GTK, about 200 bytes plus
/// the paths per file, so in practice it never binds; the no-loss ledger also
/// charges `main`'s retained weight against it (design D4).
///
/// Kind: cap; a power of two, rounded down.
pub const MAX_REPLACE_UNDO_RETAINED_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static TEST_MAX_REPLACE_UNDO_BYTES: Cell<Option<u64>> = const { Cell::new(None) };
    static TEST_MAX_REPLACE_FILE_BYTES: Cell<Option<u64>> = const { Cell::new(None) };
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

/// Override the Replace All per-file limit on the current test thread, so a
/// cap-boundary test runs at KiB scale instead of allocating 512 MiB.
#[cfg(any(test, feature = "test-utils"))]
pub fn set_max_replace_file_bytes_for_test(limit: Option<u64>) {
    TEST_MAX_REPLACE_FILE_BYTES.with(|slot| slot.set(limit));
}

/// The per-file limit in force: [`MAX_REPLACE_FILE_BYTES`] unless a test
/// overrode it on this thread.
#[must_use]
pub(crate) fn effective_max_replace_file_bytes() -> u64 {
    #[cfg(any(test, feature = "test-utils"))]
    if let Some(limit) = TEST_MAX_REPLACE_FILE_BYTES.with(Cell::get) {
        return limit;
    }
    MAX_REPLACE_FILE_BYTES
}

/// `main`'s undo payload in force for the no-loss ledger: [`MAIN_REPLACE_UNDO_BYTES`]
/// unless a test overrode it on this thread.
#[must_use]
pub(crate) fn effective_main_replace_undo_bytes() -> u64 {
    #[cfg(any(test, feature = "test-utils"))]
    if let Some(limit) = TEST_MAX_REPLACE_UNDO_BYTES.with(Cell::get) {
        return limit;
    }
    MAIN_REPLACE_UNDO_BYTES
}

#[cfg(any(test, feature = "test-utils"))]
type AfterPassAHook = Box<dyn FnOnce(&Path) + Send + 'static>;
#[cfg(any(test, feature = "test-utils"))]
static AFTER_PASS_A_HOOKS: OnceLock<Mutex<BTreeMap<PathBuf, AfterPassAHook>>> = OnceLock::new();

/// Run `hook` once, right after pass A of `target` wrote its undo body and
/// before pass B starts, so a test can play another program writing the file.
#[cfg(any(test, feature = "test-utils"))]
pub fn register_after_pass_a_hook_for_test(
    target: impl Into<PathBuf>,
    hook: impl FnOnce(&Path) + Send + 'static,
) {
    AFTER_PASS_A_HOOKS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(target.into(), Box::new(hook));
}

#[cfg(any(test, feature = "test-utils"))]
fn run_after_pass_a_hook_for_test(path: &Path) {
    let hook = AFTER_PASS_A_HOOKS.get().and_then(|hooks| {
        hooks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(path)
    });
    if let Some(hook) = hook {
        hook(path);
    }
}

#[cfg(not(any(test, feature = "test-utils")))]
#[inline(always)]
fn run_after_pass_a_hook_for_test(_path: &Path) {}

/// The target a before-target-write hook waits for, and the hook.
#[cfg(test)]
type BeforeTargetWriteHook = (PathBuf, Box<dyn FnOnce()>);

#[cfg(test)]
thread_local! {
    static BEFORE_TARGET_WRITE_HOOK: std::cell::RefCell<Option<BeforeTargetWriteHook>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `hook` once on this test thread, right before pass B of `target`
/// starts its durable write, after the entry and the manifest are written.
#[cfg(test)]
pub(super) fn register_before_target_write_hook_for_test(
    target: impl Into<PathBuf>,
    hook: impl FnOnce() + 'static,
) {
    BEFORE_TARGET_WRITE_HOOK
        .with(|slot| *slot.borrow_mut() = Some((target.into(), Box::new(hook))));
}

#[cfg(test)]
fn run_before_target_write_hook_for_test(path: &Path) {
    let hook = BEFORE_TARGET_WRITE_HOOK.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.as_ref().is_some_and(|(target, _)| target == path) {
            slot.take().map(|(_, hook)| hook)
        } else {
            None
        }
    });
    if let Some(hook) = hook {
        hook();
    }
}

#[cfg(not(test))]
#[inline(always)]
fn run_before_target_write_hook_for_test(_path: &Path) {}

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

/// Length and SHA-256 of one byte stream: how Undo recognises a file's
/// original and replaced bytes without holding them (design D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentDigest {
    /// Byte length.
    pub len: u64,
    /// SHA-256 of the bytes.
    pub sha256: [u8; 32],
}

impl ContentDigest {
    /// The digest of an in-memory byte slice.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self {
            len: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            sha256: stream::sha256_of(bytes),
        }
    }

    /// The digest as lowercase hex, for the journal entry.
    #[must_use]
    pub fn sha256_hex(&self) -> String {
        use std::fmt::Write as _;
        self.sha256
            .iter()
            .fold(String::with_capacity(64), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            })
    }

    /// Parse the lowercase hex [`Self::sha256_hex`] wrote.
    #[must_use]
    pub fn sha256_from_hex(hex: &str) -> Option<[u8; 32]> {
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let mut digest = [0u8; 32];
        for (index, slot) in digest.iter_mut().enumerate() {
            *slot = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?;
        }
        Some(digest)
    }
}

/// Where Undo reads one file's original bytes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoOriginal {
    /// The raw body file of a `replace-all-undo-stream-entry` (absolute path).
    Body(PathBuf),
    /// A legacy inline `replace-all-undo-entry`: Undo re-reads the entry (its
    /// `entry_file`) on its worker, under `MAIN_REPLACE_UNDO_BYTES`.
    LegacyEntry,
}

/// What Undo needs to recognise and restore one file. It never holds file
/// content: GTK retains descriptors only (design D3, D6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceUndoEntry {
    /// The entry JSON in the journal (absolute path).
    pub entry_file: PathBuf,
    /// Where the original bytes are.
    pub original_source: UndoOriginal,
    /// Length and digest of the bytes before Replace All.
    pub original: ContentDigest,
    /// Length and digest of the bytes Replace All wrote.
    pub replaced: ContentDigest,
    /// The launch that last wrote targets through this journal (design D9).
    pub launch: crate::services::filesystem::leftovers::LaunchId,
}

/// Undo descriptors keyed by absolute target path, in path order.
pub type ReplaceUndoBackup = BTreeMap<PathBuf, ReplaceUndoEntry>;

/// Return every heap byte retained by an undo descriptor owner.
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
    let body = match &entry.original_source {
        UndoOriginal::Body(body) => body.capacity(),
        UndoOriginal::LegacyEntry => 0,
    };
    u64::try_from(node_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(u64::try_from(path.capacity()).unwrap_or(u64::MAX))
        .saturating_add(u64::try_from(entry.entry_file.capacity()).unwrap_or(u64::MAX))
        .saturating_add(u64::try_from(body).unwrap_or(u64::MAX))
}

/// The retained weight `main` charged one admitted file: both payloads plus
/// the path and its table node, as `main`'s `replace_undo_entry_retained_byte_weight`
/// computed it for exact-capacity vectors (design D4, no-loss ledger).
#[must_use]
pub(crate) fn main_retained_entry_weight(path: &Path, source_len: u64, output_len: u64) -> u64 {
    // `main`'s entry held two `Vec<u8>`s in a `(PathBuf, ReplaceUndoEntry)` node.
    let node_bytes = std::mem::size_of::<(PathBuf, [Vec<u8>; 2])>()
        .saturating_add(std::mem::size_of::<usize>().saturating_mul(4));
    u64::try_from(node_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(u64::try_from(path.as_os_str().len()).unwrap_or(u64::MAX))
        .saturating_add(source_len)
        .saturating_add(output_len)
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
    /// Largest targeted-line buffer, in bytes, for any one file: the only
    /// source line the engine buffers (design D2).
    pub retained_line_bytes: usize,
    /// Largest aggregate undo body bytes retained at one time.
    pub undo_bytes: u64,
    /// Undo body bytes still retained by the journal at terminal publication.
    pub undo_live_bytes: u64,
    /// Descriptor weight retained by the undo owner at terminal publication.
    pub undo_descriptor_bytes: u64,
}

impl ReplaceConstructionMetrics {
    fn absorb_construction(&mut self, other: Self) {
        self.source_lines = self.source_lines.saturating_add(other.source_lines);
        self.accepted_replacements = self
            .accepted_replacements
            .saturating_add(other.accepted_replacements);
        self.retained_edit_records = self.retained_edit_records.max(other.retained_edit_records);
        self.retained_edit_bytes = self.retained_edit_bytes.max(other.retained_edit_bytes);
        self.retained_line_bytes = self.retained_line_bytes.max(other.retained_line_bytes);
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
    pub(crate) fn is_current(&self) -> bool {
        self.generation.load(Ordering::Acquire) == self.expected
    }
}

/// Which stage a Replace All run is in, for its progress display (design D8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacePhase {
    /// Writing undo bodies and targets.
    Replacing,
    /// Cancelled: restoring the files this run already wrote.
    RollingBack,
}

/// Progress of one Replace All run, written by its worker and polled by GTK.
///
/// Plain atomics, so a poll never waits on the worker.
#[derive(Debug, Default)]
pub struct ReplaceProgress {
    files_total: AtomicUsize,
    files_done: AtomicUsize,
    bytes_done: AtomicU64,
    phase: AtomicU8,
}

impl ReplaceProgress {
    /// Start a run over `files` files.
    pub fn begin<V>(&self, files: &BTreeMap<PathBuf, V>) {
        self.files_total.store(files.len(), Ordering::Relaxed);
        self.files_done.store(0, Ordering::Relaxed);
        self.bytes_done.store(0, Ordering::Relaxed);
        self.phase.store(0, Ordering::Relaxed);
    }

    /// One file finished, whatever its outcome.
    pub fn finish_file(&self, _path: &Path) {
        self.files_done.fetch_add(1, Ordering::Relaxed);
    }

    /// Bytes written to targets so far.
    pub fn add_bytes(&self, bytes: usize) {
        self.bytes_done
            .fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::Relaxed);
    }

    /// Enter `phase`.
    pub fn set_phase(&self, phase: ReplacePhase) {
        self.phase.store(
            match phase {
                ReplacePhase::Replacing => 0,
                ReplacePhase::RollingBack => 1,
            },
            Ordering::Relaxed,
        );
    }

    /// (files done, files total, target bytes written, phase).
    #[must_use]
    pub fn snapshot(&self) -> (usize, usize, u64, ReplacePhase) {
        let phase = if self.phase.load(Ordering::Relaxed) == 1 {
            ReplacePhase::RollingBack
        } else {
            ReplacePhase::Replacing
        };
        (
            self.files_done.load(Ordering::Relaxed),
            self.files_total.load(Ordering::Relaxed),
            self.bytes_done.load(Ordering::Relaxed),
            phase,
        )
    }
}

/// Result of applying Replace All plus the undo payload needed by the caller.
#[derive(Debug)]
pub struct ApplyReplacementsOutcome {
    /// User-facing replacement counts, skips, and recoverable errors.
    pub result: ReplaceResult,
    /// Undo descriptors retained for the active undo window (no file bytes).
    pub undo_backup: ReplaceUndoBackup,
    /// Direct construction and undo ownership evidence for this operation.
    pub metrics: ReplaceConstructionMetrics,
    /// Every file not (fully) replaced, with its reason (design D7).
    pub skip_report: ReplaceSkipReport,
    /// The run was cancelled and its rollback left `undo_backup` to undo.
    pub cancelled: bool,
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

mod admission;
mod apply;
mod protocol;
mod stream;
#[cfg(test)]
mod tests;
mod undo;

pub use apply::apply_replacements;
#[cfg(test)]
pub(crate) use apply::apply_replacements_if_current;
pub(crate) use apply::apply_request_if_current;
pub use apply::{ReplaceRunRequest, apply_replacements_with};
pub(crate) use protocol::{ShrinkAction, shrink_step};
pub use stream::REPLACE_STREAM_CHUNK_BYTES;
#[cfg(feature = "property-tests")]
pub use stream::{
    apply_replacements_to_text_for_property_test,
    apply_replacements_to_text_in_chunks_for_property_test,
    apply_replacements_to_text_reference_for_property_test,
    apply_replacements_to_text_reference_outcome_for_property_test,
};
pub use undo::{undo_replacements, undo_replacements_for_open_identities};
