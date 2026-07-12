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
use crate::model::formatting_overrides::FormattingOverrides;
use crate::services::file_limits::FileSizeCheck;
use crate::services::filesystem::{
    WriteLabel, metadata as fs_metadata, read as fs_read, write as fs_write,
};
use std::borrow::Cow;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
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

#[cfg(feature = "test-utils")]
static LOAD_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Configure an artificial editor-load delay for widget race tests.
#[cfg(feature = "test-utils")]
pub fn set_load_delay_for_test(delay_ms: u64) {
    LOAD_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Successful result from `load_text_file`.
#[derive(Debug)]
pub struct LoadResult {
    pub content: String,
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
    #[error("Save already in progress")]
    SaveInProgress,
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
    if cancel.load(Ordering::Acquire) {
        return Err(EditorLoadError::Cancelled);
    }

    delay_load_for_test();

    let facts = fs_metadata::file_facts(path).map_err(|source| EditorLoadError::Metadata {
        path: path.to_path_buf(),
        source,
    })?;
    let size = facts.byte_size;
    let size_check = FileSizeCheck::classify(size);
    let canonical_path = facts.canonical_path;
    let mtime = facts.modified_at_secs;

    if size_check == FileSizeCheck::TooLarge {
        return Err(EditorLoadError::TooLarge {
            path: path.to_path_buf(),
            size_mb: size / 1_000_000,
        });
    }

    if cancel.load(Ordering::Acquire) {
        return Err(EditorLoadError::Cancelled);
    }

    let bytes = fs_read::bytes(path).map_err(|source| EditorLoadError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if cancel.load(Ordering::Acquire) {
        return Err(EditorLoadError::Cancelled);
    }

    let decoded = decode_document(&bytes, reopen_as);
    let file_health = build_file_health(&decoded.content, &decoded, &bytes);

    Ok(LoadResult {
        content: decoded.content,
        size,
        size_check,
        canonical_path,
        mtime,
        encoding_state: decoded.encoding_state,
        has_bom: decoded.has_bom,
        file_health,
    })
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
/// This deliberately reuses `decode_document()` and `build_file_health()` so
/// fuzzing exercises the production byte-ingestion path while staying free of
/// disk I/O, GTK widgets, and cancellation timing.
#[cfg(feature = "fuzzing")]
#[must_use]
pub fn classify_bytes_for_fuzzing(
    bytes: &[u8],
    reopen_as: Option<DocumentEncoding>,
) -> FuzzedEditorBytes {
    let decoded = decode_document(bytes, reopen_as);
    let file_health = build_file_health(&decoded.content, &decoded, bytes);

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
    let normalized = normalize_line_endings(text, line_ending)?;
    let bytes = encode_text(&normalized, encoding, allow_lossy)?;
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

/// Apply EditorConfig save-only text rewrites before encoding and line-ending normalization.
///
/// This is pure string processing and performs no filesystem work, so it can be
/// unit-tested separately from the atomic write path. The returned text is still
/// normalized to the active save line ending later in `write_document_to_path`.
#[must_use]
pub fn apply_save_formatting_overrides(text: &str, overrides: FormattingOverrides) -> String {
    let mut formatted = if overrides.trim_trailing_whitespace == Some(true) {
        trim_trailing_space_and_tabs(text)
    } else {
        text.to_string()
    };

    match overrides.insert_final_newline {
        Some(true) if !formatted.is_empty() && !formatted.ends_with(['\n', '\r']) => {
            formatted.push('\n');
        }
        Some(false) => {
            while formatted.ends_with(['\n', '\r']) {
                formatted.pop();
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
        DocumentEncoding::Utf8 | DocumentEncoding::Utf8Bom
    ) {
        return None;
    }

    let mut issues = Vec::new();
    let mut total_issue_count = 0usize;
    let mut line = 1usize;
    let mut column = 1usize;

    for character in text.chars() {
        let char_text = character.to_string();
        let (_, _, had_errors) = target_encoding.codec().encode(&char_text);
        if had_errors {
            total_issue_count += 1;
            if issues.len() < MAX_LOSSY_PREVIEW_ISSUES {
                issues.push(LossyEncodingIssue {
                    line,
                    column,
                    character,
                });
            }
        }

        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
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

/// Decode raw bytes into Unicode text and document metadata.
fn decode_document(bytes: &[u8], reopen_as: Option<DocumentEncoding>) -> DecodedDocument {
    let (content, opened_encoding, decode_confidence, has_bom) = if let Some(encoding) = reopen_as {
        let (decoded, had_bom) = decode_with_encoding(bytes, encoding);
        (decoded, encoding, DecodeConfidence::Exact, had_bom)
    } else if let Some((encoding, stripped)) = bom_prefixed_encoding(bytes) {
        let content = decode_bytes_without_bom(stripped, encoding);
        (content, encoding, DecodeConfidence::Exact, true)
    } else if let Ok(utf8) = simdutf8::basic::from_utf8(bytes) {
        (
            utf8.to_string(),
            DocumentEncoding::Utf8,
            DecodeConfidence::Exact,
            false,
        )
    } else if let Some(encoding) = guess_utf16_without_bom(bytes) {
        let content = decode_bytes_without_bom(bytes, encoding);
        (content, encoding, DecodeConfidence::Heuristic, false)
    } else {
        (
            decode_bytes_without_bom(bytes, DocumentEncoding::Windows1252),
            DocumentEncoding::Windows1252,
            DecodeConfidence::Low,
            false,
        )
    };

    let (detected_line_ending, suggested_line_ending) = detect_line_endings(&content);
    let encoding_state = DocumentEncodingState {
        opened_encoding,
        save_encoding: opened_encoding,
        detected_line_ending,
        save_line_ending: suggested_line_ending,
        decode_confidence,
    };

    DecodedDocument {
        content,
        encoding_state,
        has_bom,
    }
}

/// Decode bytes using an explicit encoding selection, stripping any matching BOM.
fn decode_with_encoding(bytes: &[u8], encoding: DocumentEncoding) -> (String, bool) {
    if let Some((detected_encoding, stripped)) = bom_prefixed_encoding(bytes)
        && detected_encoding == encoding
    {
        return (decode_bytes_without_bom(stripped, encoding), true);
    }

    (decode_bytes_without_bom(bytes, encoding), false)
}

/// Decode bytes with the requested encoding after BOM handling has been resolved.
fn decode_bytes_without_bom(bytes: &[u8], encoding: DocumentEncoding) -> String {
    match encoding {
        DocumentEncoding::Utf8 | DocumentEncoding::Utf8Bom
            if let Ok(utf8) = simdutf8::basic::from_utf8(bytes) =>
        {
            utf8.to_string()
        }
        _ => {
            let (decoded, _) = encoding.codec().decode_without_bom_handling(bytes);
            decoded.into_owned()
        }
    }
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
    let pair_count = bytes.len() / 2;

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

/// Detect line-ending style from decoded text and choose a safe save default.
///
/// The input must already be decoded text. CR/LF candidate discovery uses the
/// established byte-search dependency, while every candidate remains an ASCII
/// byte and therefore cannot split a UTF-8 scalar.
#[must_use]
pub fn detect_line_endings(text: &str) -> (LineEnding, LineEnding) {
    let bytes = text.as_bytes();
    let mut crlf_count = 0usize;
    let mut lf_count = 0usize;
    let mut cr_count = 0usize;
    let mut paired_lf = None;

    for index in memchr::memchr2_iter(b'\r', b'\n', bytes) {
        if paired_lf == Some(index) {
            paired_lf = None;
            continue;
        }
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                crlf_count += 1;
                paired_lf = Some(index + 1);
            }
            b'\r' => {
                cr_count += 1;
            }
            b'\n' => {
                lf_count += 1;
            }
            _ => unreachable!("memchr2_iter yields only CR or LF candidates"),
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

/// Build surfaced file-health findings from the decoded document snapshot.
fn build_file_health(
    content: &str,
    decoded: &DecodedDocument,
    raw_bytes: &[u8],
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

    if raw_bytes.contains(&0)
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

    let nbsp_count = content
        .chars()
        .filter(|&character| character == '\u{00A0}')
        .count();
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

    let zero_width_count = content
        .chars()
        .filter(|&character| is_zero_width(character))
        .count();
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
fn normalize_line_endings(text: &str, line_ending: LineEnding) -> Result<String, EditorSaveError> {
    let Some(separator) = line_ending.separator() else {
        return Err(EditorSaveError::MixedLineEndings);
    };

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
    Ok(normalized)
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
) -> Result<Vec<u8>, EditorSaveError> {
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
            Cow::Owned([vec![0xEF, 0xBB, 0xBF], text.as_bytes().to_vec()].concat())
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

    Ok(bytes.into_owned())
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
    let identity =
        fs_write::resolve_target_identity(path).map_err(|source| EditorSaveError::WriteTemp {
            path: path.to_path_buf(),
            source,
        })?;
    let write_path = identity.as_path().to_path_buf();
    let _path_lock = fs_write::TargetWriteGuard::from_identity(identity);
    fs_write::atomic_replace(&write_path, WriteLabel::SAVE, bytes)
        .map_err(|error| save_error_from_durable(error, path))
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
    fn load_text_file_reads_utf8_and_classifies_size() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(file.path(), "hello\nworld");

        let cancel = AtomicBool::new(false);
        let result = load_text_file(file.path(), &cancel).expect("expected operation to succeed");

        assert_eq!(result.content, "hello\nworld");
        assert_eq!(result.size, 11);
        assert_eq!(result.size_check, FileSizeCheck::Normal);
        assert_eq!(
            result.encoding_state.opened_encoding,
            DocumentEncoding::Utf8
        );
        assert_eq!(result.encoding_state.detected_line_ending, LineEnding::Lf);
        assert!(
            result.mtime.is_some(),
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
            result.encoding_state.opened_encoding,
            DocumentEncoding::Utf8Bom
        );
        assert_eq!(result.encoding_state.detected_line_ending, LineEnding::Crlf);
        assert!(result.has_bom);
        assert!(
            result
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
        assert!(matching.has_bom);

        let plain_utf8 =
            load_text_file_with_encoding(file.path(), &cancel, Some(DocumentEncoding::Utf8))
                .expect("expected operation to succeed");
        assert_eq!(plain_utf8.content, "\u{feff}a");
        assert!(!plain_utf8.has_bom);
    }

    #[test]
    fn load_text_file_decodes_windows_1252_when_utf8_fails() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        fixture::write_bytes(file.path(), [0x63, 0x61, 0x66, 0xE9]);

        let cancel = AtomicBool::new(false);
        let result = load_text_file(file.path(), &cancel).expect("expected operation to succeed");

        assert_eq!(result.content, "café");
        assert_eq!(
            result.encoding_state.opened_encoding,
            DocumentEncoding::Windows1252
        );
        assert_eq!(
            result.encoding_state.decode_confidence,
            DecodeConfidence::Low
        );
        assert!(
            result
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
            le_result.encoding_state.opened_encoding,
            DocumentEncoding::Utf16Le
        );
        assert_eq!(
            le_result.encoding_state.decode_confidence,
            DecodeConfidence::Heuristic
        );
        assert_eq!(be_result.content, "Hé\n");
        assert_eq!(
            be_result.encoding_state.opened_encoding,
            DocumentEncoding::Utf16Be
        );
        assert_eq!(
            be_result.encoding_state.decode_confidence,
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
            short_result.encoding_state.opened_encoding,
            DocumentEncoding::Windows1252
        );
        assert_eq!(
            odd_result.encoding_state.opened_encoding,
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
            result.encoding_state.detected_line_ending,
            LineEnding::Mixed
        );
        assert!(
            result
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
            result.encoding_state.opened_encoding,
            DocumentEncoding::Utf16Le
        );
        assert!(
            !result
                .file_health
                .iter()
                .any(|finding| finding.kind == FileHealthFindingKind::BinaryLikeContent)
        );
        assert!(
            !result
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
            .file_health
            .iter()
            .find(|finding| finding.kind == FileHealthFindingKind::NonBreakingSpace)
            .expect("NBSP finding should be present");
        let zero_width = result
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
                .file_health
                .iter()
                .any(|finding| finding.kind == FileHealthFindingKind::NonBreakingSpace)
        );
        assert!(
            !result
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
            result.encoding_state.opened_encoding,
            DocumentEncoding::ShiftJis
        );
        assert_eq!(
            result.encoding_state.decode_confidence,
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
