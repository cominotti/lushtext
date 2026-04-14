// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded, read-only file snapshots for sidebar file peek.
//!
//! This service owns the filesystem-facing part of file peek: metadata reads,
//! size-policy alignment, bounded text sampling, UTF-8 checks, and explicit
//! fallback classification. It stays GTK-free so the sidebar adapter can render
//! the result without accidentally creating editor, draft, or monitor state.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::file_limits::FileSizeCheck;

/// Maximum bytes read for one preview sample.
///
/// 16 KB is enough to identify most text files while keeping rapid Up/Down
/// navigation well below the cost of loading a full editor tab.
pub const PEEK_SAMPLE_BYTE_LIMIT: usize = 16 * 1024;

/// Maximum number of rendered lines in one preview sample.
///
/// Capping line count keeps minified files or giant single-prefix headers from
/// turning the popover into a long scroll target.
pub const PEEK_SAMPLE_LINE_LIMIT: usize = 60;

/// Monotonic request token used to drop stale async peek completions.
///
/// The sidebar stores the latest token in its widget state. Any completion that
/// arrives with an older token is ignored, which keeps fast selection changes
/// from reviving an outdated preview.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PeekRequestToken(u32);

impl PeekRequestToken {
    /// Return the zero token used before the first request is started.
    #[must_use]
    pub const fn initial() -> Self {
        Self(0)
    }

    /// Advance to the next request token, wrapping on overflow.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// Whether `self` still matches the currently active request token.
    #[must_use]
    pub const fn matches(self, active: Self) -> bool {
        self.0 == active.0
    }
}

/// UI-facing classification for one resolved preview snapshot.
///
/// The sidebar renders one of these states directly instead of inferring
/// behavior from raw I/O errors or byte-level heuristics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeekPreviewState {
    /// A request was started but the bounded filesystem read has not finished yet.
    Loading,
    /// A bounded UTF-8 text sample is available for rendering.
    Text,
    /// The bytes are not suitable for inline text preview.
    BinaryOrUnsupported,
    /// Metadata or file reads failed, usually because the path is gone or unreadable.
    Unreadable,
    /// The existing large-file policy would refuse normal open, so peek refuses too.
    TooLargeToOpen,
}

/// Resolved snapshot payload rendered by the sidebar peek card.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeekSnapshot {
    /// Absolute path of the preview target on disk.
    pub absolute_path: PathBuf,
    /// Basename shown as the title of the preview card.
    pub display_name: String,
    /// Absolute path shown under the title.
    pub display_path: String,
    /// File size read from metadata.
    pub byte_size: u64,
    /// Last modification time from the same metadata stat, if available.
    pub modified_at_secs: Option<u64>,
    /// Resolved rendering state for the preview body.
    pub preview_state: PeekPreviewState,
    /// Read-only text sample. Present only for `PeekPreviewState::Text`.
    pub sample_text: Option<String>,
    /// Number of lines rendered in `sample_text`.
    pub sample_line_count: usize,
    /// Whether the service intentionally clipped the sample.
    pub truncated: bool,
    /// Whether the existing normal open workflow should still be offered.
    pub open_allowed: bool,
}

impl PeekSnapshot {
    /// Build the immediate loading-state payload rendered before I/O completes.
    #[must_use]
    pub fn loading(path: &Path, display_path: impl Into<String>) -> Self {
        Self {
            absolute_path: path.to_path_buf(),
            display_name: display_name_for_path(path),
            display_path: display_path.into(),
            byte_size: 0,
            modified_at_secs: None,
            preview_state: PeekPreviewState::Loading,
            sample_text: None,
            sample_line_count: 0,
            truncated: false,
            open_allowed: false,
        }
    }
}

/// Load one bounded preview snapshot from disk.
///
/// This function performs blocking I/O and must run on a background thread via
/// `spawn_blocking_then`. It never mutates app state; every unsupported or
/// failing case becomes an explicit fallback snapshot.
#[must_use]
pub fn load_snapshot(path: &Path, display_path: impl Into<String>) -> PeekSnapshot {
    let display_path = display_path.into();
    let display_name = display_name_for_path(path);

    let Ok(meta) = std::fs::metadata(path) else {
        return unresolved_snapshot(
            path,
            display_name,
            display_path,
            PeekPreviewState::Unreadable,
            0,
            None,
            false,
        );
    };

    let byte_size = meta.len();
    let modified_at_secs = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    let size_check = FileSizeCheck::classify(byte_size);

    if !size_check.open_allowed() {
        return unresolved_snapshot(
            path,
            display_name,
            display_path,
            PeekPreviewState::TooLargeToOpen,
            byte_size,
            modified_at_secs,
            false,
        );
    }

    let Ok(sample) = read_bounded_bytes(path, PEEK_SAMPLE_BYTE_LIMIT) else {
        return unresolved_snapshot(
            path,
            display_name,
            display_path,
            PeekPreviewState::Unreadable,
            byte_size,
            modified_at_secs,
            false,
        );
    };

    if sample.bytes.contains(&0) {
        return unresolved_snapshot(
            path,
            display_name,
            display_path,
            PeekPreviewState::BinaryOrUnsupported,
            byte_size,
            modified_at_secs,
            false,
        );
    }

    let Ok(text) = simdutf8::basic::from_utf8(&sample.bytes) else {
        return unresolved_snapshot(
            path,
            display_name,
            display_path,
            PeekPreviewState::BinaryOrUnsupported,
            byte_size,
            modified_at_secs,
            false,
        );
    };

    let (sample_text, sample_line_count, truncated) =
        truncate_preview_text(text, sample.truncated_by_bytes);

    PeekSnapshot {
        absolute_path: path.to_path_buf(),
        display_name,
        display_path,
        byte_size,
        modified_at_secs,
        preview_state: PeekPreviewState::Text,
        sample_text: Some(sample_text),
        sample_line_count,
        truncated,
        open_allowed: true,
    }
}

/// Read at most `byte_limit + 1` bytes so the caller can tell whether the
/// preview was clipped by the byte budget.
fn read_bounded_bytes(path: &Path, byte_limit: usize) -> std::io::Result<BoundedBytes> {
    let file = File::open(path)?;
    let mut bytes = Vec::with_capacity(byte_limit.saturating_add(1));
    let mut take = file.take((byte_limit as u64).saturating_add(1));
    take.read_to_end(&mut bytes)?;
    let truncated_by_bytes = bytes.len() > byte_limit;
    if truncated_by_bytes {
        bytes.truncate(byte_limit);
    }
    Ok(BoundedBytes {
        bytes,
        truncated_by_bytes,
    })
}

/// Clip preview text by line count while preserving whether the byte cap
/// already forced truncation.
fn truncate_preview_text(text: &str, truncated_by_bytes: bool) -> (String, usize, bool) {
    let mut rendered = Vec::new();
    let mut lines = text.lines();
    for _ in 0..PEEK_SAMPLE_LINE_LIMIT {
        let Some(line) = lines.next() else {
            break;
        };
        rendered.push(line);
    }

    let sample_line_count = rendered.len();
    let truncated_by_lines = lines.next().is_some();
    let mut sample_text = rendered.join("\n");

    if text.ends_with('\n')
        && !sample_text.is_empty()
        && !sample_text.ends_with('\n')
        && !truncated_by_lines
        && !truncated_by_bytes
    {
        sample_text.push('\n');
    }

    (
        sample_text,
        sample_line_count,
        truncated_by_bytes || truncated_by_lines,
    )
}

/// Build a non-text or error snapshot while preserving file metadata.
fn unresolved_snapshot(
    path: &Path,
    display_name: String,
    display_path: String,
    preview_state: PeekPreviewState,
    byte_size: u64,
    modified_at_secs: Option<u64>,
    open_allowed: bool,
) -> PeekSnapshot {
    PeekSnapshot {
        absolute_path: path.to_path_buf(),
        display_name,
        display_path,
        byte_size,
        modified_at_secs,
        preview_state,
        sample_text: None,
        sample_line_count: 0,
        truncated: false,
        open_allowed,
    }
}

/// Lightweight bounded-read result used only inside this module.
struct BoundedBytes {
    bytes: Vec<u8>,
    truncated_by_bytes: bool,
}

/// Return the file-system display name used in peek headers.
fn display_name_for_path(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn request_token_advances_and_matches() {
        let initial = PeekRequestToken::initial();
        let next = initial.next();
        let later = next.next();

        assert!(initial.matches(PeekRequestToken::initial()));
        assert!(next.matches(next));
        assert!(!next.matches(initial));
        assert!(!next.matches(later));
    }

    #[test]
    fn load_snapshot_returns_text_preview_for_utf8_file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "alpha\nbeta\n").unwrap();

        let display_path = file.path().display().to_string();
        let snapshot = load_snapshot(file.path(), display_path.clone());

        assert_eq!(snapshot.preview_state, PeekPreviewState::Text);
        assert_eq!(
            snapshot.display_name,
            file.path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        );
        assert_eq!(snapshot.display_path, display_path);
        assert_eq!(snapshot.sample_text.as_deref(), Some("alpha\nbeta\n"));
        assert_eq!(snapshot.sample_line_count, 2);
        assert!(snapshot.open_allowed);
        assert!(!snapshot.truncated);
    }

    #[test]
    fn load_snapshot_marks_byte_truncation() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut content = String::new();
        while content.len() <= PEEK_SAMPLE_BYTE_LIMIT.saturating_add(20) {
            content.push_str("abcdefghijklmnopqrstuvwxyz\n");
        }
        std::fs::write(file.path(), content).unwrap();

        let snapshot = load_snapshot(file.path(), "long.txt");

        assert_eq!(snapshot.preview_state, PeekPreviewState::Text);
        assert!(snapshot.truncated);
        assert!(
            snapshot
                .sample_text
                .as_ref()
                .is_some_and(|text| text.len() <= PEEK_SAMPLE_BYTE_LIMIT)
        );
    }

    #[test]
    fn load_snapshot_marks_line_truncation() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut content = String::new();
        for line in 0..(PEEK_SAMPLE_LINE_LIMIT + 5) {
            content.push_str(&format!("line-{line}\n"));
        }
        std::fs::write(file.path(), content).unwrap();

        let snapshot = load_snapshot(file.path(), "lines.txt");

        assert_eq!(snapshot.preview_state, PeekPreviewState::Text);
        assert!(snapshot.truncated);
        assert_eq!(snapshot.sample_line_count, PEEK_SAMPLE_LINE_LIMIT);
        assert_eq!(
            snapshot.sample_text.unwrap().lines().count(),
            PEEK_SAMPLE_LINE_LIMIT
        );
    }

    #[test]
    fn load_snapshot_rejects_invalid_utf8_as_unsupported() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), [0xff, 0xfe, 0xfd]).unwrap();

        let snapshot = load_snapshot(file.path(), "binary.bin");

        assert_eq!(
            snapshot.preview_state,
            PeekPreviewState::BinaryOrUnsupported
        );
        assert!(!snapshot.open_allowed);
        assert!(snapshot.sample_text.is_none());
    }

    #[test]
    fn load_snapshot_rejects_nul_bytes_as_unsupported() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"abc\0def").unwrap();

        let snapshot = load_snapshot(file.path(), "nul.bin");

        assert_eq!(
            snapshot.preview_state,
            PeekPreviewState::BinaryOrUnsupported
        );
        assert!(!snapshot.open_allowed);
    }

    #[test]
    fn load_snapshot_reports_unreadable_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.txt");

        let snapshot = load_snapshot(&path, "missing.txt");

        assert_eq!(snapshot.preview_state, PeekPreviewState::Unreadable);
        assert!(!snapshot.open_allowed);
    }

    #[test]
    fn load_snapshot_reports_too_large_without_reading_contents() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut writer = std::fs::OpenOptions::new()
            .write(true)
            .open(file.path())
            .unwrap();
        writer.write_all(b"x").unwrap();
        writer.flush().unwrap();
        writer
            .set_len(super::super::file_limits::REFUSE_TO_OPEN + 1)
            .unwrap();

        let snapshot = load_snapshot(file.path(), "huge.txt");

        assert_eq!(snapshot.preview_state, PeekPreviewState::TooLargeToOpen);
        assert!(!snapshot.open_allowed);
        assert_eq!(
            snapshot.byte_size,
            super::super::file_limits::REFUSE_TO_OPEN + 1
        );
    }
}
