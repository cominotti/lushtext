// SPDX-License-Identifier: GPL-3.0-or-later

//! Utility for running blocking work on a background thread and dispatching
//! the result back to the GTK main thread.

use gtk4::glib;

/// Run `work` on a background thread, then call `then` on the main thread
/// with the result.
///
/// `state` is a non-Send value (typically a GTK object) that will be passed
/// to `then` on the main thread. Both `state` and `then` are wrapped in
/// `ThreadGuard` to safely cross the thread boundary.
pub fn spawn_blocking_then<S, T, W, F>(state: S, work: W, then: F)
where
    S: 'static,
    T: Send + 'static,
    W: FnOnce() -> T + Send + 'static,
    F: FnOnce(S, T) + 'static,
{
    let guarded_state = glib::thread_guard::ThreadGuard::new(state);
    let guarded_then = glib::thread_guard::ThreadGuard::new(then);
    std::thread::spawn(move || {
        let result = work();
        glib::idle_add_once(move || {
            let state = guarded_state.into_inner();
            let then = guarded_then.into_inner();
            then(state, result);
        });
    });
}
