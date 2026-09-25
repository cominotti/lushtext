// SPDX-License-Identifier: MIT OR Apache-2.0

//! A13: moving a scroller's adjustment from inside layout does not reliably
//! schedule a relayout of a widget that re-slices on that adjustment's
//! `value-changed`: its own `queue_allocate` from the emission is lost
//! because it is already being allocated. This is why the slice bin applies a
//! child's request to the outer scroller from an idle rather than inside
//! allocation.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::{SCROLL_SETTLE, same_value};
use crate::fixtures::FixedHost;
use crate::observation::Recorder;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// Height of the tall host inside the scroller.
pub(crate) const TALL_CONTENT: i32 = 4_000;
/// The outer move made from outside layout (the control).
pub const IDLE_MOVE: f64 = 250.0;
/// The outer move made from inside the host's allocation (the axiom).
pub const IN_LAYOUT_MOVE: f64 = 500.0;

/// The A13 fixture: a tall [`FixedHost`] inside a scroller, re-allocated on
/// every outer `value-changed` the way the slice bin re-slices.
#[derive(Clone, Debug)]
pub struct ReslicingHost {
    /// The scroller whose adjustment moves.
    pub scroller: gtk4::ScrolledWindow,
    /// The host that queues a re-allocation on every outer move.
    pub host: FixedHost,
}

impl ReslicingHost {
    /// Build the fixture.
    #[must_use]
    pub fn new() -> Self {
        let label = gtk4::Label::new(Some("tall content"));
        let host = FixedHost::new(&label, TALL_CONTENT);
        host.set_size_request(-1, TALL_CONTENT);
        let scroller = gtk4::ScrolledWindow::builder()
            .vexpand(true)
            .child(&host)
            .build();
        let weak_host = host.downgrade();
        scroller.vadjustment().connect_value_changed(move |_| {
            if let Some(host) = weak_host.upgrade() {
                host.queue_allocate();
            }
        });
        Self { scroller, host }
    }

    /// Move the outer adjustment to `value` once, from inside the host's next
    /// allocation. The returned flag turns true when the move ran.
    #[must_use]
    pub fn move_outer_inside_next_allocation(&self, value: f64) -> Rc<Cell<bool>> {
        let moved = Rc::new(Cell::new(false));
        let move_once = moved.clone();
        let outer = self.scroller.vadjustment();
        self.host.set_during_allocation(move || {
            if !move_once.replace(true) {
                outer.set_value(value);
            }
        });
        self.host.queue_allocate();
        moved
    }
}

impl Default for ReslicingHost {
    fn default() -> Self {
        Self::new()
    }
}

/// Probe A13. See the module documentation.
#[must_use]
pub fn probe_a13() -> Observation {
    Recorder::run(AxiomId::new(13), |recorder| {
        let fixture = ReslicingHost::new();
        let _shown = Presented::checked(
            recorder,
            &fixture.scroller,
            "control: the fixture window realizes",
        )?;
        let outer = fixture.scroller.vadjustment();

        // Control: the same move from outside layout re-allocates the host.
        let before_control = fixture.host.allocations();
        outer.set_value(IDLE_MOVE);
        settle(SCROLL_SETTLE);
        recorder.measure("allocations_before_idle_move", before_control);
        recorder.measure("allocations_after_idle_move", fixture.host.allocations());
        recorder.control(
            fixture.host.allocations() > before_control,
            "control: an outer move outside layout re-allocates the host",
        )?;

        let moved = fixture.move_outer_inside_next_allocation(IN_LAYOUT_MOVE);
        settle(SCROLL_SETTLE);
        recorder.control(moved.get(), "control: the in-layout move ran")?;
        let after_move = fixture.host.allocations();
        settle(SCROLL_SETTLE);
        recorder.measure("outer_value", outer.value());
        recorder.measure("allocations_after_in_layout_move", after_move);
        recorder.measure("allocations_later", fixture.host.allocations());
        recorder.control(
            same_value(outer.value(), IN_LAYOUT_MOVE),
            "control: the in-layout move itself takes effect",
        )?;
        recorder.axiom(
            fixture.host.allocations() == after_move,
            "A13: the host's re-allocation queued from inside its own allocation never arrives",
        )
    })
}
