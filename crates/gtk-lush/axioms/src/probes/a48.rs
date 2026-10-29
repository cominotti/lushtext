// SPDX-License-Identifier: MIT OR Apache-2.0

//! A48: a subtree **made visible**, or **parented**, in a mapped window is
//! styled and measured in the **next frame's layout phase**, not at the
//! call: `set_visible(true)` and `append` only queue a resize, and the work
//! lands in the frame they queue.
//!
//! LushText reveals its staged content as visibility flips
//! (`first-frame-window-shell`, design D4), so a reveal's dispatch stays
//! small and its measure lands in the next frame; each stage waits for the
//! painted frame of its predecessor, so no frame carries two stages' first
//! measures.

use gtk4::prelude::*;

use super::composition::{CountingWidget, PhaseRecorder, first_phase_measuring};
use crate::observation::{Recorder, Stop};
use crate::session::{Presented, REALIZE_BUDGET, flush_events, settle, wait_until};
use crate::{AxiomId, Observation};

/// The width each counting widget requests.
pub const COUNTING_WIDTH: i32 = 120;

/// What one late composition showed.
struct Late {
    measures_at_call: u32,
    first_layout_frame: Option<i64>,
    frame_at_call: i64,
    phases: String,
}

/// Run `compose` in an idle of a mapped window, then wait for `widget`'s
/// first measure, stamping the frames.
fn observe(
    recorder: &mut Recorder,
    shown: &Presented,
    widget: &CountingWidget,
    compose: impl FnOnce() + 'static,
) -> Result<Late, Stop> {
    let clock = recorder.require(
        shown.window().frame_clock(),
        "control: the window has a frame clock",
    )?;
    let phases = PhaseRecorder::new(&clock, widget);
    let baseline = widget.measures();
    let at_call = std::rc::Rc::new(std::cell::Cell::new((u32::MAX, -1i64)));
    let record = std::rc::Rc::clone(&at_call);
    let watched = widget.clone();
    let clock_at_call = clock;
    gtk4::glib::idle_add_local_once(move || {
        compose();
        record.set((watched.measures(), clock_at_call.frame_counter()));
    });
    let measured = wait_until(REALIZE_BUDGET, || widget.measures() > baseline);
    flush_events();
    recorder.control(
        measured,
        "control: the composed widget is measured eventually",
    )?;
    let (measures_at_call, frame_at_call) = at_call.get();
    let stamps = phases.stamps();
    Ok(Late {
        measures_at_call: measures_at_call.saturating_sub(baseline),
        first_layout_frame: first_phase_measuring(&stamps, "layout", baseline)
            .map(|stamp| stamp.frame),
        frame_at_call,
        phases: phases.describe(),
    })
}

/// Probe A48.
#[must_use]
pub fn probe_a48() -> Observation {
    Recorder::run(AxiomId::new(48), |recorder| {
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let hidden = CountingWidget::new(COUNTING_WIDTH);
        hidden.set_visible(false);
        content.append(&hidden);
        let shown = Presented::checked(recorder, &content, "control: the fixture window realizes")?;
        settle(std::time::Duration::from_millis(50));
        recorder.control(
            hidden.measures() == 0,
            "control: the invisible widget was never measured",
        )?;

        let target = hidden.clone();
        let revealed = observe(recorder, &shown, &hidden, move || target.set_visible(true))?;
        recorder.measure("reveal_measures_at_call", revealed.measures_at_call);
        recorder.measure("reveal_frame_at_call", revealed.frame_at_call);
        recorder.measure(
            "reveal_first_measuring_layout_frame",
            revealed.first_layout_frame.unwrap_or(-1),
        );
        recorder.measure("reveal_phases", revealed.phases);
        recorder.axiom(
            revealed.measures_at_call == 0,
            "A48: making a subtree visible in a mapped window measures nothing at the call",
        )?;
        recorder.axiom(
            revealed
                .first_layout_frame
                .is_some_and(|frame| frame > revealed.frame_at_call),
            "A48: the revealed subtree is first measured in a later frame's layout phase",
        )?;

        let parented = CountingWidget::new(COUNTING_WIDTH);
        let appended = {
            let content = content.clone();
            let child = parented.clone();
            observe(recorder, &shown, &parented, move || content.append(&child))?
        };
        recorder.measure("parent_measures_at_call", appended.measures_at_call);
        recorder.measure(
            "parent_first_measuring_layout_frame",
            appended.first_layout_frame.unwrap_or(-1),
        );
        recorder.measure("parent_phases", appended.phases);
        recorder.axiom(
            appended.measures_at_call == 0
                && appended
                    .first_layout_frame
                    .is_some_and(|frame| frame > appended.frame_at_call),
            "A48: a subtree parented in a mapped window is first measured in a later frame's layout phase",
        )
    })
}
