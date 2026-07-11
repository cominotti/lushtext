// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared GTK-buffer snapshot helpers for UI workflows.
//!
//! GtkTextBuffer content can only be read on the GTK thread, so this module
//! belongs in the UI layer. It gives save, draft, preview, and encoding flows a
//! common way to keep large text copies from monopolizing one main-loop turn.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;

/// Files at or above roughly 10 MB use chunked snapshotting.
///
/// The threshold mirrors LushText's syntax-disable boundary: below it, a direct
/// copy is usually cheaper than scheduling; above it, one synchronous buffer
/// copy can create a visible frame stall on slower machines.
pub(crate) const BUFFER_SNAPSHOT_SYNC_BYTE_THRESHOLD: u64 = 10_000_000;

/// Characters copied per GTK main-loop slice during a chunked snapshot.
///
/// `GtkTextBuffer::char_count()` is character-based, and 64k characters has
/// stayed comfortably below a frame on local measurements while avoiding a very
/// long chain of tiny timers for multi-megabyte buffers.
const BUFFER_SNAPSHOT_CHUNK_CHARS: i32 = 64 * 1024;

// Main-loop slices share one callback through `Rc`; `Option::take()` guarantees
// exactly one terminal invocation through success, overflow, or cancellation.
type ChunkedCallback = Rc<RefCell<Option<Box<dyn FnOnce(String)>>>>;
type BudgetedChunkedCallback = Rc<RefCell<Option<Box<dyn FnOnce(BufferSnapshotOutcome)>>>>;

/// Result of copying a GTK buffer under a UTF-8 byte budget.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BufferSnapshotOutcome {
    /// The complete buffer was captured within the configured limit.
    Captured(String),
    /// Capture exceeded the budget; the byte count is a copied lower bound.
    ExceededLimit {
        /// Bytes retained through the first chunk that proved overflow.
        observed_at_least: u64,
    },
    /// The owning workflow no longer needs the in-progress snapshot.
    Cancelled,
}

/// Main-thread cancellation handle for a chunked GTK buffer snapshot.
///
/// `Rc<Cell<_>>` is intentional: GTK buffer reads and cancellation checks both
/// stay on the main thread, so cross-thread synchronization would add no safety.
#[derive(Clone, Default)]
pub(crate) struct BufferSnapshotCancellation(Rc<Cell<bool>>);

impl BufferSnapshotCancellation {
    /// Stop the next slice before it copies more buffer content.
    pub(crate) fn cancel(&self) {
        self.0.set(true);
    }

    fn is_cancelled(&self) -> bool {
        self.0.get()
    }
}

/// Decide whether a character count is large enough to require chunked capture.
#[must_use]
pub(crate) fn char_count_requires_chunked_snapshot(char_count: i32) -> bool {
    let char_count = u64::try_from(char_count).unwrap_or(u64::MAX);
    // GTK counts Unicode scalars, while persistence budgets bytes; four is the
    // maximum UTF-8 width of one scalar.
    char_count.saturating_mul(4) >= BUFFER_SNAPSHOT_SYNC_BYTE_THRESHOLD
}

/// Decide whether copying the live buffer should yield through the main loop.
#[must_use]
pub(crate) fn buffer_requires_chunked_snapshot(buffer: &impl IsA<gtk4::TextBuffer>) -> bool {
    char_count_requires_chunked_snapshot(buffer.char_count())
}

/// Copy the whole buffer immediately for workflows already below the threshold.
#[must_use]
pub(crate) fn snapshot_buffer_text_direct(buffer: &impl IsA<gtk4::TextBuffer>) -> String {
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
}

/// Copy a small buffer immediately and enforce an exact UTF-8 byte limit.
#[must_use]
pub(crate) fn snapshot_buffer_text_direct_budgeted(
    buffer: &impl IsA<gtk4::TextBuffer>,
    max_bytes: u64,
) -> BufferSnapshotOutcome {
    classify_snapshot_text(snapshot_buffer_text_direct(buffer), max_bytes)
}

/// Convert a completed direct copy into an all-or-nothing budget outcome.
fn classify_snapshot_text(text: String, max_bytes: u64) -> BufferSnapshotOutcome {
    let observed_at_least = u64::try_from(text.len()).unwrap_or(u64::MAX);
    if observed_at_least > max_bytes {
        BufferSnapshotOutcome::ExceededLimit { observed_at_least }
    } else {
        BufferSnapshotOutcome::Captured(text)
    }
}

/// Append one character-aligned chunk and report the first proven overflow.
fn append_budgeted_chunk(
    text: &mut String,
    chunk: &str,
    max_bytes: u64,
) -> Option<BufferSnapshotOutcome> {
    text.push_str(chunk);
    let observed_at_least = u64::try_from(text.len()).unwrap_or(u64::MAX);
    (observed_at_least > max_bytes)
        .then_some(BufferSnapshotOutcome::ExceededLimit { observed_at_least })
}

/// Copy a large buffer in bounded GTK main-loop slices.
///
/// The callback runs on the GTK thread after all chunks have been collected.
/// Worker-thread I/O or pure analysis should be scheduled from that callback
/// using owned text, never by sending the `Buffer` itself across threads.
pub(crate) fn snapshot_buffer_text_async<F: FnOnce(String) + 'static>(
    buffer: impl IsA<gtk4::TextBuffer> + Clone + 'static,
    callback: F,
) {
    let buffer = buffer.upcast::<gtk4::TextBuffer>();
    let text = Rc::new(RefCell::new(String::new()));
    let callback: ChunkedCallback = Rc::new(RefCell::new(Some(Box::new(callback))));
    snapshot_buffer_text_chunk(buffer.clone(), buffer.start_iter(), text, callback);
}

/// Copy a large buffer in slices while enforcing a UTF-8 byte budget.
///
/// At most one additional 64k-character chunk is retained beyond `max_bytes`.
/// The callback never receives partial text after cancellation or overflow.
pub(crate) fn snapshot_buffer_text_async_budgeted<F: FnOnce(BufferSnapshotOutcome) + 'static>(
    buffer: impl IsA<gtk4::TextBuffer> + Clone + 'static,
    max_bytes: u64,
    cancellation: BufferSnapshotCancellation,
    callback: F,
) {
    let buffer = buffer.upcast::<gtk4::TextBuffer>();
    let text = Rc::new(RefCell::new(String::new()));
    let callback: BudgetedChunkedCallback = Rc::new(RefCell::new(Some(Box::new(callback))));
    snapshot_buffer_text_chunk_budgeted(
        buffer.clone(),
        buffer.start_iter(),
        text,
        max_bytes,
        cancellation,
        callback,
    );
}

/// Copy one budgeted slice, then yield to GTK or finish without partial text.
fn snapshot_buffer_text_chunk_budgeted(
    buffer: gtk4::TextBuffer,
    start: gtk4::TextIter,
    text: Rc<RefCell<String>>,
    max_bytes: u64,
    cancellation: BufferSnapshotCancellation,
    callback: BudgetedChunkedCallback,
) {
    if cancellation.is_cancelled() {
        if let Some(callback) = callback.borrow_mut().take() {
            callback(BufferSnapshotOutcome::Cancelled);
        }
        return;
    }

    let mut end = start;
    if !end.forward_chars(BUFFER_SNAPSHOT_CHUNK_CHARS) {
        end = buffer.end_iter();
    }
    let chunk = buffer.text(&start, &end, true);
    let overflow = append_budgeted_chunk(&mut text.borrow_mut(), chunk.as_str(), max_bytes);
    if let Some(outcome) = overflow {
        if let Some(callback) = callback.borrow_mut().take() {
            callback(outcome);
        }
        return;
    }
    if end == buffer.end_iter() {
        if let Some(callback) = callback.borrow_mut().take() {
            callback(BufferSnapshotOutcome::Captured(std::mem::take(
                &mut *text.borrow_mut(),
            )));
        }
        return;
    }

    // One millisecond gives GTK a scheduling point without materially slowing
    // a many-megabyte capture; the byte budget is checked again next slice.
    glib::timeout_add_local_once(Duration::from_millis(1), move || {
        snapshot_buffer_text_chunk_budgeted(buffer, end, text, max_bytes, cancellation, callback);
    });
}

/// Copy one unbounded slice and reschedule until the full buffer is available.
fn snapshot_buffer_text_chunk(
    buffer: gtk4::TextBuffer,
    start: gtk4::TextIter,
    text: Rc<RefCell<String>>,
    callback: ChunkedCallback,
) {
    let mut end = start;
    if !end.forward_chars(BUFFER_SNAPSHOT_CHUNK_CHARS) {
        end = buffer.end_iter();
    }

    let chunk = buffer.text(&start, &end, true);
    text.borrow_mut().push_str(chunk.as_str());

    if end == buffer.end_iter() {
        if let Some(callback) = callback.borrow_mut().take() {
            callback(std::mem::take(&mut *text.borrow_mut()));
        }
        return;
    }

    // Yield back to GTK between slices so repaint, input, and async completions
    // are not starved by a multi-megabyte buffer copy.
    glib::timeout_add_local_once(Duration::from_millis(1), move || {
        snapshot_buffer_text_chunk(buffer, end, text, callback);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_char_counts_use_direct_snapshot() {
        assert!(!char_count_requires_chunked_snapshot(1_000));
    }

    #[test]
    fn threshold_char_counts_use_chunked_snapshot() {
        assert!(char_count_requires_chunked_snapshot(2_500_000));
    }

    #[test]
    fn negative_char_counts_are_treated_as_unknown_large_values() {
        assert!(char_count_requires_chunked_snapshot(-1));
    }

    #[test]
    fn direct_budget_accepts_empty_exact_and_multibyte_text() {
        assert_eq!(
            classify_snapshot_text(String::new(), 0),
            BufferSnapshotOutcome::Captured(String::new())
        );
        assert_eq!(
            classify_snapshot_text("é".to_string(), 2),
            BufferSnapshotOutcome::Captured("é".to_string())
        );
    }

    #[test]
    fn direct_budget_rejects_one_utf8_byte_over_without_partial_text() {
        assert_eq!(
            classify_snapshot_text("é".to_string(), 1),
            BufferSnapshotOutcome::ExceededLimit {
                observed_at_least: 2
            }
        );
    }

    #[test]
    fn cancellation_handle_is_sticky() {
        let cancellation = BufferSnapshotCancellation::default();
        assert!(!cancellation.is_cancelled());
        cancellation.cancel();
        assert!(cancellation.is_cancelled());
    }

    #[test]
    fn chunked_budget_accepts_exact_limit_across_multibyte_chunks() {
        let mut text = String::new();
        assert_eq!(append_budgeted_chunk(&mut text, "é", 4), None);
        assert_eq!(append_budgeted_chunk(&mut text, "é", 4), None);
        assert_eq!(text, "éé");
    }

    #[test]
    fn chunked_budget_discards_partial_text_after_first_byte_over() {
        let mut text = String::new();
        assert_eq!(append_budgeted_chunk(&mut text, "abc", 3), None);
        let outcome = append_budgeted_chunk(&mut text, "d", 3);
        assert_eq!(
            outcome,
            Some(BufferSnapshotOutcome::ExceededLimit {
                observed_at_least: 4
            })
        );
        assert!(!matches!(outcome, Some(BufferSnapshotOutcome::Captured(_))));
    }
}
