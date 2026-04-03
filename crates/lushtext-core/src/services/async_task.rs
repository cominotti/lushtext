// SPDX-License-Identifier: GPL-3.0-or-later

//! Utility for running blocking work on a background thread and dispatching
//! the result back to the GTK main thread.

use gtk4::glib;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Maximum concurrent background threads. Prevents RAM spikes during
/// session restore (many file loads) or rapid tree expansion.
const MAX_CONCURRENT_SPAWNS: usize = 8;

static ACTIVE_THREADS: AtomicUsize = AtomicUsize::new(0);

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

    let guarded_state = glib::thread_guard::ThreadGuard::new(state);
    let guarded_then = glib::thread_guard::ThreadGuard::new(then);
    std::thread::spawn(move || {
        let result = work();
        release_slot();
        glib::idle_add_once(move || {
            let state = guarded_state.into_inner();
            let then = guarded_then.into_inner();
            then(state, result);
        });
    });
}
