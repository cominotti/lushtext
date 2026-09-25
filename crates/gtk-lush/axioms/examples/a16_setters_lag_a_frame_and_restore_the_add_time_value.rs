// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A16. Top: a `max-width: 600sp` breakpoint whose setter writes
//! the label. Resize the bin across the edge, and write the label yourself
//! while the breakpoint is applied: the next unapply restores the text the
//! label had when the setter was added, not your write.
//!
//! Bottom: two nested breakpoints, `max-width: 900sp` (writes `outer`) added
//! before `max-width: 500sp` (writes `inner`), both setting the same label.
//! Switch between them directly — width 700 ↔ 450 px, or at 900 px text
//! scale 1.0 ↔ 2.0 — and watch the signal and `notify::label` log: the
//! outgoing unapply, one notify carrying the incoming value, then the
//! incoming apply; the add-time text never appears until both are left
//! (width 1000 px).

mod support;

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a16::{
    APPLIED_LABEL, CONDITION_SP, DOUBLED_TEXT_SCALE, INNER_SP, INNER_VALUE, INSIDE_BOTH_WIDTH,
    NARROW_WIDTH, OUTER_ONLY_WIDTH, OUTER_SP, OUTER_VALUE, OUTSIDE_BOTH_WIDTH, RESCALE_REST_WIDTH,
    WIDE_WIDTH, WRITE_BEFORE_ADDING_BOTH_SETTERS, WRITE_BEFORE_ADDING_THE_SETTER,
    WRITE_WHILE_APPLIED,
};
use gtk_lush_axioms::fixtures::{BreakpointFixture, set_text_scale};
use libadwaita::prelude::*;

/// How many log entries the readout keeps.
const LOG_LENGTH: usize = 6;

fn build(ui: &support::SampleUi) {
    let single = BreakpointFixture::new(WIDE_WIDTH);
    single.label.set_text(WRITE_BEFORE_ADDING_THE_SETTER);
    let Some(breakpoint) = single.add_max_width_breakpoint(CONDITION_SP, APPLIED_LABEL) else {
        return;
    };

    let pair = BreakpointFixture::new(OUTER_ONLY_WIDTH);
    pair.label.set_text(WRITE_BEFORE_ADDING_BOTH_SETTERS);
    let (Some(outer), Some(inner)) = (
        pair.add_max_width_breakpoint(OUTER_SP, OUTER_VALUE),
        pair.add_max_width_breakpoint(INNER_SP, INNER_VALUE),
    ) else {
        return;
    };
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let push = {
        let log = log.clone();
        move |entry: String| {
            let mut log = log.borrow_mut();
            log.push(entry);
            let excess = log.len().saturating_sub(LOG_LENGTH);
            log.drain(..excess);
        }
    };
    for (bp, name) in [(&outer, "outer"), (&inner, "inner")] {
        let (push_apply, push_unapply) = (push.clone(), push.clone());
        bp.connect_apply(move |_| push_apply(format!("apply {name}")));
        bp.connect_unapply(move |_| push_unapply(format!("unapply {name}")));
    }
    pair.label
        .connect_label_notify(move |label| push(format!("notify[{}]", label.text())));

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    column.append(&single.host);
    column.append(&pair.host);
    ui.set_fixture(&column);

    for width in [NARROW_WIDTH, WIDE_WIDTH] {
        let host = single.host.clone();
        ui.add_control(&format!("Top width {width} px"), move || {
            host.set_child_width(width);
        });
    }
    let label = single.label.clone();
    ui.add_control("Write the top label", move || {
        label.set_text(WRITE_WHILE_APPLIED);
    });
    for width in [
        INSIDE_BOTH_WIDTH,
        OUTER_ONLY_WIDTH,
        RESCALE_REST_WIDTH,
        OUTSIDE_BOTH_WIDTH,
    ] {
        let host = pair.host.clone();
        ui.add_control(&format!("Bottom width {width} px"), move || {
            host.set_child_width(width);
        });
    }
    for scale in [1000, DOUBLED_TEXT_SCALE] {
        ui.add_control(&format!("Text scale {scale}‰"), move || {
            set_text_scale(scale);
        });
    }

    let (pair_bin, pair_label) = (pair.bin, pair.label);
    ui.set_readout(move || {
        let applied = single.is_current(&breakpoint);
        let current = match pair_bin.current_breakpoint() {
            Some(current) if current == outer => "outer",
            Some(current) if current == inner => "inner",
            _ => "none",
        };
        format!(
            "top: bin width {} px   applied: {applied}   label: {}\nbottom: bin width {} px   \
             current: {current}   label: {}\nlog: {}",
            single.bin.width(),
            single.label.text(),
            pair_bin.width(),
            pair_label.text(),
            log.borrow().join(" | ")
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(16), build)
}
