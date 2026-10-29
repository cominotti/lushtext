// SPDX-License-Identifier: MIT OR Apache-2.0

//! A47: `gtk_window_show` (what `present` runs) validates CSS, realizes,
//! presents the toplevel and maps it, but **does not measure**: the window's
//! size computation (`compute-size`, its first `measure`) runs in the first
//! frame's layout phase. An **invisible** subtree is neither measured nor
//! mapped then: `gtk_widget_measure` of an invisible widget returns 0 without
//! measuring it, and `gtk_css_node_validate_internal` does not descend into a
//! child whose CSS node is invisible (`gtkcssnode.c`; the CSS half is
//! source-cited, because GTK exposes no observation of node validation).
//!
//! LushText builds its window whole with the staged content invisible
//! (`first-frame-window-shell`, design D2), so that content costs the realize,
//! present and first-frame dispatches nothing.

use gtk4::prelude::*;

use super::composition::{
    CountingWidget, PhaseRecorder, first_phase_measuring, measured_before_phase,
};
use crate::observation::Recorder;
use crate::session::{REALIZE_BUDGET, flush_events, wait_until};
use crate::{AxiomId, Observation};

/// The width each counting widget requests.
pub const COUNTING_WIDTH: i32 = 120;

/// Probe A47.
#[must_use]
pub fn probe_a47() -> Observation {
    Recorder::run(AxiomId::new(47), |recorder| {
        let visible = CountingWidget::new(COUNTING_WIDTH);
        let invisible = CountingWidget::new(COUNTING_WIDTH);
        invisible.set_visible(false);
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.append(&visible);
        content.append(&invisible);
        let window = libadwaita::Window::builder()
            .default_width(320)
            .default_height(200)
            .content(&content)
            .build();
        WidgetExt::realize(&window);
        let in_realize = visible.measures();
        let clock = recorder.require(
            window.frame_clock(),
            "control: the window has a frame clock",
        )?;
        let phases = PhaseRecorder::new(&clock, &visible);
        let before_present = visible.measures();
        window.present();
        let in_present = visible.measures() - before_present;
        let invisible_in_present = invisible.measures();
        let mapped_in_present = (visible.is_mapped(), invisible.is_mapped());
        let painted = wait_until(REALIZE_BUDGET, || visible.snapshots() > 0);
        flush_events();
        let stamps = phases.stamps();
        recorder.measure("measures_inside_realize", in_realize);
        recorder.measure("measures_inside_present", in_present);
        recorder.measure("invisible_measures_inside_present", invisible_in_present);
        recorder.measure("mapped_after_present", format!("{mapped_in_present:?}"));
        recorder.measure("phases", phases.describe());
        recorder.measure("invisible_measures_after_first_frame", invisible.measures());
        recorder.measure("invisible_mapped_after_first_frame", invisible.is_mapped());
        recorder.control(
            painted,
            "control: the first frame painted the visible widget",
        )?;
        recorder.axiom(
            in_realize == 0 && in_present == 0,
            "A47: neither realize nor present measures",
        )?;
        let first_layout = first_phase_measuring(&stamps, "layout", before_present);
        recorder.axiom(
            first_layout.is_some_and(|stamp| {
                !measured_before_phase(&stamps, stamp.frame, "layout", before_present)
            }),
            "A47: the first measure runs in the first frame's layout phase",
        )?;
        recorder.axiom(
            mapped_in_present.0 && !mapped_in_present.1,
            "A47: present maps the visible subtree and not the invisible one",
        )?;
        let untouched =
            invisible.measures() == 0 && invisible.allocations() == 0 && !invisible.is_mapped();
        // The recorder disconnects from the frame clock, which the window's
        // destruction disposes along with its handlers.
        drop(phases);
        window.destroy();
        flush_events();
        recorder.axiom(
            untouched,
            "A47: an invisible subtree is neither measured, allocated nor mapped by the first frame",
        )
    })
}
