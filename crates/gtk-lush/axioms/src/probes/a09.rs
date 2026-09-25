// SPDX-License-Identifier: MIT OR Apache-2.0

//! A9: a `GtkListBase` drops a pending `scroll_to` on any `value-changed` of
//! its adjustment. This is why a honoured slice-bin request must land where
//! the re-slice republishes exactly the child's value: any emission in
//! between erases requests the child made meanwhile.

use gtk4::prelude::*;

use super::SCROLL_SETTLE;
use crate::fixtures::{HostedList, LIST_HEIGHT, ROW_HEIGHT};
use crate::observation::Recorder;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// The row the probe asks the list to scroll to.
pub const TARGET_ROW: u32 = 200;
/// How far the probe nudges the adjustment before the next allocation.
pub const NUDGE: f64 = 10.0;

/// The value at which `TARGET_ROW` is at least in view, computed from the
/// requested row height as the original probe did. Rows draw taller than
/// they request, so this is a lower bound.
pub(crate) fn target_reach() -> f64 {
    let target_top = f64::from(TARGET_ROW) * f64::from(ROW_HEIGHT);
    target_top + f64::from(ROW_HEIGHT) - f64::from(LIST_HEIGHT)
}

/// Probe A9. See the module documentation.
#[must_use]
pub fn probe_a09() -> Observation {
    Recorder::run(AxiomId::new(9), |recorder| {
        let reach = target_reach();
        recorder.measure("reach", reach);

        // Control: with no intervening emission the request is applied.
        let honoured = HostedList::new();
        let shown = Presented::checked(
            recorder,
            &honoured.host,
            "control: the fixture window realizes",
        )?;
        honoured
            .list
            .scroll_to(TARGET_ROW, gtk4::ListScrollFlags::NONE, None);
        settle(SCROLL_SETTLE);
        recorder.measure("honoured_value", honoured.adjustment.value());
        recorder.control(
            honoured.adjustment.value() >= reach - 1.0,
            "control: scroll_to brings the target row into view",
        )?;
        drop(shown);

        let dropped = HostedList::new();
        let _shown = Presented::checked(
            recorder,
            &dropped.host,
            "control: the second fixture window realizes",
        )?;
        dropped
            .list
            .scroll_to(TARGET_ROW, gtk4::ListScrollFlags::NONE, None);
        let nudged = dropped.adjustment.value() + NUDGE;
        dropped.adjustment.set_value(nudged);
        settle(SCROLL_SETTLE);
        recorder.measure("nudged_value", nudged);
        recorder.measure("dropped_value", dropped.adjustment.value());
        recorder.axiom(
            (dropped.adjustment.value() - nudged).abs() < 1.0,
            "A9: a value-changed before the next allocation drops the pending scroll_to",
        )
    })
}
