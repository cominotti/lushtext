// SPDX-License-Identifier: GPL-3.0-or-later

//! Utility for running blocking work on a background thread and dispatching
//! the result back to the GTK main thread.

use gtk4::glib;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Maximum concurrent background threads. Prevents RAM spikes during
/// session restore (many file loads) or rapid tree expansion.
const MAX_CONCURRENT_SPAWNS: usize = 8;

/// Current number of active background threads. Compared against
/// `MAX_CONCURRENT_SPAWNS` to apply back-pressure.
static ACTIVE_THREADS: AtomicUsize = AtomicUsize::new(0);

/// Attempt to claim a concurrency slot via a lock-free CAS loop.
/// Returns `true` if under the limit. `compare_exchange_weak` is sufficient
/// (spurious failure just retries) and avoids a full memory fence on ARM.
fn try_acquire_slot() -> bool {
    let mut active = ACTIVE_THREADS.load(Ordering::Relaxed);
    loop {
        if active >= MAX_CONCURRENT_SPAWNS {
            return false;
        }
        match ACTIVE_THREADS.compare_exchange_weak(
            active,
            active + 1,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => return true,
            Err(observed) => active = observed,
        }
    }
}

fn release_slot() {
    ACTIVE_THREADS.fetch_sub(1, Ordering::Release);
}

/// Run `work` on a background thread, then call `then` on the main thread
/// with the result.
///
/// `state` is a non-Send value (typically a GTK object) that will be passed
/// to `then` on the main thread. Both `state` and `then` are wrapped in
/// `ThreadGuard` to safely cross the thread boundary.
///
/// If the concurrency limit is reached, the work is deferred to the next
/// GLib main-loop idle pass (back-pressure without blocking the UI).
pub fn spawn_blocking_then<S, T, W, F>(state: S, work: W, then: F)
where
    S: 'static,
    T: Send + 'static,
    W: FnOnce() -> T + Send + 'static,
    F: FnOnce(S, T) + 'static,
{
    if !try_acquire_slot() {
        // Use a 50ms timeout instead of idle to avoid busy-wait spinning
        // when all slots are saturated (e.g., slow NFS reads).
        glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
            spawn_blocking_then(state, work, then);
        });
        return;
    }

    // ThreadGuard makes non-Send values movable across threads for the type
    // system, but enforces at runtime that they're only accessed from the
    // original (main) thread. The background thread carries the guard without
    // touching the widget inside.
    let guarded_state = glib::thread_guard::ThreadGuard::new(state);
    let guarded_then = glib::thread_guard::ThreadGuard::new(then);
    std::thread::spawn(move || {
        let result = work();
        release_slot();
        // Deliver the result to the main thread via GLib's main loop.
        // GTK widgets can only be accessed from the main thread, so
        // idle_add_once schedules the callback on the next iteration.
        glib::idle_add_once(move || {
            let state = guarded_state.into_inner();
            let then = guarded_then.into_inner();
            then(state, result);
        });
    });
}
