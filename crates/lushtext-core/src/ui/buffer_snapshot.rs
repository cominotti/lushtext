// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared GTK-buffer snapshot helpers for UI workflows.
//!
//! GtkTextBuffer content can only be read on the GTK thread, so this module
//! belongs in the UI layer. It gives save, draft, preview, and encoding flows a
//! common way to keep large text copies from monopolizing one main-loop turn.

use std::cell::RefCell;
use std::rc::{Rc, Weak};
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

type SnapshotCallback = Box<dyn FnOnce(BufferSnapshotOutcome)>;

/// Result of copying a GTK buffer under a UTF-8 byte budget.
#[derive(Debug, PartialEq, Eq)]
pub enum BufferSnapshotOutcome {
    /// The complete buffer was captured within the configured limit.
    Captured(String),
    /// Capture exceeded the budget; the byte count is a copied lower bound.
    ExceededLimit {
        /// Bytes retained through the first chunk that proved overflow.
        observed_at_least: u64,
    },
    /// The snapshot ended without publishing partial text.
    Cancelled(BufferSnapshotCancelReason),
}

/// Why a chunked snapshot rejected its partial capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferSnapshotCancelReason {
    /// The source buffer changed while capture was yielding to the main loop.
    SourceMutated,
    /// The owning workflow superseded the capture explicitly.
    Superseded,
}

#[derive(Clone, Copy, Debug)]
enum SnapshotBytePolicy {
    Unbounded,
    Limited(u64),
}

/// One lifecycle-owned chunked snapshot on GTK's main thread.
struct ChunkedSnapshotSession {
    buffer: gtk4::TextBuffer,
    progress_mark: Option<gtk4::TextMark>,
    text: String,
    byte_policy: SnapshotBytePolicy,
    cancel_reason: Option<BufferSnapshotCancelReason>,
    changed_handler: Option<glib::SignalHandlerId>,
    scheduled_source: Option<glib::SourceId>,
    callback: Option<SnapshotCallback>,
    terminal: bool,
    slice_count: usize,
    #[cfg(feature = "test-utils")]
    test_mutation: Option<BufferSnapshotTestMutation>,
}

/// Main-thread handle for superseding or disposing a chunked GTK snapshot.
///
/// The session owns the GTK resources; this weak handle lets a consumer cancel
/// without extending the session after its callback and sources are released.
#[derive(Clone, Default)]
pub struct BufferSnapshotHandle(Weak<RefCell<ChunkedSnapshotSession>>);

impl BufferSnapshotHandle {
    /// Stop the next slice and deliver a typed cancellation outcome.
    pub(crate) fn cancel(&self) {
        if let Some(session) = self.0.upgrade() {
            let mut session = session.borrow_mut();
            if !session.terminal && session.cancel_reason.is_none() {
                session.cancel_reason = Some(BufferSnapshotCancelReason::Superseded);
            }
        }
    }

    /// Tear down a disposed owner's snapshot without invoking its callback.
    pub(crate) fn dispose(&self) {
        if let Some(session) = self.0.upgrade() {
            finish_snapshot(&session, None);
        }
    }

    /// Whether the session still owns GTK resources or a terminal callback.
    pub(crate) fn is_active(&self) -> bool {
        self.0
            .upgrade()
            .is_some_and(|session| !session.borrow().terminal)
    }

    #[cfg(feature = "test-utils")]
    pub fn cancel_for_test(&self) {
        self.cancel();
    }

    #[cfg(feature = "test-utils")]
    pub fn dispose_for_test(&self) {
        self.dispose();
    }

    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn state_for_test(&self) -> BufferSnapshotStateForTest {
        let Some(session) = self.0.upgrade() else {
            return BufferSnapshotStateForTest::default();
        };
        let session = session.borrow();
        BufferSnapshotStateForTest {
            active: !session.terminal,
            progress_mark_live: session.progress_mark.is_some(),
            changed_handler_live: session.changed_handler.is_some(),
            scheduled_source_live: session.scheduled_source.is_some(),
            callback_pending: session.callback.is_some(),
            slice_count: session.slice_count,
        }
    }
}

/// Deterministic mutation injected after a selected snapshot slice in widget tests.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferSnapshotTestMutation {
    pub trigger: BufferSnapshotTestTrigger,
    pub edit: BufferSnapshotTestEdit,
}

/// Slice boundary at which a widget test mutates or disposes a snapshot.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferSnapshotTestTrigger {
    AfterSlice(usize),
    FinalSlice,
}

/// Mutation or lifecycle action performed by the deterministic snapshot seam.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferSnapshotTestEdit {
    InsertBeforeMark,
    InsertAfterMark,
    DeleteBeforeMark,
    DeleteAfterMark,
    Dispose,
}

/// Observable ownership state for deterministic widget lifecycle assertions.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferSnapshotStateForTest {
    pub active: bool,
    pub progress_mark_live: bool,
    pub changed_handler_live: bool,
    pub scheduled_source_live: bool,
    pub callback_pending: bool,
    pub slice_count: usize,
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

/// Copy a large buffer in mutation-safe GTK main-loop slices.
///
/// The callback runs exactly once on the GTK thread after capture, mutation,
/// explicit cancellation, or overflow. Disposal performs cleanup silently.
/// Worker-thread I/O or pure analysis should be scheduled from that callback
/// using owned text, never by sending the `Buffer` itself across threads.
pub(crate) fn snapshot_buffer_text_async<F: FnOnce(BufferSnapshotOutcome) + 'static>(
    buffer: impl IsA<gtk4::TextBuffer> + Clone + 'static,
    callback: F,
) -> BufferSnapshotHandle {
    start_chunked_snapshot(
        buffer.upcast::<gtk4::TextBuffer>(),
        SnapshotBytePolicy::Unbounded,
        Box::new(callback),
        #[cfg(feature = "test-utils")]
        None,
    )
}

/// Copy a large buffer in slices while enforcing a UTF-8 byte budget.
///
/// At most one additional 64k-character chunk is retained beyond `max_bytes`.
/// The callback never receives partial text after cancellation or overflow.
pub(crate) fn snapshot_buffer_text_async_budgeted<F: FnOnce(BufferSnapshotOutcome) + 'static>(
    buffer: impl IsA<gtk4::TextBuffer> + Clone + 'static,
    max_bytes: u64,
    callback: F,
) -> BufferSnapshotHandle {
    start_chunked_snapshot(
        buffer.upcast::<gtk4::TextBuffer>(),
        SnapshotBytePolicy::Limited(max_bytes),
        Box::new(callback),
        #[cfg(feature = "test-utils")]
        None,
    )
}

#[cfg(feature = "test-utils")]
pub fn snapshot_buffer_text_async_for_test<F: FnOnce(BufferSnapshotOutcome) + 'static>(
    buffer: gtk4::TextBuffer,
    max_bytes: Option<u64>,
    mutation: Option<BufferSnapshotTestMutation>,
    callback: F,
) -> BufferSnapshotHandle {
    start_chunked_snapshot(
        buffer,
        max_bytes.map_or(SnapshotBytePolicy::Unbounded, SnapshotBytePolicy::Limited),
        Box::new(callback),
        mutation,
    )
}

fn start_chunked_snapshot(
    buffer: gtk4::TextBuffer,
    byte_policy: SnapshotBytePolicy,
    callback: SnapshotCallback,
    #[cfg(feature = "test-utils")] test_mutation: Option<BufferSnapshotTestMutation>,
) -> BufferSnapshotHandle {
    let progress_mark = buffer.create_mark(None, &buffer.start_iter(), true);
    let signal_buffer = buffer.clone();
    let session = Rc::new(RefCell::new(ChunkedSnapshotSession {
        buffer,
        progress_mark: Some(progress_mark),
        text: String::new(),
        byte_policy,
        cancel_reason: None,
        changed_handler: None,
        scheduled_source: None,
        callback: Some(callback),
        terminal: false,
        slice_count: 0,
        #[cfg(feature = "test-utils")]
        test_mutation,
    }));

    let session_weak = Rc::downgrade(&session);
    let handler = signal_buffer.connect_changed(move |_| {
        let Some(session) = session_weak.upgrade() else {
            return;
        };
        let mut session = session.borrow_mut();
        if !session.terminal && session.cancel_reason.is_none() {
            // GtkTextIter becomes invalid after character-count changes. The
            // signal records cancellation synchronously; terminal delivery is
            // deferred to the slice path to avoid workflow reentrancy here.
            session.cancel_reason = Some(BufferSnapshotCancelReason::SourceMutated);
        }
    });
    session.borrow_mut().changed_handler = Some(handler);

    let handle = BufferSnapshotHandle(Rc::downgrade(&session));
    run_snapshot_slice(&session);
    handle
}

fn run_snapshot_slice(session: &Rc<RefCell<ChunkedSnapshotSession>>) {
    let (buffer, mark, cancel_reason) = {
        let mut state = session.borrow_mut();
        state.scheduled_source.take();
        if state.terminal {
            return;
        }
        (
            state.buffer.clone(),
            state.progress_mark.clone(),
            state.cancel_reason,
        )
    };
    if let Some(reason) = cancel_reason {
        finish_snapshot(session, Some(BufferSnapshotOutcome::Cancelled(reason)));
        return;
    }
    let Some(mark) = mark else {
        finish_snapshot(
            session,
            Some(BufferSnapshotOutcome::Cancelled(
                BufferSnapshotCancelReason::Superseded,
            )),
        );
        return;
    };

    // GTK 4 invalidates every outstanding iterator after an indexable content
    // mutation. The mark is the cross-turn position; both iterators below are
    // local to this slice and are discarded before yielding.
    let start = buffer.iter_at_mark(&mark);
    let mut end = start;
    if !end.forward_chars(BUFFER_SNAPSHOT_CHUNK_CHARS) {
        end = buffer.end_iter();
    }
    let reached_end = end == buffer.end_iter();
    let chunk = buffer.text(&start, &end, true);
    buffer.move_mark(&mark, &end);

    let overflow = {
        let mut state = session.borrow_mut();
        state.slice_count += 1;
        match state.byte_policy {
            SnapshotBytePolicy::Unbounded => {
                state.text.push_str(chunk.as_str());
                None
            }
            SnapshotBytePolicy::Limited(max_bytes) => {
                append_budgeted_chunk(&mut state.text, chunk.as_str(), max_bytes)
            }
        }
    };

    #[cfg(feature = "test-utils")]
    apply_test_mutation(session, reached_end);

    let cancel_reason = session.borrow().cancel_reason;
    if let Some(reason) = cancel_reason {
        finish_snapshot(session, Some(BufferSnapshotOutcome::Cancelled(reason)));
        return;
    }
    if let Some(outcome) = overflow {
        finish_snapshot(session, Some(outcome));
        return;
    }
    if reached_end {
        let text = std::mem::take(&mut session.borrow_mut().text);
        finish_snapshot(session, Some(BufferSnapshotOutcome::Captured(text)));
        return;
    }

    // One millisecond gives GTK a scheduling point without materially slowing
    // a many-megabyte capture. The source ID stays session-owned so disposal
    // removes it immediately rather than leaving a stale callback queued.
    let session_for_source = Rc::clone(session);
    let source_id = glib::timeout_add_local_once(Duration::from_millis(1), move || {
        run_snapshot_slice(&session_for_source);
    });
    session.borrow_mut().scheduled_source = Some(source_id);
}

fn finish_snapshot(
    session: &Rc<RefCell<ChunkedSnapshotSession>>,
    outcome: Option<BufferSnapshotOutcome>,
) {
    let (buffer, discarded_text, mark, handler, source, callback) = {
        let mut state = session.borrow_mut();
        if state.terminal {
            return;
        }
        state.terminal = true;
        (
            state.buffer.clone(),
            std::mem::take(&mut state.text),
            state.progress_mark.take(),
            state.changed_handler.take(),
            state.scheduled_source.take(),
            state.callback.take(),
        )
    };

    if let Some(source) = source {
        source.remove();
    }
    if let Some(handler) = handler {
        buffer.disconnect(handler);
    }
    if let Some(mark) = mark {
        buffer.delete_mark(&mark);
    }
    // Cancellation and overflow may leave a multi-megabyte allocation behind.
    // Release it before a terminal callback can synchronously start a retry.
    drop(discarded_text);
    if let (Some(outcome), Some(callback)) = (outcome, callback) {
        callback(outcome);
    }
}

#[cfg(feature = "test-utils")]
fn apply_test_mutation(session: &Rc<RefCell<ChunkedSnapshotSession>>, reached_end: bool) {
    let mutation = {
        let mut state = session.borrow_mut();
        let should_run = state
            .test_mutation
            .is_some_and(|mutation| match mutation.trigger {
                BufferSnapshotTestTrigger::AfterSlice(slice) => slice == state.slice_count,
                BufferSnapshotTestTrigger::FinalSlice => reached_end,
            });
        should_run.then(|| state.test_mutation.take()).flatten()
    };
    let Some(mutation) = mutation else {
        return;
    };
    if mutation.edit == BufferSnapshotTestEdit::Dispose {
        finish_snapshot(session, None);
        return;
    }

    let (buffer, mark) = {
        let state = session.borrow();
        (state.buffer.clone(), state.progress_mark.clone())
    };
    let Some(mark) = mark else {
        return;
    };
    match mutation.edit {
        BufferSnapshotTestEdit::InsertBeforeMark => {
            let mut iter = buffer.start_iter();
            buffer.insert(&mut iter, "before");
        }
        BufferSnapshotTestEdit::InsertAfterMark => {
            let mut iter = buffer.end_iter();
            buffer.insert(&mut iter, "after");
        }
        BufferSnapshotTestEdit::DeleteBeforeMark => {
            let mut start = buffer.start_iter();
            let mut end = start;
            if end.forward_char() {
                buffer.delete(&mut start, &mut end);
            }
        }
        BufferSnapshotTestEdit::DeleteAfterMark => {
            let mut start = buffer.iter_at_mark(&mark);
            let mut end = start;
            if end.forward_char() {
                buffer.delete(&mut start, &mut end);
            }
        }
        BufferSnapshotTestEdit::Dispose => unreachable!(),
    }
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
