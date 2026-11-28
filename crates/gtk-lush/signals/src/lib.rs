// SPDX-License-Identifier: MIT OR Apache-2.0

//! RAII lifetime helpers for gtk-rs signal handlers, property bindings, and
//! registration-like cleanups.
//!
//! This crate turns a common gtk-rs cleanup pattern into small value types:
//! record a signal handler or binding when it is created, then let the owner
//! disconnect or unbind everything in `clear()` or `Drop`.
//!
//! # Cleanup Ordering and Disposed Sources
//!
//! - **Cleanup ordering:** [`SignalBag`], [`BindingBag`], and
//!   [`RegistrationBag`] drain their stored entries in FIFO (insertion) order
//!   when [`clear()`](SignalBag::clear) or `Drop` runs, emptying the internal
//!   `RefCell` *before* running each disconnect, unbind, or cleanup callback. A
//!   destructor triggered during cleanup can therefore re-entrantly call
//!   `clear()`, `track()`, or `replace()` on the same owner without a
//!   `RefCell` borrow panic.
//! - **Disposed-but-referenced sources:** [`SignalBag`] and [`SignalSlot`] hold
//!   a `glib::WeakRef<Object>` to the emitting source. When an object is merely
//!   disposed (`ObjectImpl::dispose`) while an external strong reference still
//!   keeps it allocated, `WeakRef::upgrade()` still returns `Some(source)`;
//!   calling `source.disconnect(id)` on a disposed GObject is harmless if the
//!   signal was not already invalidated, and widget owners should still clear
//!   their bags/slots inside `dispose()` rather than waiting for `finalize()`.
//!
//! GTK Lush crates remain independently adoptable leaf crates. They do not own
//! GTK control flow, define a view DSL, add a state/message framework, depend
//! on another GTK Lush crate, or replace Libadwaita adaptive behavior.
//!
//! # Example
//!
//! ```
//! use std::cell::Cell;
//! use std::rc::Rc;
//!
//! use gio::prelude::*;
//! use gtk_lush_signals::SignalBag;
//!
//! let action = gio::SimpleAction::new("count", None);
//! let hits = Rc::new(Cell::new(0));
//! let bag = SignalBag::new();
//!
//! bag.track(&action, action.connect_activate({
//!     let hits = Rc::clone(&hits);
//!     move |_, _| hits.set(hits.get() + 1)
//! }));
//!
//! action.activate(None);
//! assert_eq!(hits.get(), 1);
//!
//! bag.clear();
//! action.activate(None);
//! assert_eq!(hits.get(), 1);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::cell::RefCell;
use std::fmt;

use glib::prelude::*;
use glib::{Binding, Object, SignalHandlerId, WeakRef};

/// Owns signal-handler registrations and disconnects them in FIFO order on
/// clear or drop.
///
/// GTK signal connections return a `SignalHandlerId`, but the source object
/// owns the actual registration. `SignalBag` stores a weak reference to that
/// source plus the handler id, so long-lived sources such as settings objects
/// can be disconnected without keeping widgets alive.
#[derive(Debug, Default)]
pub struct SignalBag {
    registrations: RefCell<Vec<SignalRegistration>>,
}

impl SignalBag {
    /// Create an empty signal bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a handler id returned by a gtk-rs `connect_*` call.
    ///
    /// The source is captured weakly. If the source is finalized before the
    /// bag clears, the dead registration is skipped.
    pub fn track<O>(&self, source: &O, handler_id: SignalHandlerId)
    where
        O: IsA<Object> + 'static,
    {
        self.registrations
            .borrow_mut()
            .push(SignalRegistration::new(source.upcast_ref(), handler_id));
    }

    /// Disconnect all currently recorded live handlers in FIFO order.
    ///
    /// Calling `clear()` more than once is safe; already-cleared handlers are
    /// not disconnected again. The internal list is drained before disconnecting
    /// each entry so destructors can re-entrantly call `clear()` or `track()`.
    pub fn clear(&self) {
        for registration in self.registrations.take() {
            registration.disconnect();
        }
    }

    /// Return the number of registrations still owned by this bag.
    #[must_use]
    pub fn len(&self) -> usize {
        self.registrations.borrow().len()
    }

    /// Return whether the bag currently owns no registrations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.registrations.borrow().is_empty()
    }
}

impl Drop for SignalBag {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Owns at most one weak-source signal-handler registration without heap `Vec`
/// allocation.
///
/// Use `SignalSlot` for single replaceable handlers (one frame-clock paint,
/// one map/unmap wait, or one window-level observer) where replacing or
/// clearing must disconnect the prior handler immediately.
#[derive(Debug, Default)]
pub struct SignalSlot {
    registration: RefCell<Option<SignalRegistration>>,
}

impl SignalSlot {
    /// Create an empty signal slot.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Disconnect any previously stored live handler, then store the new one.
    ///
    /// The previous registration is taken and disconnected before storing the
    /// new registration, so `RefCell` is never borrowed across disconnect. If
    /// disconnecting the prior registration re-entrantly calls `replace`, the
    /// re-entrantly installed registration is also disconnected cleanly before
    /// returning.
    pub fn replace<O>(&self, source: &O, handler_id: SignalHandlerId)
    where
        O: IsA<Object> + 'static,
    {
        let previous = self.registration.borrow_mut().take();
        if let Some(previous) = previous {
            previous.disconnect();
        }
        if let Some(superseded) = self
            .registration
            .borrow_mut()
            .replace(SignalRegistration::new(source.upcast_ref(), handler_id))
        {
            superseded.disconnect();
        }
    }

    /// Disconnect the stored handler if one is held and its source is still alive.
    ///
    /// Safe to call multiple times or after the source has finalized.
    pub fn clear(&self) {
        if let Some(registration) = self.registration.borrow_mut().take() {
            registration.disconnect();
        }
    }

    /// Return whether this slot currently holds a registration on a live source.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.registration
            .borrow()
            .as_ref()
            .is_some_and(SignalRegistration::is_source_alive)
    }
}

impl Drop for SignalSlot {
    fn drop(&mut self) {
        self.clear();
    }
}

/// One signal registration stored as a weak source plus handler id.
#[derive(Debug)]
struct SignalRegistration {
    source: WeakRef<Object>,
    handler_id: SignalHandlerId,
}

impl SignalRegistration {
    /// Capture a weak source so the registration owner never prolongs widget lifetime.
    fn new(source: &Object, handler_id: SignalHandlerId) -> Self {
        Self {
            source: source.downgrade(),
            handler_id,
        }
    }

    fn is_source_alive(&self) -> bool {
        self.source.upgrade().is_some()
    }

    /// Disconnect the handler if the source still exists.
    fn disconnect(self) {
        if let Some(source) = self.source.upgrade() {
            source.disconnect(self.handler_id);
        }
    }
}

/// Owns `glib::Binding` values and unbinds them in FIFO order on clear or drop.
///
/// Bindings are also lifecycle registrations: a recycled row or disposed
/// widget must stop projecting old source properties into its current view.
#[derive(Debug, Default)]
pub struct BindingBag {
    bindings: RefCell<Vec<Binding>>,
}

impl BindingBag {
    /// Create an empty binding bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a binding returned by `bind_property(...).build()`.
    pub fn track(&self, binding: Binding) {
        self.bindings.borrow_mut().push(binding);
    }

    /// Unbind all currently recorded bindings.
    ///
    /// Calling `clear()` more than once is safe; already-cleared bindings are
    /// not unbound again.
    pub fn clear(&self) {
        for binding in self.bindings.take() {
            binding.unbind();
        }
    }

    /// Return the number of bindings still owned by this bag.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.borrow().len()
    }

    /// Return whether the bag currently owns no bindings.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.borrow().is_empty()
    }
}

impl Drop for BindingBag {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Owns arbitrary one-shot cleanup callbacks for registration-like lifetimes.
///
/// This is the escape hatch for GTK registrations that are not represented by
/// `SignalHandlerId` or `glib::Binding`, such as row-local controller cleanup.
/// It keeps ownership explicit without inventing a broader signal DSL.
#[derive(Default)]
pub struct RegistrationBag {
    cleanups: RefCell<Vec<Box<dyn FnOnce() + 'static>>>,
}

impl RegistrationBag {
    /// Create an empty registration bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a cleanup callback that should run on clear or drop.
    pub fn track<F>(&self, cleanup: F)
    where
        F: FnOnce() + 'static,
    {
        self.cleanups.borrow_mut().push(Box::new(cleanup));
    }

    /// Run all currently recorded cleanup callbacks exactly once.
    pub fn clear(&self) {
        for cleanup in self.cleanups.take() {
            cleanup();
        }
    }

    /// Return the number of cleanup callbacks still owned by this bag.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cleanups.borrow().len()
    }

    /// Return whether the bag currently owns no cleanup callbacks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cleanups.borrow().is_empty()
    }
}

impl Drop for RegistrationBag {
    fn drop(&mut self) {
        self.clear();
    }
}

impl fmt::Debug for RegistrationBag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.cleanups.try_borrow().map(|cleanups| cleanups.len());
        let mut debug = f.debug_struct("RegistrationBag");
        match len {
            Ok(len) => debug.field("len", &len),
            Err(_) => debug.field("len", &"<borrowed>"),
        }
        .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gio::prelude::*;

    use super::*;

    #[test]
    fn signal_bag_disconnects_on_clear() {
        let action = gio::SimpleAction::new("test", None);
        let calls = Rc::new(Cell::new(0));
        let bag = SignalBag::new();

        bag.track(
            &action,
            action.connect_activate({
                let calls = Rc::clone(&calls);
                move |_, _| calls.set(calls.get() + 1)
            }),
        );
        action.activate(None);

        bag.clear();
        bag.clear();
        action.activate(None);

        assert_eq!(calls.get(), 1);
        assert!(bag.is_empty());
    }

    #[test]
    fn signal_bag_disconnects_on_drop() {
        let action = gio::SimpleAction::new("test", None);
        let calls = Rc::new(Cell::new(0));

        {
            let bag = SignalBag::new();
            bag.track(
                &action,
                action.connect_activate({
                    let calls = Rc::clone(&calls);
                    move |_, _| calls.set(calls.get() + 1)
                }),
            );
            action.activate(None);
        }
        action.activate(None);

        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn signal_bag_does_not_keep_source_alive() {
        let bag = SignalBag::new();
        let weak = {
            let action = gio::SimpleAction::new("test", None);
            let weak = action.downgrade();
            bag.track(&action, action.connect_activate(|_, _| {}));
            weak
        };

        assert!(weak.upgrade().is_none());
        bag.clear();
    }

    #[test]
    fn disconnecting_long_lived_source_releases_captured_consumer() {
        let action = gio::SimpleAction::new("test", None);
        let consumer_weak = {
            let consumer = Object::new::<Object>();
            let consumer_weak = consumer.downgrade();
            let bag = SignalBag::new();

            bag.track(
                &action,
                action.connect_activate(move |_, _| {
                    let _ = &consumer;
                }),
            );

            consumer_weak
        };

        assert!(consumer_weak.upgrade().is_none());
    }

    #[test]
    fn binding_bag_unbinds_on_clear() {
        let source = gio::SimpleAction::new("source", None);
        let target = gio::SimpleAction::new("target", None);
        let bag = BindingBag::new();

        bag.track(
            source
                .bind_property("enabled", &target, "enabled")
                .sync_create()
                .build(),
        );
        source.set_enabled(false);
        assert!(!target.is_enabled());

        bag.clear();
        source.set_enabled(true);

        assert!(!target.is_enabled());
        assert!(bag.is_empty());
    }

    #[test]
    fn binding_bag_supports_rebinding_after_clear() {
        let first_source = gio::SimpleAction::new("first-source", None);
        let second_source = gio::SimpleAction::new("second-source", None);
        let target = gio::SimpleAction::new("target", None);
        let bag = BindingBag::new();

        bag.track(
            first_source
                .bind_property("enabled", &target, "enabled")
                .sync_create()
                .build(),
        );
        first_source.set_enabled(false);
        assert!(!target.is_enabled());

        bag.clear();
        bag.track(
            second_source
                .bind_property("enabled", &target, "enabled")
                .sync_create()
                .build(),
        );
        second_source.set_enabled(true);
        first_source.set_enabled(false);

        assert!(target.is_enabled());
    }

    #[test]
    fn registration_bag_runs_cleanup_once() {
        let calls = Rc::new(Cell::new(0));
        let bag = RegistrationBag::new();

        bag.track({
            let calls = Rc::clone(&calls);
            move || calls.set(calls.get() + 1)
        });

        bag.clear();
        bag.clear();

        assert_eq!(calls.get(), 1);
        assert!(bag.is_empty());
    }

    #[test]
    fn signal_slot_replaces_clears_drops_and_handles_dead_source() {
        let action = gio::SimpleAction::new("slot", None);
        let hits = Rc::new(Cell::new(0));
        let slot = SignalSlot::new();
        assert!(!slot.is_connected());

        slot.replace(
            &action,
            action.connect_activate({
                let hits = Rc::clone(&hits);
                move |_, _| hits.set(hits.get() + 1)
            }),
        );
        assert!(slot.is_connected());
        action.activate(None);
        assert_eq!(hits.get(), 1);

        // Replacing disconnects the first handler immediately.
        slot.replace(
            &action,
            action.connect_activate({
                let hits = Rc::clone(&hits);
                move |_, _| hits.set(hits.get() + 10)
            }),
        );
        assert!(slot.is_connected());
        action.activate(None);
        assert_eq!(hits.get(), 11);

        // Clearing disconnects and is idempotent.
        slot.clear();
        slot.clear();
        assert!(!slot.is_connected());
        action.activate(None);
        assert_eq!(hits.get(), 11);

        // Drop disconnects live handlers.
        {
            let scoped = SignalSlot::new();
            scoped.replace(
                &action,
                action.connect_activate({
                    let hits = Rc::clone(&hits);
                    move |_, _| hits.set(hits.get() + 100)
                }),
            );
            assert!(scoped.is_connected());
        }
        action.activate(None);
        assert_eq!(hits.get(), 11);

        // Dead source tolerates clear/drop without warning.
        let dead_slot = SignalSlot::new();
        let weak = {
            let ephemeral = gio::SimpleAction::new("ephemeral", None);
            let weak = ephemeral.downgrade();
            dead_slot.replace(&ephemeral, ephemeral.connect_activate(|_, _| {}));
            weak
        };
        assert!(weak.upgrade().is_none());
        assert!(!dead_slot.is_connected());
        dead_slot.clear();
    }

    #[test]
    fn bags_and_slot_implement_debug() {
        let signal_bag = SignalBag::new();
        let signal_slot = SignalSlot::new();
        let binding_bag = BindingBag::new();
        let registration_bag = RegistrationBag::new();

        assert!(format!("{signal_bag:?}").contains("SignalBag"));
        assert!(format!("{signal_slot:?}").contains("SignalSlot"));
        assert!(format!("{binding_bag:?}").contains("BindingBag"));
        assert!(format!("{registration_bag:?}").contains("RegistrationBag"));
    }

    #[test]
    fn signal_slot_allows_reentrant_replace_during_disconnect() {
        let action = gio::SimpleAction::new("slot-reentrant", None);
        let slot = Rc::new(SignalSlot::new());
        let reentrant_hits = Rc::new(Cell::new(0u32));
        let final_hits = Rc::new(Cell::new(0u32));

        struct ReentrantGuard {
            slot: Rc<SignalSlot>,
            action: gio::SimpleAction,
            reentrant_hits: Rc<Cell<u32>>,
        }

        impl Drop for ReentrantGuard {
            fn drop(&mut self) {
                let hits = Rc::clone(&self.reentrant_hits);
                self.slot.replace(
                    &self.action,
                    self.action
                        .connect_activate(move |_, _| hits.set(hits.get() + 1)),
                );
            }
        }

        let guard = ReentrantGuard {
            slot: Rc::clone(&slot),
            action: action.clone(),
            reentrant_hits: Rc::clone(&reentrant_hits),
        };
        slot.replace(
            &action,
            action.connect_activate(move |_, _| {
                let _ = &guard;
            }),
        );

        slot.replace(&action, {
            let hits = Rc::clone(&final_hits);
            action.connect_activate(move |_, _| hits.set(hits.get() + 1))
        });

        assert!(slot.is_connected());
        action.activate(None);
        assert_eq!(reentrant_hits.get(), 0);
        assert_eq!(final_hits.get(), 1);
    }

    #[test]
    fn signal_bag_allows_reentrant_clear_and_track_during_disconnect() {
        let action = gio::SimpleAction::new("reentrant", None);
        let bag = Rc::new(SignalBag::new());
        let follow_up_hits = Rc::new(Cell::new(0));

        struct ReentrantCleanup {
            bag: Rc<SignalBag>,
            action: gio::SimpleAction,
            follow_up_hits: Rc<Cell<u32>>,
        }

        impl Drop for ReentrantCleanup {
            fn drop(&mut self) {
                self.bag.clear();
                let hits = Rc::clone(&self.follow_up_hits);
                self.bag.track(
                    &self.action,
                    self.action
                        .connect_activate(move |_, _| hits.set(hits.get() + 1)),
                );
            }
        }

        let probe = ReentrantCleanup {
            bag: Rc::clone(&bag),
            action: action.clone(),
            follow_up_hits: Rc::clone(&follow_up_hits),
        };
        bag.track(
            &action,
            action.connect_activate(move |_, _| {
                let _ = &probe;
            }),
        );

        bag.clear();
        assert_eq!(bag.len(), 1);
        action.activate(None);
        assert_eq!(follow_up_hits.get(), 1);
    }

    #[test]
    fn binding_bag_unbinds_on_drop() {
        let source = gio::SimpleAction::new("source", None);
        let target = gio::SimpleAction::new("target", None);
        {
            let bag = BindingBag::new();
            bag.track(
                source
                    .bind_property("enabled", &target, "enabled")
                    .sync_create()
                    .build(),
            );
            source.set_enabled(false);
            assert!(!target.is_enabled());
        }
        source.set_enabled(true);
        assert!(!target.is_enabled());
    }

    #[test]
    fn registration_bag_runs_cleanup_on_drop() {
        let calls = Rc::new(Cell::new(0));
        {
            let bag = RegistrationBag::new();
            bag.track({
                let calls = Rc::clone(&calls);
                move || calls.set(calls.get() + 1)
            });
        }
        assert_eq!(calls.get(), 1);
    }
}
