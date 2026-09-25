// SPDX-License-Identifier: MIT OR Apache-2.0

//! A2: a list outside a scroller reports its whole content as its minimum
//! height. `ViewportSliceBin::measure` passes that minimum on to the outer
//! scroller, so the outer range covers every row, including the rows the list
//! has not realized.

use gtk4::prelude::*;

use super::row_stride;
use crate::fixtures::{HostedList, LIST_HEIGHT, ROWS};
use crate::observation::Recorder;
use crate::session::Presented;
use crate::{AxiomId, Observation};

/// Probe A2. See the module documentation.
#[must_use]
pub fn probe_a02() -> Observation {
    Recorder::run(AxiomId::new(2), |recorder| {
        let hosted = HostedList::new();
        let _shown = Presented::checked(
            recorder,
            &hosted.host,
            "control: the fixture window realizes",
        )?;
        let content = hosted.adjustment.upper();
        let stride = row_stride(&hosted.adjustment, ROWS);
        recorder.measure("content_height", content);
        recorder.measure("allocated_height", LIST_HEIGHT);
        recorder.control(
            content > f64::from(LIST_HEIGHT) * 10.0,
            "control: the list holds far more content than it is allocated",
        )?;

        let (minimum, natural, _, _) = hosted
            .list
            .measure(gtk4::Orientation::Vertical, hosted.list.width());
        recorder.measure("minimum_height", minimum);
        recorder.measure("natural_height", natural);
        let within_a_row = |height: i32| (f64::from(height) - content).abs() < stride;
        recorder.axiom(
            within_a_row(minimum),
            "A2: the list's minimum height is its whole content",
        )?;
        recorder.axiom(
            within_a_row(natural),
            "A2: the list's natural height is its whole content too",
        )
    })
}
