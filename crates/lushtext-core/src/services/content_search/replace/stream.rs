// SPDX-License-Identifier: GPL-3.0-or-later

//! The Replace All text engine: one source-order pass over sorted edits.
//!
//! [`ReplaceStream`] is fed the source file in chunks and writes the
//! replaced bytes to any [`Write`] sink, so neither the source nor the output
//! is ever held whole (design D2). It keeps the edit semantics of the former
//! in-memory builder, which survives as [`build_replaced_text_reference`] for
//! the equivalence tests:
//!
//! - lines end at `\n`; a `\r` directly before it belongs to the terminator,
//!   and a lone `\r` is ordinary line content, so a CRLF split across two
//!   chunks needs no special case: only a targeted line is buffered, up to its
//!   `\n`, and the `\r` is removed from it then;
//! - an edit applies only when its whole line still equals the line the search
//!   recorded, and any mismatch fails the file as [`ReplacementTextOutcome::StaleLine`];
//! - UTF-8 is validated incrementally ([`utf8`]): an incomplete sequence at
//!   the end of a chunk (at most three bytes) is carried into the next one.
//!
//! The engine is I/O-free apart from its [`Write`] sink. It is generic over the
//! digest of its source and output, so production feeds SHA-256 and the tests
//! a recording sink (design D11).

mod utf8;

use std::io::Write;

use self::utf8::Utf8Carry;
use super::ReplaceConstructionMetrics;
use crate::model::content_search::{MAX_REPLACE_PREVIEW_ROWS, Replacement};

/// Bytes read from a file per chunk by both Replace All passes and by Undo.
///
/// Kind: cap; a power of two. Retained memory is two of these buffers
/// at most (the producer's read buffer and the durable writer's), independent
/// of the file's size (design D2; chosen by stream-replace-all task 2.4).
pub const REPLACE_STREAM_CHUNK_BYTES: usize = 1024 * 1024;

/// Incremental digest of one byte stream.
pub(crate) trait DigestSink {
    /// Absorb the next bytes of the stream.
    fn update(&mut self, bytes: &[u8]);
}

/// The production digest: SHA-256, with SHA-NI detected at run time by `sha2`.
#[derive(Clone, Default)]
pub(crate) struct Sha256Sink(sha2::Sha256);

impl Sha256Sink {
    /// The digest of every byte absorbed so far.
    #[must_use]
    pub(crate) fn finish(self) -> [u8; 32] {
        use sha2::Digest as _;
        self.0.finalize().into()
    }
}

impl DigestSink for Sha256Sink {
    fn update(&mut self, bytes: &[u8]) {
        use sha2::Digest as _;
        self.0.update(bytes);
    }
}

/// A digest that keeps the bytes themselves, for the equivalence tests,
/// where SHA-256 is beside the point.
#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RecordingSink(pub(crate) Vec<u8>);

#[cfg(test)]
impl DigestSink for RecordingSink {
    fn update(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
}

/// A digest that discards its input, for a stream whose digest is not needed.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NoDigest;

#[cfg(test)]
impl DigestSink for NoDigest {
    fn update(&mut self, _bytes: &[u8]) {}
}

/// Hash the SHA-256 of a whole byte slice.
#[must_use]
pub(crate) fn sha256_of(bytes: &[u8]) -> [u8; 32] {
    let mut sink = Sha256Sink::default();
    sink.update(bytes);
    sink.finish()
}

/// Pure result of applying one file's replacement preview data to its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplacementTextOutcome {
    /// At least one bounded replacement changed the file text.
    Replaced {
        /// Number of individual replacement previews consumed.
        replacement_count: usize,
    },
    /// No replacement targets the file.
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
    /// The output would exceed the caller's output limit. The stream stops at
    /// the limit: no byte past it reaches the sink.
    OutputLimitExceeded,
    /// The source is not valid UTF-8.
    NotUtf8,
    /// The owning Replace All operation was cancelled during construction.
    Cancelled,
}

/// One validated edit, clipped to character boundaries of its recorded line.
#[derive(Debug, Clone, Copy)]
pub(super) struct StreamEdit<'a> {
    /// One-based line number the edit targets.
    pub(super) line_number: u64,
    /// The whole line the search recorded; the edit applies only to it.
    pub(super) original_line: &'a str,
    /// Byte range within `original_line` the edit replaces.
    pub(super) start: usize,
    /// End of the replaced range within `original_line`.
    pub(super) end: usize,
    /// Text written in place of the range.
    pub(super) replacement: &'a str,
}

/// Sort one file's replacements and check the invariants preview generation
/// guarantees, before any byte of the file is read.
///
/// Returns the edits in source order, or the outcome that ends the file
/// without reading it: [`ReplacementTextOutcome::Unchanged`] for no edits,
/// [`ReplacementTextOutcome::InvalidPreview`] for a broken invariant and
/// [`ReplacementTextOutcome::Cancelled`].
pub(super) fn plan_edits<'a>(
    file_replacements: &mut [&'a Replacement],
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<Vec<StreamEdit<'a>>, ReplacementTextOutcome> {
    if file_replacements.len() > MAX_REPLACE_PREVIEW_ROWS {
        return Err(ReplacementTextOutcome::InvalidPreview {
            reason: "replacement count exceeds the preview limit",
        });
    }
    if file_replacements.is_empty() {
        return Err(ReplacementTextOutcome::Unchanged);
    }
    file_replacements.sort_by(|a, b| {
        a.line_number
            .cmp(&b.line_number)
            .then(a.match_range.start.cmp(&b.match_range.start))
            .then(a.match_range.end.cmp(&b.match_range.end))
    });

    let mut edits = Vec::with_capacity(file_replacements.len());
    let mut previous_line = None;
    let mut previous_end = 0usize;
    for (index, replacement) in file_replacements.iter().enumerate() {
        if index.is_multiple_of(1_024) && is_cancelled() {
            return Err(ReplacementTextOutcome::Cancelled);
        }
        if replacement.line_number == 0 {
            return Err(ReplacementTextOutcome::InvalidPreview {
                reason: "line numbers must be one-based",
            });
        }
        if replacement.match_range.start > replacement.match_range.end {
            return Err(ReplacementTextOutcome::InvalidPreview {
                reason: "replacement range endpoints are reversed",
            });
        }
        let line = replacement.original_line.as_ref();
        let start = line.floor_char_boundary(replacement.match_range.start.min(line.len()));
        let end = line.ceil_char_boundary(replacement.match_range.end.min(line.len()));
        if previous_line == Some(replacement.line_number) && start < previous_end {
            return Err(ReplacementTextOutcome::InvalidPreview {
                reason: "replacement ranges overlap",
            });
        }
        previous_line = Some(replacement.line_number);
        previous_end = end;
        edits.push(StreamEdit {
            line_number: replacement.line_number,
            original_line: line,
            start,
            end,
            replacement: replacement.replacement.as_ref(),
        });
    }
    Ok(edits)
}

/// Why a stream stopped before its end.
#[derive(Debug)]
pub(super) enum StreamStop {
    /// The file ends with this outcome; the stream must not be fed again.
    Outcome(ReplacementTextOutcome),
    /// The output sink failed.
    Io(std::io::Error),
}

/// Lengths, digests and outcome of a completed stream.
#[derive(Debug)]
pub(super) struct StreamSummary<S, O> {
    /// [`ReplacementTextOutcome::Replaced`] or a reason the file failed.
    pub(super) outcome: ReplacementTextOutcome,
    /// Bytes fed to the engine.
    pub(super) source_len: u64,
    /// Bytes the engine wrote to its sink.
    pub(super) output_len: u64,
    /// Digest of the source bytes.
    pub(super) source_digest: S,
    /// Digest of the output bytes.
    pub(super) output_digest: O,
    /// Construction evidence for this file.
    pub(super) metrics: ReplaceConstructionMetrics,
}

/// Offset of the first `\n` in `bytes`.
fn find_newline(bytes: &[u8]) -> Option<usize> {
    memchr::memchr(b'\n', bytes)
}

/// The chunked streaming engine (design D2).
///
/// Its result does not depend on how the source is chunked: whether a file is
/// replaced, and a replaced file's output, count, lengths and digests, are
/// the same at every chunk size, and on valid UTF-8 so is the outcome. Two
/// checks carry this (design D11, "Kani memory"):
///
/// - Kani proves the UTF-8 seams ([`utf8`], shard `core-replace-stream`): fed
///   in chunks of 1 to 4 bytes, the carry rejects exactly the sources of at
///   most 8 arbitrary bytes that a one-shot validation rejects. The engine
///   itself is out of CBMC's reach: one run over 2 symbolic bytes with one
///   fixed edit and no sinks passed 8 GiB in symbolic execution.
/// - The unit test
///   `the_engine_does_not_depend_on_its_chunk_size_over_a_bounded_domain`
///   checks every source, edit set, chunk size and output limit of a bounded
///   domain exhaustively, against the whole feed and the reference builder,
///   and the property test
///   `streaming_engine_matches_the_reference_at_every_chunk_size` samples
///   larger inputs against the reference builder.
pub(super) struct ReplaceStream<'e, W, S, O> {
    edits: &'e [StreamEdit<'e>],
    next_edit: usize,
    /// One-based number of the line the cursor is in.
    line_number: u64,
    /// The cursor is at the first byte of `line_number`.
    at_line_start: bool,
    /// The current line is targeted and its bytes go to `line`.
    collecting: bool,
    /// Bytes of the targeted line seen so far, at most `line_cap`.
    line: Vec<u8>,
    /// Whether any byte of the collected line was seen.
    line_seen: bool,
    /// The longest a targeted line can be and still match.
    line_cap: usize,
    utf8: Utf8Carry,
    out: W,
    output_digest: O,
    output_len: u64,
    output_limit: u64,
    source_digest: S,
    source_len: u64,
    applied: usize,
    metrics: ReplaceConstructionMetrics,
    /// The outcome a stop already decided; `finish` reports it.
    stopped: Option<ReplacementTextOutcome>,
}

impl<'e, W: Write, S: DigestSink, O: DigestSink> ReplaceStream<'e, W, S, O> {
    /// A stream that applies `edits` (from [`plan_edits`]) and writes at most
    /// `output_limit` bytes to `out`.
    pub(super) fn new(
        edits: &'e [StreamEdit<'e>],
        output_limit: u64,
        out: W,
        source_digest: S,
        output_digest: O,
    ) -> Self {
        let metrics = ReplaceConstructionMetrics {
            retained_edit_records: edits.len(),
            retained_edit_bytes: std::mem::size_of_val(edits),
            ..ReplaceConstructionMetrics::default()
        };
        let mut stream = Self {
            edits,
            next_edit: 0,
            line_number: 1,
            at_line_start: true,
            collecting: false,
            line: Vec::new(),
            line_seen: false,
            line_cap: 0,
            utf8: Utf8Carry::default(),
            out,
            output_digest,
            output_len: 0,
            output_limit,
            source_digest,
            source_len: 0,
            applied: 0,
            metrics,
            stopped: None,
        };
        stream.enter_line();
        stream
    }

    /// Feed the next chunk of the source.
    ///
    /// # Errors
    ///
    /// [`StreamStop::Outcome`] once the file's outcome is decided before its
    /// end (a stale line, invalid UTF-8, the output limit), and
    /// [`StreamStop::Io`] when the sink fails.
    pub(super) fn feed(&mut self, chunk: &[u8]) -> Result<(), StreamStop> {
        if let Some(outcome) = self.stopped {
            return Err(StreamStop::Outcome(outcome));
        }
        let result = self.feed_chunk(chunk);
        if let Err(StreamStop::Outcome(outcome)) = &result {
            self.stopped = Some(*outcome);
        }
        result
    }

    fn feed_chunk(&mut self, chunk: &[u8]) -> Result<(), StreamStop> {
        self.source_digest.update(chunk);
        self.source_len = self
            .source_len
            .saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        if self.utf8.feed(chunk).is_err() {
            return Err(StreamStop::Outcome(ReplacementTextOutcome::NotUtf8));
        }
        let mut rest = chunk;
        while !rest.is_empty() {
            if self.collecting {
                if let Some(newline) = find_newline(rest) {
                    self.collect(&rest[..newline])?;
                    rest = &rest[newline + 1..];
                    self.complete_line(true)?;
                } else {
                    self.collect(rest)?;
                    rest = &[];
                }
            } else if self.next_edit == self.edits.len() {
                self.emit(rest)?;
                rest = &[];
            } else {
                if let Some(newline) = find_newline(rest) {
                    self.emit(&rest[..=newline])?;
                    rest = &rest[newline + 1..];
                    self.advance_line();
                } else {
                    self.emit(rest)?;
                    self.at_line_start = false;
                    rest = &[];
                }
            }
        }
        Ok(())
    }

    /// End the stream at the end of the source.
    ///
    /// # Errors
    ///
    /// [`StreamStop::Io`] when the sink fails; every other end is the summary's
    /// outcome.
    pub(super) fn finish(mut self) -> Result<(StreamSummary<S, O>, W), StreamStop> {
        let outcome = match self.finish_outcome() {
            Ok(outcome) | Err(StreamStop::Outcome(outcome)) => outcome,
            Err(StreamStop::Io(error)) => return Err(StreamStop::Io(error)),
        };
        self.metrics.accepted_replacements = self.applied;
        self.metrics.output_bytes = self.output_len;
        Ok((
            StreamSummary {
                outcome,
                source_len: self.source_len,
                output_len: self.output_len,
                source_digest: self.source_digest,
                output_digest: self.output_digest,
                metrics: self.metrics,
            },
            self.out,
        ))
    }

    fn finish_outcome(&mut self) -> Result<ReplacementTextOutcome, StreamStop> {
        if let Some(outcome) = self.stopped {
            return Ok(outcome);
        }
        if self.utf8.finish().is_err() {
            return Ok(ReplacementTextOutcome::NotUtf8);
        }
        if self.collecting && self.line_seen {
            // The final line has no terminator; its bytes, a trailing `\r`
            // included, are the line.
            self.complete_line(false)?;
        }
        if let Some(edit) = self.edits.get(self.next_edit) {
            return Ok(ReplacementTextOutcome::StaleLine {
                line_number: edit.line_number,
            });
        }
        Ok(ReplacementTextOutcome::Replaced {
            replacement_count: self.applied,
        })
    }

    /// Move to the line after a `\n` the cursor just passed.
    fn advance_line(&mut self) {
        self.line_number = self.line_number.saturating_add(1);
        self.metrics.source_lines = self.metrics.source_lines.saturating_add(1);
        self.at_line_start = true;
        self.enter_line();
    }

    /// Start collecting when the line the cursor starts is targeted.
    fn enter_line(&mut self) {
        let Some(edit) = self.edits.get(self.next_edit) else {
            return;
        };
        if self.at_line_start && edit.line_number == self.line_number {
            self.collecting = true;
            self.line.clear();
            self.line_seen = false;
            // A matching line is its recorded text plus, before a `\n`, one
            // `\r`; anything longer cannot match.
            self.line_cap = self.edits[self.next_edit..]
                .iter()
                .take_while(|candidate| candidate.line_number == self.line_number)
                .map(|candidate| candidate.original_line.len())
                .max()
                .unwrap_or(0)
                .saturating_add(1);
        }
    }

    fn collect(&mut self, bytes: &[u8]) -> Result<(), StreamStop> {
        if !bytes.is_empty() {
            self.line_seen = true;
        }
        if self.line.len().saturating_add(bytes.len()) > self.line_cap {
            return Err(self.stale());
        }
        self.line.extend_from_slice(bytes);
        self.metrics.retained_line_bytes =
            self.metrics.retained_line_bytes.max(self.line.capacity());
        Ok(())
    }

    fn stale(&self) -> StreamStop {
        StreamStop::Outcome(ReplacementTextOutcome::StaleLine {
            line_number: self.line_number,
        })
    }

    /// Apply every edit of the collected line, then move past it.
    fn complete_line(&mut self, terminated: bool) -> Result<(), StreamStop> {
        self.line_seen = true;
        let mut content_len = self.line.len();
        let carriage_return = terminated && self.line.last() == Some(&b'\r');
        if carriage_return {
            content_len -= 1;
        }
        let line = std::mem::take(&mut self.line);
        let content = &line[..content_len];
        let first = self.next_edit;
        let mut last = first;
        while self
            .edits
            .get(last)
            .is_some_and(|edit| edit.line_number == self.line_number)
        {
            if self.edits[last].original_line.as_bytes() != content {
                return Err(self.stale());
            }
            last += 1;
        }
        let mut cursor = 0usize;
        for index in first..last {
            let edit = self.edits[index];
            self.emit(&content[cursor..edit.start])?;
            self.emit(edit.replacement.as_bytes())?;
            cursor = edit.end;
        }
        self.emit(&content[cursor..])?;
        if carriage_return {
            self.emit(b"\r")?;
        }
        if terminated {
            self.emit(b"\n")?;
        }
        self.applied += last - first;
        self.next_edit = last;
        self.line = line;
        self.line.clear();
        self.collecting = false;
        if terminated {
            self.advance_line();
        } else {
            self.at_line_start = false;
        }
        Ok(())
    }

    fn emit(&mut self, bytes: &[u8]) -> Result<(), StreamStop> {
        if bytes.is_empty() {
            return Ok(());
        }
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if self.output_len.saturating_add(len) > self.output_limit {
            return Err(StreamStop::Outcome(
                ReplacementTextOutcome::OutputLimitExceeded,
            ));
        }
        self.out.write_all(bytes).map_err(StreamStop::Io)?;
        self.output_digest.update(bytes);
        self.output_len += len;
        Ok(())
    }
}

/// Run the engine over a whole in-memory source, in chunks of `chunk_len`.
///
/// For tests and the property target: production always
/// feeds a file through a producer.
#[cfg(any(test, feature = "property-tests"))]
pub(super) fn stream_whole<S: DigestSink, O: DigestSink>(
    source: &[u8],
    edits: &[StreamEdit<'_>],
    chunk_len: usize,
    output_limit: u64,
    source_digest: S,
    output_digest: O,
) -> (StreamSummary<S, O>, Vec<u8>) {
    let mut stream = ReplaceStream::new(
        edits,
        output_limit,
        Vec::new(),
        source_digest,
        output_digest,
    );
    for chunk in source.chunks(chunk_len.max(1)) {
        if stream.feed(chunk).is_err() {
            break;
        }
    }
    match stream.finish() {
        Ok(done) => done,
        Err(_) => unreachable!("a Vec sink cannot fail"),
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

/// The former whole-buffer algorithm: an independent implementation the
/// streaming engine is checked against at every chunk split.
///
/// Returns the replaced text and the replacement count, or the outcome that
/// ended the file.
#[cfg(any(test, feature = "property-tests"))]
pub(super) fn build_replaced_text_reference(
    original_text: &str,
    file_replacements: &mut [&Replacement],
) -> Result<(String, usize), ReplacementTextOutcome> {
    file_replacements.sort_by(|a, b| {
        a.line_number
            .cmp(&b.line_number)
            .then(a.match_range.start.cmp(&b.match_range.start))
            .then(a.match_range.end.cmp(&b.match_range.end))
    });

    let line_spans = line_spans_reference(original_text);
    let mut edits: Vec<(usize, usize, &str)> = Vec::with_capacity(file_replacements.len());
    for replacement in file_replacements.iter() {
        let Some(line_index) = replacement.line_number.checked_sub(1) else {
            return Err(ReplacementTextOutcome::InvalidPreview {
                reason: "line numbers must be one-based",
            });
        };
        let Ok(line_index) = usize::try_from(line_index) else {
            return Err(ReplacementTextOutcome::StaleLine {
                line_number: replacement.line_number,
            });
        };
        let Some(line_span) = line_spans.get(line_index).cloned() else {
            return Err(ReplacementTextOutcome::StaleLine {
                line_number: replacement.line_number,
            });
        };
        let line = &original_text[line_span.clone()];
        if line != replacement.original_line.as_ref() {
            return Err(ReplacementTextOutcome::StaleLine {
                line_number: replacement.line_number,
            });
        }
        let start = line.floor_char_boundary(replacement.match_range.start.min(line.len()));
        let end = line.ceil_char_boundary(replacement.match_range.end.min(line.len()));
        edits.push((
            line_span.start + start,
            line_span.start + end,
            replacement.replacement.as_ref(),
        ));
    }

    if edits.is_empty() {
        return Err(ReplacementTextOutcome::Unchanged);
    }

    let mut new_content = String::with_capacity(original_text.len());
    let mut cursor = 0usize;
    for (start, end, replacement) in &edits {
        new_content.push_str(&original_text[cursor..*start]);
        new_content.push_str(replacement);
        cursor = *end;
    }
    new_content.push_str(&original_text[cursor..]);
    Ok((new_content, edits.len()))
}

/// Run the production streaming engine over whole text in one chunk.
///
/// This feature-only hook lets the property target exercise clipping and
/// ordering without touching files, locks, or the undo journal.
#[cfg(feature = "property-tests")]
#[must_use]
pub fn apply_replacements_to_text_for_property_test(
    original_text: &str,
    replacements: &[Replacement],
) -> Option<(String, usize)> {
    apply_replacements_to_text_in_chunks_for_property_test(
        original_text,
        replacements,
        original_text.len().max(1),
    )
    .ok()
    .map(|(text, count, _, _)| (text, count))
}

/// Run the streaming engine over `original_text` in chunks of `chunk_len` and
/// return the output, the replacement count and the SHA-256 digests of the
/// source and the output, or the debug form of the outcome that ended the file.
///
/// # Errors
///
/// The outcome, as `format!("{outcome:?}")`, when the file is not replaced.
#[cfg(feature = "property-tests")]
pub fn apply_replacements_to_text_in_chunks_for_property_test(
    original_text: &str,
    replacements: &[Replacement],
    chunk_len: usize,
) -> Result<(String, usize, [u8; 32], [u8; 32]), String> {
    let mut file_replacements: Vec<&Replacement> = replacements.iter().collect();
    let edits = plan_edits(&mut file_replacements, || false).map_err(|o| format!("{o:?}"))?;
    let (summary, out) = stream_whole(
        original_text.as_bytes(),
        &edits,
        chunk_len,
        u64::MAX,
        Sha256Sink::default(),
        Sha256Sink::default(),
    );
    match summary.outcome {
        ReplacementTextOutcome::Replaced { replacement_count } => Ok((
            String::from_utf8(out).map_err(|_| "output is not UTF-8".to_string())?,
            replacement_count,
            summary.source_digest.finish(),
            summary.output_digest.finish(),
        )),
        outcome => Err(format!("{outcome:?}")),
    }
}

/// The reference builder's text and count, or the debug form of its outcome.
///
/// # Errors
///
/// The outcome, as `format!("{outcome:?}")`, when the file is not replaced.
#[cfg(feature = "property-tests")]
pub fn apply_replacements_to_text_reference_outcome_for_property_test(
    original_text: &str,
    replacements: &[Replacement],
) -> Result<(String, usize), String> {
    let mut file_replacements: Vec<&Replacement> = replacements.iter().collect();
    build_replaced_text_reference(original_text, &mut file_replacements)
        .map_err(|outcome| format!("{outcome:?}"))
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
    build_replaced_text_reference(original_text, &mut file_replacements).ok()
}
