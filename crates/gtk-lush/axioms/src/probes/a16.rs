// SPDX-License-Identifier: MIT OR Apache-2.0

//! A16: when an `AdwBreakpoint`'s setters are applied and unapplied relative
//! to the child allocation, and what an unapply restores.
//!
//! The probe resizes the bin across the breakpoint's condition, which is what
//! a window resize does, and records the frame and the child-allocation count
//! at which `apply`/`unapply` fire, and what the setter's target shows as
//! each child allocation begins. The application writes the setter's
//! property itself at every stage — before the breakpoint is installed,
//! before the fixture is presented, before the first apply, while applied,
//! and between two applies — and the probe reads what each unapply leaves.

use libadwaita::prelude::*;

use super::LAYOUT_SETTLE;
use super::adaptive::{AllocationSight, BreakpointFixture, SignalSite};
use crate::observation::{Recorder, Stop};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// The breakpoint's condition, in sp.
pub const CONDITION_SP: i32 = 600;
/// A width outside the condition.
pub const WIDE_WIDTH: i32 = 700;
/// A width inside it.
pub const NARROW_WIDTH: i32 = 500;
/// The value the breakpoint's setter writes.
pub const APPLIED_LABEL: &str = "applied";
/// The value the property holds when the setter is added.
pub const WRITE_BEFORE_ADDING_THE_SETTER: &str = "written before adding the setter";
/// The value the application writes after the setter is added, before the
/// breakpoint is installed on the bin.
pub const WRITE_BEFORE_INSTALLING: &str = "written before installing";
/// The value the application writes after the breakpoint is installed,
/// before the fixture is presented.
pub const WRITE_BEFORE_PRESENTING: &str = "written before presenting";
/// The value the application writes before the first apply.
pub const WRITE_BEFORE_FIRST_APPLY: &str = "written before the first apply";
/// The value the application writes while the breakpoint is applied.
pub const WRITE_WHILE_APPLIED: &str = "written while applied";
/// The value the application writes while it is not, between two applies.
pub const WRITE_BETWEEN_APPLIES: &str = "written between applies";

/// One resize across the condition: the signal site it fired and the child
/// allocations it caused.
struct Crossing {
    site: SignalSite,
    content_before: u32,
    content_after: u32,
    sights: Vec<AllocationSight>,
    label_after: String,
}

impl Crossing {
    /// The sights as `label@frame` entries, for the observation.
    fn describe(&self) -> String {
        self.sights
            .iter()
            .map(|sight| format!("{}@{}", sight.label, sight.frame))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// Whether the signal fired inside the bin's allocation, after the child
    /// had been allocated once at the new width with `stale` and before any
    /// other child allocation, and in the same frame as that allocation.
    fn fired_after_one_stale_allocation(&self, stale: &str) -> bool {
        self.site.inside_bin_allocation
            && self.site.content_allocations == self.content_before + 1
            && self
                .sights
                .first()
                .is_some_and(|sight| sight.label == stale && sight.frame == self.site.frame)
    }

    /// Whether a later child allocation saw `fresh`, and in which frame
    /// relative to the signal.
    fn reallocated_with(&self, fresh: &str) -> Option<i64> {
        self.sights
            .iter()
            .skip(1)
            .find(|sight| sight.label == fresh)
            .map(|sight| sight.frame - self.site.frame)
    }
}

/// Resize the bin to `width`, settle, and read what the crossing did.
fn cross(
    recorder: &mut Recorder,
    fixture: &BreakpointFixture,
    site: &std::rc::Rc<std::cell::Cell<SignalSite>>,
    width: i32,
) -> Result<Crossing, Stop> {
    let _ = fixture.take_seen_at_allocation();
    let content_before = fixture.content.allocations();
    let count_before = site.get().count;
    let allocated = fixture.allocate_at(width);
    settle(LAYOUT_SETTLE);
    recorder.control(
        allocated && site.get().count == count_before + 1,
        "control: the resize crosses the condition and fires the signal once",
    )?;
    Ok(Crossing {
        site: site.get(),
        content_before,
        content_after: fixture.content.allocations(),
        sights: fixture.take_seen_at_allocation(),
        label_after: fixture.label.text().to_string(),
    })
}

/// Probe A16. See the module documentation.
#[must_use]
pub fn probe_a16() -> Observation {
    Recorder::run(AxiomId::new(16), |recorder| {
        let fixture = BreakpointFixture::new(WIDE_WIDTH);
        fixture.label.set_text(WRITE_BEFORE_ADDING_THE_SETTER);
        let Some(breakpoint) = fixture.max_width_breakpoint(CONDITION_SP, APPLIED_LABEL) else {
            return recorder.control(false, "control: the condition parses");
        };
        fixture.label.set_text(WRITE_BEFORE_INSTALLING);
        fixture.bin.add_breakpoint(breakpoint.clone());
        fixture.label.set_text(WRITE_BEFORE_PRESENTING);
        let _shown = Presented::checked(
            recorder,
            &fixture.host,
            "control: the fixture window realizes",
        )?;
        let applies = fixture.watch_signal(&breakpoint, false, APPLIED_LABEL);
        let unapplies = fixture.watch_signal(&breakpoint, true, APPLIED_LABEL);
        recorder.control(
            !fixture.is_current(&breakpoint) && fixture.label.text() == WRITE_BEFORE_PRESENTING,
            "control: at rest the breakpoint is not applied",
        )?;

        // The application writes the property before the breakpoint first
        // applies, then the first apply and unapply, with a write while
        // applied in between.
        fixture.label.set_text(WRITE_BEFORE_FIRST_APPLY);
        settle(LAYOUT_SETTLE);
        let apply = cross(recorder, &fixture, &applies, NARROW_WIDTH)?;
        recorder.measure("apply_frame", apply.site.frame);
        recorder.measure(
            "apply_inside_bin_allocation",
            apply.site.inside_bin_allocation,
        );
        recorder.measure("apply_content_allocations_before", apply.content_before);
        recorder.measure(
            "apply_content_allocations_at_signal",
            apply.site.content_allocations,
        );
        recorder.measure("apply_content_allocations_after", apply.content_after);
        recorder.measure(
            "apply_signal_saw_setter_value",
            apply.site.label_was_applied_value,
        );
        recorder.measure("apply_child_allocations", apply.describe());
        recorder.measure("label_after_apply", &apply.label_after);

        fixture.label.set_text(WRITE_WHILE_APPLIED);
        settle(LAYOUT_SETTLE);
        let unapply = cross(recorder, &fixture, &unapplies, WIDE_WIDTH)?;
        recorder.measure("unapply_frame", unapply.site.frame);
        recorder.measure(
            "unapply_inside_bin_allocation",
            unapply.site.inside_bin_allocation,
        );
        recorder.measure("unapply_content_allocations_before", unapply.content_before);
        recorder.measure(
            "unapply_content_allocations_at_signal",
            unapply.site.content_allocations,
        );
        recorder.measure("unapply_content_allocations_after", unapply.content_after);
        recorder.measure("unapply_child_allocations", unapply.describe());
        recorder.measure("label_after_first_unapply", &unapply.label_after);

        // A write while unapplied, then a second apply and unapply.
        fixture.label.set_text(WRITE_BETWEEN_APPLIES);
        settle(LAYOUT_SETTLE);
        let reapply = cross(recorder, &fixture, &applies, NARROW_WIDTH)?;
        let second_unapply = cross(recorder, &fixture, &unapplies, WIDE_WIDTH)?;
        recorder.measure("label_after_second_apply", &reapply.label_after);
        recorder.measure("label_after_second_unapply", &second_unapply.label_after);

        let reapplied_frame = apply.reallocated_with(APPLIED_LABEL);
        let restored_frame = unapply.reallocated_with(&unapply.label_after);
        recorder.measure(
            "apply_fresh_reallocation_frame_offset",
            reapplied_frame.map_or_else(|| "none".to_owned(), |offset| offset.to_string()),
        );
        recorder.measure(
            "unapply_fresh_reallocation_frame_offset",
            restored_frame.map_or_else(|| "none".to_owned(), |offset| offset.to_string()),
        );

        recorder.axiom(
            apply.fired_after_one_stale_allocation(WRITE_BEFORE_FIRST_APPLY),
            "A16: on a resize into the condition the child is first allocated at the new width \
             with the setters not yet applied, then apply fires, in the same frame and inside \
             the bin's allocation",
        )?;
        recorder.axiom(
            unapply.fired_after_one_stale_allocation(WRITE_WHILE_APPLIED),
            "A16: on a resize out of the condition the child is first allocated at the new width \
             with the setters still applied, then unapply fires, in the same frame and inside \
             the bin's allocation",
        )?;
        recorder.axiom(
            reapplied_frame == Some(1) && restored_frame == Some(1),
            "A16: the child sees the new property values from the allocation one frame later",
        )?;
        recorder.axiom(
            apply.label_after == APPLIED_LABEL
                && unapply.label_after == WRITE_BEFORE_ADDING_THE_SETTER,
            "A16: unapply restores the value the property held when the setter was added, \
             discarding every later write, including one made while the breakpoint was applied",
        )?;
        recorder.axiom(
            reapply.label_after == APPLIED_LABEL
                && second_unapply.label_after == WRITE_BEFORE_ADDING_THE_SETTER,
            "A16: a write made while unapplied is overwritten by the next apply and not restored \
             by the next unapply",
        )
    })
}
