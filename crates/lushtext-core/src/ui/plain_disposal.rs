// SPDX-License-Identifier: GPL-3.0-or-later

//! Non-blocking worker-side destruction for large plain-Rust UI payloads.
//!
//! Admission accounts for both job count and caller-estimated retained bytes.
//! Document-sized values reserve their future drop slot before crossing onto
//! GTK, so the final GTK owner performs only a guaranteed non-blocking handoff.
//! Capacity retries retain compact requests, never an unreserved large value.

#[cfg(any(test, feature = "test-utils"))]
use crate::model::plain_disposal::PlainDisposalLatest;
#[cfg(feature = "test-utils")]
use crate::model::plain_disposal::PlainDisposalLatestSnapshot;
#[cfg(any(test, feature = "test-utils"))]
use crate::model::plain_disposal::PlainDisposalSnapshot;
use crate::model::plain_disposal::{
    PlainDisposalAdmission, PlainDisposalLimits, QueuedPlainDisposal,
};
use crossbeam_channel::{Sender, TrySendError, bounded};
#[cfg(any(test, feature = "test-utils"))]
use std::cell::Cell;
use std::cell::RefCell;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

/// Two destructors may make progress without competing with file-I/O workers.
const DISPOSAL_WORKERS: usize = 2;
/// Eight queued owners bound supersession bursts independently of byte charge.
const DISPOSAL_QUEUE_CAPACITY: usize = 8;
/// Two maximum-size Replace All undo windows may drain concurrently.
const DISPOSAL_RETAINED_BYTE_CAPACITY: u64 = 128 * 1024 * 1024;
/// Recovery and Notes source construction retain independent progress capacity.
///
/// The shared ordinary lane may legitimately stay full while long-lived palette
/// or Replace All owners remain useful. This lane keeps one startup preload or
/// maximum Notes source able to cross GTK without exceeding a fixed bound.
pub(crate) const PROGRESS_DISPOSAL_RETAINED_BYTE_CAPACITY: u64 = 72 * 1024 * 1024;
/// Producers poll capacity once per display frame while retaining one latest job.
const DISPOSAL_RETRY_INTERVAL: Duration = Duration::from_millis(16);
/// Rejected-payload retry is retained only for statically small compatibility evidence.
#[cfg(any(test, feature = "test-utils"))]
const MAX_SMALL_PENDING_DISPOSAL_BYTES: u64 = 64 * 1024;

const DISPOSAL_LIMITS: PlainDisposalLimits = PlainDisposalLimits::new(
    DISPOSAL_WORKERS,
    DISPOSAL_QUEUE_CAPACITY,
    DISPOSAL_RETAINED_BYTE_CAPACITY,
)
.with_replacement_headroom(1);
const PROGRESS_DISPOSAL_LIMITS: PlainDisposalLimits =
    PlainDisposalLimits::new(1, 2, PROGRESS_DISPOSAL_RETAINED_BYTE_CAPACITY)
        .with_replacement_headroom(1);

type DisposeFn = Box<dyn FnOnce() + Send + 'static>;

/// Type-erased plain-data destructor plus its conservative retained-byte charge.
pub(crate) struct DisposalJob {
    weight: u64,
    dispose: Option<DisposeFn>,
    terminal: Option<DisposeFn>,
}

impl DisposalJob {
    /// Build one weighted destructor with no completion action.
    pub(crate) fn new(weight: u64, dispose: impl FnOnce() + Send + 'static) -> Self {
        Self {
            weight,
            dispose: Some(Box::new(dispose)),
            terminal: None,
        }
    }

    /// Build one weighted destructor whose compact terminal runs exactly once.
    pub(crate) fn with_terminal(
        weight: u64,
        dispose: impl FnOnce() + Send + 'static,
        terminal: impl FnOnce() + Send + 'static,
    ) -> Self {
        Self {
            weight,
            dispose: Some(Box::new(dispose)),
            terminal: Some(Box::new(terminal)),
        }
    }

    #[must_use]
    fn weight(&self) -> u64 {
        self.weight
    }

    fn run(mut self) {
        let dispose = self
            .dispose
            .take()
            .expect("an admitted disposal job runs exactly once");
        dispose();
    }
}

impl Drop for DisposalJob {
    fn drop(&mut self) {
        let Some(terminal) = self.terminal.take() else {
            return;
        };
        // A compact accounting/completion terminal must not turn a destructor
        // panic into a double panic or kill a long-lived lane worker.
        let _ = catch_unwind(AssertUnwindSafe(terminal));
    }
}

struct DisposalEnvelope {
    job: DisposalJob,
    queued: QueuedPlainDisposal,
}

struct DisposalLaneInner {
    sender: Mutex<Option<Sender<DisposalEnvelope>>>,
    admission: Arc<Mutex<PlainDisposalAdmission>>,
    workers: Mutex<Vec<JoinHandle<()>>>,
    capacity_epoch: Arc<AtomicU64>,
}

impl Drop for DisposalLaneInner {
    fn drop(&mut self) {
        self.sender
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        for worker in self
            .workers
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .drain(..)
        {
            let _ = worker.join();
        }
    }
}

#[derive(Clone)]
struct DisposalLane {
    inner: Arc<DisposalLaneInner>,
}

impl DisposalLane {
    fn new(limits: PlainDisposalLimits) -> Self {
        Self::new_named(limits, "lushtext-disposal")
    }

    fn new_named(limits: PlainDisposalLimits, worker_name_prefix: &'static str) -> Self {
        assert!(limits.worker_limit > 0, "disposal lane needs a worker");
        assert!(
            limits.queued_job_limit > 0,
            "disposal lane needs queued admission"
        );
        assert!(
            limits.retained_byte_limit > 0,
            "disposal lane needs a retained-byte ceiling"
        );

        let channel_capacity = limits
            .queued_job_limit
            .saturating_add(limits.replacement_job_headroom);
        let (sender, receiver) = bounded::<DisposalEnvelope>(channel_capacity);
        let admission = Arc::new(Mutex::new(PlainDisposalAdmission::new(limits)));
        let capacity_epoch = Arc::new(AtomicU64::new(0));
        let mut workers = Vec::with_capacity(limits.worker_limit);
        for index in 0..limits.worker_limit {
            let receiver = receiver.clone();
            let admission = Arc::clone(&admission);
            let capacity_epoch = Arc::clone(&capacity_epoch);
            workers.push(
                std::thread::Builder::new()
                    .name(format!("{worker_name_prefix}-{index}"))
                    .spawn(move || {
                        while let Ok(envelope) = receiver.recv() {
                            let active = lock_unpoisoned(&admission).start(envelope.queued);
                            let panicked = catch_unwind(AssertUnwindSafe(|| {
                                envelope.job.run();
                            }))
                            .is_err();
                            lock_unpoisoned(&admission).finish(active, panicked);
                            capacity_epoch.fetch_add(1, Ordering::AcqRel);
                            if panicked {
                                tracing::error!("Plain-data disposal destructor panicked");
                            }
                        }
                    })
                    .expect("plain disposal worker should start"),
            );
        }
        drop(receiver);

        Self {
            inner: Arc::new(DisposalLaneInner {
                sender: Mutex::new(Some(sender)),
                admission,
                workers: Mutex::new(workers),
                capacity_epoch,
            }),
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn try_submit(&self, job: DisposalJob) -> Result<(), DisposalSubmitError> {
        let Some(permit) = self.try_reserve(job.weight()) else {
            return Err(DisposalSubmitError::Full(job));
        };
        permit.submit(job)
    }

    fn try_reserve(&self, weight: u64) -> Option<DisposalPermit> {
        let queued = lock_unpoisoned(&self.inner.admission).try_queue(weight)?;
        Some(DisposalPermit {
            lane: self.clone(),
            queued: Some(queued),
            weight,
        })
    }

    fn try_reserve_replacement(&self, weight: u64, replaced_weight: u64) -> Option<DisposalPermit> {
        let queued = lock_unpoisoned(&self.inner.admission)
            .try_queue_replacement(weight, replaced_weight)?;
        Some(DisposalPermit {
            lane: self.clone(),
            queued: Some(queued),
            weight,
        })
    }

    fn note_capacity_release(&self) {
        self.inner.capacity_epoch.fetch_add(1, Ordering::AcqRel);
    }

    fn capacity_epoch(&self) -> u64 {
        self.inner.capacity_epoch.load(Ordering::Acquire)
    }

    fn submit_reserved(
        &self,
        job: DisposalJob,
        queued: QueuedPlainDisposal,
    ) -> Result<(), DisposalSubmitError> {
        let Some(sender) = lock_unpoisoned(&self.inner.sender).clone() else {
            lock_unpoisoned(&self.inner.admission).cancel_queued(queued, true);
            self.note_capacity_release();
            return Err(DisposalSubmitError::Closed(job));
        };
        match sender.try_send(DisposalEnvelope { job, queued }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(envelope)) => {
                lock_unpoisoned(&self.inner.admission).cancel_queued(envelope.queued, false);
                self.note_capacity_release();
                Err(DisposalSubmitError::Full(envelope.job))
            }
            Err(TrySendError::Disconnected(envelope)) => {
                lock_unpoisoned(&self.inner.admission).cancel_queued(envelope.queued, true);
                self.note_capacity_release();
                Err(DisposalSubmitError::Closed(envelope.job))
            }
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    fn snapshot(&self) -> PlainDisposalSnapshot {
        lock_unpoisoned(&self.inner.admission).snapshot()
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn close(&self) {
        lock_unpoisoned(&self.inner.sender).take();
    }
}

/// Future queue ownership reserved before a large value crosses onto GTK.
struct DisposalPermit {
    lane: DisposalLane,
    queued: Option<QueuedPlainDisposal>,
    weight: u64,
}

impl DisposalPermit {
    fn shrink_to(&mut self, weight: u64) {
        debug_assert!(weight <= self.weight);
        let released_capacity = weight < self.weight;
        if let Some(queued) = self.queued.as_mut() {
            lock_unpoisoned(&self.lane.inner.admission).shrink_queued(queued, weight);
            self.weight = weight;
            if released_capacity {
                self.lane.note_capacity_release();
            }
        }
    }

    fn submit(mut self, job: DisposalJob) -> Result<(), DisposalSubmitError> {
        debug_assert_eq!(self.weight, job.weight());
        let queued = self
            .queued
            .take()
            .expect("a disposal reservation submits exactly once");
        self.lane.submit_reserved(job, queued)
    }
}

impl Drop for DisposalPermit {
    fn drop(&mut self) {
        if let Some(queued) = self.queued.take() {
            lock_unpoisoned(&self.lane.inner.admission).cancel_queued(queued, false);
            self.lane.note_capacity_release();
        }
    }
}

/// A large `Send` value whose final owner is pre-admitted to the worker lane.
///
/// Workers create this wrapper before publishing a value to GTK. Its `Drop`
/// only performs one guaranteed non-blocking channel handoff; the value itself
/// is destroyed by a disposal worker even when the last wrapper dies on GTK.
#[doc(hidden)]
pub struct DisposalOwned<T: Send + 'static> {
    value: Option<T>,
    permit: Option<DisposalPermit>,
    terminal: Mutex<Option<DisposeFn>>,
}

impl<T: Default + Send + 'static> Default for DisposalOwned<T> {
    fn default() -> Self {
        Self::small_unreserved(T::default())
    }
}

/// Worker-side reservation acquired before a document-sized result is built.
pub(crate) struct DisposalReservation {
    permit: DisposalPermit,
}

impl DisposalReservation {
    /// Reduce a conservative pre-I/O charge to the completed result's exact weight.
    pub(crate) fn shrink_to(&mut self, weight: u64) {
        self.permit.shrink_to(weight);
    }

    /// Attach the reserved future worker slot to the completed value.
    pub(crate) fn own<T: Send + 'static>(self, value: T) -> DisposalOwned<T> {
        DisposalOwned::new(value, self.permit)
    }
}

impl<T: Send + 'static> DisposalOwned<T> {
    fn new(value: T, permit: DisposalPermit) -> Self {
        Self {
            value: Some(value),
            permit: Some(permit),
            terminal: Mutex::new(None),
        }
    }

    /// Wrap a statically small sentinel that does not need worker retirement.
    pub(crate) fn small_unreserved(value: T) -> Self {
        Self {
            value: Some(value),
            permit: None,
            terminal: Mutex::new(None),
        }
    }

    /// Return the byte credit carried by this guarded retained owner.
    #[must_use]
    pub(crate) fn reservation_weight(&self) -> Option<u64> {
        self.permit.as_ref().map(|permit| permit.weight)
    }

    /// Attach compact worker-terminal accounting to the future final drop.
    pub(crate) fn with_disposal_terminal(self, terminal: impl FnOnce() + Send + 'static) -> Self {
        debug_assert!(lock_unpoisoned(&self.terminal).is_none());
        *lock_unpoisoned(&self.terminal) = Some(Box::new(terminal));
        self
    }

    /// Transfer an accepted value into current UI state and release its transit reservation.
    ///
    /// Callers must use this only after freshness/lifetime checks establish that
    /// the value is no longer a stale completion. Large values that remain
    /// replaceable on GTK should keep this wrapper instead.
    pub(crate) fn into_inner_for_current_install(mut self) -> T {
        debug_assert!(lock_unpoisoned(&self.terminal).is_none());
        self.permit.take();
        self.value
            .take()
            .expect("disposal-owned value exists until current installation")
    }

    /// Consume a guarded value on a non-GTK worker without scheduling another drop job.
    pub(crate) fn into_inner_on_worker(mut self) -> T {
        debug_assert!(lock_unpoisoned(&self.terminal).is_none());
        self.permit.take();
        self.value
            .take()
            .expect("disposal-owned value exists until worker consumption")
    }

    /// Transform ownership while preserving its future disposal reservation.
    ///
    /// The mapping itself must be constant-time when called on GTK; worker
    /// callers may perform a more expensive transformation.
    pub(crate) fn map_preserving_reservation<U: Send + 'static>(
        mut self,
        map: impl FnOnce(T) -> U,
    ) -> DisposalOwned<U> {
        let value = self
            .value
            .take()
            .expect("disposal-owned value exists until worker mapping");
        DisposalOwned {
            value: Some(map(value)),
            permit: self.permit.take(),
            terminal: Mutex::new(
                self.terminal
                    .get_mut()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take(),
            ),
        }
    }

    /// Detach a compact remainder while preserving worker retirement for the heavy part.
    ///
    /// The splitter runs on GTK and therefore must only move already-owned
    /// allocations; it must not scan document text or perform nested drops.
    pub(crate) fn split_for_worker_retirement<U: Send + 'static>(
        mut self,
        split: impl FnOnce(&mut T) -> U,
    ) -> (T, DisposalOwned<U>) {
        let mut value = self
            .value
            .take()
            .expect("disposal-owned value exists until worker split");
        let retiring = split(&mut value);
        let guarded = DisposalOwned {
            value: Some(retiring),
            permit: self.permit.take(),
            terminal: Mutex::new(
                self.terminal
                    .get_mut()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take(),
            ),
        };
        (value, guarded)
    }
}

impl<T: Send + 'static> Deref for DisposalOwned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
            .as_ref()
            .expect("disposal-owned value exists until Drop")
    }
}

impl<T: Send + 'static> DerefMut for DisposalOwned<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
            .as_mut()
            .expect("disposal-owned value exists until Drop")
    }
}

impl<T: Send + 'static> Drop for DisposalOwned<T> {
    fn drop(&mut self) {
        let Some(value) = self.value.take() else {
            return;
        };
        let Some(permit) = self.permit.take() else {
            drop(value);
            if let Some(terminal) = self
                .terminal
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
            {
                terminal();
            }
            return;
        };
        let weight = permit.weight;
        let job = if let Some(terminal) = self
            .terminal
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            DisposalJob::with_terminal(weight, move || drop(value), terminal)
        } else {
            DisposalJob::new(weight, move || drop(value))
        };
        match permit.submit(job) {
            Ok(()) => {}
            Err(DisposalSubmitError::Closed(job)) => {
                // The process-wide production lane never closes. Test-owned
                // lanes can close during teardown; keep even that fallback off
                // the thread that released the final owner.
                let _ = std::thread::Builder::new()
                    .name("lushtext-disposal-closed-fallback".to_string())
                    .spawn(move || job.run());
            }
            Err(DisposalSubmitError::Full(job)) => {
                // A live reservation already occupies one of the channel's
                // physical slots, so `Full` would mean the lane invariant is
                // broken. Leak instead of running a document-sized nested
                // destructor on GTK.
                tracing::error!(
                    "Reserved plain-data disposal could not enter its guaranteed channel slot"
                );
                std::mem::forget(job);
            }
        }
    }
}

/// Pre-admit a worker-produced value before it is published to GTK.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn try_own_for_gtk<T: Send + 'static>(
    weight: u64,
    value: T,
) -> Result<DisposalOwned<T>, T> {
    let Some(permit) = disposal_lane().try_reserve(weight) else {
        return Err(value);
    };
    Ok(DisposalOwned::new(value, permit))
}

/// Reserve a future worker drop before constructing a large GTK-bound result.
pub(crate) fn try_reserve_for_gtk(weight: u64) -> Option<DisposalReservation> {
    disposal_lane()
        .try_reserve(weight)
        .map(|permit| DisposalReservation { permit })
}

/// Reserve a future replacement while crediting the current guarded owner.
pub(crate) fn try_reserve_replacement_for_gtk(
    weight: u64,
    replaced_weight: u64,
) -> Option<DisposalReservation> {
    disposal_lane()
        .try_reserve_replacement(weight, replaced_weight)
        .map(|permit| DisposalReservation { permit })
}

/// Reserve future worker destruction for a workflow that must make progress
/// even while ordinary long-lived UI owners fill the shared lane.
pub(crate) fn try_reserve_progress_for_gtk(weight: u64) -> Option<DisposalReservation> {
    progress_disposal_lane()
        .try_reserve(weight)
        .map(|permit| DisposalReservation { permit })
}

/// Replace one guarded progress owner while crediting its current byte charge.
pub(crate) fn try_reserve_progress_replacement_for_gtk(
    weight: u64,
    replaced_weight: u64,
) -> Option<DisposalReservation> {
    progress_disposal_lane()
        .try_reserve_replacement(weight, replaced_weight)
        .map(|permit| DisposalReservation { permit })
}

/// Capture the lane epoch before a non-blocking admission attempt.
#[must_use]
pub(crate) fn disposal_capacity_epoch() -> u64 {
    disposal_lane().capacity_epoch()
}

/// Capture the progress lane epoch before a non-blocking admission attempt.
#[must_use]
pub(crate) fn progress_disposal_capacity_epoch() -> u64 {
    progress_disposal_lane().capacity_epoch()
}

/// One paced GTK retry that fires only after disposal capacity changes.
#[derive(Default)]
pub(crate) struct CapacityWakeup<const PROGRESS: bool> {
    source: Rc<RefCell<Option<glib::SourceId>>>,
}

impl<const PROGRESS: bool> CapacityWakeup<PROGRESS> {
    /// Arm one scalar capacity watch without accumulating timeout sources.
    pub(crate) fn arm(&self, observed_epoch: u64, callback: impl FnOnce() + 'static) {
        if self.source.borrow().is_some() {
            return;
        }
        let source_cell = Rc::clone(&self.source);
        let mut callback = Some(callback);
        let source = glib::timeout_add_local(DISPOSAL_RETRY_INTERVAL, move || {
            let current_epoch = if PROGRESS {
                progress_disposal_capacity_epoch()
            } else {
                disposal_capacity_epoch()
            };
            if current_epoch == observed_epoch {
                return glib::ControlFlow::Continue;
            }
            source_cell.borrow_mut().take();
            if let Some(callback) = callback.take() {
                callback();
            }
            glib::ControlFlow::Break
        });
        *self.source.borrow_mut() = Some(source);
    }

    /// Cancel the exact source owned by this producer.
    pub(crate) fn cancel(&self) {
        if let Some(source) = self.source.borrow_mut().take() {
            source.remove();
        }
    }

    #[must_use]
    #[cfg(feature = "test-utils")]
    pub(crate) fn is_armed(&self) -> bool {
        self.source.borrow().is_some()
    }
}

/// Capacity wakeup for ordinary shared-lane producers.
pub(crate) type DisposalCapacityWakeup = CapacityWakeup<false>;
/// Capacity wakeup for startup recovery and Notes source progress.
pub(crate) type ProgressDisposalCapacityWakeup = CapacityWakeup<true>;

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn disposal_lane() -> DisposalLane {
    static LANE: OnceLock<DisposalLane> = OnceLock::new();
    LANE.get_or_init(|| DisposalLane::new(DISPOSAL_LIMITS))
        .clone()
}

fn progress_disposal_lane() -> DisposalLane {
    static LANE: OnceLock<DisposalLane> = OnceLock::new();
    LANE.get_or_init(|| {
        DisposalLane::new_named(PROGRESS_DISPOSAL_LIMITS, "lushtext-progress-disposal")
    })
    .clone()
}

/// Immediate non-blocking lane outcome with rejected ownership intact.
pub(crate) enum DisposalSubmitError {
    Full(DisposalJob),
    Closed(DisposalJob),
}

impl DisposalSubmitError {
    #[cfg(test)]
    #[must_use]
    fn into_job(self) -> DisposalJob {
        match self {
            Self::Full(job) | Self::Closed(job) => job,
        }
    }
}

impl fmt::Debug for DisposalSubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full(job) => formatter.debug_tuple("Full").field(&job.weight()).finish(),
            Self::Closed(job) => formatter
                .debug_tuple("Closed")
                .field(&job.weight())
                .finish(),
        }
    }
}

/// Scalar per-producer evidence without payload contents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg(feature = "test-utils")]
pub(crate) struct DisposalProducerSnapshot {
    pub(crate) latest: PlainDisposalLatestSnapshot,
    pub(crate) submitted_jobs: u64,
    pub(crate) admitted_jobs: u64,
    pub(crate) immediate_full_outcomes: u64,
    pub(crate) closed_outcomes: u64,
    pub(crate) owner_closed: bool,
}

#[cfg(any(test, feature = "test-utils"))]
struct DisposalProducerInner {
    lane: DisposalLane,
    latest: RefCell<PlainDisposalLatest<DisposalJob>>,
    retry_source: RefCell<Option<glib::SourceId>>,
    submitted_jobs: Cell<u64>,
    admitted_jobs: Cell<u64>,
    immediate_full_outcomes: Cell<u64>,
    closed_outcomes: Cell<u64>,
    closed: Cell<bool>,
}

/// Small-payload compatibility producer with one latest rejection and retry source.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone)]
pub(crate) struct DisposalProducer {
    inner: Rc<DisposalProducerInner>,
}

#[cfg(any(test, feature = "test-utils"))]
impl Default for DisposalProducer {
    fn default() -> Self {
        Self::with_lane(disposal_lane())
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl DisposalProducer {
    fn with_lane(lane: DisposalLane) -> Self {
        Self {
            inner: Rc::new(DisposalProducerInner {
                lane,
                latest: RefCell::default(),
                retry_source: RefCell::default(),
                submitted_jobs: Cell::new(0),
                admitted_jobs: Cell::new(0),
                immediate_full_outcomes: Cell::new(0),
                closed_outcomes: Cell::new(0),
                closed: Cell::new(false),
            }),
        }
    }

    fn submit_job(&self, job: DisposalJob) {
        assert!(
            job.weight() <= MAX_SMALL_PENDING_DISPOSAL_BYTES,
            "document-sized values must reserve disposal before crossing onto GTK"
        );
        let inner = &self.inner;
        inner
            .submitted_jobs
            .set(inner.submitted_jobs.get().saturating_add(1));
        if inner.closed.get() {
            inner
                .closed_outcomes
                .set(inner.closed_outcomes.get().saturating_add(1));
            drop(job);
            return;
        }

        if inner.latest.borrow().snapshot().pending_jobs > 0 {
            let replaced = inner.latest.borrow_mut().replace(job);
            drop(replaced);
            self.arm_retry_source();
            return;
        }

        match inner.lane.try_submit(job) {
            Ok(()) => inner
                .admitted_jobs
                .set(inner.admitted_jobs.get().saturating_add(1)),
            Err(DisposalSubmitError::Full(job)) => {
                inner
                    .immediate_full_outcomes
                    .set(inner.immediate_full_outcomes.get().saturating_add(1));
                let replaced = inner.latest.borrow_mut().replace(job);
                debug_assert!(replaced.is_none());
                drop(replaced);
                self.arm_retry_source();
            }
            Err(DisposalSubmitError::Closed(job)) => {
                inner
                    .closed_outcomes
                    .set(inner.closed_outcomes.get().saturating_add(1));
                drop(job);
            }
        }
    }

    fn arm_retry_source(&self) {
        if self.inner.retry_source.borrow().is_some() || self.inner.closed.get() {
            return;
        }
        let inner = Rc::clone(&self.inner);
        let source =
            glib::timeout_add_local(DISPOSAL_RETRY_INTERVAL, move || retry_pending(&inner));
        self.inner.retry_source.replace(Some(source));
    }

    /// Cancel one generation's pending rejection while keeping the producer reusable.
    pub(crate) fn cancel_pending(&self) {
        if let Some(source) = self.inner.retry_source.take() {
            source.remove();
        }
        let cancelled = self.inner.latest.borrow_mut().cancel();
        drop(cancelled);
    }

    /// Cancel pending ownership permanently when the widget or window ends.
    pub(crate) fn close(&self) {
        if self.inner.closed.replace(true) {
            return;
        }
        self.cancel_pending();
    }

    /// Return direct pending and non-blocking outcome evidence.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub(crate) fn snapshot(&self) -> DisposalProducerSnapshot {
        DisposalProducerSnapshot {
            latest: self.inner.latest.borrow().snapshot(),
            submitted_jobs: self.inner.submitted_jobs.get(),
            admitted_jobs: self.inner.admitted_jobs.get(),
            immediate_full_outcomes: self.inner.immediate_full_outcomes.get(),
            closed_outcomes: self.inner.closed_outcomes.get(),
            owner_closed: self.inner.closed.get(),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn retry_pending(inner: &Rc<DisposalProducerInner>) -> glib::ControlFlow {
    let Some(job) = inner.latest.borrow_mut().take_for_retry() else {
        inner.latest.borrow_mut().finish_retry();
        inner.retry_source.take();
        return glib::ControlFlow::Break;
    };
    match inner.lane.try_submit(job) {
        Ok(()) => {
            inner
                .admitted_jobs
                .set(inner.admitted_jobs.get().saturating_add(1));
            inner.latest.borrow_mut().finish_retry();
            inner.retry_source.take();
            glib::ControlFlow::Break
        }
        Err(DisposalSubmitError::Full(job)) => {
            inner.latest.borrow_mut().restore_after_full(job);
            glib::ControlFlow::Continue
        }
        Err(DisposalSubmitError::Closed(job)) => {
            inner
                .closed_outcomes
                .set(inner.closed_outcomes.get().saturating_add(1));
            drop(job);
            inner.latest.borrow_mut().finish_retry();
            inner.retry_source.take();
            glib::ControlFlow::Break
        }
    }
}

/// Return process-wide lane limits for direct regression evidence.
#[cfg(feature = "test-utils")]
#[must_use]
pub fn limits_for_test() -> PlainDisposalLimits {
    DISPOSAL_LIMITS
}

/// Return the reserved progress-lane limits for direct regression evidence.
#[cfg(feature = "test-utils")]
#[must_use]
pub fn progress_limits_for_test() -> PlainDisposalLimits {
    PROGRESS_DISPOSAL_LIMITS
}

/// Return process-wide current and high-water lane evidence.
#[cfg(feature = "test-utils")]
#[must_use]
pub fn lane_snapshot_for_test() -> PlainDisposalSnapshot {
    disposal_lane().snapshot()
}

/// Return current and high-water evidence for the reserved progress lane.
#[cfg(feature = "test-utils")]
#[must_use]
pub fn progress_lane_snapshot_for_test() -> PlainDisposalSnapshot {
    progress_disposal_lane().snapshot()
}

/// Test-owned exclusive reservation that keeps production admission saturated.
#[cfg(feature = "test-utils")]
pub struct DisposalCapacityHold {
    _reservation: DisposalReservation,
}

/// Test-owned exclusive reservation for deterministic progress-lane deferral.
#[cfg(feature = "test-utils")]
pub struct ProgressDisposalCapacityHold {
    _reservation: DisposalReservation,
}

/// Hold the process-wide lane at exclusive byte capacity until the guard drops.
///
/// # Panics
///
/// Panics when another test still owns production disposal capacity.
#[cfg(feature = "test-utils")]
#[must_use]
pub fn hold_disposal_capacity_for_test() -> DisposalCapacityHold {
    let weight = DISPOSAL_RETAINED_BYTE_CAPACITY.saturating_add(1);
    let reservation = try_reserve_for_gtk(weight)
        .expect("test disposal capacity hold requires an otherwise-empty lane");
    DisposalCapacityHold {
        _reservation: reservation,
    }
}

/// Hold the reserved progress lane until the guard drops.
///
/// # Panics
///
/// Panics when another test still owns progress-lane disposal capacity.
#[cfg(feature = "test-utils")]
#[must_use]
pub fn hold_progress_disposal_capacity_for_test() -> ProgressDisposalCapacityHold {
    let weight = PROGRESS_DISPOSAL_RETAINED_BYTE_CAPACITY.saturating_add(1);
    let reservation = try_reserve_progress_for_gtk(weight)
        .expect("test progress capacity hold requires an otherwise-empty lane");
    ProgressDisposalCapacityHold {
        _reservation: reservation,
    }
}

/// Aggregate multi-producer pressure evidence without payload contents.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisposalPressureEvidence {
    /// Producers that independently retained one latest rejected payload.
    pub producers: usize,
    /// Immediate full outcomes observed before any worker capacity was released.
    pub immediate_full_outcomes: u64,
    /// Largest pending count observed for any producer.
    pub producer_pending_high_water: usize,
    /// Largest retry-source count observed for any producer.
    pub producer_retry_high_water: usize,
    /// Pending jobs cancelled by exact owner teardown.
    pub teardown_cancellations: u64,
    /// GTK idle heartbeats dispatched while workers and the queue remained full.
    pub gtk_heartbeat_turns: usize,
    /// Largest executing-job count observed by the lane.
    pub running_high_water: usize,
    /// Largest queued-job count observed by the lane.
    pub queued_high_water: usize,
    /// Largest ordinary caller-estimated byte ownership observed by the lane.
    pub retained_bytes_high_water: u64,
    /// Jobs that reached a worker terminal after capacity was released.
    pub completed_jobs: u64,
    /// Accepted or cancelled producer jobs that reached exact compact terminals.
    pub producer_terminals: usize,
    /// Pending producer jobs remaining after the final drain.
    pub final_pending_jobs: usize,
    /// Pre-admitted nested owners whose final destructor completed off GTK.
    pub preadmitted_worker_drops: usize,
}

/// Exercise the production producer contract against a small saturated test lane.
///
/// # Panics
///
/// Panics when the deterministic pressure fixture violates an admission,
/// ownership, GTK-progress, teardown, or terminal-drain invariant.
#[cfg(feature = "test-utils")]
#[must_use]
pub fn aggregate_pressure_evidence_for_test() -> DisposalPressureEvidence {
    use std::sync::Condvar;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    const MIB: u64 = 1024 * 1024;
    const PRODUCER_COUNT: usize = 4;
    const SUBMISSIONS_PER_PRODUCER: usize = 3;

    let lane = DisposalLane::new(PlainDisposalLimits::new(2, 2, 4 * MIB));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let started = Arc::new(AtomicUsize::new(0));
    for _ in 0..2 {
        let release = Arc::clone(&release);
        let started = Arc::clone(&started);
        lane.try_submit(DisposalJob::new(MIB, move || {
            started.fetch_add(1, Ordering::AcqRel);
            let (lock, wake) = &*release;
            let mut released = lock_unpoisoned(lock);
            while !*released {
                released = wake
                    .wait(released)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            drop(released);
        }))
        .expect("blocking pressure job admitted");
    }
    let start_deadline = Instant::now() + Duration::from_secs(2);
    while started.load(Ordering::Acquire) != 2 && Instant::now() < start_deadline {
        std::thread::yield_now();
    }
    assert_eq!(started.load(Ordering::Acquire), 2);
    for _ in 0..2 {
        lane.try_submit(DisposalJob::new(MIB, || {}))
            .expect("queued pressure job admitted");
    }

    let producer_terminals = Arc::new(AtomicUsize::new(0));
    let producers = (0..PRODUCER_COUNT)
        .map(|_| DisposalProducer::with_lane(lane.clone()))
        .collect::<Vec<_>>();
    for producer in &producers {
        for _ in 0..SUBMISSIONS_PER_PRODUCER {
            let payload = vec![0u8; 16 * 1024];
            let terminal = Arc::clone(&producer_terminals);
            producer.submit_job(DisposalJob::with_terminal(
                16 * 1024,
                move || drop(payload),
                move || {
                    terminal.fetch_add(1, Ordering::AcqRel);
                },
            ));
        }
    }

    let teardown = DisposalProducer::with_lane(lane.clone());
    let teardown_terminal = Arc::clone(&producer_terminals);
    teardown.submit_job(DisposalJob::with_terminal(
        16 * 1024,
        || {},
        move || {
            teardown_terminal.fetch_add(1, Ordering::AcqRel);
        },
    ));
    let teardown_before = teardown.snapshot();
    assert_eq!(teardown_before.latest.pending_jobs, 1);
    teardown.close();
    let teardown_after = teardown.snapshot();

    let pre_release = producers
        .iter()
        .map(DisposalProducer::snapshot)
        .collect::<Vec<_>>();
    let immediate_full_outcomes = pre_release
        .iter()
        .map(|snapshot| snapshot.immediate_full_outcomes)
        .sum::<u64>()
        .saturating_add(teardown_before.immediate_full_outcomes);
    let producer_pending_high_water = pre_release
        .iter()
        .map(|snapshot| snapshot.latest.pending_high_water)
        .max()
        .unwrap_or_default();
    let producer_retry_high_water = pre_release
        .iter()
        .map(|snapshot| snapshot.latest.retry_high_water)
        .max()
        .unwrap_or_default();
    assert!(
        pre_release
            .iter()
            .all(|snapshot| snapshot.latest.pending_jobs == 1),
        "every saturated producer retains exactly one latest job"
    );

    let heartbeat = Rc::new(Cell::new(0usize));
    let heartbeat_clone = Rc::clone(&heartbeat);
    glib::idle_add_local_once(move || heartbeat_clone.set(1));
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
    assert_eq!(heartbeat.get(), 1, "GTK heartbeat must run before release");

    {
        let (lock, wake) = &*release;
        *lock_unpoisoned(lock) = true;
        wake.notify_all();
    }

    let drain_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        while context.pending() {
            context.iteration(false);
        }
        let lane_snapshot = lane.snapshot();
        let pending = producers
            .iter()
            .map(|producer| producer.snapshot().latest.pending_jobs)
            .sum::<usize>();
        if pending == 0 && lane_snapshot.running_jobs == 0 && lane_snapshot.queued_jobs == 0 {
            break;
        }
        assert!(
            Instant::now() < drain_deadline,
            "disposal pressure must drain"
        );
        std::thread::sleep(Duration::from_millis(1));
    }

    let final_pending_jobs = producers
        .iter()
        .map(|producer| producer.snapshot().latest.pending_jobs)
        .sum();
    for producer in &producers {
        producer.close();
    }
    lane.close();
    let lane_snapshot = lane.snapshot();
    let expected_terminals = PRODUCER_COUNT
        .saturating_mul(SUBMISSIONS_PER_PRODUCER)
        .saturating_add(1);
    assert_eq!(
        producer_terminals.load(Ordering::Acquire),
        expected_terminals,
        "every admitted, replaced, or cancelled producer owner reaches a terminal"
    );

    let preadmitted_lane = DisposalLane::new(PlainDisposalLimits::new(1, 1, 2 * MIB));
    let permit = preadmitted_lane
        .try_reserve(MIB)
        .expect("document-sized final owner reserves before GTK publication");
    let preadmitted_worker_drops = Arc::new(AtomicUsize::new(0));
    let worker_drops = Arc::clone(&preadmitted_worker_drops);
    let gtk_thread = std::thread::current().id();
    let owner = DisposalOwned::new(
        (0..4_096)
            .map(|index| format!("nested-disposal-{index}"))
            .collect::<Vec<_>>(),
        permit,
    )
    .with_disposal_terminal(move || {
        if std::thread::current().id() != gtk_thread {
            worker_drops.fetch_add(1, Ordering::AcqRel);
        }
    });
    drop(owner);
    wait_until_for_pressure(|| preadmitted_lane.snapshot().completed_jobs == 1);

    DisposalPressureEvidence {
        producers: PRODUCER_COUNT,
        immediate_full_outcomes,
        producer_pending_high_water,
        producer_retry_high_water,
        teardown_cancellations: teardown_after.latest.cancellations,
        gtk_heartbeat_turns: heartbeat.get(),
        running_high_water: lane_snapshot.running_high_water,
        queued_high_water: lane_snapshot.queued_high_water,
        retained_bytes_high_water: lane_snapshot.retained_bytes_high_water,
        completed_jobs: lane_snapshot.completed_jobs,
        producer_terminals: producer_terminals.load(Ordering::Acquire),
        final_pending_jobs,
        preadmitted_worker_drops: preadmitted_worker_drops.load(Ordering::Acquire),
    }
}

#[cfg(feature = "test-utils")]
fn wait_until_for_pressure(predicate: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !predicate() && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(
        predicate(),
        "pressure fixture did not reach its worker terminal"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    struct ThreadObservedDrop {
        thread_tx: Option<mpsc::Sender<std::thread::ThreadId>>,
        nested: Vec<String>,
    }

    impl Drop for ThreadObservedDrop {
        fn drop(&mut self) {
            let _ = self.nested.len();
            if let Some(thread_tx) = self.thread_tx.take() {
                thread_tx
                    .send(std::thread::current().id())
                    .expect("drop thread receiver");
            }
        }
    }

    fn wait_until(predicate: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !predicate() && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(predicate(), "condition did not become true before deadline");
    }

    #[test]
    fn full_and_closed_outcomes_return_job_ownership_immediately() {
        let lane = DisposalLane::new(PlainDisposalLimits::new(1, 1, 2));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        lane.try_submit(DisposalJob::new(1, move || {
            started_tx.send(()).expect("started");
            release_rx.recv().expect("released");
        }))
        .expect("active job admitted");
        started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("worker started");
        lane.try_submit(DisposalJob::new(1, || {}))
            .expect("queued job admitted");

        let dropped = Arc::new(AtomicUsize::new(0));
        let dropped_on_run = Arc::clone(&dropped);
        let full = lane
            .try_submit(DisposalJob::new(1, move || {
                dropped_on_run.fetch_add(1, Ordering::AcqRel);
            }))
            .expect_err("saturated lane returns full");
        assert!(matches!(full, DisposalSubmitError::Full(_)));
        assert_eq!(dropped.load(Ordering::Acquire), 0);
        drop(full.into_job());
        assert_eq!(
            dropped.load(Ordering::Acquire),
            0,
            "returned job did not run"
        );

        release_tx.send(()).expect("release worker");
        wait_until(|| lane.snapshot().completed_jobs == 2);
        lane.close();

        let closed = lane
            .try_submit(DisposalJob::new(1, || {}))
            .expect_err("closed lane returns ownership");
        assert!(matches!(closed, DisposalSubmitError::Closed(_)));
        assert_eq!(lane.snapshot().closed_outcomes, 1);
    }

    #[test]
    fn overweight_progress_and_panics_release_worker_accounting() {
        let lane = DisposalLane::new(PlainDisposalLimits::new(1, 1, 10));
        lane.try_submit(DisposalJob::new(11, || {
            panic!("intentional disposal panic")
        }))
        .expect("empty lane admits one overweight job");
        wait_until(|| lane.snapshot().completed_jobs == 1);
        let after_panic = lane.snapshot();
        assert_eq!(after_panic.running_jobs, 0);
        assert_eq!(after_panic.queued_jobs, 0);
        assert_eq!(after_panic.retained_bytes, 0);
        assert_eq!(after_panic.panicked_jobs, 1);
        assert_eq!(after_panic.overweight_bytes_high_water, 11);

        lane.try_submit(DisposalJob::new(10, || {}))
            .expect("ordinary work progresses after panic");
        wait_until(|| lane.snapshot().completed_jobs == 2);
    }

    #[test]
    fn dropped_job_runs_compact_terminal_without_running_payload() {
        let disposed = Arc::new(AtomicUsize::new(0));
        let terminal = Arc::new(AtomicUsize::new(0));
        let disposed_clone = Arc::clone(&disposed);
        let terminal_clone = Arc::clone(&terminal);
        drop(DisposalJob::with_terminal(
            1,
            move || {
                disposed_clone.fetch_add(1, Ordering::AcqRel);
            },
            move || {
                terminal_clone.fetch_add(1, Ordering::AcqRel);
            },
        ));
        assert_eq!(disposed.load(Ordering::Acquire), 0);
        assert_eq!(terminal.load(Ordering::Acquire), 1);
    }

    #[test]
    fn preadmitted_last_owner_hands_nested_destruction_to_worker() {
        let lane = DisposalLane::new(PlainDisposalLimits::new(1, 2, 1024));
        let permit = lane.try_reserve(512).expect("future disposal reservation");
        let (thread_tx, thread_rx) = mpsc::channel();
        let owner = DisposalOwned::new(
            ThreadObservedDrop {
                thread_tx: Some(thread_tx),
                nested: (0..128).map(|index| format!("nested-{index}")).collect(),
            },
            permit,
        );
        let gtk_thread = std::thread::current().id();

        drop(owner);

        let destructor_thread = thread_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("worker-side final destructor");
        assert_ne!(destructor_thread, gtk_thread);
        wait_until(|| lane.snapshot().completed_jobs == 1);
        assert_eq!(lane.snapshot().retained_bytes, 0);
    }
}
