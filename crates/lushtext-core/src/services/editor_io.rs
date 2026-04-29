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
use std::borrow::Cow;
use std::fmt::Write as _;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Maximum number of concrete lossy characters included in one preview.
///
/// The preview exists to explain the risk, not to render a full diff for a
/// large document, so the sample stays intentionally small and fast to inspect.
const MAX_LOSSY_PREVIEW_ISSUES: usize = 8;

/// Successful result from `load_text_file`.
pub struct LoadResult {
    pub content: String,
    pub size: u64,
    pub size_check: FileSizeCheck,
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
pub enum LoadError {
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

/// Errors that can occur when saving a file.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("No file path set")]
    NoPath,
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
    #[error("Failed to finalize {to} from {from}: {source}")]
    Finalize {
        from: PathBuf,
        to: PathBuf,
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
pub fn load_text_file(path: &Path, cancel: &AtomicBool) -> Result<LoadResult, LoadError> {
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
) -> Result<LoadResult, LoadError> {
    if cancel.load(Ordering::Acquire) {
        return Err(LoadError::Cancelled);
    }

    let meta = std::fs::metadata(path).map_err(|source| LoadError::Metadata {
        path: path.to_path_buf(),
        source,
    })?;
    let size = meta.len();
    let size_check = FileSizeCheck::classify(size);
    let mtime = mtime_from_metadata(&meta);

    if size_check == FileSizeCheck::TooLarge {
        return Err(LoadError::TooLarge {
            path: path.to_path_buf(),
            size_mb: size / 1_000_000,
        });
    }

    if cancel.load(Ordering::Acquire) {
        return Err(LoadError::Cancelled);
    }

    let bytes = std::fs::read(path).map_err(|source| LoadError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if cancel.load(Ordering::Acquire) {
        return Err(LoadError::Cancelled);
    }

    let decoded = decode_document(&bytes, reopen_as);
    let file_health = build_file_health(&decoded.content, &decoded, &bytes);

    Ok(LoadResult {
        content: decoded.content,
        size,
        size_check,
        mtime,
        encoding_state: decoded.encoding_state,
        has_bom: decoded.has_bom,
        file_health,
    })
}

/// Atomically write a UTF-8/LF snapshot to a file using temp-file-then-rename.
///
/// This compatibility wrapper keeps older call sites simple while the editor
/// workflow migrates to the richer save-policy API.
///
/// # Errors
///
/// Returns the same write or normalization errors as `write_document_to_path`.
pub fn write_snapshot_to_path(path: &Path, text: &str) -> Result<(u64, Option<u64>), SaveError> {
    write_document_to_path(path, text, DocumentEncoding::Utf8, LineEnding::Lf, false)
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
) -> Result<(u64, Option<u64>), SaveError> {
    let normalized = normalize_line_endings(text, line_ending)?;
    let bytes = encode_text(&normalized, encoding, allow_lossy)?;
    let bytes_written = bytes.len() as u64;
    write_bytes_to_path(path, &bytes)?;
    let mtime = std::fs::metadata(path)
        .ok()
        .and_then(|metadata| mtime_from_metadata(&metadata));
    Ok((bytes_written, mtime))
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

/// Extract mtime as epoch seconds from already-fetched metadata.
fn mtime_from_metadata(meta: &std::fs::Metadata) -> Option<u64> {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
}

/// Read a file's mtime as seconds since the UNIX epoch.
/// Returns `None` if the file doesn't exist or metadata can't be read.
///
/// **Threading:** Performs a blocking stat syscall.
#[must_use]
pub fn mtime_secs(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()
        .and_then(|metadata| mtime_from_metadata(&metadata))
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
fn detect_line_endings(text: &str) -> (LineEnding, LineEnding) {
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
            _ => {
                index += 1;
            }
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
fn normalize_line_endings(text: &str, line_ending: LineEnding) -> Result<String, SaveError> {
    let Some(separator) = line_ending.separator() else {
        return Err(SaveError::MixedLineEndings);
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
) -> Result<Vec<u8>, SaveError> {
    if !allow_lossy && let Some(preview) = analyze_lossy_encoding(text, encoding) {
        return Err(SaveError::LossyEncoding {
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
fn write_bytes_to_path(path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    let tmp_name = format!(
        ".{}.tmp",
        path.file_name().map_or_else(
            || "untitled".to_string(),
            |name| name.to_string_lossy().into_owned()
        )
    );
    let tmp_path = path.with_file_name(&tmp_name);
    let file = std::fs::File::create(&tmp_path).map_err(|source| SaveError::WriteTemp {
        path: tmp_path.clone(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(bytes)
        .map_err(|source| SaveError::WriteTemp {
            path: tmp_path.clone(),
            source,
        })?;
    writer.flush().map_err(|source| SaveError::WriteTemp {
        path: tmp_path.clone(),
        source,
    })?;
    writer
        .get_ref()
        .sync_all()
        .map_err(|source| SaveError::WriteTemp {
            path: tmp_path.clone(),
            source,
        })?;
    std::fs::rename(&tmp_path, path).map_err(|source| {
        let _ = std::fs::remove_file(&tmp_path);
        SaveError::Finalize {
            from: tmp_path.clone(),
            to: path.to_path_buf(),
            source,
        }
    })?;
    Ok(())
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
    use std::sync::atomic::AtomicBool;
    use tempfile::NamedTempFile;

    #[test]
    fn load_text_file_reads_utf8_and_classifies_size() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        std::fs::write(file.path(), "hello\nworld").expect("expected operation to succeed");

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
        std::fs::write(file.path(), [0xEF, 0xBB, 0xBF, b'a', b'\r', b'\n'])
            .expect("expected operation to succeed");

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
    fn load_text_file_decodes_windows_1252_when_utf8_fails() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        std::fs::write(file.path(), [0x63, 0x61, 0x66, 0xE9])
            .expect("expected operation to succeed");

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
    fn load_text_file_detects_mixed_line_endings() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        std::fs::write(file.path(), "a\r\nb\nc\r").expect("expected operation to succeed");

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
    fn load_text_file_honors_cancellation() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        std::fs::write(file.path(), "hello").expect("expected operation to succeed");

        let cancel = AtomicBool::new(true);
        let result = load_text_file(file.path(), &cancel);

        assert!(matches!(result, Err(LoadError::Cancelled)));
    }

    #[test]
    fn load_text_file_supports_explicit_reopen_encoding() {
        let file = NamedTempFile::new().expect("expected operation to succeed");
        std::fs::write(file.path(), [0x82, 0xA0]).expect("expected operation to succeed");

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
    fn write_document_to_path_replaces_destination() {
        let dir = tempfile::tempdir().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");

        let (size, mtime) = write_document_to_path(
            &path,
            "saved\ntext",
            DocumentEncoding::Utf8,
            LineEnding::Crlf,
            false,
        )
        .expect("expected operation to succeed");

        assert_eq!(size, 11);
        assert!(mtime.is_some(), "mtime should be populated after write");
        assert_eq!(
            std::fs::read_to_string(path).expect("expected operation to succeed"),
            "saved\r\ntext"
        );
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

        assert!(matches!(result, Err(SaveError::MixedLineEndings)));
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

        assert!(matches!(result, Err(SaveError::LossyEncoding { .. })));
        assert!(
            !path.exists(),
            "the file should not be written when lossy conversion is blocked"
        );
    }
}
