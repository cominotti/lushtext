// SPDX-License-Identifier: MIT OR Apache-2.0

//! A15: when a condition changed with `AdwBreakpoint::set_condition` on an
//! installed breakpoint is re-evaluated.
//!
//! The bin rests at a fixed width. The probe moves the condition across that
//! width in both directions and reads the bin's `current-breakpoint`, the
//! breakpoint's `apply`/`unapply` count, and the setter's target right after
//! `set_condition` returns and again after the allocation that follows.

use gtk4::prelude::*;

use super::LAYOUT_SETTLE;
use super::adaptive::{ALLOCATION_BUDGET, AllocationSight, BreakpointFixture, ORIGINAL_LABEL};
use crate::observation::Recorder;
use crate::session::{Presented, settle, wait_until};
use crate::{AxiomId, Observation};

/// The width the bin rests at for the whole probe.
pub const REST_WIDTH: i32 = 700;
/// The condition that does not match the rest width.
pub const NARROW_SP: i32 = 600;
/// The condition that does.
pub const WIDE_SP: i32 = 800;
/// The setter value the breakpoint writes.
pub const APPLIED_LABEL: &str = "applied";

/// Probe A15. See the module documentation.
#[must_use]
pub fn probe_a15() -> Observation {
    Recorder::run(AxiomId::new(15), |recorder| {
        let fixture = BreakpointFixture::new(REST_WIDTH);
        let Some(breakpoint) = fixture.add_max_width_breakpoint(NARROW_SP, APPLIED_LABEL) else {
            return recorder.control(false, "control: the condition parses");
        };
        let _shown = Presented::checked(
            recorder,
            &fixture.host,
            "control: the fixture window realizes",
        )?;
        let applies = fixture.watch_signal(&breakpoint, false, APPLIED_LABEL);
        let unapplies = fixture.watch_signal(&breakpoint, true, APPLIED_LABEL);
        recorder.measure("bin_width", fixture.bin.width());
        recorder.control(
            fixture.bin.width() == REST_WIDTH && !fixture.is_current(&breakpoint),
            "control: at rest the narrow condition does not match",
        )?;

        // Widen the condition so it matches the width the bin already has.
        let _ = fixture.take_seen_at_allocation();
        let widen_allocations = fixture.content.allocations();
        let bin_allocations = widen_allocations;
        let widen_call_frame = fixture.frame();
        BreakpointFixture::set_max_width(&breakpoint, WIDE_SP);
        let sync_current = fixture.is_current(&breakpoint);
        let sync_applies = applies.get().count;
        let sync_label = fixture.label.text();
        let sync_allocations = fixture.content.allocations() - bin_allocations;
        recorder.measure("widen_sync_current", sync_current);
        recorder.measure("widen_sync_apply_count", sync_applies);
        recorder.measure("widen_sync_label", &sync_label);
        recorder.measure("widen_sync_content_allocations", sync_allocations);
        let reallocated = wait_until(ALLOCATION_BUDGET, || {
            fixture.content.allocations() > bin_allocations
        });
        settle(LAYOUT_SETTLE);
        let later_current = fixture.is_current(&breakpoint);
        recorder.measure("widen_reallocated_unprompted", reallocated);
        recorder.measure("widen_later_current", later_current);
        recorder.measure("widen_later_apply_count", applies.get().count);
        recorder.measure("widen_later_label", fixture.label.text());
        recorder.measure(
            "widen_later_content_allocations",
            fixture.content.allocations() - bin_allocations,
        );
        recorder.measure(
            "widen_content_allocations_at_apply",
            applies.get().content_allocations - bin_allocations,
        );
        let widen_sights = fixture.take_seen_at_allocation();
        let widen_child_saw = widen_sights.first().cloned();
        recorder.measure("widen_call_frame", widen_call_frame);
        recorder.measure("widen_apply_frame", applies.get().frame);
        recorder.measure("widen_child_allocations", describe(&widen_sights));

        // Narrow it back so it no longer matches.
        let _ = fixture.take_seen_at_allocation();
        let bin_allocations = fixture.content.allocations();
        let narrow_call_frame = fixture.frame();
        BreakpointFixture::set_max_width(&breakpoint, NARROW_SP);
        let narrow_sync_current = fixture.is_current(&breakpoint);
        let narrow_sync_unapplies = unapplies.get().count;
        let narrow_sync_label = fixture.label.text();
        recorder.measure("narrow_sync_current", narrow_sync_current);
        recorder.measure("narrow_sync_unapply_count", narrow_sync_unapplies);
        recorder.measure("narrow_sync_label", &narrow_sync_label);
        let narrow_reallocated = wait_until(ALLOCATION_BUDGET, || {
            fixture.content.allocations() > bin_allocations
        });
        settle(LAYOUT_SETTLE);
        let narrow_later_current = fixture.is_current(&breakpoint);
        recorder.measure("narrow_reallocated_unprompted", narrow_reallocated);
        recorder.measure("narrow_later_current", narrow_later_current);
        recorder.measure("narrow_later_unapply_count", unapplies.get().count);
        recorder.measure("narrow_later_label", fixture.label.text());
        recorder.measure(
            "narrow_content_allocations_at_unapply",
            unapplies.get().content_allocations - bin_allocations,
        );
        let narrow_sights = fixture.take_seen_at_allocation();
        let narrow_child_saw = narrow_sights.first().cloned();
        recorder.measure("narrow_call_frame", narrow_call_frame);
        recorder.measure("narrow_unapply_frame", unapplies.get().frame);
        recorder.measure("narrow_child_allocations", describe(&narrow_sights));

        recorder.axiom(
            !sync_current && sync_applies == 0 && sync_label == ORIGINAL_LABEL,
            "A15: set_condition does not apply a newly matching breakpoint synchronously",
        )?;
        recorder.axiom(
            reallocated && later_current && applies.get().count == 1,
            "A15: set_condition schedules an allocation of the bin by itself, and the breakpoint \
             applies there",
        )?;
        let widen_apply_frame = applies.get().frame;
        recorder.axiom(
            widen_apply_frame == widen_call_frame + 1
                && applies.get().content_allocations == widen_allocations
                && widen_child_saw.as_ref().is_some_and(|sight| {
                    sight.label == APPLIED_LABEL && sight.frame == widen_apply_frame + 1
                }),
            "A15: apply fires in the frame after set_condition, and the child first sees the \
             setter's value in the frame after that",
        )?;
        recorder.axiom(
            narrow_sync_current && narrow_sync_unapplies == 0 && narrow_sync_label == APPLIED_LABEL,
            "A15: set_condition does not unapply a no-longer-matching breakpoint synchronously",
        )?;
        recorder.axiom(
            narrow_reallocated && !narrow_later_current && unapplies.get().count == 1,
            "A15: the unapply happens at the allocation set_condition schedules",
        )?;
        let narrow_unapply_frame = unapplies.get().frame;
        recorder.axiom(
            narrow_unapply_frame == narrow_call_frame + 1
                && unapplies.get().content_allocations == bin_allocations
                && narrow_child_saw.as_ref().is_some_and(|sight| {
                    sight.label == ORIGINAL_LABEL && sight.frame == narrow_unapply_frame + 1
                }),
            "A15: unapply fires in the frame after set_condition, and the child first sees the \
             restored value in the frame after that",
        )
    })
}

/// The sights as `label@frame` entries, for the observation.
fn describe(sights: &[AllocationSight]) -> String {
    sights
        .iter()
        .map(|sight| format!("{}@{}", sight.label, sight.frame))
        .collect::<Vec<_>>()
        .join(" | ")
}
