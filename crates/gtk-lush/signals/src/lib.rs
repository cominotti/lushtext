// SPDX-License-Identifier: MIT OR Apache-2.0

//! RAII lifetime helpers for gtk-rs signal handlers and property bindings.
//!
//! This crate turns a common gtk-rs cleanup pattern into small value types:
//! record a signal handler or binding when it is created, then let the owner
//! disconnect or unbind everything in `clear()` or `Drop`.
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

use glib::prelude::*;
use glib::{Binding, Object, SignalHandlerId, WeakRef};

/// Owns signal-handler registrations and disconnects them on clear or drop.
///
/// GTK signal connections return a `SignalHandlerId`, but the source object
/// owns the actual registration. `SignalBag` stores a weak reference to that
/// source plus the handler id, so long-lived sources such as settings objects
/// can be disconnected without keeping widgets alive.
#[derive(Default)]
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
        O: IsA<Object> + Clone + 'static,
    {
        let source: Object = source.clone().upcast();
        self.registrations
            .borrow_mut()
            .push(SignalRegistration::new(&source, handler_id));
    }

    /// Disconnect all currently recorded live handlers.
    ///
    /// Calling `clear()` more than once is safe; already-cleared handlers are
    /// not disconnected again.
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

/// One signal registration stored as a weak source plus handler id.
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

    /// Disconnect the handler if the source still exists.
    fn disconnect(self) {
        if let Some(source) = self.source.upgrade() {
            source.disconnect(self.handler_id);
        }
    }
}

/// Owns `glib::Binding` values and unbinds them on clear or drop.
///
/// Bindings are also lifecycle registrations: a recycled row or disposed
/// widget must stop projecting old source properties into its current view.
#[derive(Default)]
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
}
