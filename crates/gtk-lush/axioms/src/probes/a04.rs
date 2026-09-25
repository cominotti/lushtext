// SPDX-License-Identifier: MIT OR Apache-2.0

//! A4: a `GtkListView` applies `scroll_to` and focus scrolling inside its
//! **own** allocation, as a write to its vadjustment: nothing moves when the
//! request is made, and every resulting `value-changed` is emitted while the
//! list is being allocated. This is why `classify_child_scroll` reads the
//! child's value right after `child.allocate`.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::{SCROLL_SETTLE, row_stride, same_value};
use crate::fixtures::{FixedHost, HostedList, LIST_HEIGHT, ROWS};
use crate::observation::Recorder;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// The row the probe scrolls down to.
pub const FAR_ROW: u32 = 200;
/// The row the probe then focuses, back near the top.
pub const FOCUS_ROW: u32 = 20;

/// Counts `value-changed` emissions, and how many came while the host was
/// allocating its child.
#[derive(Clone, Debug)]
pub struct EmissionSites {
    total: Rc<Cell<u32>>,
    inside_allocation: Rc<Cell<u32>>,
}

impl EmissionSites {
    /// Start counting emissions on `adjustment`, attributing each to `host`'s
    /// allocation state at the moment it fires.
    #[must_use]
    pub fn watch(adjustment: &gtk4::Adjustment, host: &FixedHost) -> Self {
        let sites = Self {
            total: Rc::default(),
            inside_allocation: Rc::default(),
        };
        let counted = sites.clone();
        let weak_host = host.downgrade();
        adjustment.connect_value_changed(move |_| {
            counted.total.set(counted.total.get() + 1);
            if weak_host
                .upgrade()
                .is_some_and(|host| host.is_allocating_child())
            {
                counted
                    .inside_allocation
                    .set(counted.inside_allocation.get() + 1);
            }
        });
        sites
    }

    /// Every emission since counting started.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.total.get()
    }

    /// Emissions that fired inside the host's `child.allocate`.
    #[must_use]
    pub fn inside_allocation(&self) -> u32 {
        self.inside_allocation.get()
    }
}

/// Probe A4. See the module documentation.
#[must_use]
pub fn probe_a04() -> Observation {
    Recorder::run(AxiomId::new(4), |recorder| {
        let hosted = HostedList::new();
        let _shown = Presented::checked(
            recorder,
            &hosted.host,
            "control: the fixture window realizes",
        )?;
        let stride = row_stride(&hosted.adjustment, ROWS);
        recorder.measure("row_stride", stride);

        let sites = EmissionSites::watch(&hosted.adjustment, &hosted.host);
        let before = hosted.adjustment.value();
        hosted
            .list
            .scroll_to(FAR_ROW, gtk4::ListScrollFlags::NONE, None);
        recorder.measure("value_when_requested", hosted.adjustment.value());
        let unmoved_at_request = same_value(hosted.adjustment.value(), before);
        settle(SCROLL_SETTLE);
        let reach = f64::from(FAR_ROW + 1) * stride - f64::from(LIST_HEIGHT);
        recorder.measure("reach", reach);
        recorder.measure("scrolled_value", hosted.adjustment.value());
        recorder.measure("scroll_emissions", sites.total());
        recorder.measure(
            "scroll_emissions_inside_allocation",
            sites.inside_allocation(),
        );
        recorder.control(
            hosted.adjustment.value() >= reach - 1.0,
            "control: scroll_to brings the far row into view",
        )?;
        recorder.axiom(
            unmoved_at_request,
            "A4: scroll_to does not write the adjustment when it is called",
        )?;
        recorder.axiom(
            sites.total() > 0 && sites.inside_allocation() == sites.total(),
            "A4: every value-changed from scroll_to fires inside the list's allocation",
        )?;

        let focus_sites = EmissionSites::watch(&hosted.adjustment, &hosted.host);
        hosted
            .list
            .scroll_to(FOCUS_ROW, gtk4::ListScrollFlags::FOCUS, None);
        settle(SCROLL_SETTLE);
        recorder.measure("focused_value", hosted.adjustment.value());
        recorder.measure("focus_emissions", focus_sites.total());
        recorder.measure(
            "focus_emissions_inside_allocation",
            focus_sites.inside_allocation(),
        );
        recorder.control(
            hosted.adjustment.value() <= f64::from(FOCUS_ROW) * stride + 1.0,
            "control: focusing a row above the view scrolls back up to it",
        )?;
        recorder.axiom(
            focus_sites.total() > 0 && focus_sites.inside_allocation() == focus_sites.total(),
            "A4: focus scrolling writes the adjustment inside the list's allocation too",
        )
    })
}
