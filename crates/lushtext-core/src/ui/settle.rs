// SPDX-License-Identifier: GPL-3.0-or-later

//! Private GTK main-loop scheduling helpers for superseding timer work.
//!
//! This module prototypes the future `gtk-lush-settle` shape inside LushText
//! only. It keeps GTK in charge of the main loop while giving debounce, delayed
//! settle, and one-shot auto-dismiss flows one audited generation-token idiom.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;

/// Opaque generation captured by a scheduled timer callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerToken(u32);

impl TimerToken {
    /// Return the numeric generation for workflows that persist or report it.
    pub(crate) fn value(self) -> u32 {
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
    target_weak: &glib::WeakRef<T>,
    gate: &GenerationGate,
    token: TimerToken,
    callback: F,
) where
    T: IsA<glib::Object> + Clone + 'static,
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

/// Debounce helper for trailing main-loop work where only the newest request wins.
#[derive(Clone, Debug, Default)]
pub struct Debounce {
    gate: Rc<GenerationGate>,
}

impl Debounce {
    /// Advance the debounce generation for immediate or async work that should
    /// supersede any pending timer.
    pub(crate) fn advance(&self) -> TimerToken {
        self.gate.advance()
    }

    /// Invalidate pending callbacks without scheduling a replacement.
    pub(crate) fn invalidate(&self) -> TimerToken {
        self.gate.invalidate()
    }

    /// Check whether a previously captured token is still current.
    pub(crate) fn is_current(&self, token: TimerToken) -> bool {
        self.gate.is_current(token)
    }

    /// Schedule a trailing callback on GTK's main loop.
    ///
    /// The callback receives the upgraded target and the captured token. The
    /// helper performs the stale-token check before invoking the callback, so
    /// call sites only keep additional workflow guards that are specific to
    /// their surface.
    pub(crate) fn schedule<T, F>(&self, target: &T, delay: Duration, callback: F) -> TimerToken
    where
        T: IsA<glib::Object> + Clone + 'static,
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

/// Superseding one-shot timer for delayed UI cleanup or reveal decisions.
#[derive(Clone, Debug, Default)]
pub struct SupersedingTimer {
    gate: Rc<GenerationGate>,
}

impl SupersedingTimer {
    /// Arm the timer, replacing any earlier arm-generation.
    pub(crate) fn arm<T, F>(&self, target: &T, delay: Duration, callback: F) -> TimerToken
    where
        T: IsA<glib::Object> + Clone + 'static,
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
    pub(crate) fn invalidate(&self) -> TimerToken {
        self.gate.invalidate()
    }
}

/// Settle-burst helper for layout work that remains pending until repaired.
#[derive(Clone, Debug, Default)]
pub struct SettleBurst {
    gate: Rc<GenerationGate>,
    pending: Rc<Cell<bool>>,
}

impl SettleBurst {
    /// Schedule or extend the burst and run `callback` after the quiet window.
    ///
    /// The returned handle lets multi-stage repairs clear pending state only
    /// after their actual final repair completes.
    pub(crate) fn schedule<T, F>(&self, target: &T, delay: Duration, callback: F) -> SettleHandle
    where
        T: IsA<glib::Object> + Clone + 'static,
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
    fn begin(&self) -> SettleHandle {
        self.pending.set(true);
        let token = self.gate.advance();
        SettleHandle {
            gate: Rc::clone(&self.gate),
            pending: Rc::clone(&self.pending),
            token,
        }
    }

    /// Return whether the burst is currently blocking readiness.
    pub(crate) fn pending(&self) -> bool {
        self.pending.get()
    }

    /// Invalidate callbacks and clear pending state when the workflow disappears.
    pub(crate) fn clear(&self) -> TimerToken {
        let token = self.gate.invalidate();
        self.pending.set(false);
        token
    }
}

/// Handle for the currently scheduled settle-burst generation.
#[derive(Clone, Debug)]
pub struct SettleHandle {
    gate: Rc<GenerationGate>,
    pending: Rc<Cell<bool>>,
    token: TimerToken,
}

impl SettleHandle {
    /// Report whether no newer burst has superseded this handle.
    pub(crate) fn is_current(&self) -> bool {
        self.gate.is_current(self.token)
    }

    /// Clear pending state only if this handle still owns the latest generation.
    pub(crate) fn finish_if_current(&self) {
        if self.is_current() {
            self.pending.set(false);
        }
    }

    /// Schedule follow-up work tied to the same settle generation.
    pub(crate) fn schedule_follow_up<T, F>(&self, target: &T, delay: Duration, callback: F)
    where
        T: IsA<glib::Object> + Clone + 'static,
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
    fn settle_clear_invalidates_pending_work() {
        let burst = SettleBurst::default();
        let handle = burst.begin();

        burst.clear();

        assert!(!burst.pending());
        assert!(!handle.is_current());
    }

    #[test]
    fn current_runner_skips_stale_tokens() {
        let gate = GenerationGate::default();
        let stale = gate.advance();
        let _current = gate.advance();
        let target = glib::Object::new::<glib::Object>();
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
            let target = glib::Object::new::<glib::Object>();
            target.downgrade()
        };
        let ran = Cell::new(false);

        run_if_current(&target_weak, &gate, token, |_, _| {
            ran.set(true);
        });

        assert!(!ran.get());
    }
}
