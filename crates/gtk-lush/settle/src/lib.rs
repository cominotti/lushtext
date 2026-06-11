// SPDX-License-Identifier: MIT OR Apache-2.0

//! Generation-counter scheduling helpers for gtk-rs main-loop work.
//!
//! This crate provides three tiny primitives for common UI timing contracts:
//! `Debounce` for trailing latest-input work, `SettleBurst` for repair after a
//! quiet window, and `SupersedingTimer` for delayed cleanup where the newest
//! arm wins.
//!
//! GTK Lush crates remain independently adoptable leaf crates. They do not own
//! GTK control flow, define a view DSL, add a state/message framework, depend
//! on another GTK Lush crate, or replace Libadwaita adaptive behavior.
//!
//! # Example
//!
//! ```no_run
//! use std::time::Duration;
//!
//! use glib::Object;
//! use gtk_lush_settle::Debounce;
//!
//! let debounce = Debounce::new();
//! let label = Object::new::<Object>();
//!
//! debounce.schedule(&label, Duration::from_millis(150), |_, _| {
//!     // Update a GTK widget here after the input burst has quieted.
//! });
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use glib::prelude::*;
use glib::{Object, WeakRef};

/// Opaque generation captured by a scheduled timer callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerToken(u32);

impl TimerToken {
    /// Return the numeric generation for workflows that persist or report it.
    #[must_use]
    pub fn value(self) -> u32 {
        self.0
    }
}

/// Shared generation state cloned into GLib timer closures.
#[derive(Debug, Default)]
struct GenerationGate {
    /// Current generation. `Cell` keeps mutation main-thread-local and cheap.
    generation: Cell<u32>,
}

impl GenerationGate {
    /// Advance the generation and return the token the caller should capture.
    fn advance(&self) -> TimerToken {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        TimerToken(generation)
    }

    /// Invalidate already-scheduled callbacks without scheduling replacement work.
    fn invalidate(&self) -> TimerToken {
        self.advance()
    }

    /// Report whether the token still belongs to the newest scheduled operation.
    fn is_current(&self, token: TimerToken) -> bool {
        self.generation.get() == token.0
    }
}

/// Upgrade `target_weak` and run `callback` only if `token` is still current.
fn run_if_current<T, F>(
    target_weak: &WeakRef<T>,
    gate: &GenerationGate,
    token: TimerToken,
    callback: F,
) where
    T: IsA<Object> + Clone + 'static,
    F: FnOnce(T, TimerToken),
{
    let Some(target) = target_weak.upgrade() else {
        return;
    };
    if !gate.is_current(token) {
        return;
    }
    callback(target, token);
}

/// Debounces trailing main-loop work so only the newest request runs.
///
/// `Debounce` is for rapid-fire UI input and persistence coalescing. It uses
/// GLib's main loop for scheduling and a generation token for cancellation, so
/// stale callbacks no-op instead of relying on fragile source removal.
#[derive(Clone, Debug, Default)]
pub struct Debounce {
    gate: Rc<GenerationGate>,
}

impl Debounce {
    /// Create a debounce with no pending generation.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance the debounce generation for immediate or async work that should
    /// supersede any pending timer.
    #[must_use]
    pub fn advance(&self) -> TimerToken {
        self.gate.advance()
    }

    /// Invalidate pending callbacks without scheduling a replacement.
    #[must_use]
    pub fn invalidate(&self) -> TimerToken {
        self.gate.invalidate()
    }

    /// Check whether a previously captured token is still current.
    #[must_use]
    pub fn is_current(&self, token: TimerToken) -> bool {
        self.gate.is_current(token)
    }

    /// Schedule a trailing callback on GTK's main loop.
    ///
    /// The target is captured weakly. If the target is destroyed or a newer
    /// generation is scheduled before the timer fires, the callback does not
    /// run.
    pub fn schedule<T, F>(&self, target: &T, delay: Duration, callback: F) -> TimerToken
    where
        T: IsA<Object> + Clone + 'static,
        F: FnOnce(T, TimerToken) + 'static,
    {
        let token = self.advance();
        let gate = Rc::clone(&self.gate);
        let target_weak = target.downgrade();
        glib::timeout_add_local_once(delay, move || {
            run_if_current(&target_weak, &gate, token, callback);
        });
        token
    }
}

/// Delayed one-shot timer where each arm supersedes the previous arm.
///
/// This is useful for UI cleanup such as pulse removal or delayed reveal/hide
/// decisions. It does not own a custom runtime; callbacks run on GLib's main
/// loop when their current generation fires.
#[derive(Clone, Debug, Default)]
pub struct SupersedingTimer {
    gate: Rc<GenerationGate>,
}

impl SupersedingTimer {
    /// Create an unarmed superseding timer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Arm the timer, replacing any earlier arm-generation.
    pub fn arm<T, F>(&self, target: &T, delay: Duration, callback: F) -> TimerToken
    where
        T: IsA<Object> + Clone + 'static,
        F: FnOnce(T, TimerToken) + 'static,
    {
        let token = self.gate.advance();
        let gate = Rc::clone(&self.gate);
        let target_weak = target.downgrade();
        glib::timeout_add_local_once(delay, move || {
            run_if_current(&target_weak, &gate, token, callback);
        });
        token
    }

    /// Invalidate pending cleanup work without scheduling a replacement.
    #[must_use]
    pub fn invalidate(&self) -> TimerToken {
        self.gate.invalidate()
    }

    /// Check whether a previously captured arm token is still current.
    #[must_use]
    pub fn is_current(&self, token: TimerToken) -> bool {
        self.gate.is_current(token)
    }
}

/// Tracks a quiet-window repair burst and exposes whether repair is pending.
///
/// `SettleBurst` is for layout or rendering storms where observers must wait
/// until the final repair completes. Pending state remains true until the
/// current `SettleHandle` finishes.
#[derive(Clone, Debug, Default)]
pub struct SettleBurst {
    gate: Rc<GenerationGate>,
    pending: Rc<Cell<bool>>,
}

impl SettleBurst {
    /// Create a settle burst with no pending work.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedule or extend the burst and run `callback` after the quiet window.
    ///
    /// The returned handle lets multi-stage repairs clear pending state only
    /// after their actual final repair completes.
    pub fn schedule<T, F>(&self, target: &T, delay: Duration, callback: F) -> SettleHandle
    where
        T: IsA<Object> + Clone + 'static,
        F: FnOnce(T, SettleHandle) + 'static,
    {
        let handle = self.begin();
        let callback_handle = handle.clone();
        let target_weak = target.downgrade();
        glib::timeout_add_local_once(delay, move || {
            let Some(target) = target_weak.upgrade() else {
                return;
            };
            if !callback_handle.is_current() {
                return;
            }
            callback(target, callback_handle);
        });
        handle
    }

    /// Begin or supersede a settle burst without installing a GLib source.
    #[must_use]
    pub fn begin(&self) -> SettleHandle {
        self.pending.set(true);
        let token = self.gate.advance();
        SettleHandle {
            gate: Rc::clone(&self.gate),
            pending: Rc::clone(&self.pending),
            token,
        }
    }

    /// Return whether the burst is currently blocking readiness.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.pending.get()
    }

    /// Invalidate callbacks and clear pending state when the workflow disappears.
    #[must_use]
    pub fn clear(&self) -> TimerToken {
        let token = self.gate.invalidate();
        self.pending.set(false);
        token
    }
}

/// Handle for the currently scheduled settle-burst generation.
///
/// A handle is intentionally generation-bound. An older handle cannot clear a
/// newer burst's pending state, which protects readiness predicates from
/// observing a settled-before-repaired state.
#[derive(Clone, Debug)]
pub struct SettleHandle {
    gate: Rc<GenerationGate>,
    pending: Rc<Cell<bool>>,
    token: TimerToken,
}

impl SettleHandle {
    /// Return the generation token represented by this handle.
    #[must_use]
    pub fn token(&self) -> TimerToken {
        self.token
    }

    /// Report whether no newer burst has superseded this handle.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.gate.is_current(self.token)
    }

    /// Clear pending state only if this handle still owns the latest generation.
    pub fn finish_if_current(&self) {
        if self.is_current() {
            self.pending.set(false);
        }
    }

    /// Schedule follow-up work tied to the same settle generation.
    ///
    /// This is for multi-step repairs inside one settle burst, not a general
    /// task scheduler. If a newer burst starts first, the follow-up no-ops.
    pub fn schedule_follow_up<T, F>(&self, target: &T, delay: Duration, callback: F)
    where
        T: IsA<Object> + Clone + 'static,
        F: FnOnce(T) + 'static,
    {
        let handle = self.clone();
        let target_weak = target.downgrade();
        glib::timeout_add_local_once(delay, move || {
            let Some(target) = target_weak.upgrade() else {
                return;
            };
            if handle.is_current() {
                callback(target);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use proptest::prelude::*;

    use super::*;

    #[test]
    fn debounce_tokens_advance_and_reject_stale_tokens() {
        let debounce = Debounce::default();
        let first = debounce.advance();
        let second = debounce.advance();

        assert!(!debounce.is_current(first));
        assert!(debounce.is_current(second));
    }

    #[test]
    fn generation_wrapping_keeps_latest_token_current() {
        let gate = GenerationGate {
            generation: Cell::new(u32::MAX),
        };

        let wrapped = gate.advance();

        assert_eq!(wrapped.value(), 0);
        assert!(gate.is_current(wrapped));
    }

    #[test]
    fn settle_handle_only_clears_current_pending_state() {
        let burst = SettleBurst::default();
        let first = burst.begin();
        let _second = burst.begin();

        first.finish_if_current();

        assert!(burst.pending());
    }

    #[test]
    fn current_settle_handle_clears_pending_after_repair() {
        let burst = SettleBurst::default();
        let handle = burst.begin();

        assert!(burst.pending());

        handle.finish_if_current();

        assert!(!burst.pending());
    }

    #[test]
    fn settle_clear_invalidates_pending_work() {
        let burst = SettleBurst::default();
        let handle = burst.begin();

        let _ = burst.clear();

        assert!(!burst.pending());
        assert!(!handle.is_current());
    }

    #[test]
    fn current_runner_skips_stale_tokens() {
        let gate = GenerationGate::default();
        let stale = gate.advance();
        let _current = gate.advance();
        let target = Object::new::<Object>();
        let target_weak = target.downgrade();
        let ran = Cell::new(false);

        run_if_current(&target_weak, &gate, stale, |_, _| {
            ran.set(true);
        });

        assert!(!ran.get());
    }

    #[test]
    fn current_runner_noops_after_target_drop() {
        let gate = GenerationGate::default();
        let token = gate.advance();
        let target_weak = {
            let target = Object::new::<Object>();
            target.downgrade()
        };
        let ran = Cell::new(false);

        run_if_current(&target_weak, &gate, token, |_, _| {
            ran.set(true);
        });

        assert!(!ran.get());
    }

    #[test]
    fn superseding_timer_invalidation_advances_generation() {
        let timer = SupersedingTimer::default();
        let first = timer.invalidate();
        let second = timer.invalidate();

        assert_ne!(first, second);
    }

    #[test]
    fn superseding_timer_rearm_rejects_stale_token() {
        let timer = SupersedingTimer::default();
        let target = Object::new::<Object>();
        let first = timer.arm(&target, Duration::from_millis(1), |_, _| {});
        let second = timer.arm(&target, Duration::from_millis(1), |_, _| {});

        assert!(!timer.is_current(first));
        assert!(timer.is_current(second));
    }

    proptest! {
        #[test]
        fn latest_generation_is_current_after_any_advances(advance_count in 1u8..64) {
            let gate = GenerationGate::default();
            let mut latest = TimerToken(0);

            for _ in 0..advance_count {
                latest = gate.advance();
            }

            prop_assert!(gate.is_current(latest));
        }

        #[test]
        fn stale_settle_handles_never_clear_current_pending(extra_bursts in 1u8..16) {
            let burst = SettleBurst::default();
            let stale = burst.begin();

            for _ in 0..extra_bursts {
                let _ = burst.begin();
            }

            stale.finish_if_current();

            prop_assert!(burst.pending());
        }
    }
}
