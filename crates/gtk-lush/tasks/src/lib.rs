// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bounded background task dispatch helpers for gtk-rs applications.
//!
//! `spawn_blocking_then` runs blocking work on a background thread, applies a
//! small process-wide concurrency limit, and returns the result to GLib's main
//! loop. Non-`Send` GTK-thread state is carried with `glib::thread_guard` and
//! only recovered in the main-thread completion callback.
//!
//! The crate deliberately keeps application freshness rules out of the worker
//! dispatcher. Use `FreshnessToken` to make generation checks typed, then keep
//! the actual tab, path, search, or persistence policy beside the caller.
//!
//! # Example
//!
//! ```no_run
//! use gtk_lush_tasks::{FreshnessToken, spawn_blocking_then};
//!
//! let requested = FreshnessToken::new(7);
//! let current = FreshnessToken::new(7);
//!
//! spawn_blocking_then(
//!     requested,
//!     || String::from("loaded text"),
//!     move |token, text| {
//!         if let Ok(fresh) = token.accept(current, text) {
//!             assert_eq!(fresh.into_inner(), "loaded text");
//!         }
//!     },
//! );
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};

use glib::Object;
use glib::prelude::*;

/// Default maximum number of concurrent background workers.
///
/// The limit is intentionally small. GTK applications often spawn blocking
/// work from UI bursts such as session restore or tree expansion, and bounded
/// dispatch avoids turning slow filesystems into unbounded thread and memory
/// pressure.
pub const DEFAULT_MAX_CONCURRENT_SPAWNS: usize = 8;

/// Process-wide worker count shared by all dispatch calls in this crate.
///
/// GTK apps usually have one main loop, so a process-wide cap is simpler and
/// more predictable than per-widget pools. Tests exercise the pure acquisition
/// helper with caller-owned counters to avoid relying on global state.
static ACTIVE_THREADS: AtomicUsize = AtomicUsize::new(0);

type PendingTask = Box<dyn FnOnce() + 'static>;

thread_local! {
    /// Main-thread FIFO of work waiting for a worker slot.
    ///
    /// The queued closures may contain GTK-thread state protected by
    /// `ThreadGuard`, so this queue intentionally stays thread-local instead of
    /// using a `Send` global executor. Slot release wakes the queue through
    /// GLib's main context.
    static PENDING_TASKS: RefCell<VecDeque<PendingTask>> = RefCell::new(VecDeque::new());
}

/// Return the number of currently active background workers.
#[must_use]
pub fn active_worker_count() -> usize {
    ACTIVE_THREADS.load(Ordering::Acquire)
}

fn try_acquire_slot() -> bool {
    try_acquire_slot_with(&ACTIVE_THREADS, DEFAULT_MAX_CONCURRENT_SPAWNS)
}

fn try_acquire_slot_with(counter: &AtomicUsize, limit: usize) -> bool {
    counter
        .try_update(Ordering::AcqRel, Ordering::Relaxed, |active| {
            (active < limit).then_some(active + 1)
        })
        .is_ok()
}

fn release_slot_count() {
    ACTIVE_THREADS.fetch_sub(1, Ordering::Release);
}

fn release_slot_and_wake_queue() {
    release_slot_count();
    glib::MainContext::default().invoke(drain_pending_tasks);
}

/// RAII worker-slot release guard.
///
/// The guard lives through the whole result lifecycle. Worker panics release it
/// on the worker thread, while successful work moves it into the GLib idle
/// callback so large results remain covered by the concurrency cap until the
/// main loop has consumed them.
struct SlotGuard;

impl Drop for SlotGuard {
    fn drop(&mut self) {
        release_slot_and_wake_queue();
    }
}

fn start_or_queue(task: PendingTask) {
    if try_acquire_slot() {
        task();
        return;
    }
    PENDING_TASKS.with(|queue| queue.borrow_mut().push_back(task));
}

fn drain_pending_tasks() {
    loop {
        let Some(task) = PENDING_TASKS.with(|queue| queue.borrow_mut().pop_front()) else {
            break;
        };
        if try_acquire_slot() {
            task();
        } else {
            PENDING_TASKS.with(|queue| queue.borrow_mut().push_front(task));
            break;
        }
    }
}

fn start_worker<S, T, W, F>(state: S, work: W, then: F)
where
    S: 'static,
    T: Send + 'static,
    W: FnOnce() -> T + Send + 'static,
    F: FnOnce(S, T) + 'static,
{
    let guarded_state = glib::thread_guard::ThreadGuard::new(state);
    let guarded_then = glib::thread_guard::ThreadGuard::new(then);
    std::thread::spawn(move || {
        let slot_guard = SlotGuard;
        let result = work();
        glib::idle_add_once(move || {
            let _slot_guard = slot_guard;
            let state = guarded_state.into_inner();
            let then = guarded_then.into_inner();
            then(state, result);
        });
    });
}

/// Run blocking work on a background thread and deliver its result on GLib's main loop.
///
/// `state` is GTK-thread state that belongs to the completion callback. It is
/// wrapped in `glib::thread_guard::ThreadGuard` before the worker starts, so the
/// background thread can move the value without touching it. The value is
/// recovered only inside the main-loop callback.
///
/// If all worker slots are busy, the work waits in a main-thread FIFO and is
/// started when an active result has been consumed by the GLib main loop.
pub fn spawn_blocking_then<S, T, W, F>(state: S, work: W, then: F)
where
    S: 'static,
    T: Send + 'static,
    W: FnOnce() -> T + Send + 'static,
    F: FnOnce(S, T) + 'static,
{
    start_or_queue(Box::new(move || {
        start_worker(state, work, then);
    }));
}

/// Run blocking work and call back only if the target object is still alive.
///
/// This helper is for ordinary widget/object lifetime checks. It does not
/// replace application freshness rules: a live widget may still be showing a
/// different tab, path, search, or generation by the time the work completes.
pub fn spawn_blocking_then_weak<S, T, W, F>(target: &S, work: W, then: F)
where
    S: IsA<Object> + Clone + 'static,
    T: Send + 'static,
    W: FnOnce() -> T + Send + 'static,
    F: FnOnce(S, T) + 'static,
{
    let target_weak = target.downgrade();
    spawn_blocking_then(target_weak, work, move |target_weak, result| {
        let Some(target) = target_weak.upgrade() else {
            return;
        };
        then(target, result);
    });
}

/// Opaque generation token captured before asynchronous work starts.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FreshnessToken(u64);

impl FreshnessToken {
    /// Create a token from an application-owned generation number.
    #[must_use]
    pub const fn new(generation: u64) -> Self {
        Self(generation)
    }

    /// Return the application generation represented by this token.
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.0
    }

    /// Return whether this token still matches the current token.
    #[must_use]
    pub const fn is_current(self, current: Self) -> bool {
        self.0 == current.0
    }

    /// Wrap a value as fresh only when this token matches `current`.
    ///
    /// The caller chooses what the generation means. This helper only makes the
    /// check explicit and keeps stale values from being silently applied.
    ///
    /// # Errors
    ///
    /// Returns `Stale<T>` with the original value when this token does not
    /// match the caller-provided current token.
    pub fn accept<T>(self, current: Self, value: T) -> Result<Fresh<T>, Stale<T>> {
        if self.is_current(current) {
            Ok(Fresh { token: self, value })
        } else {
            Err(Stale {
                requested: self,
                current,
                value,
            })
        }
    }
}

/// A value that passed a caller-owned freshness check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fresh<T> {
    token: FreshnessToken,
    value: T,
}

impl<T> Fresh<T> {
    /// Return the token that accepted this value.
    #[must_use]
    pub const fn token(&self) -> FreshnessToken {
        self.token
    }

    /// Borrow the accepted value.
    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// Consume the wrapper and return the accepted value.
    #[must_use]
    pub fn into_inner(self) -> T {
        self.value
    }
}

/// A value rejected by a caller-owned freshness check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stale<T> {
    requested: FreshnessToken,
    current: FreshnessToken,
    value: T,
}

impl<T> Stale<T> {
    /// Return the token captured when the asynchronous work was requested.
    #[must_use]
    pub const fn requested(&self) -> FreshnessToken {
        self.requested
    }

    /// Return the token that was current when the result was checked.
    #[must_use]
    pub const fn current(&self) -> FreshnessToken {
        self.current
    }

    /// Borrow the rejected value.
    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// Consume the wrapper and return the rejected value.
    #[must_use]
    pub fn into_inner(self) -> T {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use std::panic;
    use std::sync::Mutex;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::*;

    static ACTIVE_THREADS_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_counter() -> std::sync::MutexGuard<'static, ()> {
        ACTIVE_THREADS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn pump_until<T>(receiver: &mpsc::Receiver<T>, timeout: Duration) -> Option<T> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            while glib::MainContext::default().iteration(false) {}
            if let Ok(result) = receiver.try_recv() {
                return Some(result);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        None
    }

    fn clear_pending_tasks() {
        PENDING_TASKS.with(|queue| queue.borrow_mut().clear());
    }

    fn pending_task_count() -> usize {
        PENDING_TASKS.with(|queue| queue.borrow().len())
    }

    #[test]
    fn slot_guard_releases_slot_on_panic() {
        let _counter_guard = lock_counter();
        ACTIVE_THREADS.store(1, Ordering::Relaxed);

        let result = panic::catch_unwind(|| {
            let _slot_guard = SlotGuard;
            panic!("boom");
        });

        assert!(result.is_err());
        assert_eq!(ACTIVE_THREADS.load(Ordering::Relaxed), 0);
        clear_pending_tasks();
    }

    #[test]
    fn slot_limit_saturates_and_releases() {
        let counter = AtomicUsize::new(0);

        for expected in 1..=DEFAULT_MAX_CONCURRENT_SPAWNS {
            assert!(try_acquire_slot_with(
                &counter,
                DEFAULT_MAX_CONCURRENT_SPAWNS
            ));
            assert_eq!(counter.load(Ordering::Relaxed), expected);
        }

        assert!(!try_acquire_slot_with(
            &counter,
            DEFAULT_MAX_CONCURRENT_SPAWNS
        ));
        assert_eq!(
            counter.load(Ordering::Relaxed),
            DEFAULT_MAX_CONCURRENT_SPAWNS
        );
    }

    #[test]
    fn spawn_blocking_then_holds_slot_until_callback_consumes_result() {
        let _counter_guard = lock_counter();
        clear_pending_tasks();
        ACTIVE_THREADS.store(0, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();

        spawn_blocking_then(
            (),
            || 42,
            move |(), result| {
                let _ = sender.send(result);
            },
        );

        std::thread::sleep(Duration::from_millis(75));
        assert_eq!(
            ACTIVE_THREADS.load(Ordering::Relaxed),
            1,
            "completed work should keep its slot until the main loop consumes the result"
        );
        assert_eq!(pump_until(&receiver, Duration::from_secs(1)), Some(42));
        assert_eq!(ACTIVE_THREADS.load(Ordering::Relaxed), 0);
        clear_pending_tasks();
        ACTIVE_THREADS.store(0, Ordering::Relaxed);
    }

    #[test]
    fn saturated_work_waits_in_fifo_until_a_slot_releases() {
        let _counter_guard = lock_counter();
        clear_pending_tasks();
        ACTIVE_THREADS.store(DEFAULT_MAX_CONCURRENT_SPAWNS, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();

        spawn_blocking_then(
            (),
            || 7,
            move |(), result| {
                let _ = sender.send(result);
            },
        );

        assert_eq!(pending_task_count(), 1);
        ACTIVE_THREADS.store(DEFAULT_MAX_CONCURRENT_SPAWNS - 1, Ordering::Relaxed);
        drain_pending_tasks();

        assert_eq!(pending_task_count(), 0);
        assert_eq!(pump_until(&receiver, Duration::from_secs(1)), Some(7));
        ACTIVE_THREADS.store(0, Ordering::Relaxed);
        clear_pending_tasks();
    }

    #[test]
    fn weak_target_completion_is_skipped_after_drop() {
        let _counter_guard = lock_counter();
        clear_pending_tasks();
        ACTIVE_THREADS.store(0, Ordering::Relaxed);
        let target = Object::new::<Object>();
        let (sender, receiver) = mpsc::channel();

        spawn_blocking_then_weak(
            &target,
            || 9,
            move |_, result| {
                let _ = sender.send(result);
            },
        );
        drop(target);

        assert_eq!(pump_until(&receiver, Duration::from_millis(200)), None);
        ACTIVE_THREADS.store(0, Ordering::Relaxed);
        clear_pending_tasks();
    }

    #[test]
    fn freshness_token_accepts_only_current_generation() {
        let requested = FreshnessToken::new(3);
        let current = FreshnessToken::new(4);

        let stale = requested
            .accept(current, "value")
            .expect_err("different generations are stale");
        assert_eq!(stale.requested(), requested);
        assert_eq!(stale.current(), current);
        assert_eq!(stale.into_inner(), "value");

        let fresh = current
            .accept(current, "new")
            .expect("same generation is fresh");
        assert_eq!(fresh.token(), current);
        assert_eq!(fresh.into_inner(), "new");
    }
}
