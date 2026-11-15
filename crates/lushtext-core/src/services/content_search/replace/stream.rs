// SPDX-License-Identifier: GPL-3.0-or-later

//! The Replace All text engine: one source-order pass over sorted edits.

use super::{MAX_REPLACE_FILE_BYTES, ReplaceConstructionMetrics};
use crate::model::content_search::{MAX_REPLACE_PREVIEW_ROWS, Replacement};

/// Pure result of applying one file's replacement preview data to text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ReplacementTextOutcome {
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
pub(super) struct ReplacementTextBuild {
    pub(super) outcome: ReplacementTextOutcome,
    pub(super) metrics: ReplaceConstructionMetrics,
}

#[derive(Debug)]
pub(super) struct PendingEdit<'a> {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) replacement: &'a str,
}

/// Monotonic source-line cursor that never retains a whole-file line index.
pub(super) struct StreamingLineCursor<'a> {
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
pub(super) fn build_replaced_text(
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
pub(super) fn line_spans_reference(text: &str) -> Vec<std::ops::Range<usize>> {
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
pub(super) fn replaced_capacity(original_len: usize, edits: &[PendingEdit<'_>]) -> usize {
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
pub(super) fn build_replaced_text_reference(
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
