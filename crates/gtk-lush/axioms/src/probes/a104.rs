// SPDX-License-Identifier: MIT OR Apache-2.0

//! A104: from GTK 4.24 a vertical `GtkListView` is **height-for-width**
//! (`gtk_list_view_get_request_mode`, GTK 4.23.2 `!9906` "Fix
//! Gtk.ListView/Gtk.GridView measure with for_size != -1"); below 4.24 it is
//! constant-size. A container that reports its child's request mode (as
//! `ViewportSliceBin` does) therefore becomes height-for-width around a 4.24 list, and
//! GTK measures its width for a height; a container that forwards that height
//! measures the list's width for a height below the list's minimum height,
//! which outside a scroller is its whole content (A2), and GTK logs
//! `Gtk-WARNING: Trying to measure GtkListView … for height of H, but it needs
//! at least N`. On 4.22 the container is constant-size and is never measured
//! for a height. A width measured **directly** for such a height warns on
//! both toolkits (measured on GTK 4.22.2 and 4.24.1; the floor half was first
//! stated as "silent" and corrected by the GNOME 50 SDK run, 2026-11-10).
//!
//! `ViewportSliceBin` measures its child's width for no height, since the
//! child is only ever allocated the visible band
//! (`gtk-lush-widgets` `viewport_slice_bin`).

use gtk4::prelude::*;

use crate::fixtures::HostedList;
use crate::observation::Recorder;
use crate::probes::text::LogCapture;
use crate::session::Presented;
use crate::version::catalogued_bound;
use crate::{AxiomId, Observation};

/// The warning text the probe counts.
pub const MEASURE_WARNING: &str = "Trying to measure GtkListView";

/// Measure `list`'s width for `height`, counting the list measure warnings it
/// logs, and return `(minimum width, warnings)`.
#[must_use]
pub fn width_for_height(list: &gtk4::ListView, height: i32) -> (i32, usize) {
    let capture = LogCapture::start();
    let (minimum, _, _, _) = list.measure(gtk4::Orientation::Horizontal, height);
    let warnings = capture
        .messages()
        .iter()
        .filter(|message| message.message.contains(MEASURE_WARNING))
        .count();
    (minimum, warnings)
}

/// Probe A104.
#[must_use]
pub fn probe_a104() -> Observation {
    Recorder::run(AxiomId::new(104), |recorder| {
        let hosted = HostedList::new();
        let _shown = Presented::checked(
            recorder,
            &hosted.host,
            "control: the fixture window realizes",
        )?;
        let (minimum_height, _, _, _) = hosted
            .list
            .measure(gtk4::Orientation::Vertical, hosted.list.width());
        recorder.measure("minimum_height", minimum_height);
        recorder.control(
            minimum_height > 0,
            "control: the list reports its content as a positive minimum height",
        )?;
        let mode = hosted.list.request_mode();
        recorder.measure("request_mode", format!("{mode:?}"));
        let (width_unconstrained, quiet) = width_for_height(&hosted.list, -1);
        let (width_at_minimum, at_minimum) = width_for_height(&hosted.list, minimum_height);
        let (width_for_zero, for_zero) = width_for_height(&hosted.list, 0);
        recorder.measure("width_for_no_height", width_unconstrained);
        recorder.measure("width_for_minimum_height", width_at_minimum);
        recorder.measure("width_for_zero_height", width_for_zero);
        recorder.measure("warnings_for_zero_height", for_zero);
        recorder.control(
            quiet == 0 && at_minimum == 0,
            "control: a width measured for no height, or for the minimum height, is silent",
        )?;
        recorder.control(
            for_zero > 0,
            "control: a width measured directly for a height below the list's minimum warns",
        )?;
        if catalogued_bound(recorder, AxiomId::new(104))?.reached() {
            recorder.axiom(
                mode == gtk4::SizeRequestMode::HeightForWidth,
                "A104 (GTK >= 4.24): a vertical list view is height-for-width, so a container \
                 taking its request mode from it is measured for a height",
            )
        } else {
            recorder.axiom(
                mode == gtk4::SizeRequestMode::ConstantSize,
                "A104 (GTK < 4.24): a list view is constant-size, so a container taking its \
                 request mode from it is never measured for a height",
            )
        }
    })
}
