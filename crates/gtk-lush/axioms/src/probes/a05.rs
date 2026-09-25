// SPDX-License-Identifier: MIT OR Apache-2.0

//! A5: a zero-height allocation makes a `GtkListView` rewrite the host's
//! adjustment — its value as well as its page — although nothing asked it to
//! scroll. This is why `viewport_slice` never shrinks the band at the bottom
//! edge: a host that read the rewrite as a request would scroll on its own.

use gtk4::prelude::*;

use super::{LAYOUT_SETTLE, same_value};
use crate::fixtures::{HostedList, LIST_HEIGHT};
use crate::observation::Recorder;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// The value the host publishes before the zero-height allocation.
pub(crate) const PUBLISHED_VALUE: f64 = 1_200.0;

/// Probe A5. See the module documentation.
#[must_use]
pub fn probe_a05() -> Observation {
    Recorder::run(AxiomId::new(5), |recorder| {
        let hosted = HostedList::new();
        let shown = Presented::new(&hosted.host);
        recorder.control(shown.realized(), "control: the fixture window realizes")?;
        recorder.measure("upper", hosted.adjustment.upper());
        recorder.control(
            hosted.adjustment.upper() > f64::from(LIST_HEIGHT),
            "control: the probe list reports more content than it is allocated",
        )?;

        hosted.adjustment.set_value(PUBLISHED_VALUE);
        settle(LAYOUT_SETTLE);
        hosted.host.queue_allocate();
        settle(LAYOUT_SETTLE);
        recorder.measure("value_at_rest", hosted.adjustment.value());
        recorder.control(
            same_value(hosted.adjustment.value(), PUBLISHED_VALUE),
            "control: a resting list at a positive height keeps the published value",
        )?;

        let emissions = hosted.count_value_changes();
        recorder.measure("page_before", hosted.adjustment.page_size());
        hosted.host.set_child_height(0);
        settle(LAYOUT_SETTLE);
        recorder.measure("value_after", hosted.adjustment.value());
        recorder.measure("page_after", hosted.adjustment.page_size());
        recorder.measure("value_changed_emissions", emissions.get());
        recorder.axiom(
            hosted.adjustment.page_size().abs() < f64::EPSILON,
            "A5: the zero-height list publishes a zero page",
        )?;
        recorder.axiom(
            (hosted.adjustment.value() - PUBLISHED_VALUE).abs() >= 1.0 && emissions.get() > 0,
            "A5: the zero-height list rewrites the host's value and emits value-changed",
        )
    })
}
