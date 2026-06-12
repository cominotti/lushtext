// SPDX-License-Identifier: MIT OR Apache-2.0

//! Scroll-adjustment viewport observation helpers for gtk-rs.
//!
//! GTK widgets that install layout managers do not receive a subclass
//! `size_allocate` override. For scrollable content, the public adjustments are
//! a reliable geometry signal instead: their page sizes change as the viewport
//! width or height changes, and their values report user-visible scroll motion.
//!
//! `ViewportObserver` wires those signals with drop-time cleanup. `RestState`
//! stores whether a viewport rested at the lower edge outside caller-declared
//! reflow pauses.
//!
//! # Example
//!
//! ```no_run
//! use gtk4::prelude::*;
//! use gtk_lush_viewport::{ViewportAxis, ViewportObserver};
//!
//! # let view = gtk4::TextView::new();
//! let _observer = ViewportObserver::for_scrollable(
//!     &view,
//!     |change| match change.axis() {
//!         ViewportAxis::Horizontal => println!("width changed"),
//!         ViewportAxis::Vertical => println!("height changed"),
//!     },
//!     |_| {},
//! );
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::cell::Cell;
use std::rc::Rc;

use glib::{Object, SignalHandlerId, WeakRef};
use gtk4::prelude::*;

// GTK adjustment values are logical pixels and can move by tiny fractional
// amounts during allocation. Treat sub-half-pixel differences as the same edge
// so reflow bookkeeping does not bounce on renderer rounding.
const LOWER_EDGE_EPSILON: f64 = 0.5;

/// Scroll axis represented by a viewport adjustment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewportAxis {
    /// Horizontal viewport width and value.
    Horizontal,
    /// Vertical viewport height and value.
    Vertical,
}

/// A filtered adjustment page-size change.
#[derive(Clone, Debug)]
pub struct ViewportBoundsChange {
    axis: ViewportAxis,
    adjustment: gtk4::Adjustment,
    previous_page_size: f64,
    page_size: f64,
}

impl ViewportBoundsChange {
    /// Build a bounds-change event.
    #[must_use]
    pub fn new(
        axis: ViewportAxis,
        adjustment: &gtk4::Adjustment,
        previous_page_size: f64,
        page_size: f64,
    ) -> Self {
        Self {
            axis,
            adjustment: adjustment.clone(),
            previous_page_size,
            page_size,
        }
    }

    /// Return the axis whose page size changed.
    #[must_use]
    pub const fn axis(&self) -> ViewportAxis {
        self.axis
    }

    /// Return the adjustment that emitted the change.
    #[must_use]
    pub const fn adjustment(&self) -> &gtk4::Adjustment {
        &self.adjustment
    }

    /// Return the last page size observed by this observer.
    #[must_use]
    pub const fn previous_page_size(&self) -> f64 {
        self.previous_page_size
    }

    /// Return the newly observed page size.
    #[must_use]
    pub const fn page_size(&self) -> f64 {
        self.page_size
    }
}

/// An adjustment value change.
#[derive(Clone, Debug)]
pub struct ViewportValueChange {
    axis: ViewportAxis,
    adjustment: gtk4::Adjustment,
    value: f64,
    lower: f64,
}

impl ViewportValueChange {
    /// Build a value-change event.
    #[must_use]
    pub fn new(axis: ViewportAxis, adjustment: &gtk4::Adjustment) -> Self {
        Self {
            axis,
            adjustment: adjustment.clone(),
            value: adjustment.value(),
            lower: adjustment.lower(),
        }
    }

    /// Return the axis whose adjustment value changed.
    #[must_use]
    pub const fn axis(&self) -> ViewportAxis {
        self.axis
    }

    /// Return the adjustment that emitted the value change.
    #[must_use]
    pub const fn adjustment(&self) -> &gtk4::Adjustment {
        &self.adjustment
    }

    /// Return the newly observed adjustment value.
    #[must_use]
    pub const fn value(&self) -> f64 {
        self.value
    }

    /// Return the adjustment lower edge observed with this value.
    #[must_use]
    pub const fn lower(&self) -> f64 {
        self.lower
    }

    /// Return whether the value is effectively resting at the lower edge.
    #[must_use]
    pub fn rests_at_lower(&self) -> bool {
        rests_at_lower(self.value, self.lower)
    }
}

/// Drop-owned adjustment signal observer.
///
/// Keep this value alive for as long as callbacks should run. Dropping it
/// disconnects all adjustment handlers.
pub struct ViewportObserver {
    registrations: Vec<SignalRegistration>,
}

impl ViewportObserver {
    /// Observe page-size and value changes on explicit horizontal and vertical adjustments.
    #[must_use]
    pub fn new<B, V>(
        hadjustment: &gtk4::Adjustment,
        vadjustment: &gtk4::Adjustment,
        on_bounds_changed: B,
        on_value_changed: V,
    ) -> Self
    where
        B: Fn(ViewportBoundsChange) + 'static,
        V: Fn(ViewportValueChange) + 'static,
    {
        let on_bounds_changed = Rc::new(on_bounds_changed);
        let on_value_changed = Rc::new(on_value_changed);
        let mut registrations = Vec::with_capacity(4);

        for (adjustment, axis) in [
            (hadjustment.clone(), ViewportAxis::Horizontal),
            (vadjustment.clone(), ViewportAxis::Vertical),
        ] {
            let last_page_size = Rc::new(Cell::new(adjustment.page_size()));
            let callback = Rc::clone(&on_bounds_changed);
            let observed_size = Rc::clone(&last_page_size);
            let handler_id = adjustment.connect_changed(move |adjustment| {
                let page_size = adjustment.page_size();
                let previous_page_size = observed_size.get();
                if (previous_page_size - page_size).abs() <= LOWER_EDGE_EPSILON {
                    return;
                }
                observed_size.set(page_size);
                callback(ViewportBoundsChange::new(
                    axis,
                    adjustment,
                    previous_page_size,
                    page_size,
                ));
            });
            registrations.push(SignalRegistration::new(&adjustment, handler_id));

            let callback = Rc::clone(&on_value_changed);
            let handler_id = adjustment.connect_value_changed(move |adjustment| {
                callback(ViewportValueChange::new(axis, adjustment));
            });
            registrations.push(SignalRegistration::new(&adjustment, handler_id));
        }

        Self { registrations }
    }

    /// Observe the adjustments exposed by a GTK scrollable.
    #[must_use]
    pub fn for_scrollable<S, B, V>(
        scrollable: &S,
        on_bounds_changed: B,
        on_value_changed: V,
    ) -> Option<Self>
    where
        S: IsA<gtk4::Scrollable>,
        B: Fn(ViewportBoundsChange) + 'static,
        V: Fn(ViewportValueChange) + 'static,
    {
        let hadjustment = scrollable.hadjustment()?;
        let vadjustment = scrollable.vadjustment()?;
        Some(Self::new(
            &hadjustment,
            &vadjustment,
            on_bounds_changed,
            on_value_changed,
        ))
    }

    /// Return the number of live signal registrations owned by the observer.
    #[must_use]
    pub fn len(&self) -> usize {
        self.registrations.len()
    }

    /// Return whether this observer owns no signal registrations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty()
    }
}

impl Drop for ViewportObserver {
    fn drop(&mut self) {
        for registration in self.registrations.drain(..) {
            registration.disconnect();
        }
    }
}

struct SignalRegistration {
    source: WeakRef<Object>,
    handler_id: SignalHandlerId,
}

impl SignalRegistration {
    fn new(source: &gtk4::Adjustment, handler_id: SignalHandlerId) -> Self {
        // The observer owns the handler, not the adjustment. Holding a weak
        // source lets drop-time cleanup disconnect when the adjustment is still
        // alive while staying harmless if GTK destroyed it first.
        let source: Object = source.clone().upcast();
        Self {
            source: source.downgrade(),
            handler_id,
        }
    }

    fn disconnect(self) {
        if let Some(source) = self.source.upgrade() {
            source.disconnect(self.handler_id);
        }
    }
}

/// Shared lower-edge rest state for horizontal and vertical viewport axes.
#[derive(Clone, Debug, Default)]
pub struct RestState {
    inner: Rc<RestStateInner>,
}

#[derive(Debug, Default)]
struct RestStateInner {
    horizontal_at_lower: Cell<bool>,
    vertical_at_lower: Cell<bool>,
    pause_depth: Cell<u32>,
}

impl RestState {
    /// Create a rest-state tracker with both axes initialized to `false`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the current value of an adjustment unless the state is paused.
    ///
    /// Returns `true` when the rest state was updated.
    #[must_use]
    pub fn record_adjustment(&self, axis: ViewportAxis, adjustment: &gtk4::Adjustment) -> bool {
        self.record_value(axis, adjustment.value(), adjustment.lower())
    }

    /// Record an adjustment value/lower-edge pair unless the state is paused.
    ///
    /// This pure form is useful for tests and for callers that already unpacked
    /// the adjustment event.
    #[must_use]
    pub fn record_value(&self, axis: ViewportAxis, value: f64, lower: f64) -> bool {
        if self.is_paused() {
            return false;
        }
        self.set_at_lower(axis, rests_at_lower(value, lower));
        true
    }

    /// Set whether an axis currently rests at its lower edge.
    pub fn set_at_lower(&self, axis: ViewportAxis, at_lower: bool) {
        match axis {
            ViewportAxis::Horizontal => self.inner.horizontal_at_lower.set(at_lower),
            ViewportAxis::Vertical => self.inner.vertical_at_lower.set(at_lower),
        }
    }

    /// Return whether an axis currently rests at its lower edge.
    #[must_use]
    pub fn at_lower(&self, axis: ViewportAxis) -> bool {
        match axis {
            ViewportAxis::Horizontal => self.inner.horizontal_at_lower.get(),
            ViewportAxis::Vertical => self.inner.vertical_at_lower.get(),
        }
    }

    /// Begin a reflow pause that excludes transient adjustment values.
    #[must_use]
    pub fn pause(&self) -> RestPause {
        self.inner
            .pause_depth
            .set(self.inner.pause_depth.get().saturating_add(1));
        RestPause {
            inner: Rc::clone(&self.inner),
            active: true,
        }
    }

    /// Return whether transient reflow values are currently excluded.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.inner.pause_depth.get() > 0
    }
}

/// RAII pause handle for `RestState`.
#[derive(Debug)]
pub struct RestPause {
    inner: Rc<RestStateInner>,
    active: bool,
}

impl RestPause {
    /// End the pause immediately instead of waiting for drop.
    pub fn finish(mut self) {
        self.release();
    }

    fn release(&mut self) {
        if !self.active {
            return;
        }
        let depth = self.inner.pause_depth.get();
        self.inner.pause_depth.set(depth.saturating_sub(1));
        self.active = false;
    }
}

impl Drop for RestPause {
    fn drop(&mut self) {
        self.release();
    }
}

/// Return whether an adjustment value is effectively at its lower edge.
#[must_use]
pub fn rests_at_lower(value: f64, lower: f64) -> bool {
    (value - lower).abs() <= LOWER_EDGE_EPSILON
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    fn adjustment(page_size: f64) -> gtk4::Adjustment {
        gtk4::Adjustment::new(0.0, 0.0, 1_000.0, 1.0, 10.0, page_size)
    }

    #[test]
    #[ignore = "requires an initialized GTK display"]
    fn observer_filters_unchanged_page_size() {
        let hadjustment = adjustment(100.0);
        let vadjustment = adjustment(200.0);
        let calls = Rc::new(Cell::new(0));
        let observed_calls = Rc::clone(&calls);
        let _observer = ViewportObserver::new(
            &hadjustment,
            &vadjustment,
            move |_| observed_calls.set(observed_calls.get() + 1),
            |_| {},
        );

        hadjustment.set_page_size(100.25);
        hadjustment.set_page_size(150.0);

        assert_eq!(calls.get(), 1);
    }

    #[test]
    #[ignore = "requires an initialized GTK display"]
    fn observer_accumulates_sub_epsilon_page_size_steps() {
        let hadjustment = adjustment(100.0);
        let vadjustment = adjustment(200.0);
        let calls = Rc::new(Cell::new(0));
        let previous = Rc::new(Cell::new(0.0));
        let observed_calls = Rc::clone(&calls);
        let observed_previous = Rc::clone(&previous);
        let _observer = ViewportObserver::new(
            &hadjustment,
            &vadjustment,
            move |change| {
                observed_calls.set(observed_calls.get() + 1);
                observed_previous.set(change.previous_page_size());
            },
            |_| {},
        );

        hadjustment.set_page_size(100.25);
        hadjustment.set_page_size(100.6);

        assert_eq!(calls.get(), 1);
        assert_eq!(previous.get(), 100.0);
    }

    #[test]
    #[ignore = "requires an initialized GTK display"]
    fn observer_reports_axis_and_previous_size() {
        let hadjustment = adjustment(100.0);
        let vadjustment = adjustment(200.0);
        let axis = Rc::new(Cell::new(None));
        let previous = Rc::new(Cell::new(0.0));
        let observed_axis = Rc::clone(&axis);
        let observed_previous = Rc::clone(&previous);
        let _observer = ViewportObserver::new(
            &hadjustment,
            &vadjustment,
            move |change| {
                observed_axis.set(Some(change.axis()));
                observed_previous.set(change.previous_page_size());
            },
            |_| {},
        );

        vadjustment.set_page_size(225.0);

        assert_eq!(axis.get(), Some(ViewportAxis::Vertical));
        assert_eq!(previous.get(), 200.0);
    }

    #[test]
    #[ignore = "requires an initialized GTK display"]
    fn observer_disconnects_on_drop() {
        let hadjustment = adjustment(100.0);
        let vadjustment = adjustment(200.0);
        let calls = Rc::new(Cell::new(0));
        {
            let observed_calls = Rc::clone(&calls);
            let observer = ViewportObserver::new(
                &hadjustment,
                &vadjustment,
                move |_| observed_calls.set(observed_calls.get() + 1),
                |_| {},
            );
            assert_eq!(observer.len(), 4);
        }

        hadjustment.set_page_size(140.0);

        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn rest_state_records_lower_edge_and_respects_pause() {
        let state = RestState::new();

        assert!(state.record_value(ViewportAxis::Vertical, 0.0, 0.0));
        assert!(state.at_lower(ViewportAxis::Vertical));

        let pause = state.pause();
        assert!(!state.record_value(ViewportAxis::Vertical, 25.0, 0.0));
        assert!(state.at_lower(ViewportAxis::Vertical));

        pause.finish();
        assert!(state.record_value(ViewportAxis::Vertical, 25.0, 0.0));
        assert!(!state.at_lower(ViewportAxis::Vertical));
    }

    #[test]
    #[ignore = "requires an initialized GTK display"]
    fn value_change_reports_rest_state() {
        let hadjustment = adjustment(100.0);
        let vadjustment = adjustment(200.0);
        let rests_at_lower = Rc::new(Cell::new(true));
        let observed_rest = Rc::clone(&rests_at_lower);
        let _observer = ViewportObserver::new(
            &hadjustment,
            &vadjustment,
            |_| {},
            move |change| observed_rest.set(change.rests_at_lower()),
        );

        vadjustment.set_value(50.0);

        assert!(!rests_at_lower.get());
    }

    #[test]
    fn lower_edge_epsilon_matches_adjustment_policy() {
        assert!(super::rests_at_lower(0.49, 0.0));
        assert!(!super::rests_at_lower(0.51, 0.0));
    }
}
