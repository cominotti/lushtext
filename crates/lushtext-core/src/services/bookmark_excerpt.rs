// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded source excerpts for bookmark previews.
//!
//! The Notes browser uses this GTK-free service to decide what source context
//! can be previewed around a bookmarked line. The UI layer still owns live
//! `GtkTextBuffer` access and rendering, while this module owns plain text,
//! disk-read, size, UTF-8, and budget policy.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::services::filesystem::{metadata, read};
use crate::services::palette::{
    PaletteSearchCoordinator, PaletteSearchCoordinatorSnapshot, PaletteSearchStart,
};

use super::file_limits::FileSizeCheck;

/// Number of source lines shown before the bookmarked line when available.
///
/// Three lines usually provide enough lead-in for prose and code without
/// making each Notes browser selection feel like a full document preview.
pub const BOOKMARK_EXCERPT_CONTEXT_BEFORE_LINES: usize = 3;
/// Number of source lines shown after the bookmarked line when available.
///
/// Seven following lines bias toward "what comes next", which tends to answer
/// why a saved line matters better than a perfectly symmetric window.
pub const BOOKMARK_EXCERPT_CONTEXT_AFTER_LINES: usize = 7;
/// Maximum bytes scanned from disk while trying to reach a bookmarked line.
///
/// One mebibyte keeps preview selection responsive on slow filesystems while
/// still covering ordinary Markdown notes and source files with plenty of room.
pub const BOOKMARK_EXCERPT_SCAN_BYTE_LIMIT: usize = 1024 * 1024;
/// Maximum logical lines scanned from disk for one closed-file preview.
///
/// Deep bookmarks beyond this point are better opened in the editor than
/// resolved through a modal preview, especially for generated or minified files.
pub const BOOKMARK_EXCERPT_SCAN_LINE_LIMIT: usize = 20_000;
/// Maximum characters retained from any one rendered line.
///
/// A single giant line can otherwise consume the whole preview allocation even
/// when the surrounding line window is tiny.
pub const BOOKMARK_EXCERPT_LINE_CHAR_LIMIT: usize = 4096;
/// Logical lines scanned between cooperative cancellation checks.
///
/// Splitting the one-mebibyte excerpt budget into lines is short but not free;
/// checking every 1024 lines keeps supersession responsive without measurable
/// per-line overhead.
pub const BOOKMARK_EXCERPT_CANCELLATION_CHECK_LINES: usize = 1024;

/// How a bookmark excerpt should be presented by the Notes browser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookmarkExcerptPresentation {
    /// Render the excerpt through the Markdown preview surface.
    Markdown,
    /// Render the excerpt as literal source text.
    PlainText,
}

/// Why a bookmark excerpt could not be produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookmarkExcerptUnavailableReason {
    /// Metadata or content reads failed, or the path is no longer a regular file.
    MissingOrUnreadable,
    /// The sampled bytes are not valid UTF-8 text.
    BinaryOrUnsupported,
    /// Existing large-file policy refuses normal open, so preview refuses too.
    TooLargeToPreview,
    /// The target line was not reachable within preview byte or line budgets.
    LineBeyondPreviewBudget,
    /// The target line is beyond the available text.
    LineOutOfRange,
}

/// Explicit state for a bookmark preview request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BookmarkExcerptState {
    /// The UI has requested a closed-file excerpt but background I/O is pending.
    Loading {
        /// Presentation mode inferred before the load starts.
        presentation: BookmarkExcerptPresentation,
    },
    /// A bounded text excerpt is ready to render.
    Ready(BookmarkExcerpt),
    /// The source cannot be previewed safely within the configured policy.
    Unavailable(BookmarkExcerptUnavailable),
}

/// Unavailable bookmark preview payload with its intended presentation mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BookmarkExcerptUnavailable {
    /// Presentation mode inferred from the source path.
    pub presentation: BookmarkExcerptPresentation,
    /// Classified fallback reason shown by the UI.
    pub reason: BookmarkExcerptUnavailableReason,
}

/// Whether the selected line window omits source context.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BookmarkExcerptTruncation {
    /// Lines before the first rendered line exist but were intentionally omitted.
    pub before: bool,
    /// Lines after the final rendered line exist or could not be scanned.
    pub after: bool,
    /// At least one rendered source line was shortened by the per-line budget.
    pub within_line: bool,
}

/// Metadata describing the rendered line window and target-line position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BookmarkExcerptLineWindow {
    /// First source line represented by `lines`, using zero-based editor numbering.
    pub first_line: u32,
    /// Bookmarked source line, using zero-based editor numbering.
    pub target_line: u32,
    /// Index of the bookmarked line within `lines`.
    pub target_line_index: usize,
    /// Context clipping applied while building this window.
    pub truncation: BookmarkExcerptTruncation,
}

/// One logical source line included in a bookmark excerpt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookmarkExcerptLine {
    /// Source line number using zero-based editor numbering.
    pub number: u32,
    /// Text content for this line, without a trailing line break.
    pub text: String,
    /// Whether this line was clipped by `BOOKMARK_EXCERPT_LINE_CHAR_LIMIT`.
    pub truncated: bool,
}

/// Bounded source context around a bookmarked line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookmarkExcerpt {
    /// Rendering mode selected from the bookmarked file path.
    pub presentation: BookmarkExcerptPresentation,
    /// Line-window metadata and target-line offset.
    pub window: BookmarkExcerptLineWindow,
    /// Logical source lines in display order.
    pub lines: Vec<BookmarkExcerptLine>,
}

impl BookmarkExcerpt {
    /// Return the raw excerpt body with no UI-specific line numbers.
    #[must_use]
    pub fn body_text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Return excerpt text with lightweight context markers for Markdown rendering.
    #[must_use]
    pub fn body_text_with_markers(&self) -> String {
        let mut parts = Vec::new();
        if self.window.truncation.before {
            parts.push("... earlier bookmark context omitted ...".to_string());
        }

        parts.push(self.body_text());

        if self.window.truncation.after {
            parts.push("... later bookmark context omitted ...".to_string());
        }
        parts.join("\n\n")
    }
}

/// Cooperative cancellation observed between bounded excerpt work stages.
pub type BookmarkExcerptCancellation = crate::services::single_flight::FlightCancellation;

/// Typed terminal outcome from one cancellable closed-file excerpt load.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BookmarkExcerptLoadOutcome {
    /// The bounded load finished with a renderable or unavailable state.
    Completed(BookmarkExcerptState),
    /// Cooperative cancellation stopped the load before completion.
    Cancelled,
}

/// Compact closed-file bookmark selection retained by the latest preview slot.
///
/// The request deliberately carries no source text and no dialog owner so a
/// superseded selection retains only path-sized state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookmarkExcerptPreviewRequest {
    /// Bookmarked file whose excerpt should be previewed.
    pub path: PathBuf,
    /// Bookmarked line using zero-based editor numbering.
    pub line: u32,
}

/// One request admitted as the sole active closed-file excerpt load.
pub type BookmarkExcerptPreviewStart = PaletteSearchStart<BookmarkExcerptPreviewRequest>;

/// Scalar one-active/one-latest ownership evidence.
pub type BookmarkExcerptPreviewCoordinatorSnapshot = PaletteSearchCoordinatorSnapshot;

/// Serialize excerpt loads while retaining only the latest superseding selection.
pub type BookmarkExcerptPreviewCoordinator =
    PaletteSearchCoordinator<BookmarkExcerptPreviewRequest>;

/// Infer whether a path should render bookmark context as Markdown.
#[must_use]
pub fn presentation_for_path(path: &Path) -> BookmarkExcerptPresentation {
    let markdown_like = path
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| {
            matches!(
                extension.as_str(),
                "md" | "markdown" | "mdown" | "mkd" | "mkdn" | "mdwn"
            )
        });

    if markdown_like {
        BookmarkExcerptPresentation::Markdown
    } else {
        BookmarkExcerptPresentation::PlainText
    }
}

/// Extract an anchored line window from already available UTF-8 text.
#[must_use]
pub fn extract_from_text(
    text: &str,
    target_line: u32,
    presentation: BookmarkExcerptPresentation,
) -> BookmarkExcerptState {
    extract_from_text_with_external_budget(text, target_line, presentation, false)
}

/// Extract an anchored window from a bounded live line slice.
#[must_use]
pub fn extract_from_context_lines(
    presentation: BookmarkExcerptPresentation,
    first_line: u32,
    target_line: u32,
    lines: Vec<String>,
    truncated_before: bool,
    truncated_after: bool,
) -> BookmarkExcerptState {
    let Some(target_line_index) = target_line
        .checked_sub(first_line)
        .and_then(|offset| usize::try_from(offset).ok())
        .filter(|offset| *offset < lines.len())
    else {
        return unavailable(
            presentation,
            BookmarkExcerptUnavailableReason::LineOutOfRange,
        );
    };

    let mut truncated_within_line = false;
    let excerpt_lines = lines
        .into_iter()
        .enumerate()
        .map(|(offset, line)| {
            let (text, truncated) = clip_line(line.trim_end_matches('\r'));
            truncated_within_line |= truncated;
            BookmarkExcerptLine {
                number: first_line.saturating_add(u32::try_from(offset).unwrap_or(u32::MAX)),
                text,
                truncated,
            }
        })
        .collect();

    BookmarkExcerptState::Ready(BookmarkExcerpt {
        presentation,
        window: BookmarkExcerptLineWindow {
            first_line,
            target_line,
            target_line_index,
            truncation: BookmarkExcerptTruncation {
                before: truncated_before,
                after: truncated_after,
                within_line: truncated_within_line,
            },
        },
        lines: excerpt_lines,
    })
}

/// Load a closed-file bookmark excerpt through bounded filesystem reads.
///
/// This performs blocking I/O and must run on a background thread via
/// `spawn_blocking_then`. It never mutates app state; failures become explicit
/// unavailable states so the UI can keep the selected row usable.
#[must_use]
pub fn load_from_path(path: &Path, target_line: u32) -> BookmarkExcerptState {
    match load_from_path_cancellable(path, target_line, &BookmarkExcerptCancellation::default()) {
        BookmarkExcerptLoadOutcome::Completed(state) => state,
        BookmarkExcerptLoadOutcome::Cancelled => {
            unreachable!("a default cancellation token is never cancelled")
        }
    }
}

/// Load a closed-file bookmark excerpt with cooperative cancellation.
///
/// Cancellation is checked around the metadata probe, between bounded read
/// chunks, and at fixed checkpoints while scanning logical lines, so a
/// superseded Notes selection stops obsolete work instead of running to a
/// discarded completion. An uncancelled call is result-equivalent to
/// [`load_from_path`].
#[must_use]
pub fn load_from_path_cancellable(
    path: &Path,
    target_line: u32,
    cancellation: &BookmarkExcerptCancellation,
) -> BookmarkExcerptLoadOutcome {
    let presentation = presentation_for_path(path);
    let refused = |reason: BookmarkExcerptUnavailableReason| {
        BookmarkExcerptLoadOutcome::Completed(unavailable(presentation, reason))
    };

    if cancellation.is_cancelled() {
        return BookmarkExcerptLoadOutcome::Cancelled;
    }
    let Ok(facts) = metadata::file_facts(path) else {
        return refused(BookmarkExcerptUnavailableReason::MissingOrUnreadable);
    };
    if cancellation.is_cancelled() {
        return BookmarkExcerptLoadOutcome::Cancelled;
    }
    if !matches!(facts.kind, crate::services::filesystem::FileKind::File) {
        return refused(BookmarkExcerptUnavailableReason::MissingOrUnreadable);
    }
    if !FileSizeCheck::classify(facts.byte_size).open_allowed() {
        return refused(BookmarkExcerptUnavailableReason::TooLargeToPreview);
    }
    if usize::try_from(target_line)
        .ok()
        .is_some_and(|line| line >= BOOKMARK_EXCERPT_SCAN_LINE_LIMIT)
    {
        return refused(BookmarkExcerptUnavailableReason::LineBeyondPreviewBudget);
    }

    let bytes = match read_bounded_bytes_cancellable(
        path,
        BOOKMARK_EXCERPT_SCAN_BYTE_LIMIT,
        cancellation,
    ) {
        Ok(bytes) => bytes,
        Err(read::BoundedFileReadError::Cancelled) => {
            return BookmarkExcerptLoadOutcome::Cancelled;
        }
        Err(_) => {
            return refused(BookmarkExcerptUnavailableReason::MissingOrUnreadable);
        }
    };
    if bytes.bytes.contains(&0) {
        return refused(BookmarkExcerptUnavailableReason::BinaryOrUnsupported);
    }

    let Ok(text) = validated_utf8_prefix(&bytes.bytes, bytes.truncated_by_bytes) else {
        return refused(BookmarkExcerptUnavailableReason::BinaryOrUnsupported);
    };
    let Some(lines) = logical_lines_cancellable(text, cancellation) else {
        return BookmarkExcerptLoadOutcome::Cancelled;
    };
    BookmarkExcerptLoadOutcome::Completed(extract_from_lines(
        &lines,
        target_line,
        presentation,
        bytes.truncated_by_bytes,
    ))
}

fn extract_from_text_with_external_budget(
    text: &str,
    target_line: u32,
    presentation: BookmarkExcerptPresentation,
    truncated_by_external_budget: bool,
) -> BookmarkExcerptState {
    extract_from_lines(
        &logical_lines(text),
        target_line,
        presentation,
        truncated_by_external_budget,
    )
}

fn extract_from_lines(
    lines: &[&str],
    target_line: u32,
    presentation: BookmarkExcerptPresentation,
    truncated_by_external_budget: bool,
) -> BookmarkExcerptState {
    let Some(target_index) = usize::try_from(target_line).ok() else {
        return unavailable(
            presentation,
            BookmarkExcerptUnavailableReason::LineOutOfRange,
        );
    };
    if target_index >= lines.len() {
        return unavailable(
            presentation,
            if truncated_by_external_budget {
                BookmarkExcerptUnavailableReason::LineBeyondPreviewBudget
            } else {
                BookmarkExcerptUnavailableReason::LineOutOfRange
            },
        );
    }

    let start = target_index.saturating_sub(BOOKMARK_EXCERPT_CONTEXT_BEFORE_LINES);
    let end = target_index
        .saturating_add(BOOKMARK_EXCERPT_CONTEXT_AFTER_LINES)
        .min(lines.len().saturating_sub(1));
    let context_lines = lines[start..=end]
        .iter()
        .map(|line| (*line).to_string())
        .collect();

    extract_from_context_lines(
        presentation,
        u32::try_from(start).unwrap_or(u32::MAX),
        target_line,
        context_lines,
        start > 0,
        end + 1 < lines.len() || truncated_by_external_budget,
    )
}

fn unavailable(
    presentation: BookmarkExcerptPresentation,
    reason: BookmarkExcerptUnavailableReason,
) -> BookmarkExcerptState {
    BookmarkExcerptState::Unavailable(BookmarkExcerptUnavailable {
        presentation,
        reason,
    })
}

fn logical_lines(text: &str) -> Vec<&str> {
    logical_lines_cancellable(text, &BookmarkExcerptCancellation::default())
        .expect("a default cancellation token is never cancelled")
}

/// Collect logical lines with periodic cancellation checkpoints.
///
/// Returns `None` when cancellation is observed at a checkpoint; an uncancelled
/// call collects exactly the same lines as [`logical_lines`].
fn logical_lines_cancellable<'text>(
    text: &'text str,
    cancellation: &BookmarkExcerptCancellation,
) -> Option<Vec<&'text str>> {
    if text.is_empty() {
        return Some(vec![""]);
    }
    let mut lines = Vec::new();
    for (index, line) in text
        .split('\n')
        .take(BOOKMARK_EXCERPT_SCAN_LINE_LIMIT)
        .enumerate()
    {
        if index % BOOKMARK_EXCERPT_CANCELLATION_CHECK_LINES == 0 && cancellation.is_cancelled() {
            return None;
        }
        lines.push(line);
    }
    Some(lines)
}

fn clip_line(line: &str) -> (String, bool) {
    let mut chars = line.chars();
    let clipped: String = chars
        .by_ref()
        .take(BOOKMARK_EXCERPT_LINE_CHAR_LIMIT)
        .collect();
    if chars.next().is_some() {
        (format!("{clipped} ..."), true)
    } else {
        (clipped, false)
    }
}

fn read_bounded_bytes_cancellable(
    path: &Path,
    byte_limit: usize,
    cancellation: &BookmarkExcerptCancellation,
) -> Result<BoundedBytes, read::BoundedFileReadError> {
    let mut bytes = read::prefix_bytes_cancellable(path, byte_limit.saturating_add(1), || {
        cancellation.is_cancelled()
    })?;
    let truncated_by_bytes = bytes.len() > byte_limit;
    if truncated_by_bytes {
        bytes.truncate(byte_limit);
    }
    Ok(BoundedBytes {
        bytes,
        truncated_by_bytes,
    })
}

fn validated_utf8_prefix(
    bytes: &[u8],
    truncated_by_bytes: bool,
) -> Result<&str, simdutf8::compat::Utf8Error> {
    match simdutf8::compat::from_utf8(bytes) {
        Ok(text) => Ok(text),
        Err(error) if truncated_by_bytes && error.error_len().is_none() => {
            simdutf8::compat::from_utf8(&bytes[..error.valid_up_to()])
        }
        Err(error) => Err(error),
    }
}

struct BoundedBytes {
    bytes: Vec<u8>,
    truncated_by_bytes: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;

    fn numbered_text(count: usize) -> String {
        (0..count)
            .map(|line| format!("line-{line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn ready(state: BookmarkExcerptState) -> BookmarkExcerpt {
        match state {
            BookmarkExcerptState::Ready(excerpt) => excerpt,
            other => panic!("expected ready excerpt, got {other:?}"),
        }
    }

    fn unavailable_reason(state: BookmarkExcerptState) -> BookmarkExcerptUnavailableReason {
        match state {
            BookmarkExcerptState::Unavailable(unavailable) => unavailable.reason,
            other => panic!("expected unavailable excerpt, got {other:?}"),
        }
    }

    #[test]
    fn bookmark_excerpt_policy_constants_are_stable() {
        assert_eq!(BOOKMARK_EXCERPT_CONTEXT_BEFORE_LINES, 3);
        assert_eq!(BOOKMARK_EXCERPT_CONTEXT_AFTER_LINES, 7);
        assert_eq!(BOOKMARK_EXCERPT_SCAN_BYTE_LIMIT, 1024 * 1024);
        assert_eq!(BOOKMARK_EXCERPT_SCAN_LINE_LIMIT, 20_000);
        assert_eq!(BOOKMARK_EXCERPT_LINE_CHAR_LIMIT, 4096);
        assert_eq!(BOOKMARK_EXCERPT_CANCELLATION_CHECK_LINES, 1024);
    }

    #[test]
    fn load_from_path_cancellable_short_circuits_before_metadata() {
        let cancellation = BookmarkExcerptCancellation::default();
        assert!(cancellation.cancel());
        assert!(
            !cancellation.cancel(),
            "only the first cancel transition reports success"
        );

        let outcome = load_from_path_cancellable(
            Path::new("/nonexistent/never-touched.md"),
            0,
            &cancellation,
        );

        assert_eq!(outcome, BookmarkExcerptLoadOutcome::Cancelled);
    }

    #[test]
    fn load_from_path_cancellable_matches_uncancelled_reference() {
        let dir = tempfile::tempdir().expect("temp dir");
        let ready = dir.path().join("ready.md");
        fixture::write_text(&ready, "alpha\nbravo\ncharlie\n");
        let missing = dir.path().join("missing.md");
        let binary = dir.path().join("binary.rs");
        fixture::write_bytes(&binary, b"fn\0main");

        for (path, line) in [(&ready, 1u32), (&missing, 0), (&binary, 0)] {
            let reference = load_from_path(path, line);
            let outcome =
                load_from_path_cancellable(path, line, &BookmarkExcerptCancellation::default());
            assert_eq!(outcome, BookmarkExcerptLoadOutcome::Completed(reference));
        }
    }

    #[test]
    fn logical_line_scan_checkpoints_observe_cancellation_and_match_reference() {
        let text = numbered_text(BOOKMARK_EXCERPT_CANCELLATION_CHECK_LINES * 2);
        let cancellation = BookmarkExcerptCancellation::default();

        assert_eq!(
            logical_lines_cancellable(&text, &cancellation).expect("uncancelled scan completes"),
            logical_lines(&text)
        );

        let _ = cancellation.cancel();
        assert_eq!(
            logical_lines_cancellable(&text, &cancellation),
            None,
            "a cancelled token must stop the scan at the next checkpoint"
        );
    }

    #[test]
    fn cancellable_prefix_read_stops_between_chunks_without_retaining_bytes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("large.txt");
        fixture::write_repeated_bytes(&file, b"x", 256 * 1024);

        let mut checks = 0usize;
        let result = read::prefix_bytes_cancellable(&file, 256 * 1024, || {
            checks += 1;
            checks > 2
        });

        assert!(matches!(result, Err(read::BoundedFileReadError::Cancelled)));
        assert_eq!(checks, 3, "cancellation is polled once per streamed chunk");

        let uncancelled = read::prefix_bytes_cancellable(&file, 300 * 1024, || false)
            .expect("uncancelled prefix read");
        assert_eq!(
            uncancelled,
            read::prefix_bytes(&file, 300 * 1024).expect("reference prefix read")
        );
    }

    #[test]
    fn extracts_window_at_file_start() {
        let excerpt = ready(extract_from_text(
            &numbered_text(6),
            0,
            BookmarkExcerptPresentation::PlainText,
        ));

        assert_eq!(excerpt.window.first_line, 0);
        assert_eq!(excerpt.window.target_line_index, 0);
        assert!(!excerpt.window.truncation.before);
        assert!(!excerpt.window.truncation.after);
        assert_eq!(
            excerpt
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["line-0", "line-1", "line-2", "line-3", "line-4", "line-5"]
        );
    }

    #[test]
    fn extracts_window_from_middle_with_target_index() {
        let excerpt = ready(extract_from_text(
            &numbered_text(30),
            12,
            BookmarkExcerptPresentation::PlainText,
        ));

        assert_eq!(excerpt.window.first_line, 9);
        assert_eq!(excerpt.window.target_line, 12);
        assert_eq!(excerpt.window.target_line_index, 3);
        assert!(excerpt.window.truncation.before);
        assert!(excerpt.window.truncation.after);
        assert_eq!(excerpt.lines.len(), 11);
        assert_eq!(excerpt.lines[3].text, "line-12");
    }

    #[test]
    fn extracts_window_at_file_end() {
        let excerpt = ready(extract_from_text(
            &numbered_text(12),
            11,
            BookmarkExcerptPresentation::PlainText,
        ));

        assert_eq!(excerpt.window.first_line, 8);
        assert_eq!(excerpt.window.target_line_index, 3);
        assert!(excerpt.window.truncation.before);
        assert!(!excerpt.window.truncation.after);
        assert_eq!(excerpt.lines.last().expect("last line").text, "line-11");
    }

    #[test]
    fn context_lines_preserve_external_truncation_metadata() {
        let excerpt = ready(extract_from_context_lines(
            BookmarkExcerptPresentation::Markdown,
            4,
            5,
            vec!["before".to_string(), "target".to_string()],
            true,
            true,
        ));

        assert_eq!(excerpt.presentation, BookmarkExcerptPresentation::Markdown);
        assert_eq!(excerpt.window.target_line_index, 1);
        assert!(excerpt.window.truncation.before);
        assert!(excerpt.window.truncation.after);
        assert_eq!(
            excerpt.body_text_with_markers(),
            "... earlier bookmark context omitted ...\n\nbefore\ntarget\n\n... later bookmark context omitted ..."
        );
    }

    #[test]
    fn context_lines_reject_target_at_end_boundary() {
        assert_eq!(
            unavailable_reason(extract_from_context_lines(
                BookmarkExcerptPresentation::PlainText,
                4,
                6,
                vec!["line-4".to_string(), "line-5".to_string()],
                false,
                false,
            )),
            BookmarkExcerptUnavailableReason::LineOutOfRange
        );
    }

    #[test]
    fn clips_very_long_lines_and_marks_within_line_truncation() {
        let long = "x".repeat(BOOKMARK_EXCERPT_LINE_CHAR_LIMIT + 10);
        let excerpt = ready(extract_from_text(
            &long,
            0,
            BookmarkExcerptPresentation::PlainText,
        ));

        assert!(excerpt.window.truncation.within_line);
        assert!(excerpt.lines[0].truncated);
        assert!(excerpt.lines[0].text.ends_with(" ..."));
    }

    #[test]
    fn classifies_markdown_like_extensions() {
        for extension in ["md", "markdown", "mdown", "mkd", "mkdn", "mdwn"] {
            let path = std::path::PathBuf::from(format!("note.{extension}"));
            assert_eq!(
                presentation_for_path(&path),
                BookmarkExcerptPresentation::Markdown
            );
        }
        assert_eq!(
            presentation_for_path(Path::new("source.rs")),
            BookmarkExcerptPresentation::PlainText
        );
    }

    #[test]
    fn load_from_path_returns_text_for_utf8_file() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        fixture::write_text(file.path(), "one\ntwo\nthree\nfour\n");

        let excerpt = ready(load_from_path(file.path(), 1));

        assert_eq!(excerpt.window.target_line_index, 1);
        assert_eq!(excerpt.lines[1].text, "two");
        assert_eq!(excerpt.presentation, BookmarkExcerptPresentation::PlainText);
    }

    #[test]
    fn load_from_path_reports_missing_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("missing.md");

        assert_eq!(
            unavailable_reason(load_from_path(&path, 0)),
            BookmarkExcerptUnavailableReason::MissingOrUnreadable
        );
    }

    #[test]
    fn load_from_path_reports_invalid_utf8_as_unsupported() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        fixture::write_bytes(file.path(), [0xff, 0xfe, 0xfd]);

        assert_eq!(
            unavailable_reason(load_from_path(file.path(), 0)),
            BookmarkExcerptUnavailableReason::BinaryOrUnsupported
        );
    }

    #[test]
    fn load_from_path_reports_nul_bytes_as_unsupported() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        fixture::write_bytes(file.path(), b"abc\0def");

        assert_eq!(
            unavailable_reason(load_from_path(file.path(), 0)),
            BookmarkExcerptUnavailableReason::BinaryOrUnsupported
        );
    }

    #[test]
    fn load_from_path_reports_too_large_without_reading_contents() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        fixture::create_sparse_file(file.path(), super::super::file_limits::REFUSE_TO_OPEN + 1);

        assert_eq!(
            unavailable_reason(load_from_path(file.path(), 0)),
            BookmarkExcerptUnavailableReason::TooLargeToPreview
        );
    }

    #[test]
    fn load_from_path_reports_line_out_of_range_for_complete_text() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        fixture::write_text(file.path(), "one\ntwo\n");

        assert_eq!(
            unavailable_reason(load_from_path(file.path(), 10)),
            BookmarkExcerptUnavailableReason::LineOutOfRange
        );
    }

    #[test]
    fn load_from_path_reports_line_beyond_byte_budget() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        let mut content = "first\n".to_string();
        content.push_str(&"x".repeat(BOOKMARK_EXCERPT_SCAN_BYTE_LIMIT + 32));
        content.push_str("\nafter-budget\n");
        fixture::write_text(file.path(), &content);

        assert_eq!(
            unavailable_reason(load_from_path(file.path(), 2)),
            BookmarkExcerptUnavailableReason::LineBeyondPreviewBudget
        );
    }

    #[test]
    fn load_from_path_reports_line_beyond_line_budget() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        fixture::write_text(file.path(), "line\n");

        assert_eq!(
            unavailable_reason(load_from_path(
                file.path(),
                u32::try_from(BOOKMARK_EXCERPT_SCAN_LINE_LIMIT).expect("line limit fits u32")
            )),
            BookmarkExcerptUnavailableReason::LineBeyondPreviewBudget
        );
    }

    #[test]
    fn read_bounded_bytes_marks_truncation_only_when_file_exceeds_limit() {
        let uncancelled = BookmarkExcerptCancellation::default();
        let exact = tempfile::NamedTempFile::new().expect("temp file");
        fixture::write_repeated_bytes(exact.path(), b"x", BOOKMARK_EXCERPT_SCAN_BYTE_LIMIT as u64);
        let exact_bytes = read_bounded_bytes_cancellable(
            exact.path(),
            BOOKMARK_EXCERPT_SCAN_BYTE_LIMIT,
            &uncancelled,
        )
        .expect("read exact");

        assert_eq!(exact_bytes.bytes.len(), BOOKMARK_EXCERPT_SCAN_BYTE_LIMIT);
        assert!(!exact_bytes.truncated_by_bytes);

        let over = tempfile::NamedTempFile::new().expect("temp file");
        fixture::write_repeated_bytes(
            over.path(),
            b"x",
            (BOOKMARK_EXCERPT_SCAN_BYTE_LIMIT + 1) as u64,
        );
        let over_bytes = read_bounded_bytes_cancellable(
            over.path(),
            BOOKMARK_EXCERPT_SCAN_BYTE_LIMIT,
            &uncancelled,
        )
        .expect("read over");

        assert_eq!(over_bytes.bytes.len(), BOOKMARK_EXCERPT_SCAN_BYTE_LIMIT);
        assert!(over_bytes.truncated_by_bytes);
    }

    #[test]
    fn validated_utf8_prefix_accepts_only_truncated_incomplete_tail_sequences() {
        assert_eq!(
            validated_utf8_prefix(&[b'o', b'k', b' ', 0xc3], true).expect("valid prefix"),
            "ok "
        );
        assert!(validated_utf8_prefix(&[b'o', b'k', b' ', 0xc3], false).is_err());
        assert!(validated_utf8_prefix(&[0xff], true).is_err());
    }
}
