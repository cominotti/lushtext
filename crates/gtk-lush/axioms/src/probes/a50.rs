// SPDX-License-Identifier: MIT OR Apache-2.0

//! A50: how `AdwBreakpointBin` (the bin inside `AdwWindow` and
//! `AdwApplicationWindow`) applies a breakpoint, by when it starts matching.
//!
//! - On the **first allocation after `map`** (`first_allocation`), a matching
//!   breakpoint's setters are applied **before** the child is allocated, so
//!   the first frame draws the breakpoint's state.
//! - A breakpoint that starts matching on a **later** allocation — a resize
//!   into its condition, or a breakpoint **added after map** — makes the bin
//!   allocate the child with the old values, snapshot it into `old_node`,
//!   hide the child (`child-visible` false), apply the setters, and swap back
//!   on the next tick (`breakpoint_changed_tick_cb`): one frame draws a frozen
//!   snapshot of the old state, and the next frame the new one.
//!
//! Libadwaita 1.9 `adw-breakpoint-bin.c`. LushText installs its three
//! breakpoints before the window is realized (`first-frame-window-shell`,
//! design D2), so a restored layout's breakpoint applies in the first frame
//! and the chrome never shows the snapshot swap at startup.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita::prelude::*;

use super::LAYOUT_SETTLE;
use super::adaptive::{
    ALLOCATION_BUDGET, APPLIED_LABEL, BreakpointFixture, ORIGINAL_LABEL, describe_sights,
};
use crate::observation::{Recorder, Stop};
use crate::session::{Presented, settle, wait_until};
use crate::{AxiomId, Observation};

/// The breakpoint's condition, in sp.
pub const CONDITION_SP: i32 = 600;
/// A width inside the condition.
pub const MATCHING_WIDTH: i32 = 500;
/// A width outside it.
pub const WIDE_WIDTH: i32 = 700;

/// One frame as the host's tick callback saw it: the frame counter, whether
/// the bin's child was child-visible, and the setter target's text.
#[derive(Clone, Debug)]
struct Tick {
    frame: i64,
    child_visible: bool,
    label: String,
}

/// Record every frame tick of `fixture`'s host until dropped.
struct TickLog {
    ticks: Rc<RefCell<Vec<Tick>>>,
    id: Option<gtk4::TickCallbackId>,
}

impl TickLog {
    fn new(fixture: &BreakpointFixture) -> Self {
        let ticks = Rc::new(RefCell::new(Vec::new()));
        let record = Rc::clone(&ticks);
        let content = fixture.content.downgrade();
        let label = fixture.label.downgrade();
        let id = fixture.host.add_tick_callback(move |_, clock| {
            record.borrow_mut().push(Tick {
                frame: clock.frame_counter(),
                child_visible: content
                    .upgrade()
                    .is_some_and(|content| content.is_child_visible()),
                label: label
                    .upgrade()
                    .map_or_else(String::new, |label| label.text().to_string()),
            });
            gtk4::glib::ControlFlow::Continue
        });
        Self {
            ticks,
            id: Some(id),
        }
    }

    fn take(&self) -> Vec<Tick> {
        self.ticks.take()
    }

    /// Whether a tick has seen the swap complete: a hidden child, then a
    /// visible one showing the applied value.
    fn saw_swap(&self) -> bool {
        swapped_ticks(&self.ticks.borrow())
    }

    fn describe(ticks: &[Tick]) -> String {
        super::join_entries(
            ticks
                .iter()
                .map(|tick| format!("{}:{}:{}", tick.frame, tick.child_visible, tick.label)),
        )
    }
}

impl Drop for TickLog {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            id.remove();
        }
    }
}

/// Whether the frames show the snapshot swap: one tick saw the child hidden,
/// and a later tick saw it visible again with the applied value.
fn swapped_ticks(ticks: &[Tick]) -> bool {
    ticks
        .iter()
        .position(|tick| !tick.child_visible)
        .is_some_and(|hidden| {
            ticks[hidden..]
                .iter()
                .any(|tick| tick.child_visible && tick.label == APPLIED_LABEL)
        })
}

/// Whether a later switch went through the snapshot swap: the frames show it
/// ([`swapped_ticks`]) and, when the switch also changed the content's width,
/// the content was allocated with the old value before the applied one. A
/// switch that keeps the width (a breakpoint added after map) is not checked
/// on allocations: GTK skips a child's `size_allocate` when its allocation is
/// unchanged, so the old-value allocation the bin makes is not observable.
fn swapped(
    ticks: &[Tick],
    sights: &[super::adaptive::AllocationSight],
    width_changed: bool,
) -> bool {
    let old_first = sights
        .first()
        .is_some_and(|sight| sight.label == ORIGINAL_LABEL);
    let applied_later = sights
        .iter()
        .skip(1)
        .any(|sight| sight.label == APPLIED_LABEL);
    swapped_ticks(ticks) && (!width_changed || (old_first && applied_later))
}

/// A breakpoint that starts matching after map: added after map at a
/// matching width (`added_after_map`), or added before map and resized into.
fn later_switch(
    recorder: &mut Recorder,
    added_after_map: bool,
) -> Result<(String, String, bool), Stop> {
    let fixture = BreakpointFixture::new(if added_after_map {
        MATCHING_WIDTH
    } else {
        WIDE_WIDTH
    });
    let breakpoint = if added_after_map {
        None
    } else {
        Some(recorder.require(
            fixture.add_max_width_breakpoint(CONDITION_SP, APPLIED_LABEL),
            "control: the condition parses",
        )?)
    };
    let _shown = Presented::checked(
        recorder,
        &fixture.host,
        "control: the fixture window realizes",
    )?;
    recorder.control(
        fixture.label.text() == ORIGINAL_LABEL,
        "control: no breakpoint applies before the switch",
    )?;
    let _ = fixture.take_seen_at_allocation();
    let ticks = TickLog::new(&fixture);
    if added_after_map {
        recorder.require(
            fixture.add_max_width_breakpoint(CONDITION_SP, APPLIED_LABEL),
            "control: the condition parses",
        )?;
        fixture.bin.queue_allocate();
        fixture.host.queue_allocate();
    } else {
        recorder.control(
            fixture.allocate_at(MATCHING_WIDTH),
            "control: the bin is allocated again",
        )?;
    }
    // Wait for the swap frame itself: the setters apply inside the layout of
    // the switching frame, and the child is shown again only on the next tick.
    let _ = wait_until(ALLOCATION_BUDGET, || ticks.saw_swap());
    settle(LAYOUT_SETTLE);
    let applied = fixture.label.text() == APPLIED_LABEL;
    let current = breakpoint.as_ref().map_or_else(
        || fixture.bin.current_breakpoint().is_some(),
        |breakpoint| fixture.is_current(breakpoint),
    );
    recorder.control(applied && current, "control: the breakpoint applied")?;
    let seen = fixture.take_seen_at_allocation();
    let frames = ticks.take();
    Ok((
        describe_sights(&seen),
        TickLog::describe(&frames),
        swapped(&frames, &seen, !added_after_map),
    ))
}

/// Probe A50.
#[must_use]
pub fn probe_a50() -> Observation {
    Recorder::run(AxiomId::new(50), |recorder| {
        // 1. Installed before map, matching on the first allocation.
        let fixture = BreakpointFixture::new(MATCHING_WIDTH);
        recorder.require(
            fixture.add_max_width_breakpoint(CONDITION_SP, APPLIED_LABEL),
            "control: the condition parses",
        )?;
        let shown = Presented::checked(
            recorder,
            &fixture.host,
            "control: the fixture window realizes",
        )?;
        let first_sights = fixture.take_seen_at_allocation();
        recorder.measure("first_allocation_sights", describe_sights(&first_sights));
        recorder.control(
            !first_sights.is_empty(),
            "control: the content was allocated",
        )?;
        recorder.axiom(
            first_sights.iter().all(|sight| sight.label == APPLIED_LABEL),
            "A50: on the first allocation after map, a matching breakpoint applies before the child is allocated",
        )?;
        drop(shown);

        // 2. Installed before map, resized into after map.
        let (sights, ticks, resize_swapped) = later_switch(recorder, false)?;
        recorder.measure("resize_sights", sights);
        recorder.measure("resize_ticks", ticks);
        recorder.axiom(
            resize_swapped,
            "A50: a breakpoint that starts matching later allocates the child with the old values, hides it for one frame, then swaps",
        )?;

        // 3. Added after map at a matching width.
        let (sights, ticks, added_swapped) = later_switch(recorder, true)?;
        recorder.measure("added_after_map_sights", sights);
        recorder.measure("added_after_map_ticks", ticks);
        recorder.axiom(
            added_swapped,
            "A50: a breakpoint added after map goes through the same one-frame snapshot swap",
        )
    })
}
