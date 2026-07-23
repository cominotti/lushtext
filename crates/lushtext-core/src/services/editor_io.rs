// SPDX-License-Identifier: GPL-3.0-or-later

//! Blocking file I/O for editor load and save operations.
//!
//! All functions in this module run on background threads via
//! `spawn_blocking_then`. The service owns raw-byte decoding, line-ending
//! normalization, lossy-conversion checks, and atomic writes so the GTK layer
//! only has to react to typed results instead of reimplementing byte policy.

use crate::model::encoding::{
    DecodeConfidence, DocumentEncoding, DocumentEncodingState, FileHealthFinding,
    FileHealthFindingKind, FileHealthSeverity, LineEnding,
};
use crate::model::file_load::transient_load_weight;
use crate::model::formatting_overrides::FormattingOverrides;
use crate::services::file_limits::{FileSizeCheck, REFUSE_TO_OPEN};
use crate::services::filesystem::{
    FileFacts, WriteLabel, metadata as fs_metadata, read as fs_read, write as fs_write,
};
use encoding_rs::EncoderResult;
use std::borrow::Cow;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
#[cfg(feature = "test-utils")]
use std::sync::Mutex;
#[cfg(feature = "test-utils")]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "test-utils")]
use std::time::Duration;

/// Maximum number of concrete lossy characters included in one preview.
///
/// The preview exists to explain the risk, not to render a full diff for a
/// large document, so the sample stays intentionally small and fast to inspect.
const MAX_LOSSY_PREVIEW_ISSUES: usize = 8;

/// Bytes processed per bounded classification, decoding, or analysis slice.
///
/// Each slice ends at a cooperative cancellation checkpoint, so 256 KiB keeps
/// obsolete work on a cancelled multi-hundred-megabyte load bounded to a few
/// milliseconds without measurable throughput loss on the success path.
const LOAD_PROCESSING_CHUNK_BYTES: usize = 256 * 1024;

/// Inputs at or below this size may use the direct whole-buffer path.
///
/// One mebibyte matches the synchronous buffer-replacement threshold family:
/// direct classification, decoding, and analysis finish fast enough that the
/// pre/post stage cancellation checks alone bound obsolete work.
const DIRECT_LOAD_PROCESSING_THRESHOLD_BYTES: usize = 1024 * 1024;

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static LOAD_PROCESSING_CHUNK_EVENTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CANCEL_LOAD_AFTER_PROCESSING_CHUNKS: std::cell::Cell<Option<u64>> =
        const { std::cell::Cell::new(None) };
}

/// Take and reset this thread's bounded load-processing slice count.
#[cfg(any(test, feature = "test-utils"))]
#[must_use]
pub fn take_load_processing_chunks_for_test() -> u64 {
    LOAD_PROCESSING_CHUNK_EVENTS.with(|events| events.replace(0))
}

/// Cancel the current thread's load token at the Nth bounded processing slice.
///
/// This per-thread, per-invocation seam makes stage-specific cancellation
/// deterministic without global mutable state; pass `None` to disarm it.
#[cfg(any(test, feature = "test-utils"))]
pub fn cancel_load_after_processing_chunks_for_test(limit: Option<u64>) {
    CANCEL_LOAD_AFTER_PROCESSING_CHUNKS.with(|slot| slot.set(limit));
}

/// Count one bounded processing slice and drive the test cancellation seam.
///
/// Callers record a slice immediately before its cancellation check, so an
/// armed test seam stops the pipeline before the counted slice does any work.
fn record_load_processing_chunk(cancel: &AtomicBool) {
    #[cfg(any(test, feature = "test-utils"))]
    {
        let chunks = LOAD_PROCESSING_CHUNK_EVENTS.with(|events| {
            let next = events.get().saturating_add(1);
            events.set(next);
            next
        });
        CANCEL_LOAD_AFTER_PROCESSING_CHUNKS.with(|slot| {
            if slot.get().is_some_and(|limit| chunks >= limit) {
                cancel.store(true, Ordering::Release);
            }
        });
    }
    #[cfg(not(any(test, feature = "test-utils")))]
    let _ = cancel;
}

/// Admit one bounded processing slice under the shared checkpoint contract.
///
/// Records the slice before reading the cancellation flag — the armed test
/// seam counts every admitted slice and must stop the pipeline before the
/// counted slice does any work — then returns the exclusive slice end for the
/// slice starting at `position`. Callers with boundary rules (UTF-8 scalar
/// extension, char-boundary backoff) adjust the returned end locally.
fn next_load_processing_slice_end(
    position: usize,
    len: usize,
    cancel: &AtomicBool,
) -> Result<usize, EditorLoadError> {
    record_load_processing_chunk(cancel);
    if cancel.load(Ordering::Acquire) {
        return Err(EditorLoadError::Cancelled);
    }
    Ok(position
        .saturating_add(LOAD_PROCESSING_CHUNK_BYTES)
        .min(len))
}

#[cfg(feature = "test-utils")]
static LOAD_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static PAYLOAD_LOAD_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static SAVE_WRITE_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static NEXT_SAVE_FAILURE: Mutex<Option<(PathBuf, SaveFailureForTest)>> = Mutex::new(None);
#[cfg(feature = "test-utils")]
static TRANSIENT_WEIGHT_OVERRIDE_BYTES: AtomicU64 = AtomicU64::new(0);

/// Configure an artificial editor-load delay for widget race tests.
#[cfg(feature = "test-utils")]
pub fn set_load_delay_for_test(delay_ms: u64) {
    LOAD_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Configure an artificial admitted payload delay for coordinator tests.
#[cfg(feature = "test-utils")]
pub fn set_payload_load_delay_for_test(delay_ms: u64) {
    PAYLOAD_LOAD_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Configure an artificial admitted save delay for coordinator/widget tests.
#[cfg(feature = "test-utils")]
pub fn set_save_write_delay_for_test(delay_ms: u64) {
    SAVE_WRITE_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Deterministic durable-write terminal injected into the next editor save.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveFailureForTest {
    BeforeRename,
    AfterRename,
}

/// Configure one path-scoped pre- or post-rename failure for widget tests.
#[cfg(feature = "test-utils")]
pub fn fail_next_save_for_path_for_test(path: &Path, failure: SaveFailureForTest) {
    NEXT_SAVE_FAILURE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .replace((path.to_path_buf(), failure));
}

/// Clear any unused path-scoped editor-save failure injection.
#[cfg(feature = "test-utils")]
pub fn clear_save_failure_for_test() {
    NEXT_SAVE_FAILURE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
}

/// Override planned transient weight without creating huge widget fixtures.
#[cfg(feature = "test-utils")]
pub fn set_transient_weight_override_for_test(weight: Option<u64>) {
    TRANSIENT_WEIGHT_OVERRIDE_BYTES.store(weight.unwrap_or(0), Ordering::Release);
}

/// Successful result from `load_text_file`.
#[derive(Debug)]
pub struct LoadResult {
    /// Document facts that flow to editor state independently of the body.
    pub metadata: LoadMetadata,
    /// Decoded document text.
    pub content: String,
}

/// Compact per-load facts that cross ownership boundaries without the body.
///
/// Guarded GTK-side results hold this value beside separately owned content so
/// no layer ships a document-sized field that is empty by construction.
#[derive(Debug)]
pub struct LoadMetadata {
    pub size: u64,
    pub size_check: FileSizeCheck,
    /// Canonical filesystem identity resolved on the background load thread.
    pub canonical_path: Option<PathBuf>,
    /// File mtime (epoch seconds), extracted from the metadata already
    /// read for size classification — no extra stat() needed by callers.
    pub mtime: Option<u64>,
    /// Encoding and line-ending facts that now define this editor tab.
    pub encoding_state: DocumentEncodingState,
    /// Whether the loaded bytes carried a leading byte-order mark.
    pub has_bom: bool,
    /// File-health findings surfaced for the current document.
    pub file_health: Vec<FileHealthFinding>,
}

/// Compact metadata and stable-identity snapshot produced before payload admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileLoadPlan {
    /// Filesystem facts that admitted ingestion must revalidate.
    pub facts: FileFacts,
    /// Large-file feature policy selected from the planned byte size.
    pub size_check: FileSizeCheck,
    /// Conservative transient charge used by the process-wide coordinator.
    pub transient_weight: u64,
}

/// Decoded byte-ingestion snapshot returned only to fuzz targets.
///
/// Fuzzing needs to drive the same in-memory decode and health-classification
/// logic as file loading without touching the filesystem or widening the normal
/// application API. The `fuzzing` feature keeps this type out of ordinary builds.
#[cfg(feature = "fuzzing")]
#[derive(Debug, Clone)]
pub struct FuzzedEditorBytes {
    /// Decoded text produced by the load pipeline.
    pub content: String,
    /// Encoding and line-ending facts inferred from the raw bytes.
    pub encoding_state: DocumentEncodingState,
    /// Whether a matching byte-order mark was consumed during decoding.
    pub has_bom: bool,
    /// File-health findings that the editor would surface for these bytes.
    pub file_health: Vec<FileHealthFinding>,
}

/// One concrete unrepresentable character captured in a lossy save preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LossyEncodingIssue {
    /// One-based line number in the current buffer snapshot.
    pub line: usize,
    /// One-based column number in the current buffer snapshot.
    pub column: usize,
    /// The original Unicode scalar that the target encoding cannot represent.
    pub character: char,
}

/// Bounded preview of a lossy encoding conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LossyEncodingPreview {
    /// Target encoding chosen for the pending save.
    pub target_encoding: DocumentEncoding,
    /// Total number of unrepresentable characters in the snapshot.
    pub total_issue_count: usize,
    /// First few concrete examples to show in the confirmation dialog.
    pub issues: Vec<LossyEncodingIssue>,
}

impl LossyEncodingPreview {
    /// Build a short human-readable summary for dialogs and error messages.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.total_issue_count == 1 {
            return format!(
                "{} cannot represent 1 character in the current document.",
                self.target_encoding.label()
            );
        }
        format!(
            "{} cannot represent {} characters in the current document.",
            self.target_encoding.label(),
            self.total_issue_count
        )
    }

    /// Render the sampled issues as compact `line:column` bullet text.
    #[must_use]
    pub fn detail_lines(&self) -> Vec<String> {
        self.issues
            .iter()
            .map(|issue| {
                let mut rendered = String::new();
                let _ = write!(
                    rendered,
                    "Line {}, column {}: {} ({})",
                    issue.line,
                    issue.column,
                    display_character(issue.character),
                    format_codepoint(issue.character)
                );
                rendered
            })
            .collect()
    }
}

/// Errors that can occur when loading a file for editing.
/// Each variant carries context (path, size) for user-facing error messages.
#[derive(Debug, thiserror::Error)]
pub enum EditorLoadError {
    #[error("load cancelled")]
    Cancelled,
    #[error("Cannot stat {path}: {source}")]
    Metadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} changed while it was being opened. Retry to load the current file.")]
    Changed { path: PathBuf },
    #[error(
        "{path} grew while it was being opened (planned {planned_bytes} bytes, observed at least {observed_min_bytes} bytes). Retry to load the current file."
    )]
    Grew {
        path: PathBuf,
        planned_bytes: u64,
        observed_min_bytes: u64,
    },
    #[error(
        "{path} grew beyond the supported limit while it was being opened (at least {observed_min_mb} MB)."
    )]
    GrownTooLarge { path: PathBuf, observed_min_mb: u64 },
    #[error("Failed to decode {path} as {encoding}")]
    Decode {
        path: PathBuf,
        encoding: DocumentEncoding,
    },
    #[error("{path} is too large to edit ({size_mb} MB). Consider a pager like `less`.")]
    TooLarge { path: PathBuf, size_mb: u64 },
}

/// Editor-facing save failures that drive distinct recovery paths.
///
/// Variants separate blocked writes, in-flight saves, lossy conversion,
/// normalization policy, and unconfirmed durability so the UI can keep the
/// right recovery surface visible.
#[derive(Debug, thiserror::Error)]
pub enum EditorSaveError {
    #[error("No file path set")]
    NoPath,
    #[error("File load or installation is still in progress")]
    LoadInProgress,
    #[error("The last file-load installation was incomplete; retry loading before saving")]
    IncompleteLoadInstallation,
    #[error("Save already in progress")]
    SaveInProgress,
    #[error("Buffer changed while the save snapshot was being captured")]
    SnapshotCancelled,
    #[error("Saving as {encoding} would replace {issue_count} character(s)")]
    LossyEncoding {
        encoding: DocumentEncoding,
        issue_count: usize,
        preview: LossyEncodingPreview,
    },
    #[error("Mixed line endings must be normalized before save")]
    MixedLineEndings,
    #[error("Failed to write {path}: {source}")]
    WriteTemp {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The new bytes reached `path`, but the directory `fsync` that proves the
    /// rename durable failed. The save is on disk yet not confirmed crash-safe,
    /// so callers must report this differently from a lost write.
    #[error("Saved {path}, but durability could not be confirmed: {source}")]
    DurabilityUnconfirmed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Read a file from disk, decode it into Unicode text, and classify its metadata.
///
/// The default path auto-detects UTF-8, BOM-guided UTF-16, and a small set of
/// fallback encodings. Use `load_text_file_with_encoding` when the user chose a
/// specific "reopen with encoding" override.
///
/// **Threading:** Performs blocking I/O — call from a background thread.
///
/// # Errors
///
/// Returns an error if the file cannot be statted or read, exceeds the
/// supported size limit, or the load is cancelled.
pub fn load_text_file(path: &Path, cancel: &AtomicBool) -> Result<LoadResult, EditorLoadError> {
    load_text_file_with_encoding(path, cancel, None)
}

/// Read a file from disk using either auto-detection or an explicit reopen encoding.
///
/// **Threading:** Performs blocking I/O — call from a background thread.
///
/// # Errors
///
/// Returns an error if the file cannot be statted or read, exceeds the
/// supported size limit, or the load is cancelled.
pub fn load_text_file_with_encoding(
    path: &Path,
    cancel: &AtomicBool,
    reopen_as: Option<DocumentEncoding>,
) -> Result<LoadResult, EditorLoadError> {
    let plan = plan_text_file(path, cancel)?;
    load_planned_text_file(plan, cancel, reopen_as)
}

/// Resolve metadata and stable identity without retaining document bytes.
///
/// **Threading:** Performs blocking metadata I/O — call from a background thread.
///
/// # Errors
///
/// Returns a typed cancellation, metadata, or initial-size failure.
pub fn plan_text_file(path: &Path, cancel: &AtomicBool) -> Result<FileLoadPlan, EditorLoadError> {
    if cancel.load(Ordering::Acquire) {
        return Err(EditorLoadError::Cancelled);
    }

    delay_load_for_test();

    let facts = fs_metadata::file_facts(path).map_err(|source| EditorLoadError::Metadata {
        path: path.to_path_buf(),
        source,
    })?;
    let size_check = FileSizeCheck::classify(facts.byte_size);

    if size_check == FileSizeCheck::TooLarge {
        return Err(EditorLoadError::TooLarge {
            path: path.to_path_buf(),
            size_mb: facts.byte_size / 1_000_000,
        });
    }

    if cancel.load(Ordering::Acquire) {
        return Err(EditorLoadError::Cancelled);
    }

    Ok(FileLoadPlan {
        transient_weight: planned_transient_weight(facts.byte_size),
        facts,
        size_check,
    })
}

/// Execute admitted bounded ingestion and decoding for one current plan.
///
/// The plan is revalidated before and after streaming so growth, rename, and
/// replacement races cannot install bytes under stale metadata ownership.
///
/// **Threading:** Performs blocking I/O and decoding — call from a background thread.
///
/// # Errors
///
/// Returns typed cancellation, growth, identity-change, metadata, read, or
/// initial planning outcomes.
pub fn load_planned_text_file(
    plan: FileLoadPlan,
    cancel: &AtomicBool,
    reopen_as: Option<DocumentEncoding>,
) -> Result<LoadResult, EditorLoadError> {
    load_planned_text_file_with_limit(plan, cancel, reopen_as, REFUSE_TO_OPEN)
}

fn load_planned_text_file_with_limit(
    plan: FileLoadPlan,
    cancel: &AtomicBool,
    reopen_as: Option<DocumentEncoding>,
    supported_limit: u64,
) -> Result<LoadResult, EditorLoadError> {
    let path = plan.facts.path.clone();
    if cancel.load(Ordering::Acquire) {
        return Err(EditorLoadError::Cancelled);
    }

    revalidate_load_plan(&plan, supported_limit)?;
    delay_payload_load_for_test();
    let read_limit = admitted_read_limit(&plan, supported_limit);
    let bytes = fs_read::bounded_bytes(&path, read_limit, plan.facts.byte_size, || {
        cancel.load(Ordering::Acquire)
    })
    .map_err(|error| match error {
        fs_read::BoundedFileReadError::Cancelled => EditorLoadError::Cancelled,
        fs_read::BoundedFileReadError::LimitExceeded { byte_limit } => EditorLoadError::Grew {
            path: path.clone(),
            planned_bytes: plan.facts.byte_size,
            observed_min_bytes: byte_limit.saturating_add(1),
        },
        fs_read::BoundedFileReadError::Io(source) => EditorLoadError::Read {
            path: path.clone(),
            source,
        },
    })?;
    if cancel.load(Ordering::Acquire) {
        return Err(EditorLoadError::Cancelled);
    }

    revalidate_load_plan(&plan, supported_limit)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) != plan.facts.byte_size {
        return Err(EditorLoadError::Changed { path });
    }

    let payload = decode_payload_cancellable(&bytes, reopen_as, cancel)?;
    if payload.had_errors {
        return Err(EditorLoadError::Decode {
            path,
            encoding: payload.opened_encoding,
        });
    }
    if cancel.load(Ordering::Acquire) {
        return Err(EditorLoadError::Cancelled);
    }
    // Line-ending and character health evidence share one bounded pass over
    // the decoded text; NUL evidence keeps its raw-byte scan but skips it for
    // UTF-16, whose zero bytes are expected and excluded from the finding.
    let analysis = analyze_decoded_content_cancellable(&payload.content, cancel)?;
    let nul_evidence = if matches!(
        payload.opened_encoding,
        DocumentEncoding::Utf16Le | DocumentEncoding::Utf16Be
    ) {
        false
    } else {
        contains_nul_cancellable(&bytes, cancel)?
    };
    if cancel.load(Ordering::Acquire) {
        return Err(EditorLoadError::Cancelled);
    }

    let decoded = assemble_decoded_document(payload, &analysis);
    let file_health = build_file_health(&decoded, &analysis, nul_evidence);

    Ok(LoadResult {
        metadata: LoadMetadata {
            size: plan.facts.byte_size,
            size_check: plan.size_check,
            canonical_path: plan.facts.canonical_path,
            mtime: plan.facts.modified_at_secs,
            encoding_state: decoded.encoding_state,
            has_bom: decoded.has_bom,
            file_health,
        },
        content: decoded.content,
    })
}

fn admitted_read_limit(plan: &FileLoadPlan, supported_limit: u64) -> u64 {
    plan.facts.byte_size.min(supported_limit)
}

fn planned_transient_weight(source_bytes: u64) -> u64 {
    #[cfg(feature = "test-utils")]
    {
        let override_bytes = TRANSIENT_WEIGHT_OVERRIDE_BYTES.load(Ordering::Acquire);
        if override_bytes > 0 {
            return override_bytes;
        }
    }
    transient_load_weight(source_bytes)
}

#[cfg(feature = "test-utils")]
fn delay_payload_load_for_test() {
    let delay_ms = PAYLOAD_LOAD_DELAY_MS.load(Ordering::Acquire);
    if delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
}

#[cfg(not(feature = "test-utils"))]
fn delay_payload_load_for_test() {}

fn revalidate_load_plan(plan: &FileLoadPlan, supported_limit: u64) -> Result<(), EditorLoadError> {
    let path = &plan.facts.path;
    let current = fs_metadata::file_facts(path).map_err(|source| EditorLoadError::Metadata {
        path: path.clone(),
        source,
    })?;
    if current.byte_size > supported_limit {
        return Err(EditorLoadError::GrownTooLarge {
            path: path.clone(),
            observed_min_mb: current.byte_size / 1_000_000,
        });
    }
    if current.byte_size > plan.facts.byte_size {
        return Err(EditorLoadError::Grew {
            path: path.clone(),
            planned_bytes: plan.facts.byte_size,
            observed_min_bytes: current.byte_size,
        });
    }
    if current.canonical_path != plan.facts.canonical_path
        || current.kind != plan.facts.kind
        || current.identity != plan.facts.identity
        || current.byte_size != plan.facts.byte_size
        || current.modified_at_nanos != plan.facts.modified_at_nanos
    {
        return Err(EditorLoadError::Changed { path: path.clone() });
    }
    Ok(())
}

#[cfg(feature = "test-utils")]
fn delay_load_for_test() {
    let delay_ms = LOAD_DELAY_MS.load(Ordering::Acquire);
    if delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
}

#[cfg(not(feature = "test-utils"))]
fn delay_load_for_test() {}

/// Classify raw editor bytes without filesystem access for fuzz targets.
///
/// This deliberately reuses the cancellable decode/analysis stages and
/// `build_file_health()` so
/// fuzzing exercises the production byte-ingestion path while staying free of
/// disk I/O, GTK widgets, and cancellation timing.
#[cfg(feature = "fuzzing")]
#[must_use]
pub fn classify_bytes_for_fuzzing(
    bytes: &[u8],
    reopen_as: Option<DocumentEncoding>,
) -> FuzzedEditorBytes {
    let never = AtomicBool::new(false);
    let payload = decode_payload_cancellable(bytes, reopen_as, &never)
        .unwrap_or_else(|_| unreachable!("an uncancelled decode has no failure terminal"));
    let analysis = analyze_decoded_content_cancellable(&payload.content, &never)
        .unwrap_or_else(|_| unreachable!("an uncancelled analysis has no failure terminal"));
    let nul_evidence = contains_nul_cancellable(bytes, &never)
        .unwrap_or_else(|_| unreachable!("an uncancelled scan has no failure terminal"));
    let decoded = assemble_decoded_document(payload, &analysis);
    let file_health = build_file_health(&decoded, &analysis, nul_evidence);

    FuzzedEditorBytes {
        content: decoded.content,
        encoding_state: decoded.encoding_state,
        has_bom: decoded.has_bom,
        file_health,
    }
}

/// Atomically write a UTF-8/LF snapshot to a file using temp-file-then-rename.
///
/// This compatibility wrapper keeps older call sites simple while the editor
/// workflow migrates to the richer save-policy API.
///
/// # Errors
///
/// Returns the same write or normalization errors as `write_document_to_path`.
pub fn write_snapshot_to_path(
    path: &Path,
    text: &str,
) -> Result<EditorWriteResult, EditorSaveError> {
    write_document_to_path(path, text, DocumentEncoding::Utf8, LineEnding::Lf, false)
}

/// Outcome of a successful editor document write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorWriteResult {
    /// Number of bytes written after line-ending normalization and transcoding.
    pub bytes_written: u64,
    /// File modification timestamp observed after the durable write, when available.
    pub modified_at_secs: Option<u64>,
}

/// Atomically write a document using the chosen save encoding and line endings.
///
/// The text snapshot is normalized and transcoded before the temp-file write.
/// If that transcoding would lose characters and `allow_lossy` is `false`, the
/// function returns a bounded preview instead of writing anything.
///
/// **Threading:** Performs blocking I/O — call from a background thread.
///
/// # Errors
///
/// Returns an error if the conversion is lossy without confirmation, if the
/// requested line-ending policy is still mixed, or if the temp-file write fails.
pub fn write_document_to_path(
    path: &Path,
    text: &str,
    encoding: DocumentEncoding,
    line_ending: LineEnding,
    allow_lossy: bool,
) -> Result<EditorWriteResult, EditorSaveError> {
    delay_save_write_for_test();
    let normalized = normalize_line_endings(text, line_ending)?;
    let bytes = encode_text(normalized.as_ref(), encoding, allow_lossy)?;
    let bytes_written = bytes.len() as u64;
    write_bytes_to_path(path, &bytes)?;
    let mtime = fs_metadata::file_facts(path)
        .ok()
        .and_then(|facts| facts.modified_at_secs);
    Ok(EditorWriteResult {
        bytes_written,
        modified_at_secs: mtime,
    })
}

#[cfg(feature = "test-utils")]
fn delay_save_write_for_test() {
    let delay_ms = SAVE_WRITE_DELAY_MS.load(Ordering::Acquire);
    if delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
}

#[cfg(not(feature = "test-utils"))]
fn delay_save_write_for_test() {}

/// Apply EditorConfig save-only text rewrites before encoding and line-ending normalization.
///
/// This is pure string processing and performs no filesystem work, so it can be
/// unit-tested separately from the atomic write path. The returned text is still
/// normalized to the active save line ending later in `write_document_to_path`.
#[must_use]
pub fn apply_save_formatting_overrides(text: &str, overrides: FormattingOverrides) -> String {
    apply_save_formatting_overrides_borrowed(text, overrides).into_owned()
}

/// Borrow unchanged text and allocate only when save-only formatting rewrites it.
#[must_use]
pub(crate) fn apply_save_formatting_overrides_borrowed(
    text: &str,
    overrides: FormattingOverrides,
) -> Cow<'_, str> {
    let mut formatted = if overrides.trim_trailing_whitespace == Some(true) {
        Cow::Owned(trim_trailing_space_and_tabs(text))
    } else {
        Cow::Borrowed(text)
    };

    match overrides.insert_final_newline {
        Some(true) if !formatted.is_empty() && !formatted.ends_with(['\n', '\r']) => {
            formatted.to_mut().push('\n');
        }
        Some(false) => {
            let keep = formatted.trim_end_matches(['\n', '\r']).len();
            if keep != formatted.len() {
                formatted.to_mut().truncate(keep);
            }
        }
        _ => {}
    }

    formatted
}

/// Analyze whether saving the current text in the given encoding would be lossy.
#[must_use]
pub fn analyze_lossy_encoding(
    text: &str,
    target_encoding: DocumentEncoding,
) -> Option<LossyEncodingPreview> {
    if matches!(
        target_encoding,
        DocumentEncoding::Utf8
            | DocumentEncoding::Utf8Bom
            | DocumentEncoding::Utf16Le
            | DocumentEncoding::Utf16Be
    ) {
        return None;
    }

    let mut encoder = target_encoding.codec().new_encoder();
    let mut scratch = [0u8; 4096];
    let mut issues = Vec::new();
    let mut total_issue_count = 0usize;
    let mut line = 1usize;
    let mut column = 1usize;
    let mut consumed = 0usize;

    while consumed < text.len() {
        let remaining = &text[consumed..];
        let (result, read, _) =
            encoder.encode_from_utf8_without_replacement(remaining, &mut scratch, true);
        match result {
            EncoderResult::InputEmpty => {
                advance_source_position(&remaining[..read], &mut line, &mut column);
                break;
            }
            EncoderResult::OutputFull => {
                debug_assert!(read > 0, "encoding analysis scratch must fit one scalar");
                advance_source_position(&remaining[..read], &mut line, &mut column);
                consumed = consumed.saturating_add(read);
            }
            EncoderResult::Unmappable(character) => {
                let character_bytes = character.len_utf8();
                debug_assert!(read >= character_bytes);
                let prefix_bytes = read.saturating_sub(character_bytes);
                advance_source_position(&remaining[..prefix_bytes], &mut line, &mut column);
                total_issue_count = total_issue_count.saturating_add(1);
                if issues.len() < MAX_LOSSY_PREVIEW_ISSUES {
                    issues.push(LossyEncodingIssue {
                        line,
                        column,
                        character,
                    });
                }
                advance_source_position(&remaining[prefix_bytes..read], &mut line, &mut column);
                consumed = consumed.saturating_add(read);
            }
        }
    }

    if total_issue_count == 0 {
        None
    } else {
        Some(LossyEncodingPreview {
            target_encoding,
            total_issue_count,
            issues,
        })
    }
}

fn advance_source_position(fragment: &str, line: &mut usize, column: &mut usize) {
    for character in fragment.chars() {
        if character == '\n' {
            *line = line.saturating_add(1);
            *column = 1;
        } else {
            *column = column.saturating_add(1);
        }
    }
}

/// Read a file's mtime as seconds since the UNIX epoch.
/// Returns `None` if the file doesn't exist or metadata can't be read.
///
/// **Threading:** Performs a blocking stat syscall.
#[must_use]
pub fn mtime_secs(path: &Path) -> Option<u64> {
    fs_metadata::file_facts(path)
        .ok()
        .and_then(|facts| facts.modified_at_secs)
}

/// Current wall-clock time as seconds since the UNIX epoch.
#[must_use]
pub fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

/// One decoded document snapshot before it becomes an editor tab.
struct DecodedDocument {
    content: String,
    encoding_state: DocumentEncodingState,
    has_bom: bool,
}

/// Decoded text plus classification metadata before content analysis runs.
struct DecodedPayload {
    content: String,
    opened_encoding: DocumentEncoding,
    decode_confidence: DecodeConfidence,
    has_bom: bool,
    had_errors: bool,
}

/// Line-ending and character evidence from one bounded pass over decoded text.
struct DecodedContentAnalysis {
    detected_line_ending: LineEnding,
    suggested_line_ending: LineEnding,
    nbsp_count: usize,
    zero_width_count: usize,
}

fn assemble_decoded_document(
    payload: DecodedPayload,
    analysis: &DecodedContentAnalysis,
) -> DecodedDocument {
    let encoding_state = DocumentEncodingState {
        opened_encoding: payload.opened_encoding,
        save_encoding: payload.opened_encoding,
        detected_line_ending: analysis.detected_line_ending,
        save_line_ending: analysis.suggested_line_ending,
        decode_confidence: payload.decode_confidence,
    };
    DecodedDocument {
        content: payload.content,
        encoding_state,
        has_bom: payload.has_bom,
    }
}

/// Classify and decode raw bytes with cooperative cancellation checkpoints.
///
/// Classification order, BOM handling, the BOM-less UTF-16 heuristic, fallback
/// choice, and decoded output are exactly equivalent to the previous
/// whole-buffer implementation; only the traversal is sliced.
fn decode_payload_cancellable(
    bytes: &[u8],
    reopen_as: Option<DocumentEncoding>,
    cancel: &AtomicBool,
) -> Result<DecodedPayload, EditorLoadError> {
    let payload = |content: String,
                   opened_encoding: DocumentEncoding,
                   decode_confidence: DecodeConfidence,
                   has_bom: bool,
                   had_errors: bool| DecodedPayload {
        content,
        opened_encoding,
        decode_confidence,
        has_bom,
        had_errors,
    };

    if let Some(encoding) = reopen_as {
        let (content, has_bom, had_errors) = decode_with_encoding(bytes, encoding, cancel)?;
        return Ok(payload(
            content,
            encoding,
            DecodeConfidence::Exact,
            has_bom,
            had_errors,
        ));
    }
    if let Some((encoding, stripped)) = bom_prefixed_encoding(bytes) {
        let (content, had_errors) = decode_bytes_without_bom(stripped, encoding, cancel)?;
        return Ok(payload(
            content,
            encoding,
            DecodeConfidence::Exact,
            true,
            had_errors,
        ));
    }
    if let Some(content) = try_decode_valid_utf8(bytes, cancel)? {
        return Ok(payload(
            content,
            DocumentEncoding::Utf8,
            DecodeConfidence::Exact,
            false,
            false,
        ));
    }
    if let Some(encoding) = guess_utf16_without_bom_cancellable(bytes, cancel)? {
        let (content, had_errors) = decode_bytes_without_bom(bytes, encoding, cancel)?;
        return Ok(payload(
            content,
            encoding,
            DecodeConfidence::Heuristic,
            false,
            had_errors,
        ));
    }
    let (content, had_errors) =
        decode_bytes_without_bom(bytes, DocumentEncoding::Windows1252, cancel)?;
    Ok(payload(
        content,
        DocumentEncoding::Windows1252,
        DecodeConfidence::Low,
        false,
        had_errors,
    ))
}

/// Decode bytes using an explicit encoding selection, stripping any matching BOM.
fn decode_with_encoding(
    bytes: &[u8],
    encoding: DocumentEncoding,
    cancel: &AtomicBool,
) -> Result<(String, bool, bool), EditorLoadError> {
    if let Some((detected_encoding, stripped)) = bom_prefixed_encoding(bytes)
        && detected_encoding == encoding
    {
        let (content, had_errors) = decode_bytes_without_bom(stripped, encoding, cancel)?;
        return Ok((content, true, had_errors));
    }

    let (content, had_errors) = decode_bytes_without_bom(bytes, encoding, cancel)?;
    Ok((content, false, had_errors))
}

/// Decode bytes with the requested encoding after BOM handling has been resolved.
fn decode_bytes_without_bom(
    bytes: &[u8],
    encoding: DocumentEncoding,
    cancel: &AtomicBool,
) -> Result<(String, bool), EditorLoadError> {
    if matches!(encoding, DocumentEncoding::Utf8 | DocumentEncoding::Utf8Bom)
        && let Some(content) = try_decode_valid_utf8(bytes, cancel)?
    {
        return Ok((content, false));
    }
    decode_with_codec_cancellable(bytes, encoding, cancel)
}

/// Validate and copy UTF-8 in bounded slices split at scalar boundaries.
///
/// Returns `Ok(None)` when the bytes are not valid UTF-8. Fusing validation
/// with the copy keeps the success path single-pass while giving very large
/// documents a cancellation checkpoint every slice.
fn try_decode_valid_utf8(
    bytes: &[u8],
    cancel: &AtomicBool,
) -> Result<Option<String>, EditorLoadError> {
    if bytes.len() <= DIRECT_LOAD_PROCESSING_THRESHOLD_BYTES {
        return Ok(simdutf8::basic::from_utf8(bytes).ok().map(str::to_string));
    }

    let mut content = String::with_capacity(bytes.len());
    let mut position = 0usize;
    while position < bytes.len() {
        let mut end = next_load_processing_slice_end(position, bytes.len(), cancel)?;
        // Extend past UTF-8 continuation bytes so a valid scalar never splits
        // across slices. Valid UTF-8 never has more than three consecutive
        // continuation bytes, so a longer run (binary padding such as repeated
        // 0x80) is rejected here instead of extending one slice arbitrarily
        // far past its cancellation checkpoint.
        let mut extended = 0usize;
        while end < bytes.len() && (bytes[end] & 0xC0) == 0x80 {
            end += 1;
            extended += 1;
            if extended > 3 {
                return Ok(None);
            }
        }
        match simdutf8::basic::from_utf8(&bytes[position..end]) {
            Ok(valid) => content.push_str(valid),
            Err(_) => return Ok(None),
        }
        position = end;
    }
    Ok(Some(content))
}

/// Stream bytes through a stateful `encoding_rs` decoder in bounded slices.
///
/// `encoding_rs` guarantees streaming output identical to the whole-buffer
/// decode, including replacement characters and the `had_errors` verdict.
fn decode_with_codec_cancellable(
    bytes: &[u8],
    encoding: DocumentEncoding,
    cancel: &AtomicBool,
) -> Result<(String, bool), EditorLoadError> {
    if bytes.len() <= DIRECT_LOAD_PROCESSING_THRESHOLD_BYTES {
        let (decoded, had_errors) = encoding.codec().decode_without_bom_handling(bytes);
        return Ok((decoded.into_owned(), had_errors));
    }

    let mut decoder = encoding.codec().new_decoder_without_bom_handling();
    let mut content = String::with_capacity(bytes.len());
    let mut had_errors = false;
    let mut position = 0usize;
    loop {
        let end = next_load_processing_slice_end(position, bytes.len(), cancel)?;
        let last = end == bytes.len();
        let mut source = &bytes[position..end];
        loop {
            let needed = decoder
                .max_utf8_buffer_length(source.len())
                .unwrap_or_else(|| source.len().saturating_mul(3).saturating_add(16));
            content.reserve(needed);
            let (result, read, errors) = decoder.decode_to_string(source, &mut content, last);
            had_errors |= errors;
            source = &source[read..];
            match result {
                encoding_rs::CoderResult::InputEmpty => break,
                encoding_rs::CoderResult::OutputFull => {}
            }
        }
        position = end;
        if last {
            return Ok((content, had_errors));
        }
    }
}

/// Report whether the raw bytes contain a NUL, with bounded slice checkpoints.
fn contains_nul_cancellable(bytes: &[u8], cancel: &AtomicBool) -> Result<bool, EditorLoadError> {
    if bytes.len() <= DIRECT_LOAD_PROCESSING_THRESHOLD_BYTES {
        return Ok(bytes.contains(&0));
    }

    let mut position = 0usize;
    while position < bytes.len() {
        let end = next_load_processing_slice_end(position, bytes.len(), cancel)?;
        if memchr::memchr(0, &bytes[position..end]).is_some() {
            return Ok(true);
        }
        position = end;
    }
    Ok(false)
}

/// Detect BOM-prefixed encodings that the load pipeline can trust exactly.
fn bom_prefixed_encoding(bytes: &[u8]) -> Option<(DocumentEncoding, &[u8])> {
    if let Some(stripped) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return Some((DocumentEncoding::Utf8Bom, stripped));
    }
    if let Some(stripped) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return Some((DocumentEncoding::Utf16Le, stripped));
    }
    if let Some(stripped) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return Some((DocumentEncoding::Utf16Be, stripped));
    }
    None
}

/// Detect likely UTF-16 text when no BOM is present.
fn guess_utf16_without_bom(bytes: &[u8]) -> Option<DocumentEncoding> {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
        return None;
    }

    let even_zeroes = bytes.iter().step_by(2).filter(|&&byte| byte == 0).count();
    let odd_zeroes = bytes
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|&&byte| byte == 0)
        .count();

    classify_utf16_zero_parity(even_zeroes, odd_zeroes, bytes.len() / 2)
}

/// Run the BOM-less UTF-16 heuristic with bounded slice checkpoints.
fn guess_utf16_without_bom_cancellable(
    bytes: &[u8],
    cancel: &AtomicBool,
) -> Result<Option<DocumentEncoding>, EditorLoadError> {
    if bytes.len() <= DIRECT_LOAD_PROCESSING_THRESHOLD_BYTES {
        return Ok(guess_utf16_without_bom(bytes));
    }
    if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
        return Ok(None);
    }

    let mut even_zeroes = 0usize;
    let mut odd_zeroes = 0usize;
    let mut position = 0usize;
    while position < bytes.len() {
        let end = next_load_processing_slice_end(position, bytes.len(), cancel)?;
        for (offset, &byte) in bytes[position..end].iter().enumerate() {
            if byte == 0 {
                if (position + offset).is_multiple_of(2) {
                    even_zeroes += 1;
                } else {
                    odd_zeroes += 1;
                }
            }
        }
        position = end;
    }
    Ok(classify_utf16_zero_parity(
        even_zeroes,
        odd_zeroes,
        bytes.len() / 2,
    ))
}

/// Shared BOM-less UTF-16 verdict from zero-byte parity counts.
fn classify_utf16_zero_parity(
    even_zeroes: usize,
    odd_zeroes: usize,
    pair_count: usize,
) -> Option<DocumentEncoding> {
    // UTF-16 English and code-like text produces many zero high bytes on one
    // side of each 16-bit unit. Keep the threshold coarse on purpose so the
    // fallback still prefers Windows-1252 when the signal is weak.
    if odd_zeroes * 2 >= pair_count && even_zeroes * 10 <= pair_count {
        return Some(DocumentEncoding::Utf16Le);
    }
    if even_zeroes * 2 >= pair_count && odd_zeroes * 10 <= pair_count {
        return Some(DocumentEncoding::Utf16Be);
    }
    None
}

/// Streaming CR/LF/CRLF tally whose CR carry survives slice boundaries.
///
/// Feeding the whole text as one slice reproduces the historical single-pass
/// tally exactly; bounded slices only add a carry for a CR that ends a slice
/// and might pair with an LF starting the next one.
#[derive(Default)]
struct LineEndingTally {
    crlf_count: usize,
    lf_count: usize,
    cr_count: usize,
    pending_cr: bool,
}

impl LineEndingTally {
    fn scan_slice(&mut self, bytes: &[u8]) {
        let mut skip_first_lf = false;
        if self.pending_cr {
            self.pending_cr = false;
            if bytes.first() == Some(&b'\n') {
                self.crlf_count += 1;
                skip_first_lf = true;
            } else {
                self.cr_count += 1;
            }
        }

        let mut paired_lf = None;
        for index in memchr::memchr2_iter(b'\r', b'\n', bytes) {
            if index == 0 && skip_first_lf {
                continue;
            }
            if paired_lf == Some(index) {
                paired_lf = None;
                continue;
            }
            match bytes[index] {
                b'\r' if index + 1 == bytes.len() => {
                    self.pending_cr = true;
                }
                b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                    self.crlf_count += 1;
                    paired_lf = Some(index + 1);
                }
                b'\r' => {
                    self.cr_count += 1;
                }
                b'\n' => {
                    self.lf_count += 1;
                }
                _ => unreachable!("memchr2_iter yields only CR or LF candidates"),
            }
        }
    }

    fn finish(mut self) -> (usize, usize, usize) {
        if self.pending_cr {
            self.cr_count += 1;
        }
        (self.crlf_count, self.lf_count, self.cr_count)
    }
}

/// Detect line-ending style from decoded text and choose a safe save default.
///
/// The input must already be decoded text. CR/LF candidate discovery uses the
/// established byte-search dependency, while every candidate remains an ASCII
/// byte and therefore cannot split a UTF-8 scalar.
#[must_use]
pub fn detect_line_endings(text: &str) -> (LineEnding, LineEnding) {
    let mut tally = LineEndingTally::default();
    tally.scan_slice(text.as_bytes());
    let (crlf_count, lf_count, cr_count) = tally.finish();
    classify_line_ending_counts(crlf_count, lf_count, cr_count)
}

/// Shared detected/suggested line-ending policy from complete tally counts.
fn classify_line_ending_counts(
    crlf_count: usize,
    lf_count: usize,
    cr_count: usize,
) -> (LineEnding, LineEnding) {
    let distinct_styles =
        usize::from(crlf_count > 0) + usize::from(lf_count > 0) + usize::from(cr_count > 0);

    let detected = match distinct_styles {
        1 if crlf_count > 0 => LineEnding::Crlf,
        1 if cr_count > 0 => LineEnding::Cr,
        0 | 1 => LineEnding::Lf,
        _ => LineEnding::Mixed,
    };

    let suggested = if crlf_count >= lf_count && crlf_count >= cr_count {
        LineEnding::Crlf
    } else if cr_count >= lf_count {
        LineEnding::Cr
    } else {
        LineEnding::Lf
    };

    (detected, suggested)
}

/// Accumulate line-ending and character health evidence in one bounded pass.
///
/// Fusing the line-ending tally with the non-breaking-space and zero-width
/// counts removes two redundant whole-document scans while keeping every
/// successful count and classification identical to the previous passes.
fn analyze_decoded_content_cancellable(
    content: &str,
    cancel: &AtomicBool,
) -> Result<DecodedContentAnalysis, EditorLoadError> {
    let mut tally = LineEndingTally::default();
    let mut nbsp_count = 0usize;
    let mut zero_width_count = 0usize;

    let mut scan_slice = |slice: &str| {
        tally.scan_slice(slice.as_bytes());
        for character in slice.chars() {
            if character == '\u{00A0}' {
                nbsp_count += 1;
            } else if is_zero_width(character) {
                zero_width_count += 1;
            }
        }
    };

    if content.len() <= DIRECT_LOAD_PROCESSING_THRESHOLD_BYTES {
        scan_slice(content);
    } else {
        let mut position = 0usize;
        while position < content.len() {
            let mut end = next_load_processing_slice_end(position, content.len(), cancel)?;
            while !content.is_char_boundary(end) {
                end -= 1;
            }
            scan_slice(&content[position..end]);
            position = end;
        }
    }

    let (crlf_count, lf_count, cr_count) = tally.finish();
    let (detected_line_ending, suggested_line_ending) =
        classify_line_ending_counts(crlf_count, lf_count, cr_count);
    Ok(DecodedContentAnalysis {
        detected_line_ending,
        suggested_line_ending,
        nbsp_count,
        zero_width_count,
    })
}

/// Build surfaced file-health findings from the decoded document snapshot.
///
/// Character counts arrive from the fused bounded analysis pass and the NUL
/// verdict from the raw-byte scan, so this assembly stage never rescans the
/// document.
fn build_file_health(
    decoded: &DecodedDocument,
    analysis: &DecodedContentAnalysis,
    nul_evidence: bool,
) -> Vec<FileHealthFinding> {
    let mut findings = Vec::new();

    if decoded.has_bom && decoded.encoding_state.opened_encoding == DocumentEncoding::Utf8Bom {
        findings.push(FileHealthFinding {
            kind: FileHealthFindingKind::Utf8Bom,
            severity: FileHealthSeverity::Info,
            title: "UTF-8 BOM detected".to_string(),
            body: "This document starts with a UTF-8 byte-order mark. Many Unix tools ignore it, but some workflows treat it as unexpected text.".to_string(),
        });
    }

    if decoded.encoding_state.detected_line_ending == LineEnding::Mixed {
        findings.push(FileHealthFinding {
            kind: FileHealthFindingKind::MixedLineEndings,
            severity: FileHealthSeverity::Warning,
            title: "Mixed line endings".to_string(),
            body: "The loaded document contains more than one line-ending style. Normalizing the save policy will keep future writes consistent.".to_string(),
        });
    }

    if decoded.encoding_state.decode_confidence.needs_warning() {
        findings.push(FileHealthFinding {
            kind: FileHealthFindingKind::LowConfidenceDecode,
            severity: FileHealthSeverity::Warning,
            title: "Low-confidence encoding guess".to_string(),
            body: format!(
                "LushText opened this file as {} with {} confidence. Reopen with another encoding if the text looks incorrect.",
                decoded.encoding_state.opened_encoding.label(),
                decoded.encoding_state.decode_confidence.label().to_lowercase()
            ),
        });
    }

    if nul_evidence
        && !matches!(
            decoded.encoding_state.opened_encoding,
            DocumentEncoding::Utf16Le | DocumentEncoding::Utf16Be
        )
    {
        findings.push(FileHealthFinding {
            kind: FileHealthFindingKind::BinaryLikeContent,
            severity: FileHealthSeverity::Warning,
            title: "Binary-like bytes detected".to_string(),
            body: "The source bytes include NUL values that are unusual for plain text. Review the content carefully before editing or resaving it.".to_string(),
        });
    }

    let nbsp_count = analysis.nbsp_count;
    if nbsp_count > 0 {
        findings.push(FileHealthFinding {
            kind: FileHealthFindingKind::NonBreakingSpace,
            severity: FileHealthSeverity::Info,
            title: "Non-breaking spaces present".to_string(),
            body: format!(
                "This document contains {nbsp_count} non-breaking space(s), which can look like normal spaces but behave differently in code and text tools."
            ),
        });
    }

    let zero_width_count = analysis.zero_width_count;
    if zero_width_count > 0 {
        findings.push(FileHealthFinding {
            kind: FileHealthFindingKind::ZeroWidthCharacter,
            severity: FileHealthSeverity::Info,
            title: "Zero-width characters present".to_string(),
            body: format!(
                "This document contains {zero_width_count} zero-width character(s), which can be hard to notice during review."
            ),
        });
    }

    findings
}

/// Normalize text to the selected save-time line ending.
fn normalize_line_endings(
    text: &str,
    line_ending: LineEnding,
) -> Result<Cow<'_, str>, EditorSaveError> {
    let Some(separator) = line_ending.separator() else {
        return Err(EditorSaveError::MixedLineEndings);
    };
    if line_endings_already_normalized(text, line_ending) {
        return Ok(Cow::Borrowed(text));
    }

    let mut normalized = String::with_capacity(text.len() + text.len() / 16);
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\r' {
            let _ = chars.next_if_eq(&'\n');
            normalized.push_str(separator);
            continue;
        }
        if character == '\n' {
            normalized.push_str(separator);
        } else {
            normalized.push(character);
        }
    }
    Ok(Cow::Owned(normalized))
}

fn line_endings_already_normalized(text: &str, line_ending: LineEnding) -> bool {
    match line_ending {
        LineEnding::Lf => !text.contains('\r'),
        LineEnding::Cr => !text.contains('\n'),
        LineEnding::Crlf => {
            let bytes = text.as_bytes();
            let mut index = 0usize;
            while index < bytes.len() {
                match bytes[index] {
                    b'\r' if bytes.get(index + 1) == Some(&b'\n') => index += 2,
                    b'\r' | b'\n' => return false,
                    _ => index += 1,
                }
            }
            true
        }
        LineEnding::Mixed => false,
    }
}

/// Strip spaces and tabs immediately before line endings and at end-of-file.
fn trim_trailing_space_and_tabs(text: &str) -> String {
    let mut trimmed = String::with_capacity(text.len());
    let mut pending_spaces = String::new();

    for character in text.chars() {
        if matches!(character, ' ' | '\t') {
            pending_spaces.push(character);
            continue;
        }

        if matches!(character, '\n' | '\r') {
            pending_spaces.clear();
            trimmed.push(character);
            continue;
        }

        trimmed.push_str(&pending_spaces);
        pending_spaces.clear();
        trimmed.push(character);
    }

    trimmed
}

/// Encode normalized text bytes for the requested save encoding.
fn encode_text(
    text: &str,
    encoding: DocumentEncoding,
    allow_lossy: bool,
) -> Result<Cow<'_, [u8]>, EditorSaveError> {
    if !allow_lossy && let Some(preview) = analyze_lossy_encoding(text, encoding) {
        return Err(EditorSaveError::LossyEncoding {
            encoding,
            issue_count: preview.total_issue_count,
            preview,
        });
    }

    let bytes = match encoding {
        DocumentEncoding::Utf8 => Cow::Borrowed(text.as_bytes()),
        DocumentEncoding::Utf8Bom => {
            let mut bytes = Vec::with_capacity(text.len().saturating_add(3));
            bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            bytes.extend_from_slice(text.as_bytes());
            Cow::Owned(bytes)
        }
        _ => {
            let (encoded, _, _) = encoding.codec().encode(text);
            if encoding.writes_bom() {
                let mut with_bom = bom_bytes_for_encoding(encoding).to_vec();
                with_bom.extend_from_slice(encoded.as_ref());
                Cow::Owned(with_bom)
            } else {
                Cow::Owned(encoded.into_owned())
            }
        }
    };

    Ok(bytes)
}

/// Write already-prepared bytes to disk atomically.
///
/// The advisory write lock keeps an in-app save from racing a workspace-wide
/// Replace All on the same path; the shared durable-write helper owns the
/// temp-file-then-rename ordering, the destination metadata preservation, and
/// the before/after-rename failure classification. A pre-rename failure leaves
/// the previous bytes intact (`WriteTemp`); a post-rename directory-sync failure
/// means the new bytes are on disk but not yet crash-durable
/// (`DurabilityUnconfirmed`).
fn write_bytes_to_path(path: &Path, bytes: &[u8]) -> Result<(), EditorSaveError> {
    #[cfg(feature = "test-utils")]
    let injected_failure = {
        let mut failure = NEXT_SAVE_FAILURE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if failure.as_ref().is_some_and(|(target, _)| target == path) {
            failure.take().map(|(_, failure)| failure)
        } else {
            None
        }
    };
    #[cfg(feature = "test-utils")]
    if injected_failure == Some(SaveFailureForTest::BeforeRename) {
        return Err(EditorSaveError::WriteTemp {
            path: path.to_path_buf(),
            source: std::io::Error::other("injected pre-rename save failure"),
        });
    }

    let identity =
        fs_write::resolve_target_identity(path).map_err(|source| EditorSaveError::WriteTemp {
            path: path.to_path_buf(),
            source,
        })?;
    let write_path = identity.as_path().to_path_buf();
    let _path_lock = fs_write::TargetWriteGuard::from_identity(identity);
    fs_write::atomic_replace(&write_path, WriteLabel::SAVE, bytes)
        .map_err(|error| save_error_from_durable(error, path))?;
    #[cfg(feature = "test-utils")]
    if injected_failure == Some(SaveFailureForTest::AfterRename) {
        return Err(EditorSaveError::DurabilityUnconfirmed {
            path: path.to_path_buf(),
            source: std::io::Error::other("injected post-rename durability failure"),
        });
    }
    Ok(())
}

/// Translate a classified durable-write failure into the save-facing error.
///
/// A before-rename failure means the document was never written, so the user
/// must keep their unsaved-work signal (`WriteTemp`). An after-rename failure
/// means the bytes are already on disk but not yet crash-durable, which must be
/// reported distinctly (`DurabilityUnconfirmed`) so a directory-sync hiccup is
/// not mistaken for a lost save.
fn save_error_from_durable(error: fs_write::DurableWriteError, path: &Path) -> EditorSaveError {
    match error {
        fs_write::DurableWriteError::BeforeRename(source) => EditorSaveError::WriteTemp {
            path: path.to_path_buf(),
            source,
        },
        fs_write::DurableWriteError::AfterRename(source) => {
            EditorSaveError::DurabilityUnconfirmed {
                path: path.to_path_buf(),
                source,
            }
        }
    }
}

/// Return the BOM prefix bytes for save encodings that write one.
fn bom_bytes_for_encoding(encoding: DocumentEncoding) -> &'static [u8] {
    match encoding {
        DocumentEncoding::Utf8Bom => &[0xEF, 0xBB, 0xBF],
        DocumentEncoding::Utf16Le => &[0xFF, 0xFE],
        DocumentEncoding::Utf16Be => &[0xFE, 0xFF],
        _ => &[],
    }
}

/// Render one character for preview text, preserving invisible cases.
fn display_character(character: char) -> String {
    match character {
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        '\t' => "\\t".to_string(),
        '\u{00A0}' => "NBSP".to_string(),
        other if is_zero_width(other) => "zero-width".to_string(),
        other => other.to_string(),
    }
}

/// Format one scalar value as a Unicode codepoint label.
fn format_codepoint(character: char) -> String {
    format!("U+{:04X}", u32::from(character))
}

/// Return whether the character is one of the zero-width values we surface.
fn is_zero_width(character: char) -> bool {
    matches!(
        character,
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::{fixture, metadata as fs_metadata};
    use proptest::prelude::*;
    use std::assert_matches;
    use std::sync::atomic::AtomicBool;
    use tempfile::NamedTempFile;

    #[test]
    fn file_load_plan_is_payload_free_and_carries_conservative_weight() {
        let file = NamedTempFile::new().expect("temp file");
        fixture::write_text(file.path(), "hello");
        let cancel = AtomicBool::new(false);

        let plan = plan_text_file(file.path(), &cancel).expect("load plan");

        assert_eq!(plan.facts.byte_size, 5);
        assert_eq!(plan.transient_weight, transient_load_weight(5));
        assert!(std::mem::size_of_val(&plan) < 512);
    }

    #[test]
    fn planned_load_reports_growth_before_allocating_past_limit() {
        let file = NamedTempFile::new().expect("temp file");
        fixture::write_text(file.path(), "tiny");
        let cancel = AtomicBool::new(false);
        let plan = plan_text_file(file.path(), &cancel).expect("load plan");
        fixture::write_text(file.path(), "this grew beyond the test limit");

        let error = load_planned_text_file_with_limit(plan, &cancel, None, 10)
            .expect_err("growth should fail");

        assert_matches!(error, EditorLoadError::GrownTooLarge { .. });
    }

    #[test]
    fn planned_load_classifies_supported_growth_against_admitted_size() {
        let file = NamedTempFile::new().expect("temp file");
        fixture::write_text(file.path(), "tiny");
        let cancel = AtomicBool::new(false);
        let plan = plan_text_file(file.path(), &cancel).expect("load plan");
        fixture::write_text(file.path(), "tiny+");

        let error = load_planned_text_file(plan, &cancel, None).expect_err("growth should fail");

        assert_matches!(
            error,
            EditorLoadError::Grew {
                planned_bytes: 4,
                observed_min_bytes: 5,
                ..
            }
        );
    }

    #[test]
    fn admitted_read_limit_never_exceeds_planned_payload() {
        let file = NamedTempFile::new().expect("temp file");
        fixture::write_text(file.path(), "tiny");
        let cancel = AtomicBool::new(false);
        let plan = plan_text_file(file.path(), &cancel).expect("load plan");

        assert_eq!(admitted_read_limit(&plan, REFUSE_TO_OPEN), 4);
        assert_eq!(admitted_read_limit(&plan, 2), 2);
    }

    #[test]
    fn planned_load_rejects_atomic_replacement_with_same_size() {
        let file = NamedTempFile::new().expect("temp file");
        fixture::write_text(file.path(), "first");
        let cancel = AtomicBool::new(false);
        let plan = plan_text_file(file.path(), &cancel).expect("load plan");
        fs_write::atomic_replace(file.path(), WriteLabel::SAVE, b"other")
            .expect("atomic replacement");

        let error = load_planned_text_file(plan, &cancel, None)
            .expect_err("replacement should fail freshness");

        assert_matches!(error, EditorLoadError::Changed { .. });
    }

    #[test]
    fn planned_load_rejects_rename_and_recreated_original_path() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("document.txt");
        let moved = dir.path().join("moved.txt");
        fixture::write_text(&path, "same");
        let cancel = AtomicBool::new(false);
        let plan = plan_text_file(&path, &cancel).expect("load plan");
        fixture::rename(&path, &moved);
        fixture::write_text(&path, "same");

        let error = load_planned_text_file(plan, &cancel, None)
            .expect_err("renamed identity should fail freshness");

        assert_matches!(error, EditorLoadError::Changed { .. });
    }

    #[test]
    fn bounded_ingestion_loops_over_short_reads_and_honors_exact_limit() {
        let file = NamedTempFile::new().expect("temp file");
        fixture::write_repeated_bytes(file.path(), "é".as_bytes(), 128 * 1024);
        let bytes = fs_read::bounded_bytes(file.path(), 128 * 1024, 128 * 1024, || false)
            .expect("exact-limit read");

        assert_eq!(bytes.len(), 128 * 1024);
        assert!(simdutf8::basic::from_utf8(&bytes).is_ok());
    }

    #[test]
    fn bounded_ingestion_checks_cancellation_while_streaming() {
        let file = NamedTempFile::new().expect("temp file");
        fixture::write_repeated_bytes(file.path(), b"x", 256 * 1024);
        let mut checkpoints = 0usize;

        let error = fs_read::bounded_bytes(file.path(), 256 * 1024, 256 * 1024, || {
            checkpoints += 1;
            checkpoints > 1
        })
        .expect_err("second checkpoint should cancel");

        assert_matches!(error, fs_read::BoundedFileReadError::Cancelled);
    }

    #[test]
    fn bounded_ingestion_detects_one_byte_beyond_limit_without_retaining_it() {
        const SENTINEL_TEST_LIMIT: u64 = 64 * 1024;
        let file = NamedTempFile::new().expect("temp file");
        fixture::write_repeated_bytes(file.path(), b"x", SENTINEL_TEST_LIMIT + 1);

        let error = fs_read::bounded_bytes(
            file.path(),
            SENTINEL_TEST_LIMIT,
            SENTINEL_TEST_LIMIT,
            || false,
        )
        .expect_err("sentinel byte should prove overflow");

        assert_matches!(
            error,
            fs_read::BoundedFileReadError::LimitExceeded {
                byte_limit: SENTINEL_TEST_LIMIT
            }
        );
    }

    #[test]
    fn load_text_file_reads_utf8_and_classifies_size() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(file.path(), "hello\nworld");

        let cancel = AtomicBool::new(false);
        let result = load_text_file(file.path(), &cancel).expect("expected operation to succeed");

        assert_eq!(result.content, "hello\nworld");
        assert_eq!(result.metadata.size, 11);
        assert_eq!(result.metadata.size_check, FileSizeCheck::Normal);
        assert_eq!(
            result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::Utf8
        );
        assert_eq!(
            result.metadata.encoding_state.detected_line_ending,
            LineEnding::Lf
        );
        assert!(
            result.metadata.mtime.is_some(),
            "mtime should be populated from metadata"
        );
    }

    #[test]
    fn load_text_file_detects_utf8_bom_and_crlf() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(file.path(), [0xEF, 0xBB, 0xBF, b'a', b'\r', b'\n']);

        let cancel = AtomicBool::new(false);
        let result = load_text_file(file.path(), &cancel).expect("expected operation to succeed");

        assert_eq!(result.content, "a\r\n");
        assert_eq!(
            result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::Utf8Bom
        );
        assert_eq!(
            result.metadata.encoding_state.detected_line_ending,
            LineEnding::Crlf
        );
        assert!(result.metadata.has_bom);
        assert!(
            result
                .metadata
                .file_health
                .iter()
                .any(|finding| finding.kind == FileHealthFindingKind::Utf8Bom)
        );
    }

    #[test]
    fn load_text_file_explicit_reopen_strips_only_matching_bom() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(file.path(), [0xEF, 0xBB, 0xBF, b'a']);

        let cancel = AtomicBool::new(false);
        let matching =
            load_text_file_with_encoding(file.path(), &cancel, Some(DocumentEncoding::Utf8Bom))
                .expect("expected operation to succeed");
        assert_eq!(matching.content, "a");
        assert!(matching.metadata.has_bom);

        let plain_utf8 =
            load_text_file_with_encoding(file.path(), &cancel, Some(DocumentEncoding::Utf8))
                .expect("expected operation to succeed");
        assert_eq!(plain_utf8.content, "\u{feff}a");
        assert!(!plain_utf8.metadata.has_bom);
    }

    #[test]
    fn explicit_reopen_reports_typed_decode_failure() {
        let file = NamedTempFile::new().expect("temp file");
        fixture::write_bytes(file.path(), [0xFF, 0xFE, 0xFF]);
        let cancel = AtomicBool::new(false);

        let error =
            load_text_file_with_encoding(file.path(), &cancel, Some(DocumentEncoding::Utf8))
                .expect_err("invalid forced UTF-8 should fail decoding");

        assert_matches!(
            error,
            EditorLoadError::Decode {
                encoding: DocumentEncoding::Utf8,
                ..
            }
        );
    }

    #[test]
    fn load_text_file_decodes_windows_1252_when_utf8_fails() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(file.path(), [0x63, 0x61, 0x66, 0xE9]);

        let cancel = AtomicBool::new(false);
        let result = load_text_file(file.path(), &cancel).expect("expected operation to succeed");

        assert_eq!(result.content, "café");
        assert_eq!(
            result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::Windows1252
        );
        assert_eq!(
            result.metadata.encoding_state.decode_confidence,
            DecodeConfidence::Low
        );
        assert!(
            result
                .metadata
                .file_health
                .iter()
                .any(|finding| finding.kind == FileHealthFindingKind::LowConfidenceDecode)
        );
    }

    #[test]
    fn load_text_file_guesses_utf16_without_bom_when_signal_is_strong() {
        let le_file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(le_file.path(), [b'H', 0, 0xE9, 0, b'\n', 0]);

        let be_file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(be_file.path(), [0, b'H', 0, 0xE9, 0, b'\n']);

        let cancel = AtomicBool::new(false);
        let le_result =
            load_text_file(le_file.path(), &cancel).expect("expected operation to succeed");
        let be_result =
            load_text_file(be_file.path(), &cancel).expect("expected operation to succeed");

        assert_eq!(le_result.content, "Hé\n");
        assert_eq!(
            le_result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::Utf16Le
        );
        assert_eq!(
            le_result.metadata.encoding_state.decode_confidence,
            DecodeConfidence::Heuristic
        );
        assert_eq!(be_result.content, "Hé\n");
        assert_eq!(
            be_result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::Utf16Be
        );
        assert_eq!(
            be_result.metadata.encoding_state.decode_confidence,
            DecodeConfidence::Heuristic
        );
    }

    #[test]
    fn utf16_without_bom_heuristic_handles_boundary_and_ambiguous_zero_patterns() {
        assert_eq!(
            guess_utf16_without_bom(&[0xE9, 0, 0xE8, 0]),
            Some(DocumentEncoding::Utf16Le)
        );
        assert_eq!(
            guess_utf16_without_bom(&[0, 0xE9, 0, 0xE8]),
            Some(DocumentEncoding::Utf16Be)
        );

        let ambiguous_le_signal = [
            0, 0, 0, 0, 0xE9, 0, 0xE8, 0, 0xE7, 0, 0xE9, 0xE9, 0xE8, 0xE8, 0xE7, 0xE7, 0xE6, 0xE6,
            0xE5, 0xE5,
        ];
        let ambiguous_be_signal = [
            0, 0, 0, 0, 0, 0xE9, 0, 0xE8, 0, 0xE7, 0xE9, 0xE9, 0xE8, 0xE8, 0xE7, 0xE7, 0xE6, 0xE6,
            0xE5, 0xE5,
        ];

        assert_eq!(guess_utf16_without_bom(&ambiguous_le_signal), None);
        assert_eq!(guess_utf16_without_bom(&ambiguous_be_signal), None);
    }

    #[test]
    fn load_text_file_does_not_guess_utf16_for_short_or_odd_inputs() {
        let short_file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(short_file.path(), [0xFF, 0]);

        let odd_file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(odd_file.path(), [0, 0xFF, 0]);

        let cancel = AtomicBool::new(false);
        let short_result =
            load_text_file(short_file.path(), &cancel).expect("expected operation to succeed");
        let odd_result =
            load_text_file(odd_file.path(), &cancel).expect("expected operation to succeed");

        assert_eq!(
            short_result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::Windows1252
        );
        assert_eq!(
            odd_result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::Windows1252
        );
    }

    #[test]
    fn load_text_file_detects_mixed_line_endings() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(file.path(), "a\r\nb\nc\r");

        let cancel = AtomicBool::new(false);
        let result = load_text_file(file.path(), &cancel).expect("expected operation to succeed");

        assert_eq!(
            result.metadata.encoding_state.detected_line_ending,
            LineEnding::Mixed
        );
        assert!(
            result
                .metadata
                .file_health
                .iter()
                .any(|finding| finding.kind == FileHealthFindingKind::MixedLineEndings)
        );
    }

    #[test]
    fn detect_line_endings_classifies_single_styles_and_suggests_majority() {
        assert_eq!(detect_line_endings(""), (LineEnding::Lf, LineEnding::Crlf));
        assert_eq!(
            detect_line_endings("a\nb\n"),
            (LineEnding::Lf, LineEnding::Lf)
        );
        assert_eq!(
            detect_line_endings("a\r\nb\r\n"),
            (LineEnding::Crlf, LineEnding::Crlf)
        );
        assert_eq!(
            detect_line_endings("a\rb\r"),
            (LineEnding::Cr, LineEnding::Cr)
        );
        assert_eq!(
            detect_line_endings("a\r\nb\r\nc\n"),
            (LineEnding::Mixed, LineEnding::Crlf)
        );
        assert_eq!(
            detect_line_endings("a\rb\rc\n"),
            (LineEnding::Mixed, LineEnding::Cr)
        );
        assert_eq!(
            detect_line_endings("\r\nstart\nend\r\n"),
            (LineEnding::Mixed, LineEnding::Crlf)
        );
    }

    fn detect_line_endings_scalar(text: &str) -> (LineEnding, LineEnding) {
        let bytes = text.as_bytes();
        let mut index = 0usize;
        let mut crlf_count = 0usize;
        let mut lf_count = 0usize;
        let mut cr_count = 0usize;
        while index < bytes.len() {
            match bytes[index] {
                b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                    crlf_count += 1;
                    index += 2;
                }
                b'\r' => {
                    cr_count += 1;
                    index += 1;
                }
                b'\n' => {
                    lf_count += 1;
                    index += 1;
                }
                _ => index += 1,
            }
        }
        let distinct_styles =
            usize::from(crlf_count > 0) + usize::from(lf_count > 0) + usize::from(cr_count > 0);
        let detected = match distinct_styles {
            1 if crlf_count > 0 => LineEnding::Crlf,
            1 if cr_count > 0 => LineEnding::Cr,
            0 | 1 => LineEnding::Lf,
            _ => LineEnding::Mixed,
        };
        let suggested = if crlf_count >= lf_count && crlf_count >= cr_count {
            LineEnding::Crlf
        } else if cr_count >= lf_count {
            LineEnding::Cr
        } else {
            LineEnding::Lf
        };
        (detected, suggested)
    }

    proptest! {
        #[test]
        fn optimized_line_endings_match_scalar_policy(
            pieces in prop::collection::vec(
                prop_oneof![Just("x"), Just("é"), Just("🙂"), Just("\r"), Just("\n")],
                0..4_096,
            )
        ) {
            let text = pieces.concat();
            prop_assert_eq!(detect_line_endings(&text), detect_line_endings_scalar(&text));
        }
    }

    #[test]
    fn load_text_file_does_not_mark_utf16_nul_bytes_as_binary_like() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(file.path(), [0xFF, 0xFE, b'A', 0]);

        let cancel = AtomicBool::new(false);
        let result = load_text_file(file.path(), &cancel).expect("expected operation to succeed");

        assert_eq!(
            result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::Utf16Le
        );
        assert!(
            !result
                .metadata
                .file_health
                .iter()
                .any(|finding| finding.kind == FileHealthFindingKind::BinaryLikeContent)
        );
        assert!(
            !result
                .metadata
                .file_health
                .iter()
                .any(|finding| finding.kind == FileHealthFindingKind::Utf8Bom)
        );
    }

    #[test]
    fn load_text_file_reports_space_and_zero_width_health_counts() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(file.path(), "a\u{00a0}b\u{00a0}c\u{200b}");

        let cancel = AtomicBool::new(false);
        let result = load_text_file(file.path(), &cancel).expect("expected operation to succeed");

        let nbsp = result
            .metadata
            .file_health
            .iter()
            .find(|finding| finding.kind == FileHealthFindingKind::NonBreakingSpace)
            .expect("NBSP finding should be present");
        let zero_width = result
            .metadata
            .file_health
            .iter()
            .find(|finding| finding.kind == FileHealthFindingKind::ZeroWidthCharacter)
            .expect("zero-width finding should be present");
        assert!(nbsp.body.contains("2 non-breaking space"));
        assert!(zero_width.body.contains("1 zero-width character"));
    }

    #[test]
    fn load_text_file_omits_absent_space_character_health_findings() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(file.path(), "plain text");

        let cancel = AtomicBool::new(false);
        let result = load_text_file(file.path(), &cancel).expect("expected operation to succeed");

        assert!(
            !result
                .metadata
                .file_health
                .iter()
                .any(|finding| finding.kind == FileHealthFindingKind::NonBreakingSpace)
        );
        assert!(
            !result
                .metadata
                .file_health
                .iter()
                .any(|finding| finding.kind == FileHealthFindingKind::ZeroWidthCharacter)
        );
    }

    #[test]
    fn load_text_file_honors_cancellation() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(file.path(), "hello");

        let cancel = AtomicBool::new(true);
        let result = load_text_file(file.path(), &cancel);

        assert_matches!(result, Err(EditorLoadError::Cancelled));
    }

    /// Build a chunked-path fixture body larger than the direct threshold.
    fn large_body(seed: &str) -> String {
        let target =
            DIRECT_LOAD_PROCESSING_THRESHOLD_BYTES + DIRECT_LOAD_PROCESSING_THRESHOLD_BYTES / 2;
        let mut body = String::with_capacity(target + seed.len());
        while body.len() < target {
            body.push_str(seed);
        }
        body
    }

    fn utf16_bytes(text: &str, little_endian: bool, with_bom: bool) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(text.len() * 2 + 2);
        if with_bom {
            bytes.extend_from_slice(if little_endian {
                &[0xFF, 0xFE]
            } else {
                &[0xFE, 0xFF]
            });
        }
        for unit in text.encode_utf16() {
            let pair = if little_endian {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            };
            bytes.extend_from_slice(&pair);
        }
        bytes
    }

    fn loaded(bytes: &[u8]) -> LoadResult {
        let file = NamedTempFile::new().expect("temp fixture file");
        fixture::write_bytes(file.path(), bytes);
        let cancel = AtomicBool::new(false);
        load_text_file(file.path(), &cancel).expect("uncancelled load succeeds")
    }

    fn reference_counts(content: &str) -> (usize, usize) {
        let nbsp = content
            .chars()
            .filter(|&character| character == '\u{00A0}')
            .count();
        let zero_width = content
            .chars()
            .filter(|&character| is_zero_width(character))
            .count();
        (nbsp, zero_width)
    }

    fn health_count(result: &LoadResult, kind: FileHealthFindingKind) -> usize {
        result
            .metadata
            .file_health
            .iter()
            .filter(|finding| finding.kind == kind)
            .count()
    }

    #[test]
    fn chunked_ascii_and_multibyte_utf8_match_reference_across_slice_boundaries() {
        let ascii = large_body("plain ascii line\r\n");
        let ascii_result = loaded(ascii.as_bytes());
        assert_eq!(ascii_result.content, ascii);
        assert_eq!(
            ascii_result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::Utf8
        );
        assert_eq!(
            ascii_result.metadata.encoding_state.detected_line_ending,
            detect_line_endings(&ascii).0
        );

        // Four-byte, three-byte, and two-byte scalars with a stride that is
        // deliberately coprime to the slice size so scalars straddle slices.
        let multibyte = large_body("é漢🎉 zero\u{200B}width nbsp\u{00A0} text\n");
        let multibyte_result = loaded(multibyte.as_bytes());
        assert_eq!(multibyte_result.content, multibyte);
        assert_eq!(
            multibyte_result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::Utf8
        );
        let (nbsp, zero_width) = reference_counts(&multibyte);
        assert!(nbsp > 0 && zero_width > 0);
        assert_eq!(
            health_count(&multibyte_result, FileHealthFindingKind::NonBreakingSpace),
            1
        );
        assert!(
            multibyte_result
                .metadata
                .file_health
                .iter()
                .any(
                    |finding| finding.kind == FileHealthFindingKind::NonBreakingSpace
                        && finding.body.contains(&nbsp.to_string())
                )
        );
        assert!(
            multibyte_result
                .metadata
                .file_health
                .iter()
                .any(
                    |finding| finding.kind == FileHealthFindingKind::ZeroWidthCharacter
                        && finding.body.contains(&zero_width.to_string())
                )
        );
    }

    #[test]
    fn chunked_bom_and_fallback_paths_match_reference_decodes() {
        let body = large_body("bom guarded utf8 content é\n");
        let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
        utf8_bom.extend_from_slice(body.as_bytes());
        let bom_result = loaded(&utf8_bom);
        assert_eq!(bom_result.content, body);
        assert_eq!(
            bom_result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::Utf8Bom
        );
        assert!(bom_result.metadata.has_bom);

        for (little_endian, expected) in [
            (true, DocumentEncoding::Utf16Le),
            (false, DocumentEncoding::Utf16Be),
        ] {
            let text = large_body("utf16 content with accents é and lines\r\n");
            let bytes = utf16_bytes(&text, little_endian, true);
            let result = loaded(&bytes);
            assert_eq!(result.content, text);
            assert_eq!(result.metadata.encoding_state.opened_encoding, expected);
            assert_eq!(
                result.metadata.encoding_state.decode_confidence,
                DecodeConfidence::Exact
            );
            assert!(result.metadata.has_bom);
            assert_eq!(
                health_count(&result, FileHealthFindingKind::BinaryLikeContent),
                0,
                "UTF-16 zero bytes must not be reported as binary-like content"
            );
        }

        // BOM-less UTF-16 needs a non-ASCII scalar early so chunked UTF-8
        // validation fails before the parity heuristic takes over.
        let bomless_text = format!("é{}", large_body("bomless utf16 line\n"));
        let bomless = utf16_bytes(&bomless_text, true, false);
        let bomless_result = loaded(&bomless);
        assert_eq!(bomless_result.content, bomless_text);
        assert_eq!(
            bomless_result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::Utf16Le
        );
        assert_eq!(
            bomless_result.metadata.encoding_state.decode_confidence,
            DecodeConfidence::Heuristic
        );

        // Windows-1252 fallback: 0x93/0x94 smart quotes are invalid UTF-8.
        let fallback_seed = b"fallback \x93quoted\x94 text with nul \x00 evidence\r".repeat(40_000);
        let fallback_result = loaded(&fallback_seed);
        let (reference, reference_errors) = DocumentEncoding::Windows1252
            .codec()
            .decode_without_bom_handling(&fallback_seed);
        assert!(!reference_errors);
        assert_eq!(fallback_result.content, reference);
        assert_eq!(
            fallback_result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::Windows1252
        );
        assert_eq!(
            fallback_result.metadata.encoding_state.decode_confidence,
            DecodeConfidence::Low
        );
        assert_eq!(
            health_count(&fallback_result, FileHealthFindingKind::BinaryLikeContent),
            1,
            "NUL evidence must survive the chunked raw-byte scan"
        );
        assert_eq!(
            fallback_result.metadata.encoding_state.detected_line_ending,
            detect_line_endings(&reference).0
        );
        assert_eq!(
            fallback_result.metadata.encoding_state.save_line_ending,
            detect_line_endings(&reference).1
        );
    }

    #[test]
    fn chunked_mixed_line_endings_match_whole_text_classification() {
        let mixed = large_body("one\r\ntwo\nthree\rfour\n");
        let result = loaded(mixed.as_bytes());
        let (reference_detected, reference_suggested) = detect_line_endings(&mixed);
        assert_eq!(reference_detected, LineEnding::Mixed);
        assert_eq!(
            result.metadata.encoding_state.detected_line_ending,
            reference_detected
        );
        assert_eq!(
            result.metadata.encoding_state.save_line_ending,
            reference_suggested
        );
        assert_eq!(
            health_count(&result, FileHealthFindingKind::MixedLineEndings),
            1
        );
    }

    #[test]
    fn continuation_byte_runs_fail_fast_without_extending_a_slice() {
        // A valid ASCII prefix ends exactly at the first slice boundary; the
        // rest is binary padding whose bytes all match the UTF-8 continuation
        // pattern. The boundary extension must reject the run after at most
        // three bytes instead of stretching one slice across the remainder.
        let mut bytes = vec![b'a'; LOAD_PROCESSING_CHUNK_BYTES];
        bytes.extend(std::iter::repeat_n(
            0x80u8,
            DIRECT_LOAD_PROCESSING_THRESHOLD_BYTES + LOAD_PROCESSING_CHUNK_BYTES,
        ));
        let never = AtomicBool::new(false);

        let _ = take_load_processing_chunks_for_test();
        assert_eq!(
            try_decode_valid_utf8(&bytes, &never).expect("uncancelled classification"),
            None
        );
        assert_eq!(
            take_load_processing_chunks_for_test(),
            1,
            "the run must be rejected inside the first slice"
        );

        // The fallback decode still matches the whole-buffer reference.
        let (content, had_errors) =
            decode_with_codec_cancellable(&bytes, DocumentEncoding::Windows1252, &never)
                .expect("uncancelled fallback decode");
        let (reference, reference_errors) = DocumentEncoding::Windows1252
            .codec()
            .decode_without_bom_handling(&bytes);
        assert_eq!(content, reference);
        assert_eq!(had_errors, reference_errors);
    }

    #[test]
    fn line_ending_tally_carries_cr_across_slice_boundaries() {
        let text = "alpha\r\nbeta\rgamma\nend\r";
        for split in 0..=text.len() {
            let mut tally = LineEndingTally::default();
            tally.scan_slice(&text.as_bytes()[..split]);
            tally.scan_slice(&text.as_bytes()[split..]);
            let sliced = tally.finish();

            let mut whole = LineEndingTally::default();
            whole.scan_slice(text.as_bytes());
            assert_eq!(
                sliced,
                whole.finish(),
                "sliced tally diverged at split {split}"
            );
        }
    }

    #[test]
    fn cancellation_stops_classification_decoding_and_analysis_stages() {
        // UTF-16 LE with BOM: stage layout is decode slices then analysis
        // slices; the NUL scan is skipped for UTF-16.
        let text = "a".repeat(2 * 1024 * 1024);
        let bom_bytes = utf16_bytes(&text, true, true);
        let payload_bytes = bom_bytes.len() - 2;
        let decode_slices =
            u64::try_from(payload_bytes.div_ceil(LOAD_PROCESSING_CHUNK_BYTES)).expect("fits");
        let analysis_slices =
            u64::try_from(text.len().div_ceil(LOAD_PROCESSING_CHUNK_BYTES)).expect("fits");
        let file = NamedTempFile::new().expect("utf16 fixture");
        fixture::write_bytes(file.path(), &bom_bytes);

        // Success baseline: exact bounded slice budget, no cancellation.
        let _ = take_load_processing_chunks_for_test();
        cancel_load_after_processing_chunks_for_test(None);
        let cancel = AtomicBool::new(false);
        let success = load_text_file(file.path(), &cancel).expect("uncancelled load");
        assert_eq!(success.content, text);
        assert_eq!(
            take_load_processing_chunks_for_test(),
            decode_slices + analysis_slices
        );

        // Cancellation during incremental decoding.
        cancel_load_after_processing_chunks_for_test(Some(1));
        let cancel = AtomicBool::new(false);
        let result = load_text_file(file.path(), &cancel);
        cancel_load_after_processing_chunks_for_test(None);
        assert_matches!(result, Err(EditorLoadError::Cancelled));
        assert_eq!(
            take_load_processing_chunks_for_test(),
            1,
            "no further slices may run after decode-stage cancellation"
        );

        // Cancellation during fused line-ending/health analysis.
        cancel_load_after_processing_chunks_for_test(Some(decode_slices + 2));
        let cancel = AtomicBool::new(false);
        let result = load_text_file(file.path(), &cancel);
        cancel_load_after_processing_chunks_for_test(None);
        assert_matches!(result, Err(EditorLoadError::Cancelled));
        assert_eq!(
            take_load_processing_chunks_for_test(),
            decode_slices + 2,
            "no further slices may run after analysis-stage cancellation"
        );

        // Classification-stage cancellation: BOM-less UTF-16 first fails the
        // chunked UTF-8 validation in its first slice, then the parity
        // heuristic owns the following slices.
        let bomless_text = format!("é{}", "a".repeat(2 * 1024 * 1024));
        let bomless_bytes = utf16_bytes(&bomless_text, true, false);
        let bomless_file = NamedTempFile::new().expect("bomless utf16 fixture");
        fixture::write_bytes(bomless_file.path(), &bomless_bytes);
        cancel_load_after_processing_chunks_for_test(Some(3));
        let cancel = AtomicBool::new(false);
        let result = load_text_file(bomless_file.path(), &cancel);
        cancel_load_after_processing_chunks_for_test(None);
        assert_matches!(result, Err(EditorLoadError::Cancelled));
        assert_eq!(
            take_load_processing_chunks_for_test(),
            3,
            "no further slices may run after classification-stage cancellation"
        );

        eprintln!(
            "editor-load-slice-evidence payload_bytes={payload_bytes} decode_slices={decode_slices} analysis_slices={analysis_slices} decode_cancel_slices=1 analysis_cancel_slices={} classification_cancel_slices=3",
            decode_slices + 2
        );
    }

    /// Opt-in near-supported-limit load diagnostic.
    ///
    /// Run explicitly with `cargo test -p lushtext-core --lib
    /// near_supported_limit_load_diagnostic -- --ignored --nocapture`;
    /// `LUSHTEXT_NEAR_LIMIT_LOAD_BYTES` overrides the fixture size for smaller
    /// hosts. It records fixture size, encoding, build profile, environment,
    /// resident-memory context, bounded cancellation progress, and planned
    /// transient ownership, and stays ignored so default validation gains no
    /// host-sensitive timing or memory gate.
    #[test]
    #[ignore = "opt-in near-supported-limit diagnostic, never a default CI gate"]
    fn near_supported_limit_load_diagnostic() {
        let fixture_bytes = std::env::var("LUSHTEXT_NEAR_LIMIT_LOAD_BYTES")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(REFUSE_TO_OPEN.saturating_sub(1_000_000))
            .min(REFUSE_TO_OPEN.saturating_sub(1));
        let file = NamedTempFile::new().expect("near-limit fixture file");
        fixture::write_repeated_bytes(file.path(), b"near-limit diagnostic line\n", fixture_bytes);

        let rss_before_kib = resident_memory_kib();
        let _ = take_load_processing_chunks_for_test();

        let cancel = AtomicBool::new(false);
        let plan = plan_text_file(file.path(), &cancel).expect("near-limit plan");
        let planned_weight = plan.transient_weight;
        let started = std::time::Instant::now();
        let result = load_planned_text_file(plan, &cancel, None).expect("near-limit load");
        let load_millis = started.elapsed().as_millis();
        let success_slices = take_load_processing_chunks_for_test();
        let rss_after_kib = resident_memory_kib();
        assert_eq!(
            u64::try_from(result.content.len()).expect("content length fits"),
            fixture_bytes
        );

        cancel_load_after_processing_chunks_for_test(Some(4));
        let cancel = AtomicBool::new(false);
        let plan = plan_text_file(file.path(), &cancel).expect("near-limit cancel plan");
        let cancelled = load_planned_text_file(plan, &cancel, None);
        cancel_load_after_processing_chunks_for_test(None);
        let cancelled_slices = take_load_processing_chunks_for_test();
        assert_matches!(cancelled, Err(EditorLoadError::Cancelled));
        assert_eq!(cancelled_slices, 4);

        eprintln!(
            "near-limit-load-evidence fixture_bytes={fixture_bytes} encoding={} profile={} os={} arch={} rss_before_kib={rss_before_kib:?} rss_after_kib={rss_after_kib:?} planned_transient_weight={planned_weight} success_slices={success_slices} load_millis={load_millis} cancelled_slices={cancelled_slices}",
            result.metadata.encoding_state.opened_encoding.label(),
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            std::env::consts::OS,
            std::env::consts::ARCH,
        );
    }

    /// Best-effort resident-set sample for the opt-in diagnostic only.
    fn resident_memory_kib() -> Option<u64> {
        let status = fs_read::text(Path::new("/proc/self/status")).ok()?;
        status.lines().find_map(|line| {
            line.strip_prefix("VmRSS:")
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|value| value.parse().ok())
        })
    }

    #[test]
    fn load_text_file_too_large_reports_decimal_megabytes_without_reading() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        file.as_file()
            .set_len(501_000_000)
            .expect("expected operation to succeed");

        let cancel = AtomicBool::new(false);
        let result = load_text_file(file.path(), &cancel);

        assert_matches!(result, Err(EditorLoadError::TooLarge { size_mb: 501, .. }));
    }

    #[test]
    fn load_text_file_supports_explicit_reopen_encoding() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(file.path(), [0x82, 0xA0]);

        let cancel = AtomicBool::new(false);
        let result =
            load_text_file_with_encoding(file.path(), &cancel, Some(DocumentEncoding::ShiftJis))
                .expect("expected operation to succeed");

        assert_eq!(result.content, "あ");
        assert_eq!(
            result.metadata.encoding_state.opened_encoding,
            DocumentEncoding::ShiftJis
        );
        assert_eq!(
            result.metadata.encoding_state.decode_confidence,
            DecodeConfidence::Exact
        );
    }

    #[test]
    fn analyze_lossy_encoding_reports_unrepresentable_characters() {
        let preview = analyze_lossy_encoding("hello € 😀", DocumentEncoding::Windows1252)
            .expect("expected lossy preview");

        assert_eq!(preview.total_issue_count, 1);
        assert_eq!(preview.issues[0].character, '😀');
    }

    #[test]
    fn lossy_encoding_preview_summarizes_and_details_sampled_issues() {
        let singular = LossyEncodingPreview {
            target_encoding: DocumentEncoding::Windows1252,
            total_issue_count: 1,
            issues: vec![LossyEncodingIssue {
                line: 1,
                column: 5,
                character: '😀',
            }],
        };
        assert_eq!(
            singular.summary(),
            "Windows-1252 cannot represent 1 character in the current document."
        );

        let plural = LossyEncodingPreview {
            target_encoding: DocumentEncoding::ShiftJis,
            total_issue_count: 3,
            issues: vec![
                LossyEncodingIssue {
                    line: 1,
                    column: 1,
                    character: 'A',
                },
                LossyEncodingIssue {
                    line: 2,
                    column: 1,
                    character: '\n',
                },
                LossyEncodingIssue {
                    line: 3,
                    column: 2,
                    character: '\u{200b}',
                },
            ],
        };

        assert_eq!(
            plural.summary(),
            "Shift_JIS cannot represent 3 characters in the current document."
        );
        assert_eq!(
            plural.detail_lines(),
            vec![
                "Line 1, column 1: A (U+0041)",
                "Line 2, column 1: \\n (U+000A)",
                "Line 3, column 2: zero-width (U+200B)",
            ]
        );
    }

    #[test]
    fn analyze_lossy_encoding_counts_all_issues_but_caps_preview_sample() {
        let preview = analyze_lossy_encoding("😀😀😀😀\n😀😀😀😀😀", DocumentEncoding::Windows1252)
            .expect("expected lossy preview");

        assert_eq!(preview.total_issue_count, 9);
        assert_eq!(preview.issues.len(), MAX_LOSSY_PREVIEW_ISSUES);
        assert_eq!(preview.issues[0].line, 1);
        assert_eq!(preview.issues[0].column, 1);
        assert_eq!(preview.issues[4].line, 2);
        assert_eq!(preview.issues[4].column, 1);
        assert_eq!(preview.issues[7].line, 2);
        assert_eq!(preview.issues[7].column, 4);
    }

    #[test]
    fn analyze_lossy_encoding_short_circuits_unicode_encodings() {
        let text = "ASCII\r\n日本語 e\u{301} 😀";
        for encoding in [
            DocumentEncoding::Utf8,
            DocumentEncoding::Utf8Bom,
            DocumentEncoding::Utf16Le,
            DocumentEncoding::Utf16Be,
        ] {
            assert_eq!(analyze_lossy_encoding(text, encoding), None);
        }
    }

    #[test]
    fn analyze_lossy_encoding_preserves_crlf_combining_and_consecutive_positions() {
        let preview =
            analyze_lossy_encoding("A\r\nB\u{301}😀\n😀😀", DocumentEncoding::Windows1252)
                .expect("expected exact Windows-1252 issues");

        assert_eq!(preview.total_issue_count, 4);
        assert_eq!(
            preview.issues,
            vec![
                LossyEncodingIssue {
                    line: 2,
                    column: 2,
                    character: '\u{301}',
                },
                LossyEncodingIssue {
                    line: 2,
                    column: 3,
                    character: '😀',
                },
                LossyEncodingIssue {
                    line: 3,
                    column: 1,
                    character: '😀',
                },
                LossyEncodingIssue {
                    line: 3,
                    column: 2,
                    character: '😀',
                },
            ]
        );
    }

    #[test]
    fn analyze_lossy_encoding_accepts_shift_jis_boundaries_and_reports_astral() {
        let preview = analyze_lossy_encoding("ASCII あいう 😀", DocumentEncoding::ShiftJis)
            .expect("expected one Shift_JIS issue");
        assert_eq!(preview.total_issue_count, 1);
        assert_eq!(preview.issues[0].character, '😀');
        assert_eq!(preview.issues[0].column, 11);
    }

    #[test]
    fn write_document_to_path_replaces_destination() {
        let dir = tempfile::tempdir().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");

        let write_result = write_document_to_path(
            &path,
            "saved\ntext",
            DocumentEncoding::Utf8,
            LineEnding::Crlf,
            false,
        )
        .expect("expected operation to succeed");

        assert_eq!(write_result.bytes_written, 11);
        assert!(
            write_result.modified_at_secs.is_some(),
            "mtime should be populated after write"
        );
        assert_eq!(fixture::read_text(&path), "saved\r\ntext");
    }

    #[cfg(unix)]
    #[test]
    fn write_document_to_path_updates_symlink_target_without_replacing_link() {
        let dir = tempfile::tempdir().expect("symlink save tempdir");
        let target = dir.path().join("target.txt");
        let link = dir.path().join("link.txt");
        fixture::write_bytes(&target, "old target");
        fixture::symlink(&target, &link);

        write_document_to_path(
            &link,
            "new target",
            DocumentEncoding::Utf8,
            LineEnding::Lf,
            false,
        )
        .expect("save through symlink");

        assert!(
            fixture::is_symlink(&link),
            "the display path must remain a symlink"
        );
        assert_eq!(fixture::read_text(&target), "new target");
    }

    #[cfg(unix)]
    #[test]
    fn write_document_to_path_fails_broken_symlink_before_replacement() {
        let dir = tempfile::tempdir().expect("broken symlink save tempdir");
        let missing_target = dir.path().join("missing-target.txt");
        let link = dir.path().join("link.txt");
        fixture::symlink(&missing_target, &link);

        let result = write_document_to_path(
            &link,
            "new target",
            DocumentEncoding::Utf8,
            LineEnding::Lf,
            false,
        );

        assert_matches!(result, Err(EditorSaveError::WriteTemp { .. }));
        assert!(
            fixture::is_symlink(&link),
            "failed save must leave the symlink untouched"
        );
    }

    #[test]
    fn write_document_to_path_writes_bom_for_bom_save_encodings() {
        let dir = tempfile::tempdir().expect("expected operation to succeed");
        let utf8_path = dir.path().join("utf8.txt");
        let utf16le_path = dir.path().join("utf16le.txt");
        let utf16be_path = dir.path().join("utf16be.txt");

        write_document_to_path(
            &utf8_path,
            "A",
            DocumentEncoding::Utf8Bom,
            LineEnding::Lf,
            false,
        )
        .expect("expected operation to succeed");
        write_document_to_path(
            &utf16le_path,
            "A",
            DocumentEncoding::Utf16Le,
            LineEnding::Lf,
            false,
        )
        .expect("expected operation to succeed");
        write_document_to_path(
            &utf16be_path,
            "A",
            DocumentEncoding::Utf16Be,
            LineEnding::Lf,
            false,
        )
        .expect("expected operation to succeed");

        assert_eq!(fixture::read_bytes(&utf8_path), [0xEF, 0xBB, 0xBF, b'A']);
        assert_eq!(
            bom_bytes_for_encoding(DocumentEncoding::Utf8Bom),
            &[0xEF, 0xBB, 0xBF]
        );
        assert!(fixture::read_bytes(&utf16le_path).starts_with(&[0xFF, 0xFE]));
        assert!(fixture::read_bytes(&utf16be_path).starts_with(&[0xFE, 0xFF]));
    }

    #[test]
    fn write_document_to_path_blocks_mixed_line_endings_policy() {
        let dir = tempfile::tempdir().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");

        let result = write_document_to_path(
            &path,
            "saved",
            DocumentEncoding::Utf8,
            LineEnding::Mixed,
            false,
        );

        assert_matches!(result, Err(EditorSaveError::MixedLineEndings));
    }

    #[test]
    fn unchanged_save_transforms_borrow_the_captured_payload() {
        let text = "first\nsecond\n";
        assert!(matches!(
            apply_save_formatting_overrides_borrowed(text, FormattingOverrides::default()),
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            normalize_line_endings(text, LineEnding::Lf),
            Ok(Cow::Borrowed(_))
        ));
        assert!(matches!(
            encode_text(text, DocumentEncoding::Utf8, false),
            Ok(Cow::Borrowed(_))
        ));

        assert!(matches!(
            normalize_line_endings(text, LineEnding::Crlf),
            Ok(Cow::Owned(_))
        ));
        assert!(matches!(
            apply_save_formatting_overrides_borrowed(
                "text",
                FormattingOverrides {
                    insert_final_newline: Some(true),
                    ..FormattingOverrides::default()
                }
            ),
            Cow::Owned(_)
        ));
    }

    #[test]
    fn apply_save_formatting_overrides_trims_trailing_space() {
        let formatted = apply_save_formatting_overrides(
            "keep  middle\t \ntrim\t\r\nend\t ",
            FormattingOverrides {
                trim_trailing_whitespace: Some(true),
                ..Default::default()
            },
        );

        assert_eq!(formatted, "keep  middle\ntrim\r\nend");
    }

    #[test]
    fn apply_save_formatting_overrides_controls_final_newline() {
        let inserted = apply_save_formatting_overrides(
            "text",
            FormattingOverrides {
                insert_final_newline: Some(true),
                ..Default::default()
            },
        );
        assert_eq!(inserted, "text\n");

        let removed = apply_save_formatting_overrides(
            "text\r\n\n",
            FormattingOverrides {
                insert_final_newline: Some(false),
                ..Default::default()
            },
        );
        assert_eq!(removed, "text");
    }

    #[test]
    fn apply_save_formatting_overrides_does_not_add_redundant_final_newline() {
        let overrides = FormattingOverrides {
            insert_final_newline: Some(true),
            ..Default::default()
        };

        assert_eq!(apply_save_formatting_overrides("", overrides), "");
        assert_eq!(
            apply_save_formatting_overrides("text\n", overrides),
            "text\n"
        );
        assert_eq!(
            apply_save_formatting_overrides("text\r", overrides),
            "text\r"
        );
    }

    #[test]
    fn mtime_and_now_epoch_helpers_report_current_nonzero_seconds() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(file.path(), "mtime");

        assert!(now_epoch_secs() > 1_700_000_000);
        assert!(mtime_secs(file.path()).expect("mtime should exist") > 1_700_000_000);
    }

    #[test]
    fn write_document_to_path_returns_lossy_preview_before_write() {
        let dir = tempfile::tempdir().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");

        let result = write_document_to_path(
            &path,
            "emoji 😀",
            DocumentEncoding::Windows1252,
            LineEnding::Lf,
            false,
        );

        assert_matches!(result, Err(EditorSaveError::LossyEncoding { .. }));
        assert!(
            !fs_metadata::exists(&path),
            "the file should not be written when lossy conversion is blocked"
        );
    }

    #[test]
    fn save_error_from_durable_maps_failure_phases_to_distinct_variants() {
        let path = std::path::Path::new("/tmp/does-not-matter.txt");

        let before = save_error_from_durable(
            fs_write::DurableWriteError::BeforeRename(std::io::Error::other("temp failed")),
            path,
        );
        assert_matches!(before, EditorSaveError::WriteTemp { .. });

        let after = save_error_from_durable(
            fs_write::DurableWriteError::AfterRename(std::io::Error::other("dir sync failed")),
            path,
        );
        assert_matches!(after, EditorSaveError::DurabilityUnconfirmed { .. });
    }

    #[cfg(unix)]
    #[test]
    fn write_document_to_path_preserves_existing_mode_and_executable_bit() {
        let dir = tempfile::tempdir().expect("expected operation to succeed");

        let private = dir.path().join("private.txt");
        fixture::write_bytes(&private, "old");
        fixture::set_mode(&private, 0o600);
        write_document_to_path(
            &private,
            "new",
            DocumentEncoding::Utf8,
            LineEnding::Lf,
            false,
        )
        .expect("save private file");
        let private_mode = fixture::mode(&private) & 0o777;
        assert_eq!(
            private_mode, 0o600,
            "saving must not widen a 0600 file's permissions"
        );

        let script = dir.path().join("script.sh");
        fixture::write_bytes(&script, "#!/bin/sh\necho old\n");
        fixture::set_mode(&script, 0o755);
        write_document_to_path(
            &script,
            "#!/bin/sh\necho new\n",
            DocumentEncoding::Utf8,
            LineEnding::Lf,
            false,
        )
        .expect("save script");
        let script_mode = fixture::mode(&script);
        assert_ne!(
            script_mode & 0o111,
            0,
            "saving an executable script must keep it executable"
        );
    }
}
