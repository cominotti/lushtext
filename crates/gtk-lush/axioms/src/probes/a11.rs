// SPDX-License-Identifier: MIT OR Apache-2.0

//! A11: `configure` and `set_value` clamp the value into
//! `[lower, upper - page_size]`, and a `set_value` that does not change the
//! value emits nothing. The slice bin reads its published offset back after
//! `configure` because of the first half, and an unhonourable outer request
//! ends quietly because of the second.

use gtk4::prelude::*;

use super::same_value;
use crate::fixtures::count_value_changes;
use crate::observation::Recorder;
use crate::{AxiomId, Observation};

/// Probe A11. See the module documentation. It needs GTK initialized but no
/// window: an adjustment is not a widget.
#[must_use]
pub fn probe_a11() -> Observation {
    Recorder::run(AxiomId::new(11), |recorder| {
        let adjustment = gtk4::Adjustment::new(0.0, 0.0, 0.0, 1.0, 1.0, 0.0);
        let emissions = count_value_changes(&adjustment);

        adjustment.configure(500.0, 0.0, 300.0, 1.0, 100.0, 100.0);
        recorder.measure("configure_500_gives", adjustment.value());
        recorder.axiom(
            same_value(adjustment.value(), 200.0),
            "A11: configure clamps 500 to upper - page = 200",
        )?;
        adjustment.set_value(-5.0);
        recorder.measure("set_value_minus_5_gives", adjustment.value());
        recorder.axiom(
            same_value(adjustment.value(), 0.0),
            "A11: set_value clamps -5 to lower = 0",
        )?;
        adjustment.set_value(1_000.0);
        recorder.measure("set_value_1000_gives", adjustment.value());
        recorder.axiom(
            same_value(adjustment.value(), 200.0),
            "A11: set_value clamps 1000 to upper - page = 200",
        )?;
        let before = emissions.get();
        adjustment.set_value(200.0);
        adjustment.set_value(5_000.0);
        recorder.measure("emissions_for_unchanged_values", emissions.get() - before);
        recorder.axiom(
            emissions.get() == before,
            "A11: a set_value that leaves the clamped value unchanged emits nothing",
        )?;
        adjustment.set_value(150.0);
        recorder.measure("emissions_for_a_change", emissions.get() - before);
        recorder.control(
            emissions.get() == before + 1,
            "control: a set_value that changes the value emits exactly once",
        )
    })
}
