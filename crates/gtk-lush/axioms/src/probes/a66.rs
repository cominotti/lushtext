// SPDX-License-Identifier: MIT OR Apache-2.0

//! A66 (cost evidence, like A36): reading one fixed-size chunk (64 Ki
//! characters) of a single-line paragraph inserted in one call (one
//! character segment), at increasing depth, by each way an iterator can be
//! positioned:
//!
//! - **by character offset** (`iter_at_offset`, then `forward_chars`): GTK
//!   finds the segment by character count, then walks UTF-8 from the
//!   segment's start to turn the offset into a byte position, so the cost
//!   grows with the depth;
//! - **by a mark walking forward** chunk by chunk (`iter_at_mark`,
//!   `forward_chars`, `move_mark`), the route `ui/buffer_snapshot.rs`'s
//!   chunked capture takes;
//! - **by line and byte index** (`iter_at_line_index` for both ends, ASCII
//!   so every byte is a character boundary);
//! - **chained by byte index** (start by `iter_at_line_index`, end by
//!   `forward_chars`, the next start from the end's `line_index`), the
//!   multibyte-safe form, on ASCII and multibyte paragraphs.
//!
//! The verdict is structural: every positioning reads exactly the expected
//! text. The per-chunk times are recorded values; the ledger row says which
//! grow with depth. Design D4 of Protected View (C2) captures an edit-grown
//! source by the positioning whose time stays flat, or declares the capture
//! an envelope.

use std::time::Instant;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{ParagraphKind, paragraph};
use crate::{AxiomId, Observation};

/// Characters in the paragraph.
pub const PARAGRAPH_CHARS: usize = 2_000_000;
/// Characters per chunk.
pub const CHUNK_CHARS: usize = 64 * 1024;
/// Depths (in characters) the direct positionings read at.
pub const DEPTHS: [usize; 6] = [
    0,
    256 * 1024,
    512 * 1024,
    1024 * 1024,
    1536 * 1024,
    1_900_000,
];
/// Repetitions of each direct read; the fastest is recorded.
const REPEATS: usize = 3;

/// A buffer holding `text` inserted in one call.
#[must_use]
pub fn single_segment_buffer(text: &str) -> gtk4::TextBuffer {
    let buffer = gtk4::TextBuffer::new(None);
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, text);
    buffer
}

fn offset(chars: usize) -> i32 {
    i32::try_from(chars).unwrap_or(i32::MAX)
}

/// Read the chunk at character `depth` by character offset.
#[must_use]
pub fn read_by_offset(buffer: &gtk4::TextBuffer, depth: usize) -> String {
    let start = buffer.iter_at_offset(offset(depth));
    let mut end = start;
    end.forward_chars(offset(CHUNK_CHARS));
    buffer.text(&start, &end, true).to_string()
}

/// Read the chunk at byte `depth` by line and byte index (both ends).
#[must_use]
pub fn read_by_line_index(buffer: &gtk4::TextBuffer, depth: usize) -> String {
    let line_end = buffer.end_iter().line_index();
    let start = buffer.iter_at_line_index(0, offset(depth));
    let end = buffer.iter_at_line_index(0, offset(depth + CHUNK_CHARS).min(line_end));
    match (start, end) {
        (Some(start), Some(end)) => buffer.text(&start, &end, true).to_string(),
        _ => String::new(),
    }
}

fn fastest<T>(mut work: impl FnMut() -> T) -> (T, u128) {
    let mut best = u128::MAX;
    let mut value = None;
    for _ in 0..REPEATS {
        let started = Instant::now();
        let result = work();
        best = best.min(started.elapsed().as_micros());
        value = Some(result);
    }
    (value.unwrap_or_else(work), best)
}

/// Walk the whole paragraph chunk by chunk with a mark, as the chunked
/// snapshot does. Returns the text and each chunk's microseconds.
#[must_use]
pub fn walk_by_mark(buffer: &gtk4::TextBuffer) -> (String, Vec<u128>) {
    let mark = buffer.create_mark(None, &buffer.start_iter(), true);
    let mut text = String::new();
    let mut times = Vec::new();
    loop {
        let started = Instant::now();
        let start = buffer.iter_at_mark(&mark);
        if start.is_end() {
            break;
        }
        let mut end = start;
        end.forward_chars(offset(CHUNK_CHARS));
        text.push_str(&buffer.text(&start, &end, true));
        buffer.move_mark(&mark, &end);
        times.push(started.elapsed().as_micros());
    }
    buffer.delete_mark(&mark);
    (text, times)
}

/// Walk the whole paragraph chunk by chunk, each start positioned by line
/// and byte index and each end by `forward_chars`. Returns the text and
/// each chunk's microseconds.
#[must_use]
pub fn walk_by_line_index(buffer: &gtk4::TextBuffer) -> (String, Vec<u128>) {
    let mut text = String::new();
    let mut times = Vec::new();
    let mut byte = 0;
    loop {
        let started = Instant::now();
        let Some(start) = buffer.iter_at_line_index(0, byte) else {
            break;
        };
        if start.is_end() {
            break;
        }
        let mut end = start;
        end.forward_chars(offset(CHUNK_CHARS));
        text.push_str(&buffer.text(&start, &end, true));
        byte = if end.is_end() {
            i32::MAX
        } else {
            end.line_index()
        };
        times.push(started.elapsed().as_micros());
        if byte == i32::MAX {
            break;
        }
    }
    (text, times)
}

/// Steps per `forward_chars` call in [`walk_by_persistent_iter`]: below
/// GtkTextIter's `MAX_LINEAR_SCAN` (150), so each call walks character by
/// character and keeps the iterator's byte offset current instead of
/// re-deriving it from the segment's start.
pub const LINEAR_STEP: i32 = 128;

/// Walk the whole paragraph chunk by chunk with one iterator kept across
/// chunks (valid while the buffer does not change), positioned by byte
/// index and advanced in steps of [`LINEAR_STEP`]. Returns the text and each chunk's microseconds.
#[must_use]
pub fn walk_by_persistent_iter(buffer: &gtk4::TextBuffer) -> (String, Vec<u128>) {
    let mut text = String::new();
    let mut times = Vec::new();
    // Positioned by byte index, so the iterator starts with both its byte
    // and its character offset known (`start_iter` resolves the byte offset
    // lazily, and every later read would re-derive it from the segment's
    // start); `forward_chars` below `MAX_LINEAR_SCAN` keeps both current.
    let Some(mut start) = buffer.iter_at_line_index(0, 0) else {
        return (text, times);
    };
    while !start.is_end() {
        let started = Instant::now();
        let mut end = start;
        let mut advanced = 0;
        while advanced < offset(CHUNK_CHARS) && !end.is_end() {
            let step = LINEAR_STEP.min(offset(CHUNK_CHARS) - advanced);
            end.forward_chars(step);
            advanced += step;
        }
        ADVANCE_US.with(|cell| cell.borrow_mut().push(started.elapsed().as_micros()));
        text.push_str(&buffer.text(&start, &end, true));
        start = end;
        times.push(started.elapsed().as_micros());
    }
    (text, times)
}

thread_local! {
    static ADVANCE_US: std::cell::RefCell<Vec<u128>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn mean(times: &[u128]) -> u128 {
    times.iter().sum::<u128>() / u128::try_from(times.len().max(1)).unwrap_or(1)
}

/// The chunk times at the indexes that match [`DEPTHS`] (chunk `i` starts
/// at `i * CHUNK_CHARS`).
fn at_depths(times: &[u128]) -> String {
    super::join_entries(DEPTHS.iter().filter_map(|depth| {
        let index = depth / CHUNK_CHARS;
        times
            .get(index)
            .map(|micros| format!("{}k: {micros} us", index * CHUNK_CHARS / 1024))
    }))
}

/// Probe A66.
#[must_use]
pub fn probe_a66() -> Observation {
    Recorder::run(AxiomId::new(66), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let ascii = paragraph(ParagraphKind::Ascii, PARAGRAPH_CHARS, 66);
        let multibyte = paragraph(ParagraphKind::Multibyte, PARAGRAPH_CHARS, 67);
        let ascii_chars: Vec<char> = ascii.chars().collect();
        let expected = |depth: usize| -> String {
            ascii_chars[depth..(depth + CHUNK_CHARS).min(PARAGRAPH_CHARS)]
                .iter()
                .collect()
        };

        let buffer = single_segment_buffer(&ascii);
        recorder.control(
            buffer.line_count() == 1
                && usize::try_from(buffer.char_count()).ok() == Some(PARAGRAPH_CHARS),
            "control: the buffer is one line of the whole paragraph",
        )?;
        let mut by_offset = Vec::new();
        let mut by_index = Vec::new();
        let mut direct_correct = true;
        for depth in DEPTHS {
            let (text, micros) = fastest(|| read_by_offset(&buffer, depth));
            direct_correct &= text == expected(depth);
            by_offset.push(format!("{}k: {micros} us", depth / 1024));
            let (text, micros) = fastest(|| read_by_line_index(&buffer, depth));
            direct_correct &= text == expected(depth);
            by_index.push(format!("{}k: {micros} us", depth / 1024));
        }
        recorder.measure("chunk_by_char_offset", super::join_entries(&by_offset));
        recorder.measure("chunk_by_line_byte_index", super::join_entries(&by_index));

        let (chained, chained_times) = walk_by_line_index(&buffer);
        recorder.measure(
            "walk_chained_by_line_index_ascii",
            at_depths(&chained_times),
        );
        recorder.measure(
            "walk_chained_by_line_index_ascii_total_us",
            chained_times.iter().sum::<u128>(),
        );
        drop(buffer);

        let marked = single_segment_buffer(&ascii);
        let (walked, mark_times) = walk_by_mark(&marked);
        recorder.measure("walk_by_mark_ascii", at_depths(&mark_times));
        recorder.measure("walk_by_mark_ascii_mean_chunk_us", mean(&mark_times));
        recorder.measure(
            "walk_by_mark_ascii_total_us",
            mark_times.iter().sum::<u128>(),
        );
        drop(marked);
        let half: String = ascii_chars[..PARAGRAPH_CHARS / 2].iter().collect();
        let (walked_half, half_times) = walk_by_mark(&single_segment_buffer(&half));
        recorder.measure(
            "walk_by_mark_half_paragraph_mean_chunk_us",
            mean(&half_times),
        );

        let persistent_buffer = single_segment_buffer(&ascii);
        let (persistent, persistent_times) = walk_by_persistent_iter(&persistent_buffer);
        recorder.measure(
            "walk_by_persistent_iter_ascii",
            at_depths(&persistent_times),
        );
        recorder.measure(
            "walk_by_persistent_iter_ascii_advance_only",
            at_depths(&ADVANCE_US.with(|cell| std::mem::take(&mut *cell.borrow_mut()))),
        );
        recorder.measure(
            "walk_by_persistent_iter_ascii_total_us",
            persistent_times.iter().sum::<u128>(),
        );
        let (whole, whole_us) = fastest(|| {
            let (start, end) = persistent_buffer.bounds();
            persistent_buffer.text(&start, &end, true).len()
        });
        recorder.measure("whole_paragraph_one_read_us", whole_us);
        drop(persistent_buffer);

        let multibyte_buffer = single_segment_buffer(&multibyte);
        let (chained_multibyte, multibyte_times) = walk_by_line_index(&multibyte_buffer);
        recorder.measure(
            "walk_chained_by_line_index_multibyte",
            at_depths(&multibyte_times),
        );
        recorder.measure(
            "walk_chained_by_line_index_multibyte_total_us",
            multibyte_times.iter().sum::<u128>(),
        );
        let (persistent_multibyte, persistent_multibyte_times) =
            walk_by_persistent_iter(&multibyte_buffer);
        recorder.measure(
            "walk_by_persistent_iter_multibyte",
            at_depths(&persistent_multibyte_times),
        );
        recorder.measure(
            "walk_by_persistent_iter_multibyte_total_us",
            persistent_multibyte_times.iter().sum::<u128>(),
        );
        let multibyte_marked = single_segment_buffer(&multibyte);
        let (walked_multibyte, multibyte_mark_times) = walk_by_mark(&multibyte_marked);
        recorder.measure("walk_by_mark_multibyte", at_depths(&multibyte_mark_times));
        recorder.measure(
            "walk_by_mark_multibyte_total_us",
            multibyte_mark_times.iter().sum::<u128>(),
        );

        recorder.axiom(
            direct_correct,
            "a chunk read by character offset and by line and byte index is exactly the \
             expected text at every depth",
        )?;
        recorder.axiom(
            walked == ascii
                && chained == ascii
                && persistent == ascii
                && whole == ascii.len()
                && walked_half == half,
            "every walk reads the ASCII paragraph exactly",
        )?;
        recorder.axiom(
            walked_multibyte == multibyte
                && chained_multibyte == multibyte
                && persistent_multibyte == multibyte,
            "every walk reads the multibyte paragraph exactly",
        )
    })
}
