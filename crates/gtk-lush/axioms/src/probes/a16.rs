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
//!
//! It then installs two nested breakpoints whose setters write the same
//! property of the same object — to different values, and to equal ones —
//! and switches the current breakpoint directly from one to the other, both
//! by resizing the bin across the inner condition and, at a fixed width, by
//! changing only `gtk-xft-dpi`. It records the property after each switch,
//! the order in which the outgoing `unapply` and the incoming `apply` fire,
//! what the property reads inside each handler, and every value
//! `notify::label` reports during the switch.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita::prelude::*;

use super::LAYOUT_SETTLE;
use super::adaptive::{
    ALLOCATION_BUDGET, AllocationSight, BreakpointFixture, SettingsOverride, SignalSite,
};
use crate::observation::{Recorder, Stop};
use crate::session::{Presented, settle, wait_until};
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

/// The outer of the two nested breakpoints, added first, in sp.
pub const OUTER_SP: i32 = 900;
/// The inner one, added last, so it wins wherever both match (A18), in sp.
pub const INNER_SP: i32 = 500;
/// A width inside only the outer condition.
pub const OUTER_ONLY_WIDTH: i32 = 700;
/// A width inside both conditions.
pub const INSIDE_BOTH_WIDTH: i32 = 450;
/// A width outside both conditions.
pub const OUTSIDE_BOTH_WIDTH: i32 = 1_000;
/// The fixed width of the text-scale switch: inside only the outer condition
/// at text scale 1.0 (900 ≤ 900 px), inside both at [`DOUBLED_TEXT_SCALE`]
/// (900 ≤ 1000 px).
pub const RESCALE_REST_WIDTH: i32 = 900;
/// The text scale, in permille, at which the fixed-width bin matches the
/// inner breakpoint too.
pub const DOUBLED_TEXT_SCALE: i32 = 2000;
/// The value the property holds when both setters are added.
pub const WRITE_BEFORE_ADDING_BOTH_SETTERS: &str = "written before adding both setters";
/// The outer breakpoint's setter value when the two differ.
pub const OUTER_VALUE: &str = "outer";
/// The inner breakpoint's setter value when the two differ.
pub const INNER_VALUE: &str = "inner";
/// Both breakpoints' setter value when they are equal.
pub const SHARED_VALUE: &str = "shared";

/// What one direct switch between the two nested breakpoints showed.
struct Switch {
    /// The property after the switch.
    label_after: String,
    /// The switch's events in order: `unapply <name>[<label>]`,
    /// `apply <name>[<label>]`, and `notify[<label>]`.
    events: Vec<String>,
}

impl Switch {
    /// The events, for the observation.
    fn describe(&self) -> String {
        if self.events.is_empty() {
            "none".to_owned()
        } else {
            self.events.join(" | ")
        }
    }

    /// Whether the switch was exactly: the outgoing breakpoint's `unapply`,
    /// with the property still at its value; one `notify::label` carrying
    /// the incoming value; then the incoming breakpoint's `apply`.
    fn is_direct(&self, leg: &Leg<'_>) -> bool {
        self.events
            == [
                format!("unapply {}[{}]", leg.from, leg.from_value),
                format!("notify[{}]", leg.to_value),
                format!("apply {}[{}]", leg.to, leg.to_value),
            ]
    }

    /// Whether any event read `value` in the property.
    fn saw(&self, value: &str) -> bool {
        let needle = format!("[{value}]");
        self.events.iter().any(|event| event.ends_with(&needle))
    }
}

/// One switch with the breakpoints it leaves and enters and their setter
/// values.
struct Leg<'a> {
    switch: &'a Switch,
    from: &'static str,
    from_value: &'a str,
    to: &'static str,
    to_value: &'a str,
}

/// A `max-width: <sp>sp` breakpoint whose one setter writes `value` into
/// `label`.
fn max_width_setter(sp: i32, label: &gtk4::Label, value: &str) -> libadwaita::Breakpoint {
    let breakpoint = libadwaita::Breakpoint::new(libadwaita::BreakpointCondition::new_length(
        libadwaita::BreakpointConditionLengthType::MaxWidth,
        f64::from(sp),
        libadwaita::LengthUnit::Sp,
    ));
    breakpoint.add_setter(label, "label", Some(&value.to_value()));
    breakpoint
}

/// Two nested breakpoints on one fixture whose setters both write its label,
/// with every apply, unapply, and label notification logged.
struct NestedPair {
    fixture: BreakpointFixture,
    outer: libadwaita::Breakpoint,
    inner: libadwaita::Breakpoint,
    log: Rc<RefCell<Vec<String>>>,
}

impl NestedPair {
    /// Build the pair, the bin allocated `width` pixels wide, the outer
    /// setter writing `outer_value` and the inner one `inner_value`.
    fn new(width: i32, outer_value: &str, inner_value: &str) -> Self {
        let fixture = BreakpointFixture::new(width);
        fixture.label.set_text(WRITE_BEFORE_ADDING_BOTH_SETTERS);
        let outer = max_width_setter(OUTER_SP, &fixture.label, outer_value);
        let inner = max_width_setter(INNER_SP, &fixture.label, inner_value);
        fixture.bin.add_breakpoint(outer.clone());
        fixture.bin.add_breakpoint(inner.clone());
        let log = Rc::new(RefCell::new(Vec::new()));
        for (breakpoint, name) in [(&outer, "outer"), (&inner, "inner")] {
            for unapply in [false, true] {
                let log = log.clone();
                let label = fixture.label.downgrade();
                let verb = if unapply { "unapply" } else { "apply" };
                let handler = move |_: &libadwaita::Breakpoint| {
                    let text = label
                        .upgrade()
                        .map_or_else(String::new, |label| label.text().to_string());
                    log.borrow_mut().push(format!("{verb} {name}[{text}]"));
                };
                if unapply {
                    breakpoint.connect_unapply(handler);
                } else {
                    breakpoint.connect_apply(handler);
                }
            }
        }
        let notified = log.clone();
        fixture.label.connect_label_notify(move |label| {
            notified
                .borrow_mut()
                .push(format!("notify[{}]", label.text()));
        });
        Self {
            fixture,
            outer,
            inner,
            log,
        }
    }

    /// Which breakpoint is current: `outer`, `inner`, or `none`.
    fn current(&self) -> &'static str {
        if self.fixture.is_current(&self.outer) {
            "outer"
        } else if self.fixture.is_current(&self.inner) {
            "inner"
        } else {
            "none"
        }
    }

    /// Run `change`, wait for the bin allocation it causes, settle, and read
    /// the switch, checking it went directly from `from` to `to` (`none`
    /// when it leaves both).
    fn switch(
        &self,
        recorder: &mut Recorder,
        from: &'static str,
        to: &'static str,
        change: impl FnOnce(),
    ) -> Result<Switch, Stop> {
        recorder.control(
            self.current() == from,
            "control: the switch starts at the outgoing breakpoint",
        )?;
        self.log.borrow_mut().clear();
        let before = self.fixture.host.allocations();
        change();
        let allocated = wait_until(ALLOCATION_BUDGET, || {
            self.fixture.host.allocations() > before
        });
        settle(LAYOUT_SETTLE);
        recorder.control(
            allocated && self.current() == to,
            "control: the change switches the current breakpoint directly",
        )?;
        Ok(Switch {
            label_after: self.fixture.label.text().to_string(),
            events: self.log.take(),
        })
    }
}

/// The four direct switches of one nested pair — into the inner breakpoint
/// and back out, by width and then by text scale alone — and the exit from
/// both after the width switches.
struct PairSwitches {
    width_in: Switch,
    width_out: Switch,
    scale_in: Switch,
    scale_out: Switch,
    exit: Switch,
}

impl PairSwitches {
    /// Every direct switch, with the breakpoints it leaves and enters and
    /// their setter values.
    fn legs<'a>(&'a self, outer_value: &'a str, inner_value: &'a str) -> [Leg<'a>; 4] {
        let inward = |switch| Leg {
            switch,
            from: "outer",
            from_value: outer_value,
            to: "inner",
            to_value: inner_value,
        };
        let outward = |switch| Leg {
            switch,
            from: "inner",
            from_value: inner_value,
            to: "outer",
            to_value: outer_value,
        };
        [
            inward(&self.width_in),
            outward(&self.width_out),
            inward(&self.scale_in),
            outward(&self.scale_out),
        ]
    }
}

/// Switch a nested pair whose setters write `outer_value` and `inner_value`
/// directly between its breakpoints, by width and by text scale alone.
fn switch_pair(
    recorder: &mut Recorder,
    outer_value: &str,
    inner_value: &str,
) -> Result<PairSwitches, Stop> {
    let settings = SettingsOverride::new();
    recorder.control(
        settings.available(),
        "control: a default GtkSettings exists",
    )?;
    settings.set_text_scale(1000);

    let by_width = NestedPair::new(OUTER_ONLY_WIDTH, outer_value, inner_value);
    let shown = Presented::checked(
        recorder,
        &by_width.fixture.host,
        "control: the nested-pair window realizes",
    )?;
    let width_in = by_width.switch(recorder, "outer", "inner", || {
        by_width.fixture.host.set_child_width(INSIDE_BOTH_WIDTH);
    })?;
    let width_out = by_width.switch(recorder, "inner", "outer", || {
        by_width.fixture.host.set_child_width(OUTER_ONLY_WIDTH);
    })?;
    let exit = by_width.switch(recorder, "outer", "none", || {
        by_width.fixture.host.set_child_width(OUTSIDE_BOTH_WIDTH);
    })?;
    drop(shown);

    let by_scale = NestedPair::new(RESCALE_REST_WIDTH, outer_value, inner_value);
    let _shown = Presented::checked(
        recorder,
        &by_scale.fixture.host,
        "control: the nested-pair window realizes",
    )?;
    let scale_in = by_scale.switch(recorder, "outer", "inner", || {
        settings.set_text_scale(DOUBLED_TEXT_SCALE);
    })?;
    let scale_out = by_scale.switch(recorder, "inner", "outer", || {
        settings.set_text_scale(1000);
    })?;
    Ok(PairSwitches {
        width_in,
        width_out,
        scale_in,
        scale_out,
        exit,
    })
}

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
        )?;

        // Two nested breakpoints owning the same property, switched directly
        // from one to the other.
        let different = switch_pair(recorder, OUTER_VALUE, INNER_VALUE)?;
        let equal = switch_pair(recorder, SHARED_VALUE, SHARED_VALUE)?;
        for (switch, keys) in [
            (
                &different.width_in,
                ("different_width_in_label", "different_width_in_events"),
            ),
            (
                &different.width_out,
                ("different_width_out_label", "different_width_out_events"),
            ),
            (
                &different.scale_in,
                ("different_scale_in_label", "different_scale_in_events"),
            ),
            (
                &different.scale_out,
                ("different_scale_out_label", "different_scale_out_events"),
            ),
            (
                &equal.width_in,
                ("equal_width_in_label", "equal_width_in_events"),
            ),
            (
                &equal.width_out,
                ("equal_width_out_label", "equal_width_out_events"),
            ),
            (
                &equal.scale_in,
                ("equal_scale_in_label", "equal_scale_in_events"),
            ),
            (
                &equal.scale_out,
                ("equal_scale_out_label", "equal_scale_out_events"),
            ),
            (
                &different.exit,
                ("different_exit_label", "different_exit_events"),
            ),
            (&equal.exit, ("equal_exit_label", "equal_exit_events")),
        ] {
            recorder.measure(keys.0, &switch.label_after);
            recorder.measure(keys.1, switch.describe());
        }

        let legs: Vec<Leg<'_>> = different
            .legs(OUTER_VALUE, INNER_VALUE)
            .into_iter()
            .chain(equal.legs(SHARED_VALUE, SHARED_VALUE))
            .collect();
        recorder.axiom(
            legs.iter()
                .all(|leg| leg.switch.label_after == leg.to_value),
            "A16: a direct switch between two breakpoints that both set a property leaves it at \
             the incoming breakpoint's value, whether the two values differ or are equal, by \
             width and by text scale alone",
        )?;
        recorder.axiom(
            legs.iter().all(|leg| leg.switch.is_direct(leg)),
            "A16: the switch is the outgoing unapply (the property still at its value), one \
             notify carrying the incoming value (even when it is equal), then the incoming apply",
        )?;
        recorder.axiom(
            legs.iter()
                .all(|leg| !leg.switch.saw(WRITE_BEFORE_ADDING_BOTH_SETTERS)),
            "A16: the add-time value is never observable during a direct switch",
        )?;
        recorder.axiom(
            different.exit.label_after == WRITE_BEFORE_ADDING_BOTH_SETTERS
                && equal.exit.label_after == WRITE_BEFORE_ADDING_BOTH_SETTERS,
            "A16: leaving both breakpoints after direct switches still restores the add-time value",
        )
    })
}
