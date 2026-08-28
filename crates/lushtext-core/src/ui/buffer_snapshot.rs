// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared GTK-buffer snapshot helpers for UI workflows.
//!
//! GtkTextBuffer content can only be read on the GTK thread, so this module
//! belongs in the UI layer. It gives save, draft, preview, and encoding flows a
//! common way to keep large text copies from monopolizing one main-loop turn.
//!
//! # This is a cross-cutting lane, not a workflow
//!
//! Five workflows call it — save, draft autosave, encoding analysis, Markdown
//! preview, and local history — and it has no user-initiated operation of its
//! own. So it owes **no facade, no stage narration, and no role names**, and its
//! matrix status stays `cross-cutting`. It does owe the **evidence surface**
//! rules, because those follow from "one accessor reads the whole surface" plus
//! interior mutability and do not depend on being a workflow.
//!
//! ## The one observation path
//!
//! [`BufferSnapshotEvidence`] is that surface. It replaced **three parallel
//! typed observation types** — `BufferSnapshotMetrics`,
//! `BufferSnapshotStateForTest`, and `BufferSnapshotCountersForTest` — which is
//! the duplication the evidence-surface rules forbid, and which no migration
//! event would ever have fired for, because this lane will never migrate. The
//! three became **named components** of one surface rather than peers, and the
//! five capture-metric fields that were declared in *two* of them are now
//! declared once, in [`BufferSnapshotCaptureMetrics`].
//!
//! Reading is side-effect free and takes no mutable borrow: every field is
//! copied out under a shared borrow, and the accessor answers honestly for a
//! session that has already reached its terminal or been dropped.
//!
//! ## What is deliberately *not* part of the surface
//!
//! * [`BufferSnapshotTestMutation`] and its trigger/edit enums are a **test-only
//!   mutation injector** — a configuration and actuation seam, not an
//!   observation. They make a mid-capture edit, dispose, or pause happen at a
//!   chosen slice boundary; they report nothing. Classifying them as a fourth
//!   observation path would have been wrong, and leaving them unclassified would
//!   have left them looking like one beside the new surface.
//! * `coalesce_snapshot_payload_for_test` **consumes** a payload rather than
//!   observing it, so it is an actuator and stays a separate function.
//! * [`char_count_requires_chunked_snapshot`] is a **shared limit**: the save
//!   workflow calls it and slot 3a deliberately did not fork it into save
//!   policy. Consolidating the surface must not move or duplicate it, and does
//!   not.

use std::cell::RefCell;
use std::fmt;
use std::mem::size_of;
use std::rc::{Rc, Weak};
#[cfg(feature = "test-utils")]
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::plain_disposal::{
    DisposalCapacityWakeup, DisposalOwned, DisposalReservation, ProgressDisposalCapacityWakeup,
    disposal_capacity_epoch, progress_disposal_capacity_epoch, try_reserve_for_gtk,
    try_reserve_progress_for_gtk,
};

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
/// Maximum UTF-8 bytes copied from GTK in one character-aligned slice.
const BUFFER_SNAPSHOT_CHUNK_MAX_BYTES: u64 = 4 * BUFFER_SNAPSHOT_CHUNK_CHARS as u64;
#[cfg(feature = "test-utils")]
const BUFFER_SNAPSHOT_TEST_PAUSE_TIMEOUT: Duration = Duration::from_secs(60);

#[cfg(feature = "test-utils")]
static SNAPSHOT_WORKER_COALESCES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static SNAPSHOT_GTK_COALESCES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static SNAPSHOT_WORKER_DROPS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static SNAPSHOT_GTK_DROPS: AtomicU64 = AtomicU64::new(0);

type SnapshotCallback = Box<dyn FnOnce(BufferSnapshotOutcome)>;

/// Result of copying a GTK buffer under a UTF-8 byte budget.
#[derive(Debug, PartialEq, Eq)]
pub enum BufferSnapshotOutcome {
    /// The complete buffer was captured within the configured limit.
    Captured(BufferSnapshotPayload),
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

/// Lane used to pre-admit final destruction before the first GTK slice.
#[derive(Clone, Copy, Debug)]
pub(crate) enum BufferSnapshotAdmissionLane {
    Ordinary,
    Progress,
}

/// Pre-reserved chunk ownership acquired before a recovery workflow starts capture.
pub(crate) struct BufferSnapshotAdmission {
    reservation: DisposalReservation,
    chunk_capacity: usize,
}

impl BufferSnapshotAdmission {
    /// Attach a synchronous small-path copy to the same future worker-drop guard.
    pub(crate) fn own_direct(mut self, text: String) -> BufferSnapshotPayload {
        let byte_len = u64::try_from(text.len()).unwrap_or(u64::MAX);
        let chunks = vec![text];
        #[cfg(feature = "test-utils")]
        let metrics = BufferSnapshotCaptureMetrics {
            slice_count: 1,
            chunk_count: 1,
            reserved_chunk_capacity: chunks.capacity(),
            max_chunk_bytes: chunks.first().map_or(0, String::len),
            captured_bytes: byte_len,
        };
        let retained_weight =
            byte_len.saturating_add(u64::try_from(size_of::<String>()).unwrap_or(u64::MAX));
        self.reservation.shrink_to(retained_weight);
        BufferSnapshotPayload {
            storage: BufferSnapshotStorage::Chunked(self.reservation.own(SnapshotChunks {
                chunks,
                byte_len,
                #[cfg(feature = "test-utils")]
                metrics,
            })),
        }
    }
}

/// Scalar capture metrics, carried without exposing document text.
///
/// A **component** of [`BufferSnapshotEvidence`], not an observation path of its
/// own. These five fields were previously declared twice — here and inline in
/// the session state type — which is exactly the duplication one surface
/// removes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferSnapshotCaptureMetrics {
    /// Main-loop turns the capture has consumed.
    pub slice_count: usize,
    /// Independently allocated UTF-8 chunks held so far.
    pub chunk_count: usize,
    /// Capacity reserved for the chunk vector.
    pub reserved_chunk_capacity: usize,
    /// Largest single chunk, in bytes.
    pub max_chunk_bytes: usize,
    /// Total bytes observed by the capture.
    pub captured_bytes: u64,
}

struct SnapshotChunks {
    chunks: Vec<String>,
    byte_len: u64,
    #[cfg(feature = "test-utils")]
    metrics: BufferSnapshotCaptureMetrics,
}

impl SnapshotChunks {
    fn coalesce(mut self) -> String {
        #[cfg(feature = "test-utils")]
        if glib::MainContext::default().is_owner() {
            SNAPSHOT_GTK_COALESCES.fetch_add(1, Ordering::AcqRel);
        } else {
            SNAPSHOT_WORKER_COALESCES.fetch_add(1, Ordering::AcqRel);
        }
        let mut text = String::with_capacity(usize::try_from(self.byte_len).unwrap_or(usize::MAX));
        for chunk in std::mem::take(&mut self.chunks) {
            text.push_str(&chunk);
        }
        text
    }
}

impl Drop for SnapshotChunks {
    fn drop(&mut self) {
        #[cfg(feature = "test-utils")]
        if glib::MainContext::default().is_owner() {
            SNAPSHOT_GTK_DROPS.fetch_add(1, Ordering::AcqRel);
        } else {
            SNAPSHOT_WORKER_DROPS.fetch_add(1, Ordering::AcqRel);
        }
    }
}

enum BufferSnapshotStorage {
    Direct(String),
    Chunked(DisposalOwned<SnapshotChunks>),
}

/// Complete snapshot ownership that cannot accidentally coalesce a large body on GTK.
pub struct BufferSnapshotPayload {
    storage: BufferSnapshotStorage,
}

impl BufferSnapshotPayload {
    #[must_use]
    pub(crate) fn direct(text: String) -> Self {
        Self {
            storage: BufferSnapshotStorage::Direct(text),
        }
    }

    /// Coalesce independent chunks after the caller has crossed to its worker lane.
    #[must_use]
    pub(crate) fn into_string_on_worker(self) -> String {
        match self.storage {
            BufferSnapshotStorage::Direct(text) => text,
            BufferSnapshotStorage::Chunked(chunks) => chunks.into_inner_on_worker().coalesce(),
        }
    }

    /// Consume the established synchronous small-buffer path.
    #[must_use]
    pub(crate) fn into_direct_string(self) -> String {
        match self.storage {
            BufferSnapshotStorage::Direct(text) => text,
            BufferSnapshotStorage::Chunked(_) => {
                panic!("chunked snapshot must be coalesced on a worker")
            }
        }
    }

    /// Coalesce on a worker while preserving the future off-GTK disposal reservation.
    #[must_use]
    pub(crate) fn into_guarded_string_on_worker(self) -> DisposalOwned<String> {
        match self.storage {
            BufferSnapshotStorage::Direct(text) => DisposalOwned::small_unreserved(text),
            BufferSnapshotStorage::Chunked(chunks) => {
                chunks.map_preserving_reservation(SnapshotChunks::coalesce)
            }
        }
    }

    #[must_use]
    pub(crate) fn byte_len(&self) -> u64 {
        match &self.storage {
            BufferSnapshotStorage::Direct(text) => u64::try_from(text.len()).unwrap_or(u64::MAX),
            BufferSnapshotStorage::Chunked(chunks) => chunks.byte_len,
        }
    }

    /// The capture metrics for this completed payload.
    ///
    /// Reads a component of the lane's surface rather than being a second
    /// observation path: a payload is a finished capture, so it has metrics but
    /// no live session or process counters.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn capture_metrics(&self) -> BufferSnapshotCaptureMetrics {
        match &self.storage {
            BufferSnapshotStorage::Direct(text) => BufferSnapshotCaptureMetrics {
                slice_count: 1,
                chunk_count: 1,
                reserved_chunk_capacity: 1,
                max_chunk_bytes: text.len(),
                captured_bytes: u64::try_from(text.len()).unwrap_or(u64::MAX),
            },
            BufferSnapshotStorage::Chunked(chunks) => chunks.metrics,
        }
    }

    fn bytes_equal(&self, other: &Self) -> bool {
        match (&self.storage, &other.storage) {
            (BufferSnapshotStorage::Direct(left), BufferSnapshotStorage::Direct(right)) => {
                left == right
            }
            (BufferSnapshotStorage::Chunked(left), BufferSnapshotStorage::Chunked(right)) => {
                left.chunks == right.chunks
            }
            (BufferSnapshotStorage::Direct(direct), BufferSnapshotStorage::Chunked(chunked))
            | (BufferSnapshotStorage::Chunked(chunked), BufferSnapshotStorage::Direct(direct)) => {
                direct
                    .as_bytes()
                    .iter()
                    .copied()
                    .eq(chunked.chunks.iter().flat_map(|chunk| chunk.bytes()))
            }
        }
    }
}

impl fmt::Debug for BufferSnapshotPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BufferSnapshotPayload")
            .field("byte_len", &self.byte_len())
            .finish_non_exhaustive()
    }
}

impl PartialEq for BufferSnapshotPayload {
    fn eq(&self, other: &Self) -> bool {
        self.bytes_equal(other)
    }
}

impl Eq for BufferSnapshotPayload {}

/// Process-wide handoff counters proving which thread coalesced and dropped.
///
/// A **component** of [`BufferSnapshotEvidence`]. Process-wide rather than
/// per-session, which is why the surface carries it beside an optional session
/// rather than inside one: the whole point is to prove that a document-sized
/// body was coalesced and destroyed off the GTK thread even after the session
/// that produced it is gone.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferSnapshotHandoffCounters {
    /// Bodies coalesced on a worker thread.
    pub worker_coalesces: u64,
    /// Bodies coalesced on the GTK thread.
    pub gtk_coalesces: u64,
    /// Bodies destroyed on a worker thread.
    pub worker_drops: u64,
    /// Bodies destroyed on the GTK thread.
    pub gtk_drops: u64,
}

/// The whole observable state of the snapshot lane.
///
/// One accessor reads all of it — see [`buffer_snapshot_evidence`]. Both
/// components answer honestly in every stage: `session` is `None` when no
/// capture is live or the session has been dropped, and `handoff` is always
/// readable because its counters outlive any session.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferSnapshotEvidence {
    /// The live chunked capture, if one is still owned.
    pub session: Option<BufferSnapshotSessionEvidence>,
    /// Process-wide worker/GTK handoff counters.
    pub handoff: BufferSnapshotHandoffCounters,
}

/// Read the lane's whole surface, optionally including one session's state.
///
/// Pass `None` to read the process-wide counters alone, which is the only thing
/// observable before a capture starts or after every session has ended.
#[cfg(feature = "test-utils")]
#[must_use]
pub fn buffer_snapshot_evidence(session: Option<&BufferSnapshotHandle>) -> BufferSnapshotEvidence {
    BufferSnapshotEvidence {
        session: session.and_then(BufferSnapshotHandle::session_evidence),
        handoff: BufferSnapshotHandoffCounters {
            worker_coalesces: SNAPSHOT_WORKER_COALESCES.load(Ordering::Acquire),
            gtk_coalesces: SNAPSHOT_GTK_COALESCES.load(Ordering::Acquire),
            worker_drops: SNAPSHOT_WORKER_DROPS.load(Ordering::Acquire),
            gtk_drops: SNAPSHOT_GTK_DROPS.load(Ordering::Acquire),
        },
    }
}

/// Consume a payload the way a worker would, for handoff assertions.
///
/// An **actuator**, not an observation: it destroys the payload. Kept separate
/// from the surface for that reason.
#[cfg(feature = "test-utils")]
#[must_use]
pub fn coalesce_snapshot_payload_for_test(payload: BufferSnapshotPayload) -> String {
    payload.into_string_on_worker()
}

#[derive(Clone, Copy, Debug)]
enum SnapshotBytePolicy {
    Unbounded,
    Limited(u64),
}

enum SnapshotCapacityWakeup {
    Ordinary(DisposalCapacityWakeup),
    Progress(ProgressDisposalCapacityWakeup),
}

impl SnapshotCapacityWakeup {
    fn new(lane: BufferSnapshotAdmissionLane) -> Self {
        match lane {
            BufferSnapshotAdmissionLane::Ordinary => {
                Self::Ordinary(DisposalCapacityWakeup::default())
            }
            BufferSnapshotAdmissionLane::Progress => {
                Self::Progress(ProgressDisposalCapacityWakeup::default())
            }
        }
    }

    fn arm(&self, observed_epoch: u64, callback: impl FnOnce() + 'static) {
        match self {
            Self::Ordinary(wakeup) => wakeup.arm(observed_epoch, callback),
            Self::Progress(wakeup) => wakeup.arm(observed_epoch, callback),
        }
    }

    fn cancel(&self) {
        match self {
            Self::Ordinary(wakeup) => wakeup.cancel(),
            Self::Progress(wakeup) => wakeup.cancel(),
        }
    }

    #[cfg(feature = "test-utils")]
    fn is_armed(&self) -> bool {
        match self {
            Self::Ordinary(wakeup) => wakeup.is_armed(),
            Self::Progress(wakeup) => wakeup.is_armed(),
        }
    }
}

/// One lifecycle-owned chunked snapshot on GTK's main thread.
struct ChunkedSnapshotSession {
    buffer: gtk4::TextBuffer,
    progress_mark: Option<gtk4::TextMark>,
    chunks: Vec<String>,
    observed_bytes: u64,
    reservation: Option<DisposalReservation>,
    byte_policy: SnapshotBytePolicy,
    admission_lane: BufferSnapshotAdmissionLane,
    capacity_wakeup: SnapshotCapacityWakeup,
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
            let should_schedule = {
                let mut state = session.borrow_mut();
                if state.terminal || state.cancel_reason.is_some() {
                    return;
                }
                state.cancel_reason = Some(BufferSnapshotCancelReason::Superseded);
                let awaiting_admission = state.reservation.is_none();
                if awaiting_admission {
                    state.capacity_wakeup.cancel();
                }
                awaiting_admission && state.scheduled_source.is_none()
            };
            if should_schedule {
                schedule_snapshot_slice(&session);
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
    /// Replace a paused test source with an immediate continuation.
    pub fn resume_for_test(&self) {
        let Some(session) = self.0.upgrade() else {
            return;
        };
        let should_schedule = {
            let mut state = session.borrow_mut();
            if state.terminal || state.reservation.is_none() {
                false
            } else {
                if let Some(source) = state.scheduled_source.take() {
                    source.remove();
                }
                true
            }
        };
        if should_schedule {
            schedule_snapshot_slice(&session);
        }
    }

    /// This session's component of the lane's evidence surface.
    ///
    /// `None` once the session has been dropped, which is the honest answer
    /// rather than a zeroed record that reads like a live idle capture. Every
    /// field is copied out under one shared borrow that is released before the
    /// value is returned, so no field can be read from inside a mutable borrow
    /// of the same state.
    #[cfg(feature = "test-utils")]
    #[must_use]
    fn session_evidence(&self) -> Option<BufferSnapshotSessionEvidence> {
        let session = self.0.upgrade()?;
        let session = session.borrow();
        Some(BufferSnapshotSessionEvidence {
            active: !session.terminal,
            progress_mark_live: session.progress_mark.is_some(),
            changed_handler_live: session.changed_handler.is_some(),
            scheduled_source_live: session.scheduled_source.is_some(),
            admission_retry_source_live: session.capacity_wakeup.is_armed(),
            callback_pending: session.callback.is_some(),
            capture: BufferSnapshotCaptureMetrics {
                slice_count: session.slice_count,
                chunk_count: session.chunks.len(),
                reserved_chunk_capacity: session.chunks.capacity(),
                max_chunk_bytes: session.chunks.iter().map(String::len).max().unwrap_or(0),
                captured_bytes: session.observed_bytes,
            },
        })
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
    Pause,
}

/// One live chunked capture's ownership state and capture metrics.
///
/// A **component** of [`BufferSnapshotEvidence`]. The capture metrics are
/// embedded rather than repeated: this type previously re-declared all five of
/// them, so the same fact had two declarations and could disagree with itself.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferSnapshotSessionEvidence {
    /// Whether the session has not yet reached its terminal.
    pub active: bool,
    /// Whether the progress mark is still installed in the buffer.
    pub progress_mark_live: bool,
    /// Whether the buffer-changed guard handler is still connected.
    pub changed_handler_live: bool,
    /// Whether a slice is scheduled on the main loop.
    pub scheduled_source_live: bool,
    /// Whether the session is waiting on disposal admission capacity.
    pub admission_retry_source_live: bool,
    /// Whether the terminal callback has yet to run.
    pub callback_pending: bool,
    /// Scalar capture metrics for this session.
    pub capture: BufferSnapshotCaptureMetrics,
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
        BufferSnapshotOutcome::Captured(BufferSnapshotPayload::direct(text))
    }
}

/// Charge one independently allocated character-aligned chunk and report overflow.
fn classify_chunk_bytes(
    observed_bytes: &mut u64,
    chunk_bytes: usize,
    max_bytes: u64,
) -> Option<BufferSnapshotOutcome> {
    *observed_bytes = observed_bytes.saturating_add(u64::try_from(chunk_bytes).unwrap_or(u64::MAX));
    let observed_at_least = *observed_bytes;
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
        BufferSnapshotAdmissionLane::Ordinary,
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
        BufferSnapshotAdmissionLane::Ordinary,
        Box::new(callback),
        #[cfg(feature = "test-utils")]
        None,
    )
}

/// Try to reserve progress-lane ownership without copying any buffer text.
pub(crate) fn try_admit_progress_snapshot(
    buffer: &impl IsA<gtk4::TextBuffer>,
    max_bytes: u64,
) -> Option<BufferSnapshotAdmission> {
    try_snapshot_admission(
        buffer.char_count(),
        SnapshotBytePolicy::Limited(max_bytes),
        BufferSnapshotAdmissionLane::Progress,
    )
}

/// Start a progress-lane capture from an already accepted scalar admission.
pub(crate) fn snapshot_buffer_text_async_progress_budgeted_admitted<
    F: FnOnce(BufferSnapshotOutcome) + 'static,
>(
    buffer: impl IsA<gtk4::TextBuffer> + Clone + 'static,
    max_bytes: u64,
    admission: BufferSnapshotAdmission,
    callback: F,
) -> BufferSnapshotHandle {
    start_chunked_snapshot_admitted(
        buffer.upcast::<gtk4::TextBuffer>(),
        SnapshotBytePolicy::Limited(max_bytes),
        admission,
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
        BufferSnapshotAdmissionLane::Ordinary,
        Box::new(callback),
        mutation,
    )
}

fn start_chunked_snapshot(
    buffer: gtk4::TextBuffer,
    byte_policy: SnapshotBytePolicy,
    admission_lane: BufferSnapshotAdmissionLane,
    callback: SnapshotCallback,
    #[cfg(feature = "test-utils")] test_mutation: Option<BufferSnapshotTestMutation>,
) -> BufferSnapshotHandle {
    let session = new_chunked_snapshot_session(
        buffer,
        byte_policy,
        admission_lane,
        callback,
        #[cfg(feature = "test-utils")]
        test_mutation,
    );
    let handle = BufferSnapshotHandle(Rc::downgrade(&session));
    retry_snapshot_admission(&session);
    handle
}

fn start_chunked_snapshot_admitted(
    buffer: gtk4::TextBuffer,
    byte_policy: SnapshotBytePolicy,
    admission: BufferSnapshotAdmission,
    callback: SnapshotCallback,
    #[cfg(feature = "test-utils")] test_mutation: Option<BufferSnapshotTestMutation>,
) -> BufferSnapshotHandle {
    let session = new_chunked_snapshot_session(
        buffer,
        byte_policy,
        BufferSnapshotAdmissionLane::Progress,
        callback,
        #[cfg(feature = "test-utils")]
        test_mutation,
    );
    let handle = BufferSnapshotHandle(Rc::downgrade(&session));
    activate_snapshot_session(&session, admission);
    handle
}

fn new_chunked_snapshot_session(
    buffer: gtk4::TextBuffer,
    byte_policy: SnapshotBytePolicy,
    admission_lane: BufferSnapshotAdmissionLane,
    callback: SnapshotCallback,
    #[cfg(feature = "test-utils")] test_mutation: Option<BufferSnapshotTestMutation>,
) -> Rc<RefCell<ChunkedSnapshotSession>> {
    Rc::new(RefCell::new(ChunkedSnapshotSession {
        buffer,
        progress_mark: None,
        chunks: Vec::new(),
        observed_bytes: 0,
        reservation: None,
        byte_policy,
        admission_lane,
        capacity_wakeup: SnapshotCapacityWakeup::new(admission_lane),
        cancel_reason: None,
        changed_handler: None,
        scheduled_source: None,
        callback: Some(callback),
        terminal: false,
        slice_count: 0,
        #[cfg(feature = "test-utils")]
        test_mutation,
    }))
}

fn retry_snapshot_admission(session: &Rc<RefCell<ChunkedSnapshotSession>>) {
    let (char_count, byte_policy, admission_lane, cancel_reason, terminal) = {
        let state = session.borrow();
        (
            state.buffer.char_count(),
            state.byte_policy,
            state.admission_lane,
            state.cancel_reason,
            state.terminal,
        )
    };
    if terminal {
        return;
    }
    if let Some(reason) = cancel_reason {
        finish_snapshot(session, Some(BufferSnapshotOutcome::Cancelled(reason)));
        return;
    }

    let observed_epoch = match admission_lane {
        BufferSnapshotAdmissionLane::Ordinary => disposal_capacity_epoch(),
        BufferSnapshotAdmissionLane::Progress => progress_disposal_capacity_epoch(),
    };
    if let Some(admission) = try_snapshot_admission(char_count, byte_policy, admission_lane) {
        activate_snapshot_session(session, admission);
        return;
    }

    let session_for_wakeup = Rc::clone(session);
    session
        .borrow()
        .capacity_wakeup
        .arm(observed_epoch, move || {
            retry_snapshot_admission(&session_for_wakeup);
        });
}

fn activate_snapshot_session(
    session: &Rc<RefCell<ChunkedSnapshotSession>>,
    admission: BufferSnapshotAdmission,
) {
    let BufferSnapshotAdmission {
        reservation,
        chunk_capacity,
    } = admission;
    let signal_buffer = session.borrow().buffer.clone();
    let progress_mark = signal_buffer.create_mark(None, &signal_buffer.start_iter(), true);
    {
        let mut state = session.borrow_mut();
        debug_assert!(state.reservation.is_none());
        state.capacity_wakeup.cancel();
        state.progress_mark = Some(progress_mark);
        state.chunks = Vec::with_capacity(chunk_capacity);
        state.reservation = Some(reservation);
    }

    let session_weak = Rc::downgrade(session);
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

    run_snapshot_slice(session);
}

fn schedule_snapshot_slice(session: &Rc<RefCell<ChunkedSnapshotSession>>) {
    let session_for_source = Rc::clone(session);
    let source_id = glib::idle_add_local_once(move || {
        run_snapshot_slice(&session_for_source);
    });
    session.borrow_mut().scheduled_source = Some(source_id);
}

fn try_snapshot_admission(
    char_count: i32,
    byte_policy: SnapshotBytePolicy,
    admission_lane: BufferSnapshotAdmissionLane,
) -> Option<BufferSnapshotAdmission> {
    let (reservation_weight, chunk_capacity) = snapshot_allocation_plan(char_count, byte_policy);
    let reservation = match admission_lane {
        BufferSnapshotAdmissionLane::Ordinary => try_reserve_for_gtk(reservation_weight),
        BufferSnapshotAdmissionLane::Progress => try_reserve_progress_for_gtk(reservation_weight),
    }?;
    Some(BufferSnapshotAdmission {
        reservation,
        chunk_capacity,
    })
}

fn snapshot_allocation_plan(char_count: i32, byte_policy: SnapshotBytePolicy) -> (u64, usize) {
    let characters = u64::try_from(char_count).unwrap_or(u64::MAX);
    let chunk_chars = u64::try_from(BUFFER_SNAPSHOT_CHUNK_CHARS).unwrap_or(u64::MAX);
    let chunk_count = characters
        .saturating_add(chunk_chars.saturating_sub(1))
        .checked_div(chunk_chars)
        .unwrap_or(u64::MAX)
        .max(1);
    let worst_case_bytes = characters.saturating_mul(4);
    let retained_bytes = match byte_policy {
        SnapshotBytePolicy::Unbounded => worst_case_bytes,
        SnapshotBytePolicy::Limited(max_bytes) => {
            worst_case_bytes.min(max_bytes.saturating_add(BUFFER_SNAPSHOT_CHUNK_MAX_BYTES))
        }
    };
    let header_bytes =
        chunk_count.saturating_mul(u64::try_from(size_of::<String>()).unwrap_or(u64::MAX));
    (
        retained_bytes.saturating_add(header_bytes),
        usize::try_from(chunk_count).unwrap_or(usize::MAX),
    )
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
        let chunk = chunk.to_string();
        let chunk_bytes = chunk.len();
        let overflow = match state.byte_policy {
            SnapshotBytePolicy::Unbounded => {
                state.observed_bytes = state
                    .observed_bytes
                    .saturating_add(u64::try_from(chunk_bytes).unwrap_or(u64::MAX));
                None
            }
            SnapshotBytePolicy::Limited(max_bytes) => {
                classify_chunk_bytes(&mut state.observed_bytes, chunk_bytes, max_bytes)
            }
        };
        state.chunks.push(chunk);
        overflow
    };

    #[cfg(feature = "test-utils")]
    let paused = apply_test_mutation(session, reached_end);

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
        finish_captured_snapshot(session);
        return;
    }
    #[cfg(feature = "test-utils")]
    if paused {
        // The public lifecycle handle is intentionally weak. Retain the paused
        // test session through a removable source just like an ordinary slice.
        let session_for_source = Rc::clone(session);
        let source_id =
            glib::timeout_add_local_once(BUFFER_SNAPSHOT_TEST_PAUSE_TIMEOUT, move || {
                run_snapshot_slice(&session_for_source);
            });
        session.borrow_mut().scheduled_source = Some(source_id);
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
    finish_snapshot_inner(session, outcome, false);
}

fn finish_captured_snapshot(session: &Rc<RefCell<ChunkedSnapshotSession>>) {
    finish_snapshot_inner(session, None, true);
}

fn finish_snapshot_inner(
    session: &Rc<RefCell<ChunkedSnapshotSession>>,
    outcome: Option<BufferSnapshotOutcome>,
    captured: bool,
) {
    session.borrow().capacity_wakeup.cancel();
    let (buffer, chunks, observed_bytes, reservation, mark, handler, source, callback, metrics) = {
        let mut state = session.borrow_mut();
        if state.terminal {
            return;
        }
        state.terminal = true;
        let metrics = BufferSnapshotCaptureMetrics {
            slice_count: state.slice_count,
            chunk_count: state.chunks.len(),
            reserved_chunk_capacity: state.chunks.capacity(),
            max_chunk_bytes: state.chunks.iter().map(String::len).max().unwrap_or(0),
            captured_bytes: state.observed_bytes,
        };
        (
            state.buffer.clone(),
            std::mem::take(&mut state.chunks),
            state.observed_bytes,
            state.reservation.take(),
            state.progress_mark.take(),
            state.changed_handler.take(),
            state.scheduled_source.take(),
            state.callback.take(),
            metrics,
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
    let Some(mut reservation) = reservation else {
        debug_assert!(!captured);
        debug_assert!(chunks.is_empty());
        if let (Some(outcome), Some(callback)) = (outcome, callback) {
            callback(outcome);
        }
        return;
    };
    let retained_weight = observed_bytes.saturating_add(
        u64::try_from(metrics.reserved_chunk_capacity)
            .unwrap_or(u64::MAX)
            .saturating_mul(u64::try_from(size_of::<String>()).unwrap_or(u64::MAX)),
    );
    reservation.shrink_to(retained_weight);
    let guarded = reservation.own(SnapshotChunks {
        chunks,
        byte_len: observed_bytes,
        #[cfg(feature = "test-utils")]
        metrics,
    });
    if captured {
        if let Some(callback) = callback {
            callback(BufferSnapshotOutcome::Captured(BufferSnapshotPayload {
                storage: BufferSnapshotStorage::Chunked(guarded),
            }));
        }
    } else {
        // Cancellation, overflow, and silent teardown hand off every retained
        // chunk before a compact terminal callback can synchronously retry.
        drop(guarded);
        if let (Some(outcome), Some(callback)) = (outcome, callback) {
            callback(outcome);
        }
    }
}

#[cfg(feature = "test-utils")]
fn apply_test_mutation(session: &Rc<RefCell<ChunkedSnapshotSession>>, reached_end: bool) -> bool {
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
        return false;
    };
    if mutation.edit == BufferSnapshotTestEdit::Dispose {
        finish_snapshot(session, None);
        return false;
    }
    if mutation.edit == BufferSnapshotTestEdit::Pause {
        return true;
    }

    let (buffer, mark) = {
        let state = session.borrow();
        (state.buffer.clone(), state.progress_mark.clone())
    };
    let Some(mark) = mark else {
        return false;
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
        BufferSnapshotTestEdit::Dispose | BufferSnapshotTestEdit::Pause => unreachable!(),
    }
    false
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
        let BufferSnapshotOutcome::Captured(empty) = classify_snapshot_text(String::new(), 0)
        else {
            panic!("empty direct snapshot should fit");
        };
        assert_eq!(empty.into_string_on_worker(), "");
        let BufferSnapshotOutcome::Captured(multibyte) = classify_snapshot_text("é".to_string(), 2)
        else {
            panic!("exact multibyte direct snapshot should fit");
        };
        assert_eq!(multibyte.into_string_on_worker(), "é");
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
        let mut observed = 0;
        assert_eq!(classify_chunk_bytes(&mut observed, "é".len(), 4), None);
        assert_eq!(classify_chunk_bytes(&mut observed, "é".len(), 4), None);
        assert_eq!(observed, 4);
    }

    #[test]
    fn chunked_budget_discards_partial_text_after_first_byte_over() {
        let mut observed = 0;
        assert_eq!(classify_chunk_bytes(&mut observed, 3, 3), None);
        let outcome = classify_chunk_bytes(&mut observed, 1, 3);
        assert_eq!(
            outcome,
            Some(BufferSnapshotOutcome::ExceededLimit {
                observed_at_least: 4
            })
        );
        assert!(!matches!(outcome, Some(BufferSnapshotOutcome::Captured(_))));
    }
}
