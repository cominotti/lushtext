// SPDX-License-Identifier: MIT OR Apache-2.0

//! A7: a `GtkListView` re-derives its value from its scroll anchor when its
//! page changes. The anchor is a row plus an alignment expressed as a
//! fraction of the page, so the value is a straight line in the page height,
//! `value = anchor_position − align × page`, and a page change moves the
//! value although nobody wrote it. The alignment is not confined to
//! `[0, 1]`: after a host moves the value with `set_value`, GTK 4.22 was
//! measured keeping a row far above the view as the anchor (align ≈ −4.4), so
//! the value moved about 4.4 px per pixel of page change. After `scroll_to` a
//! row below the view, the anchor is that row's top edge at the fraction of
//! the page where it landed (≈ 0.89 here), so shrinking the page by 100 px
//! moves the value by about 89 px. This settle is what the bound in
//! `classify_child_scroll` (`reconfigure_shift`) accounts for.

use gtk4::prelude::*;

use super::{LAYOUT_SETTLE, SCROLL_SETTLE, row_stride, same_value};
use crate::fixtures::{HostedList, LIST_HEIGHT, ROWS, count_value_changes};
use crate::observation::Recorder;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// The page heights the host sweeps through, starting from [`LIST_HEIGHT`].
pub const PAGE_SWEEP: [i32; 3] = [250, 200, 150];
/// The page height the bottom-anchored list shrinks to.
pub(crate) const SHRUNK_HEIGHT: i32 = 200;
/// The row the probe anchors at the bottom edge.
pub const BOTTOM_ANCHORED_ROW: u32 = 200;
/// The row the host moves the value to before the sweep.
pub const HOST_VALUE_ROW: u32 = 40;
/// A value re-derived at an integer page may round by up to a pixel.
pub(crate) const ROUNDING: f64 = 1.5;

/// Probe A7. See the module documentation.
#[must_use]
pub fn probe_a07() -> Observation {
    Recorder::run(AxiomId::new(7), |recorder| {
        let swept = HostedList::new();
        let shown = Presented::checked(
            recorder,
            &swept.host,
            "control: the fixture window realizes",
        )?;
        let stride = row_stride(&swept.adjustment, ROWS);
        recorder.measure("row_stride", stride);
        let host_value = f64::from(HOST_VALUE_ROW) * stride;
        swept.adjustment.set_value(host_value);
        settle(LAYOUT_SETTLE);
        recorder.measure("value_at_page_300", swept.adjustment.value());
        recorder.control(
            same_value(swept.adjustment.value(), host_value),
            "control: the host's value holds at rest",
        )?;

        let mut points = vec![(f64::from(LIST_HEIGHT), swept.adjustment.value())];
        for (page, key) in PAGE_SWEEP.into_iter().zip([
            "value_at_page_250",
            "value_at_page_200",
            "value_at_page_150",
        ]) {
            swept.host.set_child_height(page);
            settle(LAYOUT_SETTLE);
            recorder.measure(key, swept.adjustment.value());
            points.push((f64::from(page), swept.adjustment.value()));
        }
        let (first_page, first_value) = points[0];
        let (last_page, last_value) = points[points.len() - 1];
        let slope = (last_value - first_value) / (last_page - first_page);
        recorder.measure("implied_align", -slope);
        let on_one_line = points.iter().all(|&(page, value)| {
            (first_value + slope * (page - first_page) - value).abs() < ROUNDING
        });
        recorder.axiom(
            on_one_line,
            "A7: across page changes the value stays on one line in the page height \
             (re-derived from one anchor)",
        )?;
        recorder.axiom(
            (last_value - first_value).abs() >= 1.0,
            "A7: a page change moves the value although nobody wrote it",
        )?;
        drop(shown);

        let bottom = HostedList::new();
        let _shown = Presented::checked(
            recorder,
            &bottom.host,
            "control: the second fixture window realizes",
        )?;
        bottom
            .list
            .scroll_to(BOTTOM_ANCHORED_ROW, gtk4::ListScrollFlags::NONE, None);
        settle(SCROLL_SETTLE);
        let bottom_value = bottom.adjustment.value();
        recorder.measure("bottom_anchored_value", bottom_value);
        recorder.control(
            bottom_value > 0.0,
            "control: scroll_to a row below the view scrolls down",
        )?;
        let emissions = count_value_changes(&bottom.adjustment);
        bottom.host.set_child_height(SHRUNK_HEIGHT);
        settle(LAYOUT_SETTLE);
        recorder.measure(
            "bottom_anchored_value_after_shrink",
            bottom.adjustment.value(),
        );
        recorder.measure("value_changed_emissions", emissions.get());
        // scroll_to anchors the target row's top edge at the fraction of the
        // page where it landed, so the shrunk value is predictable from it.
        let row_top = f64::from(BOTTOM_ANCHORED_ROW) * stride;
        let align = (row_top - bottom_value) / f64::from(LIST_HEIGHT);
        let predicted = row_top - align * f64::from(SHRUNK_HEIGHT);
        recorder.measure("bottom_anchored_align", align);
        recorder.measure("bottom_anchored_predicted_value", predicted);
        recorder.axiom(
            (bottom.adjustment.value() - predicted).abs() < ROUNDING,
            "A7: after scroll_to the value is re-derived from the target row's top edge and \
             its alignment in the page",
        )?;
        recorder.axiom(
            emissions.get() > 0,
            "A7: the re-derived value is published with a value-changed",
        )
    })
}
