// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared GTK-buffer snapshot helpers for UI workflows.
//!
//! GtkTextBuffer content can only be read on the GTK thread, so this module
//! belongs in the UI layer. It gives save, draft, preview, and encoding flows a
//! common way to keep large text copies from monopolizing one main-loop turn.

use std::cell::RefCell;
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

type ChunkedCallback = Rc<RefCell<Option<Box<dyn FnOnce(String)>>>>;

/// Decide whether a character count is large enough to require chunked capture.
#[must_use]
pub(crate) fn char_count_requires_chunked_snapshot(char_count: i32) -> bool {
    let char_count = u64::try_from(char_count).unwrap_or(u64::MAX);
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
}
