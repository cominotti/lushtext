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

use std::cell::{Cell, RefCell};
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
/// GLib's main loop for scheduling and a generation token for cancellation:
/// a stale callback no-ops. A new `schedule` (and `invalidate`) also removes
/// the superseded GLib source at once, so at most one source, and one weak
/// reference to the target, is pending however many times it is scheduled in
/// one dispatch. Keeping every superseded source until its deadline made a
/// burst of 131,072 edits in one dispatch register 131,072 weak references on
/// one object, past GLib's per-object limit (`Too many GWeakRef registered`).
#[derive(Clone, Debug, Default)]
pub struct Debounce {
    state: Rc<SupersedingTimerState>,
}

impl Debounce {
    /// Create a debounce with no pending generation.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance the debounce generation for immediate or async work that should
    /// supersede any pending timer, removing any pending GLib source.
    #[must_use]
    pub fn advance(&self) -> TimerToken {
        self.invalidate()
    }

    /// Invalidate pending callbacks without scheduling a replacement, and
    /// remove the pending source.
    #[must_use]
    pub fn invalidate(&self) -> TimerToken {
        self.state.invalidate()
    }

    /// Report whether a scheduled callback is currently armed.
    #[must_use]
    pub fn is_armed(&self) -> bool {
        self.state.is_armed()
    }

    /// Check whether a previously captured token is still current.
    #[must_use]
    pub fn is_current(&self, token: TimerToken) -> bool {
        self.state.gate.is_current(token)
    }

    /// Schedule a trailing callback on GTK's main loop.
    ///
    /// The target is captured weakly. If the target is destroyed or a newer
    /// generation is scheduled before the timer fires, the callback does not
    /// run.
    ///
    /// # Example
    ///
    /// ```
    /// use std::cell::Cell;
    /// use std::rc::Rc;
    /// use std::time::{Duration, Instant};
    ///
    /// use glib::Object;
    /// use gtk_lush_settle::Debounce;
    ///
    /// let debounce = Debounce::new();
    /// let ran = Rc::new(Cell::new(false));
    /// let ran_flag = Rc::clone(&ran);
    /// {
    ///     let target = Object::new::<Object>();
    ///     let _ = debounce.schedule(&target, Duration::from_millis(5), move |_, _| {
    ///         ran_flag.set(true);
    ///     });
    ///     assert!(debounce.is_armed());
    /// }
    /// let deadline = Instant::now() + Duration::from_millis(100);
    /// while debounce.is_armed() && Instant::now() < deadline {
    ///     while glib::MainContext::default().iteration(false) {}
    ///     std::thread::sleep(Duration::from_millis(1));
    /// }
    /// assert!(!ran.get());
    /// assert!(!debounce.is_armed());
    /// ```
    pub fn schedule<T, F>(&self, target: &T, delay: Duration, callback: F) -> TimerToken
    where
        T: IsA<Object> + Clone + 'static,
        F: FnOnce(T, TimerToken) + 'static,
    {
        SupersedingTimerState::schedule(&self.state, target, delay, callback)
    }
}

/// Shared state for a delayed one-shot whose obsolete source is removed eagerly.
#[derive(Debug, Default)]
struct SupersedingTimerState {
    gate: GenerationGate,
    scheduled: RefCell<Option<(TimerToken, glib::SourceId)>>,
}

impl SupersedingTimerState {
    fn take_scheduled(&self) -> Option<(TimerToken, glib::SourceId)> {
        self.scheduled.borrow_mut().take()
    }

    fn remove_source(scheduled: Option<(TimerToken, glib::SourceId)>) {
        if let Some((_token, source_id)) = scheduled {
            source_id.remove();
        }
    }

    fn is_armed(&self) -> bool {
        self.scheduled.borrow().is_some()
    }

    fn invalidate(&self) -> TimerToken {
        let scheduled = self.take_scheduled();
        let token = self.gate.invalidate();
        Self::remove_source(scheduled);
        token
    }

    fn schedule<T, F>(state: &Rc<Self>, target: &T, delay: Duration, callback: F) -> TimerToken
    where
        T: IsA<Object> + Clone + 'static,
        F: FnOnce(T, TimerToken) + 'static,
    {
        let scheduled = state.take_scheduled();
        let token = state.gate.advance();
        // Removing a source synchronously drops its callback captures. Advance
        // first so a timer operation re-entered from a destructor stays the newest.
        Self::remove_source(scheduled);
        if !state.gate.is_current(token) {
            return token;
        }
        let weak_state = Rc::clone(state);
        let target_weak = target.downgrade();
        let source_id = glib::timeout_add_local_once(delay, move || {
            weak_state.forget_if_current(token);
            run_if_current(&target_weak, &weak_state.gate, token, callback);
        });
        state.scheduled.replace(Some((token, source_id)));
        token
    }

    fn forget_if_current(&self, token: TimerToken) {
        if self
            .scheduled
            .borrow()
            .as_ref()
            .is_some_and(|(scheduled_token, _)| *scheduled_token == token)
        {
            self.scheduled.borrow_mut().take();
        }
    }
}

/// Delayed one-shot timer where each arm supersedes the previous arm.
///
/// This is useful for UI cleanup such as pulse removal or delayed reveal/hide
/// decisions. Re-arming or invalidating removes the obsolete GLib source
/// immediately instead of retaining it until its original deadline. It does
/// not own a custom runtime; callbacks run on GLib's main loop when current.
#[derive(Clone, Debug, Default)]
pub struct SupersedingTimer {
    state: Rc<SupersedingTimerState>,
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
        SupersedingTimerState::schedule(&self.state, target, delay, callback)
    }

    /// Invalidate pending cleanup work without scheduling a replacement.
    #[must_use]
    pub fn invalidate(&self) -> TimerToken {
        self.state.invalidate()
    }

    /// Report whether an armed callback is currently pending.
    #[must_use]
    pub fn is_armed(&self) -> bool {
        self.state.is_armed()
    }

    /// Check whether a previously captured arm token is still current.
    #[must_use]
    pub fn is_current(&self, token: TimerToken) -> bool {
        self.state.gate.is_current(token)
    }
}

/// Shared state for a quiet-window repair burst and its armed GLib sources.
#[derive(Debug, Default)]
struct SettleBurstState {
    gate: GenerationGate,
    pending: Cell<bool>,
    next_source_id: Cell<u64>,
    sources: RefCell<Vec<(u64, glib::SourceId)>>,
}

impl SettleBurstState {
    fn next_source_token(&self) -> u64 {
        let id = self.next_source_id.get().wrapping_add(1);
        self.next_source_id.set(id);
        id
    }

    fn push_source(&self, id: u64, source_id: glib::SourceId) {
        self.sources.borrow_mut().push((id, source_id));
    }

    fn forget_source(&self, id: u64) {
        let mut sources = self.sources.borrow_mut();
        if let Some(pos) = sources.iter().position(|(entry_id, _)| *entry_id == id) {
            sources.swap_remove(pos);
        }
    }

    fn take_sources(&self) -> Vec<(u64, glib::SourceId)> {
        std::mem::take(&mut *self.sources.borrow_mut())
    }

    fn remove_sources(sources: Vec<(u64, glib::SourceId)>) {
        for (_, source_id) in sources {
            source_id.remove();
        }
    }
}

/// Tracks a quiet-window repair burst and exposes whether repair is pending.
///
/// `SettleBurst` is for layout or rendering storms where observers must wait
/// until the final repair completes. Pending state remains true until the
/// current `SettleHandle` finishes, and opening, extending, or clearing a burst
/// eagerly removes all armed primary and follow-up GLib sources.
#[derive(Clone, Debug, Default)]
pub struct SettleBurst {
    state: Rc<SettleBurstState>,
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
    /// after their actual final repair completes. If the weak target drops
    /// before the callback fires, `pending()` clears if the handle is still
    /// current.
    pub fn schedule<T, F>(&self, target: &T, delay: Duration, callback: F) -> SettleHandle
    where
        T: IsA<Object> + Clone + 'static,
        F: FnOnce(T, SettleHandle) + 'static,
    {
        let handle = self.begin();
        if !handle.is_current() {
            return handle;
        }
        let callback_handle = handle.clone();
        let state = Rc::clone(&self.state);
        let source_token = self.state.next_source_token();
        let target_weak = target.downgrade();
        let source_id = glib::timeout_add_local_once(delay, move || {
            state.forget_source(source_token);
            let Some(target) = target_weak.upgrade() else {
                callback_handle.finish_if_current();
                return;
            };
            if !callback_handle.is_current() {
                return;
            }
            callback(target, callback_handle);
        });
        self.state.push_source(source_token, source_id);
        handle
    }

    /// Begin or supersede a settle burst without installing a GLib source,
    /// removing any earlier armed primary and follow-up sources.
    #[must_use]
    pub fn begin(&self) -> SettleHandle {
        let sources = self.state.take_sources();
        self.state.pending.set(true);
        let token = self.state.gate.advance();
        // Advance and drain before removing sources so re-entrant callbacks
        // from destructors keep their newer state intact.
        SettleBurstState::remove_sources(sources);
        SettleHandle {
            state: Rc::clone(&self.state),
            token,
        }
    }

    /// Return whether the burst is currently blocking readiness.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.state.pending.get()
    }

    /// Invalidate callbacks, remove all armed primary and follow-up GLib
    /// sources, and clear pending state when the workflow disappears.
    #[must_use]
    pub fn clear(&self) -> TimerToken {
        let sources = self.state.take_sources();
        let token = self.state.gate.invalidate();
        self.state.pending.set(false);
        SettleBurstState::remove_sources(sources);
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
    state: Rc<SettleBurstState>,
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
        self.state.gate.is_current(self.token)
    }

    /// Clear pending state only if this handle still owns the latest generation.
    pub fn finish_if_current(&self) {
        if self.is_current() {
            self.state.pending.set(false);
        }
    }

    /// Schedule follow-up work tied to the same settle generation.
    ///
    /// This is for multi-step repairs inside one settle burst, not a general
    /// task scheduler. If a newer burst starts first, the follow-up no-ops.
    /// If the weak target drops before the follow-up fires, `pending()` clears
    /// when this handle is still current.
    pub fn schedule_follow_up<T, F>(&self, target: &T, delay: Duration, callback: F)
    where
        T: IsA<Object> + Clone + 'static,
        F: FnOnce(T) + 'static,
    {
        if !self.is_current() {
            return;
        }
        let handle = self.clone();
        let state = Rc::clone(&self.state);
        let source_token = self.state.next_source_token();
        let target_weak = target.downgrade();
        let source_id = glib::timeout_add_local_once(delay, move || {
            state.forget_source(source_token);
            let Some(target) = target_weak.upgrade() else {
                handle.finish_if_current();
                return;
            };
            if handle.is_current() {
                callback(target);
            }
        });
        self.state.push_source(source_token, source_id);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::{Mutex, MutexGuard};

    #[cfg(feature = "property-tests")]
    use proptest::prelude::*;

    use super::*;

    static DEFAULT_MAIN_CONTEXT_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_default_main_context_for_timer_test() -> MutexGuard<'static, ()> {
        // Local timeout sources transiently acquire GLib's process-global default
        // context, so parallel test-harness threads must not install them together.
        DEFAULT_MAIN_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    struct DropProbe(Rc<Cell<u32>>);

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    struct ReentrantInvalidationProbe {
        timer: SupersedingTimer,
        reentered: Rc<Cell<bool>>,
    }

    impl Drop for ReentrantInvalidationProbe {
        fn drop(&mut self) {
            let _ = self.timer.invalidate();
            self.reentered.set(true);
        }
    }

    struct ReentrantArmProbe {
        timer: SupersedingTimer,
        target: Object,
        nested_drops: Rc<Cell<u32>>,
        reentered: Rc<Cell<bool>>,
    }

    impl Drop for ReentrantArmProbe {
        fn drop(&mut self) {
            let nested_probe = DropProbe(Rc::clone(&self.nested_drops));
            let _ = self
                .timer
                .arm(&self.target, Duration::from_secs(60), move |_, _| {
                    let _nested_probe = nested_probe;
                });
            self.reentered.set(true);
        }
    }

    #[test]
    fn debounce_tokens_advance_and_reject_stale_tokens() {
        let debounce = Debounce::default();
        let first = debounce.advance();
        let second = debounce.advance();

        assert!(!debounce.is_current(first));
        assert!(debounce.is_current(second));
    }

    #[test]
    fn debounce_reschedule_removes_the_superseded_source_immediately() {
        let _main_context_guard = lock_default_main_context_for_timer_test();
        let debounce = Debounce::default();
        let target = Object::new::<Object>();
        let drops = Rc::new(Cell::new(0));
        let probe = DropProbe(Rc::clone(&drops));
        let _ = debounce.schedule(&target, Duration::from_secs(60), move |_, _| {
            let _probe = probe;
        });

        let _ = debounce.schedule(&target, Duration::from_secs(60), |_, _| {});

        assert_eq!(drops.get(), 1);

        let _ = debounce.invalidate();
    }

    #[test]
    fn debounce_keeps_one_pending_source_across_a_burst() {
        let _main_context_guard = lock_default_main_context_for_timer_test();
        let debounce = Debounce::default();
        let target = Object::new::<Object>();
        let drops = Rc::new(Cell::new(0));
        // More schedules than GLib allows weak references on one object.
        for _ in 0..70_000 {
            let probe = DropProbe(Rc::clone(&drops));
            let _ = debounce.schedule(&target, Duration::from_secs(60), move |_, _| {
                let _probe = probe;
            });
        }

        assert_eq!(drops.get(), 69_999);

        let _ = debounce.invalidate();
        assert_eq!(drops.get(), 70_000);
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
        let _main_context_guard = lock_default_main_context_for_timer_test();
        let timer = SupersedingTimer::default();
        let target = Object::new::<Object>();
        let first = timer.arm(&target, Duration::from_millis(1), |_, _| {});
        let second = timer.arm(&target, Duration::from_millis(1), |_, _| {});

        assert!(!timer.is_current(first));
        assert!(timer.is_current(second));

        let _ = timer.invalidate();
    }

    #[test]
    fn superseding_timer_rearm_removes_the_obsolete_source_immediately() {
        let _main_context_guard = lock_default_main_context_for_timer_test();
        let timer = SupersedingTimer::default();
        let target = Object::new::<Object>();
        let drops = Rc::new(Cell::new(0));
        let probe = DropProbe(Rc::clone(&drops));
        let _ = timer.arm(&target, Duration::from_secs(60), move |_, _| {
            let _probe = probe;
        });

        let _ = timer.arm(&target, Duration::from_secs(60), |_, _| {});

        assert_eq!(drops.get(), 1);

        let _ = timer.invalidate();
    }

    #[test]
    fn superseding_timer_invalidate_removes_the_current_source_immediately() {
        let _main_context_guard = lock_default_main_context_for_timer_test();
        let timer = SupersedingTimer::default();
        let target = Object::new::<Object>();
        let drops = Rc::new(Cell::new(0));
        let probe = DropProbe(Rc::clone(&drops));
        let _ = timer.arm(&target, Duration::from_secs(60), move |_, _| {
            let _probe = probe;
        });

        let _ = timer.invalidate();

        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn superseding_timer_source_cleanup_allows_reentrant_invalidation() {
        let _main_context_guard = lock_default_main_context_for_timer_test();
        let timer = SupersedingTimer::default();
        let target = Object::new::<Object>();
        let reentered = Rc::new(Cell::new(false));
        let probe = ReentrantInvalidationProbe {
            timer: timer.clone(),
            reentered: Rc::clone(&reentered),
        };
        let _ = timer.arm(&target, Duration::from_secs(60), move |_, _| {
            let _probe = probe;
        });

        let _ = timer.invalidate();

        assert!(reentered.get());
    }

    #[test]
    fn superseding_timer_source_cleanup_preserves_reentrant_arm() {
        let _main_context_guard = lock_default_main_context_for_timer_test();
        let timer = SupersedingTimer::default();
        let target = Object::new::<Object>();
        let nested_drops = Rc::new(Cell::new(0));
        let reentered = Rc::new(Cell::new(false));
        let probe = ReentrantArmProbe {
            timer: timer.clone(),
            target: target.clone(),
            nested_drops: Rc::clone(&nested_drops),
            reentered: Rc::clone(&reentered),
        };
        let _ = timer.arm(&target, Duration::from_secs(60), move |_, _| {
            let _probe = probe;
        });

        let superseded = timer.arm(&target, Duration::from_secs(60), |_, _| {});

        assert!(reentered.get());
        assert!(!timer.is_current(superseded));

        let _ = timer.invalidate();

        assert_eq!(nested_drops.get(), 1);
    }

    fn pump_until_flag(flag: &Cell<bool>, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while !flag.get() && std::time::Instant::now() < deadline {
            while glib::MainContext::default().iteration(false) {}
            if flag.get() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        flag.get()
    }

    #[test]
    fn debounce_schedule_fires_on_main_loop_and_replaces_eagerly() {
        let _main_context_guard = lock_default_main_context_for_timer_test();
        let debounce = Debounce::new();
        let target = Object::new::<Object>();
        assert!(!debounce.is_armed());

        let first_ran = Rc::new(Cell::new(false));
        let second_ran = Rc::new(Cell::new(false));
        let first_flag = Rc::clone(&first_ran);
        let second_flag = Rc::clone(&second_ran);
        let drops = Rc::new(Cell::new(0));
        let probe = DropProbe(Rc::clone(&drops));

        let _ = debounce.schedule(&target, Duration::from_millis(5), move |_, _| {
            let _probe = probe;
            first_flag.set(true);
        });
        assert!(debounce.is_armed());

        let _ = debounce.schedule(&target, Duration::from_millis(5), move |_, _| {
            second_flag.set(true);
        });
        assert_eq!(drops.get(), 1);
        assert!(debounce.is_armed());

        assert!(pump_until_flag(&second_ran, Duration::from_secs(1)));
        assert!(!first_ran.get());
        assert!(!debounce.is_armed());
    }

    #[test]
    fn debounce_advance_removes_pending_source_and_disarms() {
        let _main_context_guard = lock_default_main_context_for_timer_test();
        let debounce = Debounce::new();
        let target = Object::new::<Object>();
        let drops = Rc::new(Cell::new(0));
        let probe = DropProbe(Rc::clone(&drops));

        let _ = debounce.schedule(&target, Duration::from_secs(60), move |_, _| {
            let _probe = probe;
        });
        assert!(debounce.is_armed());

        let _ = debounce.advance();
        assert_eq!(
            drops.get(),
            1,
            "Debounce::advance() must remove the superseded GLib source"
        );
        assert!(!debounce.is_armed());
    }

    #[test]
    fn superseding_timer_arm_fires_on_main_loop_and_tracks_is_armed() {
        let _main_context_guard = lock_default_main_context_for_timer_test();
        let timer = SupersedingTimer::new();
        let target = Object::new::<Object>();
        assert!(!timer.is_armed());

        let ran = Rc::new(Cell::new(false));
        let ran_flag = Rc::clone(&ran);
        let _ = timer.arm(&target, Duration::from_millis(5), move |_, _| {
            ran_flag.set(true);
        });
        assert!(timer.is_armed());

        assert!(pump_until_flag(&ran, Duration::from_secs(1)));
        assert!(!timer.is_armed());
    }

    #[test]
    fn settle_burst_schedule_and_follow_up_fire_and_remove_sources_on_begin_and_clear() {
        let _main_context_guard = lock_default_main_context_for_timer_test();
        let burst = SettleBurst::new();
        let target = Object::new::<Object>();

        // (1) begin() / schedule() removes superseded primary source eagerly.
        let drops = Rc::new(Cell::new(0));
        let probe = DropProbe(Rc::clone(&drops));
        let _ = burst.schedule(&target, Duration::from_secs(60), move |_, _| {
            let _probe = probe;
        });
        let _ = burst.begin();
        assert_eq!(
            drops.get(),
            1,
            "SettleBurst::begin() must remove earlier scheduled primary sources"
        );
        let _ = burst.clear();

        // (2) schedule() + schedule_follow_up() fire on MainContext and clear removes follow-up sources.
        let follow_up_ran = Rc::new(Cell::new(false));
        let follow_up_flag = Rc::clone(&follow_up_ran);
        let _ = burst.schedule(&target, Duration::from_millis(5), move |target, handle| {
            assert!(handle.is_current());
            let finish_handle = handle.clone();
            handle.schedule_follow_up(&target, Duration::from_millis(5), move |_| {
                follow_up_flag.set(true);
                finish_handle.finish_if_current();
            });
        });
        assert!(burst.pending());
        assert!(pump_until_flag(&follow_up_ran, Duration::from_secs(1)));
        assert!(!burst.pending());

        // (3) clear() removes armed follow-up sources immediately.
        let follow_drops = Rc::new(Cell::new(0));
        let follow_probe = DropProbe(Rc::clone(&follow_drops));
        let handle = burst.begin();
        handle.schedule_follow_up(&target, Duration::from_secs(60), move |_| {
            let _probe = follow_probe;
        });
        let _ = burst.clear();
        assert_eq!(
            follow_drops.get(),
            1,
            "SettleBurst::clear() must remove armed follow-up sources"
        );
        assert!(!burst.pending());
    }

    #[test]
    fn settle_burst_clears_pending_when_target_drops_before_dispatch() {
        let _main_context_guard = lock_default_main_context_for_timer_test();
        let burst = SettleBurst::new();
        let ran = Rc::new(Cell::new(false));
        let ran_flag = Rc::clone(&ran);
        {
            let target = Object::new::<Object>();
            let _ = burst.schedule(&target, Duration::from_millis(5), move |_, handle| {
                ran_flag.set(true);
                handle.finish_if_current();
            });
        }
        assert!(burst.pending());

        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while burst.pending() && std::time::Instant::now() < deadline {
            while glib::MainContext::default().iteration(false) {}
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            !burst.pending(),
            "dropping the weak target before dispatch must clear pending()"
        );
        assert!(!ran.get());
    }

    #[cfg(feature = "property-tests")]
    const DEFAULT_PROPERTY_CASES: u32 = 64;
    #[cfg(feature = "property-tests")]
    const MAX_PROPERTY_CASES: u32 = 4096;

    #[cfg(feature = "property-tests")]
    fn configured_case_count() -> u32 {
        std::env::var("LUSHTEXT_PROPTEST_CASES")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|cases| *cases > 0)
            .map_or(DEFAULT_PROPERTY_CASES, |cases| {
                cases.min(MAX_PROPERTY_CASES)
            })
    }

    #[cfg(feature = "property-tests")]
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(configured_case_count()))]

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
