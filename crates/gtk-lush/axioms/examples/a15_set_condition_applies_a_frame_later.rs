// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A15: the bin rests at 700 px while the buttons move its
//! breakpoint's condition across that width with `set_condition`. The readout
//! shows what `set_condition` left behind as it returned, and the live state:
//! the breakpoint changes only at the bin's next allocation, a frame later.

mod support;

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::BreakpointFixture;
use gtk_lush_axioms::fixtures::a15::{APPLIED_LABEL, NARROW_SP, REST_WIDTH, WIDE_SP};

fn build(ui: &support::SampleUi) {
    let fixture = BreakpointFixture::new(REST_WIDTH);
    let Some(breakpoint) = fixture.add_max_width_breakpoint(NARROW_SP, APPLIED_LABEL) else {
        return;
    };
    ui.set_fixture(&fixture.host);
    let at_return = Rc::new(RefCell::new(String::from("—")));
    for sp in [WIDE_SP, NARROW_SP] {
        let breakpoint = breakpoint.clone();
        let fixture = fixture.clone();
        let at_return = at_return.clone();
        ui.add_control(&format!("set_condition(max-width: {sp}sp)"), move || {
            BreakpointFixture::set_max_width(&breakpoint, sp);
            let applied = fixture.is_current(&breakpoint);
            at_return.replace(format!("max-width: {sp}sp → applied at return: {applied}"));
        });
    }
    ui.set_readout(move || {
        let applied = fixture.is_current(&breakpoint);
        format!(
            "{}   now applied: {applied}   setter target: {}",
            at_return.borrow(),
            fixture.label.text()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(15), build)
}
