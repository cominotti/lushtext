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

/// Files at or above 8 MiB use chunked snapshotting.
///
/// The threshold mirrors LushText's syntax-disable boundary: below it, a direct
/// copy is usually cheaper than scheduling; above it, one synchronous buffer
/// copy can create a visible frame stall on slower machines.
///
/// Kind: cap; 8 MiB, aligned with the 8 MiB tier it mirrors (was 10,000,000
/// bytes, `power-of-two-thresholds` D3).
pub(crate) const BUFFER_SNAPSHOT_SYNC_BYTE_THRESHOLD: u64 = 8 * 1024 * 1024;

/// Characters copied per GTK main-loop slice during a chunked snapshot.
///
/// `GtkTextBuffer::char_count()` is character-based, and 64k characters has
/// stayed comfortably below a frame on local measurements while avoiding a very
/// long chain of tiny timers for multi-megabyte buffers.
///
/// Kind: cap; a power of two, rounded down.
const BUFFER_SNAPSHOT_CHUNK_CHARS: i32 = 64 * 1024;
/// Maximum UTF-8 bytes copied from GTK in one character-aligned slice.
///
/// Kind: cap; a power of two, rounded down.
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
    Captured(BufferSnapshotPayload),
    ExceededLimit { observed_at_least: u64 },
    Cancelled(BufferSnapshotCancelReason),
}

/// Why a chunked snapshot rejected its partial capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferSnapshotCancelReason {
    SourceMutated,
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
        let guarded = self.reservation.own(SnapshotChunks {
            chunks,
            byte_len,
            #[cfg(feature = "test-utils")]
            metrics,
        });
        BufferSnapshotPayload {
            storage: BufferSnapshotStorage::Chunked(guarded),
        }
    }
}

/// Scalar capture metrics, carried without exposing document text.
///
/// A component of [`BufferSnapshotEvidence`], declared once and embedded in session evidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferSnapshotCaptureMetrics {
    pub slice_count: usize,
    pub chunk_count: usize,
    pub reserved_chunk_capacity: usize,
    pub max_chunk_bytes: usize,
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
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferSnapshotHandoffCounters {
    pub worker_coalesces: u64,
    pub gtk_coalesces: u64,
    pub worker_drops: u64,
    pub gtk_drops: u64,
}

/// The whole observable state of the snapshot lane (`session` is `None` once dropped).
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferSnapshotEvidence {
    pub session: Option<BufferSnapshotSessionEvidence>,
    pub handoff: BufferSnapshotHandoffCounters,
}

/// Read the lane's whole surface, optionally including one live session's state.
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
            Self::Ordinary(w) => w.arm(observed_epoch, callback),
            Self::Progress(w) => w.arm(observed_epoch, callback),
        }
    }
    fn cancel(&self) {
        match self {
            Self::Ordinary(w) => w.cancel(),
            Self::Progress(w) => w.cancel(),
        }
    }
    #[cfg(feature = "test-utils")]
    fn is_armed(&self) -> bool {
        match self {
            Self::Ordinary(w) => w.is_armed(),
            Self::Progress(w) => w.is_armed(),
        }
    }
}

/// Explicit lifecycle phase for one chunked GTK snapshot session.
#[derive(Default)]
enum SnapshotPhase {
    AwaitingAdmission {
        capacity_wakeup: SnapshotCapacityWakeup,
        callback: Option<SnapshotCallback>,
        pending_cancel: Option<(BufferSnapshotCancelReason, Option<glib::SourceId>)>,
    },
    Capturing {
        progress_mark: gtk4::TextMark,
        chunks: Vec<String>,
        reservation: DisposalReservation,
        changed_handler: glib::SignalHandlerId,
        scheduled_source: Option<glib::SourceId>,
        callback: Option<SnapshotCallback>,
        cancel_reason: Option<BufferSnapshotCancelReason>,
    },
    #[default]
    Terminal,
}

impl SnapshotPhase {
    fn capture_metrics(
        chunks: Option<&Vec<String>>,
        slice_count: usize,
        observed_bytes: u64,
    ) -> BufferSnapshotCaptureMetrics {
        BufferSnapshotCaptureMetrics {
            slice_count,
            chunk_count: chunks.map_or(0, Vec::len),
            reserved_chunk_capacity: chunks.map_or(0, Vec::capacity),
            max_chunk_bytes: chunks
                .and_then(|c| c.iter().map(String::len).max())
                .unwrap_or(0),
            captured_bytes: observed_bytes,
        }
    }

    #[cfg(feature = "test-utils")]
    fn to_evidence(
        &self,
        slice_count: usize,
        observed_bytes: u64,
    ) -> BufferSnapshotSessionEvidence {
        let (active, mark, handler, source, wakeup, cb, chunks) = match self {
            Self::AwaitingAdmission {
                capacity_wakeup,
                callback,
                pending_cancel,
            } => (
                true,
                false,
                false,
                pending_cancel.as_ref().is_some_and(|(_, s)| s.is_some()),
                capacity_wakeup.is_armed(),
                callback.is_some(),
                None,
            ),
            Self::Capturing {
                chunks,
                scheduled_source,
                callback,
                ..
            } => (
                true,
                true,
                true,
                scheduled_source.is_some(),
                false,
                callback.is_some(),
                Some(chunks),
            ),
            Self::Terminal => (false, false, false, false, false, false, None),
        };
        BufferSnapshotSessionEvidence {
            active,
            progress_mark_live: mark,
            changed_handler_live: handler,
            scheduled_source_live: source,
            admission_retry_source_live: wakeup,
            callback_pending: cb,
            capture: Self::capture_metrics(chunks, slice_count, observed_bytes),
        }
    }
}

/// One lifecycle-owned chunked snapshot on GTK's main thread.
struct ChunkedSnapshotSession {
    buffer: gtk4::TextBuffer,
    observed_bytes: u64,
    byte_policy: SnapshotBytePolicy,
    admission_lane: BufferSnapshotAdmissionLane,
    slice_count: usize,
    #[cfg(feature = "test-utils")]
    test_mutation: Option<BufferSnapshotTestMutation>,
    phase: SnapshotPhase,
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
        let Some(session) = self.0.upgrade() else {
            return;
        };
        let should_schedule = {
            let mut state = session.borrow_mut();
            match &mut state.phase {
                SnapshotPhase::Terminal => return,
                SnapshotPhase::AwaitingAdmission {
                    capacity_wakeup,
                    pending_cancel,
                    ..
                } => {
                    if pending_cancel.is_some() {
                        return;
                    }
                    capacity_wakeup.cancel();
                    *pending_cancel = Some((BufferSnapshotCancelReason::Superseded, None));
                    true
                }
                SnapshotPhase::Capturing { cancel_reason, .. } => {
                    if cancel_reason.is_some() {
                        return;
                    }
                    *cancel_reason = Some(BufferSnapshotCancelReason::Superseded);
                    false
                }
            }
        };
        if should_schedule {
            schedule_snapshot_slice(&session);
        }
    }

    /// Tear down a disposed owner's snapshot without invoking its callback.
    pub(crate) fn dispose(&self) {
        if let Some(session) = self.0.upgrade() {
            finish_snapshot(&session, None, false);
        }
    }

    /// Whether the session still owns GTK resources or a terminal callback.
    pub(crate) fn is_active(&self) -> bool {
        self.0
            .upgrade()
            .is_some_and(|session| !matches!(session.borrow().phase, SnapshotPhase::Terminal))
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
        let removed_source = {
            let mut state = session.borrow_mut();
            match &mut state.phase {
                SnapshotPhase::Capturing {
                    scheduled_source, ..
                } => Some(scheduled_source.take()),
                SnapshotPhase::AwaitingAdmission { .. } | SnapshotPhase::Terminal => None,
            }
        };
        let Some(source) = removed_source else {
            return;
        };
        if let Some(source) = source {
            source.remove();
        }
        schedule_snapshot_slice(&session);
    }

    /// This session's component of the lane's evidence surface (`None` once dropped).
    #[cfg(feature = "test-utils")]
    #[must_use]
    fn session_evidence(&self) -> Option<BufferSnapshotSessionEvidence> {
        let session = self.0.upgrade()?;
        let session = session.borrow();
        Some(
            session
                .phase
                .to_evidence(session.slice_count, session.observed_bytes),
        )
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
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferSnapshotSessionEvidence {
    pub active: bool,
    pub progress_mark_live: bool,
    pub changed_handler_live: bool,
    pub scheduled_source_live: bool,
    pub admission_retry_source_live: bool,
    pub callback_pending: bool,
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
    let buffer = buffer.upcast_ref();
    buffer
        .text(
            &buffer.start_iter(),
            &crate::ui::protected_fold::document_end_iter(buffer),
            true,
        )
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
        buffer.upcast(),
        SnapshotBytePolicy::Unbounded,
        BufferSnapshotAdmissionLane::Ordinary,
        None,
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
        buffer.upcast(),
        SnapshotBytePolicy::Limited(max_bytes),
        BufferSnapshotAdmissionLane::Ordinary,
        None,
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
    start_chunked_snapshot(
        buffer.upcast(),
        SnapshotBytePolicy::Limited(max_bytes),
        BufferSnapshotAdmissionLane::Progress,
        Some(admission),
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
    let policy = max_bytes.map_or(SnapshotBytePolicy::Unbounded, SnapshotBytePolicy::Limited);
    start_chunked_snapshot(
        buffer,
        policy,
        BufferSnapshotAdmissionLane::Ordinary,
        None,
        Box::new(callback),
        mutation,
    )
}

fn start_chunked_snapshot(
    buffer: gtk4::TextBuffer,
    byte_policy: SnapshotBytePolicy,
    admission_lane: BufferSnapshotAdmissionLane,
    preadmitted: Option<BufferSnapshotAdmission>,
    callback: SnapshotCallback,
    #[cfg(feature = "test-utils")] test_mutation: Option<BufferSnapshotTestMutation>,
) -> BufferSnapshotHandle {
    let phase = SnapshotPhase::AwaitingAdmission {
        capacity_wakeup: SnapshotCapacityWakeup::new(admission_lane),
        callback: Some(callback),
        pending_cancel: None,
    };
    let session = Rc::new(RefCell::new(ChunkedSnapshotSession {
        buffer,
        observed_bytes: 0,
        byte_policy,
        admission_lane,
        slice_count: 0,
        #[cfg(feature = "test-utils")]
        test_mutation,
        phase,
    }));
    let handle = BufferSnapshotHandle(Rc::downgrade(&session));
    match preadmitted {
        Some(admission) => activate_snapshot_session(&session, admission),
        None => retry_snapshot_admission(&session),
    }
    handle
}

fn retry_snapshot_admission(session: &Rc<RefCell<ChunkedSnapshotSession>>) {
    let (char_count, byte_policy, admission_lane, pending_cancel) = {
        let state = session.borrow();
        let SnapshotPhase::AwaitingAdmission { pending_cancel, .. } = &state.phase else {
            return;
        };
        (
            state.buffer.char_count(),
            state.byte_policy,
            state.admission_lane,
            pending_cancel.as_ref().map(|(reason, _)| *reason),
        )
    };
    if let Some(reason) = pending_cancel {
        finish_snapshot(
            session,
            Some(BufferSnapshotOutcome::Cancelled(reason)),
            false,
        );
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
    if let SnapshotPhase::AwaitingAdmission {
        capacity_wakeup, ..
    } = &session.borrow().phase
    {
        capacity_wakeup.arm(observed_epoch, move || {
            retry_snapshot_admission(&session_for_wakeup);
        });
    }
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

    let session_weak = Rc::downgrade(session);
    let changed_handler = signal_buffer.connect_changed(move |_| {
        let Some(session) = session_weak.upgrade() else {
            return;
        };
        let mut session = session.borrow_mut();
        if let SnapshotPhase::Capturing { cancel_reason, .. } = &mut session.phase
            && cancel_reason.is_none()
        {
            // GtkTextIter becomes invalid after character-count changes. The
            // signal records cancellation synchronously; terminal delivery is
            // deferred to the slice path to avoid workflow reentrancy here.
            *cancel_reason = Some(BufferSnapshotCancelReason::SourceMutated);
        }
    });

    let stale_source = {
        let mut state = session.borrow_mut();
        let SnapshotPhase::AwaitingAdmission {
            capacity_wakeup,
            callback,
            pending_cancel,
        } = std::mem::take(&mut state.phase)
        else {
            drop(reservation);
            signal_buffer.disconnect(changed_handler);
            signal_buffer.delete_mark(&progress_mark);
            return;
        };
        capacity_wakeup.cancel();
        let (cancel_reason, stale_source) =
            pending_cancel.map_or((None, None), |(r, s)| (Some(r), s));
        state.phase = SnapshotPhase::Capturing {
            progress_mark,
            chunks: Vec::with_capacity(chunk_capacity),
            reservation,
            changed_handler,
            scheduled_source: None,
            callback,
            cancel_reason,
        };
        stale_source
    };
    if let Some(source) = stale_source {
        source.remove();
    }

    run_snapshot_slice(session);
}

fn store_scheduled_source(
    session: &Rc<RefCell<ChunkedSnapshotSession>>,
    source_id: glib::SourceId,
) {
    let mut state = session.borrow_mut();
    match &mut state.phase {
        SnapshotPhase::AwaitingAdmission {
            pending_cancel: Some((_, slot)),
            ..
        } => {
            *slot = Some(source_id);
        }
        SnapshotPhase::Capturing {
            scheduled_source, ..
        } => *scheduled_source = Some(source_id),
        SnapshotPhase::AwaitingAdmission { .. } | SnapshotPhase::Terminal => {
            drop(state);
            source_id.remove();
        }
    }
}

fn schedule_snapshot_slice(session: &Rc<RefCell<ChunkedSnapshotSession>>) {
    let session_for_source = Rc::clone(session);
    let source_id = glib::idle_add_local_once(move || run_snapshot_slice(&session_for_source));
    store_scheduled_source(session, source_id);
}

fn try_snapshot_admission(
    char_count: i32,
    byte_policy: SnapshotBytePolicy,
    lane: BufferSnapshotAdmissionLane,
) -> Option<BufferSnapshotAdmission> {
    let (weight, chunk_capacity) = snapshot_allocation_plan(char_count, byte_policy);
    let reservation = match lane {
        BufferSnapshotAdmissionLane::Ordinary => try_reserve_for_gtk(weight),
        BufferSnapshotAdmissionLane::Progress => try_reserve_progress_for_gtk(weight),
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
    let step = {
        let mut state = session.borrow_mut();
        let buffer = state.buffer.clone();
        match &mut state.phase {
            SnapshotPhase::Terminal => return,
            SnapshotPhase::AwaitingAdmission { pending_cancel, .. } => {
                match pending_cancel.take() {
                    Some((reason, source)) => Err((reason, source)),
                    None => return,
                }
            }
            SnapshotPhase::Capturing {
                progress_mark,
                scheduled_source,
                cancel_reason,
                ..
            } => {
                scheduled_source.take();
                cancel_reason
                    .map_or_else(|| Ok((buffer, progress_mark.clone())), |r| Err((r, None)))
            }
        }
    };
    let (buffer, mark) = match step {
        Ok(pair) => pair,
        Err((reason, source)) => {
            if let Some(s) = source {
                s.remove();
            }
            finish_snapshot(
                session,
                Some(BufferSnapshotOutcome::Cancelled(reason)),
                false,
            );
            return;
        }
    };

    // GTK 4 invalidates every outstanding iterator after an indexable content
    // mutation. The mark is the cross-turn position; both iterators below are
    // local to this slice and are discarded before yielding.
    let start = buffer.iter_at_mark(&mark);
    // The document ends before a protected install's stand-in terminator,
    // which is not document text (`ui::protected_fold::document_end_iter`).
    let document_end = crate::ui::protected_fold::document_end_iter(buffer.upcast_ref());
    let mut end = start;
    if !end.forward_chars(BUFFER_SNAPSHOT_CHUNK_CHARS) || end > document_end {
        end = document_end;
    }
    let reached_end = end == document_end;
    let chunk = buffer.text(&start, &end, true);
    buffer.move_mark(&mark, &end);

    let overflow = {
        let state = &mut *session.borrow_mut();
        let byte_policy = state.byte_policy;
        let chunk = chunk.to_string();
        let chunk_bytes = chunk.len();
        let overflow = match byte_policy {
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
        if let SnapshotPhase::Capturing { chunks, .. } = &mut state.phase {
            state.slice_count += 1;
            chunks.push(chunk);
        }
        overflow
    };

    #[cfg(feature = "test-utils")]
    let paused = apply_test_mutation(session, reached_end);

    let cancel_reason = match &session.borrow().phase {
        SnapshotPhase::Capturing { cancel_reason, .. } => *cancel_reason,
        _ => None,
    };
    if let Some(reason) = cancel_reason {
        finish_snapshot(
            session,
            Some(BufferSnapshotOutcome::Cancelled(reason)),
            false,
        );
        return;
    }
    if let Some(outcome) = overflow {
        finish_snapshot(session, Some(outcome), false);
        return;
    }
    if reached_end {
        finish_snapshot(session, None, true);
        return;
    }
    #[cfg(feature = "test-utils")]
    let delay = if paused {
        BUFFER_SNAPSHOT_TEST_PAUSE_TIMEOUT
    } else {
        Duration::from_millis(1)
    };
    #[cfg(not(feature = "test-utils"))]
    let delay = Duration::from_millis(1);
    let session_for_source = Rc::clone(session);
    let source_id =
        glib::timeout_add_local_once(delay, move || run_snapshot_slice(&session_for_source));
    store_scheduled_source(session, source_id);
}

fn finish_snapshot(
    session: &Rc<RefCell<ChunkedSnapshotSession>>,
    outcome: Option<BufferSnapshotOutcome>,
    captured: bool,
) {
    let (buffer, observed_bytes, slice_count, old_phase) = {
        let mut state = session.borrow_mut();
        let old_phase = std::mem::take(&mut state.phase);
        if matches!(old_phase, SnapshotPhase::Terminal) {
            return;
        }
        (
            state.buffer.clone(),
            state.observed_bytes,
            state.slice_count,
            old_phase,
        )
    };
    match old_phase {
        SnapshotPhase::Terminal => {}
        SnapshotPhase::AwaitingAdmission {
            capacity_wakeup,
            callback,
            pending_cancel,
        } => {
            capacity_wakeup.cancel();
            if let Some((_, Some(source))) = pending_cancel {
                source.remove();
            }
            debug_assert!(!captured);
            if let (Some(outcome), Some(callback)) = (outcome, callback) {
                callback(outcome);
            }
        }
        SnapshotPhase::Capturing {
            progress_mark,
            chunks,
            mut reservation,
            changed_handler,
            scheduled_source,
            callback,
            ..
        } => {
            if let Some(source) = scheduled_source {
                source.remove();
            }
            buffer.disconnect(changed_handler);
            buffer.delete_mark(&progress_mark);
            let metrics =
                SnapshotPhase::capture_metrics(Some(&chunks), slice_count, observed_bytes);
            let header = u64::try_from(metrics.reserved_chunk_capacity)
                .unwrap_or(u64::MAX)
                .saturating_mul(u64::try_from(size_of::<String>()).unwrap_or(u64::MAX));
            reservation.shrink_to(observed_bytes.saturating_add(header));
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
                drop(guarded);
                if let (Some(outcome), Some(callback)) = (outcome, callback) {
                    callback(outcome);
                }
            }
        }
    }
}

#[cfg(feature = "test-utils")]
fn apply_test_mutation(session: &Rc<RefCell<ChunkedSnapshotSession>>, reached_end: bool) -> bool {
    let mutation = {
        let mut state = session.borrow_mut();
        let matches = state.test_mutation.is_some_and(|m| match m.trigger {
            BufferSnapshotTestTrigger::AfterSlice(slice) => slice == state.slice_count,
            BufferSnapshotTestTrigger::FinalSlice => reached_end,
        });
        matches.then(|| state.test_mutation.take()).flatten()
    };
    let Some(mutation) = mutation else {
        return false;
    };
    match mutation.edit {
        BufferSnapshotTestEdit::Dispose => {
            finish_snapshot(session, None, false);
            return false;
        }
        BufferSnapshotTestEdit::Pause => return true,
        _ => {}
    }
    let (buffer, mark) = {
        let state = session.borrow();
        let mark = match &state.phase {
            SnapshotPhase::Capturing { progress_mark, .. } => Some(progress_mark.clone()),
            _ => None,
        };
        (state.buffer.clone(), mark)
    };
    let Some(mark) = mark else { return false };
    let delete_one = |mut s: gtk4::TextIter| {
        let mut e = s;
        if e.forward_char() {
            buffer.delete(&mut s, &mut e);
        }
    };
    match mutation.edit {
        BufferSnapshotTestEdit::InsertBeforeMark => {
            buffer.insert(&mut buffer.start_iter(), "before");
        }
        BufferSnapshotTestEdit::InsertAfterMark => buffer.insert(&mut buffer.end_iter(), "after"),
        BufferSnapshotTestEdit::DeleteBeforeMark => delete_one(buffer.start_iter()),
        BufferSnapshotTestEdit::DeleteAfterMark => delete_one(buffer.iter_at_mark(&mark)),
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
